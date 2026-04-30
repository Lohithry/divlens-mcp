#[cfg(target_os = "windows")]
pub mod windows_hw {
    use wmi::WMIConnection;
    use serde::Deserialize;

    // --- WMI Schemas ---
    // Defined exactly as WMI provides them (PascalCase)
    // Note: Struct names with underscores are required to match WMI class names
    #[allow(non_camel_case_types)]
    #[derive(Deserialize, Debug)]
    #[serde(rename_all = "PascalCase")]
    struct Win32_VideoController {
        name: String,
        adapter_ram: Option<u64>, 
        driver_version: Option<String>,
        video_processor: Option<String>,
    }

    #[allow(non_camel_case_types)]
    #[derive(Deserialize, Debug)]
    #[serde(rename_all = "PascalCase")]
    struct Win32_BaseBoard {
        manufacturer: String,
        product: String, // Model
        serial_number: String,
        version: String,
    }

    #[allow(non_camel_case_types)]
    #[derive(Deserialize, Debug)]
    #[serde(rename_all = "PascalCase")]
    struct Win32_BIOS {
        manufacturer: String,
        version: String,
        release_date: Option<String>,
        serial_number: String, // Often Chassis Serial
    }

    #[allow(non_camel_case_types)]
    #[derive(Deserialize, Debug)]
    #[serde(rename_all = "PascalCase")]
    struct Win32_ComputerSystem {
        manufacturer: String,
        model: String,
        system_type: String, // x64-based PC
    }
    
    #[allow(non_camel_case_types)]
    #[derive(Deserialize, Debug)]
    #[serde(rename_all = "PascalCase")]
    struct Win32_SystemEnclosure {
        chassis_types: Option<Vec<u16>>,
        s_m_b_i_o_s_asset_tag: Option<String>, // Tag
        sku_number: Option<String>,
    }

    // --- Output Structs ---

    #[derive(Debug, serde::Serialize, Default)]
    pub struct WindowsDetailedHw {
        pub gpu_model: String,
        pub gpu_vram_gb: f64,
        pub driver_version: String,
        
        // Baseboard
        pub motherboard_vendor: String,
        pub motherboard_model: String,
        pub motherboard_serial: String,
        
        // BIOS
        pub bios_vendor: String,
        pub bios_version: String,
        pub bios_date: String,
        
        // Chassis / System
        pub chassis_manufacturer: String,
        pub chassis_model: String,
        pub chassis_serial: String,
        pub chassis_type: String,
        pub chassis_asset_tag: String,
        pub sku: String,
    }

    pub fn scan() -> WindowsDetailedHw {
        // WMI 0.18+ uses WMIConnection::new() without arguments
        // It initializes COM internally
        let wmi_con = match WMIConnection::new() {
            Ok(c) => c,
            Err(_) => return WindowsDetailedHw::default(),
        };

        // 1. GPU
        let gpus: Vec<Win32_VideoController> = wmi_con.raw_query("SELECT Name, AdapterRAM, DriverVersion, VideoProcessor FROM Win32_VideoController").unwrap_or_default();
        let (gpu_name, gpu_vram, driver_ver) = if let Some(gpu) = gpus.first() {
            (
                gpu.name.clone(),
                gpu.adapter_ram.unwrap_or(0) as f64 / 1024.0 / 1024.0 / 1024.0, 
                gpu.driver_version.clone().unwrap_or("Unknown".to_string())
            )
        } else {
            ("Standard VGA".to_string(), 0.0, "Unknown".to_string())
        };

        // 2. Motherboard
        let mobos: Vec<Win32_BaseBoard> = wmi_con.raw_query("SELECT Manufacturer, Product, SerialNumber, Version FROM Win32_BaseBoard").unwrap_or_default();
        let mobo = mobos.first();

        // 3. BIOS
        let bioses: Vec<Win32_BIOS> = wmi_con.raw_query("SELECT Manufacturer, Version, ReleaseDate, SerialNumber FROM Win32_BIOS").unwrap_or_default();
        let bios = bioses.first();
        
        // 4. System Info
        let systems: Vec<Win32_ComputerSystem> = wmi_con.raw_query("SELECT Manufacturer, Model, SystemType FROM Win32_ComputerSystem").unwrap_or_default();
        let sys = systems.first();
        
        // 5. Chassis
        let enclosures: Vec<Win32_SystemEnclosure> = wmi_con.raw_query("SELECT ChassisTypes, SMBIOSAssetTag, SKUNumber FROM Win32_SystemEnclosure").unwrap_or_default();
        let enclosure = enclosures.first();

        WindowsDetailedHw {
            gpu_model: gpu_name,
            gpu_vram_gb: (gpu_vram * 100.0).round() / 100.0,
            driver_version: driver_ver,
            
            motherboard_vendor: mobo.map(|m| m.manufacturer.clone()).unwrap_or("Unknown".into()),
            motherboard_model: mobo.map(|m| m.product.clone()).unwrap_or("Unknown".into()),
            motherboard_serial: mobo.map(|m| m.serial_number.clone()).unwrap_or("Unknown".into()),
            
            bios_vendor: bios.map(|b| b.manufacturer.clone()).unwrap_or("Unknown".into()),
            bios_version: bios.map(|b| b.version.clone()).unwrap_or("Unknown".into()),
            bios_date: bios.map(|b| b.release_date.clone().unwrap_or("Unknown".into())).unwrap_or("Unknown".into()),
            
            chassis_manufacturer: sys.map(|s| s.manufacturer.clone()).unwrap_or("Unknown".into()),
            chassis_model: sys.map(|s| s.model.clone()).unwrap_or("Unknown".into()),
            chassis_serial: bios.map(|b| b.serial_number.clone()).unwrap_or("Unknown".into()),
            
            chassis_type: enclosure.and_then(|e| e.chassis_types.as_ref())
                .map(|t| if t.contains(&9) || t.contains(&10) { "Notebook".to_string() } else { "Desktop".to_string() })
                .unwrap_or("Unknown".to_string()),
                
            chassis_asset_tag: enclosure.and_then(|e| e.s_m_b_i_o_s_asset_tag.clone()).unwrap_or("Unknown".into()),
            sku: enclosure.and_then(|e| e.sku_number.clone()).unwrap_or("Unknown".into()),
        }
    }
}
