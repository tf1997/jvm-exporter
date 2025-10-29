#![cfg(target_os = "windows")]

use std::ffi::c_void;
use std::iter::once;
use std::ptr;
use windows_sys::Win32::System::EventLog::{
    DeregisterEventSource, RegisterEventSourceW, ReportEventW, EVENTLOG_ERROR_TYPE,
};

// Helper function to convert a Rust string to a Windows wide string (UTF-16)
fn to_wide_string(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(once(0)).collect()
}

pub fn setup_panic_hook() {
    let app_name = env!("CARGO_PKG_NAME");
    std::panic::set_hook(Box::new(move |panic_info| {
        let message = format!("A panic occurred in {}: {:?}", app_name, panic_info);
        eprintln!("{}", message); // Also print to stderr for visibility

        // Manually report the event using low-level Windows API
        unsafe {
            let source_name = to_wide_string(app_name);
            let event_source = RegisterEventSourceW(ptr::null(), source_name.as_ptr());

            if event_source != 0 {
                let msg = to_wide_string(&message);
                let strings: [*const u16; 1] = [msg.as_ptr()];

                ReportEventW(
                    event_source,
                    EVENTLOG_ERROR_TYPE,
                    0,       // Event Category
                    101,     // Event ID
                    ptr::null_mut(), // User SID
                    1,       // Number of strings
                    0,       // Raw data size
                    strings.as_ptr(),
                    ptr::null_mut(), // Raw data
                );

                DeregisterEventSource(event_source);
            } else {
                eprintln!("Failed to register event source with Windows.");
            }
        }
    }));
}
