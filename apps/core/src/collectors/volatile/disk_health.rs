use sysinfo::Disks;
use std::process::Command;
use serde::Serialize;
use std::collections::HashMap;
use crate::collectors::volatile::disk_speed::DiskSpeedCollector;

#[derive(Serialize, Clone, Debug)]
pub struct DiskDiagnostics {
    pub name: String,
    pub health_status: String,
    pub temperature_c: Option<u32>,
    pub read_mb_per_sec: f64,
    pub write_mb_per_sec: f64,
    pub iops_estimate: u64,
}

pub struct DiskHealthCollector {
    disks: Disks,
    // Store previous counters to calculate rate: (DiskName -> (ReadBytes, WriteBytes, Timestamp))
    prev_io: HashMap<String, (u64, u64, std::time::Instant)>,
}

impl DiskHealthCollector {
    pub fn new() -> Self {
        Self {
            disks: Disks::new_with_refreshed_list(),
            prev_io: HashMap::new(),
        }
    }

    pub fn update(&mut self) -> Vec<DiskDiagnostics> {
        self.disks.refresh(true); 
        
        // 1. Get Global Disk Speed (Native Wrapper)
        // This is efficient and accurate for the whole system.
        let speed = DiskSpeedCollector::collect();
        
        let mut diagnostics = Vec::new();
        let mut speed_distributed = false;

        for (_i, disk) in self.disks.iter().enumerate() {
            let (status, temp) = self.get_smart_data(disk.name().to_str().unwrap_or(""));

            // Distribute the global speed to the first disk (usually Main) 
            // so the frontend Aggregation logic works correctly.
            // (Frontend sums up all disks. 0 + Total = Total)
            let (r, w, io) = if !speed_distributed { 
                speed_distributed = true;
                (speed.read_mb_s, speed.write_mb_s, speed.iops) 
            } else { 
                (0.0, 0.0, 0) 
            };

            diagnostics.push(DiskDiagnostics {
                name: disk.name().to_string_lossy().to_string(),
                health_status: status,
                temperature_c: temp,
                read_mb_per_sec: r, 
                write_mb_per_sec: w, 
                iops_estimate: io,
            });
        }
        
        diagnostics
    }

    fn get_smart_data(&self, disk_name: &str) -> (String, Option<u32>) {
        // ... (Existing implementation) ...
        let output = Command::new("smartctl")
            .args(&["-j", "-H", "-A", disk_name])
            .output();

        match output {
            Ok(out) if out.status.success() => {
                let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_default();
                let status = json["smart_status"]["passed"].as_bool().unwrap_or(false);
                let temp = json["temperature"]["current"].as_u64().map(|t| t as u32);
                (if status { "Healthy".to_string() } else { "Critical".to_string() }, temp)
            },
            _ => ("Unknown".to_string(), None)
        }
    }
}
