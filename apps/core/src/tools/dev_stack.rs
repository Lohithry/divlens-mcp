// ==================================================================================
// 🧠 DIVLENS DEV STACK TOOL - Hybrid Package & Runtime Detection
// ==================================================================================
// This tool provides comprehensive developer tools information:
// - Installed language runtimes (Python, Node, Java, C++, etc.)
// - Path conflicts (multiple versions shadowing each other)
// - Installed packages from all sources (pip, npm, cargo, pkg-config, java)
//
// DESIGN PRINCIPLES:
// 1. Hybrid Approach: Fast file IO + robust terminal fallback
// 2. Comprehensive: Covers modern and legacy languages
// 3. AI-Friendly: Pre-analyzed data with clear structure
// 4. Performance: Parallel execution for speed
// 5. DEDUPLICATION: Smart merging of packages from multiple sources
//
// DEDUPLICATION:
// When a package exists in multiple sources (e.g., numpy in pip AND conda),
// we show it ONCE with the primary source, but track all sources for reference.
// This prevents the confusing "1223 packages" when there are only ~800 unique.
// ==================================================================================

use async_trait::async_trait;
use serde_json::Value;
use crate::db::DatabaseManager;
use crate::db::models::devtools::DevToolsModel;
use super::Tool;
use std::sync::Arc;
use crate::modules::metrics::SystemCollector;
use crate::modules::datahub::DataHub;
use crate::modules::memory::MemoryManager;
use crate::collectors::persistent::env_doctor;
use crate::collectors::ondemand::package_scanner::HybridLibrarian;
use crate::models::devtools::DevToolsSnapshot;

pub struct GetDevStackTool {
    db: Arc<DatabaseManager>,
}

