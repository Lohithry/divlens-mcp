// ==================================================================================
// 🛡️ LINUX MCE COLLECTOR - Machine Check Exception & RAS Events
// ==================================================================================  
// This module implements Linux Machine Check Exception (MCE) and RAS event collection
// using modern rasdaemon (not deprecated mcelog) for predictive hardware analysis.
//
// CRITICAL DESIGN NOTES:
// - Uses rasdaemon SQLite database (modern approach)
// - INCLUDES "Corrected" errors for predictive analysis (never filter them out)
// - Falls back to journald parsing if rasdaemon unavailable
// - Queries EDAC tracing events for memory error detection
// ==================================================================================

use crate::models::hardware_diagnostics::{
    HardwareEvent, EventSeverity, HardwareComponent, EventDetails
};
use anyhow::{Result, anyhow};
use std::process::Command;
use std::time::{Duration, Instant};
use std::fs;
use chrono;
use serde_json;

/// Main Linux MCE/RAS event collection function
pub async fn collect() -> Result<Vec<HardwareEvent>> {
    #[cfg(target_os = "linux")]
    {
        collect_linux_mce_events().await
    }
    
    #[cfg(not(target_os = "linux"))]
    {
        Ok(Vec::new())
    }
}

#[cfg(target_os = "linux")]
async fn collect_linux_mce_events() -> Result<Vec<HardwareEvent>> {
    let mut events = Vec::new();
    let start_time = Instant::now();
    
    // Try rasdaemon first (modern approach)
    match collect_rasdaemon_events().await {
        Ok(mut ras_events) => {
            events.append(&mut ras_events);
        },
        Err(e) => {
            eprintln!("[MCE] rasdaemon collection failed: {}, trying fallback", e);
            
            // Fallback to journald parsing
            if let Ok(mut journal_events) = collect_journald_mce_events().await {
                events.append(&mut journal_events);
            }
            
            // Also try EDAC tracing if available
            if let Ok(mut edac_events) = collect_edac_events().await {
                events.append(&mut edac_events);
            }
        }
    }
    
    // Performance monitoring
    let collection_time = start_time.elapsed();
    if collection_time > Duration::from_millis(100) {
        eprintln!("[MCE] Collection took {}ms (target: <100ms)", collection_time.as_millis());
    }
    
    Ok(events)
}

#[cfg(target_os = "linux")]
async fn collect_rasdaemon_events() -> Result<Vec<HardwareEvent>> {
    let mut events = Vec::new();
    
    // Check if rasdaemon service is running
    let status_output = Command::new("systemctl")
        .args(&["is-active", "rasdaemon"])
        .output()?;
    
    if !status_output.status.success() {
        return Err(anyhow!("rasdaemon service not active"));
    }
    
    // Query rasdaemon SQLite database
    // CRITICAL: Include ALL errors for predictive analysis (don't filter Corrected)
    let db_path = "/var/lib/rasdaemon/ras-mc_event.db";
    
    if !std::path::Path::new(db_path).exists() {
        return Err(anyhow!("rasdaemon database not found at {}", db_path));
    }
    
    // Query recent events (last 24 hours, ordered by timestamp)
    let query = "SELECT timestamp, error_type, error_count, mc_index, top_layer, middle_layer, lower_layer, address, grain, syndrome, driver_detail FROM mc_event WHERE datetime(timestamp) >= datetime('now', '-24 hours') ORDER BY timestamp DESC LIMIT 100";
    
    let output = Command::new("sqlite3")
        .args(&[db_path, "-header", "-csv", query])
        .output()?;
    
    if !output.status.success() {
        return Err(anyhow!("sqlite3 query failed: {}", String::from_utf8_lossy(&output.stderr)));
    }
    
    let csv_data = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = csv_data.lines().collect();
    
    if lines.len() <= 1 {
        return Ok(events); // No events (header only)
    }
    
    // Parse CSV data (skip header)
    for line in lines.iter().skip(1) {
        if let Ok(event) = parse_rasdaemon_csv_line(line) {
            events.push(event);
        }
    }
    
    Ok(events)
}

