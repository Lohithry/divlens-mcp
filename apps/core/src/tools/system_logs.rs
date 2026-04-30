// ==================================================================================
// 🛡️ DIVLENS SYSTEM LOGS TOOL - AI-Ready Log Analysis
// ==================================================================================
// This tool provides comprehensive system log analysis for AI consumption.
// It solves the "Signal-to-Noise Ratio" problem by:
// 1. Filtering for Errors/Critical only (reduces noise)
// 2. Clustering duplicate errors (prevents token overflow)
// 3. Providing pre-analyzed insights (AI doesn't read raw logs)
//
// DESIGN PRINCIPLES:
// 1. AI-Friendly: Provides clusters and patterns, not raw dumps
// 2. Cross-Platform: Works on Windows, Linux, macOS
// 3. Performance: Fast collection using native system tools
// 4. Intelligent: Pre-analyzes logs to identify critical patterns
// ==================================================================================

use async_trait::async_trait;
use serde_json::{json, Value};
use crate::collectors::ondemand::log_sentinel::LogSentinel;
use super::Tool;
use crate::modules::metrics::SystemCollector;
use crate::modules::datahub::DataHub;
use crate::modules::memory::MemoryManager;

/// MCP Tool for analyzing system logs
/// This tool is called by the AI to understand system health through log analysis
pub struct GetSystemLogsTool;

impl GetSystemLogsTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for GetSystemLogsTool {
    fn name(&self) -> &str {
        "get_system_logs"
    }

    fn description(&self) -> &str {
        "Analyze system logs to identify errors, critical events, and patterns. Returns clustered insights for AI analysis. Works on Windows (Event Log), Linux (journald), and macOS (Unified Log)."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "include_clusters": {
                    "type": "boolean",
                    "description": "Include clustered error patterns (default: true). Clusters group duplicate errors to show patterns instead of noise.",
                    "default": true
                },
                "max_events": {
                    "type": "number",
                    "description": "Maximum number of raw events to return (default: 20). Clusters are always included regardless of this limit.",
                    "default": 20
                }
            }
        })
    }

    async fn call(
        &self,
        args: &Value,
        _col: &mut SystemCollector,
        _dh: &DataHub,
        _mem: &MemoryManager,
    ) -> anyhow::Result<Value> {
        // Parse optional arguments
        let include_clusters = args
            .get("include_clusters")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        
        // Cap max_events at 30 to prevent timeouts and large responses
        let max_events = std::cmp::min(
            args.get("max_events")
                .and_then(|v| v.as_u64())
                .unwrap_or(20) as usize,
            30
        );

        // Fetch system log health snapshot
        // This internally:
        // 1. Collects platform-specific logs (journalctl, log show, Get-WinEvent)
        // 2. Filters for Errors/Critical only (99.9% data reduction at source)
        // 3. Clusters duplicate errors (prevents AI token overflow)
        let snapshot = LogSentinel::scan_system_health();

        // Pre-analyze logs for AI consumption
        // 1. Check for kernel panics/critical system errors
        let kernel_panics = snapshot.clustered_insights.iter()
            .filter(|c| {
                let sig_lower = c.signature.to_lowercase();
                sig_lower.contains("kernel") || 
                sig_lower.contains("panic") || 
                sig_lower.contains("fatal") ||
                sig_lower.contains("critical")
            })
            .count();

        // 2. Identify high-frequency error patterns (potential crash loops)
        let crash_loops = snapshot.clustered_insights.iter()
            .filter(|c| c.count > 10)
            .count();

        // 3. Determine overall system health status
        let status = if snapshot.error_count > 50 {
            "Critical"
        } else if snapshot.error_count > 20 {
            "Unstable"
        } else if snapshot.error_count > 5 {
            "Degraded"
        } else if kernel_panics > 0 {
            "Degraded" // Kernel issues are always concerning
        } else {
            "Healthy"
        };

        // 4. Get top error sources
        let mut source_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for event in &snapshot.critical_events {
            *source_counts.entry(event.source.clone()).or_insert(0) += 1;
        }
        let top_sources: Vec<_> = source_counts
            .into_iter()
            .map(|(source, count)| json!({
                "source": source,
                "error_count": count
            }))
            .take(5)
            .collect();

        // Build AI-friendly response
        // The AI gets:
        // - Status summary (quick health check)
        // - Clustered patterns (what's happening, not noise)
        // - Top error sources (where problems are)
        // - Raw events (limited, for detailed analysis if needed)
        let response = json!({
            "status": status,
            "summary": {
                "total_errors_last_hour": snapshot.error_count,
                "unique_error_patterns": snapshot.clustered_insights.len(),
                "kernel_warnings": kernel_panics,
                "potential_crash_loops": crash_loops,
                "top_error_sources": top_sources
            },
            // Clustered insights: The "AI Hero" feature
            // This shows patterns instead of dumping 1000 lines of duplicate errors
            // Example: "WifiDriver: Connection Timeout" repeated 45 times
            "clustered_insights": if include_clusters {
                snapshot.clustered_insights
            } else {
                Vec::new()
            },
            // Raw events: Limited to prevent token overflow
            // UI can show these for detailed debugging
            "recent_events": snapshot.critical_events
                .into_iter()
                .take(max_events)
                .collect::<Vec<_>>(),
            // Platform-specific notes
            "platform_info": {
                "note": "Logs collected using native system tools: journalctl (Linux), log show (macOS), Get-WinEvent (Windows)"
            }
        });

        Ok(response)
    }
}

