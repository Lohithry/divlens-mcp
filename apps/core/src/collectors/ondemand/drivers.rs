use serde::Serialize;
#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
use std::process::Command;

#[derive(Debug, Serialize, Clone)]
pub struct DeviceDriver {
    pub category: String, // "Network", "Multimedia", "Other"
    pub name: String,
    pub version: String,
    pub provider: String,
}

// ==================================================================================
// WINDOWS DRIVER CACHE - Caches driver data for 2 MINUTES to avoid slow PowerShell calls
// Also supports async background fetch on app startup
// ==================================================================================
#[cfg(target_os = "windows")]
static WINDOWS_DRIVER_CACHE: Mutex<Option<(Instant, Vec<DeviceDriver>)>> = Mutex::new(None);

#[cfg(target_os = "windows")]
static WINDOWS_DRIVER_FETCH_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

#[cfg(target_os = "windows")]
const WINDOWS_CACHE_DURATION: Duration = Duration::from_secs(120); // 2 minutes cache

/// Initialize driver cache in background (call this on app startup)
/// This pre-fetches driver data so the first user request is instant
#[cfg(target_os = "windows")]
pub fn init_driver_cache_background() {
    // Only start if not already fetching
    if WINDOWS_DRIVER_FETCH_IN_PROGRESS.compare_exchange(
        false, true, Ordering::SeqCst, Ordering::SeqCst
    ).is_ok() {
        std::thread::spawn(|| {
            tracing::info!("🔧 [Drivers] Starting background driver scan...");
            let start = Instant::now();
            
            let drivers = fetch_windows_drivers();
            
            // Update cache
            {
                let mut cache = WINDOWS_DRIVER_CACHE.lock().unwrap();
                *cache = Some((Instant::now(), drivers.clone()));
            }
            
            WINDOWS_DRIVER_FETCH_IN_PROGRESS.store(false, Ordering::SeqCst);
            tracing::info!("🔧 [Drivers] Background scan complete: {} drivers found in {:?}", 
                drivers.len(), start.elapsed());
        });
    }
}

/// No-op for non-Windows platforms
#[cfg(not(target_os = "windows"))]
pub fn init_driver_cache_background() {
    // No background fetch needed for Mac/Linux (they're fast enough)
}

pub fn get_important_drivers() -> Vec<DeviceDriver> {
    #[cfg(target_os = "macos")]
    return scan_mac_extensions();
    
    #[cfg(target_os = "windows")]
    return scan_windows_pnp(); 

    #[cfg(target_os = "linux")]
    return scan_linux_modules(); 
    
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    vec![]
}

#[cfg(target_os = "macos")]
fn scan_mac_extensions() -> Vec<DeviceDriver> {
    let mut drivers = Vec::new();
    // System Profiler Extensions
    if let Ok(output) = Command::new("system_profiler")
        .args(&["SPExtensionsDataType", "-json"])
        .output() 
    {
        if let Ok(json_str) = String::from_utf8(output.stdout) {
             if let Ok(v) = serde_json::from_str::<serde_json::Value>(&json_str) {
                if let Some(items) = v["SPExtensionsDataType"].as_array() {
                    for item in items {
                         let name = item["_name"].as_str().unwrap_or("Unknown").to_string();
                         let version = item["version"].as_str().unwrap_or("Unknown").to_string();
                         let provider = item["obtained_from"].as_str().unwrap_or("Unknown").to_string();
                         
                         // Simple categorization
                         let category = if name.contains("AirPort") || name.contains("Bluetooth") || name.contains("Ethernet") {
                             "Network".to_string()
                         } else if name.contains("Audio") || name.contains("Graphics") || name.contains("Video") {
                             "Multimedia".to_string()
                         } else {
                             "Other".to_string()
                         };
                         
                         // Filter for only interesting providers or known kexts to keep list sane
                         if provider != "Apple" || category != "Other" {
                             drivers.push(DeviceDriver { category, name, version, provider });
                         }
                    }
                }
             }
        }
    }
    drivers
}

#[cfg(target_os = "linux")]
fn scan_linux_modules() -> Vec<DeviceDriver> {
    let mut drivers = Vec::new();
    // lspci -k gives kernel modules
    if let Ok(output) = Command::new("lspci").arg("-k").output() {
         let out = String::from_utf8_lossy(&output.stdout);
         let mut current_device = String::new();
         for line in out.lines() {
             if !line.starts_with('\t') {
                 current_device = line.to_string();
             } else if line.contains("Kernel driver in use:") {
                 let driver = line.split(':').nth(1).unwrap_or("").trim().to_string();
                 let category = if current_device.contains("Network") || current_device.contains("Ethernet") || current_device.contains("Wireless") {
                     "Network".to_string()
                 } else if current_device.contains("Audio") || current_device.contains("VGA") || current_device.contains("3D") {
                     "Multimedia".to_string()
                 } else {
                     "Other".to_string()
                 };
                 
                 if category != "Other" {
                     drivers.push(DeviceDriver { 
                        category, 
                        name: driver, 
                        version: "Kernel Built-in".to_string(), 
                        provider: "Linux Kernel".to_string() 
                    });
                 }
             }
         }
    }
    drivers
}

