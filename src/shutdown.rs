// This module is conditionally compiled and will only be included on Windows.
#![cfg(target_os = "windows")]

use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};
use std::thread;
use log::info;

/// Spawns a new thread to run an event loop with an invisible window to block system shutdown.
/// This function is Windows-specific.
pub fn prevent_shutdown() {
    thread::spawn(|| {
        info!("Starting shutdown prevention loop...");
        let event_loop = EventLoop::new().unwrap();
        
        // Create a window, but we don't need it to be visible.
        let _window = WindowBuilder::new()
            .with_title("Shutdown Blocker")
            .with_visible(false) // Set to invisible
            .build(&event_loop)
            .unwrap();

        event_loop.run(move |event, elwt| {
            elwt.set_control_flow(ControlFlow::Wait);

            match event {
                Event::WindowEvent {
                    event: WindowEvent::CloseRequested,
                    ..
                } => {
                    // When a close request is received (e.g., from the system shutting down),
                    // we ignore it. By not closing the window, we signal to the OS that
                    // the application is not ready to terminate, thus blocking the shutdown.
                    info!("System shutdown request received and blocked.");
                    // Intentionally do not call elwt.exit() to keep the application running.
                },
                Event::WindowEvent {
                    event: WindowEvent::Destroyed,
                    ..
                } => {
                    // If the window is destroyed for any other reason, exit the loop.
                    elwt.exit();
                }
                _ => (),
            }
        }).unwrap();
    });
}

// For non-Windows platforms, we provide a stub function that does nothing.
// This ensures that the code compiles on all platforms.
#[cfg(not(target_os = "windows"))]
pub fn prevent_shutdown() {
    // This feature is only available on Windows.
}
