use log::error;
use tokio::net::TcpStream;
use tokio::time::{timeout, Duration};
use std::net::IpAddr;
use ping;
use crate::metrics::metrics::Metrics;
use std::sync::Arc;
use std::time::Instant;
use prometheus::{Registry, GaugeVec, Encoder, TextEncoder};

pub async fn tcp_probe(metrics: Arc<Metrics>, host: String, port: u16) -> String {
    let timer = Instant::now();
    let address = format!("{}:{}", host, port);

    let result = timeout(Duration::from_secs(5), TcpStream::connect(&address)).await;

    let success = match result {
        Ok(Ok(_)) => {
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

    // Update global metrics
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

    // Create a new registry for probe-specific metrics
    let registry = Registry::new();

    let probe_tcp_success_local = GaugeVec::new(
        prometheus::Opts::new("local_probe_tcp_success", "TCP probe success status"),
        &["host", "port"],
    )
    .expect("Failed to create probe_tcp_success GaugeVec for probe");
    registry.register(Box::new(probe_tcp_success_local.clone())).expect("Failed to register probe_tcp_success_local metric");

    let probe_tcp_duration_seconds_local = GaugeVec::new(
        prometheus::Opts::new("local_probe_tcp_duration_seconds", "Duration of TCP probe in seconds"),
        &["host", "port"],
    )
    .expect("Failed to create probe_tcp_duration_seconds GaugeVec for probe");
    registry.register(Box::new(probe_tcp_duration_seconds_local.clone())).expect("Failed to register probe_tcp_duration_seconds_local metric");

    probe_tcp_success_local
        .with_label_values(&[host.as_str(), port.to_string().as_str()])
        .set(success);
    probe_tcp_duration_seconds_local
        .with_label_values(&[host.as_str(), port.to_string().as_str()])
        .set(timer.elapsed().as_secs_f64());

    let encoder = TextEncoder::new();
    let metric_families = registry.gather();
    let mut buffer = Vec::new();
    encoder.encode(&metric_families, &mut buffer).expect("Failed to encode probe metrics");

    String::from_utf8(buffer).expect("Failed to convert probe metrics buffer to String")
}

pub async fn ping_probe(metrics: Arc<Metrics>, host: String) -> String {
    let timer = Instant::now();

    let success = match host.parse::<IpAddr>() {
        Ok(ip_addr) => {
            let result = timeout(Duration::from_secs(5), tokio::task::spawn_blocking(move || {
                ping::ping(ip_addr, None, None, None, None, None)
            })).await;

            match result {
                Ok(Ok(_)) => {
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

    // Create a new registry for probe-specific metrics
    let registry = Registry::new();

    let probe_ping_success_local = GaugeVec::new(
        prometheus::Opts::new("local_probe_ping_success", "Ping probe success status"),
        &["host"],
    )
    .expect("Failed to create probe_ping_success GaugeVec for probe");
    registry.register(Box::new(probe_ping_success_local.clone())).expect("Failed to register probe_ping_success_local metric");

    let probe_ping_duration_seconds_local = GaugeVec::new(
        prometheus::Opts::new("local_probe_ping_duration_seconds", "Duration of Ping probe in seconds"),
        &["host"],
    )
    .expect("Failed to create probe_ping_duration_seconds GaugeVec for probe");
    registry.register(Box::new(probe_ping_duration_seconds_local.clone())).expect("Failed to register probe_ping_duration_seconds_local metric");

    probe_ping_success_local
        .with_label_values(&[host.as_str()])
        .set(success);
    probe_ping_duration_seconds_local
        .with_label_values(&[host.as_str()])
        .set(timer.elapsed().as_secs_f64());

    let encoder = TextEncoder::new();
    let metric_families = registry.gather();
    let mut buffer = Vec::new();
    encoder.encode(&metric_families, &mut buffer).expect("Failed to encode probe metrics");

    String::from_utf8(buffer).expect("Failed to convert probe metrics buffer to String")
}