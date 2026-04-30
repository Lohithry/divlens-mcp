use crate::db::DatabaseManager;
use super::Collector;
use sysinfo::{System, ProcessesToUpdate};

pub struct ApplicationsCollector;

impl Collector for ApplicationsCollector {
    fn collect(db: &DatabaseManager) -> anyhow::Result<()> {
        let mut sys = System::new_all();
        sys.refresh_processes(ProcessesToUpdate::All, true);
        
        // Note: sysinfo processes aren't exactly "Installed Apps", but it's a start for running apps.
        // For a true "Installed Apps" list, we'd need OS-specific queries (e.g. /Applications on Mac).
        // For now, we'll just populate with a placeholder or partial scan if needed.
        // The schema has: name, version, install_date, size_mb
        
        let conn_guard = db.get_connection();
        let conn = &*conn_guard;
        conn.execute("DELETE FROM installed_apps", [])?;

        // In a real implementation, we would iterate over /Applications or use mdfind on macOS
        
        Ok(())
    }
}
