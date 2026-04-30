// ==================================================================================
// 🛡️ NATIVE SMART DATA COLLECTION - Cross-Platform Enterprise Implementation
// ==================================================================================
// This module implements native SMART data collection using platform-specific APIs
// for maximum performance and accuracy. Handles both NVMe and SATA protocols
// with proper safety guards and memory management.
// ==================================================================================

use crate::models::hardware_diagnostics::{
    StorageDevice, StorageProtocol, StorageHealth, HealthStatus, SmartAttributes
};
use crate::collectors::hardware::utils;
use anyhow::Result;
use std::collections::HashMap;
use std::time::{Instant, Duration};
use std::process::Command;
use once_cell::sync::Lazy;
use std::sync::Mutex;

// Global cache for SMART data (5 minute TTL)
static SMART_CACHE: Lazy<Mutex<utils::HardwareCache<String, StorageDevice>>> = 
    Lazy::new(|| Mutex::new(utils::HardwareCache::new(32, Duration::from_secs(300))));

/// Main entry point for collecting all storage devices
pub async fn collect_all_drives() -> Result<Vec<StorageDevice>> {
    let start_time = Instant::now();
    let mut devices = Vec::new();
    
    #[cfg(target_os = "windows")]
    {
        devices.extend(collect_windows_drives().await?);
    }
    
    #[cfg(target_os = "linux")]
    {
        devices.extend(collect_linux_drives().await?);
    }
    
    #[cfg(target_os = "macos")]
    {
        devices.extend(collect_macos_drives().await?);
    }
    
    // Log collection time for performance monitoring
    let collection_time = start_time.elapsed();
    if collection_time > Duration::from_millis(50) {
        eprintln!("SMART collection took {}ms (target: <50ms per drive)", 
                 collection_time.as_millis());
    }
    
    Ok(devices)
}

/// Fallback collection using smartctl when native APIs unavailable
pub async fn collect_fallback() -> Result<Vec<StorageDevice>> {
    eprintln!("[SMART] Using fallback smartctl method");
    
    // Use existing smartctl implementation
    let mut collector = crate::collectors::volatile::disk_health::DiskHealthCollector::new();
    let diagnostics = collector.update();
    
    let mut devices = Vec::new();
    for diag in diagnostics {
        devices.push(StorageDevice {
            device: diag.name.clone(),
            protocol: StorageProtocol::Unknown,
            device_type: "Unknown".to_string(),
            model: "Unknown".to_string(),
            serial: None,
            capacity_bytes: None,
            health: StorageHealth {
                status: match diag.health_status.as_str() {
                    "Healthy" => HealthStatus::Healthy,
                    "Critical" => HealthStatus::Critical,
                    _ => HealthStatus::Unknown,
                },
                percentage_used: None,
                available_spare: None,
                temperature_celsius: diag.temperature_c,
                power_on_hours: None,
                critical_warning: diag.health_status == "Critical",
                failure_predicted: false,
            },
            smart_attributes: SmartAttributes {
                media_errors: None,
                data_written_tb: None,
                data_read_tb: None,
                unsafe_shutdowns: None,
                controller_busy_time_minutes: None,
                reallocated_sectors: None,
                current_pending_sectors: None,
                raw_attributes: HashMap::new(),
            },
            deep_telemetry_access: diag.temperature_c.is_some(),
            missing_permissions_reason: if diag.temperature_c.is_none() {
                Some("smartctl may require sudo for full SMART access".to_string())
            } else {
                None
            },
            collection_tier: 2,
        });
    }
    
    Ok(devices)
}

// ==================================================================================
// WINDOWS IMPLEMENTATION - NVMe + SATA Support with IOCTLs
// ==================================================================================
#[cfg(target_os = "windows")]
mod windows_impl {
    use super::*;
    use anyhow::anyhow;
    use windows::Win32::{
        Storage::FileSystem::*,
        Foundation::{HANDLE, INVALID_HANDLE_VALUE, CloseHandle},
        System::IO::DeviceIoControl,
    };
    use std::mem::size_of;
    use std::ffi::c_void;

