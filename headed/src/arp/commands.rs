//! ARP commands — mutation operations sent from mobile clients to the desktop app.
//!
//! These are dispatched via an `mpsc` channel from the ARP server (async runtime)
//! to the main thread, where they're processed each frame in `about_to_wait()`.

use uuid::Uuid;

/// A command from a mobile client that mutates desktop state.
#[derive(Debug, Clone)]
pub enum ArpCommand {
    // ─── Tab operations ───
    /// Create a new tab and optionally navigate to a URL.
    TabCreate { url: Option<String> },
    /// Navigate a specific tab to a URL. If tab_id is None, navigate active tab.
    TabNavigate { tab_id: Option<Uuid>, url: String },
    /// Close a specific tab. If tab_id is None, close active tab.
    TabClose { tab_id: Option<Uuid> },
    /// Activate (focus) a specific tab.
    TabActivate { tab_id: Uuid },
    /// Go back in a tab's history.
    TabGoBack { tab_id: Option<Uuid> },
    /// Go forward in a tab's history.
    TabGoForward { tab_id: Option<Uuid> },
    /// Reload a tab.
    TabReload { tab_id: Option<Uuid> },

    // ─── Clipboard operations ───
    /// Set the system clipboard contents.
    ClipboardSet { text: String },
    /// Read the system clipboard contents. Result is pushed back via ARP notify.
    ClipboardGet { request_id: u64 },

    // ─── Quickmark operations ───
    /// Open a quickmark by its key.
    QuickmarkOpen { key: String },
}
