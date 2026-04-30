// ==================================================================================
// 🧠 DIVLENS VOLATILE NETWORK METRICS COLLECTOR
// ==================================================================================
// This module handles real-time network interface monitoring with signal strength.
// It runs frequently (every 1 second) to capture live traffic and Wi-Fi signal.
//
// DESIGN PRINCIPLES:
// 1. Cross-Platform: Uses native OS commands for accurate signal strength
// 2. Performance: Fast execution, filters out noise (loopback, Docker bridges)
// 3. Accuracy: Real data from OS APIs, not generic library approximations
// ==================================================================================

use sysinfo::{System, Networks};
use get_if_addrs::{get_if_addrs, IfAddr};
use crate::models::network_schema::InterfaceDetail;

/// Scans all network interfaces and returns detailed information
/// including traffic metrics, IP addresses, and Wi-Fi signal strength
/// 
/// # Arguments
/// * `sys` - Reference to the System object (must have networks refreshed)
/// * `networks` - Reference to the Networks object (must be refreshed)
/// 
/// # Returns
/// Vector of InterfaceDetail structs, one per active interface
pub fn scan_interfaces(_sys: &System, networks: &Networks) -> Vec<InterfaceDetail> {
    let mut details = Vec::new();

    // Get IP addresses for all interfaces using get_if_addrs
    let if_addrs = get_if_addrs().unwrap_or_default();
    
    // Create a map of interface name -> IP addresses for quick lookup
    let mut ipv4_map: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    let mut ipv6_map: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    
    for iface in if_addrs {
        let name = iface.name.clone();
        match iface.addr {
            IfAddr::V4(ipv4) => {
                ipv4_map.entry(name).or_insert_with(Vec::new).push(ipv4.ip.to_string());
            }
            IfAddr::V6(ipv6) => {
                ipv6_map.entry(name).or_insert_with(Vec::new).push(ipv6.ip.to_string());
            }
        }
    }

    // Iterate through all network interfaces from sysinfo
    for (name, data) in networks {
        // FILTER: Skip noise (Localhost, Docker internal bridges, virtual interfaces)
        // These interfaces don't provide useful information for network monitoring
        if name == "lo" || 
           name == "lo0" || 
           name.starts_with("veth") || 
           name.starts_with("br-") ||
           name.starts_with("docker") ||
           name.starts_with("virbr") {
            continue;
        }

        // Determine if this is a Wi-Fi interface
        // Different OSes use different naming conventions
        let name_lower = name.to_lowercase();
        let is_wifi = name_lower.contains("wi-fi") || 
                     name_lower.contains("wlan") || 
                     name_lower.contains("wifi") ||
                     (cfg!(target_os = "macos") && name.starts_with("en") && name != "en0" && !name.contains("bridge")) ||
                     (cfg!(target_os = "linux") && name.starts_with("wlp") || name.starts_with("wlan"));

        // Get signal strength for Wi-Fi interfaces
        let signal = if is_wifi { 
            get_wifi_dbm(name) 
        } else { 
            None 
        };

        // Determine connection type
        let connection_type = if is_wifi {
            "Wi-Fi".to_string()
        } else if name_lower.contains("eth") || name_lower.contains("ethernet") {
            "Ethernet".to_string()
        } else if name_lower.contains("bridge") || name_lower.contains("vmnet") {
            "Virtual".to_string()
        } else {
            "Other".to_string()
        };

        // Get IP addresses for this interface
        let ipv4_addresses = ipv4_map.get(name).cloned().unwrap_or_default();
        let ipv6_addresses = ipv6_map.get(name).cloned().unwrap_or_default();

        // Get MAC address from sysinfo data
        let mac_address = data.mac_address().to_string();

        details.push(InterfaceDetail {
            name: name.clone(),
            mac_address,
            ipv4_addresses,
            ipv6_addresses,
            connection_type,
            rx_bytes_sec: data.received(),  // Bytes received since last refresh
            tx_bytes_sec: data.transmitted(), // Bytes transmitted since last refresh
            signal_strength_dbm: signal,
            signal_quality: rate_signal(signal),
        });
    }
    
    details
}

// ==================================================================================
// CROSS-PLATFORM SIGNAL STRENGTH DETECTION
// ==================================================================================
// Uses native OS commands for maximum accuracy:
// - macOS: airport command (Apple's private framework)
// - Linux: /proc/net/wireless (kernel interface)
// - Windows: netsh wlan (Windows native tool)
// ==================================================================================

