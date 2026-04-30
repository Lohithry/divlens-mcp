use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::collectors::persistent::storage::{StorageCollector, AppInventory};
use crate::modules::metrics::SystemCollector;
use crate::modules::datahub::DataHub;
use crate::modules::memory::MemoryManager;
use crate::tools::Tool;
use crate::db::DatabaseManager;
use crate::db::models::inventory::{InventoryModel, AppInventoryItem};
use crate::collectors::persistent::file_types::FileTypeCollector;
use crate::db::models::file_types::{FileTypeModel, FileTypeStatsModel};
use crate::collectors::persistent::advanced_stats::AdvancedStorageCollector;

// --- Tool 1: Get Disk Health (Fast) ---

pub struct GetStorageHealthTool {
    pub collector: Arc<Mutex<StorageCollector>>,
}

#[async_trait]
impl Tool for GetStorageHealthTool {
    fn name(&self) -> &str { "get_storage_health" }
    fn description(&self) -> &str { 
        "Returns the health, total size, and available space of all disk drives. Use this to check for low disk space." 
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

    async fn call(&self, _args: &Value, _col: &mut SystemCollector, _dh: &DataHub, _mem: &MemoryManager) -> anyhow::Result<Value> {
        let mut collector = self.collector.lock().await;
        let health = collector.scan_health();
        Ok(serde_json::to_value(health)?)
    }
}

// --- Tool 2: Scan Inventory (Heavy + Persistent) ---

pub struct ScanStorageInventoryTool {
    pub collector: Arc<Mutex<StorageCollector>>,
    pub db: Arc<DatabaseManager>,
}

#[derive(Deserialize)]
struct InventoryArgs {
    force_refresh: bool,
}

#[async_trait]
impl Tool for ScanStorageInventoryTool {
    fn name(&self) -> &str { "scan_storage_inventory" }
    fn description(&self) -> &str { 
        "Scans for installed applications and large folders. This is a heavy operation. Use 'force_refresh' to trigger a new scan, otherwise it returns cached data from the database." 
    }
    
    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "force_refresh": { "type": "boolean", "description": "If true, performs a fresh filesystem scan and updates the database." }
            }
        })
    }

    async fn call(&self, args: &Value, _col: &mut SystemCollector, _dh: &DataHub, _mem: &MemoryManager) -> anyhow::Result<Value> {
        let args: InventoryArgs = serde_json::from_value(args.clone()).unwrap_or(InventoryArgs { force_refresh: false });
        
        // 1. Try to load from DB if not forcing refresh
        if !args.force_refresh {
            let conn = self.db.get_connection();
            if let Ok(items) = InventoryModel::get_all(&conn) {
                if !items.is_empty() {
                    // Map DB items back to AppInventory struct for consistency
                    let result: Vec<AppInventory> = items.into_iter().map(|item| {
                        AppInventory {
                            name: item.name,
                            size_mb: item.size_mb,
                            path: item.path,
                            install_date: item.install_date,
                        }
                    }).collect();
                    return Ok(serde_json::to_value(result)?);
                }
            }
        }

        // 2. Perform Scan (Heavy)
        let collector = self.collector.lock().await;
        let inventory = collector.scan_inventory();
        
        // 3. Save to DB
        let db_items: Vec<AppInventoryItem> = inventory.iter().map(|app| {
            AppInventoryItem {
                name: app.name.clone(),
                size_mb: app.size_mb,
                path: app.path.clone(),
                install_date: app.install_date.clone(),
                last_scan: String::new(), // Default timestamp
            }
        }).collect();

        // New scope for mutable borrow of connection
        {
            let mut conn = self.db.get_connection();
            if let Err(e) = InventoryModel::save_batch(&mut conn, &db_items) {
                eprintln!("Failed to save inventory to DB: {}", e);
            }
        }
        
        Ok(serde_json::to_value(inventory)?)
    }
}

// --- Tool 3: Get File Type Summary (Smart Scan) ---

pub struct GetFileTypeSummaryTool {
    pub db: Arc<DatabaseManager>,
}

