#[cfg(test)]
mod tests {
    use crate::models::hardware_diagnostics::*;
    use crate::tools::Tool;
    use crate::tools::hardware_diagnostics::{
        assess_storage_risk, generate_prediction_analysis, GetHardwareDiagnosticsTool,
    };
    
    #[tokio::test]
    async fn test_hardware_diagnostics_tool_creation() {
        let tool = GetHardwareDiagnosticsTool::new();
        assert_eq!(tool.name(), "get_hardware_diagnostics");
        assert!(tool.description().contains("enterprise-grade"));
    }
    
    #[test]
    fn test_storage_risk_assessment() {
        // Test NVMe high wear risk
        let nvme_device = StorageDevice {
            device: "/dev/nvme0n1".to_string(),
            protocol: StorageProtocol::NVMe,
            device_type: "SSD".to_string(),
            model: "Test NVMe".to_string(),
            serial: None,
            capacity_bytes: None,
            health: StorageHealth {
                status: HealthStatus::Healthy,
                percentage_used: Some(95), // High wear
                available_spare: Some(100),
                temperature_celsius: Some(45),
                power_on_hours: Some(10000),
                critical_warning: false,
                failure_predicted: true,
            },
            smart_attributes: SmartAttributes::default(),
            deep_telemetry_access: true,
            missing_permissions_reason: None,
            collection_tier: 1,
        };
        
        let risk = assess_storage_risk(&nvme_device);
        assert!(matches!(risk, RiskLevel::Medium));
        
        // Test SATA with reallocated sectors
        let sata_device = StorageDevice {
            device: "/dev/sda".to_string(),
            protocol: StorageProtocol::SATA,
            device_type: "HDD".to_string(), 
            model: "Test SATA".to_string(),
            serial: None,
            capacity_bytes: None,
            health: StorageHealth {
                status: HealthStatus::Critical,
                percentage_used: None,
                available_spare: Some(0), // Failed
                temperature_celsius: Some(55),
                power_on_hours: Some(50000),
                critical_warning: true,
                failure_predicted: true,
            },
            smart_attributes: SmartAttributes {
                reallocated_sectors: Some(100), // Bad sectors
                current_pending_sectors: Some(5),
                ..Default::default()
            },
            deep_telemetry_access: true,
            missing_permissions_reason: None,
            collection_tier: 1,
        };
        
        let risk = assess_storage_risk(&sata_device);
        assert!(matches!(risk, RiskLevel::Critical));
    }
    
    #[test]
    fn test_hardware_event_risk_scoring() {
        let event = HardwareEvent {
            timestamp: "2026-02-26T10:00:00Z".to_string(),
            severity: EventSeverity::Warning,
            component: HardwareComponent::Memory,
            event_type: "ecc_corrected".to_string(),
            details: EventDetails {
                count: Some(50), // Moderate frequency
                bank: None,
                error_type: Some("corrected_error".to_string()),
                address: None,
                context: None,
            },
            source_id: Some("rasdaemon".to_string()),
        };
        
        let risk_score = event.risk_score();
        assert!(risk_score >= 25 && risk_score <= 50); // Warning level with moderate frequency
        
        let high_freq_event = HardwareEvent {
            details: EventDetails {
                count: Some(200), // High frequency
                ..event.details.clone()
            },
            ..event
        };
        
        let high_risk_score = high_freq_event.risk_score();
        assert!(high_risk_score > risk_score); // Should be higher due to frequency
    }
    
    #[test]
    fn test_protocol_health_interpretation() {
        // NVMe: higher percentage_used is worse
        assert!(StorageProtocol::NVMe.higher_percentage_is_worse());
        
        // SATA: higher available_spare is better  
        assert!(!StorageProtocol::SATA.higher_percentage_is_worse());
    }
    
    #[test]
    fn test_prediction_analysis_generation() {
        let mut diagnostics = HardwareDiagnostics::default();
        
        // Add a failing NVMe drive
        diagnostics.storage.push(StorageDevice {
            device: "/dev/nvme0n1".to_string(),
            protocol: StorageProtocol::NVMe,
            device_type: "SSD".to_string(),
            model: "Test Drive".to_string(),
            serial: None,
            capacity_bytes: None,
            health: StorageHealth {
                status: HealthStatus::Critical,
                percentage_used: Some(98),
                available_spare: Some(95),
                temperature_celsius: Some(75), // High temp
                power_on_hours: Some(20000),
                critical_warning: true,
                failure_predicted: true,
            },
            smart_attributes: SmartAttributes {
                media_errors: Some(10),
                ..Default::default()
            },
            deep_telemetry_access: true,
            missing_permissions_reason: None,
            collection_tier: 1,
        });
        
        // Add frequent hardware events
        for i in 0..25 {
            diagnostics.hardware_events.push(HardwareEvent {
                timestamp: format!("2026-02-26T{:02}:00:00Z", i % 24),
                severity: EventSeverity::Warning,
                component: HardwareComponent::Memory,
                event_type: "ecc_corrected".to_string(),
                details: EventDetails {
                    count: Some(1),
                    bank: Some(0),
                    error_type: Some("corrected_memory_error".to_string()),
                    address: None,
                    context: None,
                },
                source_id: Some("test".to_string()),
            });
        }
        
        let prediction = generate_prediction_analysis(&diagnostics);
        
        // Should detect critical system health
        assert!(matches!(prediction.system_health, HealthStatus::Critical));
        
        // Should have at-risk components
        assert!(!prediction.at_risk_components.is_empty());
        
        // Should have recommendations
        assert!(!prediction.recommendations.is_empty());
        
        // Should have reasonable confidence
        assert!(prediction.confidence >= 60 && prediction.confidence <= 100);
    }
    
    #[tokio::test]
    async fn test_tool_schema_validation() {
        let tool = GetHardwareDiagnosticsTool::new();
        let schema = tool.schema();
        
        // Verify required schema elements
        assert!(schema["type"].as_str().unwrap() == "object");
        assert!(schema["properties"]["include_prediction"].is_object());
        assert!(schema["properties"]["force_refresh"].is_object());
        assert!(schema["properties"]["components"].is_object());
    }
}