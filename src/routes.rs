use crate::config::{with_config, Config};
use crate::metrics;
use crate::probes;
use prometheus::Registry;
use sysinfo::{System};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use warp::http::StatusCode;
use warp::Filter;
use warp::Reply; // Make sure warp::Rejection is in scope if not already

pub fn setup_routes(
    java_home: Arc<Option<String>>,
    full_path: bool,
    config: Arc<RwLock<Config>>,
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    let registry = Arc::new(Registry::new());
    let metrics_instance = Arc::new(metrics::metrics::Metrics::new(&registry, config.clone()));
    metrics::timer::run(metrics_instance.clone());
    metrics_instance
        .version
        .with_label_values(&[env!("CARGO_PKG_VERSION")])
        .set(env!("CARGO_PKG_VERSION").replace(".", "").parse().unwrap_or(0.0));

    let os_type = System::name().unwrap_or_else(|| "unknown".to_string());
    let os_release = System::kernel_version().unwrap_or_else(|| "unknown".to_string());
    let os_version = System::long_os_version().unwrap_or_else(|| "unknown".to_string());
    let arch = std::env::consts::ARCH.to_string();
    metrics_instance.os_version_info
        .with_label_values(&[
            &os_type,
            &os_release,
            &os_version,
            &arch
        ]).set(1);

    let metrics_route = warp::path("metrics").and_then({
        let metrics_handler_metrics = Arc::clone(&metrics_instance);
        let metrics_handler_registry = Arc::clone(&registry);
        let metrics_handler_java_home = Arc::clone(&java_home);

        move || {
            let metrics = Arc::clone(&metrics_handler_metrics);
            let registry = Arc::clone(&metrics_handler_registry);
            let java_home_clone = metrics_handler_java_home.clone(); // Renamed to avoid conflict if java_home is used directly
            let current_full_path = full_path; // Renamed for clarity if full_path is used directly

            async move {
                // Assuming handle_metrics returns Result<impl Reply, warp::Rejection>
                metrics::collect::handle_metrics(
                    metrics,
                    registry,
                    java_home_clone,
                    current_full_path,
                )
                .await
            }
        }
    });

    let config_route = warp::path("config")
        .and(warp::get())
        .and(with_config(config.clone()))
        .map(|config_arc: Arc<RwLock<Config>>| {
            let config_guard = config_arc.read().unwrap();
            let config_data = (*config_guard).clone();
            warp::reply::json(&config_data)
        })
        .or(warp::path("config")
            .and(warp::post())
            .and(warp::body::json())
            .and(with_config(config.clone()))
            .map(|new_config: Config, config_arc: Arc<RwLock<Config>>| {
                let mut config_guard = config_arc.write().unwrap();
                *config_guard = new_config;
                warp::reply::json(&*config_guard)
            }));

    let probe_route = warp::path("probe")
        .and(warp::query::<HashMap<String, String>>())
        .and_then(move |params: HashMap<String, String>| {
            let metrics_handler_metrics = Arc::clone(&metrics_instance);
            async move {
                let module = params.get("module").cloned();
                let target = params.get("target").cloned();

                if let (Some(module_str), Some(target_str)) = (module, target) {
                    match module_str.as_str() {
                        "tcp" => {
                            let parts: Vec<&str> = target_str.split(':').collect();
                            if parts.len() == 2 {
                                let host_str = parts[0].to_string(); // Use a distinct variable name
                                if let Ok(port_val) = parts[1].parse::<u16>() {
                                    // Use a distinct variable name
                                    let reply = warp::reply::with_status(
                                        probes::tcp_probe(
                                            metrics_handler_metrics,
                                            host_str,
                                            port_val,
                                        )
                                        .await,
                                        StatusCode::OK,
                                    );
                                    return Ok::<(Box<dyn Reply>,), warp::Rejection>((
                                        Box::new(reply) as Box<dyn Reply>,
                                    ));
                                } else {
                                    let reply = warp::reply::with_status(
                                        "Invalid port in target for TCP probe.".to_string(),
                                        StatusCode::BAD_REQUEST,
                                    );
                                    return Ok::<(Box<dyn Reply>,), warp::Rejection>((
                                        Box::new(reply) as Box<dyn Reply>,
                                    ));
                                }
                            }
                            let reply = warp::reply::with_status(
                                "Invalid target format for TCP probe. Expected host:port"
                                    .to_string(),
                                StatusCode::BAD_REQUEST,
                            );
                            Ok::<(Box<dyn Reply>,), warp::Rejection>((
                                Box::new(reply) as Box<dyn Reply>,
                            ))
                        }
                        "ping" => {
                            let reply = warp::reply::with_status(
                                probes::ping_probe(metrics_handler_metrics, target_str).await,
                                StatusCode::OK,
                            );
                            Ok::<(Box<dyn Reply>,), warp::Rejection>((
                                Box::new(reply) as Box<dyn Reply>,
                            ))
                        }
                        _ => {
                            let reply = warp::reply::with_status(
                                "Unknown module. Supported modules: tcp, ping".to_string(),
                                StatusCode::BAD_REQUEST,
                            );
                            Ok::<(Box<dyn Reply>,), warp::Rejection>((
                                Box::new(reply) as Box<dyn Reply>,
                            ))
                        }
                    }
                } else {
                    let reply = warp::reply::with_status(
                        "Missing module or target parameter".to_string(),
                        StatusCode::BAD_REQUEST,
                    );
                    Ok::<(Box<dyn Reply>,), warp::Rejection>((Box::new(reply) as Box<dyn Reply>,))
                }
            }
        });

    let routes = metrics_route.or(config_route).or(probe_route); // probe_route is added here

    routes
}
