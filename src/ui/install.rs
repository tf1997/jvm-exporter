
use crate::installer;
use ribir::prelude::*;
use std::rc::Rc;
use tokio::runtime::Runtime;
use log::{info, error};
fn app_buttons() -> impl WidgetBuilder {
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
                on_tap: move |_| {
                    std::thread::spawn(move || {
                        let rt = Runtime::new().unwrap();
                        rt.block_on(async {
                            match installer::install_application().await {
                                Ok(_) => {
                                    // show_info_dialog("Installation successful! Please restart the application.", window.clone());
                                    info!("Installation successful! Please restart the application.");
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

pub fn app() {
    App::run(app_buttons())
        .with_title("Ferris Watch")
        .with_size(Size::new(400., 150.))
        .with_resizable(false);
}
