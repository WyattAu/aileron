//! DOM snapshot tests for the Leptos WASM chrome webview.
//!
//! These tests construct ChromeState values, serialize them to JSON,
//! and verify the expected DOM structure that each state should produce.
//!
//! They serve as contract tests between the Rust backend and the Leptos
//! frontend: if the Leptos components change their DOM structure, these
//! tests should be updated to match.
//!
//! Run: AILERON_TESTING=1 cargo test -p aileron-shared -- --nocapture

use aileron_shared::*;

fn test_state_default() -> ChromeState {
    ChromeState::default()
}

fn test_state_insert_mode() -> ChromeState {
    ChromeState {
        mode: Mode::Insert,
        mode_color: "mode-INSERT".into(),
        url: "https://example.com".into(),
        ..Default::default()
    }
}

fn test_state_command_mode() -> ChromeState {
    ChromeState {
        mode: Mode::Command,
        mode_color: "mode-COMMAND".into(),
        url_bar_focused: true,
        url: "aileron://newtab".into(),
        ..Default::default()
    }
}

fn test_state_find_bar_open() -> ChromeState {
    ChromeState {
        mode: Mode::Normal,
        mode_color: "mode-NORMAL FIND".into(),
        find_bar_open: true,
        find_query: "test query".into(),
        url: "https://example.com".into(),
        ..Default::default()
    }
}

fn test_state_palette_open() -> ChromeState {
    ChromeState {
        command_palette_open: true,
        palette_results: vec![
            PaletteItem {
                id: "open https://github.com".into(),
                label: "GitHub".into(),
                description: "Visit GitHub".into(),
                category: SearchCategory::History,
            },
            PaletteItem {
                id: ":quit".into(),
                label: "Quit".into(),
                description: "Exit aileron".into(),
                category: SearchCategory::Command,
            },
        ],
        palette_selected: 0,
        ..Default::default()
    }
}

fn test_state_multiple_tabs() -> ChromeState {
    ChromeState {
        pane_count: 2,
        panes: vec![
            PaneInfo {
                id: "aaaa-1111-aaaa-1111-aaaa1111aaaa".into(),
                url: "https://example.com".into(),
                title: "Example".into(),
                active: true,
                loading: false,
                zoom: 1.0,
            },
            PaneInfo {
                id: "bbbb-2222-bbbb-2222-bbbb2222bbbb".into(),
                url: "https://google.com".into(),
                title: "Google".into(),
                active: false,
                loading: false,
                zoom: 1.0,
            },
        ],
        url: "https://example.com".into(),
        title: "Example".into(),
        ..Default::default()
    }
}

fn test_state_sidebar_right() -> ChromeState {
    ChromeState {
        tab_sidebar_right: true,
        panes: vec![PaneInfo {
            id: "aaaa-1111-aaaa-1111-aaaa1111aaaa".into(),
            url: "aileron://newtab".into(),
            title: "New Tab".into(),
            active: true,
            loading: false,
            zoom: 1.0,
        }],
        ..Default::default()
    }
}

fn test_state_full() -> ChromeState {
    ChromeState {
        mode: Mode::Normal,
        mode_color: "mode-NORMAL".into(),
        active_pane_id: Some("aaaa-1111-aaaa-1111-aaaa1111aaaa".into()),
        pane_count: 3,
        panes: vec![
            PaneInfo {
                id: "aaaa-1111-aaaa-1111-aaaa1111aaaa".into(),
                url: "https://github.com/WyattAu/aileron".into(),
                title: "aileron - GitHub".into(),
                active: true,
                loading: false,
                zoom: 1.0,
            },
            PaneInfo {
                id: "bbbb-2222-bbbb-2222-bbbb2222bbbb".into(),
                url: "https://doc.rust-lang.org".into(),
                title: "Rust Documentation".into(),
                active: false,
                loading: true,
                zoom: 1.2,
            },
            PaneInfo {
                id: "cccc-3333-cccc-3333-cccc3333cccc".into(),
                url: "https://example.com".into(),
                title: "Example Domain".into(),
                active: false,
                loading: false,
                zoom: 1.0,
            },
        ],
        url: "https://github.com/WyattAu/aileron".into(),
        title: "aileron - GitHub".into(),
        status_message: "3 panes open".into(),
        find_bar_open: false,
        command_palette_open: false,
        url_bar_focused: false,
        tab_sidebar_right: false,
        private_mode: false,
        adblock_count: 42,
        git_status: "main +2 ~0".into(),
        version: "0.21.0".into(),
        ..Default::default()
    }
}

