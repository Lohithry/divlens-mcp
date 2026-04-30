use serde::Serialize;
use sysinfo::{System, RefreshKind, CpuRefreshKind, MemoryRefreshKind, Networks, ProcessesToUpdate, Disks};
use crate::models::system_schema::PowerManagement;

pub mod cpu;
pub mod memory;
pub mod disk_io;
pub mod network_io;
pub mod processes; 
pub mod disk_health; 
pub mod power_thermal; 

pub mod cpu_advanced;
pub mod memory_advanced;
pub mod network_advanced;
pub mod network_metrics;
pub mod disk_speed;

#[derive(Serialize, Clone, Debug)]
pub struct RealTimeMetrics {
    pub cpu: cpu_advanced::CpuAdvancedMetrics,
    pub ram: memory_advanced::MemAdvancedMetrics,
    pub net: network_advanced::NetAdvancedMetrics,
    pub disk_read_mbps: f64,
    pub disk_write_mbps: f64,
    pub power: PowerManagement,
}

pub struct VolatileCollector {
    sys: System,
    networks: Networks,
    disks: Disks, // Add Disks back as sysinfo 0.30+ separates them
    // Removed persistent battery_manager to ensure Send+Sync safety
}

impl VolatileCollector {
    pub fn new() -> Self {
        let refresh = RefreshKind::nothing()
            .with_cpu(CpuRefreshKind::everything())
            .with_memory(MemoryRefreshKind::everything());
            
        let mut sys = System::new_with_specifics(refresh);
        let networks = Networks::new_with_refreshed_list();
        let disks = Disks::new_with_refreshed_list();
        
        sys.refresh_cpu_all();
        sys.refresh_memory();
        
        Self { sys, networks, disks }
    }

    pub fn collect_snapshot(&mut self) -> RealTimeMetrics {
        self.sys.refresh_cpu_all();
        self.sys.refresh_memory();
        self.networks.refresh(true);
        self.disks.refresh(true);

        let cpu_data = cpu_advanced::get_advanced_cpu(&self.sys);
        let ram_data = memory_advanced::get_advanced_memory(&self.sys);
        let net_data = network_advanced::get_advanced_network(&self.networks);
        let disk_data = disk_io::get_io(&self.sys);

        // Power & Thermal
        // We initialize battery manager temporarily here.
        // This is safe because it's local to the function call and not stored in the shared struct.
        // It incurs a small overhead, but ensures thread safety for the VolatileCollector.
        let mut battery_manager = battery::Manager::new().ok();
        let power_data = power_thermal::get_power_thermal(&self.sys, &mut battery_manager);

        RealTimeMetrics {
            cpu: cpu_data,
            ram: ram_data,
            net: net_data,
            disk_read_mbps: disk_data.read,
            disk_write_mbps: disk_data.write,
            power: power_data,
        }
    }

    pub fn scan_processes_snapshot(&mut self, limit: usize, sort_by: &str) -> Vec<processes::ProcessDto> {
        self.sys.refresh_processes(ProcessesToUpdate::All, true);
        processes::scan_processes(&self.sys, limit, sort_by)
    }

    pub fn get_sys(&self) -> &System {
        &self.sys
    }
    
    pub fn get_sys_mut(&mut self) -> &mut System {
        &mut self.sys
    }
    
    pub fn get_disks(&self) -> &Disks {
        &self.disks
    }
    
    pub fn get_networks(&self) -> &Networks {
        &self.networks
    }
    
    pub fn get_networks_mut(&mut self) -> &mut Networks {
        &mut self.networks
    }
}
