use log::{info, error};
use tokio::net::TcpStream;
use tokio::time::{timeout, Duration};
use std::net::IpAddr;
use ping; // Import the crate directly
use crate::metrics::metrics::Metrics; // Import the Metrics struct
use std::sync::Arc; // Needed for Arc<Metrics>
use std::time::Instant; // Use std::time::Instant directly

pub async fn tcp_probe(metrics: Arc<Metrics>, host: String, port: u16) -> String {
    let timer = Instant::now();
    let address = format!("{}:{}", host, port);
    log::info!("Attempting TCP probe to {}", address);

    let result = timeout(Duration::from_secs(5), TcpStream::connect(&address)).await;

    let success = match result {
        Ok(Ok(_)) => {
            info!("TCP probe to {} successful.", address);
            1.0
        },
        Ok(Err(e)) => {
            error!("TCP probe to {} failed: {}", address, e);
            0.0
        },
        Err(_) => {
            error!("TCP probe to {} timed out.", address);
            0.0
        },
    };

    metrics
        .probe_metrics
        .probe_tcp_success
        .with_label_values(&[host.as_str(), port.to_string().as_str()])
        .set(success);
    metrics
        .probe_metrics
        .probe_tcp_duration_seconds
        .with_label_values(&[host.as_str(), port.to_string().as_str()])
        .set(timer.elapsed().as_secs_f64());

    // Since metrics are now global, we don't need to encode them here.
    // The /metrics endpoint will handle the collection.
    // Return an empty string or a simple success message.
    "OK".to_string()
}

pub async fn ping_probe(metrics: Arc<Metrics>, host: String) -> String {
    let timer = Instant::now();
    log::info!("Attempting Ping probe to {}", host);

    let success = match host.parse::<IpAddr>() {
        Ok(ip_addr) => {
            let result = timeout(Duration::from_secs(5), tokio::task::spawn_blocking(move || {
                ping::ping(ip_addr, None, None, None, None, None)
            })).await;

            match result {
                Ok(Ok(_)) => { // Match on Ok(Ok(_)) as PingResult might not be directly accessible
                    info!("Ping probe to {} successful.", host);
                    1.0
                },
                Ok(Err(e)) => {
                    error!("Ping probe to {} failed: {}", host, e);
                    0.0
                },
                Err(_) => {
                    error!("Ping probe to {} timed out.", host);
                    0.0
                }
            }
        },
        Err(e) => {
            error!("Invalid host address for ping probe: {}. Error: {}", host, e);
            0.0
        }
    };

    metrics
        .probe_metrics
        .probe_ping_success
        .with_label_values(&[host.as_str()])
        .set(success);
    metrics
        .probe_metrics
        .probe_ping_duration_seconds
        .with_label_values(&[host.as_str()])
        .set(timer.elapsed().as_secs_f64());

    // Since metrics are now global, we don't need to encode them here.
    // The /metrics endpoint will handle the collection.
    // Return an empty string or a simple success message.
    "OK".to_string()
}
