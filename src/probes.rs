use log::error;
use tokio::net::TcpStream;
use tokio::time::{timeout, Duration};
use std::net::IpAddr;
use ping;
use crate::metrics::metrics::Metrics;
use std::sync::Arc;
use std::time::Instant;
use prometheus::{Registry, GaugeVec, Encoder, TextEncoder, CounterVec};
use reqwest;
use url::Url;
use crate::collectors::ssl::get_cert_expiry_seconds;

async fn run_diagnostic_probes(metrics: &Arc<Metrics>, local_failures_counter: &CounterVec, target: &str, parsed_url: &Url) {
    let domain = match parsed_url.host_str() {
        Some(h) => h,
        None => {
            metrics.probe_metrics.probe_http_phase_failures_total.with_label_values(&[target, "dns"]).inc();
            local_failures_counter.with_label_values(&[target, "dns"]).inc();
            return;
        }
    };
    let port = parsed_url.port_or_known_default().unwrap_or(80);

    // 1. DNS Probe
    match tokio::net::lookup_host((domain, port)).await {
        Ok(mut addrs) => {
            if addrs.next().is_none() {
                error!("DNS probe for {} failed: No addresses found", domain);
                metrics.probe_metrics.probe_http_phase_failures_total.with_label_values(&[target, "dns"]).inc();
                local_failures_counter.with_label_values(&[target, "dns"]).inc();
                return;
            }
        }
        Err(e) => {
            error!("DNS probe for {} failed: {}", domain, e);
            metrics.probe_metrics.probe_http_phase_failures_total.with_label_values(&[target, "dns"]).inc();
            local_failures_counter.with_label_values(&[target, "dns"]).inc();
            return;
        }
    }

    // 2. TCP Probe
    let address = format!("{}:{}", domain, port);
    if let Err(e) = timeout(Duration::from_secs(2), TcpStream::connect(&address)).await {
        error!("TCP connect probe to {} failed: {:?}", address, e);
        metrics.probe_metrics.probe_http_phase_failures_total.with_label_values(&[target, "tcp"]).inc();
        local_failures_counter.with_label_values(&[target, "tcp"]).inc();
        return;
    }

    // 3. TLS Probe (if applicable)
    if parsed_url.scheme() == "https" {
        let domain_clone = domain.to_string();
        let probe_result = tokio::task::spawn_blocking(move || {
            get_cert_expiry_seconds(&domain_clone)
        }).await;

        match probe_result {
            Ok(Ok(_)) => (), // Success
            Ok(Err(e)) => {
                error!("TLS handshake probe to {} failed: {}", domain, e);
                metrics.probe_metrics.probe_http_phase_failures_total.with_label_values(&[target, "tls"]).inc();
                local_failures_counter.with_label_values(&[target, "tls"]).inc();
                return;
            },
            Err(e) => {
                error!("TLS handshake probe task failed for {}: {}", domain, e);
                metrics.probe_metrics.probe_http_phase_failures_total.with_label_values(&[target, "tls"]).inc();
                local_failures_counter.with_label_values(&[target, "tls"]).inc();
                return;
            }
        }
    }

    // 4. If all above passed, it's an HTTP-level failure
    metrics.probe_metrics.probe_http_phase_failures_total.with_label_values(&[target, "http"]).inc();
    local_failures_counter.with_label_values(&[target, "http"]).inc();
}

