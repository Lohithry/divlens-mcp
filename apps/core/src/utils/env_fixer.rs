//! # Environment Rehydration Module (Enterprise-Grade)
//!
//! ## The Problem: "Empty Shell" in Packaged Desktop Apps
//!
//! When you run `npm run dev`, your terminal has access to the full PATH:
//! ```text
//! /opt/anaconda3/bin:/opt/homebrew/bin:/usr/local/bin:~/.cargo/bin:...
//! ```
//!
//! But when Electron packages the app into a `.dmg` or `.exe`, it launches
//! with a **minimal system PATH**:
//! ```text
//! /usr/bin:/bin:/usr/sbin:/sbin
//! ```
//!
//! This means `pip3`, `npm`, `cargo`, and other dev tools are NOT found!
//!
//! ## The Solution: Shell Injection
//!
//! We spawn the user's actual shell (zsh, bash, PowerShell) in interactive
//! login mode, capture ALL environment variables, and inject them into
//! the current process. This is the same technique used by VS Code, Hyper,
//! and IntelliJ.
//!
//! ## Safety Features
//!
//! 1. **Timeout Guard (2 seconds)**: Prevents app freeze if user has slow shell
//!    plugins (like oh-my-zsh with git status on large repos)
//!
//! 2. **Banner Filter**: Ignores "Welcome to Oh-My-Zsh!" and "Last login" messages
//!    that would break the KEY=VALUE parser
//!
//! 3. **Graceful Fallback**: If shell method fails, we scan known tool locations
//!    (/opt/anaconda3, /opt/homebrew, ~/.cargo, etc.)
//!
//! ## Usage
//!
//! Call `rehydrate()` ONCE at the very start of main(), before any other code:
//!
//! ```rust
//! fn main() {
//!     utils::env_fixer::rehydrate();  // FIRST!
//!     // ... rest of your app
//! }
//! ```
//!
//! ## Cross-Platform Support
//!
//! - **macOS**: Uses `zsh -ilc "env"` (default shell since Catalina)
//! - **Linux**: Uses `bash -ilc "env"` (or user's $SHELL)
//! - **Windows**: Uses PowerShell to dump environment variables

use std::collections::HashMap;
use std::env;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;
use wait_timeout::ChildExt;

// =============================================================================
// CONFIGURATION CONSTANTS
// =============================================================================

/// Maximum time to wait for shell environment extraction.
///
/// ## Why 5 seconds?
/// - Most shells load in <500ms
/// - Slow plugins (oh-my-zsh with pyenv/nvm) can take 2-4 seconds
/// - MCP servers have no visible UI freeze, so a longer timeout is safe
/// - Beyond 5 seconds something is genuinely wrong; fall back gracefully
///
/// If timeout occurs, we gracefully fall back to scanning known paths.
const SHELL_TIMEOUT_SECS: u64 = 5;

// =============================================================================
// PUBLIC API
// =============================================================================

