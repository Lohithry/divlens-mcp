//! # CLI Module — DivLens Lifecycle Management
//!
//! Provides terminal-facing subcommands for managing DivLens installations:
//!
//! - `status`    — Show installation status and connected AI clients
//! - `doctor`    — Run diagnostic health checks on the installation
//! - `config`    — Show or verify AI client configurations
//! - `uninstall` — Completely remove DivLens from the machine
//!
//! ## Design Philosophy
//!
//! These commands run **outside** the MCP protocol — they write directly to
//! stdout/stderr with colored, human-readable output. They are designed for
//! terminal users, not AI clients.
//!
//! ## Cross-Platform
//!
//! All commands use platform-aware path resolution to find binaries, configs,
//! and data directories on macOS, Linux, and Windows.

pub mod status;
pub mod doctor;
pub mod config;
pub mod uninstall;
mod ui;
mod paths;
