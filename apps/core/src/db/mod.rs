pub mod models;

use rusqlite::{Connection, Result};
use std::sync::{Arc, Mutex};
use self::models::inventory::InventoryModel;
use self::models::file_types::FileTypeModel;
use self::models::devtools::DevToolsModel;

pub struct DatabaseManager {
    pool: Arc<Mutex<Connection>>,
}

impl DatabaseManager {
    pub fn new(path: &str) -> Result<Self> {
        let conn = Connection::open(path)?;
        Ok(Self {
            pool: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn init_schema(&self) -> Result<()> {
        let conn = self.pool.lock().unwrap();
        
        // Initialize Inventory Table
        InventoryModel::init(&conn)?;
        // Initialize FileType Table
        FileTypeModel::init(&conn)?;
        // Initialize DevTools Packages Table
        DevToolsModel::init(&conn)?;
        
        Ok(())
    }

    pub fn get_connection(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.pool.lock().unwrap()
    }
}

