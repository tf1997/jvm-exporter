use crate::config::Config;
use crate::routes::setup_routes;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::{Arc, RwLock};
use log::{info, error};

#[cfg(target_os = "windows")]
use winreg::enums::*;
#[cfg(target_os = "windows")]
use winreg::RegKey;

pub(crate) async fn init_and_run(
    auto_start: bool,
    should_disable_auto_start: bool,
    java_home_arg: Option<String>,
    full_path_arg: bool,
    config: Arc<RwLock<Config>>, // Add config as a parameter
) {

    let config = Arc::clone(&config);

    if auto_start {
        match configure_auto_start() {
            Ok(_) => println!("Auto-start configuration successful."),
            Err(e) => eprintln!("Failed to configure auto-start: {}", e),
        }
    } else if should_disable_auto_start {
        match disable_auto_start() { // Call async function
            Ok(_) => println!("Auto-start disabled successfully."),
            Err(e) => eprintln!("Failed to disable auto-start: {}", e),
        }
    } else {
        run_server(config.clone(), java_home_arg, full_path_arg).await;
    }
}

pub async fn run_server(
    config: Arc<RwLock<Config>>,
    java_home: Option<String>,
    full_path: bool,
) {
    let config = Arc::clone(&config);
    let java_home = Arc::new(java_home);

    let addr = ([0, 0, 0, 0], 29090);
    let ip_addr = std::net::Ipv4Addr::from(addr.0);
    let routes = setup_routes(java_home, full_path, config);
    let server = warp::serve(routes).bind((ip_addr, addr.1));
    let server_handle = tokio::spawn(server);

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    info!("Server started successfully");
    info!("Listening on http://{}:{}/metrics", "127.0.0.1", addr.1);

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("Received Ctrl+C, shutting down.");
            std::process::exit(0);
        },
        res = server_handle => {
            if let Err(e) = res {
                error!("Server error: {}", e);
            }
        },
    }
}

