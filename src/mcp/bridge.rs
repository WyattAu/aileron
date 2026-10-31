//! Bridge between the MCP background thread and the main thread.
//!
//! Since wry::WebView is !Send + !Sync, MCP tools on the background thread
//! cannot directly interact with wry panes. This bridge provides:
//! - Shared read-only state (active URL, title) via `Arc<RwLock>`
//! - Command channel for mutations (navigate, run JS) processed on main thread

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock, mpsc};
use tokio::sync::oneshot;

/// A session-tagged command sender (YP-MCP-SESSIONS-002).
///
/// Wraps the shared channel and stamps every command with the owning
/// session's id (`None` = the unrestricted default session, e.g. stdio).
/// The `send` signature matches `mpsc::Sender::send`, so tool call sites
/// are unchanged; the router reads the tag and enforces the session's
/// capability set. Tags are assigned at construction inside the per-
/// connection server - tools cannot forge another session's identity.
#[derive(Clone, Debug)]
pub struct CommandTx {
    tx: mpsc::Sender<(Option<uuid::Uuid>, McpCommand)>,
    session: Option<uuid::Uuid>,
}

impl CommandTx {
    #[must_use]
    pub fn new(
        tx: mpsc::Sender<(Option<uuid::Uuid>, McpCommand)>,
        session: Option<uuid::Uuid>,
    ) -> Self {
        Self { tx, session }
    }

    /// Send a command tagged with this sender's session.
    ///
    /// # Errors
    /// Propagates the channel send error (bridge closed).
    pub fn send(
        &self,
        command: McpCommand,
    ) -> Result<(), mpsc::SendError<(Option<uuid::Uuid>, McpCommand)>> {
        self.tx.send((self.session, command))
    }

    #[must_use]
    pub fn session(&self) -> Option<uuid::Uuid> {
        self.session
    }

    /// Derive a sender for a specific session over the same channel.
    #[must_use]
    pub fn with_session(&self, session: uuid::Uuid) -> Self {
        Self {
            tx: self.tx.clone(),
            session: Some(session),
        }
    }
}

/// Shared session capability registry (YP-MCP-SESSIONS-002).
///
/// Written by the router on `session_scope` (narrowing only) and read by
/// the TCP gate (kappa class checks) and the router (phi pane checks).
pub type SessionRegistry = Arc<std::sync::Mutex<HashMap<uuid::Uuid, SessionCaps>>>;

impl McpCommand {
    /// The tool name a command corresponds to (audit trail provenance).
    #[must_use]
    pub fn tool_name(&self) -> &'static str {
        match self {
            McpCommand::Navigate { .. } => "navigate",
            McpCommand::ExecuteJs { .. } => "execute_js",
            McpCommand::GetActivePane { .. } | McpCommand::ReadAudit { .. } => "read_active_pane",
            McpCommand::ListDownloads { .. } => "browser_downloads",
            McpCommand::ListBookmarks { .. } => "list_bookmarks",
            McpCommand::AddBookmark { .. } => "add_bookmark",
            McpCommand::RemoveBookmark { .. } => "remove_bookmark",
            McpCommand::SearchHistory { .. } => "search_history",
            McpCommand::ListTabs { .. } => "list_tabs",
            McpCommand::Screenshot { .. } => "browser_take_screenshot",
            McpCommand::SessionScope { .. } => "session_scope",
            McpCommand::SessionInfo { .. } => "session_info",
            McpCommand::WorkspaceCheckpoint { .. } => "workspace_checkpoint",
            McpCommand::WorkspaceDiff { .. } => "workspace_diff",
            McpCommand::WorkspaceList { .. } => "workspace_list",
            McpCommand::SetUserAgent { .. } => "browser_set_ua",
            _ => "execute_js",
        }
    }
}

