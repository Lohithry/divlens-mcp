// ==================================================================================
// 🧠 DIVLENS STATIC NETWORK CONFIG COLLECTOR
// ==================================================================================
// This module handles collection of static network configuration:
// - DNS servers (where DNS queries go)
// - Routing table (how packets are routed)
//
// DESIGN PRINCIPLES:
// 1. Static Data: This changes rarely, so we collect it once or on-demand
// 2. Cross-Platform: Uses native OS commands for accuracy
// 3. Robust Parsing: Handles different OS output formats gracefully
// ==================================================================================

use std::process::Command;
use crate::models::network_schema::RouteDetail;

/// Gets DNS server configuration from the system
/// Returns a list of DNS server IP addresses
pub fn get_dns_servers() -> Vec<String> {
    #[cfg(target_os = "windows")]
    {
        get_dns_servers_windows()
    }
    
    #[cfg(target_os = "macos")]
    {
        get_dns_servers_macos()
    }
    
    #[cfg(target_os = "linux")]
    {
        get_dns_servers_linux()
    }
    
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        // Fallback: Try to read resolv.conf (Unix-like systems)
        get_dns_servers_resolv_conf()
    }
}

/// Gets routing table from the system
/// Returns a list of route entries
pub fn get_routing_table() -> Vec<RouteDetail> {
    #[cfg(target_os = "windows")]
    {
        get_routing_table_windows()
    }
    
    #[cfg(target_os = "macos")]
    {
        get_routing_table_macos()
    }
    
    #[cfg(target_os = "linux")]
    {
        get_routing_table_linux()
    }
    
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        // Fallback: Try netstat -rn (Unix-like systems)
        get_routing_table_netstat()
    }
}

// ==================================================================================
// DNS SERVER COLLECTION (Platform-Specific)
// ==================================================================================

#[cfg(target_os = "windows")]
fn get_dns_servers_windows() -> Vec<String> {
    // Windows: Parse `ipconfig /all` output
    // This is the most reliable method on Windows
    let output = match Command::new("ipconfig").arg("/all").output() {
        Ok(o) => o,
        Err(_) => return Vec::new(),
    };
    
    let out = String::from_utf8_lossy(&output.stdout);
    let mut servers = Vec::new();
    let mut in_dns_section = false;
    
    for line in out.lines() {
        let line = line.trim();
        
        // Look for "DNS Servers" section
        if line.contains("DNS Servers") {
            in_dns_section = true;
            // Extract IP from the same line if present
            if let Some(ip) = extract_ip_from_line(line) {
                servers.push(ip);
            }
            continue;
        }
        
        // If we're in DNS section, look for IP addresses
        if in_dns_section {
            if let Some(ip) = extract_ip_from_line(line) {
                servers.push(ip);
            } else if line.is_empty() || line.starts_with("Ethernet") || line.starts_with("Wireless") {
                // End of DNS section
                in_dns_section = false;
            }
        }
    }
    
    servers
}

#[cfg(target_os = "macos")]
fn get_dns_servers_macos() -> Vec<String> {
    // macOS: Use `scutil --dns` or read resolv.conf
    // scutil is more reliable on macOS
    let output = match Command::new("scutil").arg("--dns").output() {
        Ok(o) => o,
        Err(_) => return get_dns_servers_resolv_conf(),
    };
    
    let out = String::from_utf8_lossy(&output.stdout);
    let mut servers = Vec::new();
    
    for line in out.lines() {
        let line = line.trim();
        // Format: "nameserver[0] : 8.8.8.8"
        if line.contains("nameserver") && line.contains(":") {
            if let Some(ip) = line.split(':').nth(1) {
                let ip = ip.trim();
                if is_valid_ip(ip) {
                    servers.push(ip.to_string());
                }
            }
        }
    }
    
    if servers.is_empty() {
        // Fallback to resolv.conf
        return get_dns_servers_resolv_conf();
    }
    
    servers
}

#[cfg(target_os = "linux")]
fn get_dns_servers_linux() -> Vec<String> {
    // Linux: Try systemd-resolve first (modern systems)
    if let Ok(output) = Command::new("systemd-resolve").arg("--status").output() {
        let out = String::from_utf8_lossy(&output.stdout);
        let mut servers = Vec::new();
        
        for line in out.lines() {
            let line = line.trim();
            // Format: "DNS Servers: 8.8.8.8"
            if line.starts_with("DNS Servers:") {
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() > 1 {
                    for ip in parts[1].split_whitespace() {
                        if is_valid_ip(ip) {
                            servers.push(ip.to_string());
                        }
                    }
                }
            }
        }
        
        if !servers.is_empty() {
            return servers;
        }
    }
    
    // Fallback to resolv.conf (works on all Linux systems)
    get_dns_servers_resolv_conf()
}