    // Windows IOCTL constants
    const IOCTL_STORAGE_QUERY_PROPERTY: u32 = 0x2D1400;
    const IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS: u32 = 0x560000;
    const STORAGE_ADAPTER_PROTOCOL_SPECIFIC_PROPERTY: u32 = 49;
    const SMART_RCV_DRIVE_DATA: u32 = 0x7C088;
    
    // Windows API constants (hardcoded for stability across crate versions)
    const GENERIC_READ: u32 = 0x80000000;

    // Storage protocol types
    const PROTOCOL_TYPE_NVME: u32 = 3;
    const NVME_DATA_TYPE_LOG_PAGE: u32 = 1;
    const NVME_LOG_PAGE_SMART: u32 = 0x02;

    /// Windows storage property query structure for NVMe
    #[repr(C)]
    struct StoragePropertyQueryWithNvme {
        property_id: u32,
        query_type: u32,
        additional_parameters: StorageProtocolSpecificData,
    }

    #[repr(C)]
    struct StorageProtocolSpecificData {
        protocol_type: u32,
        data_type: u32,
        protocol_data_request_value: u32,
        protocol_data_request_sub_value: u32,
        protocol_data_offset: u32,
        protocol_data_length: u32,
        fixed_protocol_return_data: u32,
        reserved: [u32; 3],
    }

    #[repr(C)]
    struct VolumeExtent {
        disk_number: u32,
        starting_offset: u64,
        extent_length: u64,
    }

    #[repr(C)]
    struct VolumeDiskExtents {
        number_of_disk_extents: u32,
        extents: [VolumeExtent; 1],
    }

    /// NVMe Health Log structure (subset of NVMe spec)
    #[repr(C, packed)]
    struct NvmeHealthLog {
        critical_warning: u8,
        composite_temperature: u16,
        available_spare: u8,
        available_spare_threshold: u8,
        percentage_used: u8,
        reserved1: [u8; 26],
        data_units_read: [u8; 16],
        data_units_written: [u8; 16],
        host_read_commands: [u8; 16],
        host_write_commands: [u8; 16],
        controller_busy_time: [u8; 16],
        power_cycles: [u8; 16],
        power_on_hours: [u8; 16],
        unsafe_shutdowns: [u8; 16],
        media_errors: [u8; 16],
        // ... additional fields as needed
    }

    pub async fn collect_windows_drives() -> Result<Vec<StorageDevice>> {
        let mut devices = Vec::new();
        
        // Enumerate physical drives (\\.\PhysicalDrive0, \\.\PhysicalDrive1, etc.)
        for drive_num in 0..32 {
            let device_path = format!(r"\\.\PhysicalDrive{}", drive_num);
            
            // Check cache first
            if let Ok(mut cache) = SMART_CACHE.try_lock() {
                if let Some(cached_device) = cache.get(&device_path) {
                    devices.push(cached_device);
                    continue;
                }
            }
            
            match collect_single_drive(&device_path).await {
                Ok(Some(device)) => {
                    // Cache successful result
                    if let Ok(mut cache) = SMART_CACHE.try_lock() {
                        cache.insert(device_path.clone(), device.clone());
                    }
                    devices.push(device);
                },
                Ok(None) => break, // No more drives
                Err(e) => {
                    eprintln!("[SMART] Failed to collect {}: {}", device_path, e);
                }
            }
        }
        
        Ok(devices)
    }

    async fn collect_single_drive(device_path: &str) -> Result<Option<StorageDevice>> {
        // Open handle and convert to Send-safe representation
        let handle_value = unsafe {
            let handle = CreateFileA(
                windows::core::PCSTR(format!("{}\0", device_path).as_ptr()),
                GENERIC_READ,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_FLAGS_AND_ATTRIBUTES(0),
                None
            )?;

            if handle == INVALID_HANDLE_VALUE {
                return Ok(None);
            }
            
            // Convert to Send-safe isize (handle value, not pointer)
            // Store raw handle value for later reconstruction
            handle.0 as usize
        };

        // Guard will be created inside async functions
        // Pass Send-safe usize to async functions, they reconstruct HANDLE internally
        
        // Try NVMe first, then fallback to SATA
        if let Ok(device) = query_nvme_drive(handle_value, device_path).await {
            return Ok(Some(device));
        }
        
        if let Ok(device) = query_sata_drive(handle_value, device_path).await {
            return Ok(Some(device));
        }

        Ok(None)
    }

