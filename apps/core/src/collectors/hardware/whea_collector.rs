// ==================================================================================
// 🛡️ WINDOWS WHEA COLLECTOR - Hardware Error Architecture Events
// ==================================================================================
// This module implements native Windows Hardware Error Architecture (WHEA) event
// collection using Win32 Event Log APIs for predictive hardware failure analysis.
// 
// CRITICAL DESIGN NOTES:
// - Event ID mappings are corrected per Microsoft documentation
// - Focuses on "Corrected" errors for predictive analysis
// - Uses native Win32 APIs for performance (not PowerShell)
// ==================================================================================

use crate::models::hardware_diagnostics::{
    HardwareEvent, EventSeverity, HardwareComponent, EventDetails
};
use anyhow::{Result, anyhow};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use chrono;

#[cfg(target_os = "windows")]
use windows::{
    core::{PCWSTR, w},
    Win32::System::EventLog::{
        EvtQuery, EvtRender, EvtClose, EvtNext, EvtCreateRenderContext,
        EVT_HANDLE,
        EvtQueryChannelPath, EvtQueryReverseDirection, EvtRenderEventXml,
        EvtRenderContextSystem,
    },
};

// WHEA Provider and Event ID mappings (corrected per plan feedback)
const WHEA_PROVIDER: &str = "Microsoft-Windows-WHEA-Logger";
const SYSTEM_CHANNEL: &str = "System";

// Corrected Event ID mappings
const EVENT_CORRECTED_MCE: u32 = 17;           // Corrected Machine Check Exception
const EVENT_FATAL_MCE: u32 = 18;               // Fatal Machine Check Exception  
const EVENT_CORRECTED_CPU_MCE: u32 = 19;       // Corrected CPU/Machine Check Exception
const EVENT_CORRECTED_MEMORY_ERROR: u32 = 47;  // Corrected Memory Error (ECC RAM)

/// Component mapping for WHEA events
fn map_event_to_component(event_id: u32) -> HardwareComponent {
    match event_id {
        EVENT_CORRECTED_MCE | EVENT_FATAL_MCE | EVENT_CORRECTED_CPU_MCE => {
            HardwareComponent::CPU
        },
        EVENT_CORRECTED_MEMORY_ERROR => {
            HardwareComponent::Memory
        },
        _ => HardwareComponent::Unknown
    }
}

/// Severity mapping for WHEA events
fn map_event_to_severity(event_id: u32) -> EventSeverity {
    match event_id {
        EVENT_CORRECTED_MCE | EVENT_CORRECTED_CPU_MCE | EVENT_CORRECTED_MEMORY_ERROR => {
            EventSeverity::Warning // Corrected errors are predictive signals
        },
        EVENT_FATAL_MCE => {
            EventSeverity::Fatal
        },
        _ => EventSeverity::Error
    }
}

/// Main WHEA event collection function
pub async fn collect() -> Result<Vec<HardwareEvent>> {
    #[cfg(target_os = "windows")]
    {
        collect_whea_events().await
    }
    
    #[cfg(not(target_os = "windows"))]
    {
        Ok(Vec::new())
    }
}

#[cfg(target_os = "windows")]
async fn collect_whea_events() -> Result<Vec<HardwareEvent>> {
    let mut events = Vec::new();
    let start_time = Instant::now();
    
    // Create the query string
    let query = format!(
        "*[System[Provider[@Name='{}'] and TimeCreated[timediff(@SystemTime) <= 604800000]]]",
        WHEA_PROVIDER
    );
    
    // CRITICAL MEMORY FIX: Keep the vector alive for the duration of the query
    let query_utf16: Vec<u16> = query.encode_utf16().chain(std::iter::once(0)).collect();
    let query_path = PCWSTR::from_raw(query_utf16.as_ptr());
    
    // Open event log query (0.62 API compliant)
    let query_handle = unsafe {
        EvtQuery(
            None, // 0.62 expects None instead of EVT_HANDLE::default()
            w!("System"),
            query_path,
            EvtQueryChannelPath.0 | EvtQueryReverseDirection.0, // Unwrap the flags
        )?
    };
    
    let _guard = EvtHandleGuard(query_handle);
    
    // Create render context
    let render_context = unsafe {
        EvtCreateRenderContext(None, EvtRenderContextSystem.0)? // Unwrap the flag
    };
    let _render_guard = EvtHandleGuard(render_context);
    
    // Process events in batches (0.62 API expects isize slices)
    let mut event_handles = vec![0isize; 10]; 
    let mut returned = 0u32;
    
    loop {
        let result = unsafe {
            EvtNext(
                query_handle,
                &mut event_handles, // Pass the slice directly!
                5000,
                0,
                &mut returned,
            )
        };
        
        if result.is_err() || returned == 0 {
            break;
        }
        
        for i in 0..returned as usize {
            // Re-cast the isize to an EVT_HANDLE (0.62: EVT_HANDLE is isize wrapper)
            let handle = EVT_HANDLE(event_handles[i]);
            if let Ok(event) = parse_whea_event(handle, render_context) {
                events.push(event);
            }
            unsafe { let _ = EvtClose(handle); }
        }
        
        if start_time.elapsed() > Duration::from_millis(80) {
            eprintln!("[WHEA] Collection timeout, stopping at {} events", events.len());
            break;
        }
        
        if events.len() >= 50 { break; }
    }
    
    Ok(events)
}

