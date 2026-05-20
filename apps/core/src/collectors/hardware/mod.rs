// ==================================================================================
// 🛡️ DIVLENS HARDWARE COLLECTORS - Enterprise-Grade Tiered Access System
// ==================================================================================
// This module implements a "Graceful Degradation" architecture that collects
// as much hardware telemetry as possible regardless of permission level.
//
// TIERED ACCESS LEVELS:
// - Tier 1 (Full Native): IOKit/IOCTLs - requires root/admin
// - Tier 2 (CLI Tools): smartctl, nvme-cli - may require root
// - Tier 3 (Basic System): sysinfo, system_profiler - no elevation needed
// - Tier 4 (Minimal): Only what OS freely provides
//
// DESIGN PRINCIPLE: "Everyone gets into the lobby (basic data), but only VIPs
// get into the back room (raw kernel data)."
// ==================================================================================

pub mod smart_native;
pub mod utils;
pub mod tiered_thermal;

#[cfg(target_os = "windows")]
pub mod whea_collector;

#[cfg(target_os = "linux")]
pub mod mce_collector;

#[cfg(target_os = "macos")]
pub mod panic_collector;

#[cfg(target_os = "macos")]
pub mod iokit_smart;

use crate::models::hardware_diagnostics::{
    HardwareDiagnostics, StorageDevice, HardwareEvent, ThermalData, 
    CollectionMetadata, CollectionError, AccessTier
};
use anyhow::Result;
use std::time::Instant;
use std::collections::HashMap;

#[cfg(any(target_os = "linux", target_os = "macos"))]
extern crate libc;

/// Elevation check for hardware access permissions
pub fn check_elevation() -> bool {
    #[cfg(target_os = "windows")]
    {
        unsafe {
            use windows::Win32::UI::Shell::IsUserAnAdmin;
            IsUserAnAdmin().as_bool()
        }
    }
    
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        unsafe { libc::geteuid() == 0 }
    }
    
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        false
    }
}

/// Main hardware diagnostics collection orchestrator
/// Implements TIERED ACCESS with graceful degradation for sub-500ms performance
pub async fn collect_hardware_diagnostics() -> Result<HardwareDiagnostics> {
    let start_time = Instant::now();
    let mut diagnostics = HardwareDiagnostics::default();
    let mut collector_timings: HashMap<String, u64> = HashMap::new();
    let mut errors: Vec<CollectionError> = Vec::new();
    let mut fallback_methods: Vec<String> = Vec::new();
    let mut unavailable_without_elevation: Vec<String> = Vec::new();
    
    // Check elevation status
    let elevated = check_elevation();
    diagnostics.collection_metadata.elevated_permissions = elevated;
    
    // Determine access tier based on elevation
    let access_tier = if elevated {
        AccessTier::FullNative
    } else {
        // Even without elevation, we try multiple tiers
        AccessTier::BasicSystem
    };
    
    // ==================================================================================
    // CONCURRENT TIERED COLLECTION
    // All collectors run in parallel, each implementing their own tiered fallback
    // ==================================================================================
    
    let storage_start = Instant::now();
    let events_start = Instant::now();
    let thermal_start = Instant::now();
    
    let (storage_result, events_result, thermal_result) = tokio::join!(
        collect_storage_tiered(elevated),
        collect_hardware_events_tiered(elevated),
        collect_thermal_tiered(elevated)
    );
    
    // Process storage results
    match storage_result {
        Ok((devices, tier_used, tier_errors)) => {
            diagnostics.storage = devices;
            if tier_used > 1 {
                fallback_methods.push(format!("storage_tier_{}", tier_used));
            }
            errors.extend(tier_errors);
        }
        Err(e) => {
            errors.push(CollectionError {
                component: "storage".to_string(),
                message: e.to_string(),
                fallback_used: true,
            });
        }
    }
    collector_timings.insert("storage".to_string(), storage_start.elapsed().as_millis() as u64);
    
    // Process hardware events results
    match events_result {
        Ok((events, tier_used, tier_errors)) => {
            diagnostics.hardware_events = events;
            if tier_used > 1 {
                fallback_methods.push(format!("events_tier_{}", tier_used));
            }
            errors.extend(tier_errors);
            if !elevated {
                unavailable_without_elevation.push("Raw WHEA/MCE hardware event logs".to_string());
            }
        }
        Err(e) => {
            errors.push(CollectionError {
                component: "hardware_events".to_string(),
                message: e.to_string(),
                fallback_used: true,
            });
        }
    }
    collector_timings.insert("events".to_string(), events_start.elapsed().as_millis() as u64);
    
    // Process thermal results
    match thermal_result {
        Ok((thermal, tier_used, tier_errors)) => {
            diagnostics.thermal = thermal;
            if tier_used > 1 {
                fallback_methods.push(format!("thermal_tier_{}", tier_used));
            }
            errors.extend(tier_errors);
        }
        Err(e) => {
            errors.push(CollectionError {
                component: "thermal".to_string(),
                message: e.to_string(),
                fallback_used: true,
            });
        }
    }
    collector_timings.insert("thermal".to_string(), thermal_start.elapsed().as_millis() as u64);
    
    // Finalize metadata
    diagnostics.collection_metadata = CollectionMetadata {
        collection_time_ms: start_time.elapsed().as_millis() as u64,
        collector_timings,
        elevated_permissions: elevated,
        fallback_methods,
        errors,
        access_tier,
        unavailable_without_elevation,
    };
    
    Ok(diagnostics)
}

