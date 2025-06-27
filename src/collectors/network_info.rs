use pnet::datalink::{self, NetworkInterface};
use pnet::ipnetwork::IpNetwork;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct InterfaceInfo {
    pub name: String,
    pub mac_address: String,
    pub ips: Vec<IpNetwork>,
    pub interface_type: String,
    pub link_speed_mbps: Option<u64>,
    pub gateway: Option<String>,
    pub dns_servers: Vec<String>,
}

// OS-agnostic helper to determine type from pnet flags
fn get_interface_type(iface: &NetworkInterface) -> String {
    if iface.is_loopback() {
        "Loopback".to_string()
    } else if iface.is_up() && iface.mac.is_some() {
        if iface.name.to_lowercase().starts_with("w") { // A simple heuristic for WiFi
            "Wireless".to_string()
        } else {
            "Ethernet/Virtual".to_string()
        }
    } else {
        "Other/Down".to_string()
    }
}

// 主收集函数
pub fn collect_all_interface_info() -> Vec<InterfaceInfo> {
    let interfaces = datalink::interfaces();
    let gateways_and_dns = get_gateways_and_dns();

    interfaces.into_iter()
        .filter(|iface| !iface.is_loopback() && iface.is_up())
        .map(|iface| {
            let gateway_info = gateways_and_dns.get(&iface.name);
            
            InterfaceInfo {
                link_speed_mbps: get_link_speed(&iface.name),
                gateway: gateway_info.and_then(|g| g.gateway.clone()),
                dns_servers: gateway_info.map_or(vec![], |g| g.dns.clone()),
                name: iface.name.clone(),
                mac_address: iface.mac.map_or("N/A".to_string(), |m| m.to_string()),
                ips: iface.ips.clone(),
                interface_type: get_interface_type(&iface),
            }
        })
        .collect()
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
    use super::{GatewayInfo};
    use std::collections::HashMap;
    use std::ptr;
    use std::alloc::{Layout, alloc, dealloc};
    use windows_sys::Win32::Foundation::{NO_ERROR};
    use windows_sys::Win32::NetworkManagement::IpHelper::{GetAdaptersAddresses, IP_ADAPTER_ADDRESSES_LH, GAA_FLAG_INCLUDE_PREFIX, IP_ADAPTER_DNS_SERVER_ADDRESS_XP, IP_ADAPTER_GATEWAY_ADDRESS_LH};
    
    // Re-use wcslen from previous example
    unsafe fn wcslen(ptr: *const u16) -> usize {
        let mut len = 0;
        while *ptr.add(len) != 0 { len += 1; }
        len
    }

    pub fn get_link_speed(if_name: &str) -> Option<u64> {
        get_gateways_and_dns().get(if_name).and_then(|info| info.gateway.as_ref().and_then(|_| get_speed_from_winapi(if_name)))
    }
    
    // Helper to get speed, since we are already iterating adapters in the other function
    fn get_speed_from_winapi(if_name: &str) -> Option<u64> {
         // This is a simplified version. A full implementation would reuse the GetAdaptersAddresses logic.
         // For brevity, we are calling it again here.
         let mut size = 0;
         unsafe { GetAdaptersAddresses(0, 0, ptr::null_mut(), ptr::null_mut(), &mut size); }
         if size == 0 { return None; }
         let layout = Layout::from_size_align(size as usize, 1).unwrap();
         let buffer = unsafe { alloc(layout) as *mut IP_ADAPTER_ADDRESSES_LH };
         if unsafe { GetAdaptersAddresses(0, 0, ptr::null_mut(), buffer, &mut size) } != NO_ERROR {
             unsafe { dealloc(buffer as *mut u8, layout) };
             return None;
         }
 
         let mut current = buffer;
         while !current.is_null() {
             unsafe {
                 let name = String::from_utf16_lossy(std::slice::from_raw_parts((*current).FriendlyName, wcslen((*current).FriendlyName)));
                 if name == if_name {
                     let speed = (*current).ReceiveLinkSpeed / 1_000_000;
                     dealloc(buffer as *mut u8, layout);
                     return Some(speed);
                 }
                 current = (*current).Next;
             }
         }
         unsafe { dealloc(buffer as *mut u8, layout) };
         None
    }


    pub fn get_gateways_and_dns() -> HashMap<String, GatewayInfo> {
        let mut result = HashMap::new();
        let mut size = 0;
        
        unsafe { GetAdaptersAddresses(0, GAA_FLAG_INCLUDE_PREFIX, ptr::null_mut(), ptr::null_mut(), &mut size); }

        if size == 0 { return result; }
        
        let layout = Layout::from_size_align(size as usize, 1).unwrap();
        let buffer = unsafe { alloc(layout) as *mut IP_ADAPTER_ADDRESSES_LH };

        if unsafe { GetAdaptersAddresses(0, GAA_FLAG_INCLUDE_PREFIX, ptr::null_mut(), buffer, &mut size) } == NO_ERROR {
            let mut current = buffer;
            while !current.is_null() {
                unsafe {
                    let name = String::from_utf16_lossy(std::slice::from_raw_parts((*current).FriendlyName, wcslen((*current).FriendlyName)));
                    let mut info = GatewayInfo::default();
                    
                    // Gateway
                    let mut gateway_ptr = (*current).FirstGatewayAddress;
                    if !gateway_ptr.is_null() {
                        let sockaddr = (*gateway_ptr).Address.lpSockaddr;
                        if (*sockaddr).sa_family == 2 { // AF_INET (IPv4)
                            let sin_addr = &(*(sockaddr as *const std::net::SocketAddrIn)).sin_addr;
                            info.gateway = Some(std::net::Ipv4Addr::from(sin_addr.s_addr.to_ne_bytes()).to_string());
                        }
                    }
                    
                    // DNS
                    let mut dns_ptr = (*current).FirstDnsServerAddress;
                    while !dns_ptr.is_null() {
                        let sockaddr = (*dns_ptr).Address.lpSockaddr;
                        if (*sockaddr).sa_family == 2 { // AF_INET (IPv4)
                            let sin_addr = &(*(sockaddr as *const std::net::SocketAddrIn)).sin_addr;
                            info.dns.push(std::net::Ipv4Addr::from(sin_addr.s_addr.to_ne_bytes()).to_string());
                        }
                        dns_ptr = (*dns_ptr).Next;
                    }
                    
                    result.insert(name, info);
                    current = (*current).Next;
                }
            }
        }

        unsafe { dealloc(buffer as *mut u8, layout); }
        result
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


// Expose the platform-specific functions
use platform::{get_link_speed, get_gateways_and_dns};