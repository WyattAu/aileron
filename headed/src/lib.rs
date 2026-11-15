//! aileron-headed: the headed shell layer (S4 stage 3 step 2).
//!
//! GPU compositor, terminal, input, chrome bridge, popup — everything
//! that needs a display. Depends on aileron-core (leaf) and
//! aileron-engine (wry/WebKit panes). The agent binary never links
//! this crate.

pub mod gfx;

#[cfg(feature = "terminal")]
pub mod terminal;

pub mod ui;

pub mod chrome_bridge;
pub mod popup;

pub mod input;

#[cfg(feature = "sync")]
pub mod sync;

#[cfg(feature = "arp")]
pub mod arp;

#[cfg(feature = "lua")]
pub mod lua;

#[cfg(feature = "platform")]
#[allow(unsafe_code)] // x11_dl FFI bindings require unsafe; module-scoped override
pub mod platform;

// ---- Root surface: mirrors the main crate's lib.rs so modules moved
// from the main crate keep resolving `crate::x::` paths unchanged.
pub use aileron_core::downloads;
#[cfg(feature = "llm")]
pub use aileron_core::llm;
#[cfg(feature = "passwords")]
pub use aileron_core::passwords;
pub use aileron_core::{
    color, config, db, debug_capturer, extensions, git, i18n, mcp, net, profiling, scripts, state,
    update_check, wm, workspace_restore,
};
pub use aileron_engine::{offscreen_webview, pane_ops, servo};

// app::instance touches env vars pre-thread-spawn (documented SAFETY there)
#[allow(unsafe_code)]
pub mod app;
#[cfg(feature = "native-chrome")]
pub mod chrome_native;

/// Fresco chrome bar spike: status bar painted by fresco, composited
/// CPU-side over the live webview frame before gfx presents. See
/// docs/spikes/fresco-chrome.md (step 2 of the sequencing).
#[cfg(feature = "fresco-chrome")]
pub mod chrome;
pub mod frame_tasks;
#[cfg(feature = "headed")]
pub mod test_harness;
pub mod wry_actions;