// ==================================================================================
// TIERED STORAGE COLLECTION
// ==================================================================================

/// Collect storage diagnostics using tiered access
/// Returns: (devices, tier_used, errors)
#[allow(unused_variables)]
async fn collect_storage_tiered(elevated: bool) -> Result<(Vec<StorageDevice>, u8, Vec<CollectionError>)> {
    let mut errors = Vec::new();
    
    // TIER 1: Native APIs (IOKit on macOS, IOCTLs on Windows)
    #[cfg(target_os = "macos")]
    {
        // Try IOKit first (works WITHOUT root on macOS!)
        match iokit_smart::collect_iokit_storage().await {
            Ok(devices) if !devices.is_empty() => {
                return Ok((devices, 1, errors));
            }
            Ok(_) => {
                errors.push(CollectionError {
                    component: "iokit".to_string(),
                    message: "IOKit returned no devices".to_string(),
                    fallback_used: true,
                });
            }
            Err(e) => {
                errors.push(CollectionError {
                    component: "iokit".to_string(),
                    message: e.to_string(),
                    fallback_used: true,
                });
            }
        }
    }
    
    #[cfg(target_os = "windows")]
    {
        if elevated {
            match smart_native::collect_all_drives().await {
                Ok(devices) if !devices.is_empty() => {
                    return Ok((devices, 1, errors));
                }
                Ok(_) => {
                    errors.push(CollectionError {
                        component: "windows_ioctl".to_string(),
                        message: "IOCTL returned no devices".to_string(),
                        fallback_used: true,
                    });
                }
                Err(e) => {
                    errors.push(CollectionError {
                        component: "windows_ioctl".to_string(),
                        message: e.to_string(),
                        fallback_used: true,
                    });
                }
            }
        } else {
            // TIER 3: WMI fallback (works without admin!)
            match collect_windows_wmi_storage().await {
                Ok(devices) if !devices.is_empty() => {
                    return Ok((devices, 3, errors));
                }
                Ok(_) => {
                    errors.push(CollectionError {
                        component: "windows_wmi".to_string(),
                        message: "WMI returned no devices".to_string(),
                        fallback_used: true,
                    });
                }
                Err(e) => {
                    errors.push(CollectionError {
                        component: "windows_wmi".to_string(),
                        message: e.to_string(),
                        fallback_used: true,
                    });
                }
            }
        }
    }
    
    // TIER 2: CLI Tools (smartctl, nvme-cli)
    match smart_native::collect_fallback().await {
        Ok(devices) if !devices.is_empty() => {
            return Ok((devices, 2, errors));
        }
        Ok(_) => {
            errors.push(CollectionError {
                component: "smartctl".to_string(),
                message: "smartctl returned no devices".to_string(),
                fallback_used: true,
            });
        }
        Err(e) => {
            errors.push(CollectionError {
                component: "smartctl".to_string(),
                message: e.to_string(),
                fallback_used: true,
            });
        }
    }
    
    // TIER 3: Basic System APIs (sysinfo, system_profiler)
    match collect_basic_storage_info().await {
        Ok(devices) => {
            return Ok((devices, 3, errors));
        }
        Err(e) => {
            errors.push(CollectionError {
                component: "basic_storage".to_string(),
                message: e.to_string(),
                fallback_used: true,
            });
        }
    }
    
    // TIER 4: Minimal fallback
    Ok((Vec::new(), 4, errors))
}