#[async_trait]
impl Tool for GetFileTypeSummaryTool {
    fn name(&self) -> &str { "get_filetype_summary" }
    fn description(&self) -> &str {
        "Analyzes file types in user directories (Documents, Downloads, etc.) to show storage usage by extension. Returns top consumers."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                 "force_refresh": { "type": "boolean", "description": "If true, performs a fresh scan. Otherwise returns cached data." }
            }
        })
    }

    async fn call(&self, args: &Value, _col: &mut SystemCollector, _dh: &DataHub, _mem: &MemoryManager) -> anyhow::Result<Value> {
        let force_refresh = args["force_refresh"].as_bool().unwrap_or(false);

        // 1. Try to load from DB if not forcing refresh
        if !force_refresh {
            let conn = self.db.get_connection();
            if let Ok(items) = FileTypeModel::get_top_consumers(&conn, 10) {
                 if !items.is_empty() {
                     return Ok(serde_json::json!({
                         "summary": "Storage Breakdown by File Type (Cached)",
                         "top_consumers": items
                     }));
                 }
            }
        }

        // 2. Perform Scan (Heavy - but optimized)
        // Run in blocking task because it does IO
        let stats = tokio::task::spawn_blocking(move || {
            FileTypeCollector::collect()
        }).await?;

        // 3. Save to DB
        let db_items: Vec<FileTypeStatsModel> = stats.iter().map(|s| {
            FileTypeStatsModel {
                extension: s.extension.clone(),
                count: s.count,
                size_mb: s.size_bytes as f64 / 1_048_576.0, // Convert Bytes to MB
            }
        }).collect();

        {
            let mut conn = self.db.get_connection();
            if let Err(e) = FileTypeModel::save_batch(&mut conn, &db_items) {
                eprintln!("Failed to save file stats to DB: {}", e);
            }
        }

        // Return Top 10 for display
        let mut top_items = db_items.clone();
        top_items.sort_by(|a, b| b.size_mb.partial_cmp(&a.size_mb).unwrap());
        top_items.truncate(10);

        Ok(serde_json::json!({
            "summary": "Storage Breakdown by File Type",
            "top_consumers": top_items
        }))
    }
}

// --- Tool 4: Get Specific File Type Stats ---

pub struct GetSpecificFileTypeTool {
    pub db: Arc<DatabaseManager>,
}

#[async_trait]
impl Tool for GetSpecificFileTypeTool {
    fn name(&self) -> &str { "get_specific_filetype_stats" }
    fn description(&self) -> &str {
        "Returns total count and size for a specific file extension (e.g., 'pdf', 'mp4')."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "extension": { "type": "string", "description": "The file extension to check (e.g., 'pdf')" }
            },
            "required": ["extension"]
        })
    }

    async fn call(&self, args: &Value, _col: &mut SystemCollector, _dh: &DataHub, _mem: &MemoryManager) -> anyhow::Result<Value> {
        let ext = args["extension"].as_str().unwrap_or("").to_lowercase();
        // Clean input (remove dot if user typed ".pdf")
        let clean_ext = ext.trim_start_matches('.').to_string();

        let conn = self.db.get_connection();
        let mut stmt = conn.prepare(
            "SELECT count, size_mb FROM storage_filetypes WHERE extension = ?1"
        )?;

        // We use query_row because extension is unique
        let result = stmt.query_row([clean_ext.clone()], |row| {
            let count: i64 = row.get(0)?;
            let size_mb: f64 = row.get(1)?;
            
            // Helper to format MB nicely
            let formatted_size = if size_mb >= 1024.0 {
                format!("{:.2} GB", size_mb / 1024.0)
            } else {
                format!("{:.2} MB", size_mb)
            };

            Ok(serde_json::json!({
                "extension": clean_ext,
                "count": count,
                "size_mb": size_mb,
                "size_formatted": formatted_size
            }))
        });

        match result {
            Ok(json) => Ok(json),
            Err(_) => Ok(serde_json::json!({ 
                "extension": clean_ext,
                "count": 0,
                "size_mb": 0.0,
                "size_formatted": "0 MB",
                "message": "No files found of this type." 
            }))
        }
    }
}

// --- Tool 5: Get Advanced Storage Stats (Staleness & Top Files) ---

pub struct GetAdvancedStorageStatsTool;

#[async_trait]
impl Tool for GetAdvancedStorageStatsTool {
    fn name(&self) -> &str { "get_storage_analysis" } // Matches the requested name
    fn description(&self) -> &str {
        "Deep scan that provides 'Top 50 Largest Files' and 'Staleness' analysis (Cold vs Fresh data). Use this to find what to delete."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                // Future: Add path override?
            }
        })
    }

    async fn call(&self, _args: &Value, _col: &mut SystemCollector, _dh: &DataHub, _mem: &MemoryManager) -> anyhow::Result<Value> {
        // Run in blocking task because it scans the filesystem
        let stats = tokio::task::spawn_blocking(move || {
            AdvancedStorageCollector::scan()
        }).await?;

        // Format for AI consumption
        Ok(serde_json::json!({
            "status": "success",
            "insights": {
                "stale_data_mb": stats.stale_files_size_mb,
                "fresh_data_mb": stats.fresh_files_size_mb,
                "stale_data_gb": (stats.stale_files_size_mb / 1024.0).round(),
                "largest_individual_files": stats.top_files
            },
            "summary_text": format!("Found {:.1} GB of stale data (> 6 months old) and {} large files.", stats.stale_files_size_mb / 1024.0, stats.top_files.len())
        }))
    }
}
