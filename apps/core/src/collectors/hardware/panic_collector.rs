// ==================================================================================
// 🛡️ MACOS PANIC COLLECTOR - Kernel Panic & Hardware Report Analysis  
// ==================================================================================
// This module implements macOS kernel panic and hardware report collection with
// dual-mode monitoring: post-mortem panic analysis and live telemetry streaming
// for predictive hardware failure detection.
//
// CRITICAL DESIGN NOTES:
// - Dual approach: panic file analysis + live unified logging
// - Live streaming option for 0ms query time (background daemon)
// - Reduced time window (2 minutes) to prevent timeout issues
// - Focuses on XNU kernel corrected errors for prediction
// ==================================================================================

use crate::models::hardware_diagnostics::{
    HardwareEvent, EventSeverity, HardwareComponent, EventDetails
};
use anyhow::{Result, anyhow};
use std::process::{Command, Child, Stdio};
use std::collections::{VecDeque, HashMap};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;
use chrono;
use serde_json;

/// Background hardware monitor for real-time log streaming
pub struct MacOSHardwareMonitor {
    events: Arc<Mutex<VecDeque<HardwareEvent>>>,
    stream_handle: Option<Child>,
    is_running: bool,
}

impl MacOSHardwareMonitor {
    pub fn new() -> Self {
        Self {
            events: Arc::new(Mutex::new(VecDeque::new())),
            stream_handle: None,
            is_running: false,
        }
    }
    
    /// Start background log stream for 0ms query performance
    pub fn start_background_stream(&mut self) -> Result<()> {
        if self.is_running {
            return Ok(());
        }
        
        let events = self.events.clone();
        
        // Start log stream with hardware-focused predicate
        let mut child = Command::new("log")
            .args(&[
                "stream",
                "--predicate",
                "subsystem == 'com.apple.kernel' AND (eventMessage CONTAINS 'Machine Check' OR eventMessage CONTAINS 'thermal' OR eventMessage CONTAINS 'ECC' OR eventMessage CONTAINS 'SMC' OR eventMessage CONTAINS 'SOCD')",
                "--style", "json"
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        
        // Parse stream output in background thread
        let stdout = child.stdout.take().unwrap();
        tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                if let Ok(line) = line {
                    if let Ok(event) = parse_hardware_event_from_json(&line) {
                        if let Ok(mut events_guard) = events.lock() {
                            events_guard.push_back(event);
                            // Keep only last 1000 events to manage memory
                            if events_guard.len() > 1000 {
                                events_guard.pop_front();
                            }
                        }
                    }
                }
            }
        });
        
        self.stream_handle = Some(child);
        self.is_running = true;
        
        Ok(())
    }
    
    /// Get recent events from memory (instant retrieval)
    pub fn get_recent_events(&self) -> Vec<HardwareEvent> {
        if let Ok(events) = self.events.lock() {
            events.iter().cloned().collect()
        } else {
            Vec::new()
        }
    }
    
    /// Stop background streaming
    pub fn stop_background_stream(&mut self) {
        if let Some(mut child) = self.stream_handle.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.is_running = false;
    }
}

impl Drop for MacOSHardwareMonitor {
    fn drop(&mut self) {
        self.stop_background_stream();
    }
}

/// Main macOS hardware event collection function
pub async fn collect() -> Result<Vec<HardwareEvent>> {
    #[cfg(target_os = "macos")]
    {
        collect_macos_hardware_events().await
    }
    
    #[cfg(not(target_os = "macos"))]
    {
        Ok(Vec::new())
    }
}

#[cfg(target_os = "macos")]
async fn collect_macos_hardware_events() -> Result<Vec<HardwareEvent>> {
    let mut events = Vec::new();
    let start_time = Instant::now();
    
    // Collect from both sources concurrently
    let (panic_events, live_events) = tokio::join!(
        collect_panic_reports(),
        collect_live_telemetry()
    );
    
    // Merge results
    if let Ok(mut panic_evts) = panic_events {
        events.append(&mut panic_evts);
    }
    
    if let Ok(mut live_evts) = live_events {
        events.append(&mut live_evts);
    }
    
    // Performance monitoring
    let collection_time = start_time.elapsed();
    if collection_time > Duration::from_millis(200) {
        eprintln!("[PANIC] Collection took {}ms (target: <200ms)", collection_time.as_millis());
    }
    
    Ok(events)
}

