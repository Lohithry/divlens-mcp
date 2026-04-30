use async_trait::async_trait;
use serde_json::{json, Value};
use crate::modules::metrics::SystemCollector;
use crate::modules::datahub::DataHub;
use crate::modules::memory::MemoryManager;
use crate::collectors::ondemand::drivers::get_important_drivers;
use super::Tool;

pub struct GetDriversTool;

#[async_trait]
impl Tool for GetDriversTool {
    fn name(&self) -> &str { "get_system_drivers" }
    fn description(&self) -> &str { "Lazily scans and returns a list of important system drivers (Network, Media, etc)." }
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
        let drivers = get_important_drivers();
        Ok(json!({
            "summary": "Important System Drivers",
            "count": drivers.len(),
            "drivers": drivers
        }))
    }
}

