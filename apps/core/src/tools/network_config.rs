use async_trait::async_trait;
use serde_json::Value;
use crate::db::DatabaseManager;
use super::Tool;
use std::sync::Arc;
use crate::modules::metrics::SystemCollector;
use crate::modules::datahub::DataHub;
use crate::modules::memory::MemoryManager;

pub struct GetNetworkConfigTool {
    db: Arc<DatabaseManager>,
}

impl GetNetworkConfigTool {
    pub fn new(db: Arc<DatabaseManager>) -> Self { Self { db } }
}

#[async_trait]
impl Tool for GetNetworkConfigTool {
    fn name(&self) -> &str { "get_network_config" }
    fn description(&self) -> &str { "Returns network interfaces and configurations." }
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

    async fn call(&self, _args: &Value, _col: &mut SystemCollector, _dh: &DataHub, _mem: &MemoryManager) -> anyhow::Result<Value> {
        let conn = self.db.get_connection();
        let mut stmt = conn.prepare("SELECT name, mac_address, is_up FROM network_interfaces")?;
        
        let rows = stmt.query_map([], |row| {
            Ok(serde_json::json!({
                "name": row.get::<_, String>(0)?,
                "mac": row.get::<_, String>(1)?,
                "active": row.get::<_, bool>(2)?,
            }))
        })?;

        let interfaces: Vec<Value> = rows.filter_map(|r| r.ok()).collect();
        Ok(serde_json::Value::Array(interfaces))
    }
}

