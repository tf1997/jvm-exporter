mod monitor;
mod routes;
mod config; // New module import
mod ui;

mod metrics {
    pub mod collect;
    pub mod metrics;
    pub mod timer;
}



fn main() {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        crate::ui::app();
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        println!("Starting JVM Exporter directly (non-Windows/macOS).");
        monitor::main();
    }
}
