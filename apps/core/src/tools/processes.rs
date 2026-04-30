// ==================================================================================
// 🧠 DIVLENS PROCESS LIST TOOL
// ==================================================================================
// This tool exposes the Process Scanner to:
// 1. The React Frontend (for the System Page)
// 2. The AI Agents (via MCP) so they can "see" what's running.
//
// DESIGN PRINCIPLES:
// - Stateless: It doesn't store data, just queries the Collector.
// - Safe: It handles locking the shared System object.
// - Typed: It uses Serde to strictly validate inputs.
// ==================================================================================

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use std::sync::{Arc, Mutex};
use tracing::warn;

use crate::modules::metrics::SystemCollector;
use crate::modules::datahub::DataHub;
use crate::modules::memory::MemoryManager;
use crate::collectors::volatile::VolatileCollector; // Import our Logic Module
use super::Tool;

// 1. Define Valid Input Arguments
#[derive(Deserialize)]
struct ProcessArgs {
    sort_by: Option<String>, // Optional: "cpu", "ram", "name" (Default: "cpu")
    limit: Option<usize>,    // Optional: Number of results (Default: 50)
}

pub struct GetProcessListTool {
    // We hold a reference to the Shared Volatile Collector
    collector: Arc<Mutex<VolatileCollector>>,
}

impl GetProcessListTool {
    pub fn new(collector: Arc<Mutex<VolatileCollector>>) -> Self {
        Self { collector }
    }
}

#[async_trait]
impl Tool for GetProcessListTool {
    fn name(&self) -> &str { "get_process_list" }
    
    // AI-Optimized Description
    // We explicitly tell the AI what this does so it knows when to use it.
    fn description(&self) -> &str { 
        "Returns a snapshot of currently running system processes. \
        Use this to identify what apps are consuming high CPU or Memory. \
        Supports sorting by 'cpu', 'ram', or 'name'. Returns limited results to prevent overwhelming responses."
    }
    
    // JSON Schema for MCP/Frontend validation
    // NOTE: AWS Bedrock has strict schema requirements:
    // - No 'enum' fields (use description instead)
    // - No 'default' fields (use description instead)
    // - No 'minimum'/'maximum' fields (use description instead)
    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "sort_by": {
                    "type": "string",
                    "description": "Metric to sort by. Valid values: 'cpu', 'ram', or 'name'. Default is 'cpu' if not specified."
                },
                "limit": {
                    "type": "integer",
                    "description": "Max number of processes to return (must be >= 1). Default is 50 if not specified."
                }
            }
        })
    }

    async fn call(&self, args: &Value, _legacy_col: &mut SystemCollector, _dh: &DataHub, _mem: &MemoryManager) -> anyhow::Result<Value> {
        // 1. Parse Arguments safely
        let args: ProcessArgs = serde_json::from_value(args.clone())
            .map_err(|e| anyhow::anyhow!("Invalid arguments for get_process_list: {}", e))?;
        
        // Validate and set defaults
        let limit = args.limit.unwrap_or(50);
        // Ensure limit is reasonable (prevent excessive memory usage and AWS Bedrock rejection)
        // Cap at 10 to prevent overly large tool responses that AWS Bedrock might reject
        let limit = limit.min(25).max(1);
        
        let sort_by = args.sort_by.unwrap_or("cpu".to_string());
        // Validate sort_by value
        let sort_by = match sort_by.as_str() {
            "cpu" | "ram" | "name" => sort_by,
            _ => {
                // Invalid sort_by, default to "cpu"
                "cpu".to_string()
            }
        };

        // 2. Refresh Processes in a BLOCKING task
        //    Scanning processes is CPU-intensive and can block the async runtime.
        //    We offload it to a blocking thread to ensure keep-alives continue sending.
        let collector_clone = self.collector.clone();
        let limit_clone = limit;
        let sort_by_clone = sort_by.clone();

        let procs = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<crate::collectors::volatile::processes::ProcessDto>> {
            // Lock inside the blocking thread
            let mut col = collector_clone.lock().map_err(|e| {
                anyhow::anyhow!("Failed to acquire lock on process collector: {}", e)
        })?;

            // Refresh and scan (blocking operation)
            Ok(col.scan_processes_snapshot(limit_clone, &sort_by_clone))
        }).await??; // First ? for JoinHandle error, second ? for Result inside closure
        
        // Get the count
        let proc_count = procs.len();
        
        // 4. Serialize to JSON
        //    This should never fail, but handle it gracefully just in case
        let result = serde_json::to_value(procs)
            .map_err(|e| anyhow::anyhow!("Failed to serialize process list to JSON: {}", e))?;

        // 5. Check result size to prevent AWS Bedrock rejection
        // AWS Bedrock may reject tool results that are too large
        let json_string = serde_json::to_string(&result)
            .map_err(|e| anyhow::anyhow!("Failed to stringify JSON result: {}", e))?;

        if json_string.len() > 15000 { // 10KB limit
            warn!("Process list JSON too large ({} bytes), truncating to prevent AWS Bedrock rejection", json_string.len());
            // Return a summary instead of full data
            return Ok(serde_json::json!({
                "error": "Process list too large for AI analysis",
                "summary": format!("Found {} processes, but result too large", proc_count),
                "recommendation": "Use system monitoring tools directly for full process analysis"
            }));
        }

        Ok(result)
    }
}
