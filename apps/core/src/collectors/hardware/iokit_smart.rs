// ==================================================================================
// 🍎 DIVLENS macOS IOKit SMART Collector - Tier 1 Native API (NO ROOT REQUIRED!)
// ==================================================================================
// This module uses Apple's IOKit framework to read SSD health data directly from
// the IORegistry. Unlike smartctl, IOKit does NOT require root/sudo permissions
// because the macOS kernel exposes this data publicly.
//
// HOW IT WORKS:
// The macOS kernel constantly monitors SSD health and stores metrics in the
// IORegistry - a public memory dictionary. Any user-space application can read it.
//
// WHAT WE CAN GET WITHOUT ROOT:
// - SSD Model and Serial Number
// - Temperature (from IORegistry or SMC)
// - Wear Level / Percentage Used
// - Available Spare
// - Power-on Hours
// - Total Bytes Written/Read
//
// WHAT STILL REQUIRES ROOT:
// - Raw SMART attribute bytes (512-byte blob)
// - Low-level NVMe admin commands
// ==================================================================================

use crate::models::hardware_diagnostics::{
    StorageDevice, StorageHealth, SmartAttributes, StorageProtocol, HealthStatus
};
use anyhow::{Result, anyhow};

/// Collect storage devices using IOKit (works without root on macOS)
#[cfg(target_os = "macos")]
pub async fn collect_iokit_storage() -> Result<Vec<StorageDevice>> {
    let mut devices = Vec::new();
    
    // Strategy 1: Use system_profiler for basic info (always works)
    // Strategy 2: Try to read IORegistry for SMART data
    // Strategy 3: Parse diskutil for additional details
    
    // Get storage info from system_profiler (no root needed)
    let profiler_devices = collect_from_system_profiler().await?;
    
    // Enhance with IORegistry data where possible
    for mut device in profiler_devices {
        // Try to get SMART data from IORegistry
        if let Ok(smart_data) = get_ioregistry_smart_data(&device.device).await {
            device.health = smart_data.0;
            device.smart_attributes = smart_data.1;
            device.deep_telemetry_access = true;
            device.missing_permissions_reason = None;
            device.collection_tier = 1;
        }
        devices.push(device);
    }
    
    // If no devices found, try diskutil
    if devices.is_empty() {
        devices = collect_from_diskutil().await?;
    }
    
    Ok(devices)
}

#[cfg(not(target_os = "macos"))]
pub async fn collect_iokit_storage() -> Result<Vec<StorageDevice>> {
    Err(anyhow!("IOKit is only available on macOS"))
}

/// Collect storage info from system_profiler (no root required)
#[cfg(target_os = "macos")]
async fn collect_from_system_profiler() -> Result<Vec<StorageDevice>> {
    use std::process::Command;
    
    let output = Command::new("system_profiler")
        .args(&["SPNVMeDataType", "SPSerialATADataType", "-json"])
        .output()?;
    
    if !output.status.success() {
        return Err(anyhow!("system_profiler failed"));
    }
    
    let json_str = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&json_str)?;
    
    let mut devices = Vec::new();
    
    // Parse NVMe devices
    if let Some(nvme_data) = json.get("SPNVMeDataType") {
        if let Some(controllers) = nvme_data.as_array() {
            for controller in controllers {
                if let Some(items) = controller.get("_items").and_then(|v| v.as_array()) {
                    for item in items {
                        if let Some(device) = parse_nvme_device(item) {
                            devices.push(device);
                        }
                    }
                }
                // Also check top-level device info
                if let Some(device) = parse_nvme_device(controller) {
                    devices.push(device);
                }
            }
        }
    }
    
    // Parse SATA devices
    if let Some(sata_data) = json.get("SPSerialATADataType") {
        if let Some(controllers) = sata_data.as_array() {
            for controller in controllers {
                if let Some(items) = controller.get("_items").and_then(|v| v.as_array()) {
                    for item in items {
                        if let Some(device) = parse_sata_device(item) {
                            devices.push(device);
                        }
                    }
                }
            }
        }
    }
    
    Ok(devices)
}

