//! Shared types between Aileron native backend and Leptos WASM chrome.
//!
//! This crate compiles into both the native target and wasm32-unknown-unknown
//! to provide type-safe IPC between the Rust backend and the Leptos frontend.

use serde::{Deserialize, Serialize};

/// Input mode (Normal, Insert, Command) — the core of the modal editing system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Mode {
    #[default]
    Normal,
    Insert,
    Command,
}

impl Mode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Mode::Normal => "NORMAL",
            Mode::Insert => "INSERT",
            Mode::Command => "COMMAND",
        }
    }
}

/// Modifier state for key events.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub super_key: bool,
}

/// Key identifier for routing.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Key {
    Character(char),
    Escape,
    CtrlBracket,
    Enter,
    Backspace,
    Tab,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    F(u8),
    Unknown,
}

/// Key event with modifier state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyEvent {
    pub key: Key,
    pub modifiers: Modifiers,
}

/// Actions that can be bound to key combinations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    Quit,
    ScrollUp,
    ScrollDown,
    ScrollLeft,
    ScrollRight,
    HalfPageDown,
    HalfPageUp,
    ScrollTop,
    ScrollBottom,
    SplitHorizontal,
    SplitVertical,
    ClosePane,
    NavigateUp,
    NavigateDown,
    NavigateLeft,
    NavigateRight,
    NavigateBack,
    NavigateForward,
    Reload,
    BookmarkToggle,
    OpenCommandPalette,
    OpenExternalBrowser,
    EnterInsertMode,
    ToggleDevTools,
    NewTab,
    Yank,
    Paste,
    CopyUrl,
    Find,
    FindNext,
    FindPrev,
    FindClose,
    ToggleLinkHints,
    FollowLinkNewTab,
    SaveWorkspace,
    OpenTerminal,
    NewWindow,
    Custom(String),
    ZoomIn,
    ZoomOut,
    ZoomReset,
    ResizePane(u8), // Direction enum would add circular dep; use u8 for now
    SetMark(char),
    GoToMark(char),
    ToggleReaderMode,
    ToggleMinimalMode,
    ToggleNetworkLog,
    ToggleConsoleLog,
    DetachPane,
    CloseOtherPanes,
    Print,
    PinPane,
    NewTabInPane,
    CloseTab,
    NextTab,
    PrevTab,
}

/// Information about a pane, sent from Rust backend to Leptos chrome.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaneInfo {
    pub id: String, // UUID as string for wasm compatibility
    pub url: String,
    pub title: String,
    pub active: bool,
    pub loading: bool,
    pub zoom: f64,
}

/// Summary state sent from Rust backend to Leptos chrome (debounced).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChromeState {
    pub mode: Mode,
    pub active_pane_id: Option<String>,
    pub panes: Vec<PaneInfo>,
    pub pane_count: usize,
    pub url: String,
    pub title: String,
    pub status_message: String,
    pub find_bar_open: bool,
    pub find_query: String,
    pub command_palette_open: bool,
    pub palette_results: Vec<PaletteItem>,
    pub palette_selected: usize,
    pub url_bar_focused: bool,
    pub tab_layout: String, // "sidebar" or "topbar"
    pub tab_sidebar_width: f64,
    pub tab_sidebar_right: bool,
    pub private_mode: bool,
    pub adblock_count: usize,
    pub git_status: String,
    pub mode_color: String, // CSS class for mode indicator
    pub version: String,
}

/// IPC message from Leptos chrome to Rust backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChromeRequest {
    pub kind: String, // "action", "navigate", "find", "palette", "query", "config"
    pub payload: serde_json::Value,
}

/// Category of a palette/search item, for display and routing.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum SearchCategory {
    History,
    Bookmark,
    Command,
    Credential,
    Custom,
    OpenTab,
    Setting,
}

/// A single item in the command palette search results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaletteItem {
    pub id: String,
    pub label: String,
    pub description: String,
    pub category: SearchCategory,
}