#[cfg(target_os = "macos")]
async fn collect_panic_reports() -> Result<Vec<HardwareEvent>> {
    let mut events = Vec::new();
    
    let panic_dir = "/Library/Logs/DiagnosticReports/";
    if !Path::new(panic_dir).exists() {
        return Ok(events);
    }
    
    // Read directory and find recent panic files
    let entries = fs::read_dir(panic_dir)?;
    let mut panic_files = Vec::new();
    
    for entry in entries.flatten() {
        if let Some(filename) = entry.file_name().to_str() {
            if filename.ends_with(".panic") {
                if let Ok(metadata) = entry.metadata() {
                    if let Ok(modified) = metadata.modified() {
                        // Only process panic files from last 7 days
                        if modified.elapsed().unwrap_or(Duration::MAX) < Duration::from_secs(7 * 24 * 3600) {
                            panic_files.push((entry.path(), modified));
                        }
                    }
                }
            }
        }
    }
    
    // Sort by modification time (newest first) and limit to 10 files
    panic_files.sort_by(|a, b| b.1.cmp(&a.1));
    panic_files.truncate(10);
    
    // Parse each panic file
    for (panic_path, modified_time) in panic_files {
        if let Ok(content) = fs::read_to_string(&panic_path) {
            if let Ok(mut panic_events) = parse_panic_report(&content, modified_time) {
                events.append(&mut panic_events);
            }
        }
    }
    
    Ok(events)
}

#[cfg(target_os = "macos")]
fn parse_panic_report(content: &str, timestamp: std::time::SystemTime) -> Result<Vec<HardwareEvent>> {
    let mut events = Vec::new();
    
    let iso_timestamp = timestamp
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| chrono::DateTime::from_timestamp(d.as_secs() as i64, 0))
        .unwrap_or(None)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
    
    // Look for hardware-related panic signatures
    if content.contains("Machine Check") || content.contains("MCA") {
        events.push(HardwareEvent {
            timestamp: iso_timestamp.clone(),
            severity: EventSeverity::Fatal,
            component: HardwareComponent::CPU,
            event_type: "machine_check_panic".to_string(),
            details: EventDetails {
                count: Some(1),
                bank: extract_mca_bank(&content),
                error_type: Some("machine_check".to_string()),
                address: extract_memory_address(&content),
                context: Some("Kernel panic due to machine check".to_string()),
            },
            source_id: Some("panic_report".to_string()),
        });
    }
    
    if content.contains("SMC") && (content.contains("PANIC") || content.contains("ERROR")) {
        let component = if content.contains("voltage") || content.contains("power") {
            HardwareComponent::PowerSupply
        } else if content.contains("thermal") || content.contains("temperature") {
            HardwareComponent::Cooling
        } else {
            HardwareComponent::Motherboard
        };
        
        events.push(HardwareEvent {
            timestamp: iso_timestamp.clone(),
            severity: EventSeverity::Critical,
            component,
            event_type: "smc_error".to_string(),
            details: EventDetails {
                count: Some(1),
                bank: None,
                error_type: Some("smc_error".to_string()),
                address: None,
                context: Some("SMC (System Management Controller) error".to_string()),
            },
            source_id: Some("panic_report".to_string()),
        });
    }
    
    if content.contains("SOCD") {
        events.push(HardwareEvent {
            timestamp: iso_timestamp,
            severity: EventSeverity::Critical,
            component: HardwareComponent::CPU,
            event_type: "socd_report".to_string(),
            details: EventDetails {
                count: Some(1),
                bank: None,
                error_type: Some("socd".to_string()),
                address: None,
                context: Some("Silicon on Chip Debug report".to_string()),
            },
            source_id: Some("panic_report".to_string()),
        });
    }
    
    Ok(events)
}

#[cfg(target_os = "macos")]
async fn collect_live_telemetry() -> Result<Vec<HardwareEvent>> {
    let mut events = Vec::new();
    
    // Use reduced time window (2 minutes) to prevent timeout
    let output = Command::new("log")
        .args(&[
            "show",
            "--style", "json",
            "--last", "2m", // Reduced from 10m to prevent timeout
            "--predicate",
            "subsystem == 'com.apple.kernel' AND (eventMessage CONTAINS 'Machine Check' OR eventMessage CONTAINS 'thermal' OR eventMessage CONTAINS 'ECC' OR eventMessage CONTAINS 'SMC')"
        ])
        .output()?;
    
    if !output.status.success() {
        return Err(anyhow!("macOS log show command failed"));
    }
    
    let json_str = String::from_utf8_lossy(&output.stdout);
    
    // Apple JSON sometimes includes preamble text. Find start of array.
    let start = json_str.find('[').unwrap_or(0);
    
    if let Ok(entries) = serde_json::from_str::<Vec<serde_json::Value>>(&json_str[start..]) {
        for entry in entries.into_iter().take(50) { // Limit to prevent overflow
            if let Ok(event) = parse_unified_log_entry(&entry) {
                events.push(event);
            }
        }
    }
    
    Ok(events)
}