fn get_dns_servers_resolv_conf() -> Vec<String> {
    // Unix-like: Read /etc/resolv.conf
    // This is the standard location for DNS configuration
    match std::fs::read_to_string("/etc/resolv.conf") {
        Ok(content) => {
            content
                .lines()
                .filter_map(|line| {
                    let line = line.trim();
                    if line.starts_with("nameserver") {
                        // Format: "nameserver 8.8.8.8"
                        let parts: Vec<&str> = line.split_whitespace().collect();
                        if parts.len() > 1 {
                            let ip = parts[1];
                            if is_valid_ip(ip) {
                                return Some(ip.to_string());
                            }
                        }
                    }
                    None
                })
                .collect()
        }
        Err(_) => Vec::new(),
    }
}

// ==================================================================================
// ROUTING TABLE COLLECTION (Platform-Specific)
// ==================================================================================

#[cfg(target_os = "windows")]
fn get_routing_table_windows() -> Vec<RouteDetail> {
    // Windows: Use `route print` or `netstat -rn`
    let output = match Command::new("route").args(&["print", "-4"]).output() {
        Ok(o) => o,
        Err(_) => return Vec::new(),
    };
    
    let out = String::from_utf8_lossy(&output.stdout);
    let mut routes = Vec::new();
    let mut in_table = false;
    
    for line in out.lines() {
        let line = line.trim();
        
        // Skip header lines
        if line.starts_with("Network Destination") {
            in_table = true;
            continue;
        }
        
        if in_table && !line.is_empty() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                let destination = parts[0].to_string();
                let gateway = parts[2].to_string();
                let interface = if parts.len() > 3 {
                    parts[parts.len() - 1].to_string()
                } else {
                    "Unknown".to_string()
                };
                
                routes.push(RouteDetail {
                    destination,
                    gateway,
                    interface,
                });
            }
        }
    }
    
    routes
}

#[cfg(target_os = "macos")]
fn get_routing_table_macos() -> Vec<RouteDetail> {
    // macOS: Use `netstat -rn`
    get_routing_table_netstat()
}

#[cfg(target_os = "linux")]
fn get_routing_table_linux() -> Vec<RouteDetail> {
    // Linux: Use `ip route` (modern) or `netstat -rn` (fallback)
    let output = match Command::new("ip").arg("route").output() {
        Ok(o) => o,
        Err(_) => return get_routing_table_netstat(),
    };
    
    let out = String::from_utf8_lossy(&output.stdout);
    let mut routes = Vec::new();
    
    for line in out.lines() {
        let line = line.trim();
        // Format: "default via 192.168.1.1 dev en0"
        // or "192.168.1.0/24 dev en0 proto kernel scope link src 192.168.1.100"
        
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }
        
        let destination = if parts[0] == "default" {
            "0.0.0.0".to_string()
        } else {
            parts[0].to_string()
        };
        
        let mut gateway = "0.0.0.0".to_string();
        let mut interface = "Unknown".to_string();
        
        // Parse "via <gateway>" and "dev <interface>"
        for i in 0..parts.len() {
            if parts[i] == "via" && i + 1 < parts.len() {
                gateway = parts[i + 1].to_string();
            } else if parts[i] == "dev" && i + 1 < parts.len() {
                interface = parts[i + 1].to_string();
            }
        }
        
        routes.push(RouteDetail {
            destination,
            gateway,
            interface,
        });
    }
    
    routes
}

fn get_routing_table_netstat() -> Vec<RouteDetail> {
    // Fallback: Use `netstat -rn` (works on most Unix-like systems)
    let output = match Command::new("netstat").args(&["-rn"]).output() {
        Ok(o) => o,
        Err(_) => return Vec::new(),
    };
    
    let out = String::from_utf8_lossy(&output.stdout);
    let mut routes = Vec::new();
    let mut in_table = false;
    
    for line in out.lines() {
        let line = line.trim();
        
        // Skip header
        if line.starts_with("Destination") || line.starts_with("Routing tables") {
            in_table = true;
            continue;
        }
        
        if in_table && !line.is_empty() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                routes.push(RouteDetail {
                    destination: parts[0].to_string(),
                    gateway: parts[1].to_string(),
                    interface: parts[parts.len() - 1].to_string(),
                });
            }
        }
    }
    
    routes
}

// ==================================================================================
// HELPER FUNCTIONS
// ==================================================================================

/// Extracts an IP address from a line of text
fn extract_ip_from_line(line: &str) -> Option<String> {
    // Simple regex-like extraction: look for IPv4 pattern
    // This is a basic implementation - a proper regex would be better
    for word in line.split_whitespace() {
        if is_valid_ip(word) {
            return Some(word.to_string());
        }
    }
    None
}

/// Checks if a string is a valid IP address (IPv4)
fn is_valid_ip(ip: &str) -> bool {
    let parts: Vec<&str> = ip.split('.').collect();
    if parts.len() != 4 {
        return false;
    }
    
    for part in parts {
        match part.parse::<u8>() {
            Ok(_) => continue,
            Err(_) => return false,
        }
    }
    
    true
}

