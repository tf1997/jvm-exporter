use prometheus::{Encoder, GaugeVec, Registry};
use std::collections::{HashMap, HashSet};
use std::io::Write;
use tokio::process::Command;
use std::sync::Arc;
use warp::Filter;
use log::{warn, error, info};
use clap::{App, Arg};
use env_logger::Env;
use sysinfo::{System};
use tokio::sync::Mutex;

const JSTAT_COMMANDS: &[&str] = &["-gc", "-gcutil", "-class", "-compiler"];
const EXCLUDED_PROCESSES: &[&str] = &["jps"];

struct Metrics {
    jstat_metrics_map: HashMap<&'static str, GaugeVec>,
    cpu_usage: GaugeVec,
    memory_usage: GaugeVec,
    memory_usage_percentage: GaugeVec,
    start_time: GaugeVec,
    up_time: GaugeVec,
    active_pids: Mutex<HashMap<String, String>>, // Key: container#pid
    jstat_labels: Mutex<HashMap<(&'static str, String, String, String), HashSet<String>>>, // (command, container, pid, process_name)
}

impl Metrics {
    fn new(registry: &Registry) -> Self {
        let mut metrics_map = HashMap::new();

        for &cmd in JSTAT_COMMANDS.iter() {
            let metric = GaugeVec::new(
                prometheus::Opts::new(
                    format!("jstat_{}_metrics", &cmd[1..]), // Remove leading '-'
                    format!("Metrics from jstat {}", cmd),
                ),
                &["container", "pid", "process_name", "metric_name"],
            ).expect(&format!("Failed to create GaugeVec for command {}", cmd));
            registry.register(Box::new(metric.clone())).expect(&format!("Failed to register metric for {}", cmd));
            metrics_map.insert(cmd, metric);
        }

        let cpu_usage = GaugeVec::new(
            prometheus::Opts::new("process_cpu_usage", "CPU usage percentage of the process"),
            &["container", "pid", "process_name"],
        ).expect("Failed to create CPU usage GaugeVec");
        registry.register(Box::new(cpu_usage.clone())).expect("Failed to register CPU usage metric");

        let memory_usage = GaugeVec::new(
            prometheus::Opts::new("process_memory_usage", "Memory usage (in bytes) of the process"),
            &["container", "pid", "process_name"],
        ).expect("Failed to create Memory usage GaugeVec");
        registry.register(Box::new(memory_usage.clone())).expect("Failed to register Memory usage metric");

        let memory_usage_percentage = GaugeVec::new(
            prometheus::Opts::new("process_memory_usage_percentage", "Memory usage percentage of the process"),
            &["container", "pid", "process_name"],
        ).expect("Failed to create Memory usage percentage GaugeVec");
        registry.register(Box::new(memory_usage_percentage.clone())).expect("Failed to register Memory usage percentage metric");

        let start_time = GaugeVec::new(
            prometheus::Opts::new("process_start_time", "Start time of the process in seconds since the epoch"),
            &["container", "pid", "process_name"],
        ).expect("Failed to create start time GaugeVec");
        registry.register(Box::new(start_time.clone())).expect("Failed to register start time metric");

        let up_time = GaugeVec::new(
            prometheus::Opts::new("process_up_time", "Up time of the process in seconds"),
            &["container", "pid", "process_name"],
        ).expect("Failed to create up time GaugeVec");
        registry.register(Box::new(up_time.clone())).expect("Failed to register up time metric");

        Metrics {
            jstat_metrics_map: metrics_map,
            cpu_usage,
            memory_usage,
            memory_usage_percentage,
            start_time,
            up_time,
            active_pids: Mutex::new(HashMap::new()),
            jstat_labels: Mutex::new(HashMap::new()),
        }
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

    let addr = ([0, 0, 0, 0], 29090);
    let ip_addr = std::net::Ipv4Addr::from(addr.0);

    let server = warp::serve(metrics_route).bind((ip_addr, addr.1));
    let server_handle = tokio::spawn(server);

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    println!("Server started successfully");
    println!("Listening on http://{}:{}/metrics", "127.0.0.1", addr.1);

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            println!("Received Ctrl+C, shutting down.");
        },
        res = server_handle => {
            if let Err(e) = res {
                eprintln!("Server error: {}", e);
            }
        },
    }
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

struct ProcessInfo {
    container: String, // "host" or container ID
    pid: String,
    process_name: String,
}

// 更新所有指标
async fn update_metrics(
    metrics: Arc<Metrics>,
    java_home: Option<&str>,
    full_path: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut all_processes = Vec::new();
    // 1. Collect Host Processes
    let host_processes = get_java_processes(java_home, full_path, "host".to_string()).await?;
    info!("Detect and Collect Host Processes{}", host_processes.len());
    for (pid, pname) in host_processes {
        all_processes.push(ProcessInfo {
            container: "host".to_string(),
            pid,
            process_name: pname,
        });
    }
    // 2. Detect and Collect Container Processes
    let container_processes = get_container_java_processes(java_home, full_path).await?;
    info!("Detect and Collect Container Processes{}", container_processes.len());
    all_processes.extend(container_processes);

    // Create a unique key for each process as "container#pid"
    let current_pids: HashMap<String, String> = all_processes.iter()
        .map(|p| (format!("{}#{}", p.container, p.pid), p.process_name.clone()))
        .collect();

    // Identify removed PIDs
    let removed_pids: Vec<(String, String)> = {
        let active_pids = metrics.active_pids.lock().await;
        active_pids
            .iter()
            .filter(|(key, _)| !current_pids.contains_key(*key))
            .map(|(key, pname)| (key.clone(), pname.clone()))
            .collect()
    };

    // Remove metrics for removed PIDs
    if !removed_pids.is_empty() {
        let mut active_pids = metrics.active_pids.lock().await;
        for (key, _) in &removed_pids {
            active_pids.remove(key);
        }
        info!("Removed PIDs from active_pids");

        let mut jstat_labels = metrics.jstat_labels.lock().await;
        for (key, process_name) in &removed_pids {
            let parts: Vec<&str> = key.split('#').collect();
            if parts.len() != 2 {
                continue;
            }
            let container = parts[0];
            let pid = parts[1];

            // Remove CPU and Memory metrics
            metrics.cpu_usage.remove_label_values(&[container, pid, process_name]);
            metrics.memory_usage.remove_label_values(&[container, pid, process_name]);
            metrics.memory_usage_percentage.remove_label_values(&[container, pid, process_name]);
            metrics.start_time.remove_label_values(&[container, pid, process_name]);
            metrics.up_time.remove_label_values(&[container, pid, process_name]);

            // Remove jstat metrics
            for &command in JSTAT_COMMANDS.iter() {
                let key_jstat = (command, container.to_string(), pid.to_string(), process_name.clone());
                if let Some(metric_names) = jstat_labels.get(&key_jstat) {
                    if let Some(metric) = metrics.jstat_metrics_map.get(command) {
                        for metric_name in metric_names.iter() {
                            let _ = metric.remove_label_values(&[container, pid, process_name, metric_name]);
                        }
                    }
                }
                // Remove recorded metric_names
                jstat_labels.remove(&key_jstat);
            }
        }
    }

    // Update active_pids
    {
        let mut active_pids = metrics.active_pids.lock().await;
        *active_pids = current_pids.clone();
    }

    // Update CPU and Memory metrics
    if let Err(e) = update_cpu_memory_metrics(Arc::clone(&metrics), &all_processes).await {
        error!("Failed to update CPU and memory metrics: {}", e);
    }

    // Update jstat metrics
    let tasks: Vec<_> = all_processes
        .into_iter()
        .flat_map(|proc_info| {
            let metrics = Arc::clone(&metrics);
            let java_home = java_home.map(|s| s.to_string());
            let container = proc_info.container.clone();
            let pid = proc_info.pid.clone();
            let process_name = proc_info.process_name.clone();

            JSTAT_COMMANDS.iter().map(move |&command| {
                let metrics = Arc::clone(&metrics);
                let java_home = java_home.clone();
                let container = container.clone();
                let pid = pid.clone();
                let process_name = process_name.clone();

                tokio::spawn(async move {
                    if let Some(metric) = metrics.jstat_metrics_map.get(command) {
                        match fetch_and_update_jstat(&container, &pid, &process_name, command, metric, java_home.as_deref()).await {
                            Ok(metric_names) => {
                                // Record metric_names
                                let mut jstat_labels = metrics.jstat_labels.lock().await;
                                let key = (command, container.clone(), pid.clone(), process_name.clone());
                                jstat_labels.entry(key).or_insert_with(HashSet::new).extend(metric_names);
                            }
                            Err(err) => {
                                warn!("Failed to update {} metrics for PID {} ({} in {}): {}", command, pid, process_name, container, err);
                            }
                        }
                    }
                })
            })
        })
        .collect();

    futures::future::join_all(tasks).await;

    Ok(())
}

async fn fetch_and_update_jstat(
    container: &String,
    pid: &String,
    process_name: &String,
    command: &str,
    jstat_metrics: &GaugeVec,
    java_home: Option<&str>,
) -> Result<HashSet<String>, Box<dyn std::error::Error + Send + Sync>> {
    let mut cmd = if container == "host" {
        let mut command_host = Command::new("jstat");
        command_host.args(&[command, pid, "1000", "1"]);
        if let Some(jh) = java_home {
            command_host.env("JAVA_HOME", jh);
            command_host.env("PATH", format!("{}/bin:{}", jh, std::env::var("PATH").unwrap_or_default()));
        }
        command_host
    } else {
        // Execute jstat inside the container
        if is_docker_available().await {
            let mut cmd_docker = Command::new("docker");
            cmd_docker.args(&["exec", container, "jstat", command, pid, "1000", "1"]);
            if let Some(jh) = java_home {
                cmd_docker.env("JAVA_HOME", jh);
                cmd_docker.env("PATH", format!("{}/bin:{}", jh, std::env::var("PATH").unwrap_or_default()));
            }
            cmd_docker
        } else if is_crictl_available().await {
            let mut cmd_crictl = Command::new("crictl");
            cmd_crictl.args(&["exec", container, "jstat", command, pid, "1000", "1"]);
            if let Some(jh) = java_home {
                cmd_crictl.env("JAVA_HOME", jh);
                cmd_crictl.env("PATH", format!("{}/bin:{}", jh, std::env::var("PATH").unwrap_or_default()));
            }
            cmd_crictl
        } else {
            return Err("Neither Docker nor crictl is available to execute commands in containers".into());
        }
    };

    let output = cmd.output().await?;

    if !output.status.success() {
        return Err(format!(
            "jstat {} failed for PID {} in container {}: {}",
            command,
            pid,
            container,
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

    let mut metric_names = HashSet::new();

    if headers.len() != values.len() {
        warn!("Mismatch in headers and values count for command {} for PID {} in container {}: headers = {:?}, values = {:?}", command, pid, container, headers, values);
        // Only process matching header-value pairs
        let min_len = std::cmp::min(headers.len(), values.len());
        for i in 0..min_len {
            let header = headers[i];
            let value = values[i];
            let parsed_value = value.parse::<f64>().unwrap_or(0.0);
            jstat_metrics
                .with_label_values(&[container, pid, process_name, header])
                .set(parsed_value);
            metric_names.insert(header.to_string());
        }
    } else {
        for (header, value) in headers.iter().zip(values.iter()) {
            let parsed_value = if *value == "-" {
                0.0
            } else {
                match value.parse::<f64>() {
                    Ok(v) => v,
                    Err(_) => {
                        warn!(
                          "Failed to parse value for {}: {} in PID {} and Process_name {} in container {}",
                          header, value, pid, process_name, container
                      );
                        continue;
                    }
                }
            };

            jstat_metrics
                .with_label_values(&[container, pid, process_name, header])
                .set(parsed_value);
            metric_names.insert(header.to_string());
        }
    }
    Ok(metric_names)
}

// Update CPU and Memory metrics
async fn update_cpu_memory_metrics(metrics: Arc<Metrics>, processes: &[ProcessInfo]) -> Result<(), Box<dyn std::error::Error>> {
    let mut system = System::new_all();
    system.refresh_all();
    let total_memory_kb = system.total_memory() as f64;

    for proc_info in processes.iter() {
        let pid_str = &proc_info.pid;
        let container = &proc_info.container;
        let process_name = &proc_info.process_name;

        // Extract class name (last part of the package path)
        let class_name = process_name
            .split('.')
            .last()
            .unwrap_or(process_name);

        // Check if class name is in the exclusion list
        if EXCLUDED_PROCESSES.iter().any(|&excluded| excluded.eq_ignore_ascii_case(class_name)) {
            warn!("Excluding process PID {}: {}", pid_str, class_name);
            continue;
        }

        if let Ok(pid) = pid_str.parse::<usize>() {
            if let Some(process) = system.process(sysinfo::Pid::from(pid)) {
                // Update CPU usage
                metrics.cpu_usage
                    .with_label_values(&[container, pid_str, process_name])
                    .set(process.cpu_usage() as f64);

                // Update Memory usage (in bytes)
                metrics.memory_usage
                    .with_label_values(&[container, pid_str, process_name])
                    .set(process.memory() as f64); // Convert KB to Bytes

                let process_memory_kb = process.memory() as f64;
                let memory_usage_percentage = if total_memory_kb > 0.0 {
                    (process_memory_kb / total_memory_kb) * 100.0
                } else {
                    0.0
                };

                metrics.memory_usage_percentage
                    .with_label_values(&[container, pid_str, process_name])
                    .set(memory_usage_percentage);

                let start_time_secs = process.start_time();
                let up_time_secs = process.run_time();

                metrics.start_time
                    .with_label_values(&[container, pid_str, process_name])
                    .set(start_time_secs as f64);

                metrics.up_time
                    .with_label_values(&[container, pid_str, process_name])
                    .set(up_time_secs as f64);
            }
        }
    }

    Ok(())
}

// Get Java processes on the host or within containers
async fn get_java_processes(
    java_home: Option<&str>,
    full_path: bool,
    container: String,
) -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    let mut processes = HashMap::new();

    if container == "host" {
        // 在主机上直接运行 jps
        let mut command = Command::new("jps");
        command.arg("-l");
        merge_java_home(java_home, &mut command)?;
        let output = command.output().await?;

        if !output.status.success() {
            return Err(format!(
                "jps failed for host: {}",
                String::from_utf8_lossy(&output.stderr)
            )
                .into());
        }

        let stdout = String::from_utf8(output.stdout)?;
        info!("Host jps output:\n{}", stdout);

        // 解析 jps 输出
        for line in stdout.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let process_name_original = parts[1];
                let class_name = process_name_original
                    .split('.')
                    .last()
                    .unwrap_or(process_name_original);

                if EXCLUDED_PROCESSES.iter().any(|&excluded| excluded.eq_ignore_ascii_case(class_name)) {
                    continue;
                }

                let process_name = if full_path {
                    process_name_original.to_string()
                } else {
                    class_name.to_string()
                };

                processes.insert(parts[0].to_string(), process_name);
            }
        }
    } else {
        // 根据可用性选择 Docker 或 crictl 执行 jps
        let mut cmd;
        if is_docker_available().await {
            cmd = Command::new("docker");
            cmd.args(&["exec", &container, "jps", "-l"]);
            info!("Executing jps inside Docker container: {}", container);
        } else if is_crictl_available().await {
            cmd = Command::new("crictl");
            cmd.args(&["exec", &container, "jps", "-l"]);
            info!("Executing jps inside crictl container: {}", container);
        } else {
            return Err("Neither Docker nor crictl is available to execute commands in containers".into());
        }

        // 设置 JAVA_HOME 环境变量
        if let Some(jh) = java_home {
            cmd.env("JAVA_HOME", jh);
            cmd.env("PATH", format!("{}/bin:{}", jh, std::env::var("PATH").unwrap_or_default()));
        }

        let output = cmd.output().await?;

        if !output.status.success() {
            return Err(format!(
                "jps failed for container {}: {}",
                container,
                String::from_utf8_lossy(&output.stderr)
            )
                .into());
        }

        let stdout = String::from_utf8(output.stdout)?;
        info!("Container {} jps output:\n{}", container, stdout);

        // 解析 jps 输出
        for line in stdout.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let process_name_original = parts[1];
                let class_name = process_name_original
                    .split('.')
                    .last()
                    .unwrap_or(process_name_original);

                if EXCLUDED_PROCESSES.iter().any(|&excluded| excluded.eq_ignore_ascii_case(class_name)) {
                    continue;
                }

                let process_name = if full_path {
                    process_name_original.to_string()
                } else {
                    class_name.to_string()
                };

                processes.insert(parts[0].to_string(), process_name);
            }
        }
    }

    Ok(processes)
}

