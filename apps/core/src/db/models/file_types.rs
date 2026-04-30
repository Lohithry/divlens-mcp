use serde::{Serialize, Deserialize};
use rusqlite::{params, Connection, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileTypeStatsModel {
    pub extension: String,
    pub count: u64,
    pub size_mb: f64,
}

pub struct FileTypeModel;

impl FileTypeModel {
    pub fn init(conn: &Connection) -> Result<()> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS storage_filetypes (
                extension TEXT PRIMARY KEY,
                count INTEGER NOT NULL,
                size_mb REAL NOT NULL
            )",
            [],
        )?;
        Ok(())
    }

    pub fn save_batch(conn: &mut Connection, items: &[FileTypeStatsModel]) -> Result<()> {
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT OR REPLACE INTO storage_filetypes (extension, count, size_mb) 
                 VALUES (?1, ?2, ?3)"
            )?;
            
            for item in items {
                stmt.execute(params![
                    item.extension,
                    item.count,
                    item.size_mb,
                ])?;
            }
        }
        tx.commit()
    }

    pub fn get_all(conn: &Connection) -> Result<Vec<FileTypeStatsModel>> {
        let mut stmt = conn.prepare("SELECT extension, count, size_mb FROM storage_filetypes ORDER BY size_mb DESC")?;
        
        let iter = stmt.query_map([], |row| {
            Ok(FileTypeStatsModel {
                extension: row.get(0)?,
                count: row.get(1)?,
                size_mb: row.get(2)?,
            })
        })?;

        let mut items = Vec::new();
        for item in iter {
            items.push(item?);
        }
        Ok(items)
    }

    pub fn get_top_consumers(conn: &Connection, limit: usize) -> Result<Vec<FileTypeStatsModel>> {
        let mut stmt = conn.prepare("SELECT extension, count, size_mb FROM storage_filetypes ORDER BY size_mb DESC LIMIT ?1")?;
        
        let iter = stmt.query_map(params![limit], |row| {
            Ok(FileTypeStatsModel {
                extension: row.get(0)?,
                count: row.get(1)?,
                size_mb: row.get(2)?,
            })
        })?;

        let mut items = Vec::new();
        for item in iter {
            items.push(item?);
        }
        Ok(items)
    }
}

