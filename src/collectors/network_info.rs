use network_interface::{NetworkInterface, NetworkInterfaceConfig};

#[derive(Debug, Clone)]
pub struct InterfaceInfo {
    pub name: String,
    pub mac_address: String,
    pub ips: Vec<String>,
    pub interface_type: String,
    pub link_speed_mbps: Option<u64>,
    pub gateway: Option<String>,
    pub dns_servers: Vec<String>,
}

pub fn collect_all_interface_info() -> Vec<InterfaceInfo> {
    let mut results: Vec<InterfaceInfo> = Vec::new();

    // --- Windows Specific Logic ---
    #[cfg(target_os = "windows")]
    let windows_details = platform::get_all_windows_details();

    // --- Logic for other OSes ---
    #[cfg(not(target_os = "windows"))]
    let gateways_and_dns = get_gateways_and_dns();

    if let Ok(interfaces) = NetworkInterface::show() {
        for iface in interfaces {
            if iface.addr.is_empty() || iface.mac_addr.is_none() || iface.name.starts_with("lo") {
                continue;
            }
            let mac_address = iface.mac_addr.clone().unwrap();

            let entry = results.iter_mut().find(|i| i.name == iface.name);

            if let Some(existing_entry) = entry {
                for addr in &iface.addr {
                    existing_entry.ips.push(addr.ip().to_string());
                }
            } else {
                // Create a new entry, fetching platform-specific details differently for Windows
                let mut new_entry = InterfaceInfo {
                    name: iface.name.clone(),
                    mac_address: mac_address.clone(),
                    ips: iface.addr.iter().map(|addr| addr.ip().to_string()).collect(),
                    interface_type: if iface.name.starts_with("en") || iface.name.starts_with("eth") {
                        "Ethernet".to_string()
                    } else if iface.name.starts_with("wl") {
                        "Wireless".to_string()
                    } else {
                        "Other".to_string()
                    },
                    // Initialize with None/empty, will be filled below
                    link_speed_mbps: None,
                    gateway: None,
                    dns_servers: vec![],
                };

                // --- Fill in platform-specific details ---
                #[cfg(target_os = "windows")]
                {
                    if let Some(details) = windows_details.get(&mac_address) {
                        new_entry.link_speed_mbps = details.link_speed_mbps;
                        new_entry.gateway = details.gateway.clone();
                        new_entry.dns_servers = details.dns.clone();
                    }
                }
                #[cfg(not(target_os = "windows"))]
                {
                    new_entry.link_speed_mbps = get_link_speed(&iface.name);
                    if let Some(gw_info) = gateways_and_dns.get(&iface.name) {
                        new_entry.gateway = gw_info.gateway.clone();
                        new_entry.dns_servers = gw_info.dns.clone();
                    }
                }
                results.push(new_entry);
            }
        }
    }
    
    // Post-processing
    for entry in &mut results {
        entry.ips.sort();
        entry.ips.dedup();
    }
    results
}


// =======================================================
// Platform-specific Implementations
// =======================================================

#[derive(Clone, Debug, Default)]
struct GatewayInfo {
    gateway: Option<String>,
    dns: Vec<String>,
}

// === LINUX ===
#[cfg(target_os = "linux")]
mod platform {
    use super::{GatewayInfo};
    use std::collections::HashMap;
    use std::fs;

    pub fn get_link_speed(if_name: &str) -> Option<u64> {
        fs::read_to_string(format!("/sys/class/net/{}/speed", if_name))
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
    }

    pub fn get_gateways_and_dns() -> HashMap<String, GatewayInfo> {
        let mut result = HashMap::new();
        let dns_servers = fs::read_to_string("/etc/resolv.conf").ok().map_or(vec![], |content| {
            content.lines()
                .filter(|line| line.starts_with("nameserver"))
                .filter_map(|line| line.split_whitespace().nth(1).map(String::from))
                .collect()
        });

        if let Ok(content) = fs::read_to_string("/proc/net/route") {
            for line in content.lines().skip(1) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() > 2 && parts[1] == "00000000" { // Default gateway
                    let iface_name = parts[0].to_string();
                    let gateway_ip = u32::from_str_radix(parts[2], 16).ok()
                        .map(|n| std::net::Ipv4Addr::from(n.to_le_bytes()).to_string());
                    
                    let mut info = result.entry(iface_name).or_insert_with(GatewayInfo::default);
                    info.gateway = gateway_ip;
                    info.dns = dns_servers.clone(); // Assume same DNS for all
                }
            }
        }
        result
    }
}

