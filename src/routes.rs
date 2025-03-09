use crate::config::{Config, with_config};
use crate::metrics;
use prometheus::Registry;
use std::sync::{Arc, RwLock};
use warp::Filter;

pub fn setup_routes(
    java_home: Arc<Option<String>>,
    full_path: bool,
    config: Arc<RwLock<Config>>,
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    let registry = Arc::new(Registry::new());
    let metrics = Arc::new(metrics::metrics::Metrics::new(&registry, config.clone()));
    metrics::timer::run(metrics.clone());

    let metrics_route = warp::path("metrics").and_then({
        let metrics = Arc::clone(&metrics);
        let registry = Arc::clone(&registry);
        let java_home = Arc::clone(&java_home);
        let full_path = full_path;

        move || {
            let metrics = Arc::clone(&metrics);
            let registry = Arc::clone(&registry);
            let java_home = java_home.clone();
            let full_path = full_path;

            async move {
                metrics::collect::handle_metrics(metrics, registry, java_home, full_path).await
            }
        }
    });

    let config_route = warp::path("config")
        .and(warp::get())
        .and(with_config(config.clone()))
        .map(|config: Arc<RwLock<Config>>| {
            let config = config.read().unwrap();
            let config_data = (*config).clone();
            warp::reply::json(&config_data)
        })
        .or(warp::path("config")
            .and(warp::post())
            .and(warp::body::json())
            .and(with_config(config.clone()))
            .map(|new_config: Config, config: Arc<RwLock<Config>>| {
                let mut config = config.write().unwrap();
                *config = new_config;
                warp::reply::json(&*config)
            }));
    

    // let deploy_route = warp::path("deploy")
    //     .and(warp::post())
    //     .and(warp::multipart::form().max_length(100_000_000_000))
    //     .and_then(deploy::deploy::handle_deploy);

    let routes = metrics_route.or(config_route);
    Ok::<_, warp::Rejection>(routes).expect("TODO: panic message")
}
