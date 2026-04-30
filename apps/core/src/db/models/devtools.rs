// ==================================================================================
// 🧠 DIVLENS DEVTOOLS PERSISTENCE MODEL
// ==================================================================================
// This module handles persistent storage of ALL developer tools data:
// - Language runtimes
// - Path conflicts
// - Packages
// Data is stored in the database and only refreshed on:
// - Application restart (optional)
// - Manual refresh button click
// ==================================================================================

use rusqlite::{params, Connection, Result};
use crate::models::devtools::{PackageInfo, LanguageRuntime, PathConflict};

pub struct DevToolsModel;

impl DevToolsModel {
    /// Initialize all devtools tables in the database
    pub fn init(conn: &Connection) -> Result<()> {
        // Packages table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS devtools_packages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                version TEXT NOT NULL,
                source TEXT NOT NULL,
                install_path TEXT,
                last_scan TEXT DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(name, source)
            )",
            [],
        )?;
        
        // Runtimes table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS devtools_runtimes (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                version TEXT NOT NULL,
                active_path TEXT NOT NULL,
                detected_manager TEXT,
                last_scan TEXT DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(name, active_path)
            )",
            [],
        )?;
        
        // Conflicts table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS devtools_conflicts (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                binary TEXT NOT NULL,
                active_source TEXT NOT NULL,
                shadowed_sources TEXT NOT NULL,
                severity TEXT NOT NULL,
                last_scan TEXT DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(binary, active_source)
            )",
            [],
        )?;
        
        // Create indexes for faster lookups
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_devtools_source ON devtools_packages(source)",
            [],
        )?;
        
        Ok(())
    }

    /// Save packages to database (batch insert with upsert)
    pub fn save_packages(conn: &mut Connection, packages: &[PackageInfo]) -> Result<()> {
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT OR REPLACE INTO devtools_packages (name, version, source, install_path, last_scan)
                 VALUES (?1, ?2, ?3, ?4, CURRENT_TIMESTAMP)"
            )?;
            
            for pkg in packages {
                stmt.execute(params![
                    pkg.name,
                    pkg.version,
                    pkg.source,
                    pkg.install_path
                ])?;
            }
        }
        tx.commit()
    }

    /// Get all packages from database
    pub fn get_all_packages(conn: &Connection) -> Result<Vec<PackageInfo>> {
        let mut stmt = conn.prepare(
            "SELECT name, version, source, install_path FROM devtools_packages ORDER BY source, name"
        )?;
        
        let rows = stmt.query_map([], |row| {
            Ok(PackageInfo {
                name: row.get(0)?,
                version: row.get(1)?,
                source: row.get(2)?,
                install_path: row.get(3)?,
            })
        })?;
        
        let mut packages = Vec::new();
        for row in rows {
            packages.push(row?);
        }
        Ok(packages)
    }

    /// Get packages by source (e.g., "pip", "npm", "cargo")
    pub fn get_packages_by_source(conn: &Connection, source: &str) -> Result<Vec<PackageInfo>> {
        let mut stmt = conn.prepare(
            "SELECT name, version, source, install_path FROM devtools_packages 
             WHERE source = ?1 ORDER BY name"
        )?;
        
        let rows = stmt.query_map(params![source], |row| {
            Ok(PackageInfo {
                name: row.get(0)?,
                version: row.get(1)?,
                source: row.get(2)?,
                install_path: row.get(3)?,
            })
        })?;
        
        let mut packages = Vec::new();
        for row in rows {
            packages.push(row?);
        }
        Ok(packages)
    }

    /// Clear all packages from database (used for refresh)
    pub fn clear_packages(conn: &mut Connection) -> Result<()> {
        conn.execute("DELETE FROM devtools_packages", [])?;
        Ok(())
    }

    /// Check if packages exist in database
    pub fn has_packages(conn: &Connection) -> Result<bool> {
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM devtools_packages")?;
        let count: i64 = stmt.query_row([], |row| row.get(0))?;
        Ok(count > 0)
    }

    /// Get last scan timestamp
    pub fn get_last_scan(conn: &Connection) -> Result<Option<String>> {
        let mut stmt = conn.prepare(
            "SELECT MAX(last_scan) FROM devtools_packages"
        )?;
        
        match stmt.query_row([], |row| row.get::<_, String>(0)) {
            Ok(timestamp) => Ok(Some(timestamp)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    // ==================================================================================
    // RUNTIMES PERSISTENCE
    // ==================================================================================

    /// Save runtimes to database (batch insert with upsert)
    pub fn save_runtimes(conn: &mut Connection, runtimes: &[LanguageRuntime]) -> Result<()> {
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT OR REPLACE INTO devtools_runtimes (name, version, active_path, detected_manager, last_scan)
                 VALUES (?1, ?2, ?3, ?4, CURRENT_TIMESTAMP)"
            )?;
            
            for runtime in runtimes {
                stmt.execute(params![
                    runtime.name,
                    runtime.version,
                    runtime.active_path,
                    runtime.detected_manager
                ])?;
            }
        }
        tx.commit()
    }

    /// Get all runtimes from database
    pub fn get_all_runtimes(conn: &Connection) -> Result<Vec<LanguageRuntime>> {
        let mut stmt = conn.prepare(
            "SELECT name, version, active_path, detected_manager FROM devtools_runtimes ORDER BY name"
        )?;
        
        let rows = stmt.query_map([], |row| {
            Ok(LanguageRuntime {
                name: row.get(0)?,
                version: row.get(1)?,
                active_path: row.get(2)?,
                detected_manager: row.get(3)?,
            })
        })?;
        
        let mut runtimes = Vec::new();
        for row in rows {
            runtimes.push(row?);
        }
        Ok(runtimes)
    }

    /// Check if runtimes exist in database
    pub fn has_runtimes(conn: &Connection) -> Result<bool> {
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM devtools_runtimes")?;
        let count: i64 = stmt.query_row([], |row| row.get(0))?;
        Ok(count > 0)
    }

    // ==================================================================================
    // CONFLICTS PERSISTENCE
    // ==================================================================================

    /// Save conflicts to database (batch insert with upsert)
    pub fn save_conflicts(conn: &mut Connection, conflicts: &[PathConflict]) -> Result<()> {
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT OR REPLACE INTO devtools_conflicts (binary, active_source, shadowed_sources, severity, last_scan)
                 VALUES (?1, ?2, ?3, ?4, CURRENT_TIMESTAMP)"
            )?;
            
            for conflict in conflicts {
                // Serialize shadowed_sources as JSON string
                let shadowed_json = serde_json::to_string(&conflict.shadowed_sources)
                    .unwrap_or_else(|_| "[]".to_string());
                
                stmt.execute(params![
                    conflict.binary,
                    conflict.active_source,
                    shadowed_json,
                    conflict.severity
                ])?;
            }
        }
        tx.commit()
    }

    /// Get all conflicts from database
    pub fn get_all_conflicts(conn: &Connection) -> Result<Vec<PathConflict>> {
        let mut stmt = conn.prepare(
            "SELECT binary, active_source, shadowed_sources, severity FROM devtools_conflicts ORDER BY binary"
        )?;
        
        let rows = stmt.query_map([], |row| {
            let shadowed_json: String = row.get(2)?;
            let shadowed: Vec<String> = serde_json::from_str(&shadowed_json)
                .unwrap_or_else(|_| Vec::new());
            
            Ok(PathConflict {
                binary: row.get(0)?,
                active_source: row.get(1)?,
                shadowed_sources: shadowed,
                severity: row.get(3)?,
            })
        })?;
        
        let mut conflicts = Vec::new();
        for row in rows {
            conflicts.push(row?);
        }
        Ok(conflicts)
    }

    /// Check if conflicts exist in database
    pub fn has_conflicts(conn: &Connection) -> Result<bool> {
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM devtools_conflicts")?;
        let count: i64 = stmt.query_row([], |row| row.get(0))?;
        Ok(count > 0)
    }

    /// Clear all devtools data (used for refresh)
    pub fn clear_all(conn: &mut Connection) -> Result<()> {
        conn.execute("DELETE FROM devtools_packages", [])?;
        conn.execute("DELETE FROM devtools_runtimes", [])?;
        conn.execute("DELETE FROM devtools_conflicts", [])?;
        Ok(())
    }
}

