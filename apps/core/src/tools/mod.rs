pub mod dev_stack;
pub mod drivers;
pub mod filesystem;
pub mod hardware_diagnostics;
pub mod live_metrics;
pub mod memory;
pub mod network_config;
pub mod network_diagnostics;
pub mod processes;
pub mod storage;
pub mod storage_diagnostics;
pub mod system_dna;
pub mod system_logs;

#[cfg(test)]
mod hardware_diagnostics_tests;

use async_trait::async_trait;
use serde_json::Value;
use crate::modules::metrics::SystemCollector;
use crate::modules::datahub::DataHub;
use crate::modules::memory::MemoryManager;

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn schema(&self) -> Value;
    async fn call(&self, args: &Value, col: &mut SystemCollector, dh: &DataHub, mem: &MemoryManager) -> anyhow::Result<Value>;
}