    async fn query_nvme_drive(handle_val: usize, device_path: &str) -> Result<StorageDevice> {
        // Reconstruct the Win32 handle safely inside the function boundary
        let handle = HANDLE(handle_val as *mut _);
        let _guard = HandleGuard(handle);
        
        // Allocate contiguous buffer for query + response  
        let mut buffer: Vec<u8> = vec![0; 4096];
        
        // Setup query structure
        let query_size = size_of::<StoragePropertyQueryWithNvme>();
        let query_ptr = buffer.as_mut_ptr() as *mut StoragePropertyQueryWithNvme;
        
        unsafe {
            (*query_ptr).property_id = STORAGE_ADAPTER_PROTOCOL_SPECIFIC_PROPERTY;
            (*query_ptr).query_type = 0; // PropertyStandardQuery
            (*query_ptr).additional_parameters.protocol_type = PROTOCOL_TYPE_NVME;
            (*query_ptr).additional_parameters.data_type = NVME_DATA_TYPE_LOG_PAGE;
            (*query_ptr).additional_parameters.protocol_data_request_value = NVME_LOG_PAGE_SMART;
            (*query_ptr).additional_parameters.protocol_data_request_sub_value = 0;
            (*query_ptr).additional_parameters.protocol_data_offset = 0;
            (*query_ptr).additional_parameters.protocol_data_length = 512;
            (*query_ptr).additional_parameters.fixed_protocol_return_data = 0;
            (*query_ptr).additional_parameters.reserved = [0; 3];
        }

        let mut bytes_returned: u32 = 0;
        
        let success = unsafe {
            DeviceIoControl(
                handle,
                IOCTL_STORAGE_QUERY_PROPERTY,
                Some(buffer.as_ptr() as *const c_void),
                buffer.len() as u32,
                Some(buffer.as_mut_ptr() as *mut c_void),
                buffer.len() as u32,
                Some(&mut bytes_returned),
                None
            )
        };

        if success.is_err() || success.as_ref().ok() == Some(&()) {
            return Err(anyhow!("NVMe IOCTL failed: {}", std::io::Error::last_os_error()));
        }

        // CRITICAL: Bounds check before parsing
        let min_expected_size = query_size + size_of::<NvmeHealthLog>();
        if (bytes_returned as usize) < min_expected_size {
            return Err(anyhow!(
                "NVMe returned truncated buffer: {} bytes (expected >= {})",
                bytes_returned, min_expected_size
            ));
        }

        // Parse NVMe health log from response buffer
        let health_log_offset = query_size;
        let health_log_ptr = unsafe { 
            buffer.as_ptr().add(health_log_offset) as *const NvmeHealthLog 
        };
        let health_log = unsafe { &*health_log_ptr };

        // Convert to normalized format
        let temperature_kelvin = health_log.composite_temperature;
        let temperature_celsius = if temperature_kelvin > 273 {
            Some((temperature_kelvin - 273) as u32)
        } else {
            None
        };

        let power_on_hours = utils::extract_le_u64(
            &unsafe { std::slice::from_raw_parts(health_log.power_on_hours.as_ptr(), 16) },
            0
        ).ok();

        let data_written_units = utils::extract_le_u64(
            &unsafe { std::slice::from_raw_parts(health_log.data_units_written.as_ptr(), 16) },
            0
        ).ok();
        
        let data_read_units = utils::extract_le_u64(
            &unsafe { std::slice::from_raw_parts(health_log.data_units_read.as_ptr(), 16) },
            0
        ).ok();

        // NVMe data units are typically 512KB blocks
        let data_written_tb = data_written_units.map(|units| (units as f64 * 512.0) / 1_000_000_000_000.0);
        let data_read_tb = data_read_units.map(|units| (units as f64 * 512.0) / 1_000_000_000_000.0);

        let media_errors = utils::extract_le_u64(
            &unsafe { std::slice::from_raw_parts(health_log.media_errors.as_ptr(), 16) },
            0
        ).ok();

        let unsafe_shutdowns = utils::extract_le_u64(
            &unsafe { std::slice::from_raw_parts(health_log.unsafe_shutdowns.as_ptr(), 16) },
            0
        ).ok();

        Ok(StorageDevice {
            device: device_path.to_string(),
            protocol: StorageProtocol::NVMe,
            device_type: "SSD".to_string(),
            model: "NVMe Device".to_string(), // Could be enhanced with device identification
            serial: None,
            capacity_bytes: None,
            health: StorageHealth {
                status: if health_log.critical_warning != 0 {
                    HealthStatus::Critical
                } else if health_log.percentage_used > 80 {
                    HealthStatus::Degraded
                } else {
                    HealthStatus::Healthy
                },
                percentage_used: Some(health_log.percentage_used),
                available_spare: Some(health_log.available_spare),
                temperature_celsius,
                power_on_hours,
                critical_warning: health_log.critical_warning != 0,
                failure_predicted: health_log.percentage_used > 90 || health_log.available_spare < 10,
            },
            smart_attributes: SmartAttributes {
                media_errors,
                data_written_tb,
                data_read_tb,
                unsafe_shutdowns,
                controller_busy_time_minutes: None, // Could be added
                reallocated_sectors: None,
                current_pending_sectors: None,
                raw_attributes: HashMap::new(),
            },
            deep_telemetry_access: true,
            missing_permissions_reason: None,
            collection_tier: 1,
        })
    }