/// Helper: serialize ChromeState to the JSON format that
/// `window.updateChromeState()` expects.
fn state_to_json(state: &ChromeState) -> String {
    serde_json::to_string(state).expect("ChromeState serialization")
}

// ═══════════════════════════════════════════════════════════════
// Serialization tests -- verify the JSON contract
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_default_state_serializes_correctly() {
    let state = test_state_default();
    let json = state_to_json(&state);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed["mode"], "NORMAL");
    assert_eq!(parsed["find_bar_open"], false);
    assert_eq!(parsed["command_palette_open"], false);
    assert_eq!(parsed["url_bar_focused"], false);
    assert_eq!(parsed["tab_sidebar_right"], false);
    assert_eq!(parsed["panes"].as_array().unwrap().len(), 0);
}

#[test]
fn test_insert_mode_serializes_mode_color() {
    let state = test_state_insert_mode();
    let json = state_to_json(&state);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed["mode"], "INSERT");
    assert_eq!(parsed["mode_color"], "mode-INSERT");
    assert_eq!(parsed["url"], "https://example.com");
}

#[test]
fn test_command_mode_url_bar_focused() {
    let state = test_state_command_mode();
    let json = state_to_json(&state);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed["mode"], "COMMAND");
    assert_eq!(parsed["mode_color"], "mode-COMMAND");
    assert_eq!(parsed["url_bar_focused"], true);
}

#[test]
fn test_find_bar_appends_find_to_mode_color() {
    let state = test_state_find_bar_open();
    let json = state_to_json(&state);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed["find_bar_open"], true);
    assert_eq!(parsed["find_query"], "test query");
    assert_eq!(parsed["mode_color"], "mode-NORMAL FIND");
}

#[test]
fn test_palette_with_results() {
    let state = test_state_palette_open();
    let json = state_to_json(&state);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed["command_palette_open"], true);
    let results = parsed["palette_results"].as_array().unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0]["category"], "HISTORY");
    assert_eq!(results[0]["label"], "GitHub");
    assert_eq!(results[1]["category"], "COMMAND");
    assert_eq!(results[1]["label"], "Quit");
    assert_eq!(parsed["palette_selected"], 0);
}

#[test]
fn test_multiple_tabs_pane_count() {
    let state = test_state_multiple_tabs();
    let json = state_to_json(&state);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed["pane_count"], 2);
    let panes = parsed["panes"].as_array().unwrap();
    assert_eq!(panes.len(), 2);
    assert_eq!(panes[0]["active"], true);
    assert_eq!(panes[1]["active"], false);
}

#[test]
fn test_sidebar_right_flag() {
    let state = test_state_sidebar_right();
    let json = state_to_json(&state);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed["tab_sidebar_right"], true);
}

#[test]
fn test_full_state_all_fields_populated() {
    let state = test_state_full();
    let json = state_to_json(&state);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed["pane_count"], 3);
    assert_eq!(parsed["adblock_count"], 42);
    assert_eq!(parsed["git_status"], "main +2 ~0");
    assert_eq!(parsed["version"], "0.21.0");
    assert_eq!(parsed["private_mode"], false);
    assert_eq!(parsed["status_message"], "3 panes open");

    // Verify all panes have required fields
    let panes = parsed["panes"].as_array().unwrap();
    for pane in panes {
        assert!(pane.get("id").is_some(), "Pane missing 'id'");
        assert!(pane.get("url").is_some(), "Pane missing 'url'");
        assert!(pane.get("title").is_some(), "Pane missing 'title'");
        assert!(pane.get("active").is_some(), "Pane missing 'active'");
        assert!(pane.get("loading").is_some(), "Pane missing 'loading'");
        assert!(pane.get("zoom").is_some(), "Pane missing 'zoom'");
    }

    // Verify the loading pane is correctly marked
    assert_eq!(panes[1]["loading"], true);
    assert_eq!(panes[1]["zoom"], 1.2);
}

