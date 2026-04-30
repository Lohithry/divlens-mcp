use jwalk::WalkDir;
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Serialize, Default)]
pub struct StorageReport {
    pub total_size: u64,
    pub total_files: u64,
    pub file_counts: HashMap<String, u64>, // e.g., "pdf": 12
    pub file_sizes: HashMap<String, u64>,  // e.g., "pdf": 10485760 bytes
    pub error: Option<String>,
}

pub fn scan_directory(path_str: &str) -> StorageReport {
    let path = Path::new(path_str);
    
    // Safety Check: Don't scan if path doesn't exist
    if !path.exists() {
        return StorageReport {
            error: Some(format!("Path not found: {}", path_str)),
            ..Default::default()
        };
    }

    let mut report = StorageReport::default();

    // The "SWAT Team" - Parallel Scan
    // .skip_hidden(false) includes .git or AppData if needed
    for entry in WalkDir::new(path).skip_hidden(false) {
        match entry {
            Ok(dir_entry) => {
                // We only care about files for size/type stats
                if dir_entry.file_type().is_file() {
                    report.total_files += 1;

                    // Get File Metadata (Size)
                    if let Ok(metadata) = dir_entry.metadata() {
                        let size = metadata.len();
                        report.total_size += size;

                        // Get Extension (e.g., "png")
                        if let Some(ext) = dir_entry.path().extension() {
                            let ext_str = ext.to_string_lossy().to_lowercase();
                            
                            // Increment Count for this extension
                            *report.file_counts.entry(ext_str.clone()).or_insert(0) += 1;
                            
                            // Add Size for this extension
                            *report.file_sizes.entry(ext_str).or_insert(0) += size;
                        } else {
                            // Handle files with no extension
                            *report.file_counts.entry("no_ext".to_string()).or_insert(0) += 1;
                            *report.file_sizes.entry("no_ext".to_string()).or_insert(0) += size;
                        }
                    }
                }
            }
            Err(_) => {
                // Silently skip files we can't access (Permissions)
                continue;
            }
        }
    }

    report
}