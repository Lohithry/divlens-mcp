//! # `divlens uninstall` — Clean Removal
//!
//! Removes all DivLens artifacts: binary, database, config entries, service.

use super::ui;
use super::paths;

/// Run the `uninstall` subcommand.
pub fn run(force: bool) {
    ui::print_banner("🗑️", "DivLens Uninstaller", None);

    // ─── Enumerate what will be removed ──────────────────────────────────────
    let binary = paths::find_installed_binary();
    let data_dir = paths::get_data_directory();
    let service_path = paths::get_service_path();
    let clients = paths::get_ai_client_configs();

    ui::print_step("This will remove:");
    if let Some(ref b) = binary {
        ui::print_info(&format!("Binary:    {}", b.display()));
    }
    if data_dir.exists() {
        ui::print_info(&format!("Database:  {}", data_dir.display()));
    }
    let configured: Vec<_> = clients
        .iter()
        .filter(|c| {
            matches!(
                paths::config_has_divlens(&c.config_path),
                paths::ConfigCheckResult::Configured | paths::ConfigCheckResult::ConfiguredWithBom
            )
        })
        .collect();
    for c in &configured {
        ui::print_info(&format!("Config:    {} entry from {}", "divlens", c.name));
    }
    if let Some(ref sp) = service_path {
        if sp.exists() {
            ui::print_info(&format!("Service:   {}", sp.display()));
        }
    }

    // ─── Confirm ─────────────────────────────────────────────────────────────
    if !force {
        eprintln!();
        if !ui::confirm("Proceed?") {
            eprintln!();
            ui::print_info("Uninstall cancelled.");
            return;
        }
    }

    eprintln!();
    let mut errors = 0;

    // ─── Remove binary ───────────────────────────────────────────────────────
    if let Some(ref b) = binary {
        match std::fs::remove_file(b) {
            Ok(_) => ui::print_ok("Removed binary"),
            Err(e) => {
                ui::print_fail(&format!("Could not remove binary: {}", e));
                #[cfg(unix)]
                ui::print_info(&format!("Try: sudo rm {}", b.display()));
                errors += 1;
            }
        }
    }

    // ─── Remove data directory ───────────────────────────────────────────────
    if data_dir.exists() {
        match std::fs::remove_dir_all(&data_dir) {
            Ok(_) => ui::print_ok("Removed database directory"),
            Err(e) => {
                ui::print_fail(&format!("Could not remove data dir: {}", e));
                errors += 1;
            }
        }
    }

    // ─── Clean AI client configs ─────────────────────────────────────────────
    for client in &configured {
        match remove_divlens_from_config(&client.config_path) {
            Ok(_) => ui::print_ok(&format!("Cleaned {} config", client.name)),
            Err(e) => {
                ui::print_fail(&format!("Could not clean {} config: {}", client.name, e));
                errors += 1;
            }
        }
    }

    // ─── Remove service ──────────────────────────────────────────────────────
    if let Some(ref sp) = service_path {
        if sp.exists() {
            // Unload before removing (macOS launchd)
            #[cfg(target_os = "macos")]
            {
                let _ = std::process::Command::new("launchctl")
                    .args(["unload", &sp.display().to_string()])
                    .output();
            }
            #[cfg(target_os = "linux")]
            {
                let _ = std::process::Command::new("systemctl")
                    .args(["--user", "stop", "divlens-mcp"])
                    .output();
                let _ = std::process::Command::new("systemctl")
                    .args(["--user", "disable", "divlens-mcp"])
                    .output();
            }
            match std::fs::remove_file(sp) {
                Ok(_) => ui::print_ok("Removed service"),
                Err(e) => {
                    ui::print_fail(&format!("Could not remove service: {}", e));
                    errors += 1;
                }
            }
        }
    }
    #[cfg(target_os = "windows")]
    {
        let result = std::process::Command::new("schtasks")
            .args(["/Delete", "/TN", "DivLens MCP", "/F"])
            .output();
        match result {
            Ok(o) if o.status.success() => ui::print_ok("Removed scheduled task"),
            _ => {} // Task didn't exist — fine
        }
    }

    // ─── Summary ─────────────────────────────────────────────────────────────
    eprintln!();
    if errors == 0 {
        ui::print_result_box(ui::Colors::GREEN, "DivLens has been completely removed. 👋");
    } else {
        ui::print_result_box(
            ui::Colors::YELLOW,
            &format!("Uninstall completed with {} error(s)", errors),
        );
    }
}

/// Remove the `divlens` entry from an AI client's config JSON without
/// breaking other MCP servers in the same file.
fn remove_divlens_from_config(path: &std::path::Path) -> Result<(), String> {
    let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Ok(());
    }

    // Strip BOM if present
    let parse_content = if content.as_bytes().starts_with(&[0xEF, 0xBB, 0xBF]) {
        &content[3..]
    } else {
        &content
    };

    let mut val: serde_json::Value =
        serde_json::from_str(parse_content).map_err(|e| e.to_string())?;

    if let Some(servers) = val.get_mut("mcpServers").and_then(|s| s.as_object_mut()) {
        servers.remove("divlens");
    }

    let json = serde_json::to_string_pretty(&val).map_err(|e| e.to_string())?;

    // Write BOM-free UTF-8
    std::fs::write(path, json.as_bytes()).map_err(|e| e.to_string())?;
    Ok(())
}
