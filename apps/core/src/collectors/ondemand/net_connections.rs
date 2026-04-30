// ==================================================================================
// 🧠 DIVLENS LAZY NETWORK CONNECTIONS COLLECTOR
// ==================================================================================
// This module handles on-demand scanning of active network connections.
// It maps ports to processes - the "standout" feature that answers:
// "Who is using port 443?" -> "Google Chrome"
//
// DESIGN PRINCIPLES:
// 1. Lazy Loading: Only runs when user clicks "Connections" or AI asks
// 2. Process Attribution: Maps every connection to its owning process
// 3. Performance: Returns top 100 connections to prevent UI overload
// ==================================================================================

use sysinfo::System;
use crate::models::network_schema::ConnectionDetail;
use netstat2::{get_sockets_info, AddressFamilyFlags, ProtocolFlags, ProtocolSocketInfo};

/// Scans all active network connections and maps them to processes
/// This is the "standout" feature - it tells you which app is using which port
/// 
/// # Arguments
/// * `sys` - Reference to the System object (must have processes refreshed)
/// 
/// # Returns
/// Vector of ConnectionDetail structs, one per active connection
pub fn scan_connections(sys: &System) -> Vec<ConnectionDetail> {
    // Request both IPv4 and IPv6 connections
    let af = AddressFamilyFlags::IPV4 | AddressFamilyFlags::IPV6;
    
    // Request both TCP and UDP connections
    let proto = ProtocolFlags::TCP | ProtocolFlags::UDP;
    
    // Get socket information from the OS
    // This uses native OS APIs (netstat on Unix, WMI on Windows)
    let sockets = match get_sockets_info(af, proto) {
        Ok(s) => {
            tracing::debug!("Successfully retrieved {} sockets from OS", s.len());
            s
        },
        Err(e) => {
            // Log error but return empty vector instead of crashing
            // This can happen due to permissions issues or if the system call fails
            tracing::warn!("Failed to get socket info: {}. This might be a permissions issue.", e);
            eprintln!("Failed to get socket info: {}", e);
            return Vec::new();
        }
    };
    
    let mut conns = Vec::new();

    for socket_info in sockets {
        // 1. Get PID & Process Name
        // netstat2 provides associated PIDs for each socket
        let pid = socket_info.associated_pids.first().cloned().unwrap_or(0);
        
        let process_name = if pid > 0 {
            // Look up the process name using sysinfo
            // Note: sysinfo uses usize for PIDs, so we need to convert
            sys.process(sysinfo::Pid::from(pid as usize))
                .map(|p| {
                    // Get the process name, handling different OS conventions
                    // name() returns OsStr, so we convert to String
                    p.name().to_string_lossy().to_string()
                })
                .unwrap_or_else(|| "Unknown".to_string())
        } else {
            // PID 0 or no PID means kernel/system-owned connection
            "System/Kernel".to_string()
        };

        // 2. Determine App Category (AI Helper)
        // This helps the AI and users understand what type of app is making the connection
        let category = categorize_process(&process_name);

        // 3. Extract connection details based on protocol
        match socket_info.protocol_socket_info {
            ProtocolSocketInfo::Tcp(tcp_info) => {
                // Convert TcpState enum to a clean string representation
                // netstat2 uses TcpState enum, format!("{:?}") produces "TcpState::Established"
                // We extract just the state name for cleaner display
                let state_str = format!("{:?}", tcp_info.state);
                let clean_state = if state_str.contains("::") {
                    // Extract part after "::" (e.g., "Established" from "TcpState::Established")
                    state_str.split("::").last().unwrap_or(&state_str).to_string()
                } else {
                    state_str
                };
                
                conns.push(ConnectionDetail {
                    protocol: "TCP".to_string(),
                    local_ip: tcp_info.local_addr.to_string(),
                    local_port: tcp_info.local_port,
                    remote_ip: tcp_info.remote_addr.to_string(),
                    remote_port: tcp_info.remote_port,
                    state: clean_state,
                    pid,
                    process_name,
                    app_category: category,
                });
            }
            ProtocolSocketInfo::Udp(udp_info) => {
                // UDP doesn't have a "state" like TCP, so we use "N/A"
                conns.push(ConnectionDetail {
                    protocol: "UDP".to_string(),
                    local_ip: udp_info.local_addr.to_string(),
                    local_port: udp_info.local_port,
                    remote_ip: "0.0.0.0".to_string(), // UDP is connectionless
                    remote_port: 0,
                    state: "N/A".to_string(),
                    pid,
                    process_name,
                    app_category: category,
                });
            }
        }
    }
    
    // Optimization: Return only top 100 interesting connections if needed
    // For now, we return all connections. The UI can paginate if needed.
    // Sort by local port for easier reading
    conns.sort_by_key(|c| c.local_port);
    
    tracing::debug!("Processed {} network connections ({} TCP, {} UDP)", 
        conns.len(),
        conns.iter().filter(|c| c.protocol == "TCP").count(),
        conns.iter().filter(|c| c.protocol == "UDP").count()
    );
    
    conns
}

/// Categorizes a process name into a user-friendly category
/// This helps both the AI and users understand what type of application
/// is making network connections
fn categorize_process(process_name: &str) -> String {
    let name_lower = process_name.to_lowercase();
    
    // Web Browsers
    if name_lower.contains("chrome") || 
       name_lower.contains("firefox") || 
       name_lower.contains("msedge") ||
       name_lower.contains("safari") ||
       name_lower.contains("opera") ||
       name_lower.contains("brave") {
        return "Web Browser".to_string();
    }
    
    // Media Applications
    if name_lower.contains("spotify") ||
       name_lower.contains("discord") ||
       name_lower.contains("slack") ||
       name_lower.contains("zoom") ||
       name_lower.contains("teams") ||
       name_lower.contains("vlc") ||
       name_lower.contains("itunes") {
        return "Media".to_string();
    }
    
    // Developer Tools
    if name_lower.contains("postgres") ||
       name_lower.contains("mysql") ||
       name_lower.contains("docker") ||
       name_lower.contains("node") ||
       name_lower.contains("python") ||
       name_lower.contains("java") ||
       name_lower.contains("code") ||
       name_lower.contains("git") ||
       name_lower.contains("ssh") {
        return "Developer Tool".to_string();
    }
    
    // Database Servers
    if name_lower.contains("mongod") ||
       name_lower.contains("redis") ||
       name_lower.contains("sql") ||
       name_lower.contains("cassandra") {
        return "Database".to_string();
    }
    
    // System Processes
    if name_lower.contains("system") ||
       name_lower.contains("kernel") ||
       name_lower.contains("svchost") ||
       name_lower.contains("launchd") ||
       name_lower.contains("systemd") {
        return "System".to_string();
    }
    
    // Default category
    "Other".to_string()
}

