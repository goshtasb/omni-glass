//! Sandbox subsystem — OS-level process sandboxing for MCP plugins.
//!
//! Submodules:
//! - env_filter: Environment variable filtering (all platforms)
//! - macos: sandbox-exec profile generation (macOS only)
//! - risk: Permission risk level calculation
//! - windows/linux: stubs with env filtering only

pub mod env_filter;
pub mod risk;

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "linux")]
pub mod linux;
