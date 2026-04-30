// ==================================================================================
// 🧠 DIVLENS VOLATILE PROCESS COLLECTOR
// ==================================================================================
// This module handles the "Heavy Lifting" of process scanning.
// It is designed to be called rapidly (e.g., every 3-5 seconds) without blocking.
//
// DESIGN PRINCIPLES:
// 1. Logic Separation: This file does sorting/filtering. The 'Tool' file just handles API.
// 2. Efficiency: We map to a lightweight DTO immediately to drop heavy sysinfo fields.
// 3. Volatility: This data lives in RAM. We never save this to the SQLite database.
// ==================================================================================

use sysinfo::System;
use serde::Serialize;

/// The Lightweight Data Transfer Object (DTO).
/// We send this to the Frontend/AI. It is much smaller than the full 'Process' struct.
#[derive(Serialize, Clone, Debug)]
pub struct ProcessDto {
    pub pid: u32,       // Process ID (Unique Identifier)
    pub name: String,   // User-friendly name (e.g., "chrome", "spotify")
    pub cpu: f32,       // CPU Usage Percentage (0-100+)
    pub ram_mb: f64,    // Memory Usage in Megabytes (Converted from Bytes)
    pub status: String, // "Run", "Sleep", "Idle"
}

/// Scans the system for processes, sorts them, and paginates the result.
///
/// # Arguments
/// * `sys` - Reference to the System object (must be refreshed before calling!)
/// * `limit` - How many items to return (e.g., Top 50). prevents sending 1000 processes.
/// * `sort_by` - "cpu", "ram", or "name". Defaults to "cpu".
pub fn scan_processes(sys: &System, limit: usize, sort_by: &str) -> Vec<ProcessDto> {
    // 1. MAP STEP: Convert raw OS data into our Clean DTO
    //    We iterate over all processes in the system map.
    let mut procs: Vec<ProcessDto> = sys.processes().iter().map(|(pid, p)| {
        ProcessDto {
            pid: pid.as_u32(),
            // Limit process name length to prevent overly large JSON responses
            // AWS Bedrock may reject tool results that are too large
            name: p.name().to_string_lossy()
                .chars()
                .take(50) // Limit to 50 characters
                .collect::<String>(),
            cpu: p.cpu_usage(),
            // sysinfo gives bytes. We divide by 1024*1024 for human-readable MB.
            ram_mb: ((p.memory() as f64) / 1024.0 / 1024.0)
                .min(99999.0) // Cap at reasonable maximum to prevent huge numbers
                .max(0.0),     // Ensure non-negative
            status: format!("{:?}", p.status()), // Converts enum to string
        }
    }).collect();

    // 2. SORT STEP: Enterprise Business Logic
    //    Sorts the vector in-place based on the requested strategy.
    match sort_by {
        "ram" => {
            // Sort Descending by RAM (Heavy memory users first)
            procs.sort_by(|a, b| b.ram_mb.partial_cmp(&a.ram_mb).unwrap_or(std::cmp::Ordering::Equal));
        },
        "name" => {
            // Sort Ascending by Name (Alphabetical)
            procs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        },
        _ => {
            // Default: Sort Descending by CPU (Heavy computation first)
            // Using partial_cmp because f32 cannot be normally compared (NaN issues), but unwrap is safe here.
            procs.sort_by(|a, b| b.cpu.partial_cmp(&a.cpu).unwrap_or(std::cmp::Ordering::Equal));
        }
    }

    // 3. PAGINATION STEP: Optimization
    //    Only take the requested number. This keeps the JSON payload small (<10KB vs 500KB).
    procs.into_iter().take(limit).collect()
}