/// Rehydrates the process environment with the user's shell configuration.
///
/// # What This Does
///
/// 1. Detects the operating system (macOS, Linux, Windows)
/// 2. Spawns the user's shell in interactive login mode
/// 3. Captures all environment variables (PATH, CONDA_PREFIX, etc.)
/// 4. Injects them into the current process using `std::env::set_var`
/// 5. Falls back to scanning known paths if shell method fails
///
/// # When To Call
///
/// Call this **ONCE** at the very beginning of `main()`, before:
/// - Initializing any database connections
/// - Starting any servers
/// - Running any tool commands
///
/// # Example
///
/// ```rust
/// fn main() {
///     // 🛡️ CRITICAL: Rehydrate environment FIRST
///     utils::env_fixer::rehydrate();
///
///     // Now pip3, npm, cargo will be found correctly
///     let output = Command::new("pip3").arg("--version").output();
/// }
/// ```
///
/// # Performance
///
/// - Typical execution: 100-500ms
/// - Maximum (with timeout): 2 seconds
/// - Fallback path scanning: ~50ms
pub fn rehydrate() {
    eprintln!("🔄 [EnvFixer] Starting environment rehydration...");

    let start = std::time::Instant::now();

    // Step 1: Try the shell injection method (most accurate)
    let new_vars = if cfg!(target_os = "windows") {
        get_windows_env()
    } else {
        get_unix_shell_env()
    };

    // Step 2: Process the results
    match new_vars {
        Some(vars) if !vars.is_empty() => {
            // Success! Inject all variables into current process
            let path_updated = vars.contains_key("PATH");

            for (key, value) in vars {
                // SAFETY: We are setting environment variables at app startup,
                // before any other threads are spawned. This is safe because
                // there's no concurrent access to the environment.
                unsafe {
                    env::set_var(&key, &value);
                }
            }

            if path_updated {
                eprintln!(
                    "✅ [EnvFixer] Injected user environment in {:?}",
                    start.elapsed()
                );

                // Log first few PATH entries for debugging
                if let Ok(path) = env::var("PATH") {
                    let separator = if cfg!(target_os = "windows") { ";" } else { ":" };
                    let paths: Vec<&str> = path.split(separator).take(5).collect();
                    eprintln!("   PATH (first 5): {:?}...", paths);
                }
            }
        }
        _ => {
            // Shell method failed - use fallback
            eprintln!("⚠️ [EnvFixer] Shell method failed. Applying fallback paths...");
            apply_fallback_paths();
        }
    }
}

// =============================================================================
// UNIX IMPLEMENTATION (macOS / Linux)
// =============================================================================

/// Extracts environment variables from the user's interactive shell.
///
/// # How It Works
///
/// 1. Reads `$SHELL` to detect user's shell (zsh, bash, fish, etc.)
/// 2. Spawns shell with `-ilc "env"` flags:
///    - `-i`: Interactive mode (loads .zshrc, .bashrc)
///    - `-l`: Login mode (loads .zprofile, .bash_profile)
///    - `-c`: Execute command and exit
///    - `"env"`: Dump all environment variables to stdout
/// 3. Applies 2-second timeout to prevent app freeze
/// 4. Parses output with banner filtering
///
/// # Why Interactive + Login Mode?
///
/// Anaconda, NVM, and Homebrew typically add their PATH modifications to:
/// - `.zshrc` (interactive config) - loaded with `-i`
/// - `.zprofile` (login config) - loaded with `-l`
///
/// Using both flags ensures we capture ALL user customizations.
///
/// # Returns
///
/// - `Some(HashMap)`: Successfully extracted environment variables
/// - `None`: Shell failed, timed out, or produced invalid output
fn get_unix_shell_env() -> Option<HashMap<String, String>> {
    // Step 1: Detect user's default shell
    // Falls back to /bin/bash if $SHELL is not set
    let shell = env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
    eprintln!("   Detected shell: {}", shell);

    // Step 2: Spawn shell process with interactive + login flags
    //
    // Command: $SHELL -ilc "env"
    //
    // This is equivalent to opening a new terminal window and typing "env".
    // The shell will:
    // 1. Load ~/.zshrc or ~/.bashrc (interactive config)
    // 2. Load ~/.zprofile or ~/.bash_profile (login config)
    // 3. Execute "env" to dump all variables
    // 4. Exit
    let mut child = match Command::new(&shell)
        .args(&["-ilc", "env"])
        .stdout(Stdio::piped())  // Capture stdout (the env output)
        .stderr(Stdio::null())   // Suppress shell warnings/banners on stderr
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            eprintln!("❌ [EnvFixer] Failed to spawn shell: {}", e);
            return None;
        }
    };

    // Step 3: CRITICAL — Drain stdout BEFORE waiting on the process.
    //
    // BUG FIX: Previously we called wait_timeout() first (which reaps the
    // process), then wait_with_output() — but the child handle is consumed by
    // the first wait, so wait_with_output() returned empty bytes.  We now take
    // the stdout pipe handle up-front and read it on a background thread, then
    // apply the timeout to the process exit.
    let mut stdout_pipe = match child.stdout.take() {
        Some(pipe) => pipe,
        None => {
            eprintln!("❌ [EnvFixer] Failed to capture stdout pipe");
            return None;
        }
    };

    // Spawn a dedicated reader thread so the pipe never blocks the wait.
    let reader_handle = std::thread::spawn(move || -> String {
        let mut buf = String::new();
        let _ = stdout_pipe.read_to_string(&mut buf);
        buf
    });

    // Step 4: CRITICAL — Timeout Guard
    //
    // Problem: User might have slow shell plugins (oh-my-zsh with git status,
    // pyenv, nvm) that take several seconds to load.
    //
    // Solution: Kill the process if it takes longer than SHELL_TIMEOUT_SECS.
    let status = match child.wait_timeout(Duration::from_secs(SHELL_TIMEOUT_SECS)) {
        Ok(Some(status)) => status,
        Ok(None) => {
            // Timeout! Kill the hanging process — the reader thread will then
            // get EOF and return whatever partial output it collected.
            let _ = child.kill();
            eprintln!(
                "⚠️ [EnvFixer] Shell timed out after {}s (slow .zshrc?) — using fallback",
                SHELL_TIMEOUT_SECS
            );
            // Even on timeout, try to use whatever was written before kill.
            // If the shell printed env vars before hanging, we can still use them.
            let partial = reader_handle.join().unwrap_or_default();
            if !partial.is_empty() {
                let vars = parse_env_output(&partial);
                if vars.contains_key("PATH") {
                    eprintln!("   Partial output usable ({} vars)", vars.len());
                    return Some(vars);
                }
            }
            return None;
        }
        Err(e) => {
            eprintln!("❌ [EnvFixer] Error waiting for shell: {}", e);
            return None;
        }
    };

    // Step 5: Check exit status
    if !status.success() {
        eprintln!("⚠️ [EnvFixer] Shell exited with non-zero status");
        return None;
    }

    // Step 6: Collect the output from our reader thread.
    let stdout_str = reader_handle.join().unwrap_or_default();

    // Step 7: Parse with banner filtering
    let vars = parse_env_output(&stdout_str);

    // Step 8: Verify we got a valid PATH
    if !vars.contains_key("PATH") {
        eprintln!("⚠️ [EnvFixer] Parsed output but no PATH found");
        return None;
    }

    eprintln!("   Parsed {} environment variables", vars.len());
    Some(vars)
}

