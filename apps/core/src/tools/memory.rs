use async_trait::async_trait;
use serde_json::Value;
use crate::modules::metrics::SystemCollector;
use crate::modules::datahub::DataHub;
use crate::modules::memory::MemoryManager;
use super::Tool;

pub struct RecallMemoryTool;

#[async_trait]
impl Tool for RecallMemoryTool {
    fn name(&self) -> &str { "recall_memory" }
    fn description(&self) -> &str { "Search long-term memory for past fixes." }
    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "The concept to search for" }
            },
            "required": ["query"]
        })
    }
    async fn call(&self, args: &Value, _col: &mut SystemCollector, _dh: &DataHub, memory: &MemoryManager) -> anyhow::Result<Value> {
        let query = args["query"].as_str().unwrap_or("");
        let results = memory.recall(query, 3).await?;
        Ok(serde_json::to_value(results)?)
    }
}