#[cfg(target_os = "macos")]
fn parse_nvme_device(item: &serde_json::Value) -> Option<StorageDevice> {
    let model = item.get("device_model")
        .or_else(|| item.get("_name"))
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown NVMe")
        .to_string();
    
    // Skip if this is just a controller entry without device info
    if model.contains("Controller") && item.get("device_model").is_none() {
        return None;
    }
    
    let serial = item.get("device_serial")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string());
    
    let capacity_str = item.get("size")
        .or_else(|| item.get("_name"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    
    let capacity_bytes = parse_capacity_string(capacity_str);
    
    // Try to get SMART status from system_profiler
    let smart_status = item.get("smart_status")
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown");
    
    let health_status = match smart_status.to_lowercase().as_str() {
        "verified" | "ok" | "passed" => HealthStatus::Healthy,
        "failing" | "failed" => HealthStatus::Failed,
        _ => HealthStatus::Unknown,
    };
    
    // Extract wear level if available (Apple Silicon exposes this)
    let percentage_used = item.get("spnvme_percent_used")
        .or_else(|| item.get("percent_used"))
        .and_then(|v| v.as_str())
        .and_then(|s| s.trim_end_matches('%').parse::<u8>().ok());
    
    // Extract available spare
    let available_spare = item.get("spnvme_available_spare")
        .or_else(|| item.get("available_spare"))
        .and_then(|v| v.as_str())
        .and_then(|s| s.trim_end_matches('%').parse::<u8>().ok());
    
    Some(StorageDevice {
        device: item.get("bsd_name")
            .and_then(|v| v.as_str())
            .map(|s| format!("/dev/{}", s))
            .unwrap_or_else(|| "/dev/disk0".to_string()),
        protocol: StorageProtocol::NVMe,
        device_type: "SSD".to_string(),
        model,
        serial,
        capacity_bytes,
        health: StorageHealth {
            status: health_status,
            percentage_used,
            available_spare,
            temperature_celsius: None, // Will be filled by IORegistry
            power_on_hours: None,
            critical_warning: false,
            failure_predicted: false,
        },
        smart_attributes: SmartAttributes::default(),
        deep_telemetry_access: percentage_used.is_some() || available_spare.is_some(),
        missing_permissions_reason: if percentage_used.is_none() && available_spare.is_none() {
            Some("Limited SMART data from system_profiler. IORegistry may provide more.".to_string())
        } else {
            None
        },
        collection_tier: 1,
    })
}

#[cfg(target_os = "macos")]
fn parse_sata_device(item: &serde_json::Value) -> Option<StorageDevice> {
    let model = item.get("device_model")
        .or_else(|| item.get("_name"))
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown SATA")
        .to_string();
    
    let serial = item.get("device_serial")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string());
    
    let capacity_str = item.get("size")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    
    let capacity_bytes = parse_capacity_string(capacity_str);
    
    let smart_status = item.get("smart_status")
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown");
    
    let health_status = match smart_status.to_lowercase().as_str() {
        "verified" | "ok" | "passed" => HealthStatus::Healthy,
        "failing" | "failed" => HealthStatus::Failed,
        _ => HealthStatus::Unknown,
    };
    
    Some(StorageDevice {
        device: item.get("bsd_name")
            .and_then(|v| v.as_str())
            .map(|s| format!("/dev/{}", s))
            .unwrap_or_else(|| "/dev/disk0".to_string()),
        protocol: StorageProtocol::SATA,
        device_type: if model.to_lowercase().contains("ssd") { "SSD" } else { "HDD" }.to_string(),
        model,
        serial,
        capacity_bytes,
        health: StorageHealth {
            status: health_status,
            percentage_used: None,
            available_spare: None,
            temperature_celsius: None,
            power_on_hours: None,
            critical_warning: false,
            failure_predicted: false,
        },
        smart_attributes: SmartAttributes::default(),
        deep_telemetry_access: false,
        missing_permissions_reason: Some("SATA SMART requires smartctl with sudo".to_string()),
        collection_tier: 2,
    })
}

/// Parse capacity string like "500 GB" or "1 TB" to bytes
fn parse_capacity_string(s: &str) -> Option<u64> {
    let s = s.trim();
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() < 2 {
        return None;
    }
    
    let value: f64 = parts[0].replace(",", "").parse().ok()?;
    let unit = parts[1].to_uppercase();
    
    let multiplier: u64 = match unit.as_str() {
        "B" | "BYTES" => 1,
        "KB" => 1_000,
        "MB" => 1_000_000,
        "GB" => 1_000_000_000,
        "TB" => 1_000_000_000_000,
        "PB" => 1_000_000_000_000_000,
        _ => return None,
    };
    
    Some((value * multiplier as f64) as u64)
}

