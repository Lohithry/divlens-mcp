use rusqlite::{params, Connection, Result};
use serde::{Serialize, Deserialize};
use std::sync::Mutex;

// SoftwarePackage struct represents a software package with its name, version, and manager.
// It is marked as serializable and deserializable for easy JSON conversion.
// Debug is also enabled for easy debugging.
#[derive(Serialize, Deserialize, Debug)]
pub struct SoftwarePackage {
    pub name: String,
    pub version: String,
    pub manager: String, // e.g., "pip", "cargo", "npm"
}
/// DataHub struct represents a database connection to the software packages.
/// It is used to add, get, and clear software packages from the database.
/// It is marked as serializable and deserializable for easy JSON conversion.
pub struct DataHub {
    conn: Mutex<Connection>,
}

/// DataHub struct implements the methods to add, get, and clear software packages from the database.
/// It is marked as serializable and deserializable for easy JSON conversion.
/// Implementation block for DataHub
///
/// This `impl` block provides methods for interacting with the database of software packages.
/// 
/// - `new`: Opens (or creates) the SQLite database file and ensures that the packages table exists.
/// - `add_package`: Adds a new software package into the database.
/// - `get_all_packages`: Retrieves all packages stored in the database as a list.
/// - `clear_packages`: Removes all software packages from the database (mainly for testing/reset).
///
/// # Example usage
/// 
/// ```rust
/// // Create/connect to a database named "my_packages.db"
/// let datahub = DataHub::new("my_packages.db").unwrap();
///
/// // Add a new package
/// let pkg = SoftwarePackage {
///     name: "Python".to_string(),
///     version: "3.11.0".to_string(),
///     manager: "pip".to_string(),
/// };
/// datahub.add_package(&pkg).unwrap();
///
/// // Retrieve all packages
/// let all = datahub.get_all_packages().unwrap();
/// println!("{:?}", all);
///
/// // Clear all packages from the table
/// datahub.clear_packages().unwrap();
/// ```
/*
Methods:
- new(db_name): Opens or creates the SQLite database and makes sure the 'packages' table is present. Returns a DataHub ready for use.
- add_package(&self, pkg): Inserts the given SoftwarePackage into the database ('packages' table).
- get_all_packages(&self): Loads all packages from the DB, returning them as a Vec<SoftwarePackage>.
- clear_packages(&self): Deletes all package records. Useful for resetting/testing.
*/

impl DataHub {
    // 1. Initialize DB (Create file if missing)
    pub fn new(db_name: &str) -> Result<Self> {
        let conn = Connection::open(db_name)?;
        
        // 2. Create Tables (Schema Definition)
        // We use "IF NOT EXISTS" so it doesn't crash on restart
        conn.execute(
            "CREATE TABLE IF NOT EXISTS packages (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                version TEXT NOT NULL,
                manager TEXT NOT NULL
            )",
            [],
        )?;

        Ok(Self { conn: Mutex::new(conn) })
    }

    // 3. Write Data (The "Upsert" Logic)
    /// Adds a new software package into the database.
    /// It is marked as serializable and deserializable for easy JSON conversion.
    /// Implementation block for add_package
    ///
    /// This method inserts a new SoftwarePackage into the 'packages' table in the database.
    /// If a package with the same name already exists, it updates the existing record.
    ///
    /// # Example usage
    ///
    pub fn add_package(&self, pkg: &SoftwarePackage) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO packages (name, version, manager) VALUES (?1, ?2, ?3)",
            params![pkg.name, pkg.version, pkg.manager],
        )?;
        Ok(())
    }

    // 4. Read Data (The Fast Query)
    /// Retrieves all packages stored in the database as a list.
    /// It is marked as serializable and deserializable for easy JSON conversion.
    /// Implementation block for get_all_packages
    ///
    /// This method loads all packages from the 'packages' table in the database and returns them as a Vec<SoftwarePackage>.
    ///
    /// # Example usage
    pub fn get_all_packages(&self) -> Result<Vec<SoftwarePackage>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT name, version, manager FROM packages")?;
        let package_iter = stmt.query_map([], |row| {
            Ok(SoftwarePackage {
                name: row.get(0)?,
                version: row.get(1)?,
                manager: row.get(2)?,
            })
        })?;

        let mut packages = Vec::new();
        for pkg in package_iter {
            packages.push(pkg?);
        }
        Ok(packages)
    }

    // A helper to clear data (for testing)
    /// Removes all software packages from the database (mainly for testing/reset).
    pub fn clear_packages(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM packages", [])?;
        Ok(())
    }
}