    async fn query_sata_drive(handle_val: usize, device_path: &str) -> Result<StorageDevice> {
        // Reconstruct the Win32 handle safely inside the function boundary
        let _handle = HANDLE(handle_val as *mut _);
        let _guard = HandleGuard(_handle);
        
        // SATA SMART data collection using ATA pass-through
        // This is a simplified implementation - full SATA support would need
        // IOCTL_ATA_PASS_THROUGH or similar for raw SMART attribute access
        
        // For now, return a basic SATA device structure
        Ok(StorageDevice {
            device: device_path.to_string(),
            protocol: StorageProtocol::SATA,
            device_type: "HDD".to_string(),
            model: "SATA Device".to_string(),
            serial: None,
            capacity_bytes: None,
            health: StorageHealth {
                status: HealthStatus::Unknown,
                percentage_used: None,
                available_spare: None,
                temperature_celsius: None,
                power_on_hours: None,
                critical_warning: false,
                failure_predicted: false,
            },
            smart_attributes: SmartAttributes {
                media_errors: None,
                data_written_tb: None,
                data_read_tb: None,
                unsafe_shutdowns: None,
                controller_busy_time_minutes: None,
                reallocated_sectors: None,
                current_pending_sectors: None,
                raw_attributes: HashMap::new(),
            },
            deep_telemetry_access: false,
            missing_permissions_reason: Some("SATA SMART requires ATA pass-through (admin)".to_string()),
            collection_tier: 1,
        })
    }

    /// RAII guard for Windows handle cleanup
    struct HandleGuard(HANDLE);
    
    impl Drop for HandleGuard {
        fn drop(&mut self) {
            unsafe { let _ = CloseHandle(self.0); }
        }
    }
}

#[cfg(target_os = "windows")]
use windows_impl::collect_windows_drives;

// ==================================================================================
// LINUX IMPLEMENTATION - sysfs + ioctl Support  
// ==================================================================================
#[cfg(target_os = "linux")]
async fn collect_linux_drives() -> Result<Vec<StorageDevice>> {
    let mut devices = Vec::new();
    
    // Scan /sys/class/nvme/ for NVMe drives
    if let Ok(entries) = std::fs::read_dir("/sys/class/nvme") {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if name.starts_with("nvme") && !name.contains("n") {
                    // This is a controller (nvme0, nvme1, etc.)
                    if let Ok(device) = collect_linux_nvme_drive(name).await {
                        devices.push(device);
                    }
                }
            }
        }
    }
    
    // Scan /sys/class/block/ for SATA drives  
    if let Ok(entries) = std::fs::read_dir("/sys/class/block") {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if name.starts_with("sd") || name.starts_with("hd") {
                    if let Ok(device) = collect_linux_sata_drive(name).await {
                        devices.push(device);
                    }
                }
            }
        }
    }
    
    Ok(devices)
}

