// ==================================================================================
// 🛡️ DIVLENS HARDWARE DIAGNOSTICS SCHEMA - Enterprise-Grade Predictive Models
// ==================================================================================
// This module defines the unified data structures for hardware telemetry that
// enables predictive diagnostics across Windows, Linux, and macOS.
//
// DESIGN PRINCIPLES:
// 1. Protocol-Aware: Distinguishes NVMe vs SATA for correct AI interpretation
// 2. Predictive: Emphasizes "Corrected" errors as early warning signals
// 3. Normalized: Single structure for cross-platform hardware data
// 4. AI-Ready: Structured JSON that prevents hallucination
// ==================================================================================

use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use chrono;

/// The main hardware diagnostics structure that aggregates all telemetry
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HardwareDiagnostics {
    /// ISO 8601 timestamp of collection
    pub timestamp: String,
    /// Storage device health and SMART data
    pub storage: Vec<StorageDevice>,
    /// Hardware error events (predictive signals)
    pub hardware_events: Vec<HardwareEvent>,
    /// Thermal information for components
    pub thermal: ThermalData,
    /// Collection metadata and performance metrics
    pub collection_metadata: CollectionMetadata,
}

/// Storage device with protocol-aware health metrics
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StorageDevice {
    /// Device path/identifier (e.g., "/dev/nvme0n1", "\\.\PhysicalDrive0")
    pub device: String,
    /// Protocol type - CRITICAL for AI interpretation
    pub protocol: StorageProtocol,
    /// Device type (SSD, HDD, etc.)
    pub device_type: String,
    /// Manufacturer and model
    pub model: String,
    /// Serial number if available
    pub serial: Option<String>,
    /// Capacity in bytes
    pub capacity_bytes: Option<u64>,
    /// Health metrics (protocol-specific)
    pub health: StorageHealth,
    /// Raw SMART attributes
    pub smart_attributes: SmartAttributes,
    /// Tiered Access: Whether deep telemetry (raw SMART, IOCTL) was accessible
    pub deep_telemetry_access: bool,
    /// Tiered Access: Reason if deep telemetry was unavailable
    pub missing_permissions_reason: Option<String>,
    /// Data collection tier used (1=Native APIs, 2=CLI tools, 3=Basic fallback)
    pub collection_tier: u8,
}

/// Protocol distinction for correct health interpretation
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub enum StorageProtocol {
    /// NVMe drives (modern) - percentage_used where 100% = dead
    NVMe,
    /// SATA/ATA drives (legacy) - available_spare where 0% = dead  
    SATA,
    /// SCSI drives
    SCSI,
    /// Unknown protocol
    Unknown,
}

/// Storage health metrics normalized across protocols
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StorageHealth {
    /// Overall health status
    pub status: HealthStatus,
    /// NVMe: Percentage Used (0-100, higher is worse)
    pub percentage_used: Option<u8>,
    /// SATA: Available Spare (0-100, lower is worse) 
    pub available_spare: Option<u8>,
    /// Temperature in Celsius
    pub temperature_celsius: Option<u32>,
    /// Power-on hours
    pub power_on_hours: Option<u64>,
    /// Critical warning flags
    pub critical_warning: bool,
    /// Predictive failure indicator
    pub failure_predicted: bool,
}

/// Normalized health status
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    /// Device is operating normally
    Healthy,
    /// Device showing early warning signs
    Degraded,
    /// Device failure predicted soon
    Critical,
    /// Device has failed or is failing
    Failed,
    /// Status cannot be determined
    Unknown,
}

/// SMART attributes with common metrics
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct SmartAttributes {
    /// Media/uncorrectable errors
    pub media_errors: Option<u64>,
    /// Total data written in TB
    pub data_written_tb: Option<f64>,
    /// Total data read in TB  
    pub data_read_tb: Option<f64>,
    /// Unsafe shutdown count
    pub unsafe_shutdowns: Option<u64>,
    /// Controller busy time in minutes
    pub controller_busy_time_minutes: Option<u64>,
    /// Reallocated sectors (SATA/SCSI)
    pub reallocated_sectors: Option<u64>,
    /// Current pending sectors (SATA/SCSI)
    pub current_pending_sectors: Option<u64>,
    /// Additional attributes by ID
    pub raw_attributes: HashMap<u8, u64>,
}

/// Hardware error event for predictive analysis
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HardwareEvent {
    /// ISO 8601 timestamp
    pub timestamp: String,
    /// Event severity level
    pub severity: EventSeverity,
    /// Hardware component affected
    pub component: HardwareComponent,
    /// Type of error/event
    pub event_type: String,
    /// Detailed event information
    pub details: EventDetails,
    /// Source system (Windows Event ID, etc.)
    pub source_id: Option<String>,
}

/// Event severity levels for triage
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum EventSeverity {
    /// Informational event
    Info,
    /// Warning condition (early signal)
    Warning,
    /// Error condition (degradation)
    Error,
    /// Critical condition (imminent failure)
    Critical,
    /// Fatal condition (failure occurred)
    Fatal,
}

/// Hardware components for event classification
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub enum HardwareComponent {
    CPU,
    Memory,
    Storage,
    Motherboard,
    PowerSupply,
    Cooling,
    GPU,
    Network,
    PCIe,
    USB,
    Unknown,
}

/// Event details with flexible structure
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EventDetails {
    /// Error count or frequency
    pub count: Option<u64>,
    /// CPU bank number (for MCE)
    pub bank: Option<u8>,
    /// Specific error type
    pub error_type: Option<String>,
    /// Memory address (for ECC errors)
    pub address: Option<String>,
    /// Additional context
    pub context: Option<String>,
}

