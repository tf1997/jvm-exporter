use crate::config::{fetch_and_merge_config, Config};
use crate::routes::setup_routes;
use clap::{App, Arg};
use log::LevelFilter;
use log4rs::append::file::FileAppender;
use log4rs::config::{Appender, Config as Log4rsConfig, Root};
use log4rs::encode::pattern::PatternEncoder;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::{Arc, RwLock};

#[tokio::main]
pub(crate) async fn main() {
    let mut config =
        Config::new("/Users/tengfei.chu/Code/ferris-watch/src/config.yaml").unwrap_or_else(|_| Config {
            log_level: None,
            java_home: None,
            configuration_service_url: None,
            system_processes: None,
            detect_docker_processes: None,
            detect_java_processes: Some(true),
        });

    let configuration_service_url = config.configuration_service_url.clone();
    if let Some(configuration_service_url) = configuration_service_url {
        if let Err(e) = fetch_and_merge_config(&configuration_service_url, &mut config).await {
            eprintln!(
                "Failed to fetch configuration from configuration service: {}",
                e
            );
        }
    }

    // Configure logging to a file in the user's log directory
    let log_dir = dirs::data_local_dir()
        .unwrap_or_else(|| Path::new(".").to_path_buf())
        .join("ferris-watch")
        .join("logs");
    let log_file_path = log_dir.join("ferris-watch.log");

    // Create log directory if it doesn't exist
    if let Err(e) = fs::create_dir_all(&log_dir) {
        eprintln!("Failed to create log directory {:?}: {}", log_dir, e);
    }
eprintln!("Failed to create log directory {:?}", log_dir);
    let log_level_str = config
        .log_level
        .clone()
        .unwrap_or_else(|| "info".to_string());
    let log_level = match log_level_str.to_lowercase().as_str() {
        "error" => LevelFilter::Error,
        "warn" => LevelFilter::Warn,
        "info" => LevelFilter::Info,
        "debug" => LevelFilter::Debug,
        "trace" => LevelFilter::Trace,
        _ => LevelFilter::Info, // Default to Info
    };

    let file_appender = match FileAppender::builder()
        .encoder(Box::new(PatternEncoder::new("{d(%Y-%m-%d %H:%M:%S)} {l} - {m}\n")))
        .build(&log_file_path)
    {
        Ok(appender) => appender,
        Err(e) => {
            eprintln!("Failed to build file appender for {:?}: {}", log_file_path, e);
            // Fallback to stderr if file appender fails
            let stdout_appender = log4rs::append::console::ConsoleAppender::builder()
                .encoder(Box::new(PatternEncoder::new("{d(%Y-%m-%d %H:%M:%S)} {l} - {m}\n")))
                .build();
            let log4rs_config = Log4rsConfig::builder()
                .appender(Appender::builder().build("stdout", Box::new(stdout_appender)))
                .build(Root::builder().appender("stdout").build(log_level))
                .expect("Failed to build fallback log4rs config");
            log4rs::init_config(log4rs_config).expect("Failed to initialize fallback log4rs");
            log::error!("Failed to set up file logging. Logging to stderr instead.");
            return; // Exit main if logging setup fails critically
        }
    };

    let log4rs_config = Log4rsConfig::builder() // Fixed typo: log44rs_config -> log4rs_config
        .appender(Appender::builder().build("file", Box::new(file_appender)))
        .build(Root::builder().appender("file").build(log_level))
        .expect("Failed to build log4rs config");

    log4rs::init_config(log4rs_config).expect("Failed to initialize log4rs");

    log::info!("Logs are being written to: {:?}", log_file_path);
    log::debug!("Log4rs initialized successfully."); // Added for debugging

    let matches = App::new("ferris-watch") // Changed from jvm-exporter
        .version("0.3.6")
        .author("tf1997")
        .about("Monitor the JVM, cpu and memory metrics of process and the system cpu, disk, network and memory metrics.")
        .arg(
            Arg::new("java_home")
                .long("java-home")
                .value_name("JAVA_HOME")
                .help("Sets a custom JAVA_HOME")
                .takes_value(true),
        )
        .arg(
            Arg::new("full_path")
                .long("full-path")
                .help("Only use class name instead of full package path in the process name")
                .takes_value(false),
        )
        .arg(
            Arg::new("auto_start")
                .long("auto-start")
                .help("Configure the program to auto-start with the system"),
        )
        .get_matches();

    let java_home = matches.value_of("java_home").map(|s| s.to_string());
    let full_path = matches.is_present("full_path");
    let auto_start = matches.is_present("auto_start");
    if auto_start {
        match configure_auto_start() {
            Ok(_) => println!("Auto-start configuration successful."),
            Err(e) => eprintln!("Failed to configure auto-start: {}", e),
        }
    }

    let config = Arc::new(RwLock::new(config));

    // Encapsulate shared data into Arc
    let java_home = Arc::new(java_home);

    let addr = ([0, 0, 0, 0], 29090);
    let ip_addr = std::net::Ipv4Addr::from(addr.0);
    let routes = setup_routes(java_home, full_path, config.clone());
    let server = warp::serve(routes).bind((ip_addr, addr.1));
    let server_handle = tokio::spawn(server);

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    println!("Server started successfully");
    println!("Listening on http://{}:{}/metrics", "127.0.0.1", addr.1);

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            println!("Received Ctrl+C, shutting down.");
        },
        res = server_handle => {
            if let Err(e) = res {
                eprintln!("Server error: {}", e);
            }
        },
    }
}

fn configure_auto_start() -> Result<(), Box<dyn std::error::Error>> {
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
Description=JVM Exporter Service
After=network.target

[Service]
Type=simple
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
Description=JVM Exporter Service
After=network.target

[Service]
Type=simple
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

    std::process::exit(0);
}
