use std::sync::Arc;
use std::time::Duration;
use sysinfo::{Networks, System};
use tokio::time::interval;
use crate::metrics::collect::Metrics;

pub fn run(metrics: Arc<Metrics>) {
    let metrics = metrics.clone();
    tokio::spawn({
        let metrics = Arc::clone(&metrics);
        move || {
            let metrics = Arc::clone(&metrics);
            async move {
                let mut network_task_interval = interval(Duration::from_millis(500));
                loop {
                    let mut networks = Networks::new_with_refreshed_list();
                    // 等待下一次 tick
                    network_task_interval.tick().await;
                    networks.refresh();
                    for (interface_name, data) in &networks {
                        let received = data.received() as f64 * 10.0;
                        let transmitted = data.transmitted() as f64 * 10.0;
                        metrics
                            .system_metrics
                            .network_receive_bytes_per_sec
                            .with_label_values(&[interface_name])
                            .set(received);

                        metrics
                            .system_metrics
                            .network_transmit_bytes_per_sec
                            .with_label_values(&[interface_name])
                            .set(transmitted);
                    }
                };
            }
        }
    }());

    tokio::spawn({
        let metrics = Arc::clone(&metrics);
        move || {
            let metrics = Arc::clone(&metrics);
            async move {
                let mut cpu_task_interval = interval(Duration::from_millis(100));
                loop {
                    let mut system = System::new_all();
                    // 等待下一次 tick
                    cpu_task_interval.tick().await;
                    system.refresh_cpu_all();
                    // Update CPU usage
                    for (i, processor) in system.cpus().iter().enumerate() {
                        let cpu_label = format!("cpu_{}", i);
                        metrics
                            .system_metrics
                            .cpu_usage
                            .with_label_values(&[&cpu_label])
                            .set(processor.cpu_usage() as f64);
                    }
                };
            }
        }
    }());
}
