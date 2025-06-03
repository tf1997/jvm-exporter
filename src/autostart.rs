use std::env;
use std::fs;

use dirs;

#[cfg(target_os = "macos")]
use std::process::Command;

#[cfg(target_os = "macos")]
pub fn install_autostart() -> Result<(), Box<dyn std::error::Error>> {
    let current_exe = env::current_exe()?;
    let exe_path_str = current_exe.to_str().ok_or("Could not get executable path")?;

    let home_dir = dirs::home_dir().ok_or("Could not get user home directory")?;
    let launch_agents_dir = home_dir.join("Library/LaunchAgents");
    fs::create_dir_all(&launch_agents_dir)?;

    let plist_file_name = "com.ferris-watch.jvm-exporter.plist";
    let plist_path = launch_agents_dir.join(plist_file_name);

    let plist_content = format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.ferris-watch.jvm-exporter</string>
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <false/>
    <key>StandardOutPath</key>
    <string>/tmp/com.ferris-watch.jvm-exporter.stdout.log</string>
    <key>StandardErrorPath</key>
    <string>/tmp/com.ferris-watch.jvm-exporter.stderr.log</string>
</dict>
</plist>"#, exe_path_str);

    fs::write(&plist_path, plist_content)?;

    let output = Command::new("launchctl")
        .arg("load")
        .arg("-w")
        .arg(&plist_path)
        .output()?;

    if !output.status.success() {
        return Err(format!("Failed to load launch agent: {}", String::from_utf8_lossy(&output.stderr)).into());
    }

    Ok(())
}

#[cfg(target_os = "macos")]
pub fn uninstall_autostart() -> Result<(), Box<dyn std::error::Error>> {
    let home_dir = dirs::home_dir().ok_or("Could not get user home directory")?;
    let launch_agents_dir = home_dir.join("Library/LaunchAgents");
    let plist_file_name = "com.ferris-watch.jvm-exporter.plist";
    let plist_path = launch_agents_dir.join(plist_file_name);

    if plist_path.exists() {
        let output = Command::new("launchctl")
            .arg("unload")
            .arg("-w")
            .arg(&plist_path)
            .output()?;

        if !output.status.success() {
            return Err(format!("Failed to unload launch agent: {}", String::from_utf8_lossy(&output.stderr)).into());
        }

        fs::remove_file(&plist_path)?;
    } else {
        return Err("Autostart file does not exist, no need to uninstall.".into());
    }

    Ok(())
}

#[cfg(target_os = "windows")]
use winreg::{enums::*, RegKey};

#[cfg(target_os = "windows")]
pub fn install_autostart() -> Result<(), Box<dyn std::error::Error>> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let path = r"Software\Microsoft\Windows\CurrentVersion\Run";
    let (key, _) = hkcu.create_subkey(&path)?;

    let current_exe = env::current_exe()?;
    let exe_path_str = current_exe.to_str().ok_or("Could not get executable path")?;

    key.set_value("ferris-watch-jvm-exporter", &exe_path_str)?;
    Ok(())
}

#[cfg(target_os = "windows")]
pub fn uninstall_autostart() -> Result<(), Box<dyn std::error::Error>> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let path = r"Software\Microsoft\Windows\CurrentVersion\Run";
    let key = hkcu.open_subkey(&path)?;

    key.delete_value("ferris-watch-jvm-exporter")?;
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn install_autostart() -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("Autostart installation not supported on this OS.");
    Err("Autostart installation not supported on this OS.".into())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn uninstall_autostart() -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("Autostart uninstallation not supported on this OS.");
    Err("Autostart uninstallation not supported on this OS.".into())
}
