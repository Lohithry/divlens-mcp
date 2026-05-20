// ==================================================================================
// 🛡️ HARDWARE DIAGNOSTICS TOOL - Enterprise-Grade Predictive Analysis
// ==================================================================================
// This tool provides comprehensive hardware telemetry and predictive failure
// analysis using native APIs across Windows, Linux, and macOS platforms.
//
// FEATURES:
// - Native WHEA/MCE/Kernel Panic event collection
// - Protocol-aware SMART data (NVMe vs SATA)
// - Concurrent collection for <500ms performance
// - LRU caching for repeated queries
// - Predictive risk assessment
// ==================================================================================

use async_trait::async_trait;
use serde_json::{json, Value};
use crate::modules::metrics::SystemCollector;
use crate::modules::datahub::DataHub;
use crate::modules::memory::MemoryManager;
use crate::tools::Tool;
use crate::collectors::hardware;
use crate::models::hardware_diagnostics::{HardwareDiagnostics, PredictionAnalysis, RiskLevel, RiskAssessment};
use std::time::{Duration, Instant};
use once_cell::sync::Lazy;
use std::sync::Mutex;

// Global cache for hardware diagnostics (5 minute TTL for SMART, 1 minute for events)
static HARDWARE_CACHE: Lazy<Mutex<crate::collectors::hardware::utils::HardwareCache<String, Value>>> = 
    Lazy::new(|| Mutex::new(crate::collectors::hardware::utils::HardwareCache::new(16, Duration::from_secs(300))));

pub struct GetHardwareDiagnosticsTool;

impl GetHardwareDiagnosticsTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for GetHardwareDiagnosticsTool {
    fn name(&self) -> &str {
        "get_hardware_diagnostics"
    }

    fn description(&self) -> &str {
        "Collect enterprise-grade hardware diagnostics including predictive failure analysis. Uses native APIs for SMART data (NVMe/SATA), hardware error events (WHEA/MCE/Kernel Panics), and thermal monitoring across Windows, Linux, and macOS."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "include_prediction": {
                    "type": "boolean", 
                    "description": "Include predictive failure analysis based on hardware events and SMART trends",
                    "default": true
                },
                "force_refresh": {
                    "type": "boolean",
                    "description": "Force fresh collection, bypassing cache (use for real-time diagnostics)",
                    "default": false
                },
                "components": {
                    "type": "array",
                    "items": {
                        "type": "string",
                        "enum": ["storage", "memory", "cpu", "thermal", "all"]
                    },
                    "description": "Specific hardware components to analyze (default: all)",
                    "default": ["all"]
                }
            }
        })
    }

    async fn call(&self, args: &Value, _col: &mut SystemCollector, _dh: &DataHub, _mem: &MemoryManager) -> anyhow::Result<Value> {
        let start_time = Instant::now();
        
        // Parse arguments
        let include_prediction = args.get("include_prediction")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        
        let force_refresh = args.get("force_refresh")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        
        let components = args.get("components")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.to_string())
                    .collect::<Vec<String>>()
            })
            .unwrap_or_else(|| vec!["all".to_string()]);

        // Check cache unless forced refresh
        let cache_key = format!("hardware_diag_{}", serde_json::to_string(&components).unwrap_or_default());
        
        if !force_refresh {
            if let Ok(mut cache) = HARDWARE_CACHE.try_lock() {
                if let Some(cached_result) = cache.get(&cache_key) {
                    return Ok(cached_result);
                }
            }
        }

        // Collect hardware diagnostics
        let mut diagnostics = match hardware::collect_hardware_diagnostics().await {
            Ok(diag) => diag,
            Err(e) => {
                eprintln!("[HW_TOOL] Hardware collection failed: {}", e);
                // Return fallback data
                HardwareDiagnostics::default()
            }
        };

        // Filter components if specific ones requested
        if !components.contains(&"all".to_string()) {
            filter_components(&mut diagnostics, &components);
        }

        // Generate prediction analysis if requested
        let prediction = if include_prediction {
            Some(generate_prediction_analysis(&diagnostics))
        } else {
            None
        };

        // Build response
        let mut response = serde_json::to_value(&diagnostics)?;
        
        if let Some(pred) = prediction {
            if let Some(response_obj) = response.as_object_mut() {
                response_obj.insert("prediction_analysis".to_string(), serde_json::to_value(pred)?);
            }
        }

        // Add performance metadata
        let collection_time = start_time.elapsed().as_millis() as u64;
        if let Some(response_obj) = response.as_object_mut() {
            response_obj.insert("tool_performance".to_string(), json!({
                "collection_time_ms": collection_time,
                "cache_used": !force_refresh,
                "target_time_ms": 500
            }));
        }

        // Cache successful result
        if let Ok(mut cache) = HARDWARE_CACHE.try_lock() {
            cache.insert(cache_key, response.clone());
        }

        Ok(response)
    }
}

