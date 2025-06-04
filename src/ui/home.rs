use ribir::{prelude::*};
use std::rc::Rc; // Keep Rc for other dialogs if needed, or remove if not
use crate::monitor;
use crate::updater; // Add updater module
use tokio; // Import tokio for spawning async tasks
use log::{info, error}; // Import info and error for logging
use crate::config::Config; // Import Config
use std::sync::{Arc, RwLock}; // Import Arc and RwLock for shared state

fn app_buttons(config: Arc<RwLock<Config>>) -> impl WidgetBuilder {
    fn_widget! {
        let config_for_check_update = config.clone();

        @Column {
            align_items: Align::Center,
            h_align: HAlign::Center,
            margin: EdgeInsets::all(20.),
            item_gap: 20.,
            @FilledButton {
                on_tap: move |e| {
                    // 启动独立进程
                    let current_exe = std::env::current_exe();
                    match current_exe {
                        Ok(exe_path) => {
                            let mut command = std::process::Command::new(exe_path);
                            command.arg("--no-ui");
                            match command.spawn() {
                                Ok(_) => show_info_dialog("Background monitor started successfully!", e.window()),
                                Err(e1) => show_info_dialog(format!("Failed to start background monitor: {}", e1), e.window()),
                            }
                        },
                        Err(e1) => show_info_dialog(format!("Failed to get executable path: {}", e1), e.window()),
                    }
                },
                @{ Label::new("Start") }
            }
            @FilledButton {
                on_tap: move |e| {
                    match monitor::configure_auto_start(){
                        Ok(_) => show_info_dialog("Autostart installed successfully!", e.window()),
                        Err(e1) => show_info_dialog(format!("Failed to install autostart: {}", e1), e.window()),
                    }
                },
                @{ Label::new("Install Autostart") }
            }
        
            @FilledButton {
                 on_tap: move |e| {
                    match monitor::disable_auto_start(){
                        Ok(_) => show_info_dialog("Autostart uninstalled successfully!", e.window()),
                        Err(e1) => show_info_dialog(format!("Failed to uninstall autostart: {}", e1), e.window()),
                    }
                },
                @{ Label::new("Uninstall Autostart") }
            }
            @FilledButton {
                on_tap: move |_| { // Changed to |_| as e.window() is not used directly here
                    let value = config_for_check_update.clone();
                    tokio::spawn(async move {
                        match updater::check_and_update(value).await {
                            Ok(_) => {
                                info!("Update check completed. Check logs for details.");
                            },
                            Err(e) => {
                                error!("Update check failed: {}", e);
                            }
                        }
                    });
                },
                @{ Label::new("Check Update") }
            }
        }
    }
}

// Keep show_info_dialog for other buttons if they still use it.
// If not, this function and its related imports (Rc, CowArc, Overlay, Text) can be removed.
fn show_info_dialog(message: impl Into<CowArc<str>>, window: Rc<ribir::prelude::Window>) {
    let message = message.into();
    let overlay = Overlay::new(fn_widget! {
        @Text {
            h_align: HAlign::Center,
            v_align: VAlign::Center,
            text: message
        }
    });
    overlay.show(window);   
}

pub fn app(config: Arc<RwLock<Config>>)  {
    App::run(app_buttons(config))
        .with_title("Ferris Watch")
        .with_size(Size::new(400., 300.))
        .with_resizable(false);
}
