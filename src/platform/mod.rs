//! Platform abstraction layer.
//!
//! Provides a unified interface for platform-specific operations:
//! - System keyring access
//! - Config file paths
//! - Default browser detection
//! - Native dialog support
//! - OS-specific key handling

pub mod config;
pub mod paths;

pub use config::*;
pub use paths::*;