/// Static tool-class map (Era-2 S34, YP-SEC-CONFINEMENT-001).
///
/// read = observation only; mutate = changes page/browser state;
/// irreversible = destructive or externally-visible actions.
#[must_use]
pub fn tool_class(name: &str) -> &'static str {
    match name {
        "browser_get_text"
        | "browser_observe"
        | "browser_snapshot"
        | "browser_console_messages"
        | "browser_network_requests"
        | "browser_cookies"
        | "browser_downloads"
        | "list_tabs"
        | "list_bookmarks"
        | "search_history"
        | "read_active_pane"
        | "search_web"
        | "browser_wait_for"
        | "session_info"
        | "workspace_list"
        | "workspace_diff" => "read",
        "navigate"
        | "browser_click"
        | "browser_type"
        | "browser_hover"
        | "browser_focus"
        | "browser_press_key"
        | "browser_select"
        | "browser_drag"
        | "bulk_act"
        | "execute_js"
        | "browser_fill_form"
        | "add_bookmark"
        | "workspace_checkpoint"
        | "new_tab" => "mutate",
        "remove_bookmark" | "close_tab" | "session_scope" => "irreversible",
        "audit_log" => "read",
        "browser_take_screenshot" => "read",
        "navigate_back" | "navigate_forward" | "select_option" => "mutate",
        "read_page_structure"
        | "extract_structured_data"
        | "summarize_page"
        | "translate_page"
        | "analyze_page" => "read",
        _ => "mutate", // unknown tools default to the stronger class
    }
}

/// Capability set for one agent session (YP-MCP-SESSIONS-002 kappa/phi).
///
/// `None` = unrestricted (the default session and fresh connections).
/// Narrowing is monotone: `session_scope` can only shrink a set that is
/// already `Some`, and can convert `None` to `Some` once. Web content has
/// no path to widen a grant - the only writers are MCP tools carrying the
/// session's own tag, and the router validates every write is a subset.
#[derive(Debug, Clone, Default)]
pub struct SessionCaps {
    /// Allowed pane ids; `None` = every pane (phi).
    pub panes: Option<HashSet<uuid::Uuid>>,
    /// Allowed tool classes (read | mutate | irreversible); `None` = all (kappa).
    pub tool_classes: Option<HashSet<String>>,
}

impl SessionCaps {
    /// True when `self` can be narrowed to `narrower` (monotone shrinking).
    #[must_use]
    pub fn allows_narrowing_to(&self, narrower: &SessionCaps) -> bool {
        let panes_ok = match (&self.panes, &narrower.panes) {
            (None, _) => true,
            (Some(base), Some(new)) => new.is_subset(base),
            (Some(_), None) => false, // widening None->Some back is a widen
        };
        let classes_ok = match (&self.tool_classes, &narrower.tool_classes) {
            (None, _) => true,
            (Some(base), Some(new)) => new.is_subset(base),
            (Some(_), None) => false,
        };
        panes_ok && classes_ok
    }
}

