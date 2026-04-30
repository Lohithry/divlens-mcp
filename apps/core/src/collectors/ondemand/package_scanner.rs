// ==================================================================================
// 🧠 DIVLENS HYBRID PACKAGE SCANNER - The "Hybrid Librarian"
// ==================================================================================
//
// This module implements a hybrid approach to package scanning:
// - Tier 1 (Fast): Direct file IO for modern languages (Python, Node, Rust)
// - Tier 2 (Robust): Terminal fallback for legacy languages (C, C++, Java)
//
// DESIGN PRINCIPLES:
// 1. Performance: Fast file reading for languages with metadata files
// 2. Robustness: Terminal commands for languages without standard metadata
// 3. Parallelism: All scanners run simultaneously using rayon
// 4. Graceful Degradation: Returns empty list if tool is missing (no crashes)
// 5. Extensible: Easy to add new languages or package managers
// 6. DEDUPLICATION: Smart merging of packages from multiple sources
//
// DEDUPLICATION STRATEGY:
// When a package exists in multiple sources (e.g., numpy in pip AND conda),
// we keep ONE entry with the "primary" source and track all sources.
// Priority: pip > conda > npm > cargo > pkg-config > java
//
// ENVIRONMENT HANDLING:
// This module relies on `utils::env_fixer` to set up the correct PATH before
// any scanning occurs. The env_fixer runs at app startup and ensures that
// tools like pip3, npm, cargo, and conda are discoverable via PATH.
//
// Because env_fixer handles PATH, we can use simple command names (pip3, npm)
// instead of hardcoding paths like /opt/anaconda3/bin/pip3.
// ==================================================================================

use std::process::Command;
use std::collections::HashMap;
use rayon::join;
use crate::models::devtools::PackageInfo;

/// Represents a deduplicated package with all its sources
/// This is used internally for deduplication logic
#[derive(Debug, Clone)]
pub struct DeduplicatedPackage {
    /// Package name (lowercase for comparison)
    pub name: String,
    /// Display name (original case)
    pub display_name: String,
    /// Version from primary source
    pub version: String,
    /// Primary source (pip, npm, cargo, etc.)
    pub primary_source: String,
    /// All sources where this package was found
    pub all_sources: Vec<String>,
    /// Install path (if available)
    pub install_path: Option<String>,
}

/// The Hybrid Librarian - Orchestrates all package scanning strategies
/// Combines fast file IO with robust terminal fallbacks
pub struct HybridLibrarian;

impl HybridLibrarian {
    /// Main entry point: Scans all libraries from all sources in parallel
    /// 
    /// # Returns
    /// Vector of PackageInfo from all sources (pip, conda, npm, cargo, pkg-config, java)
    /// 
    /// # Performance
    /// All scanners run in parallel using rayon::join for maximum speed
    /// Typical execution time: ~500ms (vs 2-3 seconds sequential)
    /// 
    /// # Prerequisites
    /// The `utils::env_fixer::rehydrate()` function MUST be called at app startup
    /// to ensure PATH includes user's shell environment (Anaconda, NVM, etc.)
    /// 
    /// # Example
    /// ```
    /// let packages = HybridLibrarian::scan_all_libraries();
    /// // Returns: [PackageInfo { name: "requests", version: "2.31.0", source: "pip" }, ...]
    /// ```
    pub fn scan_all_libraries() -> Vec<PackageInfo> {
        let mut results = Vec::new();

        // Parallel execution: All scanners run simultaneously
        // This uses rayon's work-stealing scheduler for optimal CPU utilization
        let ((mut pip, mut conda), ((mut npm, mut cargo), (mut c_libs, mut java_props))) = join(
            || join(
                || Self::scan_pip(),
                || Self::scan_conda()
            ),
            || join(
                || join(
                    || Self::scan_npm(),
                    || Self::scan_cargo()
                ),
                || join(
                    || Self::scan_c_packages(),
                    || Self::scan_java_info()
                )
            )
        );

        // Collect all results
        results.append(&mut pip);
        results.append(&mut conda);
        results.append(&mut npm);
        results.append(&mut cargo);
        results.append(&mut c_libs);
        results.append(&mut java_props);
        
        results
    }
    
