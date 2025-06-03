#[cfg(target_os = "macos")]
use std::process::Command;

#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONINFORMATION, MB_ICONWARNING, MB_OK, MB_YESNOCANCEL, IDYES, IDNO, IDCANCEL};
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::HWND;

#[cfg(target_os = "macos")]
pub fn show_main_prompt() -> Option<i32> {
    let script = r#"
        tell application "System Events"
            activate
            set choice to button returned of (display dialog "Please select an operation for Ferris watch:" buttons {"Start", "Install Autostart", "Uninstall Autostart"} default button "Start" with title "Ferris watch (Hirain DT)")
        end tell
        return choice
    "#;

    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output();

    match output {
        Ok(output) => {
            if !output.stderr.is_empty() {
                eprintln!("osascript stderr: {}", String::from_utf8_lossy(&output.stderr));
            }
            let choice = String::from_utf8_lossy(&output.stdout).trim().to_string();
            match choice.as_str() {
                "Start" => Some(0),
                "Install Autostart" => Some(1),
                "Uninstall Autostart" => Some(2),
                _ => None, // Any other result (including closing the dialog) maps to None
            }
        },
        Err(e) => {
            eprintln!("Failed to execute osascript command: {}", e);
            None
        }
    }
}

#[cfg(target_os = "windows")]
pub fn show_main_prompt() -> Option<i32> {
    let title = "Ferris watch (Hirain DT)";
    let text = "Please select an operation for Ferris watch:\n\n(Note: To close, click the 'X' button on the window)";
    let lp_text: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let lp_caption: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();

    let result = unsafe {
        MessageBoxW(
            HWND(0),
            lp_text.as_ptr(),
            lp_caption.as_ptr(),
            MB_YESNOCANCEL | MB_ICONINFORMATION,
        )
    };

    match result {
        IDYES => Some(0), // Start
        IDNO => Some(1),  // Install Autostart
        IDCANCEL => Some(2), // Uninstall Autostart (closest to a "Close" or "Cancel" action)
        _ => None, // Dialog closed or other result
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn show_main_prompt() -> Option<i32> {
    println!("Please select an operation for Ferris watch:");
    println!("0: Start");
    println!("1: Install Autostart");
    println!("2: Uninstall Autostart");
    println!("Hirain DT");
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).ok()?;
    input.trim().parse().ok()
}

#[cfg(target_os = "macos")]
pub fn show_error_prompt(title: &str, message: &str) {
    let script = format!(r#"
        tell application "System Events"
            display dialog "{}" with title "Ferris watch - {}" buttons {{"OK"}} default button "OK" with icon caution
        end tell
    "#, message, title);
    Command::new("osascript").arg("-e").arg(script).output().unwrap();
}

#[cfg(target_os = "windows")]
pub fn show_error_prompt(title: &str, message: &str) {
    let full_title = format!("Ferris watch - {}", title);
    let lp_text: Vec<u16> = message.encode_utf16().chain(std::iter::once(0)).collect();
    let lp_caption: Vec<u16> = full_title.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        MessageBoxW(
            HWND(0),
            lp_text.as_ptr(),
            lp_caption.as_ptr(),
            MB_OK | MB_ICONWARNING,
        );
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn show_error_prompt(title: &str, message: &str) {
    eprintln!("Error: {}: {}", title, message);
}

#[cfg(target_os = "macos")]
pub fn show_success_prompt(title: &str, message: &str) {
    let script = format!(r#"
        tell application "System Events"
            display dialog "{}" with title "Ferris watch - {}" buttons {{"OK"}} default button "OK" with icon note
        end tell
    "#, message, title);
    Command::new("osascript").arg("-e").arg(script).output().unwrap();
}

#[cfg(target_os = "windows")]
pub fn show_success_prompt(title: &str, message: &str) {
    let full_title = format!("Ferris watch - {}", title);
    let lp_text: Vec<u16> = message.encode_utf16().chain(std::iter::once(0)).collect();
    let lp_caption: Vec<u16> = full_title.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        MessageBoxW(
            HWND(0),
            lp_text.as_ptr(),
            lp_caption.as_ptr(),
            MB_OK | MB_ICONINFORMATION,
        );
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn show_success_prompt(title: &str, message: &str) {
    println!("Success: {}: {}", title, message);
}