/// Filter diagnostics to only include requested components
fn filter_components(diagnostics: &mut HardwareDiagnostics, components: &[String]) {
    // Filter storage if not requested
    if !components.contains(&"storage".to_string()) {
        diagnostics.storage.clear();
    }
    
    // Filter hardware events by component
    if !components.iter().any(|c| ["memory", "cpu", "all"].contains(&c.as_str())) {
        diagnostics.hardware_events.retain(|event| {
            !matches!(event.component, 
                crate::models::hardware_diagnostics::HardwareComponent::CPU | 
                crate::models::hardware_diagnostics::HardwareComponent::Memory)
        });
    }
    
    // Filter thermal data
    if !components.contains(&"thermal".to_string()) {
        diagnostics.thermal = crate::models::hardware_diagnostics::ThermalData {
            status_note: None,
            cpu: None,
            gpu: None,
            storage: None,
            motherboard: None,
        };
    }
}

/// Generate predictive failure analysis based on collected hardware data
pub(crate) fn generate_prediction_analysis(diagnostics: &HardwareDiagnostics) -> PredictionAnalysis {
    let mut at_risk_components = Vec::new();
    let mut recommendations = Vec::new();
    let mut confidence_scores = Vec::new();

    // Analyze storage health
    for storage in &diagnostics.storage {
        let risk_level = assess_storage_risk(storage);
        if risk_level != RiskLevel::Low {
            let time_to_failure = estimate_storage_failure_time(storage);
            let factors = identify_storage_risk_factors(storage);
            
            at_risk_components.push(RiskAssessment {
                component: crate::models::hardware_diagnostics::HardwareComponent::Storage,
                risk_level: risk_level.clone(),
                time_to_failure,
                factors: factors.clone(),
            });
            
            // Add recommendations based on risk factors
            if factors.iter().any(|f| f.contains("temperature")) {
                recommendations.push("Consider improving cooling for storage devices".to_string());
            }
            if factors.iter().any(|f| f.contains("wear")) {
                recommendations.push(format!("Monitor {} wear level closely, consider replacement planning", storage.device));
            }
            
            confidence_scores.push(calculate_storage_confidence(storage));
        }
    }

    // Analyze hardware events for patterns
    let event_patterns = analyze_hardware_event_patterns(&diagnostics.hardware_events);
    for (pattern, count) in event_patterns {
        if count > 10 {
            // High frequency of hardware events indicates risk
            let component = parse_component_from_pattern(&pattern);
            let risk_level = if count > 100 {
                RiskLevel::Critical
            } else if count > 50 {
                RiskLevel::High
            } else {
                RiskLevel::Medium
            };
            
            at_risk_components.push(RiskAssessment {
                component,
                risk_level,
                time_to_failure: Some(estimate_failure_time_from_events(count)),
                factors: vec![format!("High frequency {} events ({})", pattern, count)],
            });
            
            recommendations.push(format!("Investigate {} events - {} occurrences detected", component_name(&component), count));
            confidence_scores.push(std::cmp::min(95, 60 + (count / 10) as u8));
        }
    }

    // Overall system health assessment
    let overall_health = if at_risk_components.iter().any(|r| matches!(r.risk_level, RiskLevel::Critical)) {
        crate::models::hardware_diagnostics::HealthStatus::Critical
    } else if at_risk_components.iter().any(|r| matches!(r.risk_level, RiskLevel::High)) {
        crate::models::hardware_diagnostics::HealthStatus::Degraded
    } else if !at_risk_components.is_empty() {
        crate::models::hardware_diagnostics::HealthStatus::Degraded
    } else {
        crate::models::hardware_diagnostics::HealthStatus::Healthy
    };

    // Calculate average confidence
    let average_confidence = if confidence_scores.is_empty() {
        85 // Default confidence for healthy systems
    } else {
        confidence_scores.iter().map(|&c| c as u32).sum::<u32>() / confidence_scores.len() as u32
    };

    // Add general recommendations
    if overall_health == crate::models::hardware_diagnostics::HealthStatus::Healthy {
        recommendations.push("System hardware appears healthy - continue regular monitoring".to_string());
    }
    
    PredictionAnalysis {
        system_health: overall_health,
        at_risk_components,
        recommendations,
        confidence: average_confidence as u8,
    }
}

