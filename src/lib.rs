// ---- GATE-1 S4 stage 1a: core modules moved to aileron-core ------------
// One-for-one re-exports: every `crate::x::` and `aileron::x::` call site
// keeps working while the physical crates settle.
pub use aileron_core::{
    color, config, db, debug_capturer, extensions, git, i18n, net, profiling, state, update_check,
    wm,
};

pub mod app;
#[cfg(feature = "arp")]
pub mod arp;
#[cfg(feature = "headed")]
pub mod chrome_bridge;
#[cfg(feature = "native-chrome")]
pub mod chrome_native;
pub use aileron_core::downloads;
pub mod frame_tasks;
#[cfg(feature = "headed")]
pub mod gfx;
pub mod input;
#[cfg(feature = "llm")]
pub mod llm;
#[cfg(feature = "lua")]
pub mod lua;
/// S4 stage 2: the MCP stack (bridge, TCP server, tools, transport, ACI)
/// moved to aileron-core — it is fully self-contained and is the agent
/// runtime's core dependency. Re-exported for API compatibility.
pub use aileron_core::mcp;
pub mod offscreen_webview;
#[cfg(feature = "passwords")]
pub mod passwords;
pub mod platform;
#[cfg(feature = "headed")]
pub mod popup;
pub mod scripts;
pub mod servo;
#[cfg(feature = "sync")]
pub mod sync;
#[cfg(feature = "terminal")]
pub mod terminal;
#[cfg(feature = "headed")]
pub mod test_harness;
pub mod ui;
/// S4 stage 2c: moved to aileron-core (terminal hooks inverted to callers).
pub use aileron_core::workspace_restore;
pub mod wry_actions;
