//! # `divlens status` — Installation Status
//!
//! Displays a comprehensive overview of the DivLens installation:
//! - Binary location and version
//! - Data directory
//! - Connected AI clients with config status
//! - Service registration status

use super::ui;
use super::paths;

/// Run the `status` subcommand.
pub fn run() {
    ui::print_banner("🔍", "DivLens System Intelligence", None);

    // ─── Binary ──────────────────────────────────────────────────────────────
    ui::print_step("Installation");

    match paths::find_installed_binary() {
        Some(path) => {
            ui::print_ok(&format!("Binary: {}", path.display()));

            // Try to get version
            match std::process::Command::new(&path).arg("--version").output() {
                Ok(output) => {
                    let version = String::from_utf8_lossy(&output.stdout);
                    let version = version.trim();
                    if !version.is_empty() {
                        ui::print_ok(&format!("Version: {}", version));
                    }
                }
                Err(_) => {
                    ui::print_warn("Could not determine version");
                }
            }
        }
        None => {
            ui::print_fail("Binary not found — DivLens may not be installed");
            ui::print_info("Install: curl -fsSL https://raw.githubusercontent.com/Lohithry/divlens-mcp/main/install.sh | bash");
            return;
        }
    }

    // ─── Data Directory ──────────────────────────────────────────────────────
    let data_dir = paths::get_data_directory();
    if data_dir.exists() {
        let db_path = data_dir.join("divlens.db");
        if db_path.exists() {
            let size = std::fs::metadata(&db_path)
                .map(|m| format_bytes(m.len()))
                .unwrap_or_else(|_| "unknown".to_string());
            ui::print_ok(&format!("Database: {} ({})", db_path.display(), size));
        } else {
            ui::print_skip(&format!("Database: {} (not yet created)", db_path.display()));
        }
    } else {
        ui::print_skip(&format!("Data dir: {} (not yet created)", data_dir.display()));
    }

    // ─── AI Client Configs ───────────────────────────────────────────────────
    ui::print_step("Connected Clients");

    let clients = paths::get_ai_client_configs();
    let mut connected = 0;
    let mut total = 0;

    for client in &clients {
        total += 1;
        let result = paths::config_has_divlens(&client.config_path);
        match result {
            paths::ConfigCheckResult::Configured => {
                connected += 1;
                ui::print_ok(&format!(
                    "{:<24} {}",
                    client.name,
                    shorten_path(&client.config_path)
                ));
            }
            paths::ConfigCheckResult::ConfiguredWithBom => {
                connected += 1;
                ui::print_warn(&format!(
                    "{:<24} configured (⚠ BOM detected — may cause issues)",
                    client.name
                ));
            }
            paths::ConfigCheckResult::ValidJsonNoDivlens => {
                ui::print_skip(&format!(
                    "{:<24} installed but divlens not configured",
                    client.name
                ));
            }
            paths::ConfigCheckResult::InvalidJson => {
                ui::print_fail(&format!(
                    "{:<24} config has invalid JSON",
                    client.name
                ));
            }
            paths::ConfigCheckResult::NotFound => {
                if !client.is_alternate {
                    ui::print_skip(&format!("{:<24} not installed", client.name));
                }
            }
            paths::ConfigCheckResult::Empty => {
                ui::print_skip(&format!("{:<24} config is empty", client.name));
            }
            paths::ConfigCheckResult::Unreadable => {
                ui::print_fail(&format!("{:<24} config not readable", client.name));
            }
        }
    }

    // ─── Service Status ──────────────────────────────────────────────────────
    ui::print_step("Service");

    if let Some(service_path) = paths::get_service_path() {
        if service_path.exists() {
            ui::print_ok("Auto-start on login enabled");
        } else {
            ui::print_skip("Auto-start not configured (optional)");
        }
    } else {
        // Windows: check Task Scheduler
        #[cfg(target_os = "windows")]
        {
            let output = std::process::Command::new("schtasks")
                .args(["/Query", "/TN", "DivLens MCP"])
                .output();
            match output {
                Ok(o) if o.status.success() => {
                    ui::print_ok("Auto-start on login enabled (Task Scheduler)");
                }
                _ => {
                    ui::print_skip("Auto-start not configured (optional)");
                }
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            ui::print_skip("Auto-start not available on this platform");
        }
    }

    // ─── Summary ─────────────────────────────────────────────────────────────
    eprintln!();
    ui::print_line();
    ui::print_info(&format!(
        "{}/{} AI clients connected",
        connected, total
    ));
    eprintln!();
}

/// Format bytes into human-readable size.
fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

/// Shorten a path for display by replacing home dir with ~.
fn shorten_path(path: &std::path::Path) -> String {
    if let Some(home) = dirs::home_dir() {
        if let Ok(stripped) = path.strip_prefix(&home) {
            return format!("~/{}", stripped.display());
        }
    }
    path.display().to_string()
}
