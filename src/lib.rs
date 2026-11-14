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
pub use aileron_headed::gfx;
pub mod input;
#[cfg(feature = "llm")]
/// S4 stage 2c: moved to aileron-core (self-contained, serde+tracing only).
pub use aileron_core::llm;
#[cfg(feature = "lua")]
pub mod lua;
/// S4 stage 2: the MCP stack (bridge, TCP server, tools, transport, ACI)
/// moved to aileron-core — it is fully self-contained and is the agent
/// runtime's core dependency. Re-exported for API compatibility.
pub use aileron_core::mcp;
#[cfg(feature = "passwords")]
pub use aileron_core::passwords;
pub use aileron_engine::offscreen_webview;
pub use aileron_engine::pane_ops;
pub mod platform;
#[cfg(feature = "headed")]
pub mod popup;
pub use aileron_core::scripts;
pub use aileron_engine::servo;
#[cfg(feature = "sync")]
pub mod sync;
#[cfg(feature = "terminal")]
pub use aileron_headed::terminal;
#[cfg(feature = "headed")]
pub mod test_harness;
/// S4 stage 2c: moved to aileron-core (terminal hooks inverted to callers).
pub use aileron_core::workspace_restore;
pub use aileron_headed::ui;
pub mod wry_actions;