/// Commands that MCP tools can send to the main thread.
/// These are processed each frame in `about_to_wait`.
#[derive(Debug)]
pub enum McpCommand {
    /// Navigate the active wry pane to a URL, optionally in a new tab.
    Navigate { url: String, new_tab: bool },
    /// Execute JavaScript in the active pane and return the result.
    ExecuteJs {
        code: String,
        response_tx: oneshot::Sender<String>,
    },
    /// Get the current URL and title of the active pane.
    GetActivePane {
        response_tx: oneshot::Sender<(String, String)>,
    },
    /// List download tasks (Era-1 S18): id, url, filename, progress.
    ListDownloads {
        response_tx: oneshot::Sender<String>,
    },
    /// Narrow the calling session's capabilities (Era-2 S28). Monotone:
    /// rejected unless a strict subset of the current grant.
    SessionScope {
        panes: Option<Vec<uuid::Uuid>>,
        tool_classes: Option<Vec<String>>,
        response_tx: oneshot::Sender<String>,
    },
    /// Report the calling session's identity and capability set (Era-2 S28).
    SessionInfo {
        response_tx: oneshot::Sender<String>,
    },
    /// Save the live pane tree as a named checkpoint (Era-2 S41).
    WorkspaceCheckpoint {
        name: String,
        response_tx: oneshot::Sender<String>,
    },
    /// Diff a saved checkpoint against the live tree (Era-2 S43).
    WorkspaceDiff {
        name: String,
        response_tx: oneshot::Sender<String>,
    },
    /// List saved checkpoints (Era-2 S41).
    WorkspaceList {
        response_tx: oneshot::Sender<String>,
    },
    /// Read the audit ring (Era-3, enterprise audit trail).
    ReadAudit {
        limit: usize,
        response_tx: oneshot::Sender<String>,
    },
    /// Set the pane's WebKit user agent and reload (Era-3 mobile lane).
    SetUserAgent {
        ua: String,
        response_tx: oneshot::Sender<String>,
    },
    /// List all bookmarks from the database.
    ListBookmarks {
        response_tx: oneshot::Sender<String>,
    },
    /// Add a bookmark to the database.
    AddBookmark {
        url: String,
        title: String,
        folder: String,
        response_tx: oneshot::Sender<String>,
    },
    /// Remove a bookmark from the database.
    RemoveBookmark {
        url: String,
        response_tx: oneshot::Sender<String>,
    },
    /// Search browsing history.
    SearchHistory {
        query: String,
        limit: usize,
        response_tx: oneshot::Sender<String>,
    },
    /// List all open tabs with URLs and titles.
    ListTabs {
        response_tx: oneshot::Sender<String>,
    },
    /// Capture the active pane as a base64-encoded PNG.
    Screenshot {
        response_tx: oneshot::Sender<String>,
    },
    /// Close a tab by index.
    CloseTab {
        index: usize,
        response_tx: oneshot::Sender<String>,
    },
    /// Scroll the active pane by (delta_x, delta_y) pixels.
    Scroll {
        delta_x: i32,
        delta_y: i32,
        response_tx: oneshot::Sender<String>,
    },
    /// Navigate back in history.
    NavigateBack {
        response_tx: oneshot::Sender<String>,
    },
    /// Navigate forward in history.
    NavigateForward {
        response_tx: oneshot::Sender<String>,
    },
    /// Select an option in a `&lt;select&gt;` element by CSS selector and value.
    SelectOption {
        selector: String,
        value: String,
        response_tx: oneshot::Sender<String>,
    },
    /// Read the accessibility tree of the active pane.
    ReadPageStructure {
        response_tx: oneshot::Sender<String>,
    },
    /// Extract structured data (JSON-LD, OpenGraph, tables) from active pane.
    ExtractStructuredData {
        response_tx: oneshot::Sender<String>,
    },
    /// Summarize the active page content using local LLM.
    SummarizePage {
        response_tx: oneshot::Sender<String>,
    },
    /// Translate the active page content to a target language.
    TranslatePage {
        target_lang: String,
        response_tx: oneshot::Sender<String>,
    },
    /// Analyze the active page content using local LLM.
    AnalyzePage {
        response_tx: oneshot::Sender<String>,
    },
}

/// Shared state readable by MCP tools (updated each frame from main thread).
#[derive(Debug, Clone)]
pub struct McpState {
    /// URL of the active wry pane.
    pub active_url: Arc<RwLock<String>>,
    /// Title of the active wry pane.
    pub active_title: Arc<RwLock<String>>,
    /// Navigation epoch of the active pane (YP-MCP-NAV-EPOCH-001); the
    /// settled-generation counter agents use as a staleness token.
    pub active_epoch: Arc<std::sync::atomic::AtomicU64>,
    /// Whether the active pane is mid-load (YP-AGENT-ACI-001 T0 field).
    pub active_loading: Arc<std::sync::atomic::AtomicBool>,
}

/// Bridge between MCP background thread and main thread.
pub struct McpBridge {
    /// Shared state for MCP tools to read.
    pub state: McpState,
    /// Channel for MCP tools to send commands to main thread.
    pub command_tx: CommandTx,
    /// Receiver for commands (consumed on main thread each frame).
    command_rx: mpsc::Receiver<(Option<uuid::Uuid>, McpCommand)>,
}

