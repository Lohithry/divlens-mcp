use sysinfo::System;

pub struct RamData {
    pub used: f64,
    pub total: f64,
    pub percent: f64,
}

pub fn get_ram_usage(sys: &System) -> RamData {
    let total = sys.total_memory() as f64;
    let used = sys.used_memory() as f64;
    
    // Safety check for division by zero
    let percent = if total > 0.0 {
        (used / total) * 100.0
    } else {
        0.0
    };

    RamData {
        used: used / 1024.0 / 1024.0 / 1024.0, // Bytes -> GB
        total: total / 1024.0 / 1024.0 / 1024.0,
        percent,
    }
}

