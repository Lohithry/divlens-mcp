use serde::Serialize;

#[derive(Debug, Serialize, Clone)]
pub struct SystemState {
    pub identity: SystemIdentity,
    pub cpu: CpuAdvanced,
    pub memory: MemoryArchitecture,
    pub graphics: Graphics,
    pub storage: StorageEcosystem,
    pub power: PowerManagement,
}

// 1. System Identity (Static)
#[derive(Debug, Serialize, Clone, Default)]
pub struct SystemIdentity {
    pub hostname: String,
    pub os_name: String,
    pub os_version: String,
    pub os_build: String, 
    pub kernel_version: String,
    pub architecture: String,
    pub boot_mode: String,
    pub secure_boot: bool,
    pub root_integrity: String,
    pub uptime_seconds: u64,
    
    // --- Chassis ---
    pub chassis_manufacturer: String,
    pub chassis_model: String,
    pub chassis_serial: String,
    pub chassis_type: String, 
    pub chassis_asset_tag: String, // NEW
    pub sku: String,               // NEW
    
    // --- Baseboard ---
    pub board_vendor: String,
    pub board_name: String,
    pub board_serial: String,
    
    // --- BIOS/Firmware ---
    pub bios_vendor: String,
    pub bios_version: String,
    pub bios_date: String,
    
    // --- Advanced Identity ---
    pub is_virtual_machine: bool, // NEW
    pub boot_params: String,      // NEW
}

// 2. CPU Advanced (Hybrid)
#[derive(Debug, Serialize, Clone, Default)]
pub struct CpuAdvanced {
    // Basic Specs
    pub brand: String,
    pub vendor_id: String,
    pub cores_physical: usize,
    pub cores_logical: usize,
    pub socket: String,
    pub microcode: String,
    
    // Extended CPU Info
    pub base_speed: String, 
    pub speed_range: String, 
    pub efficiency_cores: usize,
    pub performance_cores: usize,
    pub virtualization: String, 
    pub family: String,
    pub model: String,
    pub stepping: String,

    // Caches
    pub l1_cache: String,
    pub l2_cache: String,
    pub l3_cache: String,
    
    // Volatile (Live)
    pub usage_global: f32,
    pub usage_cores: Vec<f32>,
    pub temp_package: Option<f32>,
    pub temp_cores: Vec<f32>,
    pub freq_current: u64,
    pub freq_max: u64,
}

// 3. Memory Architecture (Hybrid)
#[derive(Debug, Serialize, Clone, Default)]
pub struct MemoryArchitecture {
    pub total_capacity_bytes: u64,
    pub memory_type: String, 
    pub speed: String, 
    pub form_factor: String,
    pub slots: Vec<MemorySlot>,
    pub used_bytes: u64,
    pub free_bytes: u64,
    pub cached_bytes: u64, 
    pub swap_total: u64,
    pub swap_used: u64,
}

#[derive(Debug, Serialize, Clone, Default)]
pub struct MemorySlot {
    pub bank_label: String, 
    pub capacity_bytes: u64,
    pub manufacturer: String,
    pub part_number: String,
    pub serial_number: String,
}

// 4. Graphics
#[derive(Debug, Serialize, Clone, Default)]
pub struct Graphics {
    pub gpus: Vec<Gpu>,
    pub displays: Vec<Display>,
}

#[derive(Debug, Serialize, Clone, Default)]
pub struct Gpu {
    pub name: String,
    pub vendor: String,
    pub vram: String,
    pub driver_version: Option<String>,
    pub metal_support: Option<String>,
}

#[derive(Debug, Serialize, Clone, Default)]
pub struct Display {
    pub name: String,
    pub resolution: String,
    pub refresh_rate: Option<u32>,
}

// 5. Storage Ecosystem
#[derive(Debug, Serialize, Clone, Default)]
pub struct StorageEcosystem {
    pub disks: Vec<PhysicalDisk>,
    pub partitions: Vec<Partition>,
    pub global_health: String,
}

#[derive(Debug, Serialize, Clone, Default)]
pub struct PhysicalDisk {
    pub name: String,
    pub model: String,
    pub serial: String,
    pub size_bytes: u64,
    pub interface: String,
    pub is_removable: bool,
    pub is_ssd: bool,
    pub file_system: String,
    pub partition_scheme: String, 
    pub temp_c: Option<f32>,
    pub smart_status: String,
    pub bytes_read: u64,
    pub bytes_written: u64,
}

#[derive(Debug, Serialize, Clone, Default)]
pub struct Partition {
    pub mount_point: String,
    pub file_system: String,
    pub label: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
}

// 6. Power Management
#[derive(Debug, Serialize, Clone, Default)]
pub struct PowerManagement {
    pub manufacturer: String,
    pub model: String,
    pub serial: String,
    pub technology: String, 
    pub design_capacity_mah: u32,
    pub design_voltage_mv: u32,
    pub cycle_count: u32,
    pub is_charging: bool,
    pub state: String, 
    pub charge_percent: f32,
    pub current_capacity_mah: u32,
    pub max_capacity_mah: u32, 
    pub health_percent: f32,
    pub voltage_now_mv: u32,
    pub amperage_now_ma: i32, 
    pub time_remaining_minutes: Option<u64>,
}
