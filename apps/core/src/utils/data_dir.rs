//! # Data Directory Utility
//!
//! Provides cross-platform data directory resolution for DivLens.
//! Used by MCP server mode to store databases and caches in OS-appropriate locations.
//!
//! ## Paths by Platform
//! - **macOS**: `~/Library/Application Support/divlens/`
//! - **Linux**: `~/.local/share/divlens/`
//! - **Windows**: `C:\Users\<User>\AppData\Roaming\divlens\`

use std::path::PathBuf;
use directories::ProjectDirs;

/// Get the platform-appropriate data directory for DivLens.
///
/// Creates the directory if it doesn't exist.
/// Falls back to the current directory if platform detection fails.
pub fn get_data_dir() -> PathBuf {
    if let Some(proj) = ProjectDirs::from("com", "divlens", "divlens") {
        let dir = proj.data_dir().to_path_buf();
        if let Err(e) = std::fs::create_dir_all(&dir) {
            tracing::warn!("Failed to create data directory {:?}: {}", dir, e);
            return PathBuf::from(".");
        }
        dir
    } else {
        tracing::warn!("Could not determine platform data directory, using current directory");
        PathBuf::from(".")
    }
}

/// Get the database file path for MCP server mode.
pub fn get_db_path() -> PathBuf {
    get_data_dir().join("divlens.db")
}

/// Get the LanceDB directory path for vector memory.
pub fn get_lancedb_path() -> PathBuf {
    get_data_dir().join("lancedb")
}
