use crate::metrics::collect::Metrics;
use std::sync::Arc;
use std::time::Duration;
use sysinfo::{Networks, System};
use tokio::time::interval;

pub fn run(metrics: Arc<Metrics>) {
    let metrics = metrics.clone();
    tokio::spawn({
        let metrics = Arc::clone(&metrics);
        move || {
            let metrics = Arc::clone(&metrics);
            async move {
                let mut network_task_interval = interval(Duration::from_millis(3000));
                let mut cpu_task_interval = interval(Duration::from_millis(10000));

                loop {
                    tokio::select! {
                        _ = network_task_interval.tick() => {
                            let mut networks = Networks::new_with_refreshed_list();
                            tokio::time::sleep(Duration::from_millis(1000)).await;
                            networks.refresh();
                            for (interface_name, data) in &networks {
                                let received = data.received() as f64;
                                let transmitted = data.transmitted() as f64;
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
                        },
                        _ = cpu_task_interval.tick() => {
                            let mut system = System::new_all();
                            tokio::time::sleep(Duration::from_millis(100)).await;
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
                        }
                    }
                }
            }
        }
    }());
}