    /// Scans all libraries and returns DEDUPLICATED results
    /// 
    /// # Deduplication Strategy
    /// When the same package exists in multiple sources (e.g., numpy in pip AND conda),
    /// we keep ONE entry with the "primary" source. Priority order:
    /// 1. pip (user's explicit installs)
    /// 2. npm (Node.js)
    /// 3. cargo (Rust)
    /// 4. conda (environment manager)
    /// 5. pkg-config (C/C++ system libs)
    /// 6. java (system properties)
    /// 
    /// # Why This Priority?
    /// - pip packages are usually what the user explicitly installed
    /// - conda packages often mirror pip (pypi channel)
    /// - We want to show the "authoritative" source
    /// 
    /// # Returns
    /// Tuple of (deduplicated_packages, source_breakdown)
    /// - deduplicated_packages: Unique packages with primary source
    /// - source_breakdown: HashMap of source -> count (including duplicates)
    pub fn scan_all_deduplicated() -> (Vec<PackageInfo>, HashMap<String, usize>, usize) {
        // First, get all raw packages
        let all_packages = Self::scan_all_libraries();
        let total_raw = all_packages.len();
        
        // Count packages by source (before deduplication)
        let mut source_breakdown: HashMap<String, usize> = HashMap::new();
        for pkg in &all_packages {
            // Normalize source name for counting
            let source_key = Self::normalize_source(&pkg.source);
            *source_breakdown.entry(source_key).or_insert(0) += 1;
        }
        
        // Deduplicate by package name (case-insensitive)
        // We use a HashMap where key = lowercase name, value = best package
        let mut dedup_map: HashMap<String, PackageInfo> = HashMap::new();
        
        for pkg in all_packages {
            let key = pkg.name.to_lowercase();
            
            // Check if we already have this package
            if let Some(existing) = dedup_map.get(&key) {
                // Compare priority: keep the one with higher priority source
                let existing_priority = Self::source_priority(&existing.source);
                let new_priority = Self::source_priority(&pkg.source);
                
                if new_priority > existing_priority {
                    // New package has higher priority, replace
                    dedup_map.insert(key, pkg);
                }
                // Otherwise, keep existing (it has higher or equal priority)
            } else {
                // First time seeing this package, add it
                dedup_map.insert(key, pkg);
            }
        }
        
        // Convert HashMap values to Vec
        let deduplicated: Vec<PackageInfo> = dedup_map.into_values().collect();
        
        eprintln!(
            "[PackageScanner] Deduplicated: {} raw -> {} unique packages",
            total_raw, deduplicated.len()
        );
        
        (deduplicated, source_breakdown, total_raw)
    }
    
    /// Returns priority for a source (higher = more authoritative)
    /// 
    /// # Priority Logic
    /// - pip (10): User's explicit Python installs, most authoritative
    /// - npm (9): Node.js packages
    /// - cargo (8): Rust packages
    /// - conda main channels (5): Environment manager packages
    /// - conda pypi (3): Conda's view of pip packages (duplicate)
    /// - pkg-config (2): System C/C++ libraries
    /// - java (1): Java system properties (not really packages)
    fn source_priority(source: &str) -> u8 {
        let source_lower = source.to_lowercase();
        
        if source_lower == "pip" {
            10
        } else if source_lower == "npm" {
            9
        } else if source_lower == "cargo" {
            8
        } else if source_lower.starts_with("conda") {
            // Conda has sub-channels: pkgs/main, pypi, conda-forge, etc.
            if source_lower.contains("pypi") {
                // conda (pypi) = pip packages seen by conda, lower priority
                3
            } else {
                // conda (pkgs/main), conda (conda-forge), etc.
                5
            }
        } else if source_lower == "pkg-config" {
            2
        } else if source_lower == "java" || source_lower == "java-prop" {
            1
        } else {
            0
        }
    }
    
