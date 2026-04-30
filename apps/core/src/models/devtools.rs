// ==================================================================================
// 🧠 DIVLENS DEVTOOLS SCHEMA - The Unified Data Structure
// ==================================================================================
// This module defines the complete data schema for developer tools monitoring.
// It captures everything: language runtimes, installed packages, and path conflicts.
//
// DESIGN PRINCIPLES:
// 1. Unified: Holds data from all sources (file IO, terminal commands, package managers)
// 2. Type-Safe: Uses proper Rust types for version strings, paths, etc.
// 3. Serializable: All structs can be converted to JSON for frontend/AI consumption
// 4. Extensible: Easy to add new languages or package managers
// ==================================================================================

use serde::Serialize;

/// Master container for all developer tools information
/// This is the top-level structure returned by devtools diagnostics
#[derive(Debug, Serialize, Default, Clone)]
pub struct DevToolsSnapshot {
    /// List of detected language runtimes (Python, Node, Java, C++, etc.)
    pub runtimes: Vec<LanguageRuntime>,
    
    /// List of path conflicts (multiple versions of same binary)
    pub path_conflicts: Vec<PathConflict>,
    
    /// List of installed packages from all sources
    pub packages: Vec<PackageInfo>,
}

/// Language Runtime Information
/// Represents a detected programming language runtime with version and path
#[derive(Debug, Serialize, Clone)]
pub struct LanguageRuntime {
    /// Human-readable name (e.g., "Python", "Node.js", "Java (OpenJDK)", "C++ (G++)")
    pub name: String,
    
    /// Version string (e.g., "3.11.0", "18.0.0", "17.0.2", "11.4.0")
    pub version: String,
    
    /// Active binary path (the one that would be executed)
    /// Example: "/usr/bin/python3", "C:\\Program Files\\nodejs\\node.exe"
    pub active_path: String,
    
    /// Detected version manager (if applicable)
    /// Examples: "NVM", "Conda", "Rustup", "SDKMan", "pyenv"
    /// None if installed system-wide or via package manager
    pub detected_manager: Option<String>,
}

/// Path Conflict Information
/// Detects when multiple versions of the same binary exist in PATH
/// This helps identify shadowing issues (e.g., system Python vs Conda Python)
#[derive(Debug, Serialize, Clone)]
pub struct PathConflict {
    /// Binary name (e.g., "python3", "node", "java")
    pub binary: String,
    
    /// The active path (first in PATH, will be executed)
    pub active_source: String,
    
    /// Other paths that are shadowed (exist but won't be used)
    pub shadowed_sources: Vec<String>,
    
    /// Severity level: "Low", "Medium", "High", "Critical"
    /// High/Critical: Different versions that could cause issues
    /// Low/Medium: Same version in different locations (usually fine)
    pub severity: String,
}

/// Package Information
/// Represents an installed package/library from any source
#[derive(Debug, Serialize, Clone)]
pub struct PackageInfo {
    /// Package name (e.g., "requests", "express", "serde", "openssl")
    pub name: String,
    
    /// Version string (e.g., "2.31.0", "4.18.2", "1.0.0", "3.0.2")
    pub version: String,
    
    /// Source/manager that provided this package
    /// Examples: "pip", "npm", "cargo", "pkg-config", "maven", "java-prop"
    pub source: String,
    
    /// Optional: Installation path (for packages with file locations)
    pub install_path: Option<String>,
}