#[cfg(target_os = "linux")]
fn parse_rasdaemon_csv_line(line: &str) -> Result<HardwareEvent> {
    let fields: Vec<&str> = line.split(',').collect();
    
    if fields.len() < 11 {
        return Err(anyhow!("Invalid CSV line: insufficient fields"));
    }
    
    let timestamp = fields[0].trim_matches('"');
    let error_type = fields[1].trim_matches('"');
    let error_count: u64 = fields[2].trim_matches('"').parse().unwrap_or(1);
    let mc_index = fields[3].trim_matches('"');
    let top_layer = fields[4].trim_matches('"');
    let middle_layer = fields[5].trim_matches('"');
    let lower_layer = fields[6].trim_matches('"');
    let address = fields[7].trim_matches('"');
    let grain = fields[8].trim_matches('"');
    let syndrome = fields[9].trim_matches('"');
    let driver_detail = fields[10].trim_matches('"');
    
    // Determine component based on error location
    let component = if top_layer.contains("memory") || middle_layer.contains("memory") {
        HardwareComponent::Memory
    } else if top_layer.contains("cpu") || middle_layer.contains("cpu") {
        HardwareComponent::CPU
    } else if lower_layer.contains("pci") || driver_detail.contains("pci") {
        HardwareComponent::PCIe
    } else {
        HardwareComponent::Unknown
    };
    
    // Map error type to severity
    let severity = match error_type.to_lowercase().as_str() {
        s if s.contains("corrected") => EventSeverity::Warning, // Predictive signal
        s if s.contains("uncorrected") => EventSeverity::Error,
        s if s.contains("fatal") => EventSeverity::Fatal,
        s if s.contains("recoverable") => EventSeverity::Warning,
        _ => EventSeverity::Error,
    };
    
    // Parse MC bank from mc_index if available
    let bank = if !mc_index.is_empty() {
        mc_index.parse::<u8>().ok()
    } else {
        None
    };
    
    // Format timestamp to ISO 8601
    let iso_timestamp = crate::collectors::hardware::utils::parse_timestamp(timestamp)
        .unwrap_or_else(|_| chrono::Utc::now().to_rfc3339());
    
    Ok(HardwareEvent {
        timestamp: iso_timestamp,
        severity,
        component,
        event_type: format!("ras_{}", error_type),
        details: EventDetails {
            count: Some(error_count),
            bank,
            error_type: Some(error_type.to_string()),
            address: if !address.is_empty() { Some(address.to_string()) } else { None },
            context: Some(format!("Layer: {}/{}/{}, Grain: {}", 
                                top_layer, middle_layer, lower_layer, grain)),
        },
        source_id: Some(format!("rasdaemon_{}", mc_index)),
    })
}

#[cfg(target_os = "linux")]
async fn collect_journald_mce_events() -> Result<Vec<HardwareEvent>> {
    let mut events = Vec::new();
    
    // Query journald for MCE-related events (last 24 hours)
    let output = Command::new("journalctl")
        .args(&[
            "--since", "24 hours ago",
            "--priority", "err",
            "--grep", "Machine Check|MCE|Hardware Error|EDAC|ECC",
            "--output", "json",
            "--no-pager",
            "--lines", "50"
        ])
        .output()?;
    
    if !output.status.success() {
        return Err(anyhow!("journalctl query failed"));
    }
    
    let json_data = String::from_utf8_lossy(&output.stdout);
    
    for line in json_data.lines() {
        if let Ok(event) = parse_journald_mce_line(line) {
            events.push(event);
        }
    }
    
    Ok(events)
}

