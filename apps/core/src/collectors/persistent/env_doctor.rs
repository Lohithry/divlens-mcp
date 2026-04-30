// ==================================================================================
// 🧠 DIVLENS ENVIRONMENT DOCTOR - Static Runtime Detection
// ==================================================================================
// This module detects installed language runtimes and identifies path conflicts.
// It runs ONCE at startup (static data - doesn't change frequently).
//
// DESIGN PRINCIPLES:
// 1. Static: Runs once, not continuously (runtimes don't change often)
// 2. Cross-Platform: Works on Windows, macOS, and Linux
// 3. Robust: Graceful degradation if binaries are missing
// 4. Conflict Detection: Identifies shadowing issues (multiple versions in PATH)
// ==================================================================================

use std::process::Command;
use crate::models::devtools::{LanguageRuntime, PathConflict};

/// Scans the development environment for installed runtimes and path conflicts
/// 
/// # Returns
/// Tuple of (runtimes, conflicts):
/// - runtimes: List of detected language runtimes with versions
/// - conflicts: List of path conflicts (multiple versions of same binary)
/// 
/// # Example
/// ```
/// let (runtimes, conflicts) = scan_dev_environment();
/// // runtimes: [Python 3.11.0 at /usr/bin/python3, Node.js 18.0.0 at /usr/bin/node]
/// // conflicts: [python3: /usr/bin/python3 shadows /opt/conda/bin/python3]
/// ```
pub fn scan_dev_environment() -> (Vec<LanguageRuntime>, Vec<PathConflict>) {
    let mut runtimes = Vec::new();
    let mut conflicts = Vec::new();

    // The "Big List" of binaries to check
    // Format: (binary_name, version_flag, human_readable_label)
    let targets = vec![
        ("python3", "--version", "Python"),
        ("python", "--version", "Python"),
        ("node", "--version", "Node.js"),
        ("java", "-version", "Java"), // Java uses -version (one dash, not two)
        ("javac", "-version", "Java Compiler"),
        ("gcc", "--version", "C (GCC)"),
        ("g++", "--version", "C++ (G++)"),
        ("clang", "--version", "C (Clang)"),
        ("clang++", "--version", "C++ (Clang++)"),
        ("cargo", "--version", "Rust"),
        ("rustc", "--version", "Rust Compiler"),
        ("go", "version", "Go"),
        ("ruby", "--version", "Ruby"),
        ("php", "--version", "PHP"),
        ("dotnet", "--version", ".NET"),
    ];

    // Scan each target binary
    for (bin, arg, label) in targets {
        // Get all paths where this binary exists (handles PATH conflicts)
        let paths = get_all_paths(bin);

        // If binary exists, detect version and add to runtimes
        if let Some(active) = paths.first() {
            // Robust version extraction (handles different output formats)
            if let Ok(version) = get_version(bin, arg) {
                // Detect version manager from path
                let manager = detect_manager(active);
                
                // Create runtime entry
                runtimes.push(LanguageRuntime {
                    name: label.to_string(),
                    version: version,
                    active_path: active.clone(),
                    detected_manager: manager,
                });
            }

            // Conflict Detection: If multiple paths exist, there's a potential conflict
            if paths.len() > 1 {
                // Determine severity based on whether paths are likely different versions
                let severity = determine_conflict_severity(bin, &paths);
                
                conflicts.push(PathConflict {
                    binary: bin.to_string(),
                    active_source: active.clone(),
                    shadowed_sources: paths[1..].to_vec(),
                    severity: severity,
                });
            }
        }
    }
    
    (runtimes, conflicts)
}

// ==================================================================================
// HELPER FUNCTIONS
// ==================================================================================

/// Gets all paths where a binary exists in PATH
/// This is crucial for detecting conflicts (multiple versions)
/// 
/// # Platform-Specific Implementation
/// - Unix (macOS/Linux): Uses `which -a` (all occurrences)
/// - Windows: Uses `where` command (all occurrences)
/// 
/// # Returns
/// Vector of paths, ordered by PATH precedence (first = active)
/// Gets all paths where a binary exists in PATH
/// CRITICAL: This function is fast but can still block if PATH is very large
/// The frontend handles this by rendering immediately and showing loading state
fn get_all_paths(bin: &str) -> Vec<String> {
    #[cfg(unix)]
    {
        // Unix: 'which -a' shows all occurrences
        // Example: "which -a python3" returns:
        // /usr/bin/python3
        // /opt/conda/bin/python3
        if let Ok(output) = Command::new("which").arg("-a").arg(bin).output() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            return stdout
                .lines()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
    }
    
    #[cfg(windows)]
    {
        // Windows: 'where' shows all occurrences
        // Example: "where python" returns:
        // C:\Python311\python.exe
        // C:\Users\user\AppData\Local\Programs\Python\Python311\python.exe
        if let Ok(output) = Command::new("where").arg(bin).output() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            return stdout
                .lines()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
    }
    
    // Fallback: Try single path lookup (faster, less comprehensive)
    #[cfg(unix)]
    {
        if let Ok(output) = Command::new("which").arg(bin).output() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let path = stdout.trim().to_string();
            if !path.is_empty() {
                return vec![path];
            }
        }
    }
    
    #[cfg(windows)]
    {
        if let Ok(output) = Command::new("where").arg(bin).output() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let path = stdout.lines().next().map(|s| s.trim().to_string());
            if let Some(path) = path {
                if !path.is_empty() {
                    return vec![path];
                }
            }
        }
    }
    
    vec![]
}