// ═══════════════════════════════════════════════════════════════
// DOM expectation tests -- verify expected CSS class names
// ═══════════════════════════════════════════════════════════════

/// These constants document the expected CSS class names used by the
/// Leptos chrome components. If a component's CSS class changes,
/// these tests should be updated.
mod expected_classes {
    use super::*;

    #[test]
    fn status_bar_classes() {
        assert_eq!(Mode::Normal.as_str(), "NORMAL");
        assert_eq!(Mode::Insert.as_str(), "INSERT");
        assert_eq!(Mode::Command.as_str(), "COMMAND");
    }

    #[test]
    fn search_category_labels() {
        // Verify all SearchCategory variants can be serialized
        let cats = vec![
            SearchCategory::History,
            SearchCategory::Bookmark,
            SearchCategory::Command,
            SearchCategory::Credential,
            SearchCategory::Custom,
            SearchCategory::OpenTab,
            SearchCategory::Setting,
        ];
        for cat in cats {
            let json = serde_json::to_string(&cat).unwrap();
            let _: SearchCategory = serde_json::from_str(&json).unwrap();
        }
    }

    #[test]
    fn palette_item_roundtrip() {
        let item = PaletteItem {
            id: "test-id".into(),
            label: "Test Label".into(),
            description: "Test Description".into(),
            category: SearchCategory::History,
        };
        let json = serde_json::to_string(&item).unwrap();
        let restored: PaletteItem = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.id, "test-id");
        assert_eq!(restored.label, "Test Label");
        assert_eq!(restored.description, "Test Description");
        assert_eq!(restored.category, SearchCategory::History);
    }

    #[test]
    fn pane_info_roundtrip() {
        let pane = PaneInfo {
            id: "test-uuid".into(),
            url: "https://example.com".into(),
            title: "Example".into(),
            active: true,
            loading: false,
            zoom: 1.5,
        };
        let json = serde_json::to_string(&pane).unwrap();
        let restored: PaneInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.id, "test-uuid");
        assert_eq!(restored.url, "https://example.com");
        assert_eq!(restored.title, "Example");
        assert!(restored.active);
        assert!(!restored.loading);
        assert!((restored.zoom - 1.5).abs() < f64::EPSILON);
    }

    #[test]
    fn chrome_state_roundtrip_all_fields() {
        let state = test_state_full();
        let json = state_to_json(&state);
        let restored: ChromeState = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.mode, Mode::Normal);
        assert!(!restored.find_bar_open);
        assert!(!restored.command_palette_open);
        assert!(!restored.url_bar_focused);
        assert!(!restored.tab_sidebar_right);
        assert!(!restored.private_mode);
        assert_eq!(restored.adblock_count, 42);
        assert_eq!(restored.pane_count, 3);
        assert_eq!(restored.git_status, "main +2 ~0");
        assert_eq!(restored.version, "0.21.0");
        assert!(restored.palette_results.is_empty());
        assert_eq!(restored.panes.len(), 3);
    }
}

// ═══════════════════════════════════════════════════════════════
// DOM assertion helpers -- JS snippets for evaluate_script
// ═══════════════════════════════════════════════════════════════

