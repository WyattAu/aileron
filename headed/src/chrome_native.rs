//! Native GTK chrome — the `native-chrome` spike (YP-WM-PANE-LIFECYCLE-002 /
//! GUI roadmap decision 2026-10-29).
//!
//! Replaces the Leptos-WASM chrome webview + offscreen compositing pipeline
//! with native GTK widgets: an [`gtk::Overlay`] holding the live WebKitGTK
//! webview as the base layer and an [`gtk::HeaderBar`]-derived chrome stack
//! on top. No frame capture, no wgpu compositing, no IPC state pushes.
//!
//! Spike decision gates (see docs/spikes/native-chrome-overlay.md):
//!   - idle memory flat vs the ~3GB baseline (target: no growth)
//!   - idle CPU ~0 (no compositing loop)
//!   - MCP same-batch read latency after navigate (target: <1s, no settle)
//!
//! Status: SKELETON — layout + webview attach point only. Toolbar actions,
//! omnibox, palette, and tab-strip wiring are TODO against
//! `app_state`/`frame_tasks` APIs.

use gtk::prelude::*;

/// A native chrome shell for one browser window.
///
/// Layout: `gtk::Overlay`
///   - base layer: the content container (WebKitGTK webviews positioned by
///     the BSP tiler, as real widgets)
///   - overlay: the chrome stack (header bar with omnibox + tab strip)
pub struct NativeChrome {
    /// Root widget to place in the window.
    pub root: gtk::Box,
    /// Container the content webviews attach to (BSP-managed).
    pub content_container: gtk::Box,
    /// The chrome overlay layer.
    pub chrome_overlay: gtk::Overlay,
    /// Header bar hosting the omnibox entry and tab controls.
    pub header_bar: gtk::HeaderBar,
    /// Omnibox entry (URL / search input).
    pub omnibox: gtk::Entry,
}

impl NativeChrome {
    /// Build the chrome shell. Call once per browser window, before any
    /// content panes are attached.
    pub fn build() -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);

        let header_bar = gtk::HeaderBar::new();
        header_bar.set_show_close_button(true);
        header_bar.set_title(Some("Aileron"));

        let omnibox = gtk::Entry::new();
        omnibox.set_hexpand(true);
        omnibox.set_placeholder_text(Some("Search or enter address"));
        header_bar.set_custom_title(Some(&omnibox));

        let chrome_overlay = gtk::Overlay::new();
        chrome_overlay.add(&header_bar);

        let content_container = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        chrome_overlay.add(&content_container);

        root.pack_start(&chrome_overlay, true, true, 0);

        Self {
            root,
            content_container,
            chrome_overlay,
            header_bar,
            omnibox,
        }
    }

    /// Attach the content container's GTK parent relationship for a pane.
    ///
    /// TODO(spike): replace the offscreen `gtk_fixed` + capture pipeline with
    /// direct `webkit2gtk::WebView` children here, positioned by the BSP
    /// tiler (reposition_all_panes target).
    pub fn content_slot(&self) -> &gtk::Box {
        &self.content_container
    }

    /// Connect the omnibox activate signal to a navigation handler.
    ///
    /// TODO(spike): route through the same WryAction queue as MCP navigates
    /// so keyboard and MCP paths share one navigation implementation
    /// (YP-MCP-NAV-EPOCH-001 epoch advancement included).
    pub fn on_omnibox_activate<F: Fn(&str) + 'static>(&self, f: F) {
        let _ = f;
        // gtk::Entry::connect_activate placeholder — wired in the spike's
        // second step alongside the tab-strip signals.
    }
}
