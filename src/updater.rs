use log::{info};
use std::error::Error;
use std::path::PathBuf;
use dirs;
use tokio::fs;
use std::io::Read;
use crate::config::Config;

pub async fn check_and_update(config: Config) -> Result<(), Box<dyn Error>> {
    info!("Checking for updates...");

    let update_service_url = config.update_service_url.ok_or("update_service_url not found in config.yaml")?;
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
        let downloaded_file_path = app_data_dir.join(format!("{}_new", app_name)); // Save with a temporary name

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

pub async fn setup_autostart() -> Result<(), Box<dyn Error>> {
    info!("Setting up auto-start...");
    let current_exe = std::env::current_exe()?;
    let app_name = current_exe.file_stem().unwrap().to_str().unwrap();

    #[cfg(target_os = "windows")]
    {
        use winreg::enums::*;
        use winreg::RegKey;

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let path = r"Software\Microsoft\Windows\CurrentVersion\Run";
        let (key, _) = hkcu.create_subkey(&path)?;
        key.set_value(app_name, &current_exe.to_str().unwrap())?;
        info!("Auto-start set for Windows.");
    }

    #[cfg(target_os = "macos")]
    {
        let plist_path = dirs::home_dir()
            .ok_or("Could not find home directory")?
            .join("Library/LaunchAgents")
            .join(format!("{}.plist", app_name));

        let plist_content = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <false/>
</dict>
</plist>"#,
            app_name,
            current_exe.to_str().unwrap()
        );

        fs::write(&plist_path, plist_content).await?;
        info!("Auto-start set for macOS: {:?}", plist_path);
    }

    #[cfg(target_os = "linux")]
    {
        // For Linux, a common approach is to use a .desktop file for desktop environments
        // or systemd for system-wide services. This example uses .desktop for user-level autostart.
        let autostart_dir = dirs::config_dir()
            .ok_or("Could not find config directory")?
            .join("autostart");
        fs::create_dir_all(&autostart_dir).await?;

        let desktop_file_path = autostart_dir.join(format!("{}.desktop", app_name));
        let desktop_content = format!(
            r#"[Desktop Entry]
Type=Application
Exec={}
Hidden=false
NoDisplay=false
X-GNOME-Autostart-enabled=true
Name={}
Comment=Start {} on login
"#,
            current_exe.to_str().unwrap(),
            app_name,
            app_name
        );

        fs::write(&desktop_file_path, desktop_content).await?;
        info!("Auto-start set for Linux: {:?}", desktop_file_path);
    }

    Ok(())
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