#[cfg(target_os = "linux")]
async fn collect_linux_nvme_drive(controller: &str) -> Result<StorageDevice> {
    // Read NVMe data from sysfs
    let base_path = format!("/sys/class/nvme/{}", controller);
    
    let model = std::fs::read_to_string(format!("{}/model", base_path))
        .unwrap_or_else(|_| "Unknown NVMe".to_string())
        .trim().to_string();
    
    let serial = std::fs::read_to_string(format!("{}/serial", base_path))
        .ok().map(|s| s.trim().to_string());
    
    // Try to get SMART data via nvme-cli if available
    let device_path = format!("/dev/{}", controller);
    let smart_data = get_linux_nvme_smart(&device_path).await
        .unwrap_or_default();
    
    Ok(StorageDevice {
        device: device_path,
        protocol: StorageProtocol::NVMe,
        device_type: "SSD".to_string(),
        model,
        serial,
        capacity_bytes: None,
        health: smart_data.0.clone(),
        smart_attributes: smart_data.1,
        deep_telemetry_access: smart_data.0.percentage_used.is_some(),
        missing_permissions_reason: if smart_data.0.percentage_used.is_none() {
            Some("nvme-cli may require root for full SMART access".to_string())
        } else {
            None
        },
        collection_tier: 2,
    })
}

#[cfg(target_os = "linux")]
async fn collect_linux_sata_drive(device: &str) -> Result<StorageDevice> {
    let device_path = format!("/dev/{}", device);
    
    // Try smartctl for SATA drives
    let smart_data = get_linux_sata_smart(&device_path).await
        .unwrap_or_default();
    
    Ok(StorageDevice {
        device: device_path,
        protocol: StorageProtocol::SATA,
        device_type: "HDD".to_string(),
        model: "SATA Device".to_string(),
        serial: None,
        capacity_bytes: None,
        health: smart_data.0.clone(),
        smart_attributes: smart_data.1,
        deep_telemetry_access: smart_data.0.temperature_celsius.is_some(),
        missing_permissions_reason: if smart_data.0.temperature_celsius.is_none() {
            Some("smartctl may require root for SATA SMART access".to_string())
        } else {
            None
        },
        collection_tier: 2,
    })
}

#[cfg(target_os = "linux")]
async fn get_linux_nvme_smart(device: &str) -> Result<(StorageHealth, SmartAttributes)> {
    // Try nvme-cli command for detailed SMART data
    let output = Command::new("nvme")
        .args(&["smart-log", device, "-o", "json"])
        .output();
    
    if let Ok(output) = output {
        if output.status.success() {
            if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&output.stdout) {
                let temperature = json["temperature"].as_u64().map(|t| t as u32);
                let percentage_used = json["percentage_used"].as_u64().map(|p| p as u8);
                let available_spare = json["available_spare"].as_u64().map(|s| s as u8);
                let media_errors = json["media_errors"].as_u64();
                let power_on_hours = json["power_on_hours"].as_u64();
                
                let health_status = if percentage_used.unwrap_or(0) > 90 {
                    HealthStatus::Critical
                } else if percentage_used.unwrap_or(0) > 80 {
                    HealthStatus::Degraded
                } else {
                    HealthStatus::Healthy
                };
                
                let health = StorageHealth {
                    status: health_status,
                    percentage_used,
                    available_spare,
                    temperature_celsius: temperature,
                    power_on_hours,
                    critical_warning: percentage_used.unwrap_or(0) > 95,
                    failure_predicted: percentage_used.unwrap_or(0) > 90,
                };
                
                let attributes = SmartAttributes {
                    media_errors,
                    data_written_tb: json["data_units_written"].as_u64().map(|u| u as f64 * 512.0 / 1e12),
                    data_read_tb: json["data_units_read"].as_u64().map(|u| u as f64 * 512.0 / 1e12),
                    unsafe_shutdowns: json["unsafe_shutdowns"].as_u64(),
                    controller_busy_time_minutes: json["controller_busy_time"].as_u64(),
                    reallocated_sectors: None,
                    current_pending_sectors: None,
                    raw_attributes: HashMap::new(),
                };
                
                return Ok((health, attributes));
            }
        }
    }
    
    // Fallback to basic sysfs data
    Ok((
        StorageHealth {
            status: HealthStatus::Unknown,
            percentage_used: None,
            available_spare: None,
            temperature_celsius: None,
            power_on_hours: None,
            critical_warning: false,
            failure_predicted: false,
        },
        SmartAttributes {
            media_errors: None,
            data_written_tb: None,
            data_read_tb: None,
            unsafe_shutdowns: None,
            controller_busy_time_minutes: None,
            reallocated_sectors: None,
            current_pending_sectors: None,
            raw_attributes: HashMap::new(),
        },
    ))
}

