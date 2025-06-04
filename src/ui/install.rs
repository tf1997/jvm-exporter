use ribir::{prelude::*};
use std::rc::Rc;
use crate::config::Config;
use std::sync::{Arc, RwLock};

fn app_buttons(config: Arc<RwLock<Config>>) -> impl WidgetBuilder {
    fn_widget! {
        let config_for_check_update = config.clone();

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
                    // 启动独立进程
                    // let current_exe = std::env::current_exe();
                    // match current_exe {
                    //     Ok(exe_path) => {
                    //         let mut command = std::process::Command::new(exe_path);
                    //         command.arg("--no-ui");
                    //         match command.spawn() {
                    //             Ok(_) => show_info_dialog("Background monitor started successfully!", e.window()),
                    //             Err(e1) => show_info_dialog(format!("Failed to start background monitor: {}", e1), e.window()),
                    //         }
                    //     },
                    //     Err(e1) => show_info_dialog(format!("Failed to get executable path: {}", e1), e.window()),
                    // }
                    show_info_dialog("Install successfully!", e.window())
                },
                @{ Label::new("Install") }
            }
            @OutlinedButton {
                on_tap: move |e| {
                    
                        show_info_dialog("Cancel successfully, the window will be closed in 3 seconds!", e.window())
                        sleep(std::time::Duration::from_secs(3)).await;
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

pub fn app(config: Arc<RwLock<Config>>)  {
    App::run(app_buttons(config))
    .with_title("Ferris Watch")
    .with_size(Size::new(400., 150.))
    .with_resizable(false);
}
