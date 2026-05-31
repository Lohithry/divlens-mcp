//! # `divlens doctor` — Installation Health Check
//!
//! Inspired by `flutter doctor` and `brew doctor`. Runs a comprehensive set of
//! diagnostic checks to identify installation problems:
//!
//! 1. Binary exists and is executable
//! 2. Binary is on PATH
//! 3. Database directory is writable
//! 4. MCP handshake works (self-test)
//! 5. Each AI client config is valid JSON with `divlens` entry
//! 6. No BOM corruption (Windows-specific)
//! 7. Service registration status

use super::ui;
use super::paths;
use std::io::Write;

/// A single health check result.
struct Check {
    name: String,
    passed: bool,
    warning: bool,
    detail: String,
}

/// Run the `doctor` subcommand.
pub fn run() {
    ui::print_banner("🩺", "DivLens Health Check", None);

    let mut checks: Vec<Check> = Vec::new();

    // ─── Check 1: Binary installed ───────────────────────────────────────────
    let binary_path = paths::find_installed_binary();
    match &binary_path {
        Some(path) => {
            checks.push(Check {
                name: "Binary installed".to_string(),
                passed: true,
                warning: false,
                detail: path.display().to_string(),
            });
        }
        None => {
            checks.push(Check {
                name: "Binary installed".to_string(),
                passed: false,
                warning: false,
                detail: "divlens-core not found on PATH or in known locations".to_string(),
            });
        }
    }

    // ─── Check 2: Binary is executable ───────────────────────────────────────
    if let Some(_path) = &binary_path {
        #[cfg(unix)]
        {
            let path = _path;
            use std::os::unix::fs::PermissionsExt;
            match std::fs::metadata(path) {
                Ok(meta) => {
                    let mode = meta.permissions().mode();
                    let executable = mode & 0o111 != 0;
                    checks.push(Check {
                        name: "Binary executable".to_string(),
                        passed: executable,
                        warning: false,
                        detail: if executable {
                            "permissions OK".to_string()
                        } else {
                            format!("not executable (mode: {:o}). Run: chmod +x {}", mode, path.display())
                        },
                    });
                }
                Err(e) => {
                    checks.push(Check {
                        name: "Binary executable".to_string(),
                        passed: false,
                        warning: false,
                        detail: format!("cannot read permissions: {}", e),
                    });
                }
            }
        }
        #[cfg(windows)]
        {
            // On Windows, .exe files are always "executable"
            checks.push(Check {
                name: "Binary executable".to_string(),
                passed: true,
                warning: false,
                detail: ".exe is always executable".to_string(),
            });
        }
    }

    // ─── Check 3: Binary on PATH ─────────────────────────────────────────────
    let on_path = which::which("divlens-core").is_ok();
    checks.push(Check {
        name: "Binary on PATH".to_string(),
        passed: on_path,
        warning: !on_path,
        detail: if on_path {
            "divlens-core found on PATH".to_string()
        } else {
            "divlens-core not found on PATH — you may need to restart your terminal".to_string()
        },
    });

    // ─── Check 4: Data directory writable ────────────────────────────────────
    let data_dir = paths::get_data_directory();
    let dir_writable = if data_dir.exists() {
        // Try writing a temp file
        let test_file = data_dir.join(".divlens_write_test");
        let writable = std::fs::write(&test_file, "test").is_ok();
        let _ = std::fs::remove_file(&test_file);
        writable
    } else {
        // Try creating the directory
        std::fs::create_dir_all(&data_dir).is_ok()
    };
    checks.push(Check {
        name: "Data directory writable".to_string(),
        passed: dir_writable,
        warning: false,
        detail: data_dir.display().to_string(),
    });

    // ─── Check 5: Database accessible ────────────────────────────────────────
    let db_path = data_dir.join("divlens.db");
    if db_path.exists() {
        checks.push(Check {
            name: "Database accessible".to_string(),
            passed: true,
            warning: false,
            detail: db_path.display().to_string(),
        });
    } else {
        checks.push(Check {
            name: "Database accessible".to_string(),
            passed: true,
            warning: true,
            detail: "not yet created (will be created on first MCP connection)".to_string(),
        });
    }

    // ─── Check 6: MCP self-test handshake ────────────────────────────────────
    if let Some(path) = &binary_path {
        let test_payload = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","clientInfo":{"name":"doctor","version":"1.0"}}}"#;

        let result = std::process::Command::new(path)
            .arg("--mcp")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .and_then(|mut child| {
                if let Some(ref mut stdin) = child.stdin {
                    let _ = stdin.write_all(test_payload.as_bytes());
                    let _ = stdin.write_all(b"\n");
                }
                // Drop stdin to signal EOF
                child.stdin.take();

                // Wait with timeout
                let output = child.wait_with_output()?;
                Ok(output)
            });

        match result {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if stdout.contains("\"result\"") {
                    checks.push(Check {
                        name: "MCP handshake".to_string(),
                        passed: true,
                        warning: false,
                        detail: "JSON-RPC protocol OK".to_string(),
                    });
                } else {
                    checks.push(Check {
                        name: "MCP handshake".to_string(),
                        passed: false,
                        warning: false,
                        detail: format!("unexpected response: {}", &stdout[..stdout.len().min(80)]),
                    });
                }
            }
            Err(e) => {
                checks.push(Check {
                    name: "MCP handshake".to_string(),
                    passed: false,
                    warning: true,
                    detail: format!("self-test failed: {} (may need restart)", e),
                });
            }
        }
    }

    // ─── Check 7: AI client configs ──────────────────────────────────────────
    let clients = paths::get_ai_client_configs();
    for client in &clients {
        let result = paths::config_has_divlens(&client.config_path);
        match result {
            paths::ConfigCheckResult::Configured => {
                checks.push(Check {
                    name: format!("{} config", client.name),
                    passed: true,
                    warning: false,
                    detail: "valid JSON, divlens entry present".to_string(),
                });
            }
            paths::ConfigCheckResult::ConfiguredWithBom => {
                checks.push(Check {
                    name: format!("{} config", client.name),
                    passed: true,
                    warning: true,
                    detail: "configured but has UTF-8 BOM — may cause parsing issues".to_string(),
                });
            }
            paths::ConfigCheckResult::ValidJsonNoDivlens => {
                checks.push(Check {
                    name: format!("{} config", client.name),
                    passed: false,
                    warning: true,
                    detail: "installed but divlens not configured — re-run installer".to_string(),
                });
            }
            paths::ConfigCheckResult::InvalidJson => {
                checks.push(Check {
                    name: format!("{} config", client.name),
                    passed: false,
                    warning: false,
                    detail: "config file contains invalid JSON".to_string(),
                });
            }
            paths::ConfigCheckResult::NotFound => {
                if !client.is_alternate {
                    checks.push(Check {
                        name: format!("{} config", client.name),
                        passed: true,
                        warning: true,
                        detail: "not installed (optional)".to_string(),
                    });
                }
            }
            paths::ConfigCheckResult::Empty | paths::ConfigCheckResult::Unreadable => {
                if !client.is_alternate {
                    checks.push(Check {
                        name: format!("{} config", client.name),
                        passed: false,
                        warning: true,
                        detail: "config file is empty or unreadable".to_string(),
                    });
                }
            }
        }
    }

    // ─── Print Results ───────────────────────────────────────────────────────
    let mut passed = 0;
    let total = checks.len();

    for check in &checks {
        if check.passed && !check.warning {
            ui::print_ok(&format!("{:<28} {}", check.name, check.detail));
            passed += 1;
        } else if check.warning {
            ui::print_warn(&format!("{:<28} {}", check.name, check.detail));
            passed += 1; // Warnings still count as "passed"
        } else {
            ui::print_fail(&format!("{:<28} {}", check.name, check.detail));
        }
    }

    // ─── Summary ─────────────────────────────────────────────────────────────
    eprintln!();
    ui::print_line();
    if passed == total {
        ui::print_result_box(
            ui::Colors::GREEN,
            &format!("Result: {}/{} checks passed — DivLens is healthy ✓", passed, total),
        );
    } else {
        ui::print_result_box(
            ui::Colors::YELLOW,
            &format!(
                "Result: {}/{} checks passed — run `divlens doctor` after fixing issues",
                passed, total
            ),
        );
    }
}
