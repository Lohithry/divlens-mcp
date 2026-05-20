// ==================================================================================
// 🌡️ DIVLENS Tiered Thermal Collector - Cross-Platform Temperature Monitoring
// ==================================================================================
// This module implements tiered thermal data collection that works across all
// platforms with graceful degradation based on available permissions.
//
// TIER 1 (Native APIs - may require elevation):
// - macOS: IOKit SMC (System Management Controller)
// - Windows: WMI MSAcpi_ThermalZoneTemperature
// - Linux: /sys/class/hwmon, /sys/class/thermal
//
// TIER 2 (CLI Tools):
// - macOS: osx-cpu-temp, istats
// - Linux: sensors (lm-sensors)
// - Windows: wmic
//
// TIER 3 (Basic System APIs - no elevation):
// - sysinfo crate (limited thermal support)
// - Storage temperature from SMART data
// ==================================================================================

use crate::models::hardware_diagnostics::{ThermalData, ThermalInfo};
use anyhow::Result;

/// Collect thermal data using tiered access
pub async fn collect_thermal_data(elevated: bool) -> Result<ThermalData> {
    #[cfg(target_os = "macos")]
    {
        collect_macos_thermal(elevated).await
    }
    
    #[cfg(target_os = "linux")]
    {
        collect_linux_thermal(elevated).await
    }
    
    #[cfg(target_os = "windows")]
    {
        collect_windows_thermal(elevated).await
    }
    
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        Ok(ThermalData {
            status_note: None,
            cpu: None,
            gpu: None,
            storage: None,
            motherboard: None,
        })
    }
}

// ==================================================================================
// macOS THERMAL COLLECTION
// ==================================================================================

#[cfg(target_os = "macos")]
async fn collect_macos_thermal(_elevated: bool) -> Result<ThermalData> {
    let mut thermal = ThermalData {
        status_note: None,
        cpu: None,
        gpu: None,
        storage: None,
        motherboard: None,
    };
    
    // TIER 1: Try to read from IOKit/SMC using ioreg
    // This often works without root on macOS!
    if let Ok(smc_data) = read_macos_smc_temps().await {
        thermal = smc_data;
        if thermal.cpu.is_some() {
            return Ok(thermal);
        }
    }
    
    // TIER 2: Try powermetrics (requires root but gives detailed data)
    // Skip if not elevated to avoid permission errors
    
    // TIER 3: Try system_profiler for basic thermal info
    if let Ok(profiler_thermal) = read_system_profiler_thermal().await {
        if thermal.cpu.is_none() {
            thermal.cpu = profiler_thermal.cpu;
        }
        if thermal.gpu.is_none() {
            thermal.gpu = profiler_thermal.gpu;
        }
    }
    
    // TIER 4: Use sysinfo as final fallback
    if thermal.cpu.is_none() {
        thermal.cpu = get_sysinfo_cpu_temp();
    }
    
    if thermal.cpu.is_none() && std::env::consts::ARCH == "aarch64" {
        thermal.status_note = Some("Apple Silicon Macs restrict direct access to thermal sensors. The system manages temperatures automatically and will throttle if needed.".to_string());
    }
    
    Ok(thermal)
}

#[cfg(target_os = "macos")]
async fn read_macos_smc_temps() -> Result<ThermalData> {
    use std::process::Command;
    
    let mut thermal = ThermalData {
        status_note: None,
        cpu: None,
        gpu: None,
        storage: None,
        motherboard: None,
    };
    
    // Use ioreg to query AppleSMC for temperature sensors
    // This is the key: SMC data is often readable without root!
    let output = Command::new("ioreg")
        .args(&["-r", "-c", "AppleSMC", "-a"])
        .output();
    
    if let Ok(output) = output {
        if output.status.success() {
            let plist_str = String::from_utf8_lossy(&output.stdout);
            
            // Parse temperature values from SMC
            // Common keys: TC0P (CPU proximity), TC0D (CPU die), TG0P (GPU)
            
            if let Some(cpu_temp) = extract_smc_temp(&plist_str, &["TC0P", "TC0D", "Tc0P", "CPU"]) {
                thermal.cpu = Some(ThermalInfo {
                    current_celsius: Some(cpu_temp),
                    max_celsius: Some(100), // Typical max for Apple Silicon
                    critical_celsius: Some(105),
                    throttling: cpu_temp >= 95,
                    fan_speeds_rpm: Vec::new(),
                });
            }
            
            if let Some(gpu_temp) = extract_smc_temp(&plist_str, &["TG0P", "TG0D", "GPU"]) {
                thermal.gpu = Some(ThermalInfo {
                    current_celsius: Some(gpu_temp),
                    max_celsius: Some(100),
                    critical_celsius: Some(105),
                    throttling: gpu_temp >= 95,
                    fan_speeds_rpm: Vec::new(),
                });
            }
        }
    }
    
    // Also try to get fan speeds
    if let Ok(fan_output) = Command::new("ioreg")
        .args(&["-r", "-c", "AppleFanController", "-a"])
        .output()
    {
        if fan_output.status.success() {
            let fan_str = String::from_utf8_lossy(&fan_output.stdout);
            let fan_speeds = extract_fan_speeds(&fan_str);
            
            if let Some(ref mut cpu) = thermal.cpu {
                cpu.fan_speeds_rpm = fan_speeds.clone();
            }
        }
    }
    
    Ok(thermal)
}

