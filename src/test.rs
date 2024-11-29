use prometheus::{Encoder, TextEncoder, IntGaugeVec, register_int_gauge_vec};
use std::process::Command;
use warp::Filter;
use std::collections::HashMap;
use std::str;

#[tokio::main]
pub(crate) async fn main() {
    // 定义 Prometheus 指标
    let jstat_metrics = register_int_gauge_vec!(
        "jstat_metrics",
        "Metrics from jstat",
        &["pid", "process_name", "metric_name"]
    )
        .unwrap();

    // 定义 HTTP 路由，暴露指标
    let metrics_route = warp::path("metrics").map(move || {
        // 更新指标数据
        if let Err(err) = update_metrics(&jstat_metrics) {
            eprintln!("Failed to update metrics: {}", err);
        }

        // 序列化指标为 Prometheus 格式
        let mut buffer = Vec::new();
        let encoder = TextEncoder::new();
        let metric_families = prometheus::gather();
        encoder.encode(&metric_families, &mut buffer).unwrap();

        // 返回指标
        warp::http::Response::builder()
            .header("Content-Type", encoder.format_type())
            .body(String::from_utf8(buffer).unwrap())
    });

    // 启动 HTTP 服务器
    warp::serve(metrics_route).run(([0, 0, 0, 0], 9090)).await;
}

// 更新指标函数
fn update_metrics(jstat_metrics: &IntGaugeVec) -> Result<(), Box<dyn std::error::Error>> {
    // 获取所有 Java 进程的 PID 和对应的进程名
    let processes = get_java_processes()?;

    // 遍历所有 Java 进程，更新每个进程的指标
    for (pid, process_name) in processes.iter() {
        // 调用 jstat 命令，获取输出
        let output = Command::new("jstat")
            .args(&["-gc", pid, "1000", "1"]) // 对每个进程使用 jstat
            .output()?;

        // 检查命令是否成功
        if !output.status.success() {
            eprintln!(
                "jstat failed for PID {}: {}",
                pid,
                String::from_utf8_lossy(&output.stderr)
            );
            continue;
        }

        // 解析 jstat 输出
        let stdout = String::from_utf8(output.stdout)?;
        let lines: Vec<&str> = stdout.lines().collect();
        if lines.len() < 2 {
            eprintln!("Unexpected jstat output for PID {}: {}", pid, stdout);
            continue;
        }

        // 提取标题和数据
        let headers: Vec<&str> = lines[0].split_whitespace().collect();
        let values: Vec<&str> = lines[1].split_whitespace().collect();

        if headers.len() != values.len() {
            eprintln!(
                "Mismatch between headers and values for PID {}: headers={:?}, values={:?}",
                pid, headers, values
            );
            continue;
        }

        // 更新 Prometheus 指标
        for (header, value) in headers.iter().zip(values.iter()) {
            if let Ok(parsed_value) = value.parse::<i64>() {
                jstat_metrics
                    .with_label_values(&[pid, process_name, header])
                    .set(parsed_value);
            }
        }
    }

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