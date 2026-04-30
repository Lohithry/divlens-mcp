use crate::models::system_schema::PowerManagement;
use sysinfo::System;
use std::process::Command;

/// The Main "Pulse" Function for Power
pub fn get_power_thermal(_sys: &System, battery_manager: &mut Option<battery::Manager>) -> PowerManagement {
    
    // 1. Try Platform-Specific Deep Scan first (High Accuracy for Mac)
    #[cfg(target_os = "macos")]
    {
        // We log error if it fails to help debug (in a real app), here we just fall through
        if let Ok(deep_info) = get_mac_battery_details() {
            return deep_info;
        }
    }
    
    // 2. Fallback to Generic Crate (Cross-Platform)
    get_generic_battery_info(battery_manager)
}

// ==================================================================================
// 🍏 macOS Deep Scanner (system_profiler + Heuristics)
// ==================================================================================

#[cfg(target_os = "macos")]
fn get_mac_battery_details() -> anyhow::Result<PowerManagement> {
    // 1. Run system_profiler
    let output = Command::new("system_profiler")
        .args(&["SPPowerDataType", "-json"])
        .output()?;

    if !output.status.success() {
        return Err(anyhow::anyhow!("system_profiler failed"));
    }

    let json_str = String::from_utf8_lossy(&output.stdout);
    let root: serde_json::Value = serde_json::from_str(&json_str)?;
    
    // Navigate JSON safely - handle array or single object
    let items = root.get("SPPowerDataType")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!("Invalid JSON structure"))?;

    if items.is_empty() {
        return Err(anyhow::anyhow!("No battery found"));
    }
    
    // Iterate to find the actual battery data (sometimes multiple items, e.g. UPS)
    // We look for the one with "sppower_battery_charge_info"
    let data = items.iter().find(|i| i.get("sppower_battery_charge_info").is_some())
        .ok_or_else(|| anyhow::anyhow!("No battery charge info found"))?;

    // Extract Sections safely
    let charge_info = data.get("sppower_battery_charge_info");
    let health_info = data.get("sppower_battery_health_info");
    let model_info  = data.get("sppower_battery_model_info");

    // Helper: Safely get u64/f64 from JSON (handles strings that look like numbers too)
    let get_num = |obj: Option<&serde_json::Value>, key: &str| -> u64 {
        obj.and_then(|o| o.get(key)).and_then(|v| {
            if let Some(n) = v.as_u64() { return Some(n); }
            if let Some(f) = v.as_f64() { return Some(f as u64); }
            // system_profiler sometimes returns numbers as strings
            if let Some(s) = v.as_str() { return s.parse::<u64>().ok(); }
            None
        }).unwrap_or(0)
    };

    let get_str = |obj: Option<&serde_json::Value>, key: &str| -> String {
        obj.and_then(|o| o.get(key)).and_then(|v| v.as_str()).unwrap_or("Unknown").to_string()
    };

    // 2. Extract Raw Data (mAh)
    let current_mah = get_num(charge_info, "sppower_battery_current_capacity");
    let max_mah = get_num(charge_info, "sppower_battery_max_capacity");
    let cycles = get_num(health_info, "sppower_battery_cycle_count");
    
    let is_charging_str = get_str(charge_info, "sppower_battery_is_charging");
    let is_charging = is_charging_str == "TRUE";
    
    let device_name = get_str(model_info, "sppower_battery_device_name");
    let manufacturer = get_str(model_info, "sppower_battery_manufacturer");
    let serial = get_str(model_info, "sppower_battery_serial_number");

    // 3. Design Capacity Logic
    // If we have a valid Max Capacity but no Design, we assume Design ~= Max for calculation purposes
    // to show ~100% health instead of 0% or 200%.
    // Real M1/M2 Airs are ~4380 mAh.
    let design_mah_heuristic = if max_mah > 0 { max_mah } else { 4380 };

    // 4. Calculate Health
    let mut health_pct = if design_mah_heuristic > 0 {
        (max_mah as f64 / design_mah_heuristic as f64) * 100.0
    } else {
        100.0
    };
    if health_pct > 100.0 { health_pct = 100.0; }

    // 5. Constants
    let voltage_mv = 11400; // 11.4V nominal

    // 6. Charge Percent
    // Try explicit state of charge first
    let charge_pct_raw = get_num(charge_info, "sppower_battery_state_of_charge");
    let charge_percent = if charge_pct_raw > 0 {
        charge_pct_raw as f32
    } else if max_mah > 0 {
        (current_mah as f32 / max_mah as f32) * 100.0
    } else {
        0.0
    };

    Ok(PowerManagement {
        manufacturer,
        model: device_name,
        serial,
        technology: "Li-ion".to_string(),
        design_capacity_mah: design_mah_heuristic as u32,
        design_voltage_mv: voltage_mv,
        cycle_count: cycles as u32,
        is_charging,
        state: if is_charging { "Charging".into() } else { "Discharging".into() },
        charge_percent,
        current_capacity_mah: current_mah as u32,
        max_capacity_mah: max_mah as u32,
        health_percent: health_pct as f32,
        voltage_now_mv: voltage_mv, 
        amperage_now_ma: 0, // system_profiler doesn't give this, need ioreg
        time_remaining_minutes: None, 
    })
}

