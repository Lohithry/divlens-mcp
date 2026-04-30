use crate::db::DatabaseManager;
use super::Collector;
use sysinfo::Disks;

pub struct StorageCollector;

impl Collector for StorageCollector {
    fn collect(_db: &DatabaseManager) -> anyhow::Result<()> {
        // Disks is now its own struct
        let _disks = Disks::new_with_refreshed_list();
        // You would insert into a 'storage_devices' table here (need to add to schema)
        // For now, we leave placeholder
        Ok(())
    }
}
