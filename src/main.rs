mod monitor;
mod routes;
mod config;
mod ui;
mod updater;

mod metrics {
    pub mod collect;
    pub mod metrics;
    pub mod timer;
}

use clap;
use log::{info, error, LevelFilter};
use tokio::time::{sleep, Duration};
use chrono::{Local, Timelike};
use crate::config::Config;
use log4rs::append::file::FileAppender;
use log4rs::config::{Appender, Config as Log4rsConfig, Root};
use log4rs::encode::pattern::PatternEncoder;
use std::fs;
use std::path::Path;
use dirs;

#[tokio::main]
async fn main() {
    let config = Config::new("/Users/tengfei.chu/Code/jvm-exporter/src/config.yaml").unwrap_or_else(|e| {
        error!("Failed to load config.yaml: {}", e);
        // Provide a default config if loading fails
        Config {
            log_level: None,
            java_home: None,
            configuration_service_url: None,
            system_processes: None,
            detect_docker_processes: Some(false),
            detect_java_processes: Some(true),
            update_service_url: None
        }
    });

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
    eprintln!("Create log directory successfully {:?}", log_dir);
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

    let stdout_appender = log4rs::append::console::ConsoleAppender::builder()
        .encoder(Box::new(PatternEncoder::new("{d(%Y-%m-%d %H:%M:%S)} {l} - {m}\n")))
        .build();

    let log4rs_config = {
        let mut config_builder = Log4rsConfig::builder();
        config_builder = config_builder.appender(Appender::builder().build("stdout", Box::new(stdout_appender)));

        match FileAppender::builder()
            .encoder(Box::new(PatternEncoder::new("{d(%Y-%m-%d %H:%M:%S)} {l} - {m}\n")))
            .build(&log_file_path)
        {
            Ok(file_appender) => {
                config_builder = config_builder.appender(Appender::builder().build("file", Box::new(file_appender)));
                config_builder.build(Root::builder().appender("file").appender("stdout").build(log_level))
            },
            Err(e) => {
                eprintln!("Failed to build file appender for {:?}: {}", log_file_path, e);
                log::error!("Failed to set up file logging. Logging to console only.");
                config_builder.build(Root::builder().appender("stdout").build(log_level))
            }
        }
    }.expect("Failed to build log4rs config");

    log4rs::init_config(log4rs_config).expect("Failed to initialize log4rs");

    log::info!("Logs are being written to: {:?}", log_file_path);
    log::debug!("Log4rs initialized successfully.");

    let matches = clap::App::new("ferris-watch")
        .version("0.3.6")
        .author("tf1997")
        .about("Monitor the JVM, cpu and memory metrics of process and the system cpu, disk, network and memory metrics.")
        .arg(
            clap::Arg::new("java_home")
                .long("java-home")
                .value_name("JAVA_HOME")
                .help("Sets a custom JAVA_HOME")
                .takes_value(true),
        )
        .arg(
            clap::Arg::new("full_path")
                .long("full-path")
                .help("Only use class name instead of full package path in the process name")
                .takes_value(false),
        )
        .arg(
            clap::Arg::new("auto_start")
                .long("auto-start")
                .help("Configure the program to auto-start with the system"),
        )
        .arg(
            clap::Arg::new("disable_auto_start")
                .long("disable-auto-start")
                .help("Disable the program from auto-starting with the system"),
        )
        .arg(
            clap::Arg::new("no_ui")
                .long("no-ui")
                .help("Run the program without a UI (for Windows and macOS)"),
        )
        .get_matches();

    let java_home = matches.value_of("java_home").map(|s| s.to_string());
    let full_path = matches.is_present("full_path");
    let auto_start = matches.is_present("auto_start");
    let should_disable_auto_start = matches.is_present("disable_auto_start");
    let no_ui = matches.is_present("no_ui");

    // Handle auto-start configuration
    if auto_start {
        info!("Attempting to set up auto-start...");
        if let Err(e) = updater::setup_autostart().await {
            error!("Failed to set up auto-start: {}", e);
        } else {
            info!("Auto-start setup successfully.");
        }
    } else if should_disable_auto_start {
        info!("Auto-start disable not implemented yet."); // TODO: Implement disable auto-start
    }

    // Spawn a task for daily update checks
    let config_clone_for_updater = config.clone();
    tokio::spawn(async move {
        loop {
            // Calculate time until next midnight (or a specific hour, e.g., 3 AM)
            let now = Local::now();
            let next_check = (now + Duration::from_secs(24 * 3600)) // Add 24 hours
                .with_hour(3).unwrap() // Set to 3 AM
                .with_minute(0).unwrap()
                .with_second(0).unwrap()
                .with_nanosecond(0).unwrap();

            let sleep_duration = if next_check > now {
                next_check.signed_duration_since(now).to_std().unwrap_or_default()
            } else {
                // If next_check is in the past (e.g., if current time is after 3 AM),
                // schedule for 3 AM tomorrow.
                (next_check + Duration::from_secs(24 * 3600)).signed_duration_since(now).to_std().unwrap_or_default()
            };

            info!("Next update check scheduled in: {:?}", sleep_duration);
            sleep(sleep_duration).await;

            info!("Performing scheduled update check...");
            if let Err(e) = updater::check_and_update(config_clone_for_updater.clone()).await {
                error!("Scheduled update check failed: {}", e);
            }
        }
    });

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        if auto_start || should_disable_auto_start || no_ui {
            monitor::init_and_run(auto_start, should_disable_auto_start, no_ui, java_home, full_path, config.clone()).await;
        } else {
            // If no specific flags, launch UI which will then handle starting the monitor
            crate::ui::app(config.clone());
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        println!("Starting ferris-watch directly (non-Windows/macOS).");
        monitor::init_and_run(auto_start, should_disable_auto_start, no_ui, java_home, full_path, config.clone()).await;
    }
}