pub async fn http_probe(metrics: Arc<Metrics>, target: String) -> String {
    let timer = Instant::now();
    let parsed_url = match Url::parse(&target) {
        Ok(url) => url,
        Err(e) => {
            error!("Invalid URL for http probe: {}. Error: {:?}", target, e);
            metrics.probe_metrics.probe_http_phase_failures_total.with_label_values(&[&target, "http"]).inc();
            return format!("# Invalid URL: {}", target);
        }
    };

    let client = reqwest::Client::new();
    let result = client.get(parsed_url.clone())
        .timeout(std::time::Duration::from_secs(5))
        .header("user-agent", "ferris-watch-mozilla/5.0 (compatible; FerrisWatch/1.0; +")
        .send().await;

    let (success, status_code) = match result {
        Ok(response) => {
            let status = response.status();
            if status.is_success() {
                (1.0, status.as_u16() as f64)
            } else {
                (0.0, status.as_u16() as f64)
            }
        },
        Err(e) => {
            error!("HTTP probe to {} failed: {}", target, e);
            (0.0, 0.0)
        },
    };

    // Create a new registry for probe-specific metrics
    let registry = Registry::new();

    let probe_http_phase_failures_total_local = CounterVec::new(
        prometheus::Opts::new("local_probe_http_phase_failures_total", "Total number of http probe failures by phase"),
        &["target", "phase"],
    )
    .expect("Failed to create local_probe_http_phase_failures_total CounterVec");
    registry.register(Box::new(probe_http_phase_failures_total_local.clone())).expect("Failed to register local_probe_http_phase_failures_total metric");

    if success == 0.0 {
        run_diagnostic_probes(&metrics, &probe_http_phase_failures_total_local, &target, &parsed_url).await;
    }

    let expiry_seconds = if parsed_url.scheme() == "https" {
        let domain = parsed_url.host_str().unwrap_or_default().to_string();
        let domain_clone = domain.clone();
        let cert_info_result = tokio::task::spawn_blocking(move || {
            get_cert_expiry_seconds(&domain_clone)
        }).await;

        match cert_info_result {
            Ok(Ok(seconds)) => seconds,
            Ok(Err(e)) => {
                error!("Failed to get SSL certificate expiration for {}: {}", &domain, e);
                0.0
            },
            Err(e) => {
                error!("Failed to get SSL certificate expiration task for {}: {}", &domain, e);
                0.0
            }
        }
    } else {
        -1.0 // Not a https url
    };

    let duration = timer.elapsed().as_secs_f64();

    // Update global metrics
    metrics
        .probe_metrics
        .probe_http_success
        .with_label_values(&[&target])
        .set(success);
    metrics
        .probe_metrics
        .probe_http_duration_seconds
        .with_label_values(&[&target])
        .set(duration);
    metrics
        .probe_metrics
        .probe_http_status_code
        .with_label_values(&[&target])
        .set(status_code);
    metrics
        .probe_metrics
        .probe_http_ssl_earliest_cert_expiry
        .with_label_values(&[&target])
        .set(expiry_seconds);

    let probe_http_success_local = GaugeVec::new(
        prometheus::Opts::new("local_probe_http_success", "HTTP probe success status"),
        &["target"],
    )
    .expect("Failed to create probe_http_success GaugeVec for probe");
    registry.register(Box::new(probe_http_success_local.clone())).expect("Failed to register probe_http_success_local metric");

    let probe_http_duration_seconds_local = GaugeVec::new(
        prometheus::Opts::new("local_probe_http_duration_seconds", "Duration of HTTP probe in seconds"),
        &["target"],
    )
    .expect("Failed to create probe_http_duration_seconds GaugeVec for probe");
    registry.register(Box::new(probe_http_duration_seconds_local.clone())).expect("Failed to register probe_http_duration_seconds_local metric");

    let probe_http_status_code_local = GaugeVec::new(
        prometheus::Opts::new("local_probe_http_status_code", "HTTP probe status code"),
        &["target"],
    )
    .expect("Failed to create probe_http_status_code GaugeVec for probe");
    registry.register(Box::new(probe_http_status_code_local.clone())).expect("Failed to register probe_http_status_code_local metric");

    let probe_http_ssl_earliest_cert_expiry_local = GaugeVec::new(
        prometheus::Opts::new("local_probe_http_ssl_earliest_cert_expiry", "Earliest SSL certificate expiry in seconds"),
        &["target"],
    )
    .expect("Failed to create probe_http_ssl_earliest_cert_expiry_local GaugeVec");
    registry.register(Box::new(probe_http_ssl_earliest_cert_expiry_local.clone())).expect("Failed to register probe_ssl_earliest_cert_expiry_local metric");

    probe_http_success_local
        .with_label_values(&[&target])
        .set(success);
    probe_http_duration_seconds_local
        .with_label_values(&[&target])
        .set(duration);
    probe_http_status_code_local
        .with_label_values(&[&target])
        .set(status_code);
    probe_http_ssl_earliest_cert_expiry_local
        .with_label_values(&[&target])
        .set(expiry_seconds);

    let encoder = TextEncoder::new();
    let metric_families = registry.gather();
    let mut buffer = Vec::new();
    encoder.encode(&metric_families, &mut buffer).expect("Failed to encode probe metrics");

    String::from_utf8(buffer).expect("Failed to convert probe metrics buffer to String")
}

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

    let ip_addr_result:Result<IpAddr, Box<dyn std::error::Error + Send + Sync>> = match host.parse::<IpAddr>() {
        Ok(ip) => Ok(ip),
        Err(_) => {
            match tokio::net::lookup_host((host.as_str(), 0)).await {
                Ok(mut addrs) => addrs.next().map(|sockaddr| sockaddr.ip()).ok_or("No IP found".into()),
                Err(e) => Err(Box::new(e) as Box<dyn std::error::Error + Send + Sync>),
            }
        }
    };

    let success = match ip_addr_result {
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
            error!("Invalid host address for ping probe: {}. Error: {:?}", host, e);
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
