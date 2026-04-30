// Volatile (RAM-only, fast changing)
pub mod volatile;

// Persistent (Disk/DB backed, slow changing)
pub mod persistent;

// On-Demand (Lazy, triggered by user/AI)
pub mod ondemand;

// Hardware (Native diagnostics for predictive analysis)
pub mod hardware;

// Re-export specific collectors if needed for direct access in main.rs legacy code
// However, preferred access is via the submodules above.

use crate::db::DatabaseManager;

// Scanner Interface
#[async_trait::async_trait]
pub trait Collector: Send + Sync {
    fn name(&self) -> &str;
    async fn collect(&self, db: &DatabaseManager) -> anyhow::Result<()>;
}

// System Scanner Orchestrator (Runs all persistent collectors)
pub struct SystemScanner;

impl SystemScanner {
    pub async fn run_all(_db: &DatabaseManager) -> anyhow::Result<()> {
        // Run persistent collectors that populate the DB
        // For now, only storage logic is implemented fully as persistent
        // We will skip for now or implement if needed
        Ok(())
    }
}
