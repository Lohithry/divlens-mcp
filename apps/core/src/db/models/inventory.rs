use serde::{Serialize, Deserialize};
use rusqlite::{params, Connection, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppInventoryItem {
    pub name: String,
    pub size_mb: f64,
    pub path: String,
    pub install_date: Option<String>,
    pub last_scan: String, 
}

pub struct InventoryModel;

impl InventoryModel {
    pub fn init(conn: &Connection) -> Result<()> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS app_inventory (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                size_mb REAL NOT NULL,
                path TEXT NOT NULL UNIQUE,
                install_date TEXT,
                last_scan TEXT DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;
        Ok(())
    }

    pub fn save_batch(conn: &mut Connection, items: &[AppInventoryItem]) -> Result<()> {
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT OR REPLACE INTO app_inventory (name, size_mb, path, install_date, last_scan) 
                 VALUES (?1, ?2, ?3, ?4, CURRENT_TIMESTAMP)"
            )?;
            
            for item in items {
                stmt.execute(params![
                    item.name,
                    item.size_mb,
                    item.path,
                    item.install_date
                ])?;
            }
        }
        tx.commit()
    }

    pub fn get_all(conn: &Connection) -> Result<Vec<AppInventoryItem>> {
        let mut stmt = conn.prepare("SELECT name, size_mb, path, install_date, last_scan FROM app_inventory ORDER BY size_mb DESC")?;
        
        let iter = stmt.query_map([], |row| {
            Ok(AppInventoryItem {
                name: row.get(0)?,
                size_mb: row.get(1)?,
                path: row.get(2)?,
                install_date: row.get(3)?,
                last_scan: row.get(4)?,
            })
        })?;

        let mut items = Vec::new();
        for item in iter {
            items.push(item?);
        }
        Ok(items)
    }
}