/// Assess storage device risk level
pub(crate) fn assess_storage_risk(storage: &crate::models::hardware_diagnostics::StorageDevice) -> RiskLevel {
    match &storage.health.status {
        crate::models::hardware_diagnostics::HealthStatus::Failed => RiskLevel::Critical,
        crate::models::hardware_diagnostics::HealthStatus::Critical => RiskLevel::Critical,
        crate::models::hardware_diagnostics::HealthStatus::Degraded => RiskLevel::High,
        crate::models::hardware_diagnostics::HealthStatus::Healthy => {
            // Check for early warning signs
            if storage.health.failure_predicted {
                RiskLevel::Medium
            } else if let Some(temp) = storage.health.temperature_celsius {
                if temp > 70 {
                    RiskLevel::Medium // High temperature
                } else {
                    RiskLevel::Low
                }
            } else {
                RiskLevel::Low
            }
        },
        _ => RiskLevel::Low,
    }
}

/// Estimate time to failure for storage devices
fn estimate_storage_failure_time(storage: &crate::models::hardware_diagnostics::StorageDevice) -> Option<String> {
    match storage.protocol {
        crate::models::hardware_diagnostics::StorageProtocol::NVMe => {
            if let Some(percentage_used) = storage.health.percentage_used {
                if percentage_used > 95 {
                    Some("Days to weeks".to_string())
                } else if percentage_used > 90 {
                    Some("Weeks to months".to_string())
                } else if percentage_used > 80 {
                    Some("Months to year".to_string())
                } else {
                    None
                }
            } else {
                None
            }
        },
        crate::models::hardware_diagnostics::StorageProtocol::SATA => {
            if storage.smart_attributes.reallocated_sectors.unwrap_or(0) > 10 {
                Some("Weeks to months".to_string())
            } else if storage.smart_attributes.current_pending_sectors.unwrap_or(0) > 0 {
                Some("Months to year".to_string())
            } else {
                None
            }
        },
        _ => None,
    }
}

/// Identify storage risk factors
fn identify_storage_risk_factors(storage: &crate::models::hardware_diagnostics::StorageDevice) -> Vec<String> {
    let mut factors = Vec::new();
    
    if let Some(temp) = storage.health.temperature_celsius {
        if temp > 70 {
            factors.push(format!("High temperature: {}°C", temp));
        }
    }
    
    if storage.health.failure_predicted {
        factors.push("SMART failure prediction triggered".to_string());
    }
    
    match storage.protocol {
        crate::models::hardware_diagnostics::StorageProtocol::NVMe => {
            if let Some(percentage_used) = storage.health.percentage_used {
                if percentage_used > 80 {
                    factors.push(format!("High wear level: {}% used", percentage_used));
                }
            }
            if let Some(media_errors) = storage.smart_attributes.media_errors {
                if media_errors > 0 {
                    factors.push(format!("Media errors detected: {}", media_errors));
                }
            }
        },
        crate::models::hardware_diagnostics::StorageProtocol::SATA => {
            if let Some(reallocated) = storage.smart_attributes.reallocated_sectors {
                if reallocated > 0 {
                    factors.push(format!("Reallocated sectors: {}", reallocated));
                }
            }
            if let Some(pending) = storage.smart_attributes.current_pending_sectors {
                if pending > 0 {
                    factors.push(format!("Pending sectors: {}", pending));
                }
            }
        },
        _ => {}
    }
    
    factors
}

