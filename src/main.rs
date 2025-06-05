#![windows_subsystem = "windows"]
mod monitor;
mod routes;
mod config;
mod updater;
mod installer;
mod ui{
    pub mod home;
    pub mod update;
    pub mod install;
}
mod metrics {
    pub mod collect;
    pub mod metrics;
    pub mod timer;
}

use clap;
use log::{info, error, LevelFilter};
use crate::config::{fetch_and_merge_config, Config};
use log4rs::append::file::FileAppender;
use log4rs::config::{Appender, Config as Log4rsConfig, Root};
use log4rs::encode::pattern::PatternEncoder;
use std::fs;
use dirs;
use std::sync::{Arc, RwLock};

#[tokio::main]
async fn main() {
    let app_name = env!("CARGO_PKG_NAME");
    
    let target_dir = dirs::data_dir()
            .ok_or("Could not find a suitable data directory for Windows.").unwrap()
            .join(app_name);
    let config_path = target_dir.join("config.yaml");
    
    let mut config = Config::new(config_path.to_str().unwrap()).unwrap_or_else(|e| {
        eprintln!("Failed to load config.yaml: {}", e);
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

    let configuration_service_url = config.configuration_service_url.clone();
    if let Some(configuration_service_url) = configuration_service_url {
        if let Err(e) = fetch_and_merge_config(&configuration_service_url, &mut config).await {
            eprintln!(
                "Failed to fetch configuration from configuration service: {}",
                e
            );
        }
    }

    let config = Arc::new(RwLock::new(config));

    init_logger(app_name, Arc::clone(&config));

    info!("Using config file at: {:?}", config_path);
    info!("config.to_str() is: {:?}", config);

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
        .arg(
            clap::Arg::new("install_ui")
                .long("install-ui")
                .help("Run the program with a install UI (for Windows and macOS)"),
        )
        .get_matches();

    let java_home = matches.value_of("java_home").map(|s| s.to_string());
    let full_path = matches.is_present("full_path");
    let auto_start = matches.is_present("auto_start");
    let should_disable_auto_start = matches.is_present("disable_auto_start");
    let no_ui = matches.is_present("no_ui");
    let install_ui = matches.is_present("install_ui");

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        if install_ui {
            info!("Running in install UI mode.");
            crate::ui::install::app();
            return; // Exit main after showing install UI
        }
        if !no_ui {
            info!("Checking for updates on startup...");
            let config_for_update_check = Arc::clone(&config);
            match updater::check_for_update(config_for_update_check).await {
                Ok(Some(download_url)) => {
                    info!("Update available. Navigating to update page.");
                    crate::ui::update::app(download_url);
                    return; // Exit main after showing update UI
                },
                Ok(None) => {
                    info!("No update available on startup.");
                },
                Err(e) => {
                    error!("Failed to check for updates on startup: {}", e);
                }
            }
        }

        if auto_start || should_disable_auto_start || no_ui {
            monitor::init_and_run(auto_start, should_disable_auto_start, java_home, full_path, config).await;
        } else {
            // If no specific flags, launch UI which will then handle starting the monitor
            crate::ui::home::app(config);
        }
    }

    // Spawn a task for daily update checks
    // let config_for_daily_update = Arc::clone(&config);
    // tokio::spawn(async move {
    //     loop {
    //         let config = Arc::clone(&config_for_daily_update);
    //         // Calculate time until next midnight (or a specific hour, e.g., 3 AM)
    //         let now = Local::now();
    //         let next_check = (now + Duration::from_secs(24 * 3600)) // Add 24 hours
    //             .with_hour(3).unwrap() // Set to 3 AM
    //             .with_minute(0).unwrap()
    //             .with_second(0).unwrap()
    //             .with_nanosecond(0).unwrap();

    //         let sleep_duration = if next_check > now {
    //             next_check.signed_duration_since(now).to_std().unwrap_or_default()
    //         } else {
    //             // If next_check is in the past (e.g., if current time is after 3 AM),
    //             // schedule for 3 AM tomorrow.
    //             (next_check + Duration::from_secs(24 * 3600)).signed_duration_since(now).to_std().unwrap_or_default()
    //         };

    //         info!("Next update check scheduled in: {:?}", sleep_duration);
    //         sleep(sleep_duration).await;

    //         info!("Performing scheduled update check...");
    //         if let Some(download_url) = updater::check_for_update(config).await.unwrap_or(None) {
    //             info!("New version available! Downloading update...");
    //             if let Err(e) = updater::download_update(&download_url).await {
    //                 error!("Scheduled update download failed: {}", e);
    //             }
    //         }
    //     }
    // });

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        println!("Starting ferris-watch directly (non-Windows/macOS).");
        monitor::init_and_run(auto_start, should_disable_auto_start, no_ui, java_home, full_path, config).await;
    }
}

fn init_logger(app_name: &str, config: Arc<RwLock<Config>>) {
    let log_dir = std::env::temp_dir()
        .join(app_name)
        .join("logs");

    let log_file_path = log_dir.join("ferris-watch.log");

    let log_level_str = config
        .read()
        .unwrap()
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

    // Create log directory if it doesn't exist
    if let Err(e) = fs::create_dir_all(&log_dir) {
        eprintln!("Failed to create log directory {:?}: {}", log_dir, e);
    }
    eprintln!("Create log directory successfully {:?}", log_dir);

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
}