// =============================================================================
// WINDOWS IMPLEMENTATION
// =============================================================================

/// Extracts environment variables using PowerShell.
///
/// # Why PowerShell?
///
/// On Windows, developers often set environment variables in their PowerShell
/// `$PROFILE` (similar to `.zshrc` on Mac). The standard `cmd.exe` doesn't
/// load these customizations.
///
/// # Command Used
///
/// ```powershell
/// Get-ChildItem env: | ForEach-Object { $_.Name + '=' + $_.Value }
/// ```
///
/// This outputs all environment variables in `KEY=VALUE` format, exactly like
/// the Unix `env` command.
///
/// # Note on -NoProfile
///
/// We use `-NoProfile` to skip loading the user's PowerShell profile for speed.
/// This is acceptable because we're querying the current environment, which
/// already includes any session-level variables set by the parent process.
///
/// If you need profile-specific variables, change to `-Command` without `-NoProfile`.
fn get_windows_env() -> Option<HashMap<String, String>> {
    eprintln!("   Using PowerShell strategy...");

    // Spawn PowerShell to dump environment variables
    let mut child = match Command::new("powershell")
        .args(&[
            "-NoProfile",   // Skip profile for speed
            "-Command",
            "Get-ChildItem env: | ForEach-Object { $_.Name + '=' + $_.Value }",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            eprintln!("❌ [EnvFixer] Failed to spawn PowerShell: {}", e);
            return None;
        }
    };

    // Apply timeout guard (same as Unix)
    match child.wait_timeout(Duration::from_secs(SHELL_TIMEOUT_SECS)) {
        Ok(Some(_)) => {}
        Ok(None) => {
            let _ = child.kill();
            eprintln!("⚠️ [EnvFixer] PowerShell timed out");
            return None;
        }
        Err(_) => return None,
    };

    let output = child.wait_with_output().ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);

    let vars = parse_env_output(&stdout);

    if vars.is_empty() {
        return None;
    }

    Some(vars)
}

