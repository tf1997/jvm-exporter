use crate::config::Config;
use prometheus::{GaugeVec, IntGaugeVec, Registry};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use tokio::sync::Mutex;

pub const JSTAT_COMMANDS: &[&str] = &["-gc", "-class"];
pub const EXCLUDED_PROCESSES: &[&str] = &["jps"];
pub const TCP_STATES: &[&str] = &[
    "CLOSED",
    "LISTEN",
    "SYN_SENT",
    "SYN_RCVD",
    "ESTABLISHED",
    "FIN_WAIT_1",
    "FIN_WAIT_2",
    "CLOSE_WAIT",
    "CLOSING",
    "LAST_ACK",
    "TIME_WAIT",
    "DELETE_TCB",
];
pub struct Metrics {
    pub(crate) config: Arc<RwLock<Config>>,
    pub(crate) process_metrics: ProcessMetrics,
    pub(crate) system_metrics: SystemMetrics,
    pub(crate) active_pids: Mutex<HashMap<String, String>>, // Key: container#pid
    pub(crate) jstat_labels:
        Mutex<HashMap<(&'static str, String, String, String), HashSet<String>>>, // (command, container, pid, process_name)
    pub(crate) probe_metrics: ProbeMetrics,
    pub(crate) version: GaugeVec,
    pub(crate) os_version_info: IntGaugeVec,
}

pub(crate) struct ProcessMetrics {
    pub(crate) cpu_usage: GaugeVec,
    pub(crate) process_online_status: GaugeVec,
    pub(crate) memory_usage: GaugeVec,
    pub(crate) memory_usage_percentage: GaugeVec,
    pub(crate) start_time: GaugeVec,
    pub(crate) up_time: GaugeVec,
    pub(crate) open_file: GaugeVec,
    pub(crate) open_file_limit: GaugeVec,
    pub(crate) tcp_connection_states: GaugeVec,
    pub(crate) jstat_metrics_map: HashMap<&'static str, GaugeVec>,
}

pub(crate) struct SystemMetrics {
    pub(crate) cpu_usage: GaugeVec,
    pub(crate) memory_usage: GaugeVec,
    pub(crate) total_memory: GaugeVec,
    pub(crate) disk_usage: GaugeVec,
    pub(crate) total_disk: GaugeVec,
    pub(crate) disk_smart_health_status: GaugeVec,
    pub(crate) network_receive_bytes_per_sec: GaugeVec,
    pub(crate) network_transmit_bytes_per_sec: GaugeVec,
    pub(crate) network_receive_bytes_total: GaugeVec,
    pub(crate) network_transmit_bytes_total: GaugeVec,
    pub(crate) network_link_speed: GaugeVec,
    pub(crate) network_info: GaugeVec,
    pub(crate) uptime: GaugeVec,
    pub(crate) total_swap: GaugeVec,
    pub(crate) swap_usage: GaugeVec,
    pub(crate) open_file: GaugeVec,
    pub(crate) open_file_limit: GaugeVec,
    pub(crate) tcp_connection_states: GaugeVec,
}

pub(crate) struct ProbeMetrics {
    pub(crate) probe_tcp_success: GaugeVec,
    pub(crate) probe_tcp_duration_seconds: GaugeVec,
    pub(crate) probe_ping_success: GaugeVec,
    pub(crate) probe_ping_duration_seconds: GaugeVec,
    pub(crate) probe_http_success: GaugeVec,
    pub(crate) probe_http_duration_seconds: GaugeVec,
    pub(crate) probe_http_status_code: GaugeVec,
    pub(crate) probe_http_ssl_earliest_cert_expiry: GaugeVec,
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

            let open_file = GaugeVec::new(
                prometheus::Opts::new("process_open_file", "Used open file descriptors"),
                &["container", "pid", "process_name"],
            )
            .expect("Failed to create process_open_file GaugeVec");
            registry
                .register(Box::new(open_file.clone()))
                .expect("Failed to register process_open_file metric");

            let open_file_limit = GaugeVec::new(
                prometheus::Opts::new("process_open_file_limit", "Max open file descriptors"),
                &["container", "pid", "process_name"],
            )
            .expect("Failed to create process_open_file_limit GaugeVec");
            registry
                .register(Box::new(open_file_limit.clone()))
                .expect("Failed to register process_open_file_limit metric");

            let tcp_connection_states = GaugeVec::new(
                prometheus::Opts::new(
                    "process_tcp_connection_states",
                    "Number of TCP connections in different states for the process",
                ),
                &["container", "pid", "process_name", "state"], // 添加 state 标签
            )
            .expect("Failed to create process_tcp_connection_states GaugeVec");
            registry
                .register(Box::new(tcp_connection_states.clone()))
                .expect("Failed to register process_tcp_connection_states metric");