/// Get SMART data from IORegistry (the magic - works without root!)
#[cfg(target_os = "macos")]
async fn get_ioregistry_smart_data(_device: &str) -> Result<(StorageHealth, SmartAttributes)> {
    use std::process::Command;
    
    // Use ioreg to query the IORegistry for NVMe SMART data
    // This is the key insight: IORegistry is PUBLIC and readable by any user!
    
    let output = Command::new("ioreg")
        .args(&["-r", "-c", "IONVMeController", "-a"])
        .output()?;
    
    if !output.status.success() {
        return Err(anyhow!("ioreg failed"));
    }
    
    let plist_str = String::from_utf8_lossy(&output.stdout);
    
    // Parse the plist output for SMART data
    let (health, attributes) = parse_ioreg_nvme_smart(&plist_str)?;
    
    Ok((health, attributes))
}

#[cfg(target_os = "macos")]
fn parse_ioreg_nvme_smart(plist_str: &str) -> Result<(StorageHealth, SmartAttributes)> {
    // Parse key SMART values from ioreg output
    // The IORegistry exposes these as properties on IONVMeController
    
    let mut health = StorageHealth::default();
    let mut attributes = SmartAttributes::default();
    
    // Look for key metrics in the plist
    // Temperature
    if let Some(temp) = extract_plist_value(plist_str, "Temperature") {
        health.temperature_celsius = temp.parse::<u32>().ok();
    }
    
    // Percentage Used (NVMe wear indicator)
    if let Some(used) = extract_plist_value(plist_str, "Percentage Used") {
        health.percentage_used = used.trim_end_matches('%').parse::<u8>().ok();
    }
    
    // Available Spare
    if let Some(spare) = extract_plist_value(plist_str, "Available Spare") {
        health.available_spare = spare.trim_end_matches('%').parse::<u8>().ok();
    }
    
    // Power On Hours
    if let Some(hours) = extract_plist_value(plist_str, "Power On Hours") {
        health.power_on_hours = hours.parse::<u64>().ok();
    }
    
    // Data Written (in bytes, convert to TB)
    if let Some(written) = extract_plist_value(plist_str, "Data Units Written") {
        if let Ok(units) = written.parse::<u64>() {
            // NVMe reports in 512KB units
            attributes.data_written_tb = Some((units as f64 * 512.0 * 1024.0) / 1_000_000_000_000.0);
        }
    }
    
    // Data Read
    if let Some(read) = extract_plist_value(plist_str, "Data Units Read") {
        if let Ok(units) = read.parse::<u64>() {
            attributes.data_read_tb = Some((units as f64 * 512.0 * 1024.0) / 1_000_000_000_000.0);
        }
    }
    
    // Media Errors
    if let Some(errors) = extract_plist_value(plist_str, "Media Errors") {
        attributes.media_errors = errors.parse::<u64>().ok();
    }
    
    // Unsafe Shutdowns
    if let Some(shutdowns) = extract_plist_value(plist_str, "Unsafe Shutdowns") {
        attributes.unsafe_shutdowns = shutdowns.parse::<u64>().ok();
    }
    
    // Determine health status based on metrics
    health.status = determine_health_status(&health, &attributes);
    
    // Check for critical warnings
    if let Some(warning) = extract_plist_value(plist_str, "Critical Warning") {
        health.critical_warning = warning != "0" && warning.to_lowercase() != "none";
    }
    
    Ok((health, attributes))
}

/// Extract a value from plist-style output
fn extract_plist_value(plist: &str, key: &str) -> Option<String> {
    // Look for patterns like:
    // "Temperature" = 45
    // <key>Temperature</key><integer>45</integer>
    
    // Try XML plist format first
    let xml_pattern = format!("<key>{}</key>", key);
    if let Some(key_pos) = plist.find(&xml_pattern) {
        let after_key = &plist[key_pos + xml_pattern.len()..];
        
        // Look for <integer>, <string>, or <real> value
        if let Some(start) = after_key.find('>') {
            let value_start = start + 1;
            if let Some(end) = after_key[value_start..].find('<') {
                return Some(after_key[value_start..value_start + end].trim().to_string());
            }
        }
    }
    
    // Try key = value format
    let kv_pattern = format!("\"{}\" = ", key);
    if let Some(pos) = plist.find(&kv_pattern) {
        let after_eq = &plist[pos + kv_pattern.len()..];
        // Find end of value (newline or semicolon)
        let end = after_eq.find(|c| c == '\n' || c == ';').unwrap_or(after_eq.len());
        let value = after_eq[..end].trim().trim_matches('"');
        return Some(value.to_string());
    }
    
    None
}

