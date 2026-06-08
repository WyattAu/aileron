//! Chrome bridge: manages the Leptos WASM webview for browser chrome (status bar,
//! URL bar, tab sidebar).
//!
//! The chrome webview is a transparent wry child webview embedded behind the
//! wgpu surface. In Phase 2b (dual rendering), it exists as a hidden overlay for
//! development and testing. In Phase 3, it replaces egui chrome rendering.

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use aileron_shared::{ChromeRequest, ChromeState, Mode, PaletteItem, PaneInfo, SearchCategory};
use wry::http::{Request, Response, header::HeaderValue};

/// Convert native `SearchCategory` to shared `SearchCategory` for IPC.
pub fn to_shared_category(cat: crate::ui::search::SearchCategory) -> SearchCategory {
    match cat {
        crate::ui::search::SearchCategory::History => SearchCategory::History,
        crate::ui::search::SearchCategory::Bookmark => SearchCategory::Bookmark,
        crate::ui::search::SearchCategory::Command => SearchCategory::Command,
        crate::ui::search::SearchCategory::Credential => SearchCategory::Credential,
        crate::ui::search::SearchCategory::Custom => SearchCategory::Custom,
        crate::ui::search::SearchCategory::OpenTab => SearchCategory::OpenTab,
        crate::ui::search::SearchCategory::Setting => SearchCategory::Setting,
    }
}

// ---------------------------------------------------------------------------
// Chrome asset protocol handler
// ---------------------------------------------------------------------------

/// Creates a wry protocol handler that serves files from the trunk build
/// output directory (`chrome/dist/`). Serves correct MIME types for
/// `.html`, `.js`, `.wasm`, `.css`, `.svg`.
pub fn chrome_asset_handler(
    dist_dir: PathBuf,
) -> impl Fn(&str, Request<Vec<u8>>) -> Response<Cow<'static, [u8]>> + 'static {
    move |_webview_id: &str, req: Request<Vec<u8>>| {
        let path = req.uri().path().trim_start_matches('/');
        let file_path = dist_dir.join(path);

        let (body, content_type) = match std::fs::read(&file_path) {
            Ok(data) => {
                let ct = match Path::new(path).extension().and_then(|e| e.to_str()) {
                    Some("html") => "text/html; charset=utf-8",
                    Some("js") => "application/javascript; charset=utf-8",
                    Some("wasm") => "application/wasm",
                    Some("css") => "text/css; charset=utf-8",
                    Some("svg") => "image/svg+xml",
                    _ => "application/octet-stream",
                };
                (data, ct)
            }
            Err(_) => {
                let msg = format!("404: {path} not found in chrome assets");
                tracing::warn!("{msg}");
                (msg.into_bytes(), "text/plain; charset=utf-8")
            }
        };

        Response::builder()
            .header("Content-Type", HeaderValue::from_static(content_type))
            .header(
                "Cross-Origin-Opener-Policy",
                HeaderValue::from_static("same-origin"),
            )
            .header(
                "Cross-Origin-Embedder-Policy",
                HeaderValue::from_static("require-corp"),
            )
            .body(Cow::Owned(body))
            .expect("valid http response builder")
    }
}

// ---------------------------------------------------------------------------
// Chrome state builder
// ---------------------------------------------------------------------------

/// Input data needed to build a `ChromeState` snapshot.
/// Extracted from `AileronApp` fields to avoid coupling to the private struct.
pub struct ChromeSnapshotInput<'a> {
    pub mode: crate::input::Mode,
    pub active_pane_id: uuid::Uuid,
    pub pane_count: usize,
    pub panes: Vec<PaneInfo>,
    pub status_message: &'a str,
    pub find_bar_open: bool,
    pub find_query: &'a str,
    pub command_palette_open: bool,
    pub palette_results: Vec<PaletteItem>,
    pub palette_selected: usize,
    pub url_bar_focused: bool,
    pub tab_layout: &'a str,
    pub tab_sidebar_width: f64,
    pub tab_sidebar_right: bool,
    pub version: String,
}