// === WINDOWS ===
#[cfg(target_os = "windows")]
mod platform {
    use std::collections::HashMap;
    use std::ptr;
    use std::alloc::{Layout, alloc, dealloc};
    use windows_sys::Win32::Foundation::NO_ERROR;
    use windows_sys::Win32::Networking::WinSock::SOCKADDR_IN;
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        GetAdaptersAddresses, IP_ADAPTER_ADDRESSES_LH, GAA_FLAG_INCLUDE_PREFIX, GAA_FLAG_INCLUDE_GATEWAYS
    };
    use windows_sys::Win32::NetworkManagement::Ndis::IfOperStatusUp;

    // A new struct to hold all detailed info fetched from the Win32 API
    #[derive(Clone, Debug, Default)]
    pub struct WindowsInterfaceDetails {
        pub link_speed_mbps: Option<u64>,
        pub gateway: Option<String>,
        pub dns: Vec<String>,
    }

    // Helper function to convert a MAC address byte slice to a formatted string
    fn format_mac_address(mac_slice: &[u8]) -> String {
        mac_slice.iter()
            .map(|byte| format!("{:02X}", byte))
            .collect::<Vec<String>>()
            .join(":")
    }

    // The main function that fetches all details at once and returns a map keyed by MAC address.
    pub fn get_all_windows_details() -> HashMap<String, WindowsInterfaceDetails> {
        let mut details_map = HashMap::new();
        let mut buffer_size: u32 = 0;
        
        // Flags to get all necessary information
        let flags = GAA_FLAG_INCLUDE_PREFIX | GAA_FLAG_INCLUDE_GATEWAYS;

        // First call to get the required buffer size
        unsafe {
            GetAdaptersAddresses(0, flags, ptr::null_mut(), ptr::null_mut(), &mut buffer_size);
        }

        if buffer_size == 0 {
            return details_map;
        }

        let layout = Layout::from_size_align(buffer_size as usize, 1).unwrap();
        let buffer = unsafe { alloc(layout) as *mut IP_ADAPTER_ADDRESSES_LH };

        if buffer.is_null() {
            return details_map;
        }

        // Second call to get the actual data
        if unsafe { GetAdaptersAddresses(0, flags, ptr::null_mut(), buffer, &mut buffer_size) } == NO_ERROR {
            let mut current_adapter = buffer;
            while !current_adapter.is_null() {
                unsafe {
                    // Only process active adapters with a valid MAC address
                    if (*current_adapter).OperStatus == IfOperStatusUp && (*current_adapter).PhysicalAddressLength > 0 {
                        
                        // Get the MAC address and format it as a string key
                        let mac_slice = std::slice::from_raw_parts(
                            (*current_adapter).PhysicalAddress.as_ptr(),
                            (*current_adapter).PhysicalAddressLength as usize
                        );
                        let mac_key = format_mac_address(mac_slice);

                        let mut details = WindowsInterfaceDetails::default();

                        // --- Get Link Speed ---
                        // ReceiveLinkSpeed is in bits per second. Convert to Mbps.
                        let speed_bps = (*current_adapter).ReceiveLinkSpeed;
                        if speed_bps > 0 {
                            details.link_speed_mbps = Some(speed_bps / 1_000_000);
                        }

                        // --- Get Gateway ---
                        let gateway_ptr = (*current_adapter).FirstGatewayAddress;
                        if !gateway_ptr.is_null() {
                            let sockaddr = (*gateway_ptr).Address.lpSockaddr;
                            if !sockaddr.is_null() && (*sockaddr).sa_family == 2 { // AF_INET (IPv4)
                                let sockaddr_in = sockaddr as *const SOCKADDR_IN;
                                let s_addr = (*sockaddr_in).sin_addr.S_un.S_addr;
                                details.gateway = Some(std::net::Ipv4Addr::from(s_addr.to_le_bytes()).to_string());
                            }
                        }

                        // --- Get DNS Servers ---
                        let mut dns_ptr = (*current_adapter).FirstDnsServerAddress;
                        while !dns_ptr.is_null() {
                            let sockaddr = (*dns_ptr).Address.lpSockaddr;
                            if !sockaddr.is_null() && (*sockaddr).sa_family == 2 { // AF_INET (IPv4)
                                let sockaddr_in = sockaddr as *const SOCKADDR_IN;
                                let s_addr = (*sockaddr_in).sin_addr.S_un.S_addr;
                                details.dns.push(std::net::Ipv4Addr::from(s_addr.to_le_bytes()).to_string());
                            }
                            dns_ptr = (*dns_ptr).Next;
                        }
                        
                        details_map.insert(mac_key, details);
                    }
                    current_adapter = (*current_adapter).Next;
                }
            }
        }

        unsafe { dealloc(buffer as *mut u8, layout) };
        details_map
    }
}

// === MACOS (and fallback for other Unix) ===
#[cfg(not(any(target_os = "linux", target_os = "windows")))]
mod platform {
    // macOS implementation is complex. For this example, we'll provide placeholders.
    // A real implementation would use `sysctl` or parse `netstat -nr` and `scutil --dns`.
    use super::GatewayInfo;
    use std::collections::HashMap;

    pub fn get_link_speed(_if_name: &str) -> Option<u64> { None } // Placeholder
    pub fn get_gateways_and_dns() -> HashMap<String, GatewayInfo> { HashMap::new() } // Placeholder
}

#[cfg(not(target_os = "windows"))]
// Expose the platform-specific functions
use platform::{get_link_speed, get_gateways_and_dns};