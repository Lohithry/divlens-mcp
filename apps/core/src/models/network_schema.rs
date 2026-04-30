// ==================================================================================
// 🧠 DIVLENS NETWORK SCHEMA - The Master Data Structure
// ==================================================================================
// This module defines the complete data schema for network monitoring.
// It captures everything: interfaces, connections, routing, and DNS.
//
// DESIGN PRINCIPLES:
// 1. Comprehensive: Holds every possible network detail without being messy
// 2. Serializable: All structs can be converted to JSON for frontend/AI consumption
// 3. Type-Safe: Uses proper Rust types (u16 for ports, Option for nullable fields)
// ==================================================================================

use serde::Serialize;

/// The Master Container - Holds all network information in one place
/// This is the top-level structure returned by network diagnostics
#[derive(Debug, Serialize, Clone, Default)]
pub struct NetworkDeepScan {
    /// List of all network interfaces with live traffic metrics
    pub interface_summary: Vec<InterfaceDetail>,
    
    /// Active network connections with process attribution
    /// This answers "Who is connecting?" - the standout feature
    pub active_connections: Vec<ConnectionDetail>,
    
    /// Routing table entries (default gateway, static routes, etc.)
    pub routing_table: Vec<RouteDetail>,
    
    /// DNS configuration (servers, local domain)
    pub dns_configuration: DnsConfig,
}

/// Interface Details (Physical Layer)
/// Represents a network interface (Wi-Fi, Ethernet, Virtual, etc.)
/// with real-time traffic metrics and signal strength
#[derive(Debug, Serialize, Clone)]
pub struct InterfaceDetail {
    /// Interface name (e.g., "en0", "Wi-Fi", "eth0")
    pub name: String,
    
    /// MAC address in standard format (e.g., "86:fc:07:ab:cd:ef")
    pub mac_address: String,
    
    /// List of IPv4 addresses assigned to this interface
    pub ipv4_addresses: Vec<String>,
    
    /// List of IPv6 addresses assigned to this interface
    pub ipv6_addresses: Vec<String>,
    
    /// Connection type: "Wi-Fi", "Ethernet", "Virtual", "Loopback"
    pub connection_type: String,
    
    /// Download speed in bytes per second (volatile metric)
    pub rx_bytes_sec: u64,
    
    /// Upload speed in bytes per second (volatile metric)
    pub tx_bytes_sec: u64,
    
    /// Signal strength in dBm (decibels relative to milliwatt)
    /// Range: -45 (Excellent) to -90 (Dead)
    /// None if not applicable (e.g., Ethernet connections)
    pub signal_strength_dbm: Option<i32>,
    
    /// Human-readable signal quality rating
    /// Values: "Excellent", "Good", "Fair", "Weak", "N/A"
    pub signal_quality: String,
}

/// Connection Details (Transport Layer - The "Standout" Feature)
/// Maps network connections to processes - answers "Who is using port 443?"
#[derive(Debug, Serialize, Clone)]
pub struct ConnectionDetail {
    /// Protocol: "TCP" or "UDP"
    pub protocol: String,
    
    /// Local IP address (e.g., "192.168.1.100")
    pub local_ip: String,
    
    /// Local port number (e.g., 443, 8080)
    pub local_port: u16,
    
    /// Remote IP address (e.g., "142.250.191.14" for Google)
    pub remote_ip: String,
    
    /// Remote port number
    pub remote_port: u16,
    
    /// Connection state: "ESTABLISHED", "LISTEN", "TIME_WAIT", "CLOSE_WAIT", etc.
    pub state: String,
    
    /// Process ID (PID) that owns this connection
    /// 0 if owned by kernel/system
    pub pid: u32,
    
    /// Process name (e.g., "chrome.exe", "python", "node")
    pub process_name: String,
    
    /// Application category for easier understanding
    /// Values: "Web Browser", "Media", "Developer Tool", "Database", "System/Other"
    pub app_category: String,
}

/// DNS Configuration (Application Layer)
/// Stores DNS server information and local domain settings
#[derive(Debug, Serialize, Clone, Default)]
pub struct DnsConfig {
    /// List of DNS server IP addresses (e.g., ["8.8.8.8", "1.1.1.1"])
    pub servers: Vec<String>,
    
    /// Local domain name if configured (e.g., "local", "corp.internal")
    pub local_domain: Option<String>,
}

/// Route Detail (Network Layer)
/// Represents a routing table entry
#[derive(Debug, Serialize, Clone)]
pub struct RouteDetail {
    /// Destination network (e.g., "0.0.0.0" for default gateway, "192.168.1.0/24" for subnet)
    pub destination: String,
    
    /// Gateway IP address (e.g., "192.168.1.1")
    pub gateway: String,
    
    /// Interface name used for this route (e.g., "en0")
    pub interface: String,
}

