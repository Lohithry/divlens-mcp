use crate::db::DatabaseManager;
use std::process::Stdio;
use tokio::process::Command; // Async Command
use serde_json::Value;

pub struct PackageCollector;

impl PackageCollector {
    // This is ASYNC for parallelism
    pub async fn scan_parallel(db: &DatabaseManager) -> anyhow::Result<()> {
        // Scope the lock so it is dropped before async calls
        {
            let conn_guard = db.get_connection();
            let conn = &*conn_guard;
            conn.execute("DELETE FROM dev_packages", [])?; // Clear old data
        } // conn_guard dropped here
        
        // 1. Launch all scans at the exact same time
        println!("   ⚡ [Packages] Launching Parallel Scans...");
        
        let pip_task = Self::scan_pip();
        let npm_task = Self::scan_npm();
        let cargo_task = Self::scan_cargo();

        // 2. Wait for all of them to finish
        let (pip_res, npm_res, cargo_res) = tokio::join!(pip_task, npm_task, cargo_task);

        // 3. Batch Insert (Single Transaction for Speed)
        // Now re-acquire the lock
        let conn_guard = db.get_connection();
        let conn = &*conn_guard;
        
        let mut stmt = conn.prepare("INSERT INTO dev_packages (manager, name, version) VALUES (?1, ?2, ?3)")?;
        
        // Insert Pip
        if let Ok(pkgs) = pip_res {
            for p in pkgs { stmt.execute(("pip", p.0, p.1))?; }
        }
        
        // Insert NPM
        if let Ok(pkgs) = npm_res {
             for p in pkgs { stmt.execute(("npm", p.0, p.1))?; }
        }
        // Insert Cargo
        if let Ok(pkgs) = cargo_res {
             for p in pkgs { stmt.execute(("cargo", p.0, p.1))?; }
        }

        println!("   -> Packages Updated.");
        Ok(())
    }

    // --- Helper Scanners ---
    async fn scan_pip() -> anyhow::Result<Vec<(String, String)>> {
        // Try both pip and pip3
        let pip_cmd = if Command::new("pip3").arg("--version").output().await.is_ok() {
             "pip3"
        } else {
             "pip"
        };

        let output = Command::new(pip_cmd)
            .args(["list", "--format=json"])
            .output()
            .await?; // Async wait
            
        if !output.status.success() { return Ok(vec![]); }
        
        let json: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap_or(vec![]);
        let mut results = Vec::new();

        for item in json {
            let name = item["name"].as_str().unwrap_or("").to_string();
            let ver = item["version"].as_str().unwrap_or("").to_string();
            results.push((name, ver));
        }
        Ok(results)
    }

    async fn scan_npm() -> anyhow::Result<Vec<(String, String)>> {
        // --depth=0 is crucial for speed. We don't want the whole dependency tree.
        let output = Command::new("npm")
            .args(["list", "-g", "--depth=0", "--json"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null()) // Ignore stderr noise
            .output()
            .await?;

        if !output.status.success() { return Ok(vec![]); }

        let root: Value = serde_json::from_slice(&output.stdout).unwrap_or(serde_json::json!({}));
        let mut results = Vec::new();
        
        if let Some(deps) = root["dependencies"].as_object() {
            for (name, data) in deps {
                let ver = data["version"].as_str().unwrap_or("").to_string();
                results.push((name.clone(), ver));
            }
        }
        Ok(results)
    }

    async fn scan_cargo() -> anyhow::Result<Vec<(String, String)>> {
        // Cargo doesn't have a "list installed" command natively that is fast.
        // We scan ~/.cargo/bin via 'cargo install --list'
        let output = Command::new("cargo")
            .args(["install", "--list"])
            .output()
            .await?;

        if !output.status.success() { return Ok(vec![]); }
        
        let output_str = String::from_utf8_lossy(&output.stdout);
        let mut results = Vec::new();
        
        // Output format: "crate v1.2.3:"
        for line in output_str.lines() {
            if line.ends_with(":") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    let name = parts[0].to_string();
                    let ver = parts[1].trim_start_matches('v').trim_end_matches(':').to_string();
                    results.push((name, ver));
                }
            }
        }
        Ok(results)
    }
}