impl GetDevStackTool {
    pub fn new(db: Arc<DatabaseManager>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl Tool for GetDevStackTool {
    fn name(&self) -> &str {
        "get_dev_stack"
    }

    fn description(&self) -> &str {
        "Retrieves comprehensive developer tools information including installed language runtimes, \
        path conflicts, and packages from all sources (pip, npm, cargo, pkg-config, java). \
        Uses hybrid approach: fast file IO for modern languages, terminal fallback for legacy languages. \
        Returns:\n\
        - runtimes: List of detected language runtimes with versions and paths\n\
        - path_conflicts: List of path conflicts (multiple versions of same binary)\n\
        - packages: List of installed packages from all package managers\n\
        Use this to answer questions like 'What Python version do I have?' or 'Do I have openssl installed?'"
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "include_packages": {
                    "type": "boolean",
                    "description": "Whether to include package scanning (default: true). Set to false for faster response.",
                    "default": true
                },
                "search": {
                    "type": "string",
                    "description": "Optional search query to filter packages by name",
                    "default": ""
                },
                "force_refresh": {
                    "type": "boolean",
                    "description": "Force refresh packages from disk (default: false). If false, uses cached database data.",
                    "default": false
                }
            }
        })
    }

    async fn call(
        &self,
        args: &Value,
        _col: &mut SystemCollector,
        _dh: &DataHub,
        _mem: &MemoryManager,
    ) -> anyhow::Result<Value> {
        // Parse optional arguments
        let include_packages = args
            .get("include_packages")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        
        let search_query = args
            .get("search")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_lowercase();
        
        let force_refresh = args
            .get("force_refresh")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // 1. Handle Runtimes & Conflicts with Persistent Storage
        // CRITICAL: Runtimes and conflicts are now cached in database
        // Only scan if database is empty or force refresh is requested
        let conn = self.db.get_connection();
        let has_runtimes = DevToolsModel::has_runtimes(&conn)?;
        let has_conflicts = DevToolsModel::has_conflicts(&conn)?;
        let needs_scan_env = force_refresh || !has_runtimes || !has_conflicts;
        drop(conn);
        
        let (runtimes, conflicts) = if needs_scan_env {
            // Scan environment (runtimes and conflicts)
            if force_refresh {
                eprintln!("[DevTools] Force refresh requested, scanning runtimes/conflicts...");
            } else {
                eprintln!("[DevTools] Database empty, scanning runtimes/conflicts...");
            }
            
            let (scanned_runtimes, scanned_conflicts) = env_doctor::scan_dev_environment();
            
            // Save to database
            let mut conn = self.db.get_connection();
            DevToolsModel::save_runtimes(&mut conn, &scanned_runtimes)?;
            DevToolsModel::save_conflicts(&mut conn, &scanned_conflicts)?;
            
            (scanned_runtimes, scanned_conflicts)
        } else {
            // Load from database (fast, no scanning)
            eprintln!("[DevTools] Loading runtimes/conflicts from database (cached)...");
            let conn = self.db.get_connection();
            let db_runtimes = DevToolsModel::get_all_runtimes(&conn)?;
            let db_conflicts = DevToolsModel::get_all_conflicts(&conn)?;
            (db_runtimes, db_conflicts)
        };

        // 2. Handle Packages with Persistent Storage
        // CRITICAL: Packages are stored in database and only refreshed when:
        // - force_refresh is true (manual refresh button)
        // - Database is empty (first run or after clear)
        //
        // DEDUPLICATION: We now use scan_all_deduplicated() which:
        // - Returns unique packages (by name, case-insensitive)
        // - Tracks source breakdown (pip: X, conda: Y, etc.)
        // - Returns total raw count for reference
        let (packages, source_breakdown, total_raw) = if include_packages {
            // First, check if we need to scan (database empty or force refresh)
            let conn = self.db.get_connection();
            let has_packages = DevToolsModel::has_packages(&conn)?;
            let needs_scan = force_refresh || !has_packages;
            drop(conn); // Release the read lock
            
            // Log for debugging (can be removed in production)
            if needs_scan {
                if force_refresh {
                    eprintln!("[DevTools] Force refresh requested, scanning packages...");
                } else {
                    eprintln!("[DevTools] Database empty, scanning packages...");
                }
            } else {
                eprintln!("[DevTools] Loading packages from database (cached)...");
            }
            
            if needs_scan {
                // Run hybrid scanner with DEDUPLICATION (parallel execution)
                // This returns:
                // - deduplicated: Vec<PackageInfo> with unique packages
                // - breakdown: HashMap<String, usize> with counts per source
                // - total_raw: Total packages before deduplication
                let (deduplicated, breakdown, raw_count) = HybridLibrarian::scan_all_deduplicated();
                
                // Save DEDUPLICATED packages to database for future use
                // This means cached data is also deduplicated
                let mut conn = self.db.get_connection();
                DevToolsModel::save_packages(&mut conn, &deduplicated)?;
                
                // Filter by search query if provided
                let filtered = if !search_query.is_empty() {
                    deduplicated.into_iter()
                        .filter(|p| p.name.to_lowercase().contains(&search_query))
                        .collect()
                } else {
                    deduplicated
                };
                
                (filtered, breakdown, raw_count)
            } else {
                // Load from database (fast, no scanning)
                // Database already has deduplicated packages
                let conn = self.db.get_connection();
                let db_packages = DevToolsModel::get_all_packages(&conn)?;
                
                // Reconstruct source breakdown from loaded packages
                let mut breakdown = std::collections::HashMap::new();
                for pkg in &db_packages {
                    let source_key = pkg.source.to_lowercase();
                    *breakdown.entry(source_key).or_insert(0) += 1;
                }
                let raw_count = db_packages.len(); // Cached data is already deduplicated
                
                // Filter by search query if provided
                let filtered = if !search_query.is_empty() {
                    db_packages.into_iter()
                        .filter(|p| p.name.to_lowercase().contains(&search_query))
                        .collect()
                } else {
                    db_packages
                };
                
                (filtered, breakdown, raw_count)
            }
        } else {
            (Vec::new(), std::collections::HashMap::new(), 0)
        };

        // 3. Build comprehensive snapshot (for future use if needed)
        let _snapshot = DevToolsSnapshot {
            runtimes: runtimes.clone(),
            path_conflicts: conflicts.clone(),
            packages: packages.clone(),
        };

        // 4. Return JSON optimized for frontend and AI
        // INCLUDES:
        // - unique_packages_count: Deduplicated count (what user actually has)
        // - total_raw_count: Before deduplication (for reference)
        // - source_breakdown: Detailed counts per source (including duplicates)
        // - package_sources: Simplified counts from deduplicated data
            Ok(serde_json::json!({
            "status": "success",
            "summary": {
                "runtimes_count": runtimes.len(),
                "conflicts_count": conflicts.len(),
                // UNIQUE package count (deduplicated)
                "packages_count": packages.len(),
                // Total raw count before deduplication (for transparency)
                "total_raw_packages": total_raw,
                // Detailed breakdown by source (from raw scan, includes duplicates)
                "source_breakdown": source_breakdown,
                // Simplified counts from deduplicated packages
                "package_sources": {
                    "pip": packages.iter().filter(|p| p.source.to_lowercase() == "pip").count(),
                    "npm": packages.iter().filter(|p| p.source.to_lowercase() == "npm").count(),
                    "cargo": packages.iter().filter(|p| p.source.to_lowercase() == "cargo").count(),
                    "pkg_config": packages.iter().filter(|p| p.source.to_lowercase() == "pkg-config").count(),
                    "java": packages.iter().filter(|p| p.source.to_lowercase() == "java" || p.source.to_lowercase() == "java-prop").count(),
                    // Conda packages (from main channels, not pypi duplicates)
                    "conda": packages.iter().filter(|p| p.source.to_lowercase().starts_with("conda") && !p.source.to_lowercase().contains("pypi")).count(),
                }
            },
            "runtimes": runtimes.iter().map(|r| serde_json::json!({
                "name": r.name,
                "version": r.version,
                "path": r.active_path,
                "manager": r.detected_manager
            })).collect::<Vec<_>>(),
            "path_conflicts": conflicts.iter().map(|c| serde_json::json!({
                "binary": c.binary,
                "active": c.active_source,
                "shadowed": c.shadowed_sources,
                "severity": c.severity
            })).collect::<Vec<_>>(),
            "packages": packages.iter().map(|p| serde_json::json!({
                "name": p.name,
                "version": p.version,
                "source": p.source,
                "path": p.install_path
            })).collect::<Vec<_>>()
        }))
    }
}
