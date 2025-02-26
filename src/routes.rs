use warp::Filter;
use std::sync::Arc;
use prometheus::Registry;
use crate::{metrics, deploy};

pub fn setup_routes(
    java_home: Arc<Option<String>>,
    full_path: bool,
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    let registry = Arc::new(Registry::new());
    let metrics = Arc::new(metrics::metrics::Metrics::new(&registry));

    let metrics_route = warp::path("metrics")
        .and_then({
            let metrics = Arc::clone(&metrics);
            let registry = Arc::clone(&registry);
            let java_home = Arc::clone(&java_home);
            let full_path = full_path;

            move || {
                let metrics = Arc::clone(&metrics);
                let registry = Arc::clone(&registry);
                let java_home = java_home.clone();
                let full_path = full_path;

                async move { metrics::collect::handle_metrics(metrics, registry, java_home, full_path).await }
            }
        });

    // let deploy_route = warp::path("deploy")
    //     .and(warp::post())
    //     .and(warp::multipart::form().max_length(100_000_000_000))
    //     .and_then(deploy::deploy::handle_deploy);

    // metrics_route.or(deploy_route);
    Ok::<_, warp::Rejection>(metrics_route).expect("TODO: panic message")
}
