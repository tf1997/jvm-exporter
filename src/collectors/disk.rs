//! This module encapsulates the logic for collecting disk SMART status
//! across different operating systems.

/// A unified structure to hold disk health information.
#[derive(Debug, Clone)]
pub struct DiskHealth {
    /// Device name (e.g., "sda", "\\.\PHYSICALDRIVE0", "disk0").
    pub name: String,
    /// Disk model name.
    pub model: String,
    /// Disk serial number.
    pub serial: String,
    /// A numeric representation of health: 1 for OK, 0 for Failing/Not OK.
    pub health_ok: i64,
    /// The raw status string from the underlying tool (e.g., "OK", "Verified", "PASSED").
    pub raw_status: String,
}

/// A trait for any collector that can gather SMART status.
pub trait SmartCollector {
    fn collect(&self) -> Result<Vec<DiskHealth>, String>;
}

/// Factory function to get the appropriate collector for the current OS.
#[cfg(target_os = "windows")]
pub fn new_collector() -> Box<dyn SmartCollector + Send + Sync> {
    Box::new(WmiApiCollector)
}


#[cfg(target_os = "macos")]
pub fn new_collector() -> Box<dyn SmartCollector + Send + Sync> {
    Box::new(DiskutilCollector)
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub fn new_collector() -> Box<dyn SmartCollector + Send + Sync> {
    Box::new(UnsupportedCollector)
}

// --- Windows Collector using WMIC ---
#[cfg(target_os = "windows")]
struct WmicCollector;

#[cfg(target_os = "windows")]
impl SmartCollector for WmicCollector {
    fn collect(&self) -> Result<Vec<DiskHealth>, String> {
        use std::os::windows::process::CommandExt;
        use std::process::Command;

        const CREATE_NO_WINDOW: u32 = 0x08000000;

        let mut command = Command::new("wmic");
        command.args(&["diskdrive", "get", "DeviceID,Model,SerialNumber,Status"]);
        command.creation_flags(CREATE_NO_WINDOW);
        
        let output = command.output().map_err(|e| format!("Failed to execute WMIC: {}", e))?;

        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).into_owned());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut disks = Vec::new();

        let lines: Vec<&str> = stdout.lines().filter(|l| !l.trim().is_empty()).collect();
        if lines.len() < 2 { return Ok(disks); }

        let header = lines[0];
        let device_id_pos = header.find("DeviceID").unwrap_or(0);
        let model_pos = header.find("Model").unwrap_or(0);
        let serial_pos = header.find("SerialNumber").unwrap_or(0);
        let status_pos = header.find("Status").unwrap_or(0);

        for line in lines.iter().skip(1) {
            let status_str = line[status_pos..].trim().to_string();
            let health_ok = if status_str.eq_ignore_ascii_case("ok") { 1 } else { 0 };

            disks.push(DiskHealth {
                name: line[device_id_pos..model_pos].trim().to_string(),
                model: line[model_pos..serial_pos].trim().to_string(),
                serial: line[serial_pos..status_pos].trim().to_string(),
                health_ok,
                raw_status: status_str,
            });
        }
        Ok(disks)
    }
}

#[cfg(target_os = "windows")]
struct WmiApiCollector;

#[cfg(target_os = "windows")]
use serde::Deserialize;
#[cfg(target_os = "windows")]
#[derive(Deserialize, Debug)]
#[serde(rename = "Win32_DiskDrive")]
#[serde(rename_all = "PascalCase")]
pub struct Win32DiskDrive {
    device_id: String,
    model: Option<String>,
    serial_number: Option<String>,
    status: Option<String>,
}