#[cfg(target_os = "windows")]
fn parse_whea_event(event_handle: EVT_HANDLE, render_context: EVT_HANDLE) -> Result<HardwareEvent> {
    let mut buffer_used = 0u32;
    let mut property_count = 0u32;
    
    // Call once to get required buffer size (Expect this to return an Error in 0.62)
    unsafe {
        let _ = EvtRender(
            Some(render_context), // 0.62 requires Option wrapper
            event_handle,
            EvtRenderEventXml.0,
            0,
            Some(std::ptr::null_mut()), // 0.62 requires Option wrapper
            &mut buffer_used,
            &mut property_count,
        );
    }
    
    if buffer_used == 0 {
        return Err(anyhow!("Failed to get event buffer size"));
    }
    
    // Allocate buffer
    let mut buffer: Vec<u16> = vec![0; (buffer_used / 2) as usize + 1];
    
    // 0.62 API returns Result<()>, no more .as_bool()
    unsafe {
        EvtRender(
            Some(render_context), // 0.62 requires Option wrapper
            event_handle,
            EvtRenderEventXml.0,
            (buffer.len() * 2) as u32,
            Some(buffer.as_mut_ptr() as *mut _), // 0.62 requires Option wrapper
            &mut buffer_used,
            &mut property_count,
        )?;
    }
    
    let xml_string = String::from_utf16_lossy(&buffer[..((buffer_used / 2) as usize)]);
    parse_whea_xml(&xml_string)
}

#[cfg(target_os = "windows")]
fn parse_whea_xml(xml: &str) -> Result<HardwareEvent> {
    // Simple XML parsing for WHEA events
    // In production, consider using a proper XML parser
    
    let event_id = extract_xml_value(xml, "EventID")
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0);
    
    let timestamp = extract_xml_value(xml, "TimeCreated")
        .map(|s| s.replace("SystemTime='", "").replace("'", ""))
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
    
    let component = map_event_to_component(event_id);
    let severity = map_event_to_severity(event_id);
    
    // Extract additional event data
    let error_type = match event_id {
        EVENT_CORRECTED_MCE => "corrected_mce",
        EVENT_FATAL_MCE => "fatal_mce", 
        EVENT_CORRECTED_CPU_MCE => "corrected_cpu_mce",
        EVENT_CORRECTED_MEMORY_ERROR => "corrected_memory_error",
        _ => "unknown_whea_event",
    };
    
    // Try to extract bank information for MCE events
    let bank = if xml.contains("Bank") {
        extract_xml_value(xml, "Bank")
            .and_then(|s| s.parse::<u8>().ok())
    } else {
        None
    };
    
    // Extract error count if available
    let count = if xml.contains("Count") {
        extract_xml_value(xml, "Count")
            .and_then(|s| s.parse::<u64>().ok())
    } else {
        Some(1) // Single occurrence
    };
    
    Ok(HardwareEvent {
        timestamp,
        severity,
        component,
        event_type: error_type.to_string(),
        details: EventDetails {
            count,
            bank,
            error_type: Some(error_type.to_string()),
            address: extract_xml_value(xml, "Address"),
            context: Some(format!("WHEA Event ID {}", event_id)),
        },
        source_id: Some(event_id.to_string()),
    })
}

/// Simple XML value extraction helper
fn extract_xml_value(xml: &str, tag: &str) -> Option<String> {
    let start_tag = format!("<{}>", tag);
    let end_tag = format!("</{}>", tag);
    
    if let Some(start) = xml.find(&start_tag) {
        let value_start = start + start_tag.len();
        if let Some(end) = xml[value_start..].find(&end_tag) {
            return Some(xml[value_start..value_start + end].to_string());
        }
    }
    
    // Also try attribute format
    let attr_pattern = format!("{}='", tag);
    if let Some(start) = xml.find(&attr_pattern) {
        let value_start = start + attr_pattern.len();
        if let Some(end) = xml[value_start..].find('\'') {
            return Some(xml[value_start..value_start + end].to_string());
        }
    }
    
    None
}

/// Aggregate WHEA events for pattern analysis
pub fn analyze_whea_patterns(events: &[HardwareEvent]) -> HashMap<String, u64> {
    let mut patterns = HashMap::new();
    
    for event in events {
        // Only analyze predictive events
        if event.is_predictive_signal() {
            let pattern_key = format!("{:?}_{}", event.component, event.event_type);
            *patterns.entry(pattern_key).or_insert(0) += event.details.count.unwrap_or(1);
        }
    }
    
    patterns
}

/// RAII guard for Windows event handles
#[cfg(target_os = "windows")]
struct EvtHandleGuard(EVT_HANDLE);

#[cfg(target_os = "windows")]
impl Drop for EvtHandleGuard {
    fn drop(&mut self) {
        // 0.62: EVT_HANDLE.0 is isize, 0 means null
        if !self.0.is_invalid() && self.0.0 != 0 {
            unsafe { let _ = EvtClose(self.0); }
        }
    }
}