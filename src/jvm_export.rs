use prometheus::{Encoder, IntGaugeVec, register_int_gauge_vec};
use std::collections::HashMap;
use std::io::Write;
use std::process::Command;
use clap::{App, Arg};
use env_logger::Env;
use tokio::task;
use warp::Filter;
use log::{info, warn, error};

#[tokio::main]
pub(crate) async fn main() {
    // 初始化日志
    env_logger::Builder::from_env(Env::default().default_filter_or("info,warp=info")).init();

    // 使用 clap 解析命令行参数
    let matches = App::new("jvm-exporter")
        .version("0.1")
        .author("tf1997")
        .about("Monitor the JVM metrics")
        .arg(Arg::with_name("java_home")
            .long("java-home")
            .value_name("JAVA_HOME")
            .help("Sets a custom JAVA_HOME")
            .takes_value(true))
        .arg(Arg::with_name("full_path")
            .long("full-path")
            .help("Only use class name instead of full package path in the process name")
            .takes_value(false))
        .arg(Arg::with_name("auto_start")
            .long("auto-start")
            .help("Configure the program to auto-start with the system"))
        .get_matches();

    let java_home = matches.value_of("java_home").map(|s| s.to_string());
    let full_path = matches.is_present("full_path");
    let auto_start = matches.is_present("auto_start");
    if auto_start {
        match configure_auto_start() {
            Ok(_) => println!("Auto-start configuration successful."),
            Err(e) => eprintln!("Failed to configure auto-start: {}", e),
        }
    }

    // 定义 Prometheus 指标
    let jstat_gc_metrics = register_int_gauge_vec!(
        "jstat_gc_metrics",
        "Metrics from jstat -gc",
        &["pid", "process_name", "metric_name"]
    )
        .unwrap();

    // 定义 HTTP 路由，暴露指标
    let metrics_route = warp::path("metrics").and_then({
        let jstat_gc_metrics = jstat_gc_metrics.clone();
        let full_path = full_path.clone();
        let java_home = java_home.map(|s| s.to_string());
        move || {
            let jstat_gc_metrics = jstat_gc_metrics.clone();
            let java_home = java_home.clone();
            let full_path =full_path.clone();
            async move {
                if let Err(err) = update_gc_metrics(&jstat_gc_metrics, java_home.as_deref(), full_path).await {
                    error!("Failed to update metrics: {}", err);
                }

                let mut buffer = Vec::new();
                let encoder = prometheus::TextEncoder::new();
                let metric_families = prometheus::gather();
                encoder.encode(&metric_families, &mut buffer).unwrap();

                let response = warp::http::Response::builder()
                    .header("Content-Type", encoder.format_type())
                    .body(String::from_utf8(buffer).unwrap());
                Ok::<_, warp::Rejection>(response)
            }
        }
    });

    // 启动 HTTP 服务器
    let addr = ([127, 0, 0, 1], 29090);
    let ip_addr = std::net::Ipv4Addr::from(addr.0);

    // 启动服务器并等待其完成
    let server = warp::serve(metrics_route).bind((ip_addr, addr.1));
    let server_handle = tokio::spawn(server);

    // 等待日志输出
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // 输出服务器启动信息
    println!("Server started successfully");
    println!("Listening on http://{}:{}/metrics", ip_addr, addr.1);

    // 等待服务器任务完成
    let _ = server_handle.await;

    // 等待程序结束
    tokio::signal::ctrl_c().await.ok();
}

// 更新 GC 指标函数
async fn update_gc_metrics(jstat_gc_metrics: &IntGaugeVec, java_home: Option<&str>, full_path: bool) -> Result<(), Box<dyn std::error::Error>> {
    // 获取所有 Java 进程的 PID 和对应的进程名
    let processes = get_java_processes(java_home, full_path)?;

    // 创建一个任务列表，用于并发调用 jstat -gc
    let tasks: Vec<_> = processes
        .into_iter()
        .map(|(pid, process_name)| {
            let jstat_gc_metrics = jstat_gc_metrics.clone();
            let java_home = java_home.map(|s| s.to_string());
            task::spawn(async move {
                match fetch_and_update_jstat_gc(pid.clone(), process_name.clone(), &jstat_gc_metrics, java_home.as_deref()).await {
                    Ok(_) => info!("Successfully updated metrics for PID {} ({})", pid, process_name),
                    Err(err) => warn!("Failed to update metrics for PID {} ({}): {}", pid, process_name, err),
                }
            })
        })
        .collect();

    // 等待所有任务完成
    futures::future::join_all(tasks).await;

    Ok(())
}