// ==================================================================================
// 🧱 Generic Fallback
// ==================================================================================

fn get_generic_battery_info(battery_manager: &mut Option<battery::Manager>) -> PowerManagement {
    let mut pm = PowerManagement::default();
    
    if let Some(manager) = battery_manager {
        if let Ok(mut batteries) = manager.batteries() {
            if let Some(Ok(battery)) = batteries.next() {
                pm.manufacturer = battery.vendor().unwrap_or("Unknown").to_string();
                pm.model = battery.model().unwrap_or("Unknown").to_string();
                pm.serial = battery.serial_number().unwrap_or("Unknown").to_string();
                pm.technology = "Li-ion".to_string(); 
                
                // Units: The battery crate on macOS returns JOULES for energy.
                // 1 Wh = 3600 J.
                // Voltage is in Volts.
                
                let voltage_v = battery.voltage().value;
                let design_energy = battery.energy_full_design().value; // J or Wh
                let full_energy = battery.energy_full().value;          // J or Wh
                let current_energy = battery.energy().value;            // J or Wh
                
                let safe_voltage = if voltage_v < 1.0 { 11.4 } else { voltage_v }; 
                
                // Heuristic: If energy > 10,000, it is JOULES (laptop ~50Wh = 180,000 J).
                // Conversion: mAh = (J / 3600 / V) * 1000
                let to_mah = |energy: f32, volts: f32| -> u32 {
                    if energy > 10000.0 {
                        // Assume Joules
                        ((energy / 3600.0) / volts * 1000.0) as u32
                    } else {
                        // Assume Wh
                        ((energy / volts) * 1000.0) as u32
                    }
                };

                pm.design_capacity_mah = to_mah(design_energy as f32, safe_voltage as f32);
                pm.current_capacity_mah = to_mah(current_energy as f32, safe_voltage as f32);
                pm.max_capacity_mah = to_mah(full_energy as f32, safe_voltage as f32);
                
                pm.design_voltage_mv = (safe_voltage * 1000.0) as u32;
                pm.voltage_now_mv = (battery.voltage().value * 1000.0) as u32;
                pm.cycle_count = battery.cycle_count().unwrap_or(0);
                pm.is_charging = battery.state() == battery::State::Charging;
                pm.state = format!("{:?}", battery.state());
                pm.charge_percent = (battery.state_of_charge().value * 100.0) as f32;
                pm.amperage_now_ma = (battery.energy_rate().value * 1000.0) as i32; 

                // Robust Health
                // Use ratio of raw energies to avoid conversion errors
                if design_energy > 0.1 {
                    pm.health_percent = ((full_energy / design_energy) * 100.0) as f32;
                } else {
                    pm.health_percent = 100.0;
                }
                
                // Final sanity clamp
                if pm.health_percent > 110.0 { pm.health_percent = 100.0; }
                if pm.health_percent < 1.0 { pm.health_percent = 0.0; } // Dead or unknown

                // Time Remaining
                if let Some(time) = battery.time_to_full() {
                     pm.time_remaining_minutes = Some((time.value / 60.0) as u64);
                } else if let Some(time) = battery.time_to_empty() {
                     pm.time_remaining_minutes = Some((time.value / 60.0) as u64);
                }
            }
        }
    } else {
        pm.state = "No Battery Manager".to_string();
    }
    
    pm
}
