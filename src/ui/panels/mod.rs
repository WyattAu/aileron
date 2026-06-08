//! UI panels module.
//!
//! All panel rendering has been migrated to the Leptos WASM chrome webview
//! (aileron-chrome crate). This module is retained for the `build_ui` export
//! used by the library API; the function is now a no-op since chrome handles
//! all rendering.

/// No-op: all UI rendering is handled by the Leptos WASM chrome webview.
/// Retained for API compatibility.
#[allow(dead_code)]
pub fn build_ui(_app_state: &mut crate::app::AppState, _chrome_active: bool) {
    // No-op: chrome webview renders all panels.
}