            // Process Online Status
            let process_online_status = GaugeVec::new(
                prometheus::Opts::new(
                    "process_online_status",
                    "Online status of the process (1 = online, 0 = offline)",
                ),
                &["container", "process_name"],
            )
            .expect("Failed to create process_online_status GaugeVec");
            registry
                .register(Box::new(process_online_status.clone()))
                .expect("Failed to register process_online_status metric");

            ProcessMetrics {
                cpu_usage,
                memory_usage,
                memory_usage_percentage,
                start_time,
                up_time,
                jstat_metrics_map,
                open_file,
                open_file_limit,
                tcp_connection_states,
                process_online_status,
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
                &["disk", "mount_point", "filesystem", "kind"],
            )
            .expect("Failed to create system_disk_usage_bytes GaugeVec");
            registry
                .register(Box::new(disk_usage.clone()))
                .expect("Failed to register system_disk_usage_bytes metric");

            // System Total Disk
            let total_disk = GaugeVec::new(
                prometheus::Opts::new("system_total_disk_bytes", "Total disk space in bytes"),
                &["disk", "mount_point", "filesystem", "kind"],
            )
            .expect("Failed to create system_total_disk_bytes GaugeVec");
            registry
                .register(Box::new(total_disk.clone()))
                .expect("Failed to register system_total_disk_bytes metric");

            let disk_smart_health_status = GaugeVec::new(
                prometheus::Opts::new("system_disk_smart_health_status", "SMART health status of the disk (1 = OK, 0 = Failing)"),
                &["disk", "model", "serial", "raw_status"],
            )
            .expect("Failed to create disk_smart_health_status GaugeVec");
            registry
                .register(Box::new(disk_smart_health_status.clone()))
                .expect("Failed to register disk_smart_health_status metric");

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

            let network_receive_bytes_total = GaugeVec::new(
                prometheus::Opts::new(
                    "system_network_receive_bytes_total",
                    "Total number of bytes received on an interface",
                ),
                &["interface"],
            )
            .expect("Failed to create system_network_receive_bytes_total GaugeVec");
            registry
                .register(Box::new(network_receive_bytes_total.clone()))
                .expect("Failed to register system_network_receive_bytes_total metric");

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

            let network_transmit_bytes_total = GaugeVec::new(
                prometheus::Opts::new(
                    "system_network_transmit_bytes_total",
                    "Total number of bytes transmitted on an interface",
                ),
                &["interface"],
            )
            .expect("Failed to create system_network_transmit_bytes GaugeVec");
            registry
                .register(Box::new(network_transmit_bytes_total.clone()))
                .expect("Failed to register system_network_transmit_bytes metric");

            let network_link_speed = GaugeVec::new(
                prometheus::Opts::new(
                    "system_network_link_speed",
                    "Network interface link speed in Mbps",
                ),
                &["interface"],
            )
            .expect("Failed to create system_network_link_speed GaugeVec");
            registry
                .register(Box::new(network_link_speed.clone()))
                .expect("Failed to register system_network_link_speed metric");

            let network_info = GaugeVec::new(
                prometheus::Opts::new("system_network_interface_info", "Network interface info"),
                &["interface", "mac", "ip", "type", "gateway", "dns"],
            )
            .expect("Failed to create system_network_info GaugeVec");
            registry
                .register(Box::new(network_info.clone()))
                .expect("Failed to register system_network_interface_info metric");

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

            let open_file = GaugeVec::new(
                prometheus::Opts::new("system_open_file", "Used open file descriptors"),
                &["type"],
            )
            .expect("Failed to create system_open_file GaugeVec");
            registry
                .register(Box::new(open_file.clone()))
                .expect("Failed to register system_open_file metric");

            let open_file_limit = GaugeVec::new(
                prometheus::Opts::new("system_open_file_limit", "Max open file descriptors"),
                &["type"],
            )
            .expect("Failed to create system_open_file_limit GaugeVec");
            registry
                .register(Box::new(open_file_limit.clone()))
                .expect("Failed to register system_open_file_limit metric");

            let tcp_connection_states = GaugeVec::new(
                prometheus::Opts::new(
                    "system_tcp_connection_states",
                    "Number of TCP connections in different states for the system",
                ),
                &["type", "state"], // 添加 state 标签
            )
            .expect("Failed to create system_tcp_connection_states GaugeVec");
            registry
                .register(Box::new(tcp_connection_states.clone()))
                .expect("Failed to register system_tcp_connection_states metric");

            SystemMetrics {
                cpu_usage,
                memory_usage,
                total_memory,
                disk_usage,
                total_disk,
                disk_smart_health_status,
                network_receive_bytes_per_sec,
                network_receive_bytes_total,
                network_transmit_bytes_per_sec,
                network_transmit_bytes_total,
                network_link_speed,
                network_info,
                uptime,
                total_swap,
                swap_usage,
                open_file,
                open_file_limit,
                tcp_connection_states,
            }
        };

