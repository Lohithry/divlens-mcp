use crate::db::DatabaseManager;
use super::Collector;
use sysinfo::Networks;

pub struct NetworkCollector;

impl Collector for NetworkCollector {
    fn collect(db: &DatabaseManager) -> anyhow::Result<()> {
        // Networks is now its own struct, separate from System
        let networks = Networks::new_with_refreshed_list();
        
        let conn_guard = db.get_connection();
        let conn = &*conn_guard;
        conn.execute("DELETE FROM network_interfaces", [])?;
        
        for (name, data) in &networks {
            conn.execute(
                "INSERT INTO network_interfaces (name, mac_address, is_up) VALUES (?1, ?2, 1)",
                (name, data.mac_address().to_string()),
            )?;
        }
        Ok(())
    }
}
