use async_trait::async_trait;
use serde_json::{Value, json};
// use std::sync::{Arc, Mutex};
use crate::modules::metrics::SystemCollector;
use crate::modules::datahub::DataHub;
use crate::modules::memory::MemoryManager;
use crate::tools::Tool;
use crate::collectors::volatile::disk_health::DiskHealthCollector;

pub struct GetStorageDiagnosticsTool;

#[async_trait]
impl Tool for GetStorageDiagnosticsTool {
    fn name(&self) -> &str { "get_storage_diagnostics" }
    fn description(&self) -> &str { 
        "Returns volatile diagnostic data for disks: IOPS (Activity), Throughput (Speed), and SMART Health (Physical Status). Use this to diagnose 'slow' systems where the disk might be the bottleneck." 
    }
    
    fn schema(&self) -> Value {
        json!({
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

    async fn call(&self, _args: &Value, _col: &mut SystemCollector, _dh: &DataHub, _mem: &MemoryManager) -> anyhow::Result<Value> {
        // This collector is lightweight and volatile, so we can instantiate it on-the-fly 
        // or (better) we could add it to the main VolatileCollector.
        // For MVP simplicity and robustness (avoiding lock contention on main loop), 
        // we instantiate a dedicated reader here. sysinfo is fast enough.
        
        let mut collector = DiskHealthCollector::new();
        let diagnostics = collector.update();
        
        Ok(json!(diagnostics))
    }
}

