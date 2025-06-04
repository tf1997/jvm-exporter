use log::{info};
use std::error::Error;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use dirs;
use tokio::fs;
use std::io::Read;
use crate::config::Config;

pub async fn check_and_update(config: Arc<RwLock<Config>>) -> Result<(), Box<dyn Error>> {
    info!("Checking for updates...");

    let update_service_url = config
        .read()
        .unwrap()
        .update_service_url
        .as_deref()
        .ok_or("update_service_url not found in config.yaml")?
        .to_string();
    let version_info_url = update_service_url.clone() + "/version.json";
    let download_file_url = update_service_url.clone() + "/release/latest";

    let current_version = env!("CARGO_PKG_VERSION"); // Get current version from Cargo.toml
    info!("Current version: {}", current_version);

    // Fetch latest version from URL
    let latest_version_str = fetch_latest_version(&version_info_url).await?;
    info!("Latest version available: {}", latest_version_str);

    let latest_version = semver::Version::parse(&latest_version_str)?;
    let current_version_parsed = semver::Version::parse(current_version)?;

    if latest_version > current_version_parsed {
        info!("New version available! Downloading update...");
        // Manually download the file
        let response = ureq::get(&download_file_url).call()?;
        if response.status() != 200 {
            return Err(format!("Failed to download update: HTTP {}", response.status()).into());
        }
        let mut bytes = Vec::new();
        response.into_reader().read_to_end(&mut bytes)?;

        let app_data_dir = get_app_data_dir()?;
        let current_exe = std::env::current_exe()?;
        let app_name = current_exe.file_name().unwrap().to_str().unwrap();
        let downloaded_file_path: PathBuf = app_data_dir.join(format!("{}_new", app_name)); // Save with a temporary name

        info!("Saving new executable to: {:?}", downloaded_file_path);
        fs::write(&downloaded_file_path, bytes).await?;
        info!("New executable downloaded successfully.");

        info!("Please manually replace your current executable at {:?} with the new one at {:?} and restart the application.",
            current_exe, downloaded_file_path);
    } else {
        info!("No update available. You are running the latest version.");
    }

    Ok(())
}

async fn fetch_latest_version(url: &str) -> Result<String, Box<dyn Error>> {
    let response = ureq::get(url).call()?;
    if response.status() == 200 {
        let json_response: serde_json::Value = serde_json::from_str(&response.into_string()?)?;
        // Assuming the version is in a "version" field in the JSON
        let version = json_response["version"]
            .as_str()
            .ok_or("Version not found or not a string in JSON response")?
            .to_string();
        Ok(version)
    } else {
        Err(format!("Failed to fetch version info: HTTP {}", response.status()).into())
    }
}

// Function to get the application's data directory
pub fn get_app_data_dir() -> Result<PathBuf, Box<dyn Error>> {
    let data_dir = dirs::data_dir()
        .ok_or("Could not find data directory")?
        .join(env!("CARGO_PKG_NAME")); // Use package name for app-specific directory
    
    // Ensure the directory exists
    std::fs::create_dir_all(&data_dir)?;
    Ok(data_dir)
}
