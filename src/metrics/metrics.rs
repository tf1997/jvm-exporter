use crate::config::Config;
use prometheus::{GaugeVec, Registry};
use std::collections::{HashMap, HashSet};
use tokio::sync::Mutex;
use std::sync::{Arc, RwLock};

pub const JSTAT_COMMANDS: &[&str] = &["-gc", "-class"];
pub const EXCLUDED_PROCESSES: &[&str] = &["jps"];
pub struct Metrics {
    pub(crate) config: Arc<RwLock<Config>>,
    pub(crate) process_metrics: ProcessMetrics,
    pub(crate) system_metrics: SystemMetrics,
    pub(crate) active_pids: Mutex<HashMap<String, String>>, // Key: container#pid
    pub(crate) jstat_labels:
        Mutex<HashMap<(&'static str, String, String, String), HashSet<String>>>, // (command, container, pid, process_name)
}

pub(crate) struct ProcessMetrics {
    pub(crate) cpu_usage: GaugeVec,
    pub(crate) memory_usage: GaugeVec,
    pub(crate) memory_usage_percentage: GaugeVec,
    pub(crate) start_time: GaugeVec,
    pub(crate) up_time: GaugeVec,
    pub(crate) jstat_metrics_map: HashMap<&'static str, GaugeVec>,
}

pub(crate) struct SystemMetrics {
    pub(crate) cpu_usage: GaugeVec,
    pub(crate) memory_usage: GaugeVec,
    pub(crate) total_memory: GaugeVec,
    pub(crate) disk_usage: GaugeVec,
    pub(crate) total_disk: GaugeVec,
    pub(crate) network_receive_bytes_per_sec: GaugeVec,
    pub(crate) network_transmit_bytes_per_sec: GaugeVec,
    pub(crate) uptime: GaugeVec,
    pub(crate) total_swap: GaugeVec,
    pub(crate) swap_usage: GaugeVec,
}

impl Metrics {
    pub(crate) fn new(registry: &Registry, config: Arc<RwLock<Config>>) -> Self {
        // Initialize Process Metrics
        let process_metrics = {
            // CPU Usage
            let cpu_usage = GaugeVec::new(
                prometheus::Opts::new("process_cpu_usage", "CPU usage percentage of the process"),
                &["container", "pid", "process_name"],
            )
            .expect("Failed to create process_cpu_usage GaugeVec");
            registry
                .register(Box::new(cpu_usage.clone()))
                .expect("Failed to register process_cpu_usage metric");

            // Memory Usage
            let memory_usage = GaugeVec::new(
                prometheus::Opts::new(
                    "process_memory_usage_bytes",
                    "Memory usage in bytes of the process",
                ),
                &["container", "pid", "process_name"],
            )
            .expect("Failed to create process_memory_usage_bytes GaugeVec");
            registry
                .register(Box::new(memory_usage.clone()))
                .expect("Failed to register process_memory_usage_bytes metric");

            // Memory Usage Percentage
            let memory_usage_percentage = GaugeVec::new(
                prometheus::Opts::new(
                    "process_memory_usage_percentage",
                    "Memory usage percentage of the process",
                ),
                &["container", "pid", "process_name"],
            )
            .expect("Failed to create process_memory_usage_percentage GaugeVec");
            registry
                .register(Box::new(memory_usage_percentage.clone()))
                .expect("Failed to register process_memory_usage_percentage metric");

            // Start Time
            let start_time = GaugeVec::new(
                prometheus::Opts::new(
                    "process_start_time_seconds",
                    "Start time of the process in seconds since the epoch",
                ),
                &["container", "pid", "process_name"],
            )
            .expect("Failed to create process_start_time_seconds GaugeVec");
            registry
                .register(Box::new(start_time.clone()))
                .expect("Failed to register process_start_time_seconds metric");

            // Up Time
            let up_time = GaugeVec::new(
                prometheus::Opts::new(
                    "process_up_time_seconds",
                    "Up time of the process in seconds",
                ),
                &["container", "pid", "process_name"],
            )
            .expect("Failed to create process_up_time_seconds GaugeVec");
            registry
                .register(Box::new(up_time.clone()))
                .expect("Failed to register process_up_time_seconds metric");

            // jstat Metrics
            let mut jstat_metrics_map = HashMap::new();
            for &cmd in JSTAT_COMMANDS.iter() {
                let metric = GaugeVec::new(
                    prometheus::Opts::new(
                        format!("jstat_{}_metrics", &cmd[1..]),
                        format!("Metrics from jstat {}", cmd),
                    ),
                    &["container", "pid", "process_name", "metric_name"],
                )
                .expect(&format!("Failed to create GaugeVec for command {}", cmd));
                registry
                    .register(Box::new(metric.clone()))
                    .expect(&format!("Failed to register metric for {}", cmd));
                jstat_metrics_map.insert(cmd, metric);
            }

            ProcessMetrics {
                cpu_usage,
                memory_usage,
                memory_usage_percentage,
                start_time,
                up_time,
                jstat_metrics_map,
            }
        };

        // Initialize System Metrics
        let system_metrics = {
            // System CPU Usage
            let cpu_usage = GaugeVec::new(
                prometheus::Opts::new(
                    "system_cpu_usage_percentage",
                    "Total system CPU usage percentage",
                ),
                &["cpu"],
            )
            .expect("Failed to create system_cpu_usage_percentage GaugeVec");
            registry
                .register(Box::new(cpu_usage.clone()))
                .expect("Failed to register system_cpu_usage_percentage metric");

            // System Memory Usage
            let memory_usage = GaugeVec::new(
                prometheus::Opts::new(
                    "system_memory_usage_bytes",
                    "Total system memory usage in bytes",
                ),
                &["memory_type"],
            )
            .expect("Failed to create system_memory_usage_bytes GaugeVec");
            registry
                .register(Box::new(memory_usage.clone()))
                .expect("Failed to register system_memory_usage_bytes metric");

            // System Total Memory
            let total_memory = GaugeVec::new(
                prometheus::Opts::new("system_total_memory_bytes", "Total system memory in bytes"),
                &["memory_type"],
            )
            .expect("Failed to create system_total_memory_bytes GaugeVec");
            registry
                .register(Box::new(total_memory.clone()))
                .expect("Failed to register system_total_memory_bytes metric");

            // System Disk Usage
            let disk_usage = GaugeVec::new(
                prometheus::Opts::new("system_disk_usage_bytes", "Disk usage in bytes"),
                &["disk", "mount_point"],
            )
            .expect("Failed to create system_disk_usage_bytes GaugeVec");
            registry
                .register(Box::new(disk_usage.clone()))
                .expect("Failed to register system_disk_usage_bytes metric");

            // System Total Disk
            let total_disk = GaugeVec::new(
                prometheus::Opts::new("system_total_disk_bytes", "Total disk space in bytes"),
                &["disk", "mount_point"],
            )
            .expect("Failed to create system_total_disk_bytes GaugeVec");
            registry
                .register(Box::new(total_disk.clone()))
                .expect("Failed to register system_total_disk_bytes metric");

            // Network Receive Bytes Per Sec
            let network_receive_bytes_per_sec = GaugeVec::new(
                prometheus::Opts::new(
                    "system_network_receive_bytes_per_sec",
                    "Network receive rate in bytes per second",
                ),
                &["interface"],
            )
            .expect("Failed to create system_network_receive_bytes_per_sec GaugeVec");
            registry
                .register(Box::new(network_receive_bytes_per_sec.clone()))
                .expect("Failed to register system_network_receive_bytes_per_sec metric");

            // Network Transmit Bytes Per Sec
            let network_transmit_bytes_per_sec = GaugeVec::new(
                prometheus::Opts::new(
                    "system_network_transmit_bytes_per_sec",
                    "Network transmit rate in bytes per second",
                ),
                &["interface"],
            )
            .expect("Failed to create system_network_transmit_bytes_per_sec GaugeVec");
            registry
                .register(Box::new(network_transmit_bytes_per_sec.clone()))
                .expect("Failed to register system_network_transmit_bytes_per_sec metric");

            // System Uptime
            let uptime = GaugeVec::new(
                prometheus::Opts::new("system_uptime_seconds", "Total system uptime in seconds"),
                &["type"],
            )
            .expect("Failed to create system_uptime_seconds GaugeVec");
            registry
                .register(Box::new(uptime.clone()))
                .expect("Failed to register system_uptime_seconds metric");

            // System Swap Total Bytes
            let total_swap = GaugeVec::new(
                prometheus::Opts::new("system_total_swap_bytes", "Total swap memory in bytes"),
                &["swap_type"],
            )
            .expect("Failed to create system_total_swap GaugeVec");
            registry
                .register(Box::new(total_swap.clone()))
                .expect("Failed to register system_total_swap metric");

            // System Swap Used Bytes
            let swap_usage = GaugeVec::new(
                prometheus::Opts::new("system_swap_usage_bytes", "Used swap memory in bytes"),
                &["swap_type"],
            )
            .expect("Failed to create system_swap_usage GaugeVec");
            registry
                .register(Box::new(swap_usage.clone()))
                .expect("Failed to register system_swap_usage metric");

            SystemMetrics {
                cpu_usage,
                memory_usage,
                total_memory,
                disk_usage,
                total_disk,
                network_receive_bytes_per_sec,
                network_transmit_bytes_per_sec,
                uptime,
                total_swap,
                swap_usage,
            }
        };

        Metrics {
            process_metrics,
            system_metrics,
            active_pids: Mutex::new(HashMap::new()),
            jstat_labels: Mutex::new(HashMap::new()),
            config,
        }
    }
}
pub struct ProcessInfo {
    pub(crate) container: String, // "host" or container ID
    pub(crate) pid: String,
    pub(crate) process: String,
}
