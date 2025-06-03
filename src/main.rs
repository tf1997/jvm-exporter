mod monitor;
mod routes;
mod config;
mod ui;

mod metrics {
    pub mod collect;
    pub mod metrics;
    pub mod timer;
}

use clap::{App, Arg};

#[tokio::main]
async fn main() {
    let matches = App::new("ferris-watch")
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
        .arg(
            Arg::new("disable_auto_start")
                .long("disable-auto-start")
                .help("Disable the program from auto-starting with the system"),
        )
        .arg(
            Arg::new("no_ui")
                .long("no-ui")
                .help("Run the program without a UI (for Windows and macOS)"),
        )
        .get_matches();

    let java_home = matches.value_of("java_home").map(|s| s.to_string());
    let full_path = matches.is_present("full_path");
    let auto_start = matches.is_present("auto_start");
    let should_disable_auto_start = matches.is_present("disable_auto_start");
    let no_ui = matches.is_present("no_ui");

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        if auto_start || should_disable_auto_start || no_ui {
            monitor::init_and_run(auto_start, should_disable_auto_start, no_ui, java_home, full_path).await;
        } else {
            // If no specific flags, launch UI which will then handle starting the monitor
            crate::ui::app();
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        println!("Starting ferris-watch directly (non-Windows/macOS).");
        monitor::init_and_run(auto_start, should_disable_auto_start, no_ui, java_home, full_path).await;
    }
}