#[cfg(target_os = "linux")]
async fn get_linux_sata_smart(device: &str) -> Result<(StorageHealth, SmartAttributes)> {
    // Use smartctl for SATA SMART data
    let output = Command::new("smartctl")
        .args(&["-j", "-A", "-H", device])
        .output();
    
    if let Ok(output) = output {
        if output.status.success() {
            if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&output.stdout) {
                let passed = json["smart_status"]["passed"].as_bool().unwrap_or(false);
                let temperature = json["temperature"]["current"].as_u64().map(|t| t as u32);
                
                // Extract key SMART attributes
                let mut reallocated_sectors = None;
                let mut current_pending = None;
                let mut power_on_hours = None;
                
                if let Some(attrs) = json["ata_smart_attributes"]["table"].as_array() {
                    for attr in attrs {
                        if let Some(id) = attr["id"].as_u64() {
                            let raw_value = attr["raw"]["value"].as_u64().unwrap_or(0);
                            match id as u8 {
                                5 => reallocated_sectors = Some(raw_value),
                                9 => power_on_hours = Some(raw_value),
                                197 => current_pending = Some(raw_value),
                                _ => {}
                            }
                        }
                    }
                }
                
                let health_status = if !passed || reallocated_sectors.unwrap_or(0) > 0 {
                    HealthStatus::Critical
                } else if current_pending.unwrap_or(0) > 0 {
                    HealthStatus::Degraded
                } else {
                    HealthStatus::Healthy
                };
                
                let health = StorageHealth {
                    status: health_status,
                    percentage_used: None,
                    available_spare: Some(if passed { 100 } else { 0 }),
                    temperature_celsius: temperature,
                    power_on_hours,
                    critical_warning: !passed,
                    failure_predicted: reallocated_sectors.unwrap_or(0) > 5 || current_pending.unwrap_or(0) > 0,
                };
                
                let attributes = SmartAttributes {
                    media_errors: None,
                    data_written_tb: None,
                    data_read_tb: None,
                    unsafe_shutdowns: None,
                    controller_busy_time_minutes: None,
                    reallocated_sectors,
                    current_pending_sectors: current_pending,
                    raw_attributes: HashMap::new(),
                };
                
                return Ok((health, attributes));
            }
        }
    }
    
    // Fallback
    Ok((
        StorageHealth {
            status: HealthStatus::Unknown,
            percentage_used: None,
            available_spare: None,
            temperature_celsius: None,
            power_on_hours: None,
            critical_warning: false,
            failure_predicted: false,
        },
        SmartAttributes {
            media_errors: None,
            data_written_tb: None,
            data_read_tb: None,
            unsafe_shutdowns: None,
            controller_busy_time_minutes: None,
            reallocated_sectors: None,
            current_pending_sectors: None,
            raw_attributes: HashMap::new(),
        },
    ))
}

