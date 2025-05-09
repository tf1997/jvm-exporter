pub use crate::metrics::metrics::{Metrics, ProcessInfo, EXCLUDED_PROCESSES, JSTAT_COMMANDS, TCP_STATES};
use log::{error, info, warn};
use netstat::{get_sockets_info, AddressFamilyFlags, ProtocolFlags, ProtocolSocketInfo};
use prometheus::{Encoder, GaugeVec, Registry};
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use sysinfo::{Disks, Pid, System};
use tokio::process::Command;

pub(crate) async fn handle_metrics(
    metrics: Arc<Metrics>,
    registry: Arc<Registry>,
    java_home: Arc<Option<String>>,
    full_path: bool,
) -> Result<impl warp::Reply, warp::Rejection> {
    if let Err(err) = update_metrics(metrics.clone(), java_home.as_deref(), full_path).await {
        error!("Failed to update metrics: {}", err);
    }

    let mut buffer = Vec::new();
    let encoder = prometheus::TextEncoder::new();
    let metric_families = registry.gather();
    encoder
        .encode(&metric_families, &mut buffer)
        .expect("Failed to encode metrics");

    let response = warp::http::Response::builder()
        .header("Content-Type", encoder.format_type())
        .body(String::from_utf8(buffer).expect("Failed to convert buffer to String"));
    Ok(response)
}
async fn update_metrics(
    metrics: Arc<Metrics>,
    java_home: Option<&str>,
    full_path: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut all_processes = Vec::new();
    let mut host_process_names: HashSet<String> = HashSet::new();

    // 1. Collect Host Processes
    let host_processes = get_java_processes(java_home, full_path, "host".to_string()).await?;
    info!(
        "Detect and Collect Host Processes: {}",
        host_processes.len()
    );
    for (pid, pname) in host_processes {
        all_processes.push(ProcessInfo {
            container: "host".to_string(),
            pid,
            process: pname.clone(),
        });
        host_process_names.insert(pname.clone());
    }

    // 2. Detect and Collect Container Processes
    let container_processes =
        get_container_java_processes(metrics.clone(), java_home, full_path).await?;
    info!(
        "Detect and Collect Container Processes: {}",
        container_processes.len()
    );
    let filtered_container_processes: Vec<ProcessInfo> = container_processes
        .into_iter()
        .filter(|proc_info| {
            if host_process_names.contains(&proc_info.process) {
                info!(
                    "Skipping container process '{}' in '{}': already exists on host.",
                    proc_info.process, proc_info.container
                );
                false
            } else {
                true
            }
        })
        .collect();

    info!(
        "Filtered Container Processes (excluding duplicates): {}",
        filtered_container_processes.len()
    );
    all_processes.extend(filtered_container_processes);

    // 3. Collect System Processes from Config
    let config = metrics.config.read().unwrap().clone();
    if let Some(system_processes) = &config.system_processes {
        let system_processes_regex: Vec<Regex> = system_processes
            .iter()
            .filter_map(|pattern| Regex::new(pattern).ok())
            .collect();

        let system = System::new_all();
        for (pid, process) in system.processes() {
            let process_name = process.name().to_str().unwrap_or_default().to_string();
            let ppid = process.parent().unwrap_or(Pid::from_u32(0)).as_u32();
            if system_processes_regex
                .iter()
                .any(|re| re.is_match(&process_name))
                && ppid == 1u32
            {
                info!(
                    "System process detected: PID={}, Process={}",
                    pid, process_name
                );
                all_processes.push(ProcessInfo {
                    container: "system".to_string(),
                    pid: pid.to_string(),
                    process: process_name,
                });
            }
        }
    }

    // Create a unique key for each process as "container#pid"
    let current_pids: HashMap<String, String> = all_processes
        .iter()
        .map(|p| (format!("{}#{}", p.container, p.pid), p.process.clone()))
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
            let _ = metrics.process_metrics.cpu_usage.remove_label_values(&[
                container,
                pid,
                process_name,
            ]);
            let _ = metrics.process_metrics.memory_usage.remove_label_values(&[
                container,
                pid,
                process_name,
            ]);
            let _ = metrics
                .process_metrics
                .memory_usage_percentage
                .remove_label_values(&[container, pid, process_name]);
            let _ = metrics.process_metrics.start_time.remove_label_values(&[
                container,
                pid,
                process_name,
            ]);
            let _ = metrics.process_metrics.up_time.remove_label_values(&[
                container,
                pid,
                process_name,
            ]);

            let _ = metrics.process_metrics.open_file.remove_label_values(&[
                container,
                pid,
                process_name,
            ]);

            let _ = metrics
                .process_metrics
                .open_file_limit
                .remove_label_values(&[container, pid, process_name]);

            for state in TCP_STATES {
                let _ = metrics
                    .process_metrics
                    .tcp_connection_states
                    .remove_label_values(&[container, pid, process_name, state]);
            }

            // Remove jstat metrics
            for &command in JSTAT_COMMANDS.iter() {
                let key_jstat = (
                    command,
                    container.to_string(),
                    pid.to_string(),
                    process_name.clone(),
                );
                if let Some(metric_names) = jstat_labels.get(&key_jstat) {
                    if let Some(metric) = metrics.process_metrics.jstat_metrics_map.get(command) {
                        for metric_name in metric_names.iter() {
                            let _ = metric.remove_label_values(&[
                                container,
                                pid,
                                process_name,
                                metric_name,
                            ]);
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

    // Update System metrics
    if let Err(e) = update_system_metrics(Arc::clone(&metrics)).await {
        error!("Failed to update system metrics: {}", e);
    }

    // Update jstat metrics
    let tasks: Vec<_> = all_processes
        .into_iter()
        .filter(|proc_info| proc_info.container != "system")
        .flat_map(|proc_info| {
            let metrics = Arc::clone(&metrics);
            let java_home = java_home.map(|s| s.to_string());
            let container = proc_info.container.clone();
            let pid = proc_info.pid.clone();
            let process = proc_info.process.clone();
            JSTAT_COMMANDS
                .iter()
                .map(move |&command| {
                    let metrics = Arc::clone(&metrics);
                    let java_home = java_home.clone();
                    let container = container.clone();
                    let pid = pid.clone();
                    let process = process.clone();

                    tokio::spawn(async move {
                        if let Some(metric) = metrics.process_metrics.jstat_metrics_map.get(command)
                        {
                            match fetch_and_update_jstat(
                                &container,
                                &pid,
                                &process,
                                command,
                                metric,
                                java_home.as_deref(),
                            )
                            .await
                            {
                                Ok(metric_names) => {
                                    // Record metric_names
                                    let mut jstat_labels = metrics.jstat_labels.lock().await;
                                    let key =
                                        (command, container.clone(), pid.clone(), process.clone());
                                    jstat_labels
                                        .entry(key)
                                        .or_insert_with(HashSet::new)
                                        .extend(metric_names);
                                }
                                Err(err) => {
                                    warn!(
                                        "Failed to update {} metrics for PID {} ({} in {}): {}",
                                        command, pid, process, container, err
                                    );
                                }
                            }
                        }
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect();

    futures::future::join_all(tasks).await;

    Ok(())
}

async fn fetch_and_update_jstat(
    container: &String,
    pid: &String,
    process: &String,
    command: &str,
    jstat_metrics: &GaugeVec,
    java_home: Option<&str>,
) -> Result<HashSet<String>, Box<dyn std::error::Error + Send + Sync>> {
    let mut cmd = if container == "host" {
        let mut command_host = Command::new("jstat");
        command_host.args(&[command, pid, "1000", "1"]);
        if let Some(jh) = java_home {
            command_host.env("JAVA_HOME", jh);
            command_host.env(
                "PATH",
                format!("{}/bin:{}", jh, std::env::var("PATH").unwrap_or_default()),
            );
        }
        command_host
    } else {
        // Execute jstat inside the container
        if is_docker_available().await {
            let mut cmd_docker = Command::new("docker");
            cmd_docker.args(&["exec", container, "jstat", command, pid, "1000", "1"]);
            if let Some(jh) = java_home {
                cmd_docker.env("JAVA_HOME", jh);
                cmd_docker.env(
                    "PATH",
                    format!("{}/bin:{}", jh, std::env::var("PATH").unwrap_or_default()),
                );
            }
            cmd_docker
        } else if is_crictl_available().await {
            let mut cmd_crictl = Command::new("crictl");
            cmd_crictl.args(&["exec", container, "jstat", command, pid, "1000", "1"]);
            if let Some(jh) = java_home {
                cmd_crictl.env("JAVA_HOME", jh);
                cmd_crictl.env(
                    "PATH",
                    format!("{}/bin:{}", jh, std::env::var("PATH").unwrap_or_default()),
                );
            }
            cmd_crictl
        } else {
            return Err(
                "Neither Docker nor crictl is available to execute commands in containers".into(),
            );
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
        )
        .into());
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
        warn!(
           "Mismatch in headers and values count for command {} for PID {} in container {}: headers = {:?}, values = {:?}",
           command, pid, container, headers, values
       );
        // Only process matching header-value pairs
        let min_len = std::cmp::min(headers.len(), values.len());
        for i in 0..min_len {
            let header = headers[i];
            let value = values[i];
            let parsed_value = value.parse::<f64>().unwrap_or(0.0);
            jstat_metrics
                .with_label_values(&[container, pid, process, header])
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
                           "Failed to parse value for {}: {} in PID {} and Process {} in container {}",
                           header, value, pid, process, container
                       );
                        continue;
                    }
                }
            };

            jstat_metrics
                .with_label_values(&[container, pid, process, header])
                .set(parsed_value);
            metric_names.insert(header.to_string());
        }
    }
    Ok(metric_names)
}

// Update CPU and Memory metrics
async fn update_cpu_memory_metrics(
    metrics: Arc<Metrics>,
    processes: &[ProcessInfo],
) -> Result<(), Box<dyn std::error::Error>> {
    let mut system = System::new_all();
    system.refresh_all();

    tokio::time::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL).await;

    system.refresh_processes_specifics(
        sysinfo::ProcessesToUpdate::All,
        true,
        sysinfo::ProcessRefreshKind::nothing().with_cpu(),
    );
    let total_memory_kb = system.total_memory() as f64;

    let af_flags = AddressFamilyFlags::IPV4 | AddressFamilyFlags::IPV6;
    let proto_flags = ProtocolFlags::TCP;

    let sockets = get_sockets_info(af_flags, proto_flags)?;

    for proc_info in processes.iter() {
        let pid_str = &proc_info.pid;
        let container = &proc_info.container;
        let process = &proc_info.process;

        // Extract class name (last part of the package path)
        let class_name = process.split('.').last().unwrap_or(process);

        // Check if class name is in the exclusion list
        if EXCLUDED_PROCESSES
            .iter()
            .any(|&excluded| excluded.eq_ignore_ascii_case(class_name))
        {
            warn!("Excluding process PID {}: {}", pid_str, class_name);
            continue;
        }

        if let Ok(pid) = pid_str.parse::<usize>() {
            if let Some(process_info) = system.process(sysinfo::Pid::from(pid)) {
                // Update CPU usage
                metrics
                    .process_metrics
                    .cpu_usage
                    .with_label_values(&[container, pid_str, process])
                    .set(process_info.cpu_usage() as f64);

                // Update Memory usage (in bytes)
                metrics
                    .process_metrics
                    .memory_usage
                    .with_label_values(&[container, pid_str, process])
                    .set(process_info.memory() as f64); // Convert KB to Bytes

                let process_memory_kb = process_info.memory() as f64;
                let memory_usage_percentage = if total_memory_kb > 0.0 {
                    (process_memory_kb / total_memory_kb) * 100.0
                } else {
                    0.0
                };

                metrics
                    .process_metrics
                    .memory_usage_percentage
                    .with_label_values(&[container, pid_str, process])
                    .set(memory_usage_percentage);

                let start_time_secs = process_info.start_time() as f64;
                let up_time_secs = process_info.run_time() as f64;

                metrics
                    .process_metrics
                    .start_time
                    .with_label_values(&[container, pid_str, process])
                    .set(start_time_secs);

                metrics
                    .process_metrics
                    .up_time
                    .with_label_values(&[container, pid_str, process])
                    .set(up_time_secs);

                let open_file = process_info.open_files().unwrap_or(0) as f64;
                let open_file_limit = process_info.open_files_limit().unwrap_or(0) as f64;
                metrics
                    .process_metrics
                    .open_file
                    .with_label_values(&[container, pid_str, process])
                    .set(open_file);

                metrics
                    .process_metrics
                    .open_file_limit
                    .with_label_values(&[container, pid_str, process])
                    .set(open_file_limit);

                let mut state_counts: HashMap<String, usize> = HashMap::new();

                for state in TCP_STATES {
                    state_counts.insert(state.to_string(), 0);
                }
                for socket in sockets.iter() {
                    let associated_pids = &socket.associated_pids;
                    if let ProtocolSocketInfo::Tcp(tcp_info) = &socket.protocol_socket_info {
                        // 过滤指定进程的连接
                        if associated_pids.contains(&pid_str.parse::<u32>().unwrap_or(0)) {
                            *state_counts.entry(tcp_info.state.to_string()).or_insert(0) += 1;
                        }
                    }
                }
                for (state, count) in state_counts.iter() {
                    metrics
                        .process_metrics
                        .tcp_connection_states
                        .with_label_values(&[container, pid_str, process, state])
                        .set(*count as f64);
                }
            }
        }
    }

    Ok(())
}

async fn update_system_metrics(metrics: Arc<Metrics>) -> Result<(), Box<dyn std::error::Error>> {
    let mut system = System::new_all();
    system.refresh_all();
    // Update Memory usage
    metrics
        .system_metrics
        .memory_usage
        .with_label_values(&["used"])
        .set(system.used_memory() as f64);

    metrics
        .system_metrics
        .total_memory
        .with_label_values(&["total"])
        .set(system.total_memory() as f64);

    // Update Disk usage
    for disk in &Disks::new_with_refreshed_list() {
        let disk_name = disk.name().to_str().unwrap_or("unknown").to_string();
        let mount_point = disk.mount_point().to_str().unwrap_or("/").to_string();
        if mount_point.contains("docker")
            || mount_point.contains("containerd")
            || mount_point.contains("kubelet")
        {
            continue;
        }
        let total_space = disk.total_space() as f64;
        let available_space = disk.available_space() as f64;
        let used_space = total_space - available_space;

        metrics
            .system_metrics
            .disk_usage
            .with_label_values(&[&disk_name, &mount_point])
            .set(used_space);

        metrics
            .system_metrics
            .total_disk
            .with_label_values(&[&disk_name, &mount_point])
            .set(total_space);
    }

    // Update System uptime
    let uptime = System::uptime() as f64; // uptime is in seconds
    metrics
        .system_metrics
        .uptime
        .with_label_values(&["system"])
        .set(uptime);

    // Update Swap memory
    metrics
        .system_metrics
        .total_swap
        .with_label_values(&["total"])
        .set(system.total_swap() as f64);

    metrics
        .system_metrics
        .swap_usage
        .with_label_values(&["used"])
        .set(system.used_swap() as f64);

    let open_file = system
        .processes()
        .iter()
        .map(|(_, process)| process.open_files().unwrap_or(0) as f64)
        .sum::<f64>();

    let open_file_limit = system
        .processes()
        .iter()
        .map(|(_, process)| process.open_files_limit().unwrap_or(0) as f64)
        .sum::<f64>();

    metrics
        .system_metrics
        .open_file
        .with_label_values(&["system"])
        .set(open_file);

    metrics
        .system_metrics
        .open_file_limit
        .with_label_values(&["system"])
        .set(open_file_limit);

    let af_flags = AddressFamilyFlags::IPV4 | AddressFamilyFlags::IPV6;
    let proto_flags = ProtocolFlags::TCP;

    let sockets = get_sockets_info(af_flags, proto_flags)?;
    let mut state_counts: HashMap<String, usize> = HashMap::new();
    
    for state in TCP_STATES {
        state_counts.insert(state.to_string(), 0);
    }
    for socket in sockets.iter() {
        if let ProtocolSocketInfo::Tcp(tcp_info) = &socket.protocol_socket_info {
            *state_counts.entry(tcp_info.state.to_string()).or_insert(0) += 1;
        }
    }

    for (state, count) in state_counts.iter() {
        metrics
            .system_metrics
            .tcp_connection_states
            .with_label_values(&["system", state])
            .set(*count as f64);
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
        if !is_jps_available().await {
            error!("jps command not found. Please ensure that JDK is installed and JAVA_HOME is set correctly.");
            return Ok(processes); // Return empty if jps is not available
        }
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

        for line in stdout.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let process_name_original = parts[1];
                let class_name = process_name_original
                    .split('.')
                    .last()
                    .unwrap_or(process_name_original);

                if EXCLUDED_PROCESSES
                    .iter()
                    .any(|&excluded| excluded.eq_ignore_ascii_case(class_name))
                {
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
        if !is_jps_available_inside_container(&container).await {
            error!("jps command not found inside container {}. Please ensure that JDK is installed in the container.", container);
            return Ok(processes); // Return empty if jps is not available inside the container
        }
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
            return Err(
                "Neither Docker nor crictl is available to execute commands in containers".into(),
            );
        }

        if let Some(jh) = java_home {
            cmd.env("JAVA_HOME", jh);
            cmd.env(
                "PATH",
                format!("{}/bin:{}", jh, std::env::var("PATH").unwrap_or_default()),
            );
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

        for line in stdout.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let process_name_original = parts[1];
                let class_name = process_name_original
                    .split('.')
                    .last()
                    .unwrap_or(process_name_original);

                if EXCLUDED_PROCESSES
                    .iter()
                    .any(|&excluded| excluded.eq_ignore_ascii_case(class_name))
                {
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
async fn get_container_java_processes(
    metrics: Arc<Metrics>,
    java_home: Option<&str>,
    full_path: bool,
) -> Result<Vec<ProcessInfo>, Box<dyn std::error::Error>> {
    let mut container_processes = Vec::new();
    if !metrics
        .config
        .read()
        .unwrap()
        .detect_docker_processes
        .unwrap_or_default()
    {
        return Ok(container_processes);
    }
    if is_docker_available().await {
        let containers = list_docker_containers().await?;
        for container in containers {
            match get_java_processes(java_home, full_path, container.clone()).await {
                Ok(procs) => {
                    for (pid, pname) in procs {
                        container_processes.push(ProcessInfo {
                            container: container.clone(),
                            pid,
                            process: pname,
                        });
                    }
                }
                Err(e) => {
                    warn!(
                        "Failed to get Java processes for Docker container {}: {}",
                        container, e
                    );
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
                            process: pname,
                        });
                    }
                }
                Err(e) => {
                    warn!(
                        "Failed to get Java processes for crictl container {}: {}",
                        container, e
                    );
                }
            }
        }
    }

    Ok(container_processes)
}

async fn is_jps_available() -> bool {
    Command::new("jps")
        .arg("-l")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .map(|status| status.success())
        .unwrap_or(false)
}

async fn is_jps_available_inside_container(container: &str) -> bool {
    if is_docker_available().await {
        Command::new("docker")
            .args(&["exec", container, "jps", "-l"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await
            .map(|status| status.success())
            .unwrap_or(false)
    } else if is_crictl_available().await {
        Command::new("crictl")
            .args(&["exec", container, "jps", "-l"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await
            .map(|status| status.success())
            .unwrap_or(false)
    } else {
        false
    }
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
        )
        .into());
    }

    let stdout = String::from_utf8(output.stdout)?;
    let containers: Vec<String> = stdout.lines().map(|s| s.to_string()).collect();
    Ok(containers)
}

// List crictl containers
async fn list_crictl_containers() -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let output = Command::new("crictl").args(&["ps", "-q"]).output().await?;

    if !output.status.success() {
        return Err(format!(
            "Failed to list crictl containers: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }

    let stdout = String::from_utf8(output.stdout)?;
    let containers: Vec<String> = stdout.lines().map(|s| s.to_string()).collect();
    Ok(containers)
}

fn merge_java_home(
    java_home: Option<&str>,
    command: &mut Command,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(jh) = java_home {
        command.env("JAVA_HOME", jh);
        command.env(
            "PATH",
            format!("{}/bin:{}", jh, std::env::var("PATH").unwrap_or_default()),
        );
    }
    Ok(())
}
