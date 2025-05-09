use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::sync::{Arc, RwLock};
use warp::Filter;
use ureq;

#[derive(Deserialize, Serialize, Clone)]
pub struct Config {
    pub log_level: Option<String>,
    pub java_home: Option<String>,
    pub configuration_service_url: Option<String>,
    pub system_processes: Option<Vec<String>>,
    pub detect_docker_processes: Option<bool>,
}

impl Config {
    pub fn new(file_path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let config_content = fs::read_to_string(file_path)?;
        let mut config: Config = serde_yaml::from_str(&config_content)?;
        if config.detect_docker_processes.is_none() {
            config.detect_docker_processes = Some(false);
        }
        Ok(config)
    }
}

pub fn with_config(
    config: Arc<RwLock<Config>>,
) -> impl Filter<Extract = (Arc<RwLock<Config>>,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || config.clone())
}

pub async fn fetch_and_merge_config(url: &str, config: &mut Config) -> Result<(), Box<dyn std::error::Error>> {
    let response = ureq::get(url).call()?.into_string()?;
    let remote_config: Config = serde_yaml::from_str(&response)?;
    if remote_config.log_level.is_some() {
        config.log_level = remote_config.log_level;
    }
    if remote_config.java_home.is_some() {
        config.java_home = remote_config.java_home;
    }
    if let Some(remote_processes) = remote_config.system_processes {
        let mut local_processes: HashSet<String> = config.system_processes.clone().unwrap_or_default().into_iter().collect();
        local_processes.extend(remote_processes.into_iter());
        config.system_processes = Some(local_processes.into_iter().collect());
    }
    Ok(())
}