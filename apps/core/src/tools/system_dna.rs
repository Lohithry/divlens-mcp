use async_trait::async_trait;
use serde_json::Value;
use crate::modules::metrics::SystemCollector;
use crate::modules::datahub::DataHub;
use crate::modules::memory::MemoryManager;
use crate::db::DatabaseManager;
use crate::collectors::volatile::VolatileCollector;
use crate::collectors::persistent::hardware_identity::HardwareIdentityCollector;
use crate::models::system_schema::{SystemState, StorageEcosystem, PhysicalDisk};
use super::Tool;
use std::sync::{Arc, Mutex};
use sysinfo::System;

pub struct GetSystemDnaTool {
    db: Arc<DatabaseManager>,
    volatile: Arc<Mutex<VolatileCollector>>,
}

impl GetSystemDnaTool {
    pub fn new(db: Arc<DatabaseManager>, volatile: Arc<Mutex<VolatileCollector>>) -> Self { 
        Self { db, volatile } 
    }
}

#[async_trait]
impl Tool for GetSystemDnaTool {
    fn name(&self) -> &str { "get_system_dna" }
    fn description(&self) -> &str { "Returns comprehensive System DNA (Static Identity + Live Vital Signs)." }
    fn schema(&self) -> Value { 
        serde_json::json!({ 
            "type": "object", 
            "properties": {
                // TRICK: AWS Bedrock hates empty schemas. We add a dummy field.
                "_dummy": { 
                    "type": "string",
                    "description": "Ignore this parameter."
                }
            }
        }) 
    }

    async fn call(&self, _args: &Value, _col: &mut SystemCollector, _dh: &DataHub, _mem: &MemoryManager) -> anyhow::Result<Value> {
        // 1. Get Access to Volatile Data (Live Pulse)
        // We lock it briefly to get a snapshot
        let mut volatile_guard = self.volatile.lock().unwrap();
        
        // Refresh everything live
        let metrics = volatile_guard.collect_snapshot();
        let sys = volatile_guard.get_sys();
        let disks_ref = volatile_guard.get_disks();

        // 2. Get Static Data (Hardware Identity)
        // We reuse the 'sys' object from volatile collector to avoid re-scanning basic stuff
        let identity = HardwareIdentityCollector::collect_with_system(sys);
        let memory_layout = HardwareIdentityCollector::collect_memory_layout(sys);
        let graphics = HardwareIdentityCollector::collect_graphics(); 
        
        // 3. Construct Storage Ecosystem
        // Use the Disks struct from VolatileCollector
        let disks: Vec<PhysicalDisk> = disks_ref.iter().map(|d| {
            PhysicalDisk {
                name: d.name().to_string_lossy().to_string(),
                model: "Unknown".to_string(), 
                serial: "Unknown".to_string(),
                size_bytes: d.total_space(),
                interface: "Unknown".to_string(),
                is_removable: d.is_removable(),
                is_ssd: true, 
                // NEW
                file_system: d.file_system().to_string_lossy().to_string(),
                partition_scheme: "Unknown".to_string(), // Need diskutil for this usually
                
                temp_c: None,
                smart_status: "Verified".to_string(),
                bytes_read: 0, 
                bytes_written: 0,
            }
        }).collect();

        let storage = StorageEcosystem {
            disks,
            partitions: vec![],
            global_health: "Healthy".to_string(),
        };
        
        // 4. Merge into SystemState
        // Get Deep CPU Details first
        let mut cpu_final = HardwareIdentityCollector::collect_cpu_details();
        
        // Overlay Volatile CPU Stats
        if let Some(c) = sys.cpus().first() {
             // Keep brand from deep scan if possible as it's more detailed, otherwise fallback
             if cpu_final.brand.is_empty() { cpu_final.brand = c.brand().to_string(); }
             if cpu_final.vendor_id.is_empty() { cpu_final.vendor_id = c.vendor_id().to_string(); }
        }
        
        // Fix: physical_core_count() is on System (method)
        // Use sysinfo values if deep scan failed or for confirmation
        if cpu_final.cores_physical == 0 { cpu_final.cores_physical = System::physical_core_count().unwrap_or(0); }
        if cpu_final.cores_logical == 0 { cpu_final.cores_logical = sys.cpus().len(); }
        
        cpu_final.usage_global = sys.global_cpu_usage();
        cpu_final.usage_cores = sys.cpus().iter().map(|c| c.cpu_usage()).collect();
        cpu_final.freq_current = sys.cpus().first().map(|c| c.frequency()).unwrap_or(0);
        
        // M1/Apple Cache Logic (if not already set by deep scan)
        if (identity.chassis_model.contains("Mac") || identity.architecture.contains("arm64")) && cpu_final.l2_cache.is_empty() {
             cpu_final.l2_cache = HardwareIdentityCollector::get_apple_l2_cache();
             cpu_final.l3_cache = "N/A (See L2)".to_string();
        }

        // Memory Mapping
        let mut ram_final = memory_layout; // Starts with static layout
        ram_final.used_bytes = sys.used_memory();
        ram_final.free_bytes = sys.free_memory();
        ram_final.total_capacity_bytes = sys.total_memory();
        ram_final.swap_total = sys.total_swap();
        ram_final.swap_used = sys.used_swap();

        let state = SystemState {
            identity,
            cpu: cpu_final,
            memory: ram_final,
            graphics, // NEW
            storage,
            power: metrics.power,
        };

        // Serialize
        let json = serde_json::to_value(state)?;
        Ok(json)
    }
}