/// Collect basic storage info using sysinfo (no elevation required)
async fn collect_basic_storage_info() -> Result<Vec<StorageDevice>> {
    use sysinfo::Disks;
    use crate::models::hardware_diagnostics::{StorageHealth, SmartAttributes, StorageProtocol, HealthStatus};
    
    let disks = Disks::new_with_refreshed_list();
    let mut devices = Vec::new();
    
    for disk in disks.list() {
        let device_path = disk.name().to_string_lossy().to_string();
        let mount_point = disk.mount_point().to_string_lossy().to_string();
        
        // Determine device type from disk kind
        let (device_type, protocol) = match disk.kind() {
            sysinfo::DiskKind::SSD => ("SSD".to_string(), StorageProtocol::NVMe),
            sysinfo::DiskKind::HDD => ("HDD".to_string(), StorageProtocol::SATA),
            _ => ("Unknown".to_string(), StorageProtocol::Unknown),
        };
        
        let total_bytes = disk.total_space();
        let available_bytes = disk.available_space();
        let used_percent = if total_bytes > 0 {
            ((total_bytes - available_bytes) as f64 / total_bytes as f64 * 100.0) as u8
        } else {
            0
        };
        
        devices.push(StorageDevice {
            device: if device_path.is_empty() { mount_point.clone() } else { device_path },
            protocol,
            device_type,
            model: format!("{} Storage", disk.file_system().to_string_lossy()),
            serial: None,
            capacity_bytes: Some(total_bytes),
            health: StorageHealth {
                status: HealthStatus::Unknown,
                percentage_used: Some(used_percent),
                available_spare: None,
                temperature_celsius: None,
                power_on_hours: None,
                critical_warning: false,
                failure_predicted: false,
            },
            smart_attributes: SmartAttributes::default(),
            deep_telemetry_access: false,
            missing_permissions_reason: Some(
                "Basic system API used. Run with elevated permissions for SMART data.".to_string()
            ),
            collection_tier: 3,
        });
    }
    
    Ok(devices)
}

/// Collect storage info using Windows WMI (no admin required!)
#[cfg(target_os = "windows")]
async fn collect_windows_wmi_storage() -> Result<Vec<StorageDevice>> {
    use wmi::WMIConnection;
    use serde::Deserialize;
    use crate::models::hardware_diagnostics::{StorageHealth, SmartAttributes, StorageProtocol, HealthStatus};
    
    #[derive(Deserialize)]
    struct Win32DiskDrive {
        #[serde(rename = "DeviceID")]
        device_id: Option<String>,
        #[serde(rename = "Model")]
        model: Option<String>,
        #[serde(rename = "SerialNumber")]
        serial_number: Option<String>,
        #[serde(rename = "Size")]
        size: Option<u64>,
        #[serde(rename = "MediaType")]
        media_type: Option<String>,
        #[serde(rename = "InterfaceType")]
        interface_type: Option<String>,
        #[serde(rename = "Status")]
        status: Option<String>,
    }
    
    let wmi_con = WMIConnection::new()?;
    
    let results: Vec<Win32DiskDrive> = wmi_con.query()?;
    let mut devices = Vec::new();
    
    for disk in results {
        let device_id = disk.device_id.unwrap_or_else(|| "Unknown".to_string());
        let model = disk.model.unwrap_or_else(|| "Unknown Disk".to_string());
        let interface = disk.interface_type.unwrap_or_default();
        let media = disk.media_type.unwrap_or_default();
        
        // Determine protocol from interface type
        let protocol = if interface.contains("NVMe") || interface.contains("SCSI") && model.to_lowercase().contains("nvme") {
            StorageProtocol::NVMe
        } else if interface.contains("IDE") || interface.contains("SATA") {
            StorageProtocol::SATA
        } else if interface.contains("SCSI") {
            StorageProtocol::SCSI
        } else {
            StorageProtocol::Unknown
        };
        
        // Determine device type
        let device_type = if media.to_lowercase().contains("ssd") || 
                          media.to_lowercase().contains("solid") ||
                          protocol == StorageProtocol::NVMe {
            "SSD".to_string()
        } else if media.to_lowercase().contains("fixed") {
            "HDD".to_string()
        } else {
            "Unknown".to_string()
        };
        
        // Determine health status from WMI Status field
        let health_status = match disk.status.as_deref() {
            Some("OK") => HealthStatus::Healthy,
            Some("Degraded") => HealthStatus::Degraded,
            Some("Error") | Some("Pred Fail") => HealthStatus::Critical,
            _ => HealthStatus::Unknown,
        };
        
        // Check the boolean FIRST while we still own the variable
        let is_critical = health_status == HealthStatus::Critical;
        
        devices.push(StorageDevice {
            device: device_id,
            protocol,
            device_type,
            model,
            serial: disk.serial_number,
            capacity_bytes: disk.size,
            health: StorageHealth {
                status: health_status,
                percentage_used: None,
                available_spare: None,
                temperature_celsius: None,
                power_on_hours: None,
                critical_warning: is_critical, // Use the boolean we created above
                failure_predicted: is_critical,
            },
            smart_attributes: SmartAttributes::default(),
            deep_telemetry_access: false,
            missing_permissions_reason: Some(
                "WMI provides basic disk info. Run as Administrator for SMART data.".to_string()
            ),
            collection_tier: 3,
        });
    }
    
    Ok(devices)
}