#[cfg(target_os = "windows")]
impl SmartCollector for WmiApiCollector {
    fn collect(&self) -> Result<Vec<DiskHealth>, String> {
        use wmi::{COMLibrary, WMIConnection};
        // Initialize the COM library for the current thread.
        let com_con = COMLibrary::new().map_err(|e| format!("Failed to initialize COM library: {}", e))?;
        
        // Create a connection to the local WMI namespace.
        let wmi_con = WMIConnection::new(com_con.into()).map_err(|e| format!("Failed to create WMI connection: {}", e))?;
        
        // Execute the WMI query and deserialize the results into our struct.
        let results: Vec<Win32DiskDrive> = wmi_con.query().map_err(|e| format!("Failed to query Win32_DiskDrive: {}", e))?;
        
        // Map the WMI results to our common `DiskHealth` struct.
        let disks = results
            .into_iter()
            .map(|drive| {
                let status_str = drive.status.unwrap_or_else(|| "N/A".to_string());
                let health_ok = if status_str.eq_ignore_ascii_case("ok") { 1 } else { 0 };

                DiskHealth {
                    name: drive.device_id,
                    model: drive.model.unwrap_or_else(|| "N/A".to_string()),
                    serial: drive.serial_number.unwrap_or_else(|| "N/A".to_string()).trim().to_string(),
                    health_ok,
                    raw_status: status_str,
                }
            })
            .collect();
        
        Ok(disks)
    }
}

// --- macOS Collector using diskutil ---
#[cfg(target_os = "macos")]
struct DiskutilCollector;

#[cfg(target_os = "macos")]
impl SmartCollector for DiskutilCollector {
    fn collect(&self) -> Result<Vec<DiskHealth>, String> {
        use std::process::Command;

        let list_output = Command::new("diskutil")
            .arg("list")
            .output()
            .map_err(|e| format!("Failed to execute 'diskutil list': {}", e))?;
        
        if !list_output.status.success() {
            return Err(String::from_utf8_lossy(&list_output.stderr).into_owned());
        }
        
        let list_stdout = String::from_utf8_lossy(&list_output.stdout);
        let mut device_ids = Vec::new();

        for line in list_stdout.lines() {
            if line.starts_with("/dev/disk") {
                if let Some(id) = line.split_whitespace().next() {
                    if !is_partition(id) { 
                        device_ids.push(id.to_string()); 
                    }
                }
            }
        }
        
        let mut disks = Vec::new();
        for id in device_ids {
            let info_output = Command::new("diskutil")
                .args(&["info", &id])
                .output()
                .map_err(|e| format!("Failed to execute 'diskutil info {}': {}", id, e))?;
            
            if !info_output.status.success() { continue; }

            let info_stdout = String::from_utf8_lossy(&info_output.stdout);
            let mut model = "N/A".to_string();
            let mut serial = "N/A".to_string();
            let mut status = "N/A".to_string();

            for line in info_stdout.lines() {
                if let Some((key, val)) = line.split_once(':') {
                    match key.trim() {
                        "Device / Media Name" | "Volume Name" => model = val.trim().to_string(),
                        "Serial Number" => serial = val.trim().to_string(),
                        "SMART Status" => status = val.trim().to_string(),
                        _ => {}
                    }
                }
            }
            let health_ok = if status.eq_ignore_ascii_case("verified") { 1 } else { 0 };
            
            disks.push(DiskHealth {
                name: id.replace("/dev/", ""),
                model, serial, health_ok, raw_status: status,
            });
        }
        Ok(disks)
    }
}

#[cfg(target_os = "macos")]
fn is_partition(device_id: &str) -> bool {
    // Find the part of the string after "disk"
    if let Some(after_disk) = device_id.strip_prefix("/dev/disk") {
        // Find the location of 's' which separates disk number from slice number
        if let Some(s_index) = after_disk.find('s') {
            // Get the part after 's'
            let slice_part = &after_disk[s_index + 1..];
            // If the part after 's' is not empty and contains only digits, it's a partition.
            return !slice_part.is_empty() && slice_part.chars().all(char::is_numeric);
        }
    }
    // If we can't find 's' in the right place, it's not a partition in the diskNsM format.
    false
}

// --- Fallback for unsupported OS ---
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
struct UnsupportedCollector;
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
impl SmartCollector for UnsupportedCollector {
    fn collect(&self) -> Result<Vec<DiskHealth>, String> {
        Err("This operating system is not supported.".to_string())
    }
}