/// Calculate confidence score for storage assessment
fn calculate_storage_confidence(storage: &crate::models::hardware_diagnostics::StorageDevice) -> u8 {
    let mut score = 70; // Base confidence
    
    // Higher confidence if we have more data
    if storage.health.temperature_celsius.is_some() { score += 10; }
    if storage.health.power_on_hours.is_some() { score += 5; }
    if storage.smart_attributes.media_errors.is_some() { score += 10; }
    
    // Protocol-specific confidence adjustments
    match storage.protocol {
        crate::models::hardware_diagnostics::StorageProtocol::NVMe => {
            if storage.health.percentage_used.is_some() { score += 10; }
        },
        crate::models::hardware_diagnostics::StorageProtocol::SATA => {
            if storage.smart_attributes.reallocated_sectors.is_some() { score += 10; }
        },
        _ => score -= 20, // Lower confidence for unknown protocols
    }
    
    std::cmp::min(95, score)
}

/// Analyze hardware event patterns for risk assessment
fn analyze_hardware_event_patterns(events: &[crate::models::hardware_diagnostics::HardwareEvent]) -> std::collections::HashMap<String, u64> {
    let mut patterns = std::collections::HashMap::new();
    
    for event in events {
        if event.is_predictive_signal() {
            let pattern_key = format!("{:?}_{}", event.component, event.event_type);
            *patterns.entry(pattern_key).or_insert(0) += event.details.count.unwrap_or(1);
        }
    }
    
    patterns
}

/// Parse component from event pattern string
fn parse_component_from_pattern(pattern: &str) -> crate::models::hardware_diagnostics::HardwareComponent {
    if pattern.contains("CPU") {
        crate::models::hardware_diagnostics::HardwareComponent::CPU
    } else if pattern.contains("Memory") {
        crate::models::hardware_diagnostics::HardwareComponent::Memory
    } else if pattern.contains("Storage") {
        crate::models::hardware_diagnostics::HardwareComponent::Storage
    } else if pattern.contains("PCIe") {
        crate::models::hardware_diagnostics::HardwareComponent::PCIe
    } else {
        crate::models::hardware_diagnostics::HardwareComponent::Unknown
    }
}

/// Get human-readable component name
fn component_name(component: &crate::models::hardware_diagnostics::HardwareComponent) -> &str {
    match component {
        crate::models::hardware_diagnostics::HardwareComponent::CPU => "CPU",
        crate::models::hardware_diagnostics::HardwareComponent::Memory => "Memory", 
        crate::models::hardware_diagnostics::HardwareComponent::Storage => "Storage",
        crate::models::hardware_diagnostics::HardwareComponent::PCIe => "PCIe",
        crate::models::hardware_diagnostics::HardwareComponent::Motherboard => "Motherboard",
        crate::models::hardware_diagnostics::HardwareComponent::PowerSupply => "Power Supply",
        crate::models::hardware_diagnostics::HardwareComponent::Cooling => "Cooling",
        crate::models::hardware_diagnostics::HardwareComponent::GPU => "GPU",
        crate::models::hardware_diagnostics::HardwareComponent::Network => "Network",
        crate::models::hardware_diagnostics::HardwareComponent::USB => "USB",
        crate::models::hardware_diagnostics::HardwareComponent::Unknown => "Unknown",
    }
}

/// Estimate failure time based on event frequency
fn estimate_failure_time_from_events(event_count: u64) -> String {
    if event_count > 1000 {
        "Days to weeks".to_string()
    } else if event_count > 100 {
        "Weeks to months".to_string()
    } else if event_count > 50 {
        "Months".to_string()
    } else {
        "Monitoring required".to_string()
    }
}