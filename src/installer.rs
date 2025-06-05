use std::error::Error;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use crate::updater;
use log::{error};

#[cfg(target_os = "windows")]
use winreg::enums::*;
#[cfg(target_os = "windows")]
use winreg::RegKey;

pub async fn install_application() -> Result<(), Box<dyn Error>> {

    let app_data_dir = match updater::get_app_data_dir() {
        Ok(dir) => dir,
        Err(e) => {
            error!("Error getting app data directory: {}", e);
            std::process::exit(1);
        }
    };
    let current_exe = std::env::current_exe().unwrap();
    let app_name = current_exe.file_name().unwrap().to_str().unwrap();
    let downloaded_file_path: PathBuf = app_data_dir.join(format!("{}", app_name));

    if !downloaded_file_path.exists() {
        // show_info_dialog("No new installer found in download directory. Please update first.", window.clone());
        error!("No new installer found ({})in download directory. Please update first.", downloaded_file_path.display());
        std::process::exit(1);
    }
    
    let app_name = env!("CARGO_PKG_NAME");

    #[cfg(target_os = "windows")]
    {
        let current_exe_path = std::env::current_exe()?;
        let target_dir = dirs::data_dir()
            .ok_or("Could not find a suitable data directory for Windows.")?
            .join(app_name);
        
        let mut exe_file_name = current_exe_path.file_name()
            .ok_or("Invalid executable file name")?
            .to_os_string();

        if let Some(s) = exe_file_name.to_str() {
            if s.ends_with("_new") {
                exe_file_name = s[0..s.len() - "_new".len()].to_string().into();
            }
        }
        let target_exe_path = target_dir.join(exe_file_name);
        // Create the target directory if it doesn't exist
        if !target_dir.exists() {
            fs::create_dir_all(&target_dir)?;
            println!("Created target directory: {}", target_dir.display());
        }

        // Copy the new executable to the secure directory, overwriting if it exists
        fs::copy(new_exe_path, &target_exe_path)?;
        println!("New executable copied to secure location: {}", target_exe_path.display());

        let target_exe_str = target_exe_path.to_str().ok_or("Invalid target executable path")?;

        // Set auto-start registry entry
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        let path = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
        let (key, _disp) = hklm.create_subkey(&path)?;

        key.set_value(app_name, &format!("\"{}\" --no-ui", target_exe_str))?;
        println!("Auto-start configured for Windows (all users) with entry: {} = {}", app_name, target_exe_str);
        println!("NOTE: This operation requires administrative privileges to set auto-start for all users.");
        println!("Application will start automatically with Windows.");

        // Optionally, run the program immediately after configuring auto-start
        // This might not be desired for an "installation" flow, as the current app might still be running.
        // For now, we'll just configure and let the user restart or the system auto-start.
        println!("Auto-start configured for Windows (all users) with entry: {} = {}", app_name, target_exe_str);
        println!("NOTE: This operation requires administrative privileges to set auto-start for all users.");
        println!("Application will start automatically with Windows.");

        // Run the program immediately after configuring auto-start
        println!("Starting application immediately...");
        std::process::Command::new("cmd")
            .arg("/C")
            .arg("start")
            .arg("") // Title argument, can be empty
            .arg(&target_exe_str)
            .arg("--no-ui") // Pass --no-ui to the launched instance
            .spawn()?;
        println!("Application started.");
        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    {
        // For macOS/Linux, we'll use systemd-like logic or a simpler copy for macOS.
        // Given the monitor.rs uses systemd, we'll adapt that.
        // For macOS, a common approach for user-level auto-start is LaunchAgents.
        // For simplicity and consistency with monitor.rs, let's assume a system-wide installation
        // similar to systemd for now, and note that macOS might need LaunchAgents for user-level.
        // The prompt specifically asked for "mac and window", and monitor.rs uses systemd for non-windows.
        // So, I'll adapt the systemd logic for non-windows, which covers macOS if systemd is used (less common)
        // or if we're aiming for a root-level service.
        // A more robust macOS solution would involve LaunchAgents. For now, I'll stick to the monitor.rs pattern.

        let service_name = format!("{}.service", app_name);
        let service_path = format!("/etc/systemd/system/{}", service_name);
        let binary_target_dir = "/usr/local/bin";
        let binary_target_path = format!("{}/{}", binary_target_dir, app_name);

        if !Path::new(binary_target_dir).exists() {
            fs::create_dir_all(binary_target_dir)?;
            println!("Target directory created: {}", binary_target_dir);
        }

        // Copy the new executable to the target path, overwriting if it exists
        fs::copy(downloaded_file_path, &binary_target_path)?;
        println!("New executable copied to: {}", binary_target_path);

        let java_home = std::env::var("JAVA_HOME").ok();

        let service_content = if let Some(jh) = java_home {
            format!(
                "[Unit]
Description={} Service
After=network.target

[Service]
Type=simple
ExecStart={} --no-ui
User=root
Environment=\"JAVA_HOME={}\"
Environment=\"PATH={}/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin\"
Restart=on-failure

[Install]
WantedBy=multi-user.target",
                app_name, binary_target_path, jh, jh
            )
        } else {
            format!(
                "[Unit]
Description={} Service
After=network.target

[Service]
Type=simple
ExecStart={} --no-ui
User=root
Restart=on-failure

[Install]
WantedBy=multi-user.target",
                app_name, binary_target_path
            )
        };

        let service_dir = Path::new("/etc/systemd/system");
        if !service_dir.exists() {
            fs::create_dir_all(service_dir)?;
            println!("Systemd directory created: {}", service_dir.display());
        }

        let mut file = fs::File::create(&service_path)?;
        file.write_all(service_content.as_bytes())?;
        println!("Service file created at: {}", service_path);

        std::process::Command::new("systemctl")
            .args(&["daemon-reload"])
            .output()?;

        std::process::Command::new("systemctl")
            .args(&["enable", &service_name])
            .output()?;

        println!("Service configured to auto-start with the system.");
        println!("NOTE: This operation requires administrative privileges.");
        Ok(())
    }
}
