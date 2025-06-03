use ribir::{prelude::*};
use std::rc::Rc;
use crate::monitor;

fn app_buttons() -> impl WidgetBuilder {
    fn_widget! {

        @Column {
            align_items: Align::Center,
            margin: EdgeInsets::all(20.),
            item_gap: 20.,
            @FilledButton {
                on_tap: move |e| {
                    // 启动独立进程
                    std::thread::spawn(move || {
                                    println!("Background thread for monitor::main() started.");
                                    monitor::main();
                    });
                
                    show_info_dialog("Background started successfully!", e.window());
                    // handle.detach();
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
                on_tap: move |e| {
                    show_info_dialog("Nothing to update!", e.window());
                },
                @{ Label::new("Check Update") }
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

pub fn app()  {
    App::run(app_buttons()).with_title("Ferris Watch");
}