// =============================================================================
// PARSER WITH BANNER FILTER
// =============================================================================

/// Parses shell output into a HashMap of environment variables.
///
/// # Banner Filtering
///
/// When you run `zsh -ilc "env"`, the shell might output extra text:
/// - "Welcome to Oh-My-Zsh!"
/// - "Last login: Mon Dec 30 10:00:00"
/// - ANSI color codes
/// - Plugin messages
///
/// This function filters out all non-env lines by:
/// 1. Skipping empty lines
/// 2. Skipping lines without `=`
/// 3. Validating key format (alphanumeric + underscore)
///
/// # Example Input
///
/// ```text
/// Last login: Mon Dec 30 10:00:00 on ttys000
/// Welcome to Oh-My-Zsh!
/// PATH=/opt/anaconda3/bin:/opt/homebrew/bin:/usr/local/bin
/// HOME=/Users/lohith
/// SHELL=/bin/zsh
/// ```
///
/// # Example Output
///
/// ```rust
/// {
///     "PATH" => "/opt/anaconda3/bin:/opt/homebrew/bin:/usr/local/bin",
///     "HOME" => "/Users/lohith",
///     "SHELL" => "/bin/zsh"
/// }
/// ```
fn parse_env_output(output: &str) -> HashMap<String, String> {
    let mut vars = HashMap::new();

    for line in output.lines() {
        // Skip empty lines
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Skip lines without '=' (banners, messages, etc.)
        // This filters out "Welcome to Oh-My-Zsh!" and similar
        if !line.contains('=') {
            continue;
        }

        // Split only on FIRST '=' because values can contain '='
        // Example: "MY_URL=https://example.com?foo=bar"
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim();

            // Validate key format: must be alphanumeric + underscore
            // This filters out malformed lines like "2024-01-01=date"
            if key.is_empty() {
                continue;
            }
            if !key.chars().all(|c| c.is_alphanumeric() || c == '_') {
                continue;
            }
            // Env-style names must not start with a digit (e.g. skip "123_INVALID")
            if !key
                .chars()
                .next()
                .map(|c| c.is_ascii_alphabetic() || c == '_')
                .unwrap_or(false)
            {
                continue;
            }

            vars.insert(key.to_string(), value.to_string());
        }
    }

    vars
}

// =============================================================================
// FALLBACK PATH INJECTION
// =============================================================================