/// Gets Wi-Fi signal strength in dBm for a given interface
/// Returns None if signal strength cannot be determined
fn get_wifi_dbm(iface: &str) -> Option<i32> {
    #[cfg(target_os = "macos")]
    {
        // macOS: Parse 'airport -I' output
        // The airport command is in a private framework but is available on all Macs
        let output = std::process::Command::new("/System/Library/PrivateFrameworks/Apple80211.framework/Versions/Current/Resources/airport")
            .arg("-I")
            .output()
            .ok()?;
        
        let out = String::from_utf8_lossy(&output.stdout);
        
        // Look for the RSSI (Received Signal Strength Indicator) line
        // Format: "agrCtlRSSI: -45" or "RSSI: -45"
        for line in out.lines() {
            let line = line.trim();
            if line.starts_with("agrCtlRSSI:") {
                // Extract the number after the colon
                if let Some(value_str) = line.split(':').nth(1) {
                    return value_str.trim().parse::<i32>().ok();
                }
            } else if line.starts_with("RSSI:") {
                if let Some(value_str) = line.split(':').nth(1) {
                    return value_str.trim().parse::<i32>().ok();
                }
            }
        }
        
        // Suppress unused variable warning on macOS
        let _ = iface;
    }
    
    #[cfg(target_os = "linux")]
    {
        // Linux: Parse /proc/net/wireless
        // Format: "wlan0: 0000   42.  -45.      0"
        // Columns: interface, status, link quality, level (dBm), noise level
        if let Ok(content) = std::fs::read_to_string("/proc/net/wireless") {
            for line in content.lines().skip(2) { // Skip header lines
                if line.contains(iface) {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    // Level is typically in the 3rd or 4th column (index 2 or 3)
                    // Format: "-45." (with decimal point)
                    if let Some(level_str) = parts.get(3) {
                        // Remove the decimal point and parse
                        let cleaned = level_str.trim_matches('.');
                        if let Ok(dbm) = cleaned.parse::<i32>() {
                            return Some(dbm);
                        }
                    }
                }
            }
        }
    }
    
    #[cfg(target_os = "windows")]
    {
        // Windows: Use 'netsh wlan show interfaces'
        // This is the most reliable method on Windows
        let output = std::process::Command::new("netsh")
            .args(&["wlan", "show", "interfaces"])
            .output()
            .ok()?;
        
        let out = String::from_utf8_lossy(&output.stdout);
        
        // Windows gives signal as percentage (0-100%)
        // We convert to dBm using a rough approximation:
        // dBm = -100 + (Signal% / 2)
        // This gives: 100% = -50dBm, 50% = -75dBm, 0% = -100dBm
        for line in out.lines() {
            let line = line.trim();
            if line.starts_with("Signal") || line.starts_with("Signal quality") {
                // Format: "Signal             : 75%"
                if let Some(pct_str) = line.split(':').nth(1) {
                    let cleaned = pct_str.trim().trim_matches('%');
                    if let Ok(pct) = cleaned.parse::<i32>() {
                        // Convert percentage to dBm
                        // More accurate formula: -100 + (pct / 2)
                        return Some(-100 + (pct / 2));
                    }
                }
            }
        }
        
        // Suppress unused variable warning on Windows
        let _ = iface;
    }
    
    None
}

/// Converts dBm signal strength to human-readable quality rating
/// 
/// # Signal Strength Guidelines:
/// - Excellent: > -50 dBm (Very close to router)
/// - Good: -50 to -60 dBm (Close to router)
/// - Fair: -60 to -70 dBm (Moderate distance)
/// - Weak: -70 to -80 dBm (Far from router, may have issues)
/// - Very Weak: < -80 dBm (Very far, likely connection problems)
fn rate_signal(dbm: Option<i32>) -> String {
    match dbm {
        Some(s) if s > -50 => "Excellent".to_string(),
        Some(s) if s > -60 => "Good".to_string(),
        Some(s) if s > -70 => "Fair".to_string(),
        Some(s) if s > -80 => "Weak".to_string(),
        Some(_s) => "Very Weak".to_string(),
        None => "N/A".to_string(),
    }
}

