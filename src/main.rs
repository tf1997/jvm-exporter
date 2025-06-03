mod monitor;
mod routes;
mod config;
mod gui; // New module import
mod autostart; // New module import

mod metrics {
    pub mod collect;
    pub mod metrics;
    pub mod timer;
}



fn main() {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        let choice = gui::show_main_prompt();

        match choice {
            Some(0) => { // "Start"
                println!("Starting JVM Exporter...");
                monitor::main();
            },
            Some(1) => { // "Install Autostart"
                println!("Installing autostart...");
                if let Err(e) = autostart::install_autostart() {
                    eprintln!("Failed to install autostart: {}", e);
                    gui::show_error_prompt("Error", &format!("Failed to install autostart: {}", e));
                } else {
                    println!("Autostart installed successfully.");
                    gui::show_success_prompt("Success", "Autostart installed successfully.");
                }
            },
            Some(2) => { // "Uninstall Autostart"
                println!("Uninstalling autostart...");
                if let Err(e) = autostart::uninstall_autostart() {
                    eprintln!("Failed to uninstall autostart: {}", e);
                    gui::show_error_prompt("Error", &format!("Failed to uninstall autostart: {}", e));
                } else {
                    println!("Autostart uninstalled successfully.");
                    gui::show_success_prompt("Success", "Autostart uninstalled successfully.");
                }
            },
            _ => {
                println!("No option selected or prompt closed.");
            }
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        println!("Starting JVM Exporter directly (non-Windows/macOS).");
        monitor::main();
    }
}
