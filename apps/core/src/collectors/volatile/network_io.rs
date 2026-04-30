use sysinfo::Networks;

pub struct NetTraffic {
    pub down: f64,
    pub up: f64,
}

pub fn get_traffic(networks: &Networks) -> NetTraffic {
    let mut total_down = 0;
    let mut total_up = 0;
    
    for (_name, data) in networks {
        total_down += data.received(); // Bytes since last refresh
        total_up += data.transmitted();
    }
    
    // Note: sysinfo returns bytes *since last refresh*.
    // If we refresh every 1s, this is essentially Bps.
    // Converting to KBps (Kilobytes per second)
    NetTraffic {
        down: (total_down as f64) / 1024.0, 
        up: (total_up as f64) / 1024.0,
    }
}