/// Builds a `ChromeState` from extracted application data.
pub fn build_chrome_state(input: ChromeSnapshotInput<'_>) -> ChromeState {
    let mode_color = match input.mode {
        crate::input::Mode::Normal => "mode-NORMAL".to_string(),
        crate::input::Mode::Insert => "mode-INSERT".to_string(),
        crate::input::Mode::Command => "mode-COMMAND".to_string(),
    };

    let active_pane = input.panes.iter().find(|p| p.active);
    let url = active_pane.map(|p| p.url.clone()).unwrap_or_default();
    let title = active_pane.map(|p| p.title.clone()).unwrap_or_default();

    ChromeState {
        mode: match input.mode {
            crate::input::Mode::Normal => Mode::Normal,
            crate::input::Mode::Insert => Mode::Insert,
            crate::input::Mode::Command => Mode::Command,
        },
        active_pane_id: Some(input.active_pane_id.to_string()),
        pane_count: input.pane_count,
        panes: input.panes,
        url,
        title,
        status_message: input.status_message.to_string(),
        find_bar_open: input.find_bar_open,
        find_query: input.find_query.to_string(),
        command_palette_open: input.command_palette_open,
        palette_results: input.palette_results,
        palette_selected: input.palette_selected,
        url_bar_focused: input.url_bar_focused,
        tab_layout: input.tab_layout.to_string(),
        tab_sidebar_width: input.tab_sidebar_width,
        tab_sidebar_right: input.tab_sidebar_right,
        private_mode: false,
        adblock_count: 0,
        git_status: String::new(),
        mode_color,
        version: input.version,
    }
}

// ---------------------------------------------------------------------------
// IPC handler
// ---------------------------------------------------------------------------

/// Handles IPC messages from the Leptos chrome webview.
/// Returns a `ChromeCommand` that the caller should execute.
#[derive(Debug)]
pub enum ChromeCommand {
    /// Navigate the active pane to a URL.
    Navigate(url::Url),
    /// Log a status message (for debugging chrome actions).
    StatusMessage(String),
    /// Execute an action by name (mapped from aileron_shared::Action).
    Action(String),
    /// Find-in-page: submit a query and search forward.
    FindSubmit(String),
    /// Find-in-page: go to next match.
    FindNext,
    /// Find-in-page: go to previous match.
    FindPrev,
    /// Close find bar and clear highlights.
    FindClose,
    /// Command palette: text input for filtering.
    PaletteInput(String),
    /// Command palette: select the currently highlighted item.
    PaletteSelect,
    /// Command palette: close without selection.
    PaletteClose,
    /// No action needed.
    None,
}

/// Parse an IPC message from the Leptos chrome and produce a command.
pub fn parse_chrome_ipc(json_str: &str) -> ChromeCommand {
    let request = match serde_json::from_str::<ChromeRequest>(json_str) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("Invalid chrome IPC message: {e}");
            return ChromeCommand::None;
        }
    };

    match request.kind.as_str() {
        "action" => {
            let action_name = request
                .payload
                .get("action")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            ChromeCommand::Action(action_name.to_string())
        }
        "navigate" => {
            let url_str = request
                .payload
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            match url::Url::parse(url_str) {
                Ok(url) => ChromeCommand::Navigate(url),
                Err(e) => {
                    tracing::warn!("Chrome navigate with invalid URL: {e}");
                    ChromeCommand::None
                }
            }
        }
        "find" => {
            let sub = request
                .payload
                .get("sub")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            match sub {
                "submit" => {
                    let query = request
                        .payload
                        .get("query")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    ChromeCommand::FindSubmit(query)
                }
                "next" => ChromeCommand::FindNext,
                "prev" => ChromeCommand::FindPrev,
                "close" => ChromeCommand::FindClose,
                _ => {
                    tracing::warn!("Unknown find sub-command: {sub}");
                    ChromeCommand::None
                }
            }
        }
        "palette" => {
            let sub = request
                .payload
                .get("sub")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            match sub {
                "input" => {
                    let query = request
                        .payload
                        .get("query")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    ChromeCommand::PaletteInput(query)
                }
                "select" => ChromeCommand::PaletteSelect,
                "close" => ChromeCommand::PaletteClose,
                _ => {
                    tracing::warn!("Unknown palette sub-command: {sub}");
                    ChromeCommand::None
                }
            }
        }
        _ => {
            tracing::warn!("Unknown chrome IPC kind: {}", request.kind);
            ChromeCommand::None
        }
    }
}

// ---------------------------------------------------------------------------
// Dist directory locator
// ---------------------------------------------------------------------------

