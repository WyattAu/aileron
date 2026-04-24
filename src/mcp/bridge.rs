//! Bridge between the MCP background thread and the main thread.
//!
//! Since wry::WebView is !Send + !Sync, MCP tools on the background thread
//! cannot directly interact with wry panes. This bridge provides:
//! - Shared read-only state (active URL, title) via Arc<RwLock>
//! - Command channel for mutations (navigate, run JS) processed on main thread

use std::sync::{mpsc, Arc, RwLock};

/// Commands that MCP tools can send to the main thread.
/// These are processed each frame in `about_to_wait`.
#[derive(Debug, Clone)]
pub enum McpCommand {
    /// Navigate the active wry pane to a URL.
    Navigate { url: String },
    /// Execute JavaScript in the active pane and return the result.
    ExecuteJs {
        code: String,
        response_tx: mpsc::Sender<String>,
    },
    /// Get the current URL and title of the active pane.
    GetActivePane {
        response_tx: mpsc::Sender<(String, String)>,
    },
    /// List all bookmarks from the database.
    ListBookmarks {
        response_tx: mpsc::Sender<String>,
    },
    /// Add a bookmark to the database.
    AddBookmark {
        url: String,
        title: String,
        folder: String,
        response_tx: mpsc::Sender<String>,
    },
    /// Remove a bookmark from the database.
    RemoveBookmark {
        url: String,
        response_tx: mpsc::Sender<String>,
    },
    /// Search browsing history.
    SearchHistory {
        query: String,
        limit: usize,
        response_tx: mpsc::Sender<String>,
    },
    /// List all open tabs with URLs and titles.
    ListTabs {
        response_tx: mpsc::Sender<String>,
    },
}

/// Shared state readable by MCP tools (updated each frame from main thread).
#[derive(Debug, Clone)]
pub struct McpState {
    /// URL of the active wry pane.
    pub active_url: Arc<RwLock<String>>,
    /// Title of the active wry pane.
    pub active_title: Arc<RwLock<String>>,
}

/// Bridge between MCP background thread and main thread.
pub struct McpBridge {
    /// Shared state for MCP tools to read.
    pub state: McpState,
    /// Channel for MCP tools to send commands to main thread.
    pub command_tx: mpsc::Sender<McpCommand>,
    /// Receiver for commands (consumed on main thread each frame).
    command_rx: mpsc::Receiver<McpCommand>,
}

impl McpBridge {
    /// Create a new bridge.
    pub fn new() -> Self {
        let (command_tx, command_rx) = mpsc::channel();
        Self {
            state: McpState {
                active_url: Arc::new(RwLock::new(String::new())),
                active_title: Arc::new(RwLock::new(String::new())),
            },
            command_tx,
            command_rx,
        }
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

    /// Poll pending commands from MCP tools. Returns an iterator.
    /// Call this from the main thread each frame.
    pub fn poll_commands(&self) -> mpsc::TryIter<'_, McpCommand> {
        self.command_rx.try_iter()
    }
}

impl Default for McpState {
    fn default() -> Self {
        Self {
            active_url: Arc::new(RwLock::new(String::new())),
            active_title: Arc::new(RwLock::new(String::new())),
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
            })
            .unwrap();

        // Poll receives it
        let commands: Vec<_> = bridge.poll_commands().collect();
        assert_eq!(commands.len(), 1);
    }
}