/// Determine health status from metrics
fn determine_health_status(health: &StorageHealth, attributes: &SmartAttributes) -> HealthStatus {
    // Critical conditions
    if health.critical_warning {
        return HealthStatus::Critical;
    }
    
    // Check percentage used (NVMe wear)
    if let Some(used) = health.percentage_used {
        if used >= 100 {
            return HealthStatus::Failed;
        } else if used >= 90 {
            return HealthStatus::Critical;
        } else if used >= 80 {
            return HealthStatus::Degraded;
        }
    }
    
    // Check available spare
    if let Some(spare) = health.available_spare {
        if spare <= 10 {
            return HealthStatus::Critical;
        } else if spare <= 25 {
            return HealthStatus::Degraded;
        }
    }
    
    // Check media errors
    if let Some(errors) = attributes.media_errors {
        if errors > 100 {
            return HealthStatus::Critical;
        } else if errors > 10 {
            return HealthStatus::Degraded;
        }
    }
    
    // Check temperature
    if let Some(temp) = health.temperature_celsius {
        if temp >= 70 {
            return HealthStatus::Critical;
        } else if temp >= 60 {
            return HealthStatus::Degraded;
        }
    }
    
    // If we have any data, assume healthy
    if health.percentage_used.is_some() || health.available_spare.is_some() {
        return HealthStatus::Healthy;
    }
    
    HealthStatus::Unknown
}

/// Fallback: collect from diskutil (no root required)
#[cfg(target_os = "macos")]
async fn collect_from_diskutil() -> Result<Vec<StorageDevice>> {
    use std::process::Command;
    
    let output = Command::new("diskutil")
        .args(&["list", "-plist"])
        .output()?;
    
    if !output.status.success() {
        return Err(anyhow!("diskutil failed"));
    }
    
    let plist_str = String::from_utf8_lossy(&output.stdout);
    let mut devices = Vec::new();
    
    // Parse disk identifiers from plist
    // This gives us basic disk info without SMART data
    
    // For each disk, get more info
    let disk_pattern = "disk";
    for line in plist_str.lines() {
        if line.contains("<string>") && line.contains(disk_pattern) {
            if let Some(start) = line.find("<string>") {
                if let Some(end) = line.find("</string>") {
                    let disk_id = &line[start + 8..end];
                    if disk_id.starts_with("disk") && !disk_id.contains("s") {
                        // Get info for this disk
                        if let Ok(device) = get_diskutil_info(disk_id).await {
                            devices.push(device);
                        }
                    }
                }
            }
        }
    }
    
    Ok(devices)
}

#[cfg(target_os = "macos")]
async fn get_diskutil_info(disk_id: &str) -> Result<StorageDevice> {
    use std::process::Command;
    
    let output = Command::new("diskutil")
        .args(&["info", "-plist", disk_id])
        .output()?;
    
    if !output.status.success() {
        return Err(anyhow!("diskutil info failed for {}", disk_id));
    }
    
    let plist_str = String::from_utf8_lossy(&output.stdout);
    
    let model = extract_plist_value(&plist_str, "MediaName")
        .or_else(|| extract_plist_value(&plist_str, "DeviceModel"))
        .unwrap_or_else(|| "Unknown".to_string());
    
    let size_str = extract_plist_value(&plist_str, "TotalSize")
        .or_else(|| extract_plist_value(&plist_str, "Size"));
    
    let capacity_bytes = size_str.and_then(|s| s.parse::<u64>().ok());
    
    let is_ssd = extract_plist_value(&plist_str, "SolidState")
        .map(|v| v == "Yes" || v == "true" || v == "1")
        .unwrap_or(false);
    
    let protocol = if extract_plist_value(&plist_str, "DeviceProtocol")
        .map(|p| p.contains("NVMe") || p.contains("PCI"))
        .unwrap_or(false) 
    {
        StorageProtocol::NVMe
    } else if is_ssd {
        StorageProtocol::NVMe // Apple Silicon SSDs are NVMe
    } else {
        StorageProtocol::SATA
    };
    
    Ok(StorageDevice {
        device: format!("/dev/{}", disk_id),
        protocol,
        device_type: if is_ssd { "SSD" } else { "HDD" }.to_string(),
        model,
        serial: extract_plist_value(&plist_str, "DeviceSerialNumber"),
        capacity_bytes,
        health: StorageHealth::default(),
        smart_attributes: SmartAttributes::default(),
        deep_telemetry_access: false,
        missing_permissions_reason: Some("diskutil provides limited SMART data".to_string()),
        collection_tier: 3,
    })
}

#[cfg(not(target_os = "macos"))]
async fn collect_from_system_profiler() -> Result<Vec<StorageDevice>> {
    Err(anyhow!("system_profiler is only available on macOS"))
}

#[cfg(not(target_os = "macos"))]
async fn collect_from_diskutil() -> Result<Vec<StorageDevice>> {
    Err(anyhow!("diskutil is only available on macOS"))
}