/// Extracts version string from binary output
/// Handles different output formats across languages and platforms
/// 
/// # Arguments
/// * `bin` - Binary name to check
/// * `arg` - Version flag (e.g., "--version", "-version", "version")
/// 
/// # Returns
/// Version string or "Detected" if parsing fails
/// 
/// # Examples
/// - Python: "Python 3.11.0" -> "3.11.0"
/// - Node: "v18.0.0" -> "18.0.0"
/// - Java: "openjdk version \"17.0.1\"" -> "17.0.1"
/// - GCC: "gcc (GCC) 11.4.0" -> "11.4.0"
/// Extracts version string from binary output
/// Handles different output formats across languages and platforms
/// 
/// # Arguments
/// * `bin` - Binary name to check
/// * `arg` - Version flag (e.g., "--version", "-version", "version")
/// 
/// # Returns
/// Version string or "Detected" if parsing fails
/// 
/// # Examples
/// - Python: "Python 3.11.0" -> "3.11.0"
/// - Node: "v18.0.0" -> "18.0.0"
/// - Java: "openjdk version \"17.0.1\"" -> "17.0.1"
/// - GCC: "gcc (GCC) 11.4.0" -> "11.4.0"
fn get_version(bin: &str, arg: &str) -> Result<String, ()> {
    // Execute version command
    // Note: Some commands (like Java) output to stderr, not stdout
    let output = match Command::new(bin).arg(arg).output() {
        Ok(o) => o,
        Err(_) => return Err(()), // Binary not found or execution failed
    };
    
    // Combine stdout and stderr (Java uses stderr for version)
    let stdout_str = String::from_utf8_lossy(&output.stdout);
    let stderr_str = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{} {}", stdout_str, stderr_str);
    
    // Parse version from output (different formats for different tools)
    for line in combined.lines() {
        let line_lower = line.to_lowercase();
        let line_trimmed = line.trim();
        
        // Skip empty lines
        if line_trimmed.is_empty() {
            continue;
        }
        
        // Pattern 1: "Python X.Y.Z" format (Python specific)
        if line_trimmed.starts_with("Python ") {
            let version = line_trimmed.strip_prefix("Python ").unwrap_or("").trim();
            if !version.is_empty() && version.chars().next().map_or(false, |c| c.is_numeric()) {
                return Ok(version.to_string());
            }
        }
        
        // Pattern 2: "ruby X.Y.Z" format (Ruby specific)
        if line_lower.starts_with("ruby ") {
            let parts: Vec<&str> = line_trimmed.split_whitespace().collect();
            if parts.len() >= 2 {
                let version = parts[1].trim_start_matches('v');
                if version.chars().next().map_or(false, |c| c.is_numeric()) {
                    return Ok(version.to_string());
                }
            }
        }
        
        // Pattern 3: "vX.Y.Z" format (Node.js specific)
        if line_trimmed.starts_with('v') && line_trimmed.len() > 1 {
            let version = &line_trimmed[1..];
            if version.chars().next().map_or(false, |c| c.is_numeric()) {
                return Ok(version.to_string());
            }
        }
        
        // Pattern 4: "version X.Y.Z" or contains "version"
        if line_lower.contains("version") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            
            // Look for version-like tokens (start with digit or 'v' followed by digit)
            for part in &parts {
                let trimmed = part.trim_matches('"').trim_matches('\'');
                
                // Handle "v1.2.3" format (remove 'v' prefix)
                if trimmed.starts_with('v') && trimmed.len() > 1 {
                    let version = &trimmed[1..];
                    if version.chars().next().map_or(false, |c| c.is_numeric()) {
                        return Ok(version.to_string());
                    }
                }
                
                // Handle "1.2.3" format (starts with digit)
                if trimmed.chars().next().map_or(false, |c| c.is_numeric()) {
                    // Check if it looks like a version (contains dot or is numeric)
                    if trimmed.contains('.') || trimmed.chars().all(|c| c.is_numeric()) {
                        return Ok(trimmed.to_string());
                    }
                }
            }
        }
        
        // Pattern 5: "go version goX.Y.Z" format (Go specific)
        if line_lower.contains("go version") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            for part in &parts {
                if part.starts_with("go") && part.len() > 2 {
                    let version = &part[2..];
                    if version.chars().next().map_or(false, |c| c.is_numeric()) {
                        return Ok(version.to_string());
                    }
                }
            }
        }
        
        // Pattern 6: Look for any version-like pattern in the line (X.Y.Z)
        // This is a fallback for formats we haven't explicitly handled
        let words: Vec<&str> = line_trimmed.split_whitespace().collect();
        for word in &words {
            let clean = word.trim_matches(|c: char| !c.is_numeric() && c != '.');
            if !clean.is_empty() && clean.contains('.') {
                let first_char = clean.chars().next();
                if first_char.map_or(false, |c| c.is_numeric()) {
                    // Verify it looks like a version (has at least one dot and numbers)
                    let parts: Vec<&str> = clean.split('.').collect();
                    if parts.len() >= 2 && parts.iter().all(|p| !p.is_empty()) {
                        return Ok(clean.to_string());
                    }
                }
            }
        }
    }
    
    // If we can't parse, return a generic "Detected" to indicate it exists
    Ok("Detected".to_string())
}

