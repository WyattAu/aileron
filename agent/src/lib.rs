//! aileron-agent runtime (S4 stage 2d-3): the headless MCP daemon.
//!
//! Depends on aileron-core + aileron-engine only — no headed shell.
//! The GTK loop stays here because WebKitGTK delivers its events on it.

pub mod runtime;

pub use runtime::run;
