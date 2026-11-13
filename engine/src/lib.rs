//! aileron-engine: the pane rendering layer (S4 stage 3).
//!
//! Wry/WebKitGTK panes, the offscreen webview manager, and the GTK
//! event pump. Depends on aileron-core; depends on nothing headed
//! beyond GTK itself — the wgpu compositor (gfx) stays in the headed
//! crate.

pub mod ipc;
pub mod offscreen_events;
pub mod offscreen_webview;
pub mod pane_ops;
pub mod pump;
pub mod servo;
pub mod spellcheck;
