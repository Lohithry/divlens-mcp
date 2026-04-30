use crate::models::system_schema::{SystemIdentity, MemorySlot, MemoryArchitecture, Graphics, Gpu, Display, CpuAdvanced};
use sysinfo::System;
use std::process::Command;

#[cfg(target_os = "windows")]
use super::windows_hardware;
#[cfg(target_os = "linux")]
use super::linux_hardware;

/// The "Deep Scanner" Collector
/// Runs once at startup to gather static hardware identity.
pub struct HardwareIdentityCollector;

impl HardwareIdentityCollector {
    pub fn collect() -> SystemIdentity {
        let mut sys = System::new_all();
        sys.refresh_all();
        Self::collect_with_system(&sys)
    }

    pub fn collect_with_system(_sys: &System) -> SystemIdentity {
        // 1. Basic Info from sysinfo
        let hostname = System::host_name().unwrap_or_else(|| "Unknown".to_string());
        let os_name = System::name().unwrap_or_else(|| "Unknown".to_string());
        let os_version = System::os_version().unwrap_or_else(|| "Unknown".to_string());
        let kernel = System::kernel_version().unwrap_or_else(|| "Unknown".to_string());
        let arch = System::cpu_arch();
        let uptime = System::uptime();
        let boot_mode = "UEFI".to_string(); 

        // 2. Deep Scan
        let (chassis, board, os_internals, extended) = Self::get_deep_hardware_info();

        SystemIdentity {
            hostname,
            os_name,
            os_version,
            os_build: os_internals.0,
            kernel_version: kernel,
            architecture: arch,
            boot_mode,
            secure_boot: os_internals.1,
            root_integrity: os_internals.2,
            uptime_seconds: uptime,
            
            chassis_manufacturer: chassis.0,
            chassis_model: chassis.1,
            chassis_serial: chassis.2,
            chassis_type: chassis.3,
            chassis_asset_tag: extended.0, 
            sku: extended.1,               
            
            board_vendor: board.0,
            board_name: board.1,
            board_serial: board.2,
            bios_vendor: board.3,
            bios_version: board.4,
            bios_date: board.5,
            
            is_virtual_machine: extended.2, 
            boot_params: extended.3,        
        }
    }

    pub fn collect_memory_layout(sys: &System) -> MemoryArchitecture {
        let total_bytes = sys.total_memory();
        let slots = Self::get_memory_slots();
        MemoryArchitecture {
            total_capacity_bytes: total_bytes,
            memory_type: "Unknown".to_string(),
            speed: "Unknown".to_string(),
            form_factor: "Unknown".to_string(),
            slots,
            used_bytes: 0,
            free_bytes: 0,
            cached_bytes: 0,
            swap_total: 0,
            swap_used: 0,
        }
    }

    pub fn collect_graphics() -> Graphics { Self::get_graphics_info() }
    pub fn collect_cpu_details() -> CpuAdvanced { Self::get_cpu_details_robust() }
    
    pub fn get_apple_l2_cache() -> String {
        #[cfg(target_os = "macos")]
        {
             let output = Command::new("sysctl").args(&["-n", "hw.l2cachesize"]).output().ok();
             if let Some(out) = output {
                let size_str = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if let Ok(size_u64) = size_str.parse::<u64>() {
                     return format!("{} MB (L2/SLC)", size_u64 / 1024 / 1024);
                }
            }
        }
        "Unknown".to_string()
    }

    // --- Platform Implementations ---

    #[cfg(target_os = "macos")]
    fn get_deep_hardware_info() -> ((String, String, String, String), (String, String, String, String, String, String), (String, bool, String), (String, String, bool, String)) {
        let mut chassis = ("Apple Inc.".to_string(), "Mac".to_string(), "Unknown".to_string(), "Notebook".to_string());
        let mut board = ("Apple Inc.".to_string(), "Mac Board".to_string(), "Unknown".to_string(), "Apple".to_string(), "Unknown".to_string(), "Unknown".to_string());
        let mut os_details = ("Unknown".to_string(), true, "Enabled (SIP)".to_string());
        let mut extended = ("Unknown".to_string(), "Unknown".to_string(), false, "N/A (SIP)".to_string());

        if let Ok(output) = Command::new("system_profiler").arg("SPHardwareDataType").arg("-json").output() {
            if let Ok(json_str) = String::from_utf8(output.stdout) {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&json_str) {
                    if let Some(items) = v["SPHardwareDataType"].as_array() {
                        if let Some(item) = items.first() {
                            if let Some(model) = item["machine_model"].as_str() { chassis.1 = model.to_string(); }
                            if let Some(serial) = item["serial_number"].as_str() { chassis.2 = serial.to_string(); }
                            if let Some(boot_rom) = item["boot_rom_version"].as_str() { board.4 = boot_rom.to_string(); }
                            if let Some(uuid) = item["platform_UUID"].as_str() { board.2 = uuid.to_string(); }
                            // Heuristic for VM
                            if chassis.1.contains("Virtual") || chassis.1.contains("Parallels") { extended.2 = true; }
                        }
                    }
                }
            }
        }
        