#[cfg(target_os = "linux")]
fn parse_journald_mce_line(json_line: &str) -> Result<HardwareEvent> {
    let json: serde_json::Value = serde_json::from_str(json_line)?;
    
    let timestamp = json["__REALTIME_TIMESTAMP"]
        .as_str()
        .and_then(|ts| ts.parse::<i64>().ok())
        .and_then(|micros| chrono::DateTime::from_timestamp_micros(micros))
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
    
    let message = json["MESSAGE"].as_str().unwrap_or("");
    let syslog_id = json["SYSLOG_IDENTIFIER"].as_str().unwrap_or("kernel");
    
    // Determine component from message content
    let component = if message.contains("memory") || message.contains("ECC") || message.contains("DIMM") {
        HardwareComponent::Memory
    } else if message.contains("CPU") || message.contains("processor") {
        HardwareComponent::CPU
    } else if message.contains("PCI") || message.contains("PCIe") {
        HardwareComponent::PCIe
    } else {
        HardwareComponent::Unknown
    };
    
    // Determine severity
    let severity = if message.to_lowercase().contains("corrected") {
        EventSeverity::Warning // Predictive signal
    } else if message.to_lowercase().contains("uncorrected") || message.to_lowercase().contains("fatal") {
        EventSeverity::Error
    } else {
        EventSeverity::Warning
    };
    
    // Extract bank information if present
    let bank = if message.contains("bank") {
        // Try to extract "bank N" pattern
        if let Some(start) = message.find("bank") {
            let bank_part = &message[start + 4..];
            if let Some(space_pos) = bank_part.find(' ') {
                bank_part[..space_pos].trim().parse::<u8>().ok()
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };
    
    Ok(HardwareEvent {
        timestamp,
        severity,
        component,
        event_type: "journald_mce".to_string(),
        details: EventDetails {
            count: Some(1),
            bank,
            error_type: Some("mce_event".to_string()),
            address: None,
            context: Some(message.to_string()),
        },
        source_id: Some(syslog_id.to_string()),
    })
}

#[cfg(target_os = "linux")]
async fn collect_edac_events() -> Result<Vec<HardwareEvent>> {
    let mut events = Vec::new();
    
    // Check if EDAC tracing is available
    let edac_events_path = "/sys/kernel/debug/tracing/events/ras";
    
    if !std::path::Path::new(edac_events_path).exists() {
        return Err(anyhow!("EDAC tracing not available"));
    }
    
    // Try to read recent EDAC events from trace buffer
    let trace_path = "/sys/kernel/debug/tracing/trace";
    
    if let Ok(trace_content) = fs::read_to_string(trace_path) {
        for line in trace_content.lines().rev().take(50) {
            if line.contains("ras:") || line.contains("mc_event") {
                if let Ok(event) = parse_edac_trace_line(line) {
                    events.push(event);
                }
            }
        }
    }
    
    Ok(events)
}

#[cfg(target_os = "linux")]
fn parse_edac_trace_line(line: &str) -> Result<HardwareEvent> {
    // Parse EDAC trace line format
    // Example: "kworker/0:1-123 [000] .... 12345.678: ras:mc_event: mce_record..."
    
    let timestamp = chrono::Utc::now().to_rfc3339(); // Use current time for trace events
    
    let component = if line.contains("memory") || line.contains("dimm") {
        HardwareComponent::Memory
    } else if line.contains("cpu") {
        HardwareComponent::CPU
    } else {
        HardwareComponent::Unknown
    };
    
    let severity = if line.contains("corrected") {
        EventSeverity::Warning
    } else {
        EventSeverity::Error
    };
    
    Ok(HardwareEvent {
        timestamp,
        severity,
        component,
        event_type: "edac_trace".to_string(),
        details: EventDetails {
            count: Some(1),
            bank: None,
            error_type: Some("edac_event".to_string()),
            address: None,
            context: Some(line.to_string()),
        },
        source_id: Some("edac_tracer".to_string()),
    })
}

/// Analyze MCE event patterns for risk assessment
pub fn analyze_mce_patterns(events: &[HardwareEvent]) -> std::collections::HashMap<String, u64> {
    let mut patterns = std::collections::HashMap::new();
    
    for event in events {
        if event.is_predictive_signal() {
            let pattern_key = format!("{:?}_{}", event.component, event.event_type);
            *patterns.entry(pattern_key).or_insert(0) += event.details.count.unwrap_or(1);
        }
    }
    
    patterns
}

/// Check if rasdaemon is available and configured
pub fn check_rasdaemon_status() -> bool {
    Command::new("systemctl")
        .args(&["is-active", "--quiet", "rasdaemon"])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}