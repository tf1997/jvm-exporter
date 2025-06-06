use log::{info, warn};
use std::error::Error;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use dirs;
use tokio::fs;
use std::io::Read;
use crate::config::Config;

pub async fn check_and_update(config: Arc<RwLock<Config>>) -> Result<Option<String>, Box<dyn Error>> {
    let update_url_option = check_for_update(config.clone()).await?;

    if let Some(update_url) = update_url_option {
        info!("New version available at: {}", update_url);
        download_update(&update_url).await?;
        info!("Update downloaded successfully.");
        Ok(Some(update_url))
    } else {
        info!("No update available.");
        Ok(None)
    }
}
pub async fn check_for_update(config: Arc<RwLock<Config>>) -> Result<Option<String>, Box<dyn Error>> {
    info!("Checking for updates...");

    let update_service_url = config
        .read()
        .unwrap()
        .update_service_url
        .as_deref()
        .ok_or("update_service_url not found in config.yaml")?
        .to_string();
    let version_info_url = update_service_url.clone() + "/version.json";

    let current_version = env!("CARGO_PKG_VERSION"); // Get current version from Cargo.toml
    info!("Current version: {}", current_version);

    // Fetch latest version from URL
    let latest_version_str = fetch_latest_version(&version_info_url).await?;
    info!("Latest version available: {}", latest_version_str);

    let latest_version = semver::Version::parse(&latest_version_str)?;
    let current_version_parsed = semver::Version::parse(current_version)?;

    if latest_version > current_version_parsed {
        info!("New version available!");
        let platform_string = get_platform_string();
        let mut app_name = env!("CARGO_PKG_NAME").to_string();
        if cfg!(target_os = "windows") {
            app_name.push_str(".exe");
        }
        let download_file_url = format!("{}/release/{}/{}/{}", update_service_url, latest_version_str, platform_string, app_name);
        info!("Constructed download URL: {}", download_file_url);
        Ok(Some(download_file_url))
    } else {
        info!("No update available. You are running the latest version.");
        Ok(None)
    }
}

pub async fn download_update(download_url: &str) -> Result<PathBuf, Box<dyn Error>> {
    info!("Downloading update from: {}", download_url);
    let response = ureq::get(download_url).call()?;
    if response.status() != 200 {
        return Err(format!("Failed to download update: HTTP {}", response.status()).into());
    }
    let mut bytes = Vec::new();
    response.into_reader().read_to_end(&mut bytes)?;

    let app_data_dir = get_app_data_dir()?;
    let current_exe = std::env::current_exe()?;
    let app_name = env!("CARGO_PKG_NAME");
    let downloaded_file_path: PathBuf = {
        if cfg!(target_os = "windows") {
            app_data_dir.join(format!("{}.exe_new", app_name)) // Save with a temporary name
        } else {
            app_data_dir.join(format!("{}_new", app_name)) // Save with a temporary name
        }
    };
    

    info!("Saving new executable to: {:?}", downloaded_file_path);
    fs::write(&downloaded_file_path, bytes).await?;
    info!("New executable downloaded successfully.");

    info!("Please manually replace your current executable at {:?} with the new one at {:?} and restart the application.",
        current_exe, downloaded_file_path);
    Ok(downloaded_file_path)
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

// Helper function to get the platform string (e.g., "windows-x64", "macos-x64", "linux-x64")
fn get_platform_string() -> String {
    let os = if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        "unknown"
    };

    let arch = if cfg!(target_arch = "x86_64") {
        "x64"
    } else if cfg!(target_arch = "x86") {
        "x32"
    } else if cfg!(target_arch = "arm") {
        "arm"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    }
    else {
        "unknown"
    };

    format!("{}-{}", os, arch)
}
