use sysinfo::Networks;
#[cfg(target_os = "macos")]
use std::process::Command;

#[derive(Debug, Clone, serde::Serialize)]
pub struct NetAdvancedMetrics {
    pub down_kbps: f64,
    pub up_kbps: f64,
    pub packet_errors: u64,
    pub active_socket_count: usize, // The "Pro" Stat
}

// ==================================================================================
// WINDOWS SOCKET CACHE - Caches socket count for 10 seconds (netstat is slow)
// ==================================================================================
#[cfg(target_os = "windows")]
use std::sync::Mutex;
#[cfg(target_os = "windows")]
use std::time::{Instant, Duration};
#[cfg(target_os = "windows")]
use std::process::Command;

#[cfg(target_os = "windows")]
static WINDOWS_SOCKET_CACHE: Mutex<Option<(Instant, usize)>> = Mutex::new(None);

#[cfg(target_os = "windows")]
const WINDOWS_SOCKET_CACHE_DURATION: Duration = Duration::from_secs(10);

pub fn get_advanced_network(networks: &Networks) -> NetAdvancedMetrics {
    // 1. Standard Throughput
    let mut down = 0.0;
    let mut up = 0.0;
    let mut errors = 0;

    // sysinfo provides network data since last refresh
    for (_name, data) in networks {
        down += data.received() as f64;
        up += data.transmitted() as f64;
        errors += data.errors_on_received() + data.errors_on_transmitted();
    }

    // 2. Advanced Socket Counting
    let socket_count = count_active_sockets();

    NetAdvancedMetrics {
        down_kbps: down / 1024.0,
        up_kbps: up / 1024.0,
        packet_errors: errors,
        active_socket_count: socket_count,
    }
}

// --- PLATFORM SPECIFIC SOCKET COUNTING ---

#[cfg(target_os = "linux")]
fn count_active_sockets() -> usize {
    // Linux: Extremely fast reading of /proc/net/tcp
    // Each line is a socket. We subtract 1 for header.
    if let Ok(content) = std::fs::read_to_string("/proc/net/tcp") {
        return content.lines().count().saturating_sub(1);
    }
    0
}

#[cfg(target_os = "macos")]
fn count_active_sockets() -> usize {
    // Mac: 'netstat -an -p tcp'
    // This runs quickly on Mac.
    if let Ok(output) = Command::new("netstat")
        .args(["-an", "-p", "tcp"])
        .output() 
    {
        // Count lines containing "ESTABLISHED"
        let out_str = String::from_utf8_lossy(&output.stdout);
        return out_str.lines()
            .filter(|line| line.contains("ESTABLISHED"))
            .count();
    }
    0
}

#[cfg(target_os = "windows")]
fn count_active_sockets() -> usize {
    // Check cache first - return cached data if still valid
    {
        let cache = WINDOWS_SOCKET_CACHE.lock().unwrap();
        if let Some((timestamp, value)) = *cache {
            if timestamp.elapsed() < WINDOWS_SOCKET_CACHE_DURATION {
                return value;
            }
        }
    }
    
    // Cache expired or empty - fetch fresh data
    let value = fetch_windows_socket_count_impl();
    
    // Update cache
    {
        let mut cache = WINDOWS_SOCKET_CACHE.lock().unwrap();
        *cache = Some((Instant::now(), value));
    }
    
    value
}

/// Fetch Windows active socket count using netstat
#[cfg(target_os = "windows")]
fn fetch_windows_socket_count_impl() -> usize {
    // Method 1: Use PowerShell Get-NetTCPConnection (faster on modern Windows)
    let ps_script = r#"
        try {
            (Get-NetTCPConnection -State Established -ErrorAction Stop | Measure-Object).Count
        } catch {
            -1
        }
    "#;
    
    if let Ok(output) = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", ps_script])
        .output()
    {
        if output.status.success() {
            let out_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if let Ok(value) = out_str.parse::<i32>() {
                if value >= 0 {
                    return value as usize;
                }
            }
        }
    }
    
    // Method 2: Fallback to netstat (works on all Windows versions)
    if let Ok(output) = Command::new("netstat")
        .args(["-ano"])
        .output() 
    {
        let out_str = String::from_utf8_lossy(&output.stdout);
        return out_str.lines()
            .filter(|line| line.contains("ESTABLISHED"))
            .count();
    }
    
    0
}