        if let Ok(output) = Command::new("sw_vers").arg("-buildVersion").output() {
            os_details.0 = String::from_utf8_lossy(&output.stdout).trim().to_string();
        }
        if let Ok(output) = Command::new("csrutil").arg("status").output() {
            let status = String::from_utf8_lossy(&output.stdout);
            if status.contains("disabled") {
                os_details.2 = "Disabled".to_string();
                os_details.1 = false;
            }
        }
        
        (chassis, board, os_details, extended)
    }

    #[cfg(target_os = "linux")]
    fn get_deep_hardware_info() -> ((String, String, String, String), (String, String, String, String, String, String), (String, bool, String), (String, String, bool, String)) {
        let data = linux_hardware::linux_hw::scan();
        // Read boot params
        let boot_params = std::fs::read_to_string("/proc/cmdline").unwrap_or("Unknown".into());
        
        (
            (data.chassis_vendor, data.product_name, data.chassis_serial, data.chassis_type), 
            (data.motherboard_vendor, data.motherboard_name, data.motherboard_serial, data.bios_vendor, data.bios_version, data.bios_date),
            ("Unknown".to_string(), false, "Unknown".to_string()),
            (data.chassis_asset_tag, data.product_sku, data.is_vm, boot_params)
        )
    }

    #[cfg(target_os = "windows")]
    fn get_deep_hardware_info() -> ((String, String, String, String), (String, String, String, String, String, String), (String, bool, String), (String, String, bool, String)) {
        let data = windows_hardware::windows_hw::scan();
        (
            (data.chassis_manufacturer, data.chassis_model, data.chassis_serial, data.chassis_type),
            (data.motherboard_vendor, data.motherboard_model, data.motherboard_serial, data.bios_vendor, data.bios_version, data.bios_date),
            ("Unknown".to_string(), true, "Unknown".to_string()),
            (data.chassis_asset_tag, data.sku, false, "N/A".to_string())
        )
    }
    
    // [Previous logic for CPU/Graphics/Memory is below, unchanged but re-pasted for completeness]
    fn get_cpu_details_robust() -> CpuAdvanced {
        let mut cpu = CpuAdvanced::default();
        #[cfg(target_os = "macos")] {
            let get_sysctl = |key: &str| -> String { Command::new("sysctl").arg("-n").arg(key).output().ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default() };
            cpu.brand = get_sysctl("machdep.cpu.brand_string");
            cpu.vendor_id = get_sysctl("machdep.cpu.vendor");
            cpu.family = get_sysctl("machdep.cpu.family");
            cpu.model = get_sysctl("machdep.cpu.model");
            cpu.stepping = get_sysctl("machdep.cpu.stepping");
            let freq_hz = get_sysctl("hw.cpufrequency").parse::<f64>().unwrap_or(0.0);
            if freq_hz > 0.0 { cpu.base_speed = format!("{:.2} GHz", freq_hz / 1e9); }
            cpu.cores_physical = get_sysctl("hw.physicalcpu").parse().unwrap_or(0);
            cpu.cores_logical = get_sysctl("hw.logicalcpu").parse().unwrap_or(0);
            let p_cores = get_sysctl("hw.perflevel0.physicalcpu").parse().unwrap_or(0);
            let e_cores = get_sysctl("hw.perflevel1.physicalcpu").parse().unwrap_or(0);
            if p_cores > 0 { cpu.performance_cores = p_cores; cpu.efficiency_cores = e_cores; }
            let features = get_sysctl("machdep.cpu.features");
            cpu.virtualization = if features.contains("VMX") || features.contains("SVM") { "Supported".to_string() } else { "Unknown".to_string() };
        }
        #[cfg(target_os = "linux")] {
             if let Ok(output) = Command::new("lscpu").output() {
                let out = String::from_utf8_lossy(&output.stdout);
                for line in out.lines() {
                    let parts: Vec<&str> = line.split(':').map(|s| s.trim()).collect();
                    if parts.len() < 2 { continue; }
                    match parts[0] {
                        "Model name" => cpu.brand = parts[1].to_string(),
                        "Vendor ID" => cpu.vendor_id = parts[1].to_string(),
                        "Virtualization" => cpu.virtualization = parts[1].to_string(),
                        _ => {}
                    }
                }
            }
        }
        #[cfg(target_os = "windows")] { cpu.brand = "Windows CPU (WMI Enhanced)".to_string(); }
        cpu
    }

    #[cfg(target_os = "macos")]
    fn get_graphics_info() -> Graphics {
        let mut gpus = Vec::new();
        let mut displays = Vec::new();
        if let Ok(output) = Command::new("system_profiler").arg("SPDisplaysDataType").arg("-json").output() {
            if let Ok(json_str) = String::from_utf8(output.stdout) {
                 if let Ok(v) = serde_json::from_str::<serde_json::Value>(&json_str) {
                    if let Some(items) = v["SPDisplaysDataType"].as_array() {
                        for item in items {
                            let name = item["sppci_model"].as_str().unwrap_or("Unknown GPU").to_string();
                            let vendor = item["sppci_vendor"].as_str().unwrap_or("Apple").to_string();
                            let vram = item["spdisplays_vram"].as_str().unwrap_or("Shared").to_string();
                            gpus.push(Gpu { name, vendor, vram, driver_version: None, metal_support: None });
                            if let Some(disp_list) = item["spdisplays_ndrvs"].as_array() {
                                for d in disp_list {
                                    let d_name = d["_name"].as_str().unwrap_or("Display").to_string();
                                    let resolution = d["_spdisplays_resolution"].as_str().unwrap_or("Unknown").to_string();
                                    displays.push(Display { name: d_name, resolution, refresh_rate: None });
                                }
                            }
                        }
                    }
                 }
            }
        }
        Graphics { gpus, displays }
    }

    #[cfg(target_os = "linux")]
    fn get_graphics_info() -> Graphics {
        let data = linux_hardware::linux_hw::scan();
        Graphics { gpus: vec![Gpu { name: data.gpu_model, vendor: "Unknown".to_string(), vram: "Unknown".to_string(), driver_version: None, metal_support: None }], displays: vec![] }
    }

    #[cfg(target_os = "windows")]
    fn get_graphics_info() -> Graphics {
        let data = windows_hardware::windows_hw::scan();
        Graphics { gpus: vec![Gpu { name: data.gpu_model, vendor: "Unknown".to_string(), vram: format!("{:.2} GB", data.gpu_vram_gb), driver_version: Some(data.driver_version), metal_support: Some("DirectX".to_string()) }], displays: vec![] }
    }
    
    #[cfg(target_os = "macos")]
    fn get_memory_slots() -> Vec<MemorySlot> {
        let mut slots = Vec::new();
         if let Ok(output) = Command::new("system_profiler").arg("SPMemoryDataType").arg("-json").output() {
            if let Ok(json_str) = String::from_utf8(output.stdout) {
                 if let Ok(v) = serde_json::from_str::<serde_json::Value>(&json_str) {
                    if let Some(items) = v["SPMemoryDataType"].as_array() {
                         for item in items {
                             if let Some(sub_items) = item["_items"].as_array() {
                                 for sub in sub_items {
                                     let name = sub["_name"].as_str().unwrap_or("Unknown").to_string();
                                     let size = sub["dimm_size"].as_str().unwrap_or("0").to_string();
                                     let manuf = sub["dimm_manufacturer"].as_str().unwrap_or("Unknown").to_string();
                                     let part = sub["dimm_part_number"].as_str().unwrap_or("Unknown").to_string();
                                     let serial = sub["dimm_serial_number"].as_str().unwrap_or("Unknown").to_string();
                                     let cap_bytes = if size.contains("GB") { size.replace(" GB", "").trim().parse::<u64>().unwrap_or(0) * 1024 * 1024 * 1024 } else { 0 };
                                     slots.push(MemorySlot { bank_label: name, capacity_bytes: cap_bytes, manufacturer: manuf, part_number: part, serial_number: serial });
                                 }
                             }
                         }
                    }
                 }
            }
        }
        slots
    }
    #[cfg(not(target_os = "macos"))]
    fn get_memory_slots() -> Vec<MemorySlot> { Vec::new() }
}