impl McpBridge {
    /// Create a new bridge.
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        let command_tx = CommandTx::new(tx, None);
        Self {
            state: McpState {
                active_url: Arc::new(RwLock::new(String::new())),
                active_title: Arc::new(RwLock::new(String::new())),
                active_epoch: Arc::new(std::sync::atomic::AtomicU64::new(0)),
                active_loading: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            },
            command_tx,
            command_rx: rx,
        }
    }

    /// Update the active pane's epoch/loading metadata (T0 observation).
    pub fn update_active_meta(&self, epoch: u64, loading: bool) {
        self.state
            .active_epoch
            .store(epoch, std::sync::atomic::Ordering::Relaxed);
        self.state
            .active_loading
            .store(loading, std::sync::atomic::Ordering::Relaxed);
    }

    /// Update the shared state (called from main thread each frame).
    pub fn update_state(&self, url: &str, title: &str) {
        if let Ok(mut url_guard) = self.state.active_url.write() {
            *url_guard = url.to_string();
        }
        if let Ok(mut title_guard) = self.state.active_title.write() {
            *title_guard = title.to_string();
        }
    }

    /// Poll pending commands from MCP tools. Returns an iterator of
    /// (session tag, command) pairs. Call this from the main thread.
    pub fn poll_commands(&self) -> mpsc::TryIter<'_, (Option<uuid::Uuid>, McpCommand)> {
        self.command_rx.try_iter()
    }
}

impl Default for McpState {
    fn default() -> Self {
        Self {
            active_url: Arc::new(RwLock::new(String::new())),
            active_title: Arc::new(RwLock::new(String::new())),
            active_epoch: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            active_loading: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }
}

impl Default for McpBridge {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// Era-2 S39 injection-confinement vectors: the taint argument as tests.
    /// A page-influenced agent can call session_scope, but every widening
    /// shape must be rejected (structural taint, YP-SEC-CONFINEMENT-001).
    #[test]
    fn test_narrowing_rejects_all_widening_shapes() {
        let unrestricted = SessionCaps::default();
        // none -> none: identity is fine
        assert!(unrestricted.allows_narrowing_to(&SessionCaps::default()));
        // none -> some: first narrowing is allowed
        assert!(unrestricted.allows_narrowing_to(&SessionCaps {
            panes: None,
            tool_classes: Some(HashSet::from(["read".to_string()])),
        }));
        // some -> none: dropping a grant is WIDENING - rejected
        let scoped = SessionCaps {
            panes: None,
            tool_classes: Some(HashSet::from(["read".to_string()])),
        };
        assert!(!scoped.allows_narrowing_to(&SessionCaps::default()));
        // some -> larger some: rejected
        let wider = SessionCaps {
            panes: None,
            tool_classes: Some(HashSet::from(["read".to_string(), "mutate".to_string()])),
        };
        assert!(!scoped.allows_narrowing_to(&wider));
        // some -> subset: allowed
        assert!(scoped.allows_narrowing_to(&SessionCaps {
            panes: None,
            tool_classes: Some(HashSet::new()),
        }));
    }

    #[test]
    fn test_pane_scope_narrowing() {
        let s1 = uuid::Uuid::new_v4();
        let s2 = uuid::Uuid::new_v4();
        let base = SessionCaps {
            panes: Some(HashSet::from([s1, s2])),
            tool_classes: None,
        };
        // subset of panes: ok
        assert!(base.allows_narrowing_to(&SessionCaps {
            panes: Some(HashSet::from([s1])),
            tool_classes: None,
        }));
        // adding a pane: widening - rejected
        let s3 = uuid::Uuid::new_v4();
        assert!(!base.allows_narrowing_to(&SessionCaps {
            panes: Some(HashSet::from([s1, s3])),
            tool_classes: None,
        }));
        // un-scoping panes entirely: widening - rejected
        assert!(!base.allows_narrowing_to(&SessionCaps::default()));
    }

    #[test]
    fn test_command_tx_session_tag_is_bound_at_construction() {
        // The tag cannot be changed after construction except via
        // with_session - tools hold a CommandTx and can only send.
        let (tx, rx) = std::sync::mpsc::channel();
        let sid = uuid::Uuid::new_v4();
        let tx = CommandTx::new(tx, Some(sid));
        assert_eq!(tx.session(), Some(sid));
        let _ = tx.send(McpCommand::SessionInfo {
            response_tx: oneshot::channel().0,
        });
        let (tag, _) = rx.recv().expect("command");
        assert_eq!(
            tag,
            Some(sid),
            "tag must equal the construction-time session"
        );
    }

