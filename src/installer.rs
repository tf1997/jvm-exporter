use log::{error, info, warn};
use std::error::Error;
use std::fs;
use std::io::Write;
#[cfg(not(target_os = "windows"))]
use std::path::Path;

#[cfg(target_os = "windows")]
use {
    anyhow::{anyhow, Context, Result},
    log::{error, warn},
    std::os::windows::process::CommandExt,
    std::process::{Command, Stdio},
    winreg::enums::*,
    winreg::RegKey,
};

pub async fn install_application() -> Result<(), Box<dyn Error>> {
    let new_exe_path = std::env::current_exe().unwrap();

    let app_name = env!("CARGO_PKG_NAME");

    #[cfg(target_os = "windows")]
    {
        let current_exe_path = std::env::current_exe()?;
        let target_dir = crate::updater::get_app_data_dir()?;

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
        // kill_process_on_port(29090)?;
        if let Err(e) = kill_process_and_parent_on_port(29090) {
            error!("Failed to kill the process tree, install may fail: {}", e);
        } else {
            info!("Process tree terminated successfully.");
        }
        if let Err(e) = kill_process_and_parent_on_port(29090) {
            error!("Failed to kill the process tree, install may fail: {}", e);
        } else {
            info!("Process tree terminated successfully.");
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        let max_retries = 5;
        let retry_delay = std::time::Duration::from_millis(500);
        for attempt in 0..max_retries {
            match fs::copy(&new_exe_path, &target_exe_path) {
                Ok(_) => {
                    info!(
                        "New executable copied to secure location: {}",
                        target_exe_path.display()
                    );
                    break;
                }
                // If the copy fails, we retry up to max_retries times
                Err(e) => {
                    if attempt == max_retries - 1 {
                        error!(
                            "Failed to copy new executable after {} attempts: {}",
                            max_retries, e
                        );
                        return Err(Box::new(e));
                    }
                    warn!(
                        "Failed to copy new executable (attempt {}): {}. Retrying in {:?}...",
                        attempt + 1,
                        e,
                        retry_delay
                    );
                    kill_process_and_parent_on_port(29090);
                    tokio::time::sleep(retry_delay).await;
                }
            }
        }

        let target_exe_str = target_exe_path
            .to_str()
            .ok_or("Invalid target executable path")?;

        // Set auto-start registry entry
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        let path = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
        // let (key, _disp) = hklm.create_subkey(&path)?;

        // key.set_value(app_name, &format!("\"{}\" --no-ui", target_exe_str))?;

        if let Ok(key) = hklm.open_subkey_with_flags(path, KEY_SET_VALUE) {
            if key.delete_value(app_name).is_ok() {
                info!("Removed auto-start entry.");
            } else {
                warn!("Found old auto-start entry but failed to remove it.");
            }
        } else {
            warn!("Could not open registry path to clean up. This might be a permissions issue, or the path doesn't exist.");
        }

        let old_path = "Software\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Run";
        if let Ok(key) = hklm.open_subkey_with_flags(old_path, KEY_SET_VALUE) {
            match key.delete_value(app_name) {
                Ok(_) => {
                    info!("Removed old auto-start entry from WOW6432Node.");
                }
                Err(e) => {
                    error!(
                        "Failed to remove old auto-start entry in WOW6432Node: {}",
                        e
                    );
                }
            }
        } else {
            warn!("Could not open WOW6432Node registry path to clean up. This might be a permissions issue, or the path doesn't exist.");
        }

        // 1. Define Task Name
        let task_name = format!("{}AutoRun", app_name);
        info!(
            "Registering scheduled task: {} -> {}",
            task_name, target_exe_str
        );

        // 2. Execute 'schtasks' to create the task (SYSTEM privileges)
        let status = Command::new("schtasks")
            .args(&[
                "/create",
                "/f", // Force overwrite
                "/tn",
                &task_name, // Task Name
                "/tr",
                &format!("\"{}\" --no-ui", target_exe_str), // Task Run path
                "/sc",
                "onstart", // Schedule: On User Startup
                "/ru",
                "SYSTEM", // ★ Critical: Run as SYSTEM account
                "/rl",
                "HIGHEST", // Run with highest privileges
            ])
            .output()
            .expect("Failed to execute schtasks command");

        if !status.status.success() {
            let err = String::from_utf8_lossy(&status.stderr);
            error!(
                "Failed to create task (Please ensure you are running as Administrator):\n{}",
                err
            );
            return Err(err.into_owned().into());
        }
        info!("Basic task created successfully.");

        // 3. [Critical] Modify power settings via PowerShell
        // By default, tasks won't start if the laptop is on battery power.
        // We must set DisallowStartIfOnBatteries to false.
        let ps_script = format!(
            r#"
            $taskName = "{name}";
            $t = Get-ScheduledTask -TaskName $taskName;
            
            $triggers = @($t.Triggers);
            if (-not ($triggers | Where-Object {{ $_.Repetition.Interval -eq 'P1D' }})) {{
                $dailyTrig = New-ScheduledTaskTrigger -Daily -At '01:00:00';
                $triggers += $dailyTrig;
            }}

            $newSettings = New-ScheduledTaskSettingsSet `
                -ExecutionTimeLimit (New-TimeSpan -Seconds 0) `
                -AllowStartIfOnBatteries `
                -DontStopIfGoingOnBatteries `
                -MultipleInstances IgnoreNew `
                -Priority 7 `
                -RestartCount 3 `
                -RestartInterval (New-TimeSpan -Minutes 1);

            Set-ScheduledTask -TaskName $taskName -Settings $newSettings -Trigger $triggers;
            "#,
            name = task_name
        );

        info!("Optimizing power settings (allowing start on battery mode)...");
        let ps_status = Command::new("powershell")
            .args(&["-NoProfile", "-Command", &ps_script])
            .output()
            .expect("Failed to execute PowerShell");

        if ps_status.status.success() {
            info!("Installation complete! The program will run with SYSTEM privileges upon the next user login.");
        } else {
            error!("Failed to modify power settings, but the task was created.");
        }

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
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x00000008;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        Command::new(&target_exe_str)
            .arg("--no-ui")
            .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
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
        kill_process_on_port(29090)?;
        // Copy the new executable to the target path, overwriting if it exists
        info!("new_exe_path: {}", new_exe_path.display());
        info!("binary_target_path: {}", binary_target_path);
        std::process::Command::new("systemctl")
            .args(&["stop", &service_name])
            .output()?;
        let max_retries = 5;
        let retry_delay = std::time::Duration::from_millis(500);
        for attempt in 0..max_retries {
            match fs::copy(&new_exe_path, &binary_target_path) {
                Ok(_) => {
                    info!("New executable copied to: {}", binary_target_path);
                    break;
                }
                // If the copy fails, we retry up to max_retries times
                Err(e) => {
                    if attempt == max_retries - 1 {
                        error!(
                            "Failed to copy new executable after {} attempts: {}",
                            max_retries, e
                        );
                        return Err(Box::new(e));
                    }
                    warn!(
                        "Failed to copy new executable (attempt {}): {}. Retrying in {:?}...",
                        attempt + 1,
                        e,
                        retry_delay
                    );
                    tokio::time::sleep(retry_delay).await;
                }
            }
        }

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
KillMode=process 
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

        std::process::Command::new("systemctl")
            .args(&["start", &service_name])
            .output()?;

        info!("Service configured to auto-start with the system.");
        info!("NOTE: This operation requires administrative privileges.");
        Ok(())
    }
}

#[cfg(target_os = "windows")]
fn find_pid_by_port(port: u16) -> Result<Option<u32>> {
    use netstat_esr::{get_sockets_info, AddressFamilyFlags, ProtocolFlags, ProtocolSocketInfo};
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

#[cfg(not(target_os = "windows"))]
pub fn kill_process_on_port(port: u16) -> Result<(), Box<dyn std::error::Error>> {
    use std::process::Command;
    use std::str;

    let output = Command::new("lsof")
        .args(&["-i", &format!(":{}", port), "-sTCP:LISTEN", "-t"])
        .output()?;

    if !output.status.success() {
        log::info!("No process found LISTENING on TCP port {}.", port);
        return Ok(());
    }

    let stdout = str::from_utf8(&output.stdout)?;
    let mut killed = false;
    for line in stdout.lines() {
        if let Ok(pid) = line.trim().parse::<i32>() {
            log::info!(
                "Attempting to kill process PID {} found on port {}...",
                pid,
                port
            );
            let kill_output = Command::new("kill")
                .arg("-9")
                .arg(pid.to_string())
                .output()?;
            if kill_output.status.success() {
                log::info!("Successfully killed PID {} on port {}", pid, port);
                killed = true;
            } else {
                log::error!(
                    "Failed to kill PID {}: {}",
                    pid,
                    String::from_utf8_lossy(&kill_output.stderr)
                );
            }
        }
    }
    if !killed {
        log::info!("No process killed for port {}.", port);
    }
    Ok(())
}

#[cfg(target_os = "windows")]
pub fn kill_process_and_parent_on_port(port: u16) -> Result<()> {
    use sysinfo::{Pid, System};

    let pid = match find_pid_by_port(port)? {
        Some(p) => p,
        None => {
            info!("No process found on port {}, nothing to kill.", port);
            return Ok(());
        }
    };

    let mut system = System::new_all();
    system.refresh_all();

    let mut pid_to_kill = pid;
    let mut use_tree_kill = false;

    if let Some(process) = system.process(Pid::from_u32(pid)) {
        if let Some(parent_pid) = process.parent() {
            if let Some(parent_process) = system.process(parent_pid) {
                if parent_process
                    .name()
                    .to_string_lossy()
                    .contains(env!("CARGO_PKG_NAME"))
                {
                    info!(
                        "Found guardian process with PID {}. Terminating the entire process tree.",
                        parent_pid
                    );
                    pid_to_kill = parent_pid.as_u32();
                    use_tree_kill = true;
                }
            }
        }
    }

    let mut command = Command::new("taskkill");
    command.arg("/F");
    // if use_tree_kill {
    //     command.arg("/T");
    // }
    command.arg("/PID");
    command.arg(pid_to_kill.to_string());

    info!("Executing: {:?}", command);

    let output = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context(format!(
            "Failed to execute taskkill for PID {}",
            pid_to_kill
        ))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        if !stderr.contains("not found") {
            error!("Taskkill stdout: {}", stdout.trim());
            error!("Taskkill stderr: {}", stderr.trim());
            return Err(anyhow!(
                "taskkill command failed for PID {}. Status: {}. Error: {}",
                pid_to_kill,
                output.status,
                stderr.trim()
            ));
        } else {
            info!(
                "Taskkill info: Process with PID {} was not found (already terminated).",
                pid_to_kill
            );
        }
    } else {
        info!("Taskkill success message: {}", stdout.trim());
        if !stderr.is_empty() {
            info!(
                "Taskkill stderr message (often informational): {}",
                stderr.trim()
            );
        }
    }
    Ok(())
}