/// Scans common tool installation locations and adds them to PATH.
///
/// # When This Runs
///
/// This function is called when the shell injection method fails due to:
/// - Shell timeout (slow plugins)
/// - Broken .zshrc (syntax error)
/// - Missing shell executable
///
/// # Strategy
///
/// Instead of trying to run the shell, we directly check if common tool
/// directories exist on disk and prepend them to PATH.
///
/// # Paths Scanned (macOS)
///
/// | Path | Tool |
/// |------|------|
/// | `/opt/anaconda3/bin` | Anaconda Python |
/// | `~/anaconda3/bin` | Anaconda (user install) |
/// | `~/miniconda3/bin` | Miniconda |
/// | `/opt/homebrew/bin` | Homebrew (Apple Silicon) |
/// | `/usr/local/bin` | Homebrew (Intel) |
/// | `~/.cargo/bin` | Rust/Cargo |
/// | `~/.nvm/versions/node/*/bin` | NVM Node.js |
///
/// # Paths Scanned (Windows)
///
/// | Path | Tool |
/// |------|------|
/// | `%USERPROFILE%\anaconda3\Scripts` | Anaconda |
/// | `%USERPROFILE%\miniconda3\Scripts` | Miniconda |
/// | `%APPDATA%\npm` | Global NPM packages |
/// | `%USERPROFILE%\.cargo\bin` | Rust/Cargo |
fn apply_fallback_paths() {
    // Step 1: Find user's home directory
    // Uses the `dirs` crate for cross-platform compatibility
    let home = dirs::home_dir().unwrap_or_else(|| {
        // Fallback: try environment variables
        env::var("HOME")
            .or_else(|_| env::var("USERPROFILE"))
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/"))
    });

    let mut paths_to_add: Vec<String> = Vec::new();

    // Step 2: Build list of candidate paths based on OS
    let candidates: Vec<PathBuf> = if cfg!(target_os = "macos") {
        vec![
            // ===== Anaconda / Miniconda (HIGHEST PRIORITY) =====
            // Anaconda is the most common Python distribution for data science.
            // It installs to /opt/anaconda3 (system) or ~/anaconda3 (user).
            PathBuf::from("/opt/anaconda3/bin"),
            home.join("anaconda3/bin"),
            home.join("miniconda3/bin"),
            home.join("opt/anaconda3/bin"),
            
            // ===== Homebrew (macOS Package Manager) =====
            // Apple Silicon Macs use /opt/homebrew
            // Intel Macs use /usr/local
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/opt/homebrew/sbin"),
            PathBuf::from("/usr/local/bin"),
            
            // ===== Rust / Cargo =====
            // Rust installs cargo to ~/.cargo/bin
            home.join(".cargo/bin"),
            
            // ===== NVM (Node Version Manager) =====
            // NVM installs Node to ~/.nvm/versions/node/vX.X.X/bin
            // We use a helper function to find the latest version
            find_nvm_node_path(&home),
            
            // ===== Python Framework (macOS) =====
            // Official Python.org installer
            PathBuf::from("/Library/Frameworks/Python.framework/Versions/Current/bin"),
            PathBuf::from("/Library/Frameworks/Python.framework/Versions/3.12/bin"),
            PathBuf::from("/Library/Frameworks/Python.framework/Versions/3.11/bin"),
        ]
    } else if cfg!(target_os = "windows") {
        vec![
            // ===== Anaconda / Miniconda =====
            home.join("anaconda3\\Scripts"),
            home.join("anaconda3\\condabin"),
            home.join("miniconda3\\Scripts"),
            home.join("miniconda3\\condabin"),
            PathBuf::from("C:\\ProgramData\\anaconda3\\Scripts"),
            PathBuf::from("C:\\ProgramData\\miniconda3\\Scripts"),
            
            // ===== Node.js / NPM =====
            home.join("AppData\\Roaming\\npm"),
            PathBuf::from("C:\\Program Files\\nodejs"),
            
            // ===== Rust / Cargo =====
            home.join(".cargo\\bin"),
            
            // ===== Python (Official Installer) =====
            home.join("AppData\\Local\\Programs\\Python\\Python312\\Scripts"),
            home.join("AppData\\Local\\Programs\\Python\\Python311\\Scripts"),
            home.join("AppData\\Local\\Programs\\Python\\Python310\\Scripts"),
            PathBuf::from("C:\\Python312\\Scripts"),
            PathBuf::from("C:\\Python311\\Scripts"),
        ]
    } else {
        // ===== Linux =====
        vec![
            // Anaconda / Miniconda
            PathBuf::from("/opt/anaconda3/bin"),
            home.join("anaconda3/bin"),
            home.join("miniconda3/bin"),
            
            // Rust / Cargo
            home.join(".cargo/bin"),
            
            // NVM Node.js
            find_nvm_node_path(&home),
            
            // System paths
            PathBuf::from("/usr/local/bin"),
            PathBuf::from("/snap/bin"),
            
            // Linuxbrew (Homebrew for Linux)
            home.join(".linuxbrew/bin"),
            PathBuf::from("/home/linuxbrew/.linuxbrew/bin"),
        ]
    };

    // Step 3: Check which paths actually exist
    for path in candidates {
        // Skip empty paths (from failed helper functions)
        if path.as_os_str().is_empty() {
            continue;
        }

        // Verify the directory exists and is accessible
        if path.exists() && path.is_dir() {
            eprintln!("   💡 Found: {:?}", path);
            paths_to_add.push(path.to_string_lossy().to_string());
        }
    }

    if paths_to_add.is_empty() {
        eprintln!("   No additional paths found");
        return;
    }

    // Step 4: Prepend found paths to existing PATH
    //
    // We PREPEND (not append) so our paths take priority over system defaults.
    // This ensures /opt/anaconda3/bin/python is found before /usr/bin/python.
    let separator = if cfg!(target_os = "windows") { ";" } else { ":" };
    let current_path = env::var("PATH").unwrap_or_default();
    let new_path = format!(
        "{}{}{}",
        paths_to_add.join(separator),
        separator,
        current_path
    );

    // SAFETY: We are setting environment variables at app startup,
    // before any other threads are spawned. This is safe because
    // there's no concurrent access to the environment.
    unsafe {
        env::set_var("PATH", &new_path);
    }
    eprintln!("✅ [EnvFixer] Injected {} fallback paths", paths_to_add.len());
}

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

