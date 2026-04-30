use sysinfo::Disks;
use serde::{Serialize, Deserialize};

/// Represents the health and usage of a single disk drive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskHealth {
    pub name: String,
    pub mount_point: String,
    pub total_space_gb: f64,
    pub available_space_gb: f64,
    pub is_removable: bool,
    pub file_system: String,
}

/// Represents an installed application or large directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppInventory {
    pub name: String,
    pub size_mb: f64,
    pub path: String,
    pub install_date: Option<String>,
}

// Windows app inventory cache (2 minute cache for heavy operation)
#[cfg(target_os = "windows")]
static WINDOWS_APP_CACHE: Mutex<Option<(Instant, Vec<AppInventory>)>> = Mutex::new(None);
#[cfg(target_os = "windows")]
const WINDOWS_APP_CACHE_DURATION: Duration = Duration::from_secs(120); // 2 minutes

pub struct StorageCollector {
    disks: Disks,
}

impl StorageCollector {
    pub fn new() -> Self {
        Self {
            disks: Disks::new_with_refreshed_list(),
        }
    }

    /// Fast scan: Returns health and usage stats for all mounted disks.
    pub fn scan_health(&mut self) -> Vec<DiskHealth> {
        self.disks.refresh(true);
        
        let mut seen_names = std::collections::HashSet::new();
        let mut results = Vec::new();

        for disk in self.disks.iter() {
            let name = disk.name().to_string_lossy().to_string();
            let mount = disk.mount_point().to_string_lossy().to_string();
            
            // Filter out duplicates and special partitions
            // On macOS, we often see multiple entries for the same APFS container.
            // We prioritize the root mount "/" and real volumes.
            if seen_names.contains(&name) {
                // If we already have this disk name, skip it UNLESS it's the root mount "/"
                // which implies the previous one was a sub-volume we might want to replace?
                // Actually, simple logic: if name exists, ignore.
                continue;
            }
            
            // Ignore small boot/recovery partitions (less than 1GB usually)
            if disk.total_space() < 1_000_000_000 {
                continue;
            }

            seen_names.insert(name.clone());

            let total = disk.total_space() as f64 / 1024.0 / 1024.0 / 1024.0;
            let available = disk.available_space() as f64 / 1024.0 / 1024.0 / 1024.0;
            
            results.push(DiskHealth {
                name,
                mount_point: mount,
                total_space_gb: (total * 100.0).round() / 100.0,
                available_space_gb: (available * 100.0).round() / 100.0,
                is_removable: disk.is_removable(),
                file_system: disk.file_system().to_string_lossy().to_string(),
            });
        }
        results
    }

    /// Heavy scan: Returns a list of installed applications or large folders.
    pub fn scan_inventory(&self) -> Vec<AppInventory> {
        #[cfg(target_os = "macos")]
        return self.scan_inventory_macos();

        #[cfg(target_os = "windows")]
        return self.scan_inventory_windows();

        #[cfg(target_os = "linux")]
        return self.scan_inventory_linux();

        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        vec![]
    }

    #[cfg(target_os = "macos")]
    fn scan_inventory_macos(&self) -> Vec<AppInventory> {
        use std::fs;
        use std::path::Path;

        // Recursive function to calculate directory size
        fn get_dir_size(path: &Path) -> u64 {
            let mut size = 0;
            if let Ok(entries) = fs::read_dir(path) {
                for entry in entries.flatten() {
                    if let Ok(meta) = entry.metadata() {
                        if meta.is_dir() {
                            size += get_dir_size(&entry.path());
                        } else {
                            size += meta.len();
                        }
                    }
                }
            }
            size
        }

        let app_dir = Path::new("/Applications");
        let mut apps = Vec::new();

        if let Ok(entries) = fs::read_dir(app_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                // We only care about .app bundles
                if path.extension().map_or(false, |ext| ext == "app") {
                    let name = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
                    
                    // Calculate real size
                    // Note: This can be slow, but since we run this async in a separate thread (via Tool), it's okay.
                    let size_bytes = get_dir_size(&path);
                    let size_mb = size_bytes as f64 / 1024.0 / 1024.0;
                    
                    apps.push(AppInventory {
                        name,
                        size_mb: (size_mb * 10.0).round() / 10.0, // 1 decimal place
                        path: path.to_string_lossy().to_string(),
                        install_date: None,
                    });
                }
            }
        }
        
        // Sort by size descending
        apps.sort_by(|a, b| b.size_mb.partial_cmp(&a.size_mb).unwrap_or(std::cmp::Ordering::Equal));
        