#[cfg(target_os = "macos")]
fn parse_unified_log_entry(entry: &serde_json::Value) -> Result<HardwareEvent> {
    let timestamp = entry["timestamp"]
        .as_str()
        .map(|s| s.to_string())
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
    
    let message = entry["eventMessage"]
        .as_str()
        .unwrap_or("");
    
    let process_name = entry["processImagePath"]
        .as_str()
        .map(|path| path.split('/').last().unwrap_or("kernel").to_string())
        .unwrap_or_else(|| "kernel".to_string());
    
    // Determine component and severity from message content
    let (component, severity, event_type) = if message.contains("Machine Check") {
        (HardwareComponent::CPU, EventSeverity::Warning, "machine_check_corrected")
    } else if message.contains("thermal") || message.contains("temperature") {
        let severity = if message.to_lowercase().contains("throttl") {
            EventSeverity::Warning // Thermal throttling is predictive
        } else {
            EventSeverity::Info
        };
        (HardwareComponent::Cooling, severity, "thermal_event")
    } else if message.contains("ECC") {
        (HardwareComponent::Memory, EventSeverity::Warning, "ecc_corrected")
    } else if message.contains("SMC") {
        let component = if message.contains("power") || message.contains("voltage") {
            HardwareComponent::PowerSupply
        } else {
            HardwareComponent::Motherboard
        };
        (component, EventSeverity::Warning, "smc_event")
    } else {
        (HardwareComponent::Unknown, EventSeverity::Info, "kernel_event")
    };
    
    Ok(HardwareEvent {
        timestamp,
        severity,
        component,
        event_type: event_type.to_string(),
        details: EventDetails {
            count: Some(1),
            bank: None,
            error_type: Some(event_type.to_string()),
            address: None,
            context: Some(message.to_string()),
        },
        source_id: Some(process_name),
    })
}

/// Parse hardware event from JSON log stream
fn parse_hardware_event_from_json(json_line: &str) -> Result<HardwareEvent> {
    let entry: serde_json::Value = serde_json::from_str(json_line)?;
    parse_unified_log_entry(&entry)
}

/// Extract MCA bank number from panic report text
fn extract_mca_bank(content: &str) -> Option<u8> {
    // Look for patterns like "MCA bank 4" or "Bank: 4"
    if let Some(start) = content.find("bank") {
        let after_bank = &content[start + 4..];
        if let Some(end) = after_bank.find(|c: char| !c.is_ascii_digit() && c != ' ') {
            if let Ok(bank) = after_bank[..end].trim().parse::<u8>() {
                return Some(bank);
            }
        }
    }
    None
}

/// Extract memory address from panic report
fn extract_memory_address(content: &str) -> Option<String> {
    // Look for hex addresses in panic reports
    for line in content.lines() {
        if line.contains("0x") {
            // Extract hex addresses
            let words: Vec<&str> = line.split_whitespace().collect();
            for word in words {
                if word.starts_with("0x") && word.len() > 8 {
                    return Some(word.to_string());
                }
            }
        }
    }
    None
}

/// Analyze macOS hardware event patterns
pub fn analyze_macos_patterns(events: &[HardwareEvent]) -> HashMap<String, u64> {
    let mut patterns = HashMap::new();
    
    for event in events {
        if event.is_predictive_signal() {
            let pattern_key = format!("{:?}_{}", event.component, event.event_type);
            *patterns.entry(pattern_key).or_insert(0) += event.details.count.unwrap_or(1);
        }
    }
    
    patterns
}

/// Check if system has recent kernel panics
pub fn has_recent_panics() -> bool {
    let panic_dir = "/Library/Logs/DiagnosticReports/";
    
    if let Ok(entries) = fs::read_dir(panic_dir) {
        for entry in entries.flatten() {
            if let Some(filename) = entry.file_name().to_str() {
                if filename.ends_with(".panic") {
                    if let Ok(metadata) = entry.metadata() {
                        if let Ok(modified) = metadata.modified() {
                            // Check for panics in last 24 hours
                            if modified.elapsed().unwrap_or(Duration::MAX) < Duration::from_secs(24 * 3600) {
                                return true;
                            }
                        }
                    }
                }
            }
        }
    }
    
    false
}