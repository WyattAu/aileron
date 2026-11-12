//! aileron-core: everything the agent runtime needs, nothing headed.
//!
//! Stage 1a of the GATE-1 crate split (docs/spikes/gate1-s4-plan.md):
//! the engine-free leaf modules moved out of the aileron crate. Call
//! sites in `aileron` are unchanged — the aileron lib re-exports these
//! modules one-for-one.

pub mod agent;
pub mod color;
pub mod config;
pub mod db;
pub mod debug_capturer;
pub mod downloads;
pub mod extensions;
pub mod git;
pub mod i18n;
pub mod llm;
pub mod mcp;
pub mod net;
#[cfg(feature = "passwords")]
pub mod passwords;
pub mod platform;
pub mod profiling;
pub mod scripts;
pub mod state;
pub mod ui;
pub mod update_check;
pub mod wm;
pub mod workspace_restore;
