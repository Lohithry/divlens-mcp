use sysinfo::System;
#[cfg(target_os = "macos")]
use std::process::Command;

#[derive(Debug, Clone, serde::Serialize)]
pub struct CpuAdvancedMetrics {
    pub global_usage: f32,
    pub load_avg_1m: f64,
    pub load_avg_5m: f64,
    pub context_switches: u64, // The "Pro" Stat
    pub processes_blocked: u64, // Processes waiting for I/O
}

// ==================================================================================
// WINDOWS CPU CACHE - Caches context switches for 10 seconds (expensive to fetch)
// ==================================================================================
#[cfg(target_os = "windows")]
use std::sync::Mutex;
#[cfg(target_os = "windows")]
use std::time::{Instant, Duration};
#[cfg(target_os = "windows")]
use std::process::Command;

#[cfg(target_os = "windows")]
static WINDOWS_CPU_CACHE: Mutex<Option<(Instant, u64)>> = Mutex::new(None);

#[cfg(target_os = "windows")]
const WINDOWS_CPU_CACHE_DURATION: Duration = Duration::from_secs(10);

pub fn get_advanced_cpu(sys: &System) -> CpuAdvancedMetrics {
    let global = sys.global_cpu_usage();
    let load = System::load_average(); // Load average is a static method in newer sysinfo or via System

    CpuAdvancedMetrics {
        global_usage: global,
        load_avg_1m: load.one,
        load_avg_5m: load.five,
        context_switches: fetch_context_switches(),
        processes_blocked: fetch_blocked_processes(),
    }
}

// --- PLATFORM SPECIFIC LOGIC ---

#[cfg(target_os = "linux")]
fn fetch_context_switches() -> u64 {
    // Robust Linux: Read /proc/stat
    if let Ok(content) = std::fs::read_to_string("/proc/stat") {
        for line in content.lines() {
            if line.starts_with("ctxt ") {
                if let Some(val) = line.split_whitespace().nth(1) {
                    return val.parse().unwrap_or(0);
                }
            }
        }
    }
    0
}

#[cfg(target_os = "macos")]
fn fetch_context_switches() -> u64 {
    // Robust Mac: Use vm_stat tool (Safe wrapper)
    // In C/Rust binding this is 'host_statistics', but parsing CLI is safer for stability
    let output = Command::new("vm_stat").output().ok();
    if let Some(out) = output {
        let _str_out = String::from_utf8_lossy(&out.stdout);
        // vm_stat output looks like: "Mach syscalls: 12345"
        // But context switches usually aren't directly labeled "context switches" 
        // in raw vm_stat output without calculation.
        // However, for MVP stability, we can return 0 or parse if strictly needed.
        // A common proxy is just showing load average on Mac as "stress".
        return 0; 
    }
    0
}

#[cfg(target_os = "windows")]
fn fetch_context_switches() -> u64 {
    // Check cache first - return cached data if still valid
    {
        let cache = WINDOWS_CPU_CACHE.lock().unwrap();
        if let Some((timestamp, value)) = *cache {
            if timestamp.elapsed() < WINDOWS_CPU_CACHE_DURATION {
                return value;
            }
        }
    }
    
    // Cache expired or empty - fetch fresh data
    let value = fetch_windows_context_switches_impl();
    
    // Update cache
    {
        let mut cache = WINDOWS_CPU_CACHE.lock().unwrap();
        *cache = Some((Instant::now(), value));
    }
    
    value
}

/// Fetch Windows context switches using PowerShell performance counter
#[cfg(target_os = "windows")]
fn fetch_windows_context_switches_impl() -> u64 {
    // Method 1: Use PowerShell Get-Counter (most reliable on modern Windows)
    let ps_script = r#"
        try {
            $counter = Get-Counter '\System\Context Switches/sec' -ErrorAction Stop
            [math]::Round($counter.CounterSamples[0].CookedValue)
        } catch {
            0
        }
    "#;
    
    if let Ok(output) = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", ps_script])
        .output()
    {
        if output.status.success() {
            let out_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if let Ok(value) = out_str.parse::<u64>() {
                return value;
            }
        }
    }
    
    // Method 2: Fallback to typeperf (works on older Windows)
    if let Ok(output) = Command::new("typeperf")
        .args([r"\System\Context Switches/sec", "-sc", "1"])
        .output()
    {
        if output.status.success() {
            let out_str = String::from_utf8_lossy(&output.stdout);
            // typeperf output format: "timestamp","value"
            // We need the second line, second column
            for line in out_str.lines().skip(1) {
                if line.contains(',') {
                    let parts: Vec<&str> = line.split(',').collect();
                    if parts.len() >= 2 {
                        let value_str = parts[1].trim().trim_matches('"');
                        if let Ok(value) = value_str.parse::<f64>() {
                            return value as u64;
                        }
                    }
                }
            }
        }
    }
    
    0
}

fn fetch_blocked_processes() -> u64 {
    // Counts processes waiting for I/O (Linux only usually)
    #[cfg(target_os = "linux")]
    {
        if let Ok(content) = std::fs::read_to_string("/proc/stat") {
            for line in content.lines() {
                if line.starts_with("procs_blocked ") {
                    return line.split_whitespace().nth(1).unwrap_or("0").parse().unwrap_or(0);
                }
            }
        }
    }
    0
}