#[cfg(not(target_os = "windows"))]
async fn collect_windows_wmi_storage() -> Result<Vec<StorageDevice>> {
    Err(anyhow::anyhow!("WMI is only available on Windows"))
}

// ==================================================================================
// TIERED HARDWARE EVENTS COLLECTION
// ==================================================================================

/// Collect hardware events using tiered access
#[allow(unused_variables)]
async fn collect_hardware_events_tiered(elevated: bool) -> Result<(Vec<HardwareEvent>, u8, Vec<CollectionError>)> {
    let mut errors = Vec::new();
    
    #[cfg(target_os = "windows")]
    {
        if elevated {
            match whea_collector::collect().await {
                Ok(events) => return Ok((events, 1, errors)),
                Err(e) => {
                    errors.push(CollectionError {
                        component: "whea".to_string(),
                        message: e.to_string(),
                        fallback_used: true,
                    });
                }
            }
        }
        // Windows Event Log can be read without admin for some events
        match collect_basic_windows_events().await {
            Ok(events) => return Ok((events, 3, errors)),
            Err(e) => {
                errors.push(CollectionError {
                    component: "windows_events_basic".to_string(),
                    message: e.to_string(),
                    fallback_used: true,
                });
            }
        }
    }
    
    #[cfg(target_os = "linux")]
    {
        if elevated {
            match mce_collector::collect().await {
                Ok(events) => return Ok((events, 1, errors)),
                Err(e) => {
                    errors.push(CollectionError {
                        component: "mce".to_string(),
                        message: e.to_string(),
                        fallback_used: true,
                    });
                }
            }
        }
        // Try journalctl without root (may have limited access)
        match collect_basic_linux_events().await {
            Ok(events) => return Ok((events, 3, errors)),
            Err(e) => {
                errors.push(CollectionError {
                    component: "journalctl_basic".to_string(),
                    message: e.to_string(),
                    fallback_used: true,
                });
            }
        }
    }
    
    #[cfg(target_os = "macos")]
    {
        // macOS panic reports are readable without root
        match panic_collector::collect().await {
            Ok(events) => return Ok((events, 2, errors)),
            Err(e) => {
                errors.push(CollectionError {
                    component: "panic_collector".to_string(),
                    message: e.to_string(),
                    fallback_used: true,
                });
            }
        }
    }
    
    Ok((Vec::new(), 4, errors))
}

#[cfg(target_os = "windows")]
async fn collect_basic_windows_events() -> Result<Vec<HardwareEvent>> {
    Ok(Vec::new())
}

#[cfg(target_os = "linux")]
async fn collect_basic_linux_events() -> Result<Vec<HardwareEvent>> {
    use std::process::Command;
    use crate::models::hardware_diagnostics::{EventSeverity, HardwareComponent, EventDetails};
    
    let mut events = Vec::new();
    
    // Try to read dmesg for hardware errors (may work without root on some systems)
    if let Ok(output) = Command::new("dmesg")
        .args(&["--level=err,crit,alert,emerg", "-T"])
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines().take(50) {
                if line.contains("error") || line.contains("fail") || line.contains("critical") {
                    events.push(HardwareEvent {
                        timestamp: chrono::Utc::now().to_rfc3339(),
                        severity: EventSeverity::Warning,
                        component: HardwareComponent::Unknown,
                        event_type: "dmesg_error".to_string(),
                        details: EventDetails {
                            count: Some(1),
                            bank: None,
                            error_type: Some(line.to_string()),
                            address: None,
                            context: None,
                        },
                        source_id: Some("dmesg".to_string()),
                    });
                }
            }
        }
    }
    
    Ok(events)
}

// ==================================================================================
// TIERED THERMAL COLLECTION
// ==================================================================================

/// Collect thermal data using tiered access
async fn collect_thermal_tiered(elevated: bool) -> Result<(ThermalData, u8, Vec<CollectionError>)> {
    let mut errors = Vec::new();
    
    // Try platform-specific thermal collection
    match tiered_thermal::collect_thermal_data(elevated).await {
        Ok(thermal) => {
            let tier = if thermal.cpu.is_some() || thermal.gpu.is_some() { 1 } else { 3 };
            return Ok((thermal, tier, errors));
        }
        Err(e) => {
            errors.push(CollectionError {
                component: "thermal".to_string(),
                message: e.to_string(),
                fallback_used: true,
            });
        }
    }
    
    // Minimal fallback
    Ok((ThermalData {
        status_note: None,
        cpu: None,
        gpu: None,
        storage: None,
        motherboard: None,
    }, 4, errors))
}
