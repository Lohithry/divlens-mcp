use std::process::Command;
#[allow(unused_imports)] // Used in platform-specific blocks
use std::time::Duration;

/// Represents snapshot of disk speed metrics.
#[derive(Debug, Default, Clone)]
pub struct DiskSpeed {
    pub read_mb_s: f64,
    pub write_mb_s: f64,
    pub iops: u64,
}

pub struct DiskSpeedCollector;

impl DiskSpeedCollector {
    // This function decides which OS logic to run
    // Using tokio::task::spawn_blocking if we used std::process::Command directly in async context,
    // but here we just return the sync value. The caller should handle async/blocking if needed.
    // However, since we want to support async callers, let's keep it sync for now but efficient.
    // Wait, iostat blocks. So we should use async Command or blocking thread.
    // Since we don't have tokio process as dependency here explicitly (it is in cargo), 
    // let's stick to std::process::Command and rely on the fact that for diagnostics 1s wait is acceptable
    // OR wrap it in tokio::task::spawn_blocking at the call site.
    
    pub fn collect() -> DiskSpeed {
        #[cfg(target_os = "macos")]
        return get_mac_speed();

        #[cfg(target_os = "windows")]
        return get_windows_speed();

        #[cfg(target_os = "linux")]
        return get_linux_speed();

        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        return DiskSpeed::default();
    }
}

// --- MACOS IMPLEMENTATION ---
// Uses 'iostat' which is available to all users by default (no root required).
#[cfg(target_os = "macos")]
fn get_mac_speed() -> DiskSpeed {
    // Command: iostat -d -c 2 -w 1
    // -d: Display disk statistics
    // -c 2: Display 2 samples (1st is boot avg, 2nd is current interval)
    // -w 1: Wait 1 second between samples
    // We parse the 2nd sample to get real-time data.
    
    let output = Command::new("iostat")
        .args(&["-d", "-c", "2", "-w", "1"]) 
        .output();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            parse_iostat_output(&stdout)
        },
        Err(e) => {
            eprintln!("Failed to run iostat: {}", e);
            DiskSpeed::default()
        }
    }
}

#[cfg(target_os = "macos")]
fn parse_iostat_output(out: &str) -> DiskSpeed {
    // Example Output:
    //              disk0       
    //    KB/t tps  MB/s 
    //   25.33   6  0.15 
    //   10.00  50  0.50  <-- We want this line (Last line)
    
    let lines: Vec<&str> = out.lines().collect();
    
    // We expect at least header + 2 samples. Grab the last non-empty line.
    if let Some(last_line) = lines.iter().rev().find(|l| !l.trim().is_empty()) {
        let parts: Vec<&str> = last_line.split_whitespace().collect();
        
        // Structure is typically: [KB/t, tps (IOPS), MB/s]
        // Note: If multiple disks, iostat columns repeat. 
        // For MVP we grab the first disk's stats (usually System disk).
        
        if parts.len() >= 3 {
            let iops: u64 = parts.get(1).unwrap_or(&"0").parse().unwrap_or(0);
            let mb_s: f64 = parts.get(2).unwrap_or(&"0.0").parse().unwrap_or(0.0);
            
            // iostat on Mac often combines Read+Write in one 'MB/s' column in default view?
            // Actually default `iostat -d` is `KB/t tps  MB/s`. Yes, it's total throughput.
            // We'll split it 50/50 for visualization if specific R/W isn't available without flag -I
            return DiskSpeed {
                read_mb_s: mb_s / 2.0, 
                write_mb_s: mb_s / 2.0,
                iops,
            };
        }
    }
    DiskSpeed::default()
}

// --- WINDOWS IMPLEMENTATION ---
// Uses 'typeperf', a built-in tool that reads Performance Counters.
#[cfg(target_os = "windows")]
fn get_windows_speed() -> DiskSpeed {
    // Command: typeperf "\PhysicalDisk(_Total)\Disk Bytes/sec" "\PhysicalDisk(_Total)\Disk Transfers/sec" -sc 1
    // Queries total bytes/sec and IOPS (Transfers/sec) for all disks combined.
    let output = Command::new("typeperf")
        .args(&[
            r"\PhysicalDisk(_Total)\Disk Bytes/sec", 
            r"\PhysicalDisk(_Total)\Disk Transfers/sec",
            "-sc", "1"
        ])
        .output();

    if let Ok(out) = output {
        let stdout = String::from_utf8_lossy(&out.stdout);
        // Output format is CSV-like:
        // "12/18/2025 10:00:01.000","5242880.000000","150.000000"
        // We parse the last line.
        
        let lines: Vec<&str> = stdout.lines().collect();
        if let Some(last_line) = lines.iter().rev().find(|l| l.contains(",")) {
             let parts: Vec<&str> = last_line.split(',').collect();
             if parts.len() >= 3 {
                 // Remove quotes and parse
                 let clean = |s: &str| s.trim_matches('"').parse::<f64>().unwrap_or(0.0);
                 
                 let bytes_per_sec = clean(parts[1]);
                 let transfers_per_sec = clean(parts[2]); // IOPS
                 
                 let mb_s = bytes_per_sec / 1024.0 / 1024.0;
                 
                 return DiskSpeed {
                     read_mb_s: mb_s / 2.0, // typeperf total doesn't split R/W easily here
                     write_mb_s: mb_s / 2.0,
                     iops: transfers_per_sec as u64,
                 };
             }
        }
    }
    DiskSpeed::default()
}

// --- LINUX IMPLEMENTATION ---
// Safely reads /proc/diskstats (No root needed).
#[cfg(target_os = "linux")]
fn get_linux_speed() -> DiskSpeed {
    // Note: /proc/diskstats counters are cumulative since boot.
    // To get a rate, we need State (Prev Value vs Current Value).
    // The current architecture creates a fresh collector on every request, 
    // so we cannot easily calculate rate without persistence.
    // 
    // For this strict Stateless MVP, we return 0 to avoid showing massive cumulative numbers as "current speed".
    // A proper Linux implementation requires persisting the DiskHealthCollector.
    
    // TODO: Upgrade to stateful collector to support Linux rates.
    DiskSpeed::default() 
}
