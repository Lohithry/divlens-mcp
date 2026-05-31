//! # Cross-Platform Path Resolution
//!
//! Centralised resolution of all platform-specific paths used by DivLens:
//! binary install location, AI client config files, data directories, and
//! service registration paths.
//!
//! This module is the single source of truth for "where things live" on
//! each OS — used by `status`, `doctor`, `config`, and `uninstall`.

use std::path::PathBuf;

/// An AI client that DivLens can connect to.
#[derive(Debug, Clone)]
pub struct AiClient {
    pub name: &'static str,
    pub config_path: PathBuf,
    /// Whether this is a secondary/alternative path (e.g. MSIX sandbox)
    pub is_alternate: bool,
}

/// Get all known AI client config paths for the current platform.
pub fn get_ai_client_configs() -> Vec<AiClient> {
    let mut clients = Vec::new();

    #[cfg(target_os = "macos")]
    {
        let home = dirs::home_dir().unwrap_or_default();
        clients.push(AiClient {
            name: "Claude Desktop",
            config_path: home
                .join("Library/Application Support/Claude/claude_desktop_config.json"),
            is_alternate: false,
        });
        clients.push(AiClient {
            name: "Cursor",
            config_path: home.join(".cursor/mcp.json"),
            is_alternate: false,
        });
        clients.push(AiClient {
            name: "Windsurf",
            config_path: home.join(".codeium/windsurf/mcp_config.json"),
            is_alternate: false,
        });
        clients.push(AiClient {
            name: "Antigravity",
            config_path: home.join(".gemini/mcp_config.json"),
            is_alternate: false,
        });
    }

    #[cfg(target_os = "linux")]
    {
        let home = dirs::home_dir().unwrap_or_default();
        // Standard Linux Claude Desktop path
        let claude_config = home.join(".config/Claude/claude_desktop_config.json");
        // Flatpak Claude Desktop path
        let claude_flatpak = home.join(
            ".var/app/com.anthropic.Claude/config/Claude/claude_desktop_config.json",
        );
        if claude_flatpak.parent().map_or(false, |p| p.exists()) {
            clients.push(AiClient {
                name: "Claude Desktop (Flatpak)",
                config_path: claude_flatpak,
                is_alternate: true,
            });
        }
        clients.push(AiClient {
            name: "Claude Desktop",
            config_path: claude_config,
            is_alternate: false,
        });
        clients.push(AiClient {
            name: "Cursor",
            config_path: home.join(".cursor/mcp.json"),
            is_alternate: false,
        });
        clients.push(AiClient {
            name: "Windsurf",
            config_path: home.join(".codeium/windsurf/mcp_config.json"),
            is_alternate: false,
        });
        clients.push(AiClient {
            name: "Antigravity",
            config_path: home.join(".gemini/mcp_config.json"),
            is_alternate: false,
        });
    }

    #[cfg(target_os = "windows")]
    {
        let appdata = std::env::var("APPDATA").unwrap_or_default();
        let localappdata = std::env::var("LOCALAPPDATA").unwrap_or_default();
        let userprofile = std::env::var("USERPROFILE").unwrap_or_default();

        // Standard Claude Desktop
        clients.push(AiClient {
            name: "Claude Desktop",
            config_path: PathBuf::from(&appdata).join("Claude\\claude_desktop_config.json"),
            is_alternate: false,
        });
        // MSIX/Store sandboxed Claude Desktop
        let msix_path = PathBuf::from(&localappdata).join(
            "Packages\\Claude_pzs8sxrjxfjjc\\LocalCache\\Roaming\\Claude\\claude_desktop_config.json",
        );
        let msix_package_dir =
            PathBuf::from(&localappdata).join("Packages\\Claude_pzs8sxrjxfjjc");
        if msix_package_dir.exists() {
            clients.push(AiClient {
                name: "Claude Desktop (Store)",
                config_path: msix_path,
                is_alternate: true,
            });
        }
        clients.push(AiClient {
            name: "Cursor",
            config_path: PathBuf::from(&userprofile).join(".cursor\\mcp.json"),
            is_alternate: false,
        });
        clients.push(AiClient {
            name: "Windsurf",
            config_path: PathBuf::from(&appdata).join("Codeium\\windsurf\\mcp_config.json"),
            is_alternate: false,
        });
        clients.push(AiClient {
            name: "Antigravity",
            config_path: PathBuf::from(&userprofile).join(".gemini\\mcp_config.json"),
            is_alternate: false,
        });
    }

    clients
}