/// Finds the chrome dist directory. Checks:
/// 1. `../chrome/dist/` relative to CARGO_MANIFEST_DIR (workspace layout: `src/`)
/// 2. `chrome/dist/` relative to CARGO_MANIFEST_DIR
pub fn find_chrome_dist_dir() -> PathBuf {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));

    let candidates = [
        manifest_dir.join("../chrome/dist"),
        manifest_dir.join("chrome/dist"),
    ];

    for dir in &candidates {
        if dir.join("index.html").exists() {
            return dir.clone();
        }
    }

    candidates.into_iter().next().unwrap_or(manifest_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_chrome_dist_dir() {
        let dir = find_chrome_dist_dir();
        let _ = dir.to_string_lossy();
    }

    #[test]
    fn test_parse_chrome_ipc_navigate() {
        let json = r#"{"kind":"navigate","payload":{"url":"https://example.com"}}"#;
        match parse_chrome_ipc(json) {
            ChromeCommand::Navigate(url) => assert_eq!(url.as_str(), "https://example.com/"),
            other => panic!("Expected Navigate, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_chrome_ipc_action() {
        let json = r#"{"kind":"action","payload":{"action":"scroll_down"}}"#;
        match parse_chrome_ipc(json) {
            ChromeCommand::Action(name) => {
                assert_eq!(name, "scroll_down");
            }
            other => panic!("Expected Action, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_chrome_ipc_invalid() {
        let json = r#"{"kind":"unknown","payload":{}}"#;
        assert!(matches!(parse_chrome_ipc(json), ChromeCommand::None));
    }

    #[test]
    fn test_parse_chrome_ipc_malformed() {
        let json = r#"not json"#;
        assert!(matches!(parse_chrome_ipc(json), ChromeCommand::None));
    }

    #[test]
    fn test_parse_chrome_ipc_find_submit() {
        let json = r#"{"kind":"find","payload":{"sub":"submit","query":"hello"}}"#;
        match parse_chrome_ipc(json) {
            ChromeCommand::FindSubmit(q) => assert_eq!(q, "hello"),
            other => panic!("Expected FindSubmit, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_chrome_ipc_find_next() {
        let json = r#"{"kind":"find","payload":{"sub":"next"}}"#;
        assert!(matches!(parse_chrome_ipc(json), ChromeCommand::FindNext));
    }

    #[test]
    fn test_parse_chrome_ipc_find_prev() {
        let json = r#"{"kind":"find","payload":{"sub":"prev"}}"#;
        assert!(matches!(parse_chrome_ipc(json), ChromeCommand::FindPrev));
    }

    #[test]
    fn test_parse_chrome_ipc_find_close() {
        let json = r#"{"kind":"find","payload":{"sub":"close"}}"#;
        assert!(matches!(parse_chrome_ipc(json), ChromeCommand::FindClose));
    }

    #[test]
    fn test_parse_chrome_ipc_palette_input() {
        let json = r#"{"kind":"palette","payload":{"sub":"input","query":"git"}}"#;
        match parse_chrome_ipc(json) {
            ChromeCommand::PaletteInput(q) => assert_eq!(q, "git"),
            other => panic!("Expected PaletteInput, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_chrome_ipc_palette_select() {
        let json = r#"{"kind":"palette","payload":{"sub":"select"}}"#;
        assert!(matches!(
            parse_chrome_ipc(json),
            ChromeCommand::PaletteSelect
        ));
    }

    #[test]
    fn test_parse_chrome_ipc_palette_close() {
        let json = r#"{"kind":"palette","payload":{"sub":"close"}}"#;
        assert!(matches!(
            parse_chrome_ipc(json),
            ChromeCommand::PaletteClose
        ));
    }

    #[test]
    fn test_build_chrome_state() {
        let input = ChromeSnapshotInput {
            mode: crate::input::Mode::Normal,
            active_pane_id: uuid::Uuid::new_v4(),
            pane_count: 1,
            panes: vec![PaneInfo {
                id: "test".to_string(),
                url: "https://example.com".to_string(),
                title: "Example".to_string(),
                active: true,
                loading: false,
                zoom: 1.0,
            }],
            status_message: "test",
            find_bar_open: false,
            find_query: "",
            command_palette_open: false,
            palette_results: vec![],
            palette_selected: 0,
            url_bar_focused: false,
            tab_layout: "sidebar",
            tab_sidebar_width: 180.0,
            tab_sidebar_right: false,
            version: "v0.20.0 (abc1234)".to_string(),
        };
        let state = build_chrome_state(input);
        assert_eq!(state.mode, Mode::Normal);
        assert_eq!(state.pane_count, 1);
        assert_eq!(state.url, "https://example.com");
        assert_eq!(state.mode_color, "mode-NORMAL");
    }
}