/// Finds the NVM-managed Node.js binary path.
///
/// # NVM Directory Structure
///
/// NVM (Node Version Manager) installs Node.js versions to:
/// ```text
/// ~/.nvm/versions/node/
/// ├── v18.19.0/
/// │   └── bin/
/// │       ├── node
/// │       └── npm
/// ├── v20.10.0/
/// │   └── bin/
/// └── v21.5.0/
///     └── bin/
/// ```
///
/// # Strategy
///
/// 1. List all directories in `~/.nvm/versions/node/`
/// 2. Sort by version number (descending)
/// 3. Return the `bin/` subdirectory of the latest version
///
/// # Returns
///
/// - Path to latest Node.js bin directory (e.g., `~/.nvm/versions/node/v21.5.0/bin`)
/// - Empty PathBuf if NVM is not installed or no versions found
fn find_nvm_node_path(home: &PathBuf) -> PathBuf {
    let nvm_dir = home.join(".nvm/versions/node");

    // Check if NVM is installed
    if !nvm_dir.exists() {
        return PathBuf::new();
    }

    // List all version directories
    if let Ok(entries) = std::fs::read_dir(&nvm_dir) {
        let mut versions: Vec<PathBuf> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();

        // Sort by version number (descending)
        // v21.5.0 > v20.10.0 > v18.19.0
        versions.sort_by(|a, b| {
            let a_ver = a.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let b_ver = b.file_name().and_then(|n| n.to_str()).unwrap_or("");
            b_ver.cmp(a_ver) // Descending order
        });

        // Return bin/ directory of latest version
        if let Some(latest) = versions.first() {
            let bin_path = latest.join("bin");
            if bin_path.exists() {
                return bin_path;
            }
        }
    }

    PathBuf::new()
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that the parser correctly handles banner noise
    #[test]
    fn test_parse_env_with_banners() {
        let input = r#"
Last login: Mon Dec 30 10:00:00 on ttys000
Welcome to Oh-My-Zsh!

PATH=/opt/anaconda3/bin:/usr/local/bin
HOME=/Users/test
SHELL=/bin/zsh
"#;

        let vars = parse_env_output(input);

        assert_eq!(vars.get("PATH"), Some(&"/opt/anaconda3/bin:/usr/local/bin".to_string()));
        assert_eq!(vars.get("HOME"), Some(&"/Users/test".to_string()));
        assert_eq!(vars.get("SHELL"), Some(&"/bin/zsh".to_string()));
        assert!(!vars.contains_key("Last login"));
        assert!(!vars.contains_key("Welcome to Oh-My-Zsh!"));
    }

    /// Test that values with '=' are handled correctly
    #[test]
    fn test_parse_env_with_equals_in_value() {
        let input = "MY_URL=https://example.com?foo=bar&baz=qux";
        let vars = parse_env_output(input);
        
        assert_eq!(
            vars.get("MY_URL"),
            Some(&"https://example.com?foo=bar&baz=qux".to_string())
        );
    }

    /// Test that invalid keys are filtered out
    #[test]
    fn test_parse_env_filters_invalid_keys() {
        let input = r#"
VALID_KEY=value
123_INVALID=should_be_skipped
=empty_key
ANOTHER_VALID=works
"#;

        let vars = parse_env_output(input);

        assert!(vars.contains_key("VALID_KEY"));
        assert!(vars.contains_key("ANOTHER_VALID"));
        // Keys starting with numbers or empty are filtered
        assert!(!vars.contains_key("123_INVALID"));
        assert!(!vars.contains_key(""));
    }
}