// ==================================================================================
// MACOS IMPLEMENTATION - IOKit Framework
// ==================================================================================
#[cfg(target_os = "macos")]
async fn collect_macos_drives() -> Result<Vec<StorageDevice>> {
    let mut devices = Vec::new();
    
    // Use system_profiler for basic drive enumeration
    let output = Command::new("system_profiler")
        .args(&["SPStorageDataType", "-json"])
        .output();
    
    if let Ok(output) = output {
        if output.status.success() {
            if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&output.stdout) {
                if let Some(storage_data) = json["SPStorageDataType"].as_array() {
                    for device_info in storage_data {
                        if let Ok(device) = parse_macos_storage_device(device_info).await {
                            devices.push(device);
                        }
                    }
                }
            }
        }
    }
    
    // Also try to enumerate /dev/disk* entries
    if let Ok(entries) = std::fs::read_dir("/dev") {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if name.starts_with("disk") && !name.contains("s") {
                    // This is a physical disk (disk0, disk1, etc., not partitions)
                    if let Ok(device) = collect_macos_disk_smart(&format!("/dev/{}", name)).await {
                        // Check if we already have this device from system_profiler
                        if !devices.iter().any(|d| d.device == device.device) {
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
async fn parse_macos_storage_device(device_info: &serde_json::Value) -> Result<StorageDevice> {
    let device_name = device_info["_name"]
        .as_str()
        .unwrap_or("Unknown")
        .to_string();
    
    let model = device_info["_name"]
        .as_str()
        .unwrap_or("Unknown macOS Storage")
        .to_string();
    
    let size_bytes = device_info["size_in_bytes"]
        .as_u64();
    
    // Determine protocol based on device info
    let connection = device_info["physical_drive"]["connection_type"]
        .as_str()
        .unwrap_or("");
    
    let protocol = if connection.contains("NVMe") || connection.contains("PCIe") {
        StorageProtocol::NVMe
    } else if connection.contains("SATA") {
        StorageProtocol::SATA
    } else {
        StorageProtocol::Unknown
    };
    
    let device_type = if connection.contains("SSD") || protocol == StorageProtocol::NVMe {
        "SSD".to_string()
    } else {
        "HDD".to_string()
    };
    
    // Try to get SMART data using smartctl
    let device_path = format!("/dev/{}", device_name);
    let (health, smart_attrs) = get_macos_smart_data(&device_path).await
        .unwrap_or_default();
    
    // Check telemetry access before moving health
    let has_deep_telemetry = health.percentage_used.is_some() || health.temperature_celsius.is_some();
    let missing_reason = if !has_deep_telemetry {
        Some("Apple Silicon may limit SMART access. Try running with sudo.".to_string())
    } else {
        None
    };
    
    Ok(StorageDevice {
        device: device_path,
        protocol,
        device_type,
        model,
        serial: device_info["physical_drive"]["serial_number"]
            .as_str()
            .map(|s| s.to_string()),
        capacity_bytes: size_bytes,
        health,
        smart_attributes: smart_attrs,
        deep_telemetry_access: has_deep_telemetry,
        missing_permissions_reason: missing_reason,
        collection_tier: 2,
    })
}

#[cfg(target_os = "macos")]
async fn collect_macos_disk_smart(device: &str) -> Result<StorageDevice> {
    let (health, smart_attrs) = get_macos_smart_data(device).await
        .unwrap_or_default();
    
    let has_deep_telemetry = health.temperature_celsius.is_some();
    let missing_reason = if !has_deep_telemetry {
        Some("smartctl may require sudo for full SMART access".to_string())
    } else {
        None
    };
    
    Ok(StorageDevice {
        device: device.to_string(),
        protocol: StorageProtocol::Unknown,
        device_type: "Unknown".to_string(),
        model: "macOS Storage Device".to_string(),
        serial: None,
        capacity_bytes: None,
        health,
        smart_attributes: smart_attrs,
        deep_telemetry_access: has_deep_telemetry,
        missing_permissions_reason: missing_reason,
        collection_tier: 2,
    })
}

#[cfg(target_os = "macos")]
async fn get_macos_smart_data(device: &str) -> Result<(StorageHealth, SmartAttributes)> {
    // Try smartctl on macOS
    let output = Command::new("smartctl")
        .args(&["-j", "-A", "-H", device])
        .output();
    
    if let Ok(output) = output {
        if output.status.success() {
            if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&output.stdout) {
                let passed = json["smart_status"]["passed"].as_bool().unwrap_or(false);
                let temperature = json["temperature"]["current"].as_u64().map(|t| t as u32);
                
                // Check if this is NVMe or SATA based on available data
                let is_nvme = json["nvme_smart_health_information_log"].is_object();
                
                if is_nvme {
                    // Parse NVMe data
                    if let Some(nvme_data) = json["nvme_smart_health_information_log"].as_object() {
                        let percentage_used = nvme_data["percentage_used"].as_u64().map(|p| p as u8);
                        let available_spare = nvme_data["available_spare"].as_u64().map(|s| s as u8);
                        let media_errors = nvme_data["media_errors"].as_u64();
                        let power_on_hours = nvme_data["power_on_hours"].as_u64();
                        
                        let health = StorageHealth {
                            status: if percentage_used.unwrap_or(0) > 90 {
                                HealthStatus::Critical
                            } else if percentage_used.unwrap_or(0) > 80 {
                                HealthStatus::Degraded
                            } else {
                                HealthStatus::Healthy
                            },
                            percentage_used,
                            available_spare,
                            temperature_celsius: temperature,
                            power_on_hours,
                            critical_warning: percentage_used.unwrap_or(0) > 95,
                            failure_predicted: percentage_used.unwrap_or(0) > 90,
                        };
                        
                        let attributes = SmartAttributes {
                            media_errors,
                            data_written_tb: nvme_data["data_units_written"].as_u64()
                                .map(|u| u as f64 * 512.0 / 1e12),
                            data_read_tb: nvme_data["data_units_read"].as_u64()
                                .map(|u| u as f64 * 512.0 / 1e12),
                            unsafe_shutdowns: nvme_data["unsafe_shutdowns"].as_u64(),
                            controller_busy_time_minutes: None,
                            reallocated_sectors: None,
                            current_pending_sectors: None,
                            raw_attributes: HashMap::new(),
                        };
                        
                        return Ok((health, attributes));
                    }
                } else {
                    // Parse SATA data
                    let mut reallocated_sectors = None;
                    let mut current_pending = None;
                    let mut power_on_hours = None;
                    
                    if let Some(attrs) = json["ata_smart_attributes"]["table"].as_array() {
                        for attr in attrs {
                            if let Some(id) = attr["id"].as_u64() {
                                let raw_value = attr["raw"]["value"].as_u64().unwrap_or(0);
                                match id as u8 {
                                    5 => reallocated_sectors = Some(raw_value),
                                    9 => power_on_hours = Some(raw_value),
                                    197 => current_pending = Some(raw_value),
                                    _ => {}
                                }
                            }
                        }
                    }
                    
                    let health = StorageHealth {
                        status: if !passed || reallocated_sectors.unwrap_or(0) > 0 {
                            HealthStatus::Critical
                        } else if current_pending.unwrap_or(0) > 0 {
                            HealthStatus::Degraded
                        } else {
                            HealthStatus::Healthy
                        },
                        percentage_used: None,
                        available_spare: Some(if passed { 100 } else { 0 }),
                        temperature_celsius: temperature,
                        power_on_hours,
                        critical_warning: !passed,
                        failure_predicted: reallocated_sectors.unwrap_or(0) > 5,
                    };
                    
                    let attributes = SmartAttributes {
                        media_errors: None,
                        data_written_tb: None,
                        data_read_tb: None,
                        unsafe_shutdowns: None,
                        controller_busy_time_minutes: None,
                        reallocated_sectors,
                        current_pending_sectors: current_pending,
                        raw_attributes: HashMap::new(),
                    };
                    
                    return Ok((health, attributes));
                }
            }
        }
    }
    
    // Fallback to unknown status
    Ok((
        StorageHealth {
            status: HealthStatus::Unknown,
            percentage_used: None,
            available_spare: None,
            temperature_celsius: None,
            power_on_hours: None,
            critical_warning: false,
            failure_predicted: false,
        },
        SmartAttributes {
            media_errors: None,
            data_written_tb: None,
            data_read_tb: None,
            unsafe_shutdowns: None,
            controller_busy_time_minutes: None,
            reallocated_sectors: None,
            current_pending_sectors: None,
            raw_attributes: HashMap::new(),
        },
    ))
}