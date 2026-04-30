#[cfg(target_os = "linux")]
pub mod linux_hw {
    use std::fs;
    use std::process::Command;

    #[derive(Debug, serde::Serialize)]
    pub struct LinuxDetailedHw {
        pub gpu_model: String,
        
        pub motherboard_vendor: String,
        pub motherboard_name: String,
        pub motherboard_serial: String,
        
        pub bios_vendor: String,
        pub bios_version: String,
        pub bios_date: String,
        
        pub chassis_vendor: String,
        pub chassis_type: String,
        pub chassis_serial: String,
        pub chassis_asset_tag: String,
        pub product_name: String, // Model
        pub product_sku: String,
        
        pub is_vm: bool,
    }

    pub fn scan() -> LinuxDetailedHw {
        // Read DMI data
        let product_name = read_dmi("product_name");
        
        LinuxDetailedHw {
            motherboard_vendor: read_dmi("board_vendor"),
            motherboard_name: read_dmi("board_name"),
            motherboard_serial: read_dmi("board_serial"),
            
            bios_vendor: read_dmi("bios_vendor"),
            bios_version: read_dmi("bios_version"),
            bios_date: read_dmi("bios_date"),
            
            chassis_vendor: read_dmi("chassis_vendor"),
            chassis_type: read_dmi("chassis_type"),
            chassis_serial: read_dmi("product_serial"),
            chassis_asset_tag: read_dmi("chassis_asset_tag"),
            
            product_name: product_name.clone(),
            product_sku: read_dmi("product_sku"),
            
            is_vm: product_name.to_lowercase().contains("vmware") 
                   || product_name.contains("VirtualBox")
                   || product_name.contains("KVM"),

            gpu_model: get_gpu_lspci(),
        }
    }

    fn read_dmi(file: &str) -> String {
        let path = format!("/sys/class/dmi/id/{}", file);
        fs::read_to_string(path)
            .unwrap_or_else(|_| "Unknown".to_string())
            .trim()
            .to_string()
    }

    fn get_gpu_lspci() -> String {
        let output = Command::new("lspci")
            .arg("-mm")
            .output();

        if let Ok(out) = output {
            let stdout = String::from_utf8_lossy(&out.stdout);
            for line in stdout.lines() {
                if line.contains("VGA") || line.contains("3D") {
                    let parts: Vec<&str> = line.split('"').collect();
                    if parts.len() > 7 {
                        return format!("{} {}", parts[5], parts[7]);
                    }
                }
            }
        }
        "Standard Graphics".to_string()
    }
}