/// Thermal data for all monitored components
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ThermalData {
    /// CPU thermal information
    pub cpu: Option<ThermalInfo>,
    /// GPU thermal information  
    pub gpu: Option<ThermalInfo>,
    /// Storage thermal (if separate from device data)
    pub storage: Option<ThermalInfo>,
    /// Motherboard/chipset thermal
    pub motherboard: Option<ThermalInfo>,
}

/// Thermal information for a component
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ThermalInfo {
    /// Current temperature in Celsius
    pub current_celsius: Option<u32>,
    /// Maximum safe temperature
    pub max_celsius: Option<u32>,
    /// Critical temperature threshold
    pub critical_celsius: Option<u32>,
    /// Whether component is throttling
    pub throttling: bool,
    /// Fan speeds if available
    pub fan_speeds_rpm: Vec<u32>,
}

/// Collection metadata for performance tracking and tiered access reporting
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CollectionMetadata {
    /// Total collection time in milliseconds
    pub collection_time_ms: u64,
    /// Individual collector timings
    pub collector_timings: HashMap<String, u64>,
    /// Whether elevated permissions were available
    pub elevated_permissions: bool,
    /// Fallback methods used
    pub fallback_methods: Vec<String>,
    /// Errors encountered during collection
    pub errors: Vec<CollectionError>,
    /// Tiered Access: Highest tier achieved (1=Full native, 2=CLI tools, 3=Basic)
    pub access_tier: AccessTier,
    /// Tiered Access: What data is unavailable without elevation
    pub unavailable_without_elevation: Vec<String>,
}

/// Access tier levels for tiered data collection
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AccessTier {
    /// Tier 1: Full native API access (IOKit, IOCTLs) - requires root/admin
    FullNative,
    /// Tier 2: CLI tool access (smartctl, nvme-cli) - may require root
    CliTools,
    /// Tier 3: Basic system APIs (sysinfo, system_profiler) - no elevation needed
    BasicSystem,
    /// Tier 4: Minimal fallback (only what OS freely provides)
    MinimalFallback,
}

/// Collection error information
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CollectionError {
    /// Component that failed
    pub component: String,
    /// Error message
    pub message: String,
    /// Whether fallback was used
    pub fallback_used: bool,
}

/// Platform-specific raw data structures for internal use
#[derive(Debug, Clone)]
pub enum RawHardwareData {
    /// Windows WHEA event
    WindowsWhea {
        event_id: u32,
        event_data: Vec<u8>,
        timestamp: String,
    },
    /// Linux MCE/RAS event
    LinuxMce {
        mce_data: String,
        timestamp: String,
    },
    /// macOS kernel panic/hardware report
    MacOSKernelEvent {
        report_data: String,
        timestamp: String,
    },
}

/// Prediction analysis results
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PredictionAnalysis {
    /// Overall system health prediction
    pub system_health: HealthStatus,
    /// Predicted failure components
    pub at_risk_components: Vec<RiskAssessment>,
    /// Recommended actions
    pub recommendations: Vec<String>,
    /// Confidence score (0-100)
    pub confidence: u8,
}

/// Risk assessment for individual components
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RiskAssessment {
    /// Component at risk
    pub component: HardwareComponent,
    /// Risk level
    pub risk_level: RiskLevel,
    /// Time to failure estimate
    pub time_to_failure: Option<String>,
    /// Contributing factors
    pub factors: Vec<String>,
}

/// Risk levels for component assessment
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

impl Default for HardwareDiagnostics {
    fn default() -> Self {
        Self {
            timestamp: chrono::Utc::now().to_rfc3339(),
            storage: Vec::new(),
            hardware_events: Vec::new(),
            thermal: ThermalData {
                cpu: None,
                gpu: None,
                storage: None,
                motherboard: None,
            },
            collection_metadata: CollectionMetadata {
                collection_time_ms: 0,
                collector_timings: HashMap::new(),
                elevated_permissions: false,
                fallback_methods: Vec::new(),
                errors: Vec::new(),
                access_tier: AccessTier::MinimalFallback,
                unavailable_without_elevation: Vec::new(),
            },
        }
    }
}

impl StorageProtocol {
    /// Determine if higher percentage indicates worse health
    pub fn higher_percentage_is_worse(&self) -> bool {
        match self {
            StorageProtocol::NVMe => true,  // percentage_used: 100% = dead
            StorageProtocol::SATA => false, // available_spare: 0% = dead
            StorageProtocol::SCSI => false,
            StorageProtocol::Unknown => false,
        }
    }
}

impl HardwareEvent {
    /// Check if this event indicates hardware degradation
    pub fn is_predictive_signal(&self) -> bool {
        match self.severity {
            EventSeverity::Warning | EventSeverity::Error => true,
            EventSeverity::Critical => true,
            EventSeverity::Fatal => false, // Too late for prediction
            EventSeverity::Info => false,
        }
    }
    
    /// Calculate risk score based on event frequency and severity
    pub fn risk_score(&self) -> u8 {
        let base_score = match self.severity {
            EventSeverity::Info => 0,
            EventSeverity::Warning => 25,
            EventSeverity::Error => 50,
            EventSeverity::Critical => 75,
            EventSeverity::Fatal => 100,
        };
        
        // Increase score based on event count
        if let Some(count) = self.details.count {
            let frequency_multiplier = match count {
                1..=5 => 1.0,
                6..=20 => 1.2,
                21..=100 => 1.5,
                _ => 2.0,
            };
            ((base_score as f64 * frequency_multiplier).min(100.0)) as u8
        } else {
            base_score
        }
    }
}

impl Default for StorageHealth {
    fn default() -> Self {
        Self {
            status: HealthStatus::Unknown,
            percentage_used: None,
            available_spare: None,
            temperature_celsius: None,
            power_on_hours: None,
            critical_warning: false,
            failure_predicted: false,
        }
    }
}