        // Return top 50 to avoid huge JSON
        apps.into_iter().take(50).collect()
    }

    #[cfg(target_os = "windows")]
    fn scan_inventory_windows(&self) -> Vec<AppInventory> {
        // Check cache first
        {
            let cache = WINDOWS_APP_CACHE.lock().unwrap();
            if let Some((timestamp, ref apps)) = *cache {
                if timestamp.elapsed() < WINDOWS_APP_CACHE_DURATION {
                    // Clone apps BEFORE any other operations
                    let result = apps.clone();
                    
                    // Check if we should trigger background refresh (80% expired)
                    let refresh_threshold = WINDOWS_APP_CACHE_DURATION * 4 / 5;
                    let should_refresh = timestamp.elapsed() > refresh_threshold;
                    
                    // Drop cache lock before spawning thread
                    drop(cache);
                    
                    if should_refresh {
                        trigger_background_app_refresh();
                    }
                    
                    return result;
                }
            }
        }
        
        // Cache miss or expired - fetch fresh data
        let apps = fetch_windows_apps_actual();
        
        // Update cache
        {
            let mut cache = WINDOWS_APP_CACHE.lock().unwrap();
            *cache = Some((Instant::now(), apps.clone()));
        }
        
        apps
    }

    #[cfg(target_os = "linux")]
    fn scan_inventory_linux(&self) -> Vec<AppInventory> {
        use std::process::Command;
        use std::fs;
        use std::path::Path;
        
        let mut apps = Vec::new();
        
        // Method 1: Use dpkg (Debian/Ubuntu) - FAST, reads from database file
        // No cache needed - dpkg-query reads /var/lib/dpkg/status directly
        if let Ok(output) = Command::new("dpkg-query")
            .args(&["-W", "-f", "${Package}|${Installed-Size}|${Status}\n"])
            .output()
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    let parts: Vec<&str> = line.split('|').collect();
                    if parts.len() >= 3 && parts[2].contains("installed") {
                        if let Ok(size_kb) = parts[1].parse::<f64>() {
                            let size_mb = size_kb / 1024.0;
                            if size_mb > 10.0 {
                                apps.push(AppInventory {
                                    name: parts[0].to_string(),
                                    size_mb: (size_mb * 10.0).round() / 10.0,
                                    path: format!("/usr/share/{}", parts[0]),
                                    install_date: None,
                                });
                            }
                        }
                    }
                }
            }
        }
        
        // Method 2: Try rpm (Fedora/RHEL/CentOS) if dpkg failed - FAST, reads from RPM database
        if apps.is_empty() {
            if let Ok(output) = Command::new("rpm")
                .args(&["-qa", "--queryformat", "%{NAME}|%{SIZE}|%{INSTALLTIME}\n"])
                .output()
            {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    for line in stdout.lines() {
                        let parts: Vec<&str> = line.split('|').collect();
                        if parts.len() >= 2 {
                            if let Ok(size_bytes) = parts[1].parse::<f64>() {
                                let size_mb = size_bytes / 1024.0 / 1024.0;
                                if size_mb > 10.0 {
                                    let install_date = parts.get(2)
                                        .and_then(|ts| ts.parse::<i64>().ok())
                                        .map(|ts| {
                                            // Convert Unix timestamp to date string
                                            chrono::DateTime::from_timestamp(ts, 0)
                                                .map(|dt| dt.format("%Y-%m-%d").to_string())
                                                .unwrap_or_default()
                                        })
                                        .filter(|s| !s.is_empty());
                                    
                                    apps.push(AppInventory {
                                        name: parts[0].to_string(),
                                        size_mb: (size_mb * 10.0).round() / 10.0,
                                        path: format!("/usr/share/{}", parts[0]),
                                        install_date,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
        
        // Method 3: Try pacman (Arch Linux) if others failed - FAST
        if apps.is_empty() {
            if let Ok(output) = Command::new("pacman")
                .args(&["-Qi"])
                .output()
            {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let mut current_name = String::new();
                    let mut current_size: f64 = 0.0;
                    
                    for line in stdout.lines() {
                        if line.starts_with("Name") {
                            current_name = line.split(':').nth(1).unwrap_or("").trim().to_string();
                        } else if line.starts_with("Installed Size") {
                            // Parse size like "123.45 MiB" or "1.23 GiB"
                            let size_str = line.split(':').nth(1).unwrap_or("").trim();
                            if let Some((num, unit)) = size_str.split_once(' ') {
                                if let Ok(num) = num.parse::<f64>() {
                                    current_size = match unit.to_lowercase().as_str() {
                                        "gib" => num * 1024.0,
                                        "mib" => num,
                                        "kib" => num / 1024.0,
                                        _ => num,
                                    };
                                }
                            }
                            
                            // Save the package
                            if !current_name.is_empty() && current_size > 10.0 {
                                apps.push(AppInventory {
                                    name: current_name.clone(),
                                    size_mb: (current_size * 10.0).round() / 10.0,
                                    path: format!("/usr/share/{}", current_name),
                                    install_date: None,
                                });
                            }
                            current_name.clear();
                            current_size = 0.0;
                        }
                    }
                }
            }
        }
        
        // Method 4: Fallback - Scan /opt and /usr/local for large directories
        // Only if no package manager found (rare case)
        if apps.is_empty() {
            let scan_dirs = vec!["/opt", "/usr/local"];
            
            fn get_dir_size(path: &Path, depth: u32) -> u64 {
                if depth > 3 { return 0; }
                let mut size = 0;
                if let Ok(entries) = fs::read_dir(path) {
                    for entry in entries.flatten() {
                        if let Ok(meta) = entry.metadata() {
                            if meta.is_dir() {
                                size += get_dir_size(&entry.path(), depth + 1);
                            } else {
                                size += meta.len();
                            }
                        }
                    }
                }
                size
            }
            
            for dir in scan_dirs {
                let path = Path::new(dir);
                if let Ok(entries) = fs::read_dir(path) {
                    for entry in entries.flatten() {
                        let entry_path = entry.path();
                        if entry_path.is_dir() {
                            let name = entry_path.file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .to_string();
                            
                            let size_bytes = get_dir_size(&entry_path, 0);
                            let size_mb = size_bytes as f64 / 1024.0 / 1024.0;
                            
                            if size_mb > 10.0 {
                                apps.push(AppInventory {
                                    name,
                                    size_mb: (size_mb * 10.0).round() / 10.0,
                                    path: entry_path.to_string_lossy().to_string(),
                                    install_date: None,
                                });
                            }
                        }
                    }
                }
            }
        }
        
        // Sort by size descending and return top 50
        apps.sort_by(|a, b| b.size_mb.partial_cmp(&a.size_mb).unwrap_or(std::cmp::Ordering::Equal));
        apps.into_iter().take(50).collect()
    }
    
    // Implement the trait 'collect' method required by main.rs
    pub fn collect(db: &crate::db::DatabaseManager) -> anyhow::Result<()> {
        use crate::db::models::inventory::{InventoryModel, AppInventoryItem};
        
        // Use a temporary collector instance to scan inventory
        let collector = StorageCollector::new();
        let apps = collector.scan_inventory();
        
        let items: Vec<AppInventoryItem> = apps.into_iter().map(|app| {
            AppInventoryItem {
                name: app.name,
                size_mb: app.size_mb,
                path: app.path,
                install_date: app.install_date,
                last_scan: String::new(), // DB will fill this default
            }
        }).collect();

        // Save to DB
        // We need a mutable connection.
        let mut conn = db.get_connection();
        InventoryModel::save_batch(&mut conn, &items)?;
        
        Ok(())
    }
}

// ============= Windows Helper Functions =============

/// Initialize Windows app cache in background at startup
#[cfg(target_os = "windows")]
pub fn init_app_cache_background() {
    std::thread::spawn(|| {
        let apps = fetch_windows_apps_actual();
        let mut cache = WINDOWS_APP_CACHE.lock().unwrap();
        *cache = Some((Instant::now(), apps));
    });
}

#[cfg(not(target_os = "windows"))]
pub fn init_app_cache_background() {
    // No-op on non-Windows platforms
}

#[cfg(target_os = "windows")]
fn trigger_background_app_refresh() {
    std::thread::spawn(|| {
        let apps = fetch_windows_apps_actual();
        let mut cache = WINDOWS_APP_CACHE.lock().unwrap();
        *cache = Some((Instant::now(), apps));
    });
}

#[cfg(target_os = "windows")]
fn fetch_windows_apps_actual() -> Vec<AppInventory> {
    use std::process::Command;
    use std::fs;
    use std::path::Path;
    use std::collections::HashMap;
    
    let mut apps: Vec<AppInventory> = Vec::new();
    let mut seen_names: HashMap<String, usize> = HashMap::new();
    
    // Method 1: Query Windows Registry via PowerShell - OPTIMIZED
    // Key fix: Don't require EstimatedSize, calculate from InstallLocation if missing
    let ps_script = r#"
$apps = @()
$regPaths = @(
    'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*',
    'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*',
    'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*'
)
foreach ($path in $regPaths) {
    Get-ItemProperty $path -ErrorAction SilentlyContinue | 
    Where-Object { $_.DisplayName } |
    ForEach-Object {
        $size = 0
        if ($_.EstimatedSize) {
            $size = [math]::Round($_.EstimatedSize / 1024, 1)
        } elseif ($_.InstallLocation -and (Test-Path $_.InstallLocation -ErrorAction SilentlyContinue)) {
            try {
                $dirSize = (Get-ChildItem $_.InstallLocation -Recurse -File -ErrorAction SilentlyContinue | Measure-Object -Property Length -Sum -ErrorAction SilentlyContinue).Sum
                if ($dirSize) { $size = [math]::Round($dirSize / 1MB, 1) }
            } catch {}
        }
        $apps += [PSCustomObject]@{
            Name = $_.DisplayName
            Size = $size
            Path = if ($_.InstallLocation) { $_.InstallLocation } else { '' }
            Date = $_.InstallDate
        }
    }
}
$apps | Where-Object { $_.Size -gt 0 } | Sort-Object -Property Size -Descending | Select-Object -First 50 | ConvertTo-Json -Compress
"#;
    
    if let Ok(output) = Command::new("powershell")
        .args(&["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", ps_script])
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&stdout) {
                let items = if json.is_array() {
                    json.as_array().cloned().unwrap_or_default()
                } else if json.is_object() {
                    vec![json]
                } else {
                    vec![]
                };
                
                for item in items {
                    if let Some(name) = item.get("Name").and_then(|v| v.as_str()) {
                        let size = item.get("Size").and_then(|v| v.as_f64()).unwrap_or(0.0);
                        let path = item.get("Path").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let install_date = item.get("Date").and_then(|v| v.as_str()).map(|s| s.to_string());
                        
                        // Skip duplicates (keep the one with larger size)
                        let name_lower = name.to_lowercase();
                        if let Some(&idx) = seen_names.get(&name_lower) {
                            if apps[idx].size_mb < size {
                                apps[idx].size_mb = size;
                                if !path.is_empty() {
                                    apps[idx].path = path;
                                }
                            }
                            continue;
                        }
                        
                        seen_names.insert(name_lower, apps.len());
                        apps.push(AppInventory {
                            name: name.to_string(),
                            size_mb: size,
                            path,
                            install_date,
                        });
                    }
                }
            }
        }
    }
    
    // Method 2: Fallback - Fast scan of Program Files (only if PowerShell failed)
    if apps.is_empty() {
        let program_dirs = vec![
            "C:\\Program Files",
            "C:\\Program Files (x86)",
        ];
        
        // Lightweight size estimation (depth-limited)
        fn get_dir_size_fast(path: &Path, depth: u32) -> u64 {
            if depth > 2 { return 0; } // Very shallow for speed
            let mut size = 0;
            if let Ok(entries) = fs::read_dir(path) {
                for entry in entries.take(100).flatten() { // Limit entries per dir
                    if let Ok(meta) = entry.metadata() {
                        if meta.is_dir() {
                            size += get_dir_size_fast(&entry.path(), depth + 1);
                        } else {
                            size += meta.len();
                        }
                    }
                }
            }
            size
        }
        
        for dir in program_dirs {
            let path = Path::new(dir);
            if let Ok(entries) = fs::read_dir(path) {
                for entry in entries.flatten() {
                    let entry_path = entry.path();
                    if entry_path.is_dir() {
                        let name = entry_path.file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();
                        
                        // Skip Windows system folders
                        if name.starts_with("Windows") || name == "Common Files" || name.starts_with("Microsoft") {
                            continue;
                        }
                        
                        let size_bytes = get_dir_size_fast(&entry_path, 0);
                        let size_mb = size_bytes as f64 / 1024.0 / 1024.0;
                        
                        if size_mb > 10.0 {
                            apps.push(AppInventory {
                                name,
                                size_mb: (size_mb * 10.0).round() / 10.0,
                                path: entry_path.to_string_lossy().to_string(),
                                install_date: None,
                            });
                        }
                    }
                }
            }
        }
    }
    
    // Sort by size descending and return top 50
    apps.sort_by(|a, b| b.size_mb.partial_cmp(&a.size_mb).unwrap_or(std::cmp::Ordering::Equal));
    apps.into_iter().take(50).collect()
}