        let probe_metrics = {
            let probe_tcp_success = GaugeVec::new(
                prometheus::Opts::new("probe_tcp_success", "TCP probe success status"),
                &["host", "port"],
            )
            .expect("Failed to create probe_tcp_success GaugeVec");
            registry
                .register(Box::new(probe_tcp_success.clone()))
                .expect("Failed to register probe_tcp_success metric");

            let probe_tcp_duration_seconds = GaugeVec::new(
                prometheus::Opts::new(
                    "probe_tcp_duration_seconds",
                    "Duration of TCP probe in seconds",
                ),
                &["host", "port"],
            )
            .expect("Failed to create probe_tcp_duration_seconds GaugeVec");
            registry
                .register(Box::new(probe_tcp_duration_seconds.clone()))
                .expect("Failed to register probe_tcp_duration_seconds metric");

            let probe_ping_success = GaugeVec::new(
                prometheus::Opts::new("probe_ping_success", "Ping probe success status"),
                &["host"],
            )
            .expect("Failed to create probe_ping_success GaugeVec");
            registry
                .register(Box::new(probe_ping_success.clone()))
                .expect("Failed to register probe_ping_success metric");

            let probe_ping_duration_seconds = GaugeVec::new(
                prometheus::Opts::new(
                    "probe_ping_duration_seconds",
                    "Duration of Ping probe in seconds",
                ),
                &["host"],
            )
            .expect("Failed to create probe_ping_duration_seconds GaugeVec");
            registry
                .register(Box::new(probe_ping_duration_seconds.clone()))
                .expect("Failed to register probe_ping_duration_seconds metric");

            let probe_http_success = GaugeVec::new(
                prometheus::Opts::new("probe_http_success", "HTTP probe success status"),
                &["target"],
            )
            .expect("Failed to create probe_http_success GaugeVec");
            registry
                .register(Box::new(probe_http_success.clone()))
                .expect("Failed to register probe_http_success metric");

            let probe_http_duration_seconds = GaugeVec::new(
                prometheus::Opts::new(
                    "probe_http_duration_seconds",
                    "Duration of HTTP probe in seconds",
                ),
                &["target"],
            )
            .expect("Failed to create probe_http_duration_seconds GaugeVec");
            registry
                .register(Box::new(probe_http_duration_seconds.clone()))
                .expect("Failed to register probe_http_duration_seconds metric");

            let probe_http_status_code = GaugeVec::new(
                prometheus::Opts::new("probe_http_status_code", "HTTP probe status code"),
                &["target"],
            )
            .expect("Failed to create probe_http_status_code GaugeVec");
            registry
                .register(Box::new(probe_http_status_code.clone()))
                .expect("Failed to register probe_http_status_code metric");

            let probe_http_ssl_earliest_cert_expiry = GaugeVec::new(
                prometheus::Opts::new("probe_http_ssl_earliest_cert_expiry", "Earliest SSL certificate expiry in seconds"),
                &["target"],
            )
            .expect("Failed to create probe_http_ssl_earliest_cert_expiry GaugeVec");
            registry
                .register(Box::new(probe_http_ssl_earliest_cert_expiry.clone()))
                .expect("Failed to register probe_http_ssl_earliest_cert_expiry metric");

            ProbeMetrics {
                probe_tcp_success,
                probe_tcp_duration_seconds,
                probe_ping_success,
                probe_ping_duration_seconds,
                probe_http_success,
                probe_http_duration_seconds,
                probe_http_status_code,
                probe_http_ssl_earliest_cert_expiry,
            }
        };
        let version = GaugeVec::new(
            prometheus::Opts::new("ferris_watch_version", "Version of ferris-watch"),
            &["version"],
        )
        .expect("Failed to create ferris_watch_version GaugeVec");
        registry
            .register(Box::new(version.clone()))
            .expect("Failed to register ferris_watch_version metric");

        let os_version_info = IntGaugeVec::new(
            prometheus::Opts::new(
                "os_version_info",
                "Detailed information about the host operating system",
            ),
            &[
                "os_type",    // e.g., "Windows", "Ubuntu", "macOS"
                "os_release", // Kernel release or build number
                "os_version", // User-facing full version string
                "arch",       // e.g., "x86_64", "aarch64"
            ],
        )
        .expect("Failed to create os_version_info GaugeVec");
        registry
            .register(Box::new(os_version_info.clone()))
            .expect("Failed to register os_version_info metric");

        Metrics {
            process_metrics,
            system_metrics,
            probe_metrics,
            active_pids: Mutex::new(HashMap::new()),
            jstat_labels: Mutex::new(HashMap::new()),
            config,
            version,
            os_version_info,
        }
    }
}
#[derive(Clone)]
pub struct ProcessInfo {
    pub(crate) container: String, // "host" or container ID
    pub(crate) pid: String,
    pub(crate) process: String,
}