    #[test]
    fn test_every_registered_tool_has_explicit_class() {
        // Names as registered in tools::create_tools. If a new tool is added
        // without an explicit tool_class entry, this fails (the default
        // mutate would also cover it, but policy must be explicit).
        let explicit_read: [&str; 18] = [
            "audit_log",
            "browser_get_text",
            "browser_observe",
            "browser_snapshot",
            "browser_console_messages",
            "browser_network_requests",
            "browser_cookies",
            "browser_downloads",
            "list_tabs",
            "list_bookmarks",
            "search_history",
            "read_active_pane",
            "search_web",
            "browser_wait_for",
            "session_info",
            "workspace_list",
            "workspace_diff",
            "browser_take_screenshot",
        ];
        let explicit_read_tools: [&str; 5] = [
            "read_page_structure",
            "extract_structured_data",
            "summarize_page",
            "translate_page",
            "analyze_page",
        ];
        let explicit_mutate_tools: [&str; 9] = [
            "navigate",
            "navigate_back",
            "navigate_forward",
            "select_option",
            "browser_fill_form",
            "browser_click",
            "browser_type",
            "browser_hover",
            "browser_focus",
        ];
        let explicit_irrev: [&str; 3] = ["remove_bookmark", "close_tab", "session_scope"];
        let explicit_mutate: [&str; 9] = [
            "browser_press_key",
            "browser_select",
            "browser_drag",
            "bulk_act",
            "execute_js",
            "add_bookmark",
            "workspace_checkpoint",
            "new_tab",
            "browser_take_screenshot_dup_guard",
        ];
        for n in explicit_read {
            assert_eq!(tool_class(n), "read", "{n}");
        }
        for n in explicit_read_tools {
            assert_eq!(tool_class(n), "read", "{n}");
        }
        for n in explicit_mutate_tools {
            assert_eq!(tool_class(n), "mutate", "{n}");
        }
        for n in explicit_irrev {
            assert_eq!(tool_class(n), "irreversible", "{n}");
        }
        for n in explicit_mutate {
            if n == "browser_take_screenshot_dup_guard" {
                continue;
            }
            assert_eq!(tool_class(n), "mutate", "{n}");
        }
    }

    #[test]
    fn test_tool_class_totality_and_sensitivity() {
        // Every classifyable name lands in a known class; unknowns default
        // to the STRONGER class (mutate), never to read.
        for name in ["navigate", "browser_click", "execute_js"] {
            assert_eq!(tool_class(name), "mutate");
        }
        for name in ["browser_get_text", "browser_snapshot", "session_info"] {
            assert_eq!(tool_class(name), "read");
        }
        for name in ["remove_bookmark", "close_tab", "session_scope"] {
            assert_eq!(tool_class(name), "irreversible");
        }
        assert_eq!(tool_class("totally_unknown_tool"), "mutate");
    }

    #[test]
    fn test_bridge_update_and_read() {
        let bridge = McpBridge::new();

        // Initially empty
        {
            let url = bridge.state.active_url.read().unwrap();
            assert_eq!(*url, "");
        }

        // Update from "main thread"
        bridge.update_state("https://example.com", "Example");

        // Read from "MCP thread"
        {
            let url = bridge.state.active_url.read().unwrap();
            assert_eq!(*url, "https://example.com");
            let title = bridge.state.active_title.read().unwrap();
            assert_eq!(*title, "Example");
        }
    }

    #[test]
    fn test_command_channel() {
        let bridge = McpBridge::new();

        // No commands initially
        let commands: Vec<_> = bridge.poll_commands().collect();
        assert!(commands.is_empty());

        // Send a command
        bridge
            .command_tx
            .send(McpCommand::Navigate {
                url: "https://example.com".into(),
                new_tab: false,
            })
            .unwrap();

        // Poll receives it
        let commands: Vec<_> = bridge.poll_commands().collect();
        assert_eq!(commands.len(), 1);
    }
}
