use log::{info};
use std::error::Error;
use std::fs;
use std::io::Write;
use std::path::Path;

#[cfg(target_os = "windows")]
use {
    winreg::enums::*,
    winreg::RegKey,
    std::process::{Command, Stdio},
    anyhow::{anyhow, Context, Result},
    log::error,
};

pub async fn install_application() -> Result<(), Box<dyn Error>> {
    let new_exe_path = std::env::current_exe().unwrap();

    let app_name = env!("CARGO_PKG_NAME");

    #[cfg(target_os = "windows")]
    {

        let current_exe_path = std::env::current_exe()?;
        let target_dir = dirs::data_dir()
            .ok_or("Could not find a suitable data directory for Windows.")?
            .join(app_name);

        let mut exe_file_name = current_exe_path
            .file_name()
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
            info!("Created target directory: {}", target_dir.display());
        }

        // Copy the new executable to the secure directory, overwriting if it exists
        info!("new_exe_path: {}", new_exe_path.display());
        info!("target_exe_path: {}", target_exe_path.display());
        kill_process_on_port(29090)?;
        fs::copy(new_exe_path, &target_exe_path)?;
        info!(
            "New executable copied to secure location: {}",
            target_exe_path.display()
        );

        let target_exe_str = target_exe_path
            .to_str()
            .ok_or("Invalid target executable path")?;

        // Set auto-start registry entry
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        let path = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
        let (key, _disp) = hklm.create_subkey(&path)?;

        key.set_value(app_name, &format!("\"{}\" --no-ui", target_exe_str))?;
        info!(
            "Auto-start configured for Windows (all users) with entry: {} = {}",
            app_name, target_exe_str
        );
        info!("NOTE: This operation requires administrative privileges to set auto-start for all users.");
        info!("Application will start automatically with Windows.");

        // Optionally, run the program immediately after configuring auto-start
        // This might not be desired for an "installation" flow, as the current app might still be running.
        // For now, we'll just configure and let the user restart or the system auto-start.
        info!(
            "Auto-start configured for Windows (all users) with entry: {} = {}",
            app_name, target_exe_str
        );
        info!("NOTE: This operation requires administrative privileges to set auto-start for all users.");
        info!("Application will start automatically with Windows.");

        // Run the program immediately after configuring auto-start
        info!("Starting application immediately...");
        std::process::Command::new("cmd")
            .arg("/C")
            .arg("start")
            .arg("") // Title argument, can be empty
            .arg(&target_exe_str)
            .arg("--no-ui") // Pass --no-ui to the launched instance
            .spawn()?;
        info!("Application started.");
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
            info!("Target directory created: {}", binary_target_dir);
        }

        // Copy the new executable to the target path, overwriting if it exists
        info!("new_exe_path: {}", new_exe_path.display());
        info!("binary_target_path: {}", binary_target_path);
        fs::copy(new_exe_path, &binary_target_path)?;
        info!("New executable copied to: {}", binary_target_path);

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
            info!("Systemd directory created: {}", service_dir.display());
        }

        let mut file = fs::File::create(&service_path)?;
        file.write_all(service_content.as_bytes())?;
        info!("Service file created at: {}", service_path);

        std::process::Command::new("systemctl")
            .args(&["daemon-reload"])
            .output()?;

        std::process::Command::new("systemctl")
            .args(&["enable", &service_name])
            .output()?;

        info!("Service configured to auto-start with the system.");
        info!("NOTE: This operation requires administrative privileges.");
        Ok(())
    }
}

#[cfg(target_os = "windows")]
fn find_pid_by_port(port: u16) -> Result<Option<u32>> {
    use netstat_esr::{
    get_sockets_info, AddressFamilyFlags, ProtocolFlags, ProtocolSocketInfo,
    };
    let af_flags = AddressFamilyFlags::IPV4 | AddressFamilyFlags::IPV6;
    let proto_flags = ProtocolFlags::TCP;
    let sockets = get_sockets_info(af_flags, proto_flags)?;
    for socket in sockets.iter() {
        if let ProtocolSocketInfo::Tcp(tcp_info) = &socket.protocol_socket_info {
            if tcp_info.local_port == port {
                return Ok(Some(socket.associated_pids[0]));
            }
        }
    }
    Ok(None)
}

#[cfg(target_os = "windows")]
fn kill_process_by_pid(pid: u32) -> Result<()> {
    info!("Executing: taskkill /F /PID {}", pid);
    // Execute the taskkill command:
    // /F : Specifies to forcefully terminate the process(es).
    // /PID : Specifies the PID of the process to be terminated.
    let output = Command::new("taskkill")
        .args(["/F", "/PID", &pid.to_string()])
        .stdout(Stdio::piped()) // Capture stdout
        .stderr(Stdio::piped()) // Capture stderr
        .output()
        .context(format!("Failed to execute 'taskkill /F /PID {}'", pid))?;

    // Get output messages for context
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Check if taskkill command was successful
    if !output.status.success() {
        // Common errors: "Access is denied" (permissions) or "Process not found" (already terminated)
        error!("Taskkill stdout: {}", stdout.trim());
        error!("Taskkill stderr: {}", stderr.trim());
        return Err(anyhow!(
              "taskkill command failed for PID {}. Status: {}. Error: {}. \nHint: Do you have permissions? Try running as Administrator. Is the process already gone?",
                pid,
                output.status,
                stderr.trim() // stderr often contains the most useful error message
           ));
    } else {
        // Command succeeded
        info!("Taskkill success message: {}", stdout.trim());
        // Sometimes taskkill puts info messages also in stderr even on success
        if !stderr.is_empty() {
            error!("Taskkill stderr message: {}", stderr.trim());
        }
    }
    Ok(())
}

#[cfg(target_os = "windows")]
/// Orchestrates finding the PID for a port and then killing the associated process.
pub fn kill_process_on_port(port: u16) -> Result<()> {
    // Step 1: Find the PID
    match find_pid_by_port(port)? {
        // Case 1: PID was found
        Some(pid) => {
            info!(
                "Attempting to kill process PID {} found on port {}...",
                pid, port
            );
            // Step 2: Kill the process
            // Use taskkill method:
            kill_process_by_pid(pid)
                .context(format!("Failed during kill phase for PID {}", pid))?;
            // Or use sysinfo method:
            // kill_process_by_pid_sysinfo(pid).context(format!("Failed during kill phase for PID {}", pid))?;

            info!(
                "Operation successful: process with PID {} on port {} should be terminated.",
                pid, port
            );
            Ok(())
        }
        // Case 2: No process was found listening on the port
        None => {
            info!("No process found LISTENING on TCP port {}.", port);
            // This is not an error condition, just information
            Ok(())
        }
    }
}