// 获取 Java 进程的 PID 和进程名
fn get_java_processes(java_home: Option<&str>, full_path: bool) -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    let mut command = Command::new("jps");
    command.arg("-l");

    merge_java_home(java_home, &mut command);

    let output = command.output()?;

    if !output.status.success() {
        return Err(format!(
            "jps failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
            .into());
    }

    let stdout = String::from_utf8(output.stdout)?;
    let mut processes = HashMap::new();

    // 解析 jps 输出，格式类似： "12345 some.package.MainClass"
    for line in stdout.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let process_name = if full_path {
                parts[1].to_string() // 使用完整包路径
            } else {
                parts[1].split('.').last().unwrap().to_string() // 只使用类名
            };
            processes.insert(parts[0].to_string(), process_name);
        }
    }

    Ok(processes)
}

fn merge_java_home(java_home: Option<&str>, command: &mut Command) {
    if let Some(jh) = java_home {
        command.env("JAVA_HOME", jh);
        command.env("PATH", format!("{}/bin:{}", jh, std::env::var("PATH").unwrap_or_default()));
    }
}

// 调用 jstat -gc 并更新指标
async fn fetch_and_update_jstat_gc(
    pid: String,
    process_name: String,
    jstat_gc_metrics: &IntGaugeVec,
    java_home: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    // 调用 jstat -gc
    let mut command = Command::new("jstat");
    command.args(&["-gc", &pid, "1000", "1"]);

    if let Some(jh) = java_home {
        command.env("JAVA_HOME", jh);
        command.env("PATH", format!("{}/bin:{}", jh, std::env::var("PATH").unwrap_or_default()));
    }
    let output = command.output()?;

    if !output.status.success() {
        return Err(format!(
            "jstat failed for PID {}: {}",
            pid,
            String::from_utf8_lossy(&output.stderr)
        )
            .into());
    }

    let stdout = String::from_utf8(output.stdout)?;
    let lines: Vec<&str> = stdout.lines().collect();
    if lines.len() < 2 {
        return Err("Unexpected jstat output".into());
    }

    // 提取标题和数据
    let headers: Vec<&str> = lines[0].split_whitespace().collect();
    let values: Vec<&str> = lines[1].split_whitespace().collect();

    if headers.len() != values.len() {
        return Err(format!(
            "Mismatch between headers and values. Headers: {:?}, Values: {:?}",
            headers, values
        )
            .into());
    }

    // 更新 Prometheus 指标
    for (header, value) in headers.iter().zip(values.iter()) {
        if let Ok(parsed_value) = value.parse::<f64>() {  // 解析为 f64 因为数值是浮动的
            jstat_gc_metrics
                .with_label_values(&[&pid, &process_name, header])
                .set(parsed_value as i64);  // 将解析的值传递给 jstat_gc_metrics
        } else {
            warn!(
            "Failed to parse value for {}: {} in PID {}",
            header, value, pid
        );
        }
    }
    Ok(())
}

// 配置程序为系统自启动服务的函数
fn configure_auto_start() -> Result<(), Box<dyn std::error::Error>> {
    let service_path = "/etc/systemd/system/jvm-exporter.service";

    let service_content = "[Unit]
Description=JVM Exporter Service

[Service]
ExecStart=/usr/local/bin/jvm-exporter

[Install]
WantedBy=multi-user.target".to_string();

    let mut file = std::fs::File::create(service_path)?;
    file.write_all(service_content.as_bytes())?;

    // 通知 systemd 重新加载配置
    std::process::Command::new("systemctl")
        .args(&["daemon-reload"])
        .output()?;

    // 启用服务
    std::process::Command::new("systemctl")
        .args(&["enable", "jvm-exporter.service"])
        .output()?;

    println!("Service configured to auto-start with the system.");
    println!("Service file created at: {}", service_path);
    println!("Use the following commands to manage the service:");
    println!("  Start service:    systemctl start jvm-exporter.service");
    println!("  Stop service:     systemctl stop jvm-exporter.service");
    println!("  Status of service: systemctl status jvm-exporter.service");
    println!("  Enable service on boot: systemctl enable jvm-exporter.service");
    println!("  Disable service on boot: systemctl disable jvm-exporter.service");
    println!("  Reload daemon after changes: systemctl daemon-reload");

    Ok(())
}