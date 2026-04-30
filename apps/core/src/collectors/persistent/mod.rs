pub mod advanced_stats;
pub mod hardware_identity;
pub mod storage; // Re-adding storage module
pub mod file_types; // Re-adding file_types module
pub mod env_doctor; // Environment doctor for runtime detection

#[cfg(target_os = "windows")]
pub mod windows_hardware;

#[cfg(target_os = "linux")]
pub mod linux_hardware;
