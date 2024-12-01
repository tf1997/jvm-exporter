use prometheus::{Encoder, GaugeVec, Registry};
use std::collections::HashMap;
use std::io::Write;
use std::process::Command;
use std::sync::Arc;
use warp::Filter;
use log::{warn, error};
use clap::{App, Arg};
use env_logger::Env;

const JSTAT_COMMANDS: &[&str] = &["-gc", "-gcutil", "-class"];

struct Metrics {
    metrics_map: HashMap<&'static str, GaugeVec>,
}

impl Metrics {
    fn new(registry: &Registry) -> Self {
        let mut metrics_map = HashMap::new();

        for &cmd in JSTAT_COMMANDS {
            let metric = GaugeVec::new(
                prometheus::Opts::new(
                    format!("jstat_{}_metrics", &cmd[1..]), // 去掉前导的 '-'
                    format!("Metrics from jstat {}", cmd)
                ),
                &["pid", "process_name", "metric_name"],
            ).expect(&format!("Failed to create GaugeVec for command {}", cmd));
            registry.register(Box::new(metric.clone())).expect(&format!("Failed to register metric for {}", cmd));
            metrics_map.insert(cmd, metric);
        }

        Metrics { metrics_map }
    }
}

#[tokio::main]
pub(crate) async fn main() {
    env_logger::Builder::from_env(Env::default().default_filter_or("info,warp=info")).init();

    let matches = App::new("jvm-exporter")
        .version("0.1")
        .author("tf1997")
        .about("Monitor the JVM metrics")
        .arg(Arg::new("java_home")
            .long("java-home")
            .value_name("JAVA_HOME")
            .help("Sets a custom JAVA_HOME")
            .takes_value(true))
        .arg(Arg::new("full_path")
            .long("full-path")
            .help("Only use class name instead of full package path in the process name")
            .takes_value(false))
        .arg(Arg::new("auto_start")
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

    let registry = Arc::new(prometheus::Registry::new());
    let metrics = Arc::new(Metrics::new(&registry));

    // 封装共享数据到 Arc
    let java_home = Arc::new(java_home);

    let metrics_route = warp::path("metrics").and_then({
        let metrics = Arc::clone(&metrics);
        let registry = Arc::clone(&registry);
        let java_home = Arc::clone(&java_home);
        let full_path = full_path;

        move || {
            let metrics = Arc::clone(&metrics);
            let registry = Arc::clone(&registry);
            let java_home = java_home.clone();
            let full_path = full_path;

            async move {
                handle_metrics(metrics, registry, java_home, full_path).await
            }
        }
    });

    let addr = ([127, 0, 0, 1], 29090);
    let ip_addr = std::net::Ipv4Addr::from(addr.0);

    let server = warp::serve(metrics_route).bind((ip_addr, addr.1));
    let server_handle = tokio::spawn(server);

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    println!("Server started successfully");
    println!("Listening on http://{}:{}/metrics", ip_addr, addr.1);

    let _ = server_handle.await;

    tokio::signal::ctrl_c().await.ok();
}

// 异步处理函数
async fn handle_metrics(
    metrics: Arc<Metrics>,
    registry: Arc<prometheus::Registry>,
    java_home: Arc<Option<String>>,
    full_path: bool,
) -> Result<impl warp::Reply, warp::Rejection> {
    if let Err(err) = update_metrics(metrics.clone(), java_home.as_deref(), full_path).await {
        error!("Failed to update metrics: {}", err);
    }

    let mut buffer = Vec::new();
    let encoder = prometheus::TextEncoder::new();
    let metric_families = registry.gather();
    encoder.encode(&metric_families, &mut buffer).expect("Failed to encode metrics");

    let response = warp::http::Response::builder()
        .header("Content-Type", encoder.format_type())
        .body(String::from_utf8(buffer).expect("Failed to convert buffer to String"));
    Ok(response)
}

// 更新所有指标
async fn update_metrics(
    metrics: Arc<Metrics>,
    java_home: Option<&str>,
    full_path: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let processes = get_java_processes(java_home, full_path)?;

    let tasks: Vec<_> = processes
        .into_iter()
        .flat_map(|(pid, process_name)| {
            let metrics = Arc::clone(&metrics);
            let java_home = java_home.map(|s| s.to_string());

            JSTAT_COMMANDS.iter().map(move |&command| {
                let metrics = Arc::clone(&metrics);
                let java_home = java_home.clone();
                let pid = pid.clone();
                let process_name = process_name.clone();

                tokio::spawn(async move {
                    if let Some(metric) = metrics.metrics_map.get(command) {
                        if let Err(err) = fetch_and_update_jstat(&pid, &process_name, command, metric, java_home.as_deref()).await {
                            warn!("Failed to update {} metrics for PID {} ({}): {}", command, pid, process_name, err);
                        }
                    }
                })
            })
        })
        .collect();

    // 等待所有任务完成
    futures::future::join_all(tasks).await;

    Ok(())
}

// 通用的 fetch_and_update_jstat 函数
async fn fetch_and_update_jstat(
    pid: &String,
    process_name: &String,
    command: &str,
    jstat_metrics: &GaugeVec,
    java_home: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::new("jstat");
    cmd.args(&[command, pid, "1000", "1"]);

    if let Some(jh) = java_home {
        cmd.env("JAVA_HOME", jh);
        cmd.env("PATH", format!("{}/bin:{}", jh, std::env::var("PATH").unwrap_or_default()));
    }

    let output = cmd.output()?;

    if !output.status.success() {
        return Err(format!(
            "jstat {} failed for PID {}: {}",
            command,
            pid,
            String::from_utf8_lossy(&output.stderr)
        ).into());
    }

    let stdout = String::from_utf8(output.stdout)?;
    let lines: Vec<&str> = stdout.lines().collect();
    if lines.len() < 2 {
        return Err("Unexpected jstat output".into());
    }

    let headers: Vec<&str> = lines[0].split_whitespace().collect();
    let values: Vec<&str> = lines[1].split_whitespace().collect();

    if headers.len() != values.len() {
        return Err(format!(
            "Mismatch between headers and values for command {}. Headers: {:?}, Values: {:?}",
            command, headers, values
        ).into());
    }

    for (header, value) in headers.iter().zip(values.iter()) {
        if let Ok(parsed_value) = value.parse::<f64>() {
            jstat_metrics
                .with_label_values(&[pid, process_name, header])
                .set(parsed_value);
        } else {
            warn!(
                "Failed to parse value for {}: {} in PID {}",
                header, value, pid
            );
        }
    }

    Ok(())
}

// 获取 Java 进程
fn get_java_processes(java_home: Option<&str>, full_path: bool) -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    let mut command = Command::new("jps");
    command.arg("-l");

    merge_java_home(java_home, &mut command)?;

    let output = command.output()?;

    if !output.status.success() {
        return Err(format!(
            "jps failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ).into());
    }

    let stdout = String::from_utf8(output.stdout)?;
    let mut processes = HashMap::new();

    // 解析 jps 输出，格式类似于: "12345 some.package.MainClass"
    for line in stdout.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let process_name = if full_path {
                parts[1].to_string() // 使用全包路径
            } else {
                parts[1].split('.').last().unwrap_or(parts[1]).to_string() // 只使用类名
            };
            processes.insert(parts[0].to_string(), process_name);
        }
    }

    Ok(processes)
}

// 合并 JAVA_HOME 到命令环境
fn merge_java_home(java_home: Option<&str>, command: &mut Command) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(jh) = java_home {
        command.env("JAVA_HOME", jh);
        command.env("PATH", format!("{}/bin:{}", jh, std::env::var("PATH").unwrap_or_default()));
    }
    Ok(())
}

// 配置开机自启为 systemd 服务
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
