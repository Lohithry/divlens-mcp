use async_trait::async_trait;
use serde_json::Value;
use crate::modules::metrics::SystemCollector;
use crate::modules::datahub::DataHub;
use crate::modules::memory::MemoryManager;
use super::Tool;
use std::sync::{Arc, Mutex};
use crate::collectors::volatile::VolatileCollector;

pub struct GetLiveMetricsTool {
    collector: Arc<Mutex<VolatileCollector>>,
}

impl GetLiveMetricsTool {
    pub fn new(collector: Arc<Mutex<VolatileCollector>>) -> Self {
        Self { collector }
    }
}

#[async_trait]
impl Tool for GetLiveMetricsTool {
    fn name(&self) -> &str { "get_live_metrics" }
    fn description(&self) -> &str { 
        "Retrieves a complete snapshot of system health, including basic usage (CPU%, RAM GB) and advanced diagnostics. \
        Use this to diagnose performance issues, lag, or stability problems. \
        Returns:\n\
        - CPU: Load, Context Switches (high = multitasking stress), Blocked Processes (I/O wait)\n\
        - RAM: Used/Cached split, Swap Usage, Thrashing Status (Critical if RAM+Swap full)\n\
        - Net: Upload/Download, Active Sockets, Packet Errors (high = quality loss)" 
    }
    fn schema(&self) -> Value { 
        serde_json::json!({ 
            "type": "object", 
            "properties": {
                // TRICK: AWS Bedrock hates empty schemas. We add a dummy field.
                "_dummy": { 
                    "type": "string",
                    "description": "Ignore this parameter."
                }
            }
        }) 
    }

    async fn call(&self, _args: &Value, _legacy_col: &mut SystemCollector, _dh: &DataHub, _mem: &MemoryManager) -> anyhow::Result<Value> {
        // Use the new VolatileCollector to get the correct data shape
        // Offload to blocking thread to prevent blocking async runtime
        let collector_clone = self.collector.clone();
        
        let metrics = tokio::task::spawn_blocking(move || {
            let mut col = collector_clone.lock().unwrap();
            col.collect_snapshot()
        }).await?;
        
        Ok(serde_json::to_value(metrics)?)
    }
}
