use async_trait::async_trait;
use serde_json::Value;
use crate::modules::metrics::SystemCollector;
use crate::modules::datahub::DataHub;
use crate::modules::memory::MemoryManager;
use super::Tool;

pub struct ScanDirectoryTool;

#[async_trait]
impl Tool for ScanDirectoryTool {
    fn name(&self) -> &str { "scan_directory" }
    fn description(&self) -> &str { "Analyze file system usage for a specific path." }
    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Absolute path to scan" }
            },
            "required": ["path"]
        })
    }
    async fn call(&self, args: &Value, _col: &mut SystemCollector, _dh: &DataHub, _mem: &MemoryManager) -> anyhow::Result<Value> {
        let path = args["path"].as_str().unwrap_or(".");
        let report = crate::modules::filesystem::scan_directory(path);
        Ok(serde_json::to_value(report)?)
    }
}