    /// Normalizes source name for consistent counting
    /// 
    /// # Examples
    /// - "conda (pkgs/main)" -> "conda"
    /// - "conda (pypi)" -> "conda-pypi"
    /// - "pip" -> "pip"
    fn normalize_source(source: &str) -> String {
        let source_lower = source.to_lowercase();
        
        if source_lower.starts_with("conda") {
            if source_lower.contains("pypi") {
                "conda-pypi".to_string()
            } else {
                "conda".to_string()
            }
        } else {
            source_lower
        }
    }

    // ==================================================================================
    // TIER 1: FAST FILE IO (Modern Languages with Metadata Files)
    // ==================================================================================
    // These use direct file reading (15-50ms) instead of terminal commands (200-500ms)
    // ==================================================================================

    /// Scans Python packages via pip
    /// 
    /// # Strategy
    /// Uses `pip3 list --format=json` to get all installed packages.
    /// Falls back to `pip` if `pip3` is not available.
    /// 
    /// # Environment
    /// Relies on PATH being set correctly by `env_fixer::rehydrate()`.
    /// This ensures we find the user's preferred pip (Anaconda, Homebrew, system).
    /// 
    /// # Performance
    /// Terminal command: ~200ms
    /// 
    /// # Returns
    /// Vector of PackageInfo from pip
    fn scan_pip() -> Vec<PackageInfo> {
        // Try pip3 first (preferred on most systems)
        // Thanks to env_fixer, PATH already includes /opt/anaconda3/bin, etc.
        if let Some(packages) = Self::try_pip_command("pip3") {
            if !packages.is_empty() {
                return packages;
            }
        }
        
        // Fallback to pip (some systems only have pip)
        if let Some(packages) = Self::try_pip_command("pip") {
            return packages;
        }
        
        vec![]
    }
    
    /// Attempts to run a pip command and parse its output
    /// 
    /// # Arguments
    /// * `pip_cmd` - The pip command to run (e.g., "pip3", "pip")
    /// 
    /// # Returns
    /// Some(Vec<PackageInfo>) if successful, None if command failed
    fn try_pip_command(pip_cmd: &str) -> Option<Vec<PackageInfo>> {
        let output = Command::new(pip_cmd)
            .args(&["list", "--format=json"])
            .output()
            .ok()?;
        
        if !output.status.success() {
            return None;
        }
        
        let json: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).ok()?;
        
        let packages: Vec<PackageInfo> = json.into_iter()
                        .filter_map(|item| {
                            let name = item["name"].as_str()?.to_string();
                            let version = item["version"].as_str()?.to_string();
                            Some(PackageInfo {
                                name,
                                version,
                                source: "pip".to_string(),
                                install_path: None,
                            })
                        })
                        .collect();
        
