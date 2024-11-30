use prometheus::{Encoder, TextEncoder, IntGaugeVec, register_int_gauge_vec, gather};
use std::collections::HashMap;
use std::process::Command;
use tokio::task;
use warp::Filter;
use log::{info, warn, error};

#[tokio::main]
pub(crate) async fn main() {
    // 初始化日志
    env_logger::init();

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
        move || {
            let jstat_gc_metrics = jstat_gc_metrics.clone();
            async move {
                if let Err(err) = update_gc_metrics(&jstat_gc_metrics).await {
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
    warp::serve(metrics_route).run(([0, 0, 0, 0], 9090)).await;
}

// 更新 GC 指标函数
async fn update_gc_metrics(jstat_gc_metrics: &IntGaugeVec) -> Result<(), Box<dyn std::error::Error>> {
    // 获取所有 Java 进程的 PID 和对应的进程名
    let processes = get_java_processes()?;

    // 创建一个任务列表，用于并发调用 jstat -gc
    let tasks: Vec<_> = processes
        .into_iter()
        .map(|(pid, process_name)| {
            let jstat_gc_metrics = jstat_gc_metrics.clone();
            task::spawn(async move {
                match fetch_and_update_jstat_gc(pid.clone(), process_name.clone(), &jstat_gc_metrics).await {
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
fn get_java_processes() -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    let output = Command::new("jps")
        .arg("-l") // 列出 JVM 进程及其完整类名
        .output()?;

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
            processes.insert(parts[0].to_string(), parts[1..].join(" "));
        }
    }

    Ok(processes)
}

// 调用 jstat -gc 并更新指标
async fn fetch_and_update_jstat_gc(
    pid: String,
    process_name: String,
    jstat_gc_metrics: &IntGaugeVec,
) -> Result<(), Box<dyn std::error::Error>> {
    // 调用 jstat -gc
    let output = Command::new("jstat")
        .args(&["-gc", &pid, "1000", "1"]) // 使用 "-gc" 模式
        .output()?;

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


    print_jstat_gc_metrics(jstat_gc_metrics).await;

    Ok(())
}
async fn print_jstat_gc_metrics(jstat_gc_metrics: &IntGaugeVec) {
    let mut buffer = Vec::new();
    let encoder = TextEncoder::new();
    let metric_families = gather(); // 获取所有指标

    // 编码并打印所有指标
    encoder.encode(&metric_families, &mut buffer).unwrap();
    println!("{}", String::from_utf8(buffer).unwrap()); // 打印指标的文本格式
}