#[cfg(target_os = "windows")]
fn scan_windows_pnp() -> Vec<DeviceDriver> {
    // Check cache first - return cached data if still valid
    {
        let cache = WINDOWS_DRIVER_CACHE.lock().unwrap();
        if let Some((timestamp, ref drivers)) = *cache {
            let elapsed = timestamp.elapsed();
            
            // If cache is still valid, return it
            if elapsed < WINDOWS_CACHE_DURATION {
                // If cache is 80% expired (96 seconds), trigger background refresh
                let refresh_threshold = Duration::from_secs(96); // 80% of 120 seconds
                if elapsed > refresh_threshold {
                    // Trigger background refresh (non-blocking)
                    trigger_background_refresh();
                }
                return drivers.clone();
            }
        }
    }
    
    // Cache expired or empty - check if background fetch is in progress
    if WINDOWS_DRIVER_FETCH_IN_PROGRESS.load(Ordering::SeqCst) {
        // Background fetch in progress, return empty or wait briefly
        // Return cached data even if expired (stale is better than nothing)
        let cache = WINDOWS_DRIVER_CACHE.lock().unwrap();
        if let Some((_, ref drivers)) = *cache {
            return drivers.clone();
        }
        // No cached data at all, return empty (rare case)
        return vec![];
    }
    
    // No background fetch in progress, fetch synchronously
    let drivers = fetch_windows_drivers();
    
    // Update cache
    {
        let mut cache = WINDOWS_DRIVER_CACHE.lock().unwrap();
        *cache = Some((Instant::now(), drivers.clone()));
    }
    
    drivers
}

/// Trigger a background refresh of driver cache (non-blocking)
#[cfg(target_os = "windows")]
fn trigger_background_refresh() {
    // Only start if not already fetching
    if WINDOWS_DRIVER_FETCH_IN_PROGRESS.compare_exchange(
        false, true, Ordering::SeqCst, Ordering::SeqCst
    ).is_ok() {
        std::thread::spawn(|| {
            tracing::debug!("🔧 [Drivers] Background refresh triggered...");
            
            let drivers = fetch_windows_drivers();
            
            // Update cache
            {
                let mut cache = WINDOWS_DRIVER_CACHE.lock().unwrap();
                *cache = Some((Instant::now(), drivers));
            }
            
            WINDOWS_DRIVER_FETCH_IN_PROGRESS.store(false, Ordering::SeqCst);
            tracing::debug!("🔧 [Drivers] Background refresh complete");
        });
    }
}

/// Fetch Windows drivers using PowerShell Get-PnpDevice command
/// This is more reliable than WMI and works on Windows 10/11
#[cfg(target_os = "windows")]
fn fetch_windows_drivers() -> Vec<DeviceDriver> {
    let mut drivers = Vec::new();
    
    // Method 1: Use PowerShell Get-PnpDevice for signed drivers (most reliable)
    // Query only Network and Display devices to keep it fast
    let ps_script = r#"
        Get-PnpDevice -Class Net,Display,Media,AudioEndpoint -Status OK -ErrorAction SilentlyContinue | 
        Select-Object Class,FriendlyName,Manufacturer,DriverVersion | 
        ConvertTo-Json -Compress
    "#;
    
    if let Ok(output) = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", ps_script])
        .output()
    {
        if output.status.success() {
            if let Ok(json_str) = String::from_utf8(output.stdout) {
                if let Ok(devices) = serde_json::from_str::<serde_json::Value>(&json_str) {
                    // Handle both single object and array
                    let device_list = if devices.is_array() {
                        devices.as_array().unwrap().clone()
                    } else {
                        vec![devices]
                    };
                    
                    for device in device_list {
                        let class = device["Class"].as_str().unwrap_or("Other");
                        let name = device["FriendlyName"].as_str().unwrap_or("Unknown Device").to_string();
                        let provider = device["Manufacturer"].as_str().unwrap_or("Unknown").to_string();
                        let version = device["DriverVersion"].as_str().unwrap_or("Unknown").to_string();
                        
                        // Categorize based on device class
                        let category = match class {
                            "Net" => "Network".to_string(),
                            "Display" => "Multimedia".to_string(),
                            "Media" | "AudioEndpoint" => "Multimedia".to_string(),
                            _ => "Other".to_string(),
                        };
                        
                        // Skip generic/unknown entries
                        if name != "Unknown Device" && !name.is_empty() {
                            drivers.push(DeviceDriver {
                                category,
                                name,
                                version,
                                provider,
                            });
                        }
                    }
                }
            }
        }
    }
    
    // Method 2: Fallback to WMIC if PowerShell fails (for older Windows)
    if drivers.is_empty() {
        if let Ok(output) = Command::new("wmic")
            .args(["path", "Win32_PnPSignedDriver", "get", "DeviceName,DriverVersion,Manufacturer,DeviceClass", "/format:csv"])
            .output()
        {
            if output.status.success() {
                let out_str = String::from_utf8_lossy(&output.stdout);
                for line in out_str.lines().skip(2) { // Skip header lines
                    let parts: Vec<&str> = line.split(',').collect();
                    if parts.len() >= 4 {
                        let device_class = parts.get(1).unwrap_or(&"").trim();
                        let name = parts.get(2).unwrap_or(&"Unknown").trim().to_string();
                        let version = parts.get(3).unwrap_or(&"Unknown").trim().to_string();
                        let provider = parts.get(4).unwrap_or(&"Unknown").trim().to_string();
                        
                        // Filter for Network and Multimedia devices
                        let category = if device_class.contains("Net") {
                            "Network".to_string()
                        } else if device_class.contains("Display") || device_class.contains("MEDIA") || device_class.contains("Audio") {
                            "Multimedia".to_string()
                        } else {
                            continue; // Skip other categories
                        };
                        
                        if !name.is_empty() && name != "Unknown" {
                            drivers.push(DeviceDriver {
                                category,
                                name,
                                version,
                                provider,
                            });
                        }
                    }
                }
            }
        }
    }
    
    drivers
}

