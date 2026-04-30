use std::collections::HashMap;
use walkdir::{DirEntry, WalkDir};
use directories::UserDirs; // Robust cross-platform paths
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileTypeStats {
    pub extension: String,
    pub count: u64,
    pub size_bytes: u64,
}

pub struct FileTypeCollector;

impl FileTypeCollector {
    pub fn collect() -> Vec<FileTypeStats> {
        let mut stats_map: HashMap<String, (u64, u64)> = HashMap::new();

        // 1. Robustly find User Folders (Cross-Platform)
        if let Some(user_dirs) = UserDirs::new() {
            let mut targets = vec![];
            
            // Add robust paths
            if let Some(p) = user_dirs.download_dir() { targets.push(p); }
            if let Some(p) = user_dirs.document_dir() { targets.push(p); }
            if let Some(p) = user_dirs.desktop_dir() { targets.push(p); }
            if let Some(p) = user_dirs.picture_dir() { targets.push(p); }
            if let Some(p) = user_dirs.video_dir() { targets.push(p); }
            if let Some(p) = user_dirs.audio_dir() { targets.push(p); }

            for target in targets {
                if target.exists() {
                    // 2. The "Smart Walker" - Skips Junk Folders efficiently
                    let walker = WalkDir::new(target).into_iter();
                    
                    // filter_entry is CRITICAL: It stops us from even entering "node_modules"
                    for entry in walker.filter_entry(|e| !is_junk_folder(e)) {
                        if let Ok(e) = entry {
                            if e.path().is_file() {
                                process_file(&e, &mut stats_map);
                            }
                        }
                    }
                }
            }
        }

        // Convert Map to Vector
        stats_map.into_iter().map(|(ext, (count, size))| {
            FileTypeStats { extension: ext, count, size_bytes: size }
        }).collect()
    }
}

// --- HELPER LOGIC ---

// 3. The "Junk Filter": Returns TRUE if we should SKIP this folder
fn is_junk_folder(entry: &DirEntry) -> bool {
    let name = entry.file_name().to_string_lossy();
    // Common heavy developer folders to ignore
    name == "node_modules" || 
    name == "target" || 
    name == ".git" || 
    name == "venv" ||
    name == "dist" ||
    name == "build" ||
    name == ".idea" ||
    name == ".vscode" ||
    name.starts_with('.') // Ignore hidden folders (.config, .cache)
}

fn process_file(entry: &DirEntry, map: &mut HashMap<String, (u64, u64)>) {
    if let Some(ext_os) = entry.path().extension() {
        // 4. Normalize Extension (Lowercase)
        let ext = ext_os.to_string_lossy().to_lowercase();
        
        // Skip temp files or weird system files or excessively long extensions
        if ext.len() > 10 || ext.chars().any(|c| !c.is_alphanumeric()) {
            return;
        }

        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        
        let stat = map.entry(ext).or_insert((0, 0));
        stat.0 += 1;
        stat.1 += size;
    }
}
