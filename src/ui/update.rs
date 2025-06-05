use ribir::{prelude::*};
use crate::updater;
use log::{info, error};
use std::path::PathBuf;
use std::rc::Rc;
use tokio::runtime::Runtime;

fn app_buttons(download_url: String) -> impl WidgetBuilder {
    fn_widget! {
        let download_url_clone = download_url.clone();
        @Column {
            margin: EdgeInsets::all(20.),
            item_gap: 20.,
            @Text {
                text: "New version available! Do you want to update now?",
                h_align: HAlign::Center,
            }
            @Row {
            align_items: Align::Center,
            h_align: HAlign::Center,
            margin: EdgeInsets::all(20.),
            item_gap: 20.,
            @FilledButton {
                on_tap: move |e1| {
                    let download_url_for_spawn = download_url_clone.clone();
                    let window = e1.window();
                    info!("Starting update download from UI...");
                    show_info_dialog("Downloading update, please wait...", window.clone());

                    std::thread::spawn(move || {
                        let rt = Runtime::new().unwrap();
                        rt.block_on(async move {
                            match updater::download_update(&download_url_for_spawn).await {
                                Ok(_) => {
                                    info!("Update downloaded successfully from UI. Attempting to run new executable.");
                                    let app_data_dir = match updater::get_app_data_dir() {
                                        Ok(dir) => dir,
                                        Err(e) => {
                                            error!("Failed to get app data dir: {}", e);
                                            // No UI echo as per user's request
                                            return;
                                        }
                                    };
                                    let current_exe = match std::env::current_exe() {
                                        Ok(exe) => exe,
                                        Err(e) => {
                                            error!("Failed to get current executable path: {}", e);
                                            // No UI echo as per user's request
                                            return;
                                        }
                                    };
                                    let app_name = current_exe.file_name().unwrap().to_str().unwrap();
                                    let downloaded_file_path: PathBuf = app_data_dir.join(format!("{}_new", app_name));

                                    info!("Attempting to run new executable: {:?}", downloaded_file_path);
                                    match std::process::Command::new(&downloaded_file_path)
                                        .arg("--install-ui")
                                        .spawn() {
                                        Ok(_) => {
                                            std::process::exit(0); // Exit the current process after starting the new one
                                        },
                                        Err(e) => {
                                            error!("Failed to start new executable: {}", e);
                                            // No UI echo as per user's request
                                        }
                                    }
                                },
                                Err(e) => {
                                    error!("Update download failed from UI: {}", e);
                                    // No UI echo as per user's request
                                }
                            }
                        });
                    });
                },
                @{ Label::new("Update") }
            }
            @OutlinedButton {
                on_tap: move |e| {
                    show_info_dialog("Update cancelled. The application will close in 3 seconds.", e.window());
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

pub fn app(download_url: String)  {
    App::run(app_buttons(download_url))
    .with_title("Ferris Watch Update")
    .with_size(Size::new(400., 150.))
    .with_resizable(false);
}
