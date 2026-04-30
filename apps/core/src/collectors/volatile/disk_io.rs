use sysinfo::System;

pub struct DiskTraffic {
    pub read: f64,
    pub write: f64,
}

pub fn get_io(_sys: &System) -> DiskTraffic {
    // sysinfo disk I/O support varies by OS. 
    // For MVP, we might return 0.0 or implement a specific Linux/Mac reader here later.
    // This placeholder keeps the architecture clean.
    DiskTraffic { read: 0.0, write: 0.0 }
}

