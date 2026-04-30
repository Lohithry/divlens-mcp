use sysinfo::System;

#[derive(Debug, Clone, serde::Serialize)]
pub struct MemAdvancedMetrics {
    pub total_gb: f64,
    pub used_gb: f64,
    pub cached_gb: f64, // "Good" usage
    pub swap_total_gb: f64,
    pub swap_used_gb: f64,
    pub is_thrashing: bool, // The Diagnosis
}

pub fn get_advanced_memory(sys: &System) -> MemAdvancedMetrics {
    let total = sys.total_memory() as f64;
    let used = sys.used_memory() as f64;
    
    // sysinfo calculates "available" which includes cache.
    // Cached = Total - Available - Used (Rough approximation)
    // Note: total_memory(), available_memory(), used_memory() return u64 bytes.
    let available = sys.available_memory() as f64;
    
    // Safety check for underflow, though logic implies total >= available + used usually
    let cached = (total - available - used).max(0.0); 

    let swap_total = sys.total_swap() as f64;
    let swap_used = sys.used_swap() as f64;

    // DIAGNOSIS: Is it Thrashing?
    // Rule: If Swap is > 50% full AND RAM is > 90% full, we are likely thrashing.
    let is_high_swap = swap_total > 0.0 && (swap_used / swap_total) > 0.5;
    let is_high_ram = total > 0.0 && (used / total) > 0.90;
    let is_thrashing = is_high_swap && is_high_ram;

    MemAdvancedMetrics {
        total_gb: bytes_to_gb(total),
        used_gb: bytes_to_gb(used),
        cached_gb: bytes_to_gb(cached),
        swap_total_gb: bytes_to_gb(swap_total),
        swap_used_gb: bytes_to_gb(swap_used),
        is_thrashing,
    }
}

fn bytes_to_gb(b: f64) -> f64 {
    b / 1024.0 / 1024.0 / 1024.0
}