/// Get the expected binary install locations for the current platform.
pub fn get_binary_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    #[cfg(target_os = "macos")]
    {
        paths.push(PathBuf::from("/usr/local/bin/divlens-core"));
        if let Some(home) = dirs::home_dir() {
            paths.push(home.join(".local/bin/divlens-core"));
        }
    }

    #[cfg(target_os = "linux")]
    {
        paths.push(PathBuf::from("/usr/local/bin/divlens-core"));
        if let Some(home) = dirs::home_dir() {
            paths.push(home.join(".local/bin/divlens-core"));
        }
    }

    #[cfg(target_os = "windows")]
    {
        let localappdata = std::env::var("LOCALAPPDATA").unwrap_or_default();
        paths.push(PathBuf::from(&localappdata).join("DivLens\\divlens-core.exe"));
    }

    paths
}

/// Get the DivLens data directory (where SQLite DB and LanceDB live).
pub fn get_data_directory() -> PathBuf {
    crate::utils::data_dir::get_data_dir()
}

/// Get the service file path for the current platform.
pub fn get_service_path() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        dirs::home_dir().map(|h| h.join("Library/LaunchAgents/io.divlens.mcp.plist"))
    }

    #[cfg(target_os = "linux")]
    {
        dirs::home_dir().map(|h| h.join(".config/systemd/user/divlens-mcp.service"))
    }

    #[cfg(target_os = "windows")]
    {
        // Task Scheduler — no file path, managed by schtasks
        None
    }
}

/// Find the currently installed binary (first match from known paths).
pub fn find_installed_binary() -> Option<PathBuf> {
    // First: check if we can find it on PATH via `which`
    if let Ok(path) = which::which("divlens-core") {
        return Some(path);
    }

    // Fallback: check known install locations
    for path in get_binary_paths() {
        if path.exists() {
            return Some(path);
        }
    }

    None
}

/// Check if a config file contains a valid `divlens` MCP entry.
pub fn config_has_divlens(config_path: &std::path::Path) -> ConfigCheckResult {
    if !config_path.exists() {
        return ConfigCheckResult::NotFound;
    }

    let content = match std::fs::read_to_string(config_path) {
        Ok(c) => c,
        Err(_) => return ConfigCheckResult::Unreadable,
    };

    let trimmed = content.trim();
    if trimmed.is_empty() {
        return ConfigCheckResult::Empty;
    }

    // Check for BOM (Windows issue)
    let has_bom = content.as_bytes().starts_with(&[0xEF, 0xBB, 0xBF]);

    let parse_content = if has_bom { &content[3..] } else { &content };

    match serde_json::from_str::<serde_json::Value>(parse_content) {
        Ok(val) => {
            if let Some(servers) = val.get("mcpServers") {
                if servers.get("divlens").is_some() {
                    if has_bom {
                        ConfigCheckResult::ConfiguredWithBom
                    } else {
                        ConfigCheckResult::Configured
                    }
                } else {
                    ConfigCheckResult::ValidJsonNoDivlens
                }
            } else {
                ConfigCheckResult::ValidJsonNoDivlens
            }
        }
        Err(_) => ConfigCheckResult::InvalidJson,
    }
}

/// Result of checking an AI client config file.
#[derive(Debug, PartialEq)]
pub enum ConfigCheckResult {
    /// File doesn't exist
    NotFound,
    /// File exists but can't be read
    Unreadable,
    /// File exists but is empty
    Empty,
    /// File contains valid JSON but no `divlens` entry
    ValidJsonNoDivlens,
    /// File contains invalid JSON
    InvalidJson,
    /// DivLens is properly configured
    Configured,
    /// DivLens is configured but file has a UTF-8 BOM (Windows issue)
    ConfiguredWithBom,
}