/// Detects version manager from binary path
/// Identifies common version managers like NVM, Conda, Rustup, etc.
/// 
/// # Arguments
/// * `path` - Full path to the binary
/// 
/// # Returns
/// Some(manager_name) if detected, None otherwise
/// 
/// # Examples
/// - "/home/user/.nvm/versions/node/v18.0.0/bin/node" -> Some("NVM")
/// - "/opt/conda/bin/python3" -> Some("Conda")
/// - "/home/user/.cargo/bin/cargo" -> Some("Rustup")
fn detect_manager(path: &str) -> Option<String> {
    let path_lower = path.to_lowercase();
    
    // Node Version Manager (NVM)
    if path_lower.contains(".nvm") {
        return Some("NVM".to_string());
    }
    
    // Conda / Miniconda / Anaconda
    if path_lower.contains("conda") || path_lower.contains("miniconda") || path_lower.contains("anaconda") {
        return Some("Conda".to_string());
    }
    
    // Rustup (Rust toolchain manager)
    if path_lower.contains(".cargo") || path_lower.contains("rustup") {
        return Some("Rustup".to_string());
    }
    
    // SDKMan (Java/Kotlin/Gradle manager)
    if path_lower.contains(".sdkman") {
        return Some("SDKMan".to_string());
    }
    
    // pyenv (Python version manager)
    if path_lower.contains("pyenv") {
        return Some("pyenv".to_string());
    }
    
    // Homebrew (macOS package manager)
    if path_lower.contains("/homebrew/") || path_lower.contains("/opt/homebrew/") {
        return Some("Homebrew".to_string());
    }
    
    // Volta (Node.js version manager)
    if path_lower.contains("volta") {
        return Some("Volta".to_string());
    }
    
    // asdf (Universal version manager)
    if path_lower.contains(".asdf") {
        return Some("asdf".to_string());
    }
    
    // None detected (likely system-wide installation)
    None
}

/// Determines conflict severity based on path analysis
/// 
/// # Arguments
/// * `bin` - Binary name
/// * `paths` - All paths where binary exists
/// 
/// # Returns
/// Severity string: "Low", "Medium", "High", "Critical"
/// 
/// # Logic
/// - Critical: Different version managers (e.g., system Python vs Conda Python)
/// - High: Different installation locations (likely different versions)
/// - Medium: Same location, different symlinks
/// - Low: Same location, likely same version
fn determine_conflict_severity(_bin: &str, paths: &[String]) -> String {
    if paths.len() < 2 {
        return "Low".to_string();
    }
    
    // Check if paths are from different version managers
    let managers: Vec<Option<String>> = paths.iter().map(|p| detect_manager(p)).collect();
    let unique_managers: std::collections::HashSet<Option<String>> = managers.iter().cloned().collect();
    
    if unique_managers.len() > 1 {
        // Different version managers = Critical conflict
        return "Critical".to_string();
    }
    
    // Check if paths are in completely different directories
    let dirs: Vec<&str> = paths.iter()
        .filter_map(|p| {
            std::path::Path::new(p).parent()
                .and_then(|d| d.to_str())
        })
        .collect();
    
    let unique_dirs: std::collections::HashSet<&str> = dirs.iter().cloned().collect();
    
    if unique_dirs.len() > 1 {
        // Different directories = High conflict (likely different versions)
        return "High".to_string();
    }
    
    // Same directory, different paths (might be symlinks)
    "Medium".to_string()
}

