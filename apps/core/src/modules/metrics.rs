// This module provides types and a collector for gathering system resource metrics
// using the sysinfo crate. It provides summary information about CPU usage, memory 
// utilization, and disk statistics. Data structures are serializable for easy use in 
// APIs or inter-process communication.
use serde::Serialize;
use sysinfo::{
    CpuRefreshKind, Disks, MemoryRefreshKind, RefreshKind, System,
};

// SystemMetrics structure describes a snapshot of system resource usage.
// It includes CPU, RAM, and disk information in a summarized format.
// This struct can be easily serialized (e.g. into JSON) for reporting or transport.
#[derive(Serialize)]
pub struct SystemMetrics {
    pub cpu_usage: f32,
    pub ram_total: u64,
    pub ram_used: u64,
    pub ram_percent: f32,
    pub disks: Vec<DiskInfo>,
}

// DiskInfo summarizes key attributes of a single disk or partition.
// It tracks the device name, mount location, total and available space.
// Marked serializable for convenient export.
#[derive(Serialize)]
pub struct DiskInfo {
    pub name: String,
    pub mount_point: String,
    pub total_space: u64,
    pub available_space: u64,
}

// SystemCollector is a helper for repeatedly gathering and refreshing system metrics.
// It wraps a sysinfo::System object, letting you update and get metrics conveniently.
pub struct SystemCollector {
    sys: System,
    disks: Disks,
}
/// SystemCollector is a helper for repeatedly gathering and refreshing system metrics.
/// It wraps a sysinfo::System object, letting you update and get metrics conveniently.
impl SystemCollector {
    /// Creates a new SystemCollector instance, initializing the sysinfo::System object.
    /// This object is used to gather system metrics.
    pub fn new() -> Self {
        // Initialize with specific requirements to save startup time
        let refresh = RefreshKind::nothing()
            .with_cpu(CpuRefreshKind::everything())
            .with_memory(MemoryRefreshKind::everything());
            
        Self {
            sys: System::new_with_specifics(refresh),
            disks: Disks::new_with_refreshed_list(),
        }
    }

    /// get_metrics is a method that returns the current system metrics.
    /// It refreshes the system data and calculates the CPU, RAM, and disk usage.
    /// It returns a SystemMetrics struct containing the current system metrics.
    pub fn get_metrics(&mut self) -> SystemMetrics {
        // 1. Refresh Data
        // We must pass specific flags to tell it exactly what to update
        self.sys.refresh_specifics(
            RefreshKind::nothing()
                .with_cpu(CpuRefreshKind::everything())
                .with_memory(MemoryRefreshKind::everything())
        );
        
        // Refresh disks (true = verify list matches reality)
        self.disks.refresh(true);

        // 2. Calculate CPU
        // Uses the new direct method instead of global_cpu_info()
        let cpu_usage = self.sys.global_cpu_usage();

        // 3. Calculate RAM
        let ram_total = self.sys.total_memory();
        let ram_used = self.sys.used_memory();
        
        // Safety check to prevent division by zero
        let ram_percent = if ram_total > 0 {
            (ram_used as f32 / ram_total as f32) * 100.0
        } else {
            0.0
        };

        // 4. Gather Disks
        let disks = self.disks.iter().map(|disk| DiskInfo {
            name: disk.name().to_string_lossy().to_string(),
            mount_point: disk.mount_point().to_string_lossy().to_string(),
            total_space: disk.total_space(),
            available_space: disk.available_space(),
        }).collect();

        SystemMetrics {
            cpu_usage,
            ram_total,
            ram_used,
            ram_percent,
            disks,
        }
    }

    /// Expose the inner System object for advanced collectors
    pub fn get_sys(&mut self) -> &mut System {
        &mut self.sys
    }
}