#[cfg(target_os = "windows")]
pub fn configure_auto_start() -> Result<(), Box<dyn std::error::Error>> {
    let current_exe_path = std::env::current_exe()?;
    let app_name = "ferris-watch";

    // Determine a secure directory for the executable
    // C:\ProgramData is a good choice for application-wide data and executables
    let target_dir = crate::updater::get_app_data_dir().unwrap();
    let target_exe_path = target_dir.join(current_exe_path.file_name().ok_or("Invalid executable file name")?);

    // Create the target directory if it doesn't exist
    if !target_dir.exists() {
        fs::create_dir_all(&target_dir)?;
        println!("Created target directory: {}", target_dir.display());
    }

    // Copy the executable to the secure directory
    fs::copy(&current_exe_path, &target_exe_path)?;
    println!("Executable copied to secure location: {}", target_exe_path.display());

    let target_exe_str = target_exe_path.to_str().ok_or("Invalid target executable path")?;

    // Set auto-start registry entry
    // Using HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows\CurrentVersion\Run for all users, requires admin
    // Or HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Run for current user, no admin
    // For robustness and to prevent user deletion, HKLM is better but requires admin.
    // Let's use HKLM for now, and note the admin requirement.
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let path = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
    let (key, _disp) = hklm.create_subkey(&path)?;

    key.set_value(app_name, &format!("\"{}\" --no-ui", target_exe_str))?; // Add --no-ui argument
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
pub fn configure_auto_start() -> Result<(), Box<dyn std::error::Error>> {
    let service_path = "/etc/systemd/system/ferris-watch.service"; // Consistent with new name
    let binary_target_dir = "/usr/local/bin";
    let binary_target_path = format!("{}/ferris-watch", binary_target_dir); // Consistent with new name

    let current_executable_path = std::env::current_exe()?;
    println!(
        "Current executable path: {}",
        current_executable_path.display()
    );

    if !Path::new(binary_target_dir).exists() {
        fs::create_dir_all(binary_target_dir)?;
        println!("Target directory created: {}", binary_target_dir);
    }

    fs::copy(&current_executable_path, &binary_target_path)?;
    println!("Executable copied to: {}", binary_target_path);

    let java_home = std::env::var("JAVA_HOME").ok();

    let service_content = if let Some(jh) = java_home {
        format!(
            "[Unit]
Description=ferris-watch Service
After=network.target

[Service]
Type=simple
KillMode=process 
ExecStart={}
User=root
Environment=\"JAVA_HOME={}\"
Environment=\"PATH={}/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin\"
Restart=on-failure

[Install]
WantedBy=multi-user.target",
            binary_target_path, jh, jh
        )
    } else {
        format!(
            "[Unit]
Description=ferris-watch Service
After=network.target

[Service]
Type=simple
KillMode=process 
ExecStart={}
User=root
Restart=on-failure

[Install]
WantedBy=multi-user.target",
            binary_target_path
        )
    };

    let service_dir = Path::new("/etc/systemd/system");
    if !service_dir.exists() {
        fs::create_dir_all(service_dir)?;
        println!("Systemd directory created: {}", service_dir.display());
    }

    let mut file = fs::File::create(service_path)?;
    file.write_all(service_content.as_bytes())?;
    println!("Service file created at: {}", service_path);

    std::process::Command::new("systemctl")
        .args(&["daemon-reload"])
        .output()?;

    std::process::Command::new("systemctl")
        .args(&["enable", "ferris-watch.service"]) // Consistent with new name
        .output()?;

    println!("Service configured to auto-start with the system.");
    println!("Use the following commands to manage the service:");
    println!("  Start service:    systemctl start ferris-watch.service"); // Consistent with new name
    println!("  Stop service:     systemctl stop ferris-watch.service"); // Consistent with new name
    println!("  Status of service: systemctl status ferris-watch.service"); // Consistent with new name
    println!("  Enable service on boot: systemctl enable ferris-watch.service"); // Consistent with new name
    println!("  Disable service on boot: systemctl disable ferris-watch.service"); // Consistent with new name
    println!("  Reload daemon after changes: systemctl daemon-reload");

    // Removed std::process::exit(0);
    Ok(())
}

#[cfg(target_os = "windows")]
pub fn disable_auto_start() -> Result<(), Box<dyn std::error::Error>> {
    let app_name = "ferris-watch";
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let path = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
    let key = hklm.open_subkey_with_flags(&path, KEY_SET_VALUE)?;

    key.delete_value(app_name)?;
    info!("Auto-start entry removed from Windows Registry.");

    let task_name = format!("{}AutoRun", app_name);
    info!("Removing scheduled task...");

    std::process::Command::new("schtasks")
        .args(&["/delete", "/tn", &task_name, "/f"])
        .output()
        .expect("Failed to execute delete command");
        
    info!("Task cleanup completed.");

    // Optionally, remove the copied executable
    let target_dir = crate::updater::get_app_data_dir().unwrap();
    let current_exe_path = std::env::current_exe()?;
    let target_exe_path = target_dir.join(current_exe_path.file_name().ok_or("Invalid executable file name")?);

    if target_exe_path.exists() {
        fs::remove_file(&target_exe_path)?;
        info!("Removed executable from secure location: {}", target_exe_path.display());
    }

    // If the directory is empty, remove it
    if target_dir.exists() && fs::read_dir(&target_dir)?.next().is_none() {
        fs::remove_dir(&target_dir)?;
        info!("Removed empty target directory: {}", target_dir.display());
    }

    info!("NOTE: This operation requires administrative privileges to remove auto-start for all users.");
    std::process::exit(0);
}

#[cfg(not(target_os = "windows"))]
pub fn disable_auto_start() -> Result<(), Box<dyn std::error::Error>> {
    let service_path = "/etc/systemd/system/ferris-watch.service";
    let binary_target_path = "/usr/local/bin/ferris-watch";

    // Disable the systemd service
    std::process::Command::new("systemctl")
        .args(&["disable", "ferris-watch.service"])
        .output()?;
    info!("Systemd service disabled.");

    // Stop the systemd service if it's running
    std::process::Command::new("systemctl")
        .args(&["stop", "ferris-watch.service"])
        .output()?;
    info!("Systemd service stopped.");

    // Remove the service file
    if Path::new(service_path).exists() {
        fs::remove_file(service_path)?;
        info!("Removed service file: {}", service_path);
    }

    // Remove the copied binary
    if Path::new(binary_target_path).exists() {
        fs::remove_file(binary_target_path)?;
        println!("Removed binary: {}", binary_target_path);
    }

    std::process::Command::new("systemctl")
        .args(&["daemon-reload"])
        .output()?;
    info!("Systemd daemon reloaded.");

    info!("Auto-start disabled for non-Windows systems.");
    std::process::exit(0);
}