// Detect if Docker is available
async fn is_docker_available() -> bool {
    let output = Command::new("docker")
        .arg("ps")
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false);
    output
}

// Detect if crictl is available
async fn is_crictl_available() -> bool {
    let output = Command::new("crictl")
        .arg("ps")
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false);
    output
}

// Get Java processes from all containers
async fn get_container_java_processes(java_home: Option<&str>, full_path: bool) -> Result<Vec<ProcessInfo>, Box<dyn std::error::Error>> {
    let mut container_processes = Vec::new();

    if is_docker_available().await {
        let containers = list_docker_containers().await?;
        for container in containers {
            match get_java_processes(java_home, full_path, container.clone()).await {
                Ok(procs) => {
                    for (pid, pname) in procs {
                        container_processes.push(ProcessInfo {
                            container: container.clone(),
                            pid,
                            process_name: pname,
                        });
                    }
                }
                Err(e) => {
                    warn!("Failed to get Java processes for Docker container {}: {}", container, e);
                }
            }
        }
    }

    if is_crictl_available().await {
        let containers = list_crictl_containers().await?;
        for container in containers {
            match get_java_processes(java_home, full_path, container.clone()).await {
                Ok(procs) => {
                    for (pid, pname) in procs {
                        container_processes.push(ProcessInfo {
                            container: container.clone(),
                            pid,
                            process_name: pname,
                        });
                    }
                }
                Err(e) => {
                    warn!("Failed to get Java processes for crictl container {}: {}", container, e);
                }
            }
        }
    }

    Ok(container_processes)
}

// List Docker containers
async fn list_docker_containers() -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let output = Command::new("docker")
        .args(&["ps", "--format", "{{.ID}}"])
        .output()
        .await?;

    if !output.status.success() {
        return Err(format!(
            "Failed to list Docker containers: {}",
            String::from_utf8_lossy(&output.stderr)
        ).into());
    }

    let stdout = String::from_utf8(output.stdout)?;
    let containers: Vec<String> = stdout.lines().map(|s| s.to_string()).collect();
    Ok(containers)
}

// List crictl containers
async fn list_crictl_containers() -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let output = Command::new("crictl")
        .args(&["ps", "-q"])
        .output()
        .await?;

    if !output.status.success() {
        return Err(format!(
            "Failed to list crictl containers: {}",
            String::from_utf8_lossy(&output.stderr)
        ).into());
    }

    let stdout = String::from_utf8(output.stdout)?;
    let containers: Vec<String> = stdout.lines().map(|s| s.to_string()).collect();
    Ok(containers)
}

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
