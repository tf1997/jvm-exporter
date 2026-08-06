use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::sync::{Arc, RwLock};
use warp::Filter;
use ureq;

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Config {
    pub log_level: Option<String>,
    pub java_home: Option<String>,
    pub configuration_service_url: Option<String>,
    pub system_processes: Option<Vec<String>>,
    pub detect_docker_processes: Option<bool>,
    pub detect_java_processes: Option<bool>,
    pub update_service_url: Option<String>,
}

impl Config {
    pub fn new(file_path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let config_content = fs::read_to_string(file_path)?;
        let mut config: Config = serde_yaml::from_str(&config_content)?;
        if config.detect_docker_processes.is_none() {
            config.detect_docker_processes = Some(false);
        }
        if config.detect_java_processes.is_none() {
            config.detect_java_processes = Some(true);
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
    let response = ureq::get(url)
        .timeout(std::time::Duration::from_secs(30))
        .call()?;
    let content_type = response.header("Content-Type").unwrap_or("");
    let yaml_string = if content_type.contains("yaml") || content_type.contains("text") {
        response.into_string()?
    } else {
        let mut reader = response.into_reader();
        let mut buf = String::new();
        use std::io::Read;
        reader.read_to_string(&mut buf)?;
        buf
    };
    let remote_config: Config = serde_yaml::from_str(&yaml_string)?;
    if remote_config.log_level.is_some() {
        config.log_level = remote_config.log_level;
    }
    if remote_config.java_home.is_some() {
        config.java_home = remote_config.java_home;
    }
    if remote_config.detect_docker_processes.is_some() {
        config.detect_docker_processes = remote_config.detect_docker_processes;
    }
    if remote_config.detect_java_processes.is_some() {
        config.detect_java_processes = remote_config.detect_java_processes;
    }
    if remote_config.update_service_url.is_some() {
        config.update_service_url = remote_config.update_service_url;
    }
    if let Some(remote_processes) = remote_config.system_processes {
        let mut local_processes: HashSet<String> = config.system_processes.clone().unwrap_or_default().into_iter().collect();
        local_processes.extend(remote_processes.into_iter());
        config.system_processes = Some(local_processes.into_iter().collect());
    }
    Ok(())
}
