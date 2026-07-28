#[cfg(target_os = "windows")]
use crate::collectors::disk;
pub use crate::metrics::metrics::{
    Metrics, ProcessInfo, EXCLUDED_PROCESSES, JSTAT_COMMANDS, TCP_STATES,
};
use jmon_rs::JvmMonitor;
use log::{error, info, warn};
use netstat_esr::{
    get_sockets_info, AddressFamilyFlags, ProtocolFlags, ProtocolSocketInfo, SocketInfo,
};
use prometheus::{Encoder, GaugeVec, Registry};
use regex::Regex;
use std::collections::{HashMap, HashSet};
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::str::FromStr;
use std::sync::Arc;
use sysinfo::{CpuRefreshKind, Disks, MemoryRefreshKind, Networks, Pid, RefreshKind, System};
use tokio::process::Command;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

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
    let mut collected_pids: HashSet<String> = HashSet::new();

    let af_flags = AddressFamilyFlags::IPV4 | AddressFamilyFlags::IPV6;
    let proto_flags = ProtocolFlags::TCP;
    let sockets = get_sockets_info(af_flags, proto_flags)?;

    let host_processes;
    // 1. Collect Host Processes
    if !metrics
        .config
        .read()
        .unwrap()
        .detect_java_processes
        .unwrap_or_default()
    {
        host_processes = HashMap::new();
    } else {
        host_processes = get_java_processes(java_home, full_path, "host".to_string()).await?;
    }
    for (pid, pname) in host_processes {
        if collected_pids.insert(pid.clone()) {
            all_processes.push(ProcessInfo {
                container: "host".to_string(),
                pid,
                process: pname,
            });
        }
    }

    // 2. Detect and Collect Container Processes
    let container_processes =
        get_container_java_processes(metrics.clone(), java_home, full_path).await?;
    let filtered_container_processes: Vec<ProcessInfo> = container_processes
        .into_iter()
        .filter(|proc_info| {
            if collected_pids.insert(proc_info.pid.clone()) {
                true
            } else {
                info!(
                    "Skipping container process '{}' in '{}': PID {} is already collected.",
                    proc_info.process, proc_info.container, proc_info.pid
                );
                false
            }
        })
        .collect();

    if !filtered_container_processes.is_empty() {
        info!(
            "Filtered Container Processes (excluding duplicates): {}",
            filtered_container_processes.len()
        );
    }
    all_processes.extend(filtered_container_processes);

    let mut system = System::new_all();

    // 3. Collect System Processes from Config
    let config = metrics.config.read().unwrap().clone();
    if let Some(system_processes) = &config.system_processes {
        let system_processes_regex: Vec<Regex> = system_processes
            .iter()
            .filter_map(|pattern| Regex::new(pattern).ok())
            .collect();

        // Use the already initialized 'system' object
        for (pid, process) in system.processes() {
            let process_name = process.name().to_str().unwrap_or_default().to_string();
            let pid = pid.to_string();
            if system_processes_regex
                .iter()
                .any(|re| re.is_match(&process_name))
                && collected_pids.insert(pid.clone())
            {
                all_processes.push(ProcessInfo {
                    container: "system".to_string(),
                    pid,
                    process: process_name,
                });
            }
        }
    }
    let current_process_names: HashSet<(String, String)> = all_processes
        .iter()
        .map(|p| (p.container.clone(), p.process.clone()))
        .collect();

    let previous_process_names: HashSet<(String, String)> = {
        let active_pids_guard = metrics.active_pids.lock().await;
        active_pids_guard
            .iter()
            .map(|(key, process_name)| {
                let container = key.splitn(2, '#').next().unwrap_or("").to_string();
                (container, process_name.clone())
            })
            .collect()
    };
    let offline_names = previous_process_names.difference(&current_process_names);
    for (container, process_name) in offline_names {
        info!(
            "Service '{}' in '{}' is now fully offline. Setting status to 0.0.",
            process_name, container
        );
        metrics
            .process_metrics
            .process_online_status
            .with_label_values(&[container, process_name])
            .set(0.0);
    }
    for (container, process_name) in &current_process_names {
        metrics
            .process_metrics
            .process_online_status
            .with_label_values(&[container, process_name])
            .set(1.0);
    }
    let current_pids_set: HashSet<(String, String, String)> = all_processes
        .iter()
        .map(|p| (p.container.clone(), p.pid.clone(), p.process.clone()))
        .collect();
    let previous_pids_set: HashSet<(String, String, String)> = {
        let active_pids_guard = metrics.active_pids.lock().await;
        active_pids_guard
            .iter()
            .map(|(key, process_name)| {
                let parts: Vec<&str> = key.splitn(2, '#').collect();
                (
                    parts[0].to_string(),
                    parts[1].to_string(),
                    process_name.clone(),
                )
            })
            .collect()
    };
    let pids_to_remove = previous_pids_set.difference(&current_pids_set);

    // Remove non-online-status metrics for restarted/truly offline processes
    for (container, pid, process_name) in pids_to_remove {
        remove_process_metrics(
            &metrics,
            &metrics.jstat_labels,
            &container,
            &pid,
            &process_name,
        )
        .await;
    }

    {
        let mut active_pids_locked = metrics.active_pids.lock().await;
        active_pids_locked.clear();
        for proc_info in all_processes.iter() {
            let key = format!("{}#{}", proc_info.container, proc_info.pid);
            active_pids_locked.insert(key, proc_info.process.clone());
        }
    }

    // Update System metrics
    if let Err(e) = update_system_metrics(Arc::clone(&metrics), &mut system, &sockets).await {
        error!("Failed to update system metrics: {}", e);
    }

    if all_processes.is_empty() {
        // warn!("No processes found to monitor.");
        return Ok(());
    }
    // Update CPU and Memory metrics
    if let Err(e) = update_process_cpu_memory_metrics(
        Arc::clone(&metrics),
        &mut system,
        &sockets,
        &all_processes,
    )
    .await
    {
        error!("Failed to update CPU and memory metrics: {}", e);
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
            if container == "host" {
                let task = tokio::spawn(async move {
                    match fetch_and_update_jstat_host(
                        &container,
                        &pid,
                        &process,
                        &metrics.process_metrics.jstat_metrics_map,
                        &metrics,
                    )
                    .await
                    {
                        Ok(metric_names_map) => {
                            // Record metric_names
                            let mut jstat_labels = metrics.jstat_labels.lock().await;

                            for (&command, metric_names) in &metric_names_map {
                                let key =
                                    (command, container.clone(), pid.clone(), process.clone());
                                jstat_labels
                                    .entry(key)
                                    .or_insert_with(HashSet::new)
                                    .extend(metric_names.iter().cloned());
                            }
                        }
                        Err(err) => {
                            warn!(
                                "Failed to update host jstat metrics for PID {}: {}",
                                pid, err
                            );
                        }
                    }
                });
                vec![task]
            } else {
                JSTAT_COMMANDS
                    .iter()
                    .filter(|&&cmd| cmd != "-compiler" && cmd != "-runtime")
                    .map(move |&command| {
                        let metrics = Arc::clone(&metrics);
                        let java_home = java_home.clone();
                        let container = container.clone();
                        let pid = pid.clone();
                        let process = process.clone();

                        tokio::spawn(async move {
                            if let Some(metric) =
                                metrics.process_metrics.jstat_metrics_map.get(command)
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
                                        let key = (
                                            command,
                                            container.clone(),
                                            pid.clone(),
                                            process.clone(),
                                        );
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
            }
        })
        .collect();
    futures::future::join_all(tasks).await;

    Ok(())
}

// Helper function to remove all metrics associated with a process
async fn remove_process_metrics(
    metrics: &Arc<Metrics>,
    jstat_labels_mutex: &tokio::sync::Mutex<
        HashMap<(&'static str, String, String, String), HashSet<String>>,
    >,
    container: &str,
    pid: &str,
    process_name: &str,
) {
    let mut jstat_labels = jstat_labels_mutex.lock().await;
    let _ = metrics
        .process_metrics
        .cpu_usage
        .remove_label_values(&[container, pid, process_name]);
    let _ =
        metrics
            .process_metrics
            .memory_usage
            .remove_label_values(&[container, pid, process_name]);
    let _ = metrics
        .process_metrics
        .memory_usage_percentage
        .remove_label_values(&[container, pid, process_name]);
    let _ = metrics
        .process_metrics
        .start_time
        .remove_label_values(&[container, pid, process_name]);
    let _ = metrics
        .process_metrics
        .up_time
        .remove_label_values(&[container, pid, process_name]);
    let _ = metrics
        .process_metrics
        .open_file
        .remove_label_values(&[container, pid, process_name]);
    let _ = metrics
        .process_metrics
        .open_file_limit
        .remove_label_values(&[container, pid, process_name]);
    // Do NOT remove process_online_status here. It will be handled explicitly.

    for state in TCP_STATES {
        let _ = metrics
            .process_metrics
            .tcp_connection_states
            .remove_label_values(&[container, pid, process_name, state]);
    }

    for &command in JSTAT_COMMANDS.iter() {
        let key_jstat = (
            command,
            container.to_string(),
            pid.to_string(),
            process_name.to_string(),
        );
        if let Some(metric_names) = jstat_labels.get(&key_jstat) {
            if let Some(metric) = metrics.process_metrics.jstat_metrics_map.get(command) {
                for metric_name in metric_names.iter() {
                    let _ =
                        metric.remove_label_values(&[container, pid, process_name, metric_name]);
                }
            }
        }
        jstat_labels.remove(&key_jstat);
    }
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
        #[cfg(target_os = "windows")]
        command_host.creation_flags(CREATE_NO_WINDOW);
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
            #[cfg(target_os = "windows")]
            cmd_docker.creation_flags(CREATE_NO_WINDOW);
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
            #[cfg(target_os = "windows")]
            cmd_crictl.creation_flags(CREATE_NO_WINDOW);
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

async fn fetch_and_update_jstat_host(
    container: &String,
    pid: &String,
    process: &String,
    metrics_map: &HashMap<&'static str, GaugeVec>,
    metrics: &Arc<Metrics>,
) -> Result<HashMap<&'static str, HashSet<String>>, Box<dyn std::error::Error + Send + Sync>> {
    if !metrics.jvm_monitors.contains_key(pid) {
        match JvmMonitor::connect(pid.parse()?) {
            Ok(m) => {
                metrics.jvm_monitors.insert(pid.clone(), m);
            }
            Err(e) => {
                eprintln!("Failed to connect to JVM monitor for PID {}: {}", pid, e);
            }
        }
    }

    let mut results = HashMap::new();

    if let Some(m) = metrics.jvm_monitors.get(pid) {
        if m.read_string("sun.rt.javaCommand") == "-" {
            metrics.jvm_monitors.remove(pid);
            return Err("JVM PerfData is stale, cache cleared".into());
        }

        if let Some(jstat_metrics) = metrics_map.get("-gc") {
            let mut metric_names = HashSet::new();
            let gc = m.get_gc_stats();
            let mut update = |name: &str, val: f64| {
                jstat_metrics
                    .with_label_values(&[container, pid, process, name])
                    .set(val);
                metric_names.insert(name.to_string());
            };

            update("S0C", gc.s0c);
            update("S1C", gc.s1c);
            update("S0U", gc.s0u);
            update("S1U", gc.s1u);
            update("EC", gc.ec);
            update("EU", gc.eu);
            update("OC", gc.oc);
            update("OU", gc.ou);
            update("MC", gc.mc);
            update("MU", gc.mu);
            update("CCSC", gc.ccsc);
            update("CCSU", gc.ccsu);
            update("YGC", gc.ygc as f64);
            update("YGCT", gc.ygct);
            update("FGC", gc.fgc as f64);
            update("FGCT", gc.fgct);
            update("CGC", gc.cgc as f64);
            update("CGCT", gc.cgct);
            update("GCT", gc.gct);

            results.insert("-gc", metric_names);
        }

        if let Some(jstat_metrics) = metrics_map.get("-class") {
            let mut metric_names = HashSet::new();
            let cs = m.get_class_stats();
            let mut update = |name: &str, val: f64| {
                jstat_metrics
                    .with_label_values(&[container, pid, process, name])
                    .set(val);
                metric_names.insert(name.to_string());
            };
            update("Loaded", cs.loaded as f64);
            update("BytesLoaded", cs.bytes);
            update("Unloaded", cs.unloaded as f64);
            update("BytesUnloaded", cs.unloaded_bytes);
            update("Time", cs.time);

            results.insert("-class", metric_names);
        }

        if let Some(jstat_metrics) = metrics_map.get("-compiler") {
            let mut metric_names = HashSet::new();
            let cps = m.get_compiler_stats();
            let mut update = |name: &str, val: f64| {
                jstat_metrics
                    .with_label_values(&[container, pid, process, name])
                    .set(val);
                metric_names.insert(name.to_string());
            };
            update("Compiled", cps.compiled as f64);
            update("Failed", cps.failed as f64);
            update("Invalid", cps.invalid as f64);
            update("Time", cps.time);

            results.insert("-compiler", metric_names);
        }

        if let Some(jstat_metrics) = metrics_map.get("-runtime") {
            let mut metric_names = HashSet::new();
            let rts = m.get_runtime_stats();
            let mut update = |name: &str, val: f64| {
                jstat_metrics
                    .with_label_values(&[container, pid, process, name])
                    .set(val);
                metric_names.insert(name.to_string());
            };
            update("AppTimeSenconds", rts.app_time_s);
            update("CodeCacheCapacity", rts.code_cache_capacity);
            update("CodeCacheUsed", rts.code_cache_used);
            update("CodeCacheUtilization", rts.code_cache_utilization);
            update("SafepointOverhead", rts.safepoint_overhead);
            update("SafepointTimeSeconds", rts.safepoint_time_s);
            update("Safepoints", rts.safepoints as f64);
            update("ThreadsDaemon", rts.threads_daemon as f64);
            update("ThreadsLive", rts.threads_live as f64);
            update("ThreadsPeak", rts.threads_peak as f64);

            results.insert("-runtime", metric_names);
        }
    }

    Ok(results)
}

// Update CPU and Memory metrics
async fn update_process_cpu_memory_metrics(
    metrics: Arc<Metrics>,
    system: &mut System,    // Pass system as mutable reference
    sockets: &[SocketInfo], // Pass sockets as reference (changed from ProtocolSocketInfo)
    processes: &[ProcessInfo],
) -> Result<(), Box<dyn std::error::Error>> {
    // No need for System::new_all() or system.refresh_all() here, as it's done in update_metrics
    tokio::time::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL).await;

    let pids: Vec<Pid> = processes
        .iter()
        .filter_map(|p| Pid::from_str(p.pid.as_str()).ok())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    system.refresh_processes_specifics(
        sysinfo::ProcessesToUpdate::Some(&pids), // Refresh all processes
        true,                                    // Refresh process components
        sysinfo::ProcessRefreshKind::nothing() // Changed from new() to nothing()
            .with_cpu()
            .with_memory()
            .with_disk_usage(), // Removed .with_io()
    );
    let total_memory_kb = system.total_memory() as f64;

    // Pre-process sockets to map PIDs to their associated TCP connections
    let mut pid_to_sockets: HashMap<u32, Vec<&SocketInfo>> = HashMap::new(); // Changed to SocketInfo
    for socket in sockets.iter() {
        for &pid in &socket.associated_pids {
            // Access associated_pids directly
            pid_to_sockets.entry(pid).or_default().push(socket);
        }
    }

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

        if let Ok(pid_u32) = pid_str.parse::<u32>() {
            if let Some(process_info) = system.process(sysinfo::Pid::from(pid_u32 as usize)) {
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

                if let Some(process_sockets) = pid_to_sockets.get(&pid_u32) {
                    for socket in process_sockets.iter() {
                        if let ProtocolSocketInfo::Tcp(tcp_info) = &socket.protocol_socket_info {
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

async fn update_system_metrics(
    metrics: Arc<Metrics>,
    system: &mut System,    // Pass system as mutable reference
    sockets: &[SocketInfo], // Pass sockets as reference (changed from ProtocolSocketInfo)
) -> Result<(), Box<dyn std::error::Error>> {
    // Rely on system.refresh_all() in update_metrics for overall refresh
    // Individual refreshes removed to avoid potential issues and redundancy
    // system.refresh_cpu();
    // system.refresh_memory();
    // Update Memory usage
    system.refresh_specifics(
        RefreshKind::nothing()
            .with_cpu(CpuRefreshKind::nothing().with_cpu_usage())
            .with_memory(MemoryRefreshKind::everything()),
    );
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
        let file_system = disk.file_system().to_str().unwrap_or("unknown").to_string();
        let kind = disk.kind().to_string();
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
            .with_label_values(&[&disk_name, &mount_point, &file_system, &kind])
            .set(used_space);

        metrics
            .system_metrics
            .total_disk
            .with_label_values(&[&disk_name, &mount_point, &file_system, &kind])
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

    let open_file = if cfg!(target_os = "linux") {
        if let Ok(content) = std::fs::read_to_string("/proc/sys/fs/file-nr") {
            let parts: Vec<&str> = content.split_whitespace().collect();
            if let Some(count) = parts.get(0).and_then(|s| s.parse::<u64>().ok()) {
                count as f64
            } else {
                0.0
            }
        } else {
            0.0
        }
    } else {
        system
            .processes()
            .iter()
            .map(|(_, process)| process.open_files().unwrap_or(0) as f64)
            .sum::<f64>()
    };

    let open_file_limit = System::open_files_limit().unwrap_or(0) as f64;

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

    let mut state_counts: HashMap<String, usize> = HashMap::new();

    for state in TCP_STATES {
        state_counts.insert(state.to_string(), 0);
    }
    for socket in sockets.iter() {
        if let ProtocolSocketInfo::Tcp(tcp_info) = &socket.protocol_socket_info {
            // Keep & here, as socket is &SocketInfo
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

    for (interface_name, data) in &Networks::new_with_refreshed_list() {
        let received = data.total_received() as f64;
        let transmitted = data.total_transmitted() as f64;
        metrics
            .system_metrics
            .network_receive_bytes_total
            .with_label_values(&[interface_name])
            .set(received);

        metrics
            .system_metrics
            .network_transmit_bytes_total
            .with_label_values(&[interface_name])
            .set(transmitted);
    }

    #[cfg(target_os = "windows")]
    {
        let disks = disk::new_collector().collect().unwrap();

        for disk in disks {
            metrics
                .system_metrics
                .disk_smart_health_status
                .with_label_values(&[&disk.name, &disk.model, &disk.serial, &disk.raw_status])
                .set(disk.health_ok as f64);
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
        match JvmMonitor::discover_all() {
            Ok(pis) => {
                for p in pis {
                    let process_name = p.name;
                    let pid = p.pid;
                    let class_name = process_name.split('.').last().unwrap_or(&process_name);

                    if EXCLUDED_PROCESSES
                        .iter()
                        .any(|&excluded| excluded.eq_ignore_ascii_case(class_name))
                    {
                        continue;
                    }

                    let final_process_name = if full_path {
                        process_name.clone()
                    } else {
                        class_name.to_string()
                    };

                    processes.insert(pid.to_string(), final_process_name);
                }
                return Ok(processes);
            }
            Err(e) => {
                warn!("Failed to discover Java processes using jmon_rs: {}. Falling back to jps command.", e);
                return Err(format!("Failed to discover Java processes for host: {:?}", e).into());
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
            #[cfg(target_os = "windows")]
            cmd.creation_flags(CREATE_NO_WINDOW);
            cmd.args(&["exec", &container, "jps", "-l"]);
            info!("Executing jps inside Docker container: {}", container);
        } else if is_crictl_available().await {
            cmd = Command::new("crictl");
            #[cfg(target_os = "windows")]
            cmd.creation_flags(CREATE_NO_WINDOW);
            cmd.args(&["exec", &container, "jps", "-l"]);
            info!("Executing jps inside crictl container: {}", container);
        } else {
            return Err(
                "Neither Docker nor crictl is available to execute commands in containers".into(),
            );
        }

        if let Some(jh) = java_home {
            #[cfg(target_os = "windows")]
            cmd.creation_flags(CREATE_NO_WINDOW);
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
    let mut cmd = Command::new("docker");
    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let output = cmd
        .arg("ps")
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false);
    output
}

// Detect if crictl is available
async fn is_crictl_available() -> bool {
    let mut cmd = Command::new("crictl");
    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let output = cmd
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
    let mut cmd = Command::new("jps");
    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd.arg("-l")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .map(|status| status.success())
        .unwrap_or(false)
}

async fn is_jps_available_inside_container(container: &str) -> bool {
    if is_docker_available().await {
        let mut cmd = Command::new("docker");
        #[cfg(target_os = "windows")]
        cmd.creation_flags(CREATE_NO_WINDOW);
        cmd.args(&["exec", container, "jps", "-l"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await
            .map(|status| status.success())
            .unwrap_or(false)
    } else if is_crictl_available().await {
        let mut cmd = Command::new("crictl");
        #[cfg(target_os = "windows")]
        cmd.creation_flags(CREATE_NO_WINDOW);
        cmd.args(&["exec", container, "jps", "-l"])
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
    let mut command = Command::new("docker");
    #[cfg(target_os = "windows")]
    command.creation_flags(CREATE_NO_WINDOW);
    let output = command
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
    let mut command = Command::new("crictl");
    #[cfg(target_os = "windows")]
    command.creation_flags(CREATE_NO_WINDOW);
    let output = command.args(&["ps", "-q"]).output().await?;

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
        #[cfg(target_os = "windows")]
        command.creation_flags(CREATE_NO_WINDOW);
        command.env("JAVA_HOME", jh);
        command.env(
            "PATH",
            format!("{}/bin:{}", jh, std::env::var("PATH").unwrap_or_default()),
        );
    }
    Ok(())
}
