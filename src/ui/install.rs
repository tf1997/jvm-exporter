use crate::config::Config;
use crate::installer;
use crate::updater;
use ribir::prelude::*;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, RwLock};
use tokio::runtime::Runtime;
use log::{info, error};
fn app_buttons( config: Arc<RwLock<Config>>) -> impl WidgetBuilder {
    fn_widget! {

        @Column {
            margin: EdgeInsets::all(20.),
            item_gap: 20.,
            @Text {
                text: "Do you want to install now?",
                h_align: HAlign::Center,
            }
            @Row {
            align_items: Align::Center,
            h_align: HAlign::Center,
            margin: EdgeInsets::all(20.),
            item_gap: 20.,
            @FilledButton {
                on_tap: move |e| {
                    let window = e.window();
                    std::thread::spawn(move || {
                        let rt = Runtime::new().unwrap();
                        rt.block_on(async {
                            let app_data_dir = match updater::get_app_data_dir() {
                                Ok(dir) => dir,
                                Err(e) => {
                                    // show_info_dialog(format!("Error getting app data directory: {}", e), window.clone());
                                    error!("Error getting app data directory: {}", e);
                                    return;
                                }
                            };
                            let current_exe = std::env::current_exe().unwrap();
                            let app_name = current_exe.file_name().unwrap().to_str().unwrap();
                            let downloaded_file_path: PathBuf = app_data_dir.join(format!("{}", app_name));

                            if !downloaded_file_path.exists() {
                                // show_info_dialog("No new installer found in download directory. Please update first.", window.clone());
                                error!("No new installer found ({})in download directory. Please update first.", downloaded_file_path.display());
                                return;
                            }

                            match installer::install_application(&downloaded_file_path).await {
                                Ok(_) => {
                                    // show_info_dialog("Installation successful! Please restart the application.", window.clone());
                                    info!("nstallation successful! Please restart the application.");
                                    std::process::exit(0);
                                },
                                Err(e) => {
                                    error!("Installation failed: {}. Please run as administrator.", e);
                                    // show_info_dialog(format!("Installation failed: {}. Please run as administrator.", e), window.clone());
                                }
                            }
                        });
                    });

                },
                @{ Label::new("Install") }
            }
            @OutlinedButton {
                on_tap: move |e| {

                        show_info_dialog("Cancel successfully, the window will be closed in 3 seconds!", e.window());
                        std::process::exit(0);

                },
                @{ Label::new("Cancel") }
            }
        }
        }

    }
}

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

pub fn app(config: Arc<RwLock<Config>>) {
    App::run(app_buttons(config))
        .with_title("Ferris Watch")
        .with_size(Size::new(400., 150.))
        .with_resizable(false);
}