        Some(packages)
        }
        
    /// Scans Conda packages (Anaconda/Miniconda)
    /// 
    /// # Strategy
    /// Uses `conda list --json` to get all packages in current/base environment.
    /// Falls back to `mamba` if conda is not available (faster alternative).
    /// 
    /// # Environment
    /// Relies on PATH being set correctly by `env_fixer::rehydrate()`.
    /// 
    /// # Performance
    /// Terminal command: ~200-400ms
    /// 
    /// # Returns
    /// Vector of PackageInfo from conda packages
    fn scan_conda() -> Vec<PackageInfo> {
        // Try conda first
        if let Some(packages) = Self::try_conda_command("conda") {
            if !packages.is_empty() {
                return packages;
            }
        }
        
        // Fallback to mamba (faster conda alternative)
        if let Some(packages) = Self::try_conda_command("mamba") {
            return packages;
        }
        
        vec![]
    }
    
    /// Attempts to run a conda/mamba command and parse its output
    fn try_conda_command(cmd: &str) -> Option<Vec<PackageInfo>> {
        let output = Command::new(cmd)
            .args(&["list", "--json"])
            .output()
            .ok()?;
        
        if !output.status.success() {
            return None;
        }
        
        let json: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).ok()?;
        
        let packages: Vec<PackageInfo> = json.into_iter()
                        .filter_map(|item| {
                            let name = item["name"].as_str()?.to_string();
                            let version = item["version"].as_str()?.to_string();
                let channel = item["channel"].as_str().unwrap_or("conda");
                
                            Some(PackageInfo {
                                name,
                                version,
                    source: format!("conda ({})", channel),
                                install_path: None,
                            })
                        })
                        .collect();
        
        Some(packages)
    }

    /// Scans Node.js packages via npm
    /// 
    /// # Strategy
    /// Uses `npm list -g --depth=0 --json` to get globally installed packages.
    /// 
    /// # Environment
    /// Relies on PATH being set correctly by `env_fixer::rehydrate()`.
    /// This ensures we find the user's NVM-managed npm if applicable.
    /// 
    /// # Performance
    /// Terminal command: ~300ms (npm is slow, but necessary for global packages)
    /// 
    /// # Returns
    /// Vector of PackageInfo from npm global packages
    fn scan_npm() -> Vec<PackageInfo> {
        let output = match Command::new("npm")
            .args(&["list", "-g", "--depth=0", "--json"])
            .output()
        {
            Ok(o) => o,
            Err(_) => return vec![], // npm not available
        };
        
        if !output.status.success() {
            return vec![];
        }
        
        let root: serde_json::Value = match serde_json::from_slice(&output.stdout) {
            Ok(v) => v,
            Err(_) => return vec![],
        };
        
        let deps = match root["dependencies"].as_object() {
            Some(d) => d,
            None => return vec![],
        };
        
        deps.iter()
                            .filter_map(|(name, data)| {
                                let version = data["version"].as_str()?.to_string();
                                Some(PackageInfo {
                                    name: name.clone(),
                                    version,
                                    source: "npm".to_string(),
                                    install_path: None,
                                })
                            })
            .collect()
    }

    /// Scans Rust packages via cargo
    /// 
    /// # Strategy
    /// Uses `cargo install --list` to get installed binaries.
    /// 
    /// # Environment
    /// Relies on PATH being set correctly by `env_fixer::rehydrate()`.
    /// This ensures ~/.cargo/bin is in PATH.
    /// 
    /// # Performance
    /// Terminal command: ~200ms
    /// 
    /// # Returns
    /// Vector of PackageInfo from cargo-installed packages
    fn scan_cargo() -> Vec<PackageInfo> {
        let output = match Command::new("cargo")
            .args(&["install", "--list"])
            .output()
        {
            Ok(o) => o,
            Err(_) => return vec![], // cargo not available
        };
        
        if !output.status.success() {
            return vec![];
        }
        
                let output_str = String::from_utf8_lossy(&output.stdout);
        
        output_str
                    .lines()
                    .filter_map(|line| {
                        // Format: "package_name v1.2.3:" or "package_name v1.2.3: /path/to/binary"
                // We only care about lines with version info (not the binary paths)
                if line.ends_with(":") || (line.contains(" v") && !line.starts_with("    ")) {
                            let parts: Vec<&str> = line.split_whitespace().collect();
                            if parts.len() >= 2 {
                                let name = parts[0].to_string();
                                // Extract version (remove 'v' prefix and ':' suffix)
                                let version = parts.get(1)
                                    .map(|v| v.trim_start_matches('v').trim_end_matches(':').to_string())
                                    .unwrap_or_else(|| "unknown".to_string());
                                
                                return Some(PackageInfo {
                                    name,
                                    version,
                                    source: "cargo".to_string(),
                                    install_path: None,
                                });
                            }
                        }
                        None
                    })
            .collect()
    }

    // ==================================================================================
    // TIER 2: TERMINAL FALLBACK (Legacy Languages without Standard Metadata)
    // ==================================================================================
    // These use terminal commands because there's no standard metadata file format
    // ==================================================================================

    /// Scans C/C++ packages via pkg-config
    /// 
    /// # Strategy
    /// Uses `pkg-config --list-all` to discover installed libraries.
    /// pkg-config is the standard tool for C/C++ library discovery on Unix systems.
    /// 
    /// # Performance
    /// Terminal command: ~100-200ms
    /// 
    /// # Platform Support
    /// - Unix (macOS/Linux): Full support via pkg-config
    /// - Windows: Limited (requires MSYS2/MinGW with pkg-config)
    /// 
    /// # Returns
    /// Vector of PackageInfo from pkg-config
    fn scan_c_packages() -> Vec<PackageInfo> {
        let output = match Command::new("pkg-config").arg("--list-all").output() {
            Ok(o) => o,
            Err(_) => return vec![], // pkg-config not available
        };

        if !output.status.success() {
            return vec![];
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        
        stdout
            .lines()
            .filter_map(|line| {
                let parts: Vec<&str> = line.split_whitespace().collect();
                
                if parts.is_empty() {
                    return None;
                }
                
                let name = parts[0].to_string();
                
                // Try to get version via pkg-config --modversion
                let version = Self::get_pkg_config_version(&name).unwrap_or_else(|| {
                    // Fallback: Try to extract from description
                    if parts.len() >= 3 {
                        parts.iter()
                            .skip(1)
                            .find(|p| p.chars().next().map_or(false, |c| c.is_numeric()))
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| "System".to_string())
                    } else {
                        "System".to_string()
                    }
                });
                
                Some(PackageInfo {
                    name,
                    version,
                    source: "pkg-config".to_string(),
                    install_path: None,
                })
            })
            .collect()
    }

    /// Gets version for a pkg-config package
    fn get_pkg_config_version(package_name: &str) -> Option<String> {
        let output = Command::new("pkg-config")
            .args(&["--modversion", package_name])
            .output()
            .ok()?;
        
            if output.status.success() {
                let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !version.is_empty() {
                    return Some(version);
            }
        }
        None
    }

    /// Scans Java environment information
    /// 
    /// # Strategy
    /// Uses `java -XshowSettings:properties -version` to get system properties.
    /// Only extracts java.version and java.home (not all properties).
    /// 
    /// # Performance
    /// Terminal command: ~300-500ms (Java startup is slow)
    /// 
    /// # Returns
    /// Vector of PackageInfo with core Java properties
    /// 
    /// # Note
    /// This is informational - Java libraries are typically managed per-project
    /// (Maven, Gradle) rather than globally installed
    fn scan_java_info() -> Vec<PackageInfo> {
        // java -XshowSettings:properties -version shows system properties
        // Note: Output goes to STDERR, not STDOUT (Java quirk)
        let output = match Command::new("java")
            .args(&["-XshowSettings:properties", "-version"])
            .output()
        {
            Ok(o) => o,
            Err(_) => return vec![], // Java not available
        };

        if !output.status.success() {
            return vec![];
        }

        // Java outputs to stderr (not stdout) - this is a Java quirk
        let stderr = String::from_utf8_lossy(&output.stderr);
        let mut results = Vec::new();

        // Parse system properties
        // Format: "    property.name = value"
        for line in stderr.lines() {
            let line = line.trim();
            
            if line.contains(" = ") {
                let parts: Vec<&str> = line.splitn(2, " = ").collect();
                if parts.len() == 2 {
                    let name = parts[0].trim().to_string();
                    let value = parts[1].trim().to_string();
                    
                    // Only include core Java info (version and home)
                    // Skip vendor URLs, library paths, etc. - not useful as "packages"
                    if name == "java.version" || name == "java.home" {
                        results.push(PackageInfo {
                            name,
                            version: value,
                            source: "java".to_string(),
                            install_path: None,
                        });
                    }
                }
            }
        }
        
        results
    }
}
