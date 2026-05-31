//! # `divlens config` — Show AI Client Configurations

use super::ui;
use super::paths;

/// Run the `config --show` subcommand.
pub fn run_show() {
    ui::print_banner("⚙️", "DivLens Configuration", Some("AI client config status"));

    let clients = paths::get_ai_client_configs();
    for client in &clients {
        let result = paths::config_has_divlens(&client.config_path);
        let path_str = client.config_path.display().to_string();
        match result {
            paths::ConfigCheckResult::Configured => {
                ui::print_ok(client.name);
                ui::print_info(&format!("  Path: {}", path_str));
                ui::print_info("  Status: ● configured");
            }
            paths::ConfigCheckResult::ConfiguredWithBom => {
                ui::print_warn(&format!("{} (BOM detected)", client.name));
                ui::print_info(&format!("  Path: {}", path_str));
            }
            paths::ConfigCheckResult::ValidJsonNoDivlens => {
                ui::print_skip(client.name);
                ui::print_info(&format!("  Path: {}", path_str));
                ui::print_info("  Status: ○ installed, divlens not configured");
            }
            paths::ConfigCheckResult::InvalidJson => {
                ui::print_fail(client.name);
                ui::print_info(&format!("  Path: {}", path_str));
                ui::print_info("  Status: ✗ invalid JSON");
            }
            paths::ConfigCheckResult::NotFound if !client.is_alternate => {
                ui::print_skip(client.name);
                ui::print_info("  Status: ○ not installed");
            }
            _ => {}
        }
        eprintln!();
    }
    ui::print_line();
    ui::print_info("To reconfigure, re-run the DivLens installer.");
    eprintln!();
}