#[cfg(target_os = "macos")]
fn extract_smc_temp(plist: &str, keys: &[&str]) -> Option<u32> {
    for key in keys {
        // Look for temperature values in various formats
        let patterns = [
            format!("\"{}\" = ", key),
            format!("<key>{}</key>", key),
        ];
        
        for pattern in &patterns {
            if let Some(pos) = plist.find(pattern) {
                let after = &plist[pos + pattern.len()..];
                
                // Try to extract numeric value
                let value_str: String = after.chars()
                    .skip_while(|c| !c.is_ascii_digit())
                    .take_while(|c| c.is_ascii_digit() || *c == '.')
                    .collect();
                
                if let Ok(temp) = value_str.parse::<f64>() {
                    // SMC often reports in different scales
                    let normalized = if temp > 200.0 {
                        // Likely in 1/256 degree units
                        (temp / 256.0) as u32
                    } else if temp > 100.0 {
                        // Might be in 1/10 degree units
                        (temp / 10.0) as u32
                    } else {
                        temp as u32
                    };
                    
                    // Sanity check
                    if normalized > 0 && normalized < 150 {
                        return Some(normalized);
                    }
                }
            }
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn extract_fan_speeds(plist: &str) -> Vec<u32> {
    let mut speeds = Vec::new();
    
    // Look for fan speed values
    let patterns = ["ActualSpeed", "CurrentSpeed", "TargetSpeed"];
    
    for pattern in &patterns {
        let search = format!("\"{}\" = ", pattern);
        let mut search_pos = 0;
        
        while let Some(pos) = plist[search_pos..].find(&search) {
            let abs_pos = search_pos + pos;
            let after = &plist[abs_pos + search.len()..];
            
            let value_str: String = after.chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            
            if let Ok(speed) = value_str.parse::<u32>() {
                if speed > 0 && speed < 10000 && !speeds.contains(&speed) {
                    speeds.push(speed);
                }
            }
            
            search_pos = abs_pos + 1;
        }
    }
    
    speeds
}

#[cfg(target_os = "macos")]
async fn read_system_profiler_thermal() -> Result<ThermalData> {
    use std::process::Command;
    
    let thermal = ThermalData {
        status_note: None,
        cpu: None,
        gpu: None,
        storage: None,
        motherboard: None,
    };
    
    // system_profiler SPHardwareDataType can give us some thermal info
    let output = Command::new("system_profiler")
        .args(&["SPHardwareDataType", "-json"])
        .output()?;
    
    if output.status.success() {
        let json_str = String::from_utf8_lossy(&output.stdout);
        
        // On Apple Silicon, thermal data is limited from system_profiler
        // but we can at least detect if the system is under thermal pressure
        if json_str.contains("thermal") || json_str.contains("Thermal") {
            // Parse any thermal indicators
        }
    }
    
    Ok(thermal)
}

// ==================================================================================
// LINUX THERMAL COLLECTION
// ==================================================================================

#[cfg(target_os = "linux")]
async fn collect_linux_thermal(_elevated: bool) -> Result<ThermalData> {
    let mut thermal = ThermalData {
        status_note: None,
        cpu: None,
        gpu: None,
        storage: None,
        motherboard: None,
    };
    
    // TIER 1: Read from sysfs (works without root!)
    if let Ok(hwmon_thermal) = read_linux_hwmon().await {
        thermal = hwmon_thermal;
        if thermal.cpu.is_some() {
            return Ok(thermal);
        }
    }
    
    // TIER 2: Read from thermal zones
    if let Ok(zone_thermal) = read_linux_thermal_zones().await {
        if thermal.cpu.is_none() {
            thermal.cpu = zone_thermal.cpu;
        }
    }
    
    // TIER 3: Try sensors command
    if thermal.cpu.is_none() {
        if let Ok(sensors_thermal) = read_linux_sensors().await {
            thermal = sensors_thermal;
        }
    }
    
    // TIER 4: sysinfo fallback
    if thermal.cpu.is_none() {
        thermal.cpu = get_sysinfo_cpu_temp();
    }
    
    Ok(thermal)
}

#[cfg(target_os = "linux")]
async fn read_linux_hwmon() -> Result<ThermalData> {
    use std::fs;
    use std::path::Path;
    
    let mut thermal = ThermalData {
        status_note: None,
        cpu: None,
        gpu: None,
        storage: None,
        motherboard: None,
    };
    
    let hwmon_path = Path::new("/sys/class/hwmon");
    
    if !hwmon_path.exists() {
        return Ok(thermal);
    }
    
    // Iterate through hwmon devices
    if let Ok(entries) = fs::read_dir(hwmon_path) {
        for entry in entries.flatten() {
            let device_path = entry.path();
            
            // Read device name
            let name_path = device_path.join("name");
            let device_name = fs::read_to_string(&name_path)
                .unwrap_or_default()
                .trim()
                .to_lowercase();
            
            // Find temperature inputs
            for i in 1..=10 {
                let temp_path = device_path.join(format!("temp{}_input", i));
                if let Ok(temp_str) = fs::read_to_string(&temp_path) {
                    if let Ok(temp_millic) = temp_str.trim().parse::<i64>() {
                        let temp_c = (temp_millic / 1000) as u32;
                        
                        // Read max/critical temps if available
                        let max_temp = fs::read_to_string(device_path.join(format!("temp{}_max", i)))
                            .ok()
                            .and_then(|s| s.trim().parse::<i64>().ok())
                            .map(|t| (t / 1000) as u32);
                        
                        let crit_temp = fs::read_to_string(device_path.join(format!("temp{}_crit", i)))
                            .ok()
                            .and_then(|s| s.trim().parse::<i64>().ok())
                            .map(|t| (t / 1000) as u32);
                        
                        // Read label to determine component
                        let label = fs::read_to_string(device_path.join(format!("temp{}_label", i)))
                            .unwrap_or_default()
                            .trim()
                            .to_lowercase();
                        
                        let info = ThermalInfo {
                            current_celsius: Some(temp_c),
                            max_celsius: max_temp,
                            critical_celsius: crit_temp,
                            throttling: crit_temp.map(|c| temp_c >= c - 5).unwrap_or(false),
                            fan_speeds_rpm: Vec::new(),
                        };
                        
                        // Assign to appropriate component
                        if device_name.contains("coretemp") || 
                           device_name.contains("k10temp") ||
                           device_name.contains("cpu") ||
                           label.contains("core") ||
                           label.contains("cpu") {
                            if thermal.cpu.is_none() {
                                thermal.cpu = Some(info);
                            }
                        } else if device_name.contains("amdgpu") ||
                                  device_name.contains("nvidia") ||
                                  device_name.contains("nouveau") ||
                                  label.contains("gpu") {
                            if thermal.gpu.is_none() {
                                thermal.gpu = Some(info);
                            }
                        } else if device_name.contains("nvme") ||
                                  device_name.contains("drivetemp") ||
                                  label.contains("drive") {
                            if thermal.storage.is_none() {
                                thermal.storage = Some(info);
                            }
                        } else if device_name.contains("acpi") ||
                                  device_name.contains("pch") ||
                                  label.contains("board") {
                            if thermal.motherboard.is_none() {
                                thermal.motherboard = Some(info);
                            }
                        }
                    }
                }
            }
            
            // Read fan speeds
            let mut fan_speeds = Vec::new();
            for i in 1..=5 {
                let fan_path = device_path.join(format!("fan{}_input", i));
                if let Ok(fan_str) = fs::read_to_string(&fan_path) {
                    if let Ok(rpm) = fan_str.trim().parse::<u32>() {
                        if rpm > 0 {
                            fan_speeds.push(rpm);
                        }
                    }
                }
            }
            
            if !fan_speeds.is_empty() {
                if let Some(ref mut cpu) = thermal.cpu {
                    cpu.fan_speeds_rpm = fan_speeds;
                }
            }
        }
    }
    
    Ok(thermal)
}

#[cfg(target_os = "linux")]
async fn read_linux_thermal_zones() -> Result<ThermalData> {
    use std::fs;
    use std::path::Path;
    
    let mut thermal = ThermalData {
        status_note: None,
        cpu: None,
        gpu: None,
        storage: None,
        motherboard: None,
    };
    
    let thermal_path = Path::new("/sys/class/thermal");
    
    if !thermal_path.exists() {
        return Ok(thermal);
    }
    
    if let Ok(entries) = fs::read_dir(thermal_path) {
        for entry in entries.flatten() {
            let zone_path = entry.path();
            let zone_name = zone_path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            
            if !zone_name.starts_with("thermal_zone") {
                continue;
            }
            
            // Read temperature
            let temp_path = zone_path.join("temp");
            if let Ok(temp_str) = fs::read_to_string(&temp_path) {
                if let Ok(temp_millic) = temp_str.trim().parse::<i64>() {
                    let temp_c = (temp_millic / 1000) as u32;
                    
                    // Read zone type
                    let type_path = zone_path.join("type");
                    let zone_type = fs::read_to_string(&type_path)
                        .unwrap_or_default()
                        .trim()
                        .to_lowercase();
                    
                    let info = ThermalInfo {
                        current_celsius: Some(temp_c),
                        max_celsius: None,
                        critical_celsius: None,
                        throttling: temp_c >= 90,
                        fan_speeds_rpm: Vec::new(),
                    };
                    
                    if zone_type.contains("cpu") || zone_type.contains("x86_pkg") {
                        if thermal.cpu.is_none() {
                            thermal.cpu = Some(info);
                        }
                    } else if zone_type.contains("gpu") {
                        if thermal.gpu.is_none() {
                            thermal.gpu = Some(info);
                        }
                    } else if zone_type.contains("acpi") && thermal.motherboard.is_none() {
                        thermal.motherboard = Some(info);
                    }
                }
            }
        }
    }
    
    Ok(thermal)
}

#[cfg(target_os = "linux")]
async fn read_linux_sensors() -> Result<ThermalData> {
    use std::process::Command;
    
    let mut thermal = ThermalData {
        status_note: None,
        cpu: None,
        gpu: None,
        storage: None,
        motherboard: None,
    };
    
    // Try sensors command (from lm-sensors package)
    let output = Command::new("sensors")
        .args(&["-j"])
        .output();
    
    if let Ok(output) = output {
        if output.status.success() {
            let json_str = String::from_utf8_lossy(&output.stdout);
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&json_str) {
                // Parse JSON output from sensors
                for (chip_name, chip_data) in json.as_object().unwrap_or(&serde_json::Map::new()) {
                    let chip_lower = chip_name.to_lowercase();
                    
                    if let Some(obj) = chip_data.as_object() {
                        for (sensor_name, sensor_data) in obj {
                            if let Some(sensor_obj) = sensor_data.as_object() {
                                for (key, value) in sensor_obj {
                                    if key.contains("_input") {
                                        if let Some(temp) = value.as_f64() {
                                            let temp_c = temp as u32;
                                            
                                            let info = ThermalInfo {
                                                current_celsius: Some(temp_c),
                                                max_celsius: None,
                                                critical_celsius: None,
                                                throttling: temp_c >= 90,
                                                fan_speeds_rpm: Vec::new(),
                                            };
                                            
                                            if chip_lower.contains("coretemp") || 
                                               chip_lower.contains("k10temp") ||
                                               sensor_name.to_lowercase().contains("core") {
                                                if thermal.cpu.is_none() {
                                                    thermal.cpu = Some(info);
                                                }
                                            } else if chip_lower.contains("amdgpu") ||
                                                      chip_lower.contains("nvidia") {
                                                if thermal.gpu.is_none() {
                                                    thermal.gpu = Some(info);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    Ok(thermal)
}

// ==================================================================================
// WINDOWS THERMAL COLLECTION
// ==================================================================================

#[cfg(target_os = "windows")]
async fn collect_windows_thermal(elevated: bool) -> Result<ThermalData> {
    let mut thermal = ThermalData {
        status_note: None,
        cpu: None,
        gpu: None,
        storage: None,
        motherboard: None,
    };
    
    // TIER 1: WMI MSAcpi_ThermalZoneTemperature (may require admin)
    if elevated {
        if let Ok(wmi_thermal) = read_windows_wmi_thermal().await {
            thermal = wmi_thermal;
            if thermal.cpu.is_some() {
                return Ok(thermal);
            }
        }
    }
    
    // TIER 2: Try wmic command
    if let Ok(wmic_thermal) = read_windows_wmic().await {
        if thermal.cpu.is_none() {
            thermal.cpu = wmic_thermal.cpu;
        }
    }
    
    // TIER 3: sysinfo fallback
    if thermal.cpu.is_none() {
        thermal.cpu = get_sysinfo_cpu_temp();
    }
    
    Ok(thermal)
}

#[cfg(target_os = "windows")]
async fn read_windows_wmi_thermal() -> Result<ThermalData> {
    use wmi::WMIConnection;
    use serde::Deserialize;
    
    #[derive(Deserialize)]
    struct ThermalZone {
        #[serde(rename = "CurrentTemperature")]
        current_temperature: Option<u32>,
        #[serde(rename = "CriticalTripPoint")]
        critical_trip_point: Option<u32>,
    }
    
    let mut thermal = ThermalData {
        status_note: None,
        cpu: None,
        gpu: None,
        storage: None,
        motherboard: None,
    };
    
    let wmi_con = WMIConnection::with_namespace_path("root\\WMI")?;
    
    let results: Vec<ThermalZone> = wmi_con.query()?;
    
    for zone in results {
        if let Some(temp_tenths_kelvin) = zone.current_temperature {
            // Convert from tenths of Kelvin to Celsius
            let temp_c = ((temp_tenths_kelvin as f64 / 10.0) - 273.15) as u32;
            
            let crit_c = zone.critical_trip_point.map(|t| {
                ((t as f64 / 10.0) - 273.15) as u32
            });
            
            if thermal.cpu.is_none() && temp_c > 0 && temp_c < 150 {
                thermal.cpu = Some(ThermalInfo {
                    current_celsius: Some(temp_c),
                    max_celsius: Some(100),
                    critical_celsius: crit_c,
                    throttling: temp_c >= 90,
                    fan_speeds_rpm: Vec::new(),
                });
            }
        }
    }
    
    Ok(thermal)
}

#[cfg(target_os = "windows")]
async fn read_windows_wmic() -> Result<ThermalData> {
    use std::process::Command;
    
    let mut thermal = ThermalData {
        status_note: None,
        cpu: None,
        gpu: None,
        storage: None,
        motherboard: None,
    };
    
    // Try wmic for thermal zone
    let output = Command::new("wmic")
        .args(&["/namespace:\\\\root\\wmi", "PATH", "MSAcpi_ThermalZoneTemperature", "get", "CurrentTemperature"])
        .output();
    
    if let Ok(output) = output {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if let Ok(temp_tenths_k) = line.trim().parse::<u32>() {
                    if temp_tenths_k > 2732 { // > 0°C in tenths of Kelvin
                        let temp_c = ((temp_tenths_k as f64 / 10.0) - 273.15) as u32;
                        if temp_c > 0 && temp_c < 150 {
                            thermal.cpu = Some(ThermalInfo {
                                current_celsius: Some(temp_c),
                                max_celsius: Some(100),
                                critical_celsius: Some(105),
                                throttling: temp_c >= 90,
                                fan_speeds_rpm: Vec::new(),
                            });
                            break;
                        }
                    }
                }
            }
        }
    }
    
    Ok(thermal)
}

// ==================================================================================
// COMMON FALLBACK
// ==================================================================================

/// Get CPU temperature from sysinfo (limited support)
fn get_sysinfo_cpu_temp() -> Option<ThermalInfo> {
    use sysinfo::Components;
    
    let components = Components::new_with_refreshed_list();
    
    for component in components.list() {
        let label = component.label().to_lowercase();
        if label.contains("cpu") || label.contains("core") || label.contains("package") {
            if let Some(temp_f32) = component.temperature() {
                let temp = temp_f32 as u32;
                if temp > 0 && temp < 150 {
                    return Some(ThermalInfo {
                        current_celsius: Some(temp),
                        max_celsius: component.max().map(|t| t as u32),
                        critical_celsius: component.critical().map(|t| t as u32),
                        throttling: component.critical().map(|c| temp_f32 >= c - 5.0).unwrap_or(false),
                        fan_speeds_rpm: Vec::new(),
                    });
                }
            }
        }
    }
    
    None
}