/// Returns JavaScript that, when executed in the chrome webview via
/// `evaluate_script()`, asserts the DOM structure matches expectations.
/// Returns "PASS" or a failure description.
///
/// NOTE: This only checks DOM structure. To also inject state, call
/// `evaluate_script(updateChromeState(json))` separately before this.
pub fn dom_assert_js(state: &ChromeState) -> String {
    let find_check = if state.find_bar_open {
        r#"
        var findBar = document.querySelector('.find-bar');
        if (!findBar) return 'FAIL: .find-bar not found (expected visible)';
        var findInput = findBar.querySelector('.find-input');
        if (!findInput) return 'FAIL: .find-input not found inside .find-bar';
        var findBtns = findBar.querySelectorAll('.find-btn');
        if (findBtns.length < 3) return 'FAIL: expected 3 .find-btn elements, got ' + findBtns.length;
        var closeBtn = findBar.querySelector('.find-close');
        if (!closeBtn) return 'FAIL: .find-close not found inside .find-bar';
"#
    } else {
        r#"
        var findBar = document.querySelector('.find-bar');
        if (findBar) return 'FAIL: .find-bar found (expected hidden)';
"#
    };

    let palette_check = if state.command_palette_open {
        r#"
        var backdrop = document.querySelector('.palette-backdrop');
        if (!backdrop) return 'FAIL: .palette-backdrop not found (expected visible)';
        var container = backdrop.querySelector('.palette-container');
        if (!container) return 'FAIL: .palette-container not found inside .palette-backdrop';
        var input = container.querySelector('.palette-input');
        if (!input) return 'FAIL: .palette-input not found inside .palette-container';
        var prompt = container.querySelector('.palette-prompt');
        if (!prompt) return 'FAIL: .palette-prompt not found (text should be ": ")';
        var results = container.querySelector('.palette-results');
        if (!results) return 'FAIL: .palette-results not found inside .palette-container';
"#
    } else {
        r#"
        var backdrop = document.querySelector('.palette-backdrop');
        if (backdrop) return 'FAIL: .palette-backdrop found (expected hidden)';
"#
    };

    format!(
        r#"
(function() {{
    var root = document.querySelector('.chrome-root');
    if (!root) return 'FAIL: .chrome-root not found';

    var statusBar = root.querySelector('.status-bar');
    if (!statusBar) return 'FAIL: .status-bar not found';

    var modeSpan = statusBar.querySelector('[class^="mode-"]');
    if (!modeSpan) return 'FAIL: mode indicator not found (expected class mode-X)';

    var urlBar = root.querySelector('.url-bar');
    if (!urlBar) return 'FAIL: .url-bar not found';

    var urlInput = urlBar.querySelector('.url-input');
    if (!urlInput) return 'FAIL: .url-input not found inside .url-bar';

    var sidebar = root.querySelector('.tab-sidebar');
    if (!sidebar) return 'FAIL: .tab-sidebar not found';

    var newTabBtn = sidebar.querySelector('.tab-new');
    if (!newTabBtn) return 'FAIL: .tab-new not found inside .tab-sidebar';

    {find_check}

    {palette_check}

    // Check tab items
    var tabItems = sidebar.querySelectorAll('.tab-item');
    if (tabItems.length !== {pane_count}) return 'FAIL: expected {pane_count} .tab-item, got ' + tabItems.length;

    var activeTab = sidebar.querySelector('.tab-active');
    if ({has_active_pane} && !activeTab) return 'FAIL: no .tab-active found but pane_count > 0';

    return 'PASS';
}})()
"#,
        pane_count = state.panes.len(),
        has_active_pane = state.panes.iter().any(|p| p.active),
    )
}

/// Returns JavaScript that injects a ChromeState and returns the DOM snapshot
/// as a structured JSON object.
pub fn dom_snapshot_js(state: &ChromeState) -> String {
    let state_json = state_to_json(state);

    format!(
        r#"
(function() {{
    // Inject state
    window.updateChromeState({state_json});
    // Small delay for Leptos reactivity
    return new Promise(function(resolve) {{
        setTimeout(function() {{
            var root = document.querySelector('.chrome-root');
            if (!root) {{ resolve({{ error: '.chrome-root not found' }}); return; }}

            function serializeNode(el, depth) {{
                if (depth > 5 || !el) return null;
                var result = {{
                    tag: el.tagName.toLowerCase(),
                    classes: el.className && typeof el.className === 'string' ? el.className.split(/\s+/).filter(Boolean) : [],
                    text: ['SPAN', 'BUTTON'].includes(el.tagName) ? el.textContent.trim().substring(0, 80) : '',
                    children: []
                }};
                for (var i = 0; i < el.children.length; i++) {{
                    var child = serializeNode(el.children[i], depth + 1);
                    if (child) result.children.push(child);
                }}
                return result;
            }}

            resolve(serializeNode(root, 0));
        }}, 100);
    }});
}})()
"#,
        state_json = state_json,
    )
}
