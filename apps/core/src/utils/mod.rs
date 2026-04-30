//! # Utils Module - DivLens Core Utilities
//!
//! This module contains utility functions and helpers used across DivLens.
//!
//! ## Submodules
//!
//! - `env_fixer`: Enterprise-grade environment rehydration for packaged apps
//!
//! ## Why This Exists
//!
//! When Electron/Tauri packages an app, it loses access to the user's shell
//! environment (PATH, CONDA_PREFIX, etc.). This module provides solutions
//! to "rehydrate" the environment so tools like pip, npm, and cargo work correctly.

pub mod env_fixer;
pub mod data_dir;

