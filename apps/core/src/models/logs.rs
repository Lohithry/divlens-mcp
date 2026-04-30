// ==================================================================================
// 🛡️ DIVLENS LOG SENTINEL SCHEMA - The Unified Log Structure
// ==================================================================================
// This module defines the "Truth" that the rest of the app relies on.
// It normalizes the chaotic log formats of Windows, Linux, and macOS into
// a single, AI-friendly structure.
//
// DESIGN PRINCIPLES:
// 1. Unified: Single structure for all platforms (Windows Event Log, Linux journald, macOS Unified Log)
// 2. Filtered: Only captures Errors and Critical events (solves signal-to-noise ratio)
// 3. Clustered: Groups duplicate errors to prevent AI token overflow
// 4. AI-Ready: Provides both raw events and pre-processed insights
// ==================================================================================

use serde::Serialize;

/// Represents the overall health snapshot of system logs.
/// This is what the UI and AI consume.
#[derive(Debug, Serialize, Clone, Default)]
pub struct LogSnapshot {
    /// Total number of error-level events found in the scan window.
    pub error_count: usize,
    
    /// A raw list of the most recent critical events (for UI display).
    pub critical_events: Vec<LogEntry>,
    
    /// AI-ready summary that groups duplicate errors (e.g., "Wifi Error x 500").
    pub clustered_insights: Vec<LogCluster>,
}

/// A single log event normalized across all operating systems.
#[derive(Debug, Serialize, Clone)]
pub struct LogEntry {
    /// ISO 8601 Timestamp (e.g., "2023-10-27T10:00:00Z")
    pub timestamp: String,
    
    /// Severity level (Normalized to "Error", "Critical", or "Warning")
    pub level: String,
    
    /// The component generating the log (e.g., "kernel", "DiskDriver")
    pub source: String,
    
    /// The actual log message
    pub message: String,
    
    /// Windows-specific Event ID (Useful for debugging BSOD/Crashes)
    pub event_id: Option<String>,
}

/// A grouped pattern of errors to help AI identify "Death Loops".
#[derive(Debug, Serialize, Clone)]
pub struct LogCluster {
    /// The pattern signature (e.g., "WifiDriver: Connection Timeout")
    pub signature: String,
    
    /// How many times this exact error occurred
    pub count: usize,
    
    /// Time of the first occurrence in this batch
    pub first_seen: String,
    
    /// Time of the last occurrence
    pub last_seen: String,
}

