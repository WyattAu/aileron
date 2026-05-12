//! Frame tasks integration tests.
//!
//! End-to-end verification of per-frame utility functions:
//!   - handle_pending_tab_close: BSP tree + engine + terminal cleanup
//!   - process_pending_wry_actions: action queue drain
//!   - load_default_adblock_rules: blocking + cosmetic behavior
//!   - handle_pending_import: import pipeline with database
//!
//! These tests exercise cross-module interactions that unit tests cannot cover.

use aileron::app::{AppState, WryAction};
use aileron::config::Config;
use aileron::frame_tasks;
use aileron::net::adblock::AdBlocker;
use aileron::wm::{Rect, SplitDirection};

fn test_viewport() -> Rect {
    Rect::new(0.0, 0.0, 1920.0, 1080.0)
}

fn make_state() -> AppState {
    AppState::new(test_viewport(), Config::default()).unwrap()
}

// ─── 1. handle_pending_tab_close: BSP tree lifecycle ───────────────

#[test]
fn test_tab_close_removes_pane_from_tree() {
    let mut state = make_state();
    let root_id = state.wm.active_pane_id();
    let new_id = state
        .wm
        .split(root_id, SplitDirection::Vertical, 0.5)
        .unwrap();
    assert_eq!(state.wm.leaf_count(), 2);

    frame_tasks::handle_pending_tab_close(&mut state, new_id);

    assert_eq!(
        state.wm.leaf_count(),
        1,
        "closed pane should be removed from BSP tree"
    );
    assert!(
        state.wm.panes().iter().all(|(id, _)| *id != new_id),
        "closed pane ID should no longer exist in tree"
    );
}

#[test]
fn test_tab_close_cleans_up_terminal_pane_ids() {
    let mut state = make_state();
    let root_id = state.wm.active_pane_id();
    let term_id = state
        .wm
        .split(root_id, SplitDirection::Horizontal, 0.5)
        .unwrap();

    state.terminal_pane_ids.insert(term_id);
    assert!(state.terminal_pane_ids.contains(&term_id));

    frame_tasks::handle_pending_tab_close(&mut state, term_id);

    assert!(
        !state.terminal_pane_ids.contains(&term_id),
        "terminal_pane_ids should be cleaned up after close"
    );
}

#[test]
fn test_tab_close_cleans_up_engines() {
    let mut state = make_state();
    let root_id = state.wm.active_pane_id();
    let new_id = state
        .wm
        .split(root_id, SplitDirection::Vertical, 0.5)
        .unwrap();

    // Manually create engine for new pane (simulating what execute_action does)
    let url = url::Url::parse("aileron://new").unwrap();
    state.engines.create_pane(new_id, url, None);

    let ids_before = state.engines.pane_ids();
    assert!(
        ids_before.contains(&new_id),
        "engine should exist for new pane"
    );

    frame_tasks::handle_pending_tab_close(&mut state, new_id);

    let ids_after = state.engines.pane_ids();
    assert!(
        !ids_after.contains(&new_id),
        "engine should be removed after close"
    );
}

#[test]
fn test_tab_close_on_active_pane_falls_back_to_sibling() {
    let mut state = make_state();
    let root_id = state.wm.active_pane_id();
    let other_id = state
        .wm
        .split(root_id, SplitDirection::Vertical, 0.5)
        .unwrap();
    state.wm.set_active_pane(root_id);

    frame_tasks::handle_pending_tab_close(&mut state, root_id);

    assert_eq!(state.wm.leaf_count(), 1);
    // The remaining pane should be active
    assert_eq!(
        state.wm.active_pane_id(),
        other_id,
        "active pane should fall back to sibling after close"
    );
}

#[test]
fn test_tab_close_nonexistent_pane_does_not_panic() {
    let mut state = make_state();
    let fake_id = uuid::Uuid::new_v4();

    // Should not panic when closing a pane that doesn't exist.
    // Note: the BSP close may have side effects on tree structure
    // since it first sets the pane active, but no panic is the key assertion.
    frame_tasks::handle_pending_tab_close(&mut state, fake_id);
    // No panic = pass
}

#[test]
fn test_tab_close_last_pane_does_not_panic() {
    let mut state = make_state();
    let root_id = state.wm.active_pane_id();
    assert_eq!(state.wm.leaf_count(), 1);

    // Closing the last pane -- behavior depends on implementation
    // It should either succeed (leaving empty tree) or fail gracefully
    frame_tasks::handle_pending_tab_close(&mut state, root_id);
    // No panic = pass
}

#[test]
fn test_tab_close_with_multiple_splits() {
    let mut state = make_state();
    let root = state.wm.active_pane_id();

    // Create a 4-pane grid: split root, then split each child
    let a = state.wm.split(root, SplitDirection::Vertical, 0.5).unwrap();
    let b = state
        .wm
        .split(root, SplitDirection::Horizontal, 0.5)
        .unwrap();
    let c = state.wm.split(a, SplitDirection::Horizontal, 0.5).unwrap();

    assert_eq!(state.wm.leaf_count(), 4);

    // Add some as terminal panes
    state.terminal_pane_ids.insert(b);
    state.terminal_pane_ids.insert(c);

    // Close one terminal pane
    frame_tasks::handle_pending_tab_close(&mut state, b);
    assert_eq!(state.wm.leaf_count(), 3);
    assert!(!state.terminal_pane_ids.contains(&b));
    assert!(state.terminal_pane_ids.contains(&c));

    // Close another
    frame_tasks::handle_pending_tab_close(&mut state, c);
    assert_eq!(state.wm.leaf_count(), 2);
    assert!(state.terminal_pane_ids.is_empty());
}

// ─── 2. process_pending_wry_actions: drain verification ────────────

#[test]
fn test_process_pending_wry_actions_drains_queue() {
    let mut state = make_state();
    state.pending_wry_actions.push_back(WryAction::Back);
    state.pending_wry_actions.push_back(WryAction::Forward);
    state.pending_wry_actions.push_back(WryAction::Reload);
    state
        .pending_wry_actions
        .push_back(WryAction::SmoothScroll { x: 0.0, y: 100.0 });
    assert_eq!(state.pending_wry_actions.len(), 4);

    let mut app_state = Some(state);
    let mut wry_panes = aileron::servo::WryPaneManager::new();
    let mut offscreen_panes = aileron::offscreen_webview::OffscreenWebViewManager::new();
    let content_scripts = aileron::scripts::ContentScriptManager::new();

    frame_tasks::process_pending_wry_actions(
        &mut app_state,
        &mut wry_panes,
        &mut offscreen_panes,
        &content_scripts,
    );

    let state = app_state.unwrap();
    assert!(
        state.pending_wry_actions.is_empty(),
        "all pending actions should be drained after processing"
    );
}

#[test]
fn test_process_pending_wry_actions_with_none_state() {
    let mut app_state: Option<AppState> = None;
    let mut wry_panes = aileron::servo::WryPaneManager::new();
    let mut offscreen_panes = aileron::offscreen_webview::OffscreenWebViewManager::new();
    let content_scripts = aileron::scripts::ContentScriptManager::new();

    // Should not panic with None state
    frame_tasks::process_pending_wry_actions(
        &mut app_state,
        &mut wry_panes,
        &mut offscreen_panes,
        &content_scripts,
    );
}

#[test]
fn test_process_pending_wry_actions_empty_queue() {
    let state = make_state();
    assert!(state.pending_wry_actions.is_empty());

    let mut app_state = Some(state);
    let mut wry_panes = aileron::servo::WryPaneManager::new();
    let mut offscreen_panes = aileron::offscreen_webview::OffscreenWebViewManager::new();
    let content_scripts = aileron::scripts::ContentScriptManager::new();

    frame_tasks::process_pending_wry_actions(
        &mut app_state,
        &mut wry_panes,
        &mut offscreen_panes,
        &content_scripts,
    );

    // Should not panic
    let state = app_state.unwrap();
    assert!(state.pending_wry_actions.is_empty());
}

#[test]
fn test_process_pending_wry_actions_many_actions_no_panic() {
    let mut state = make_state();

    // Queue a wide variety of actions
    let actions = vec![
        WryAction::Back,
        WryAction::Forward,
        WryAction::Reload,
        WryAction::ToggleDevTools,
        WryAction::ScrollBy { x: 10.0, y: 20.0 },
        WryAction::SmoothScroll { x: 0.0, y: 120.0 },
        WryAction::ScrollTo { fraction: 0.5 },
        WryAction::RunJs("console.log('test')".into()),
        WryAction::EnterReaderMode,
        WryAction::ExitReaderMode,
    ];

    for action in actions {
        state.pending_wry_actions.push_back(action);
    }

    let mut app_state = Some(state);
    let mut wry_panes = aileron::servo::WryPaneManager::new();
    let mut offscreen_panes = aileron::offscreen_webview::OffscreenWebViewManager::new();
    let content_scripts = aileron::scripts::ContentScriptManager::new();

    frame_tasks::process_pending_wry_actions(
        &mut app_state,
        &mut wry_panes,
        &mut offscreen_panes,
        &content_scripts,
    );

    let state = app_state.unwrap();
    assert!(
        state.pending_wry_actions.is_empty(),
        "all actions should be drained even without real panes"
    );
}

// ─── 3. load_default_adblock_rules: cross-module ───────────────────

#[test]
fn test_default_adblock_rules_block_tracking_domains() {
    let mut adblocker = AdBlocker::new();
    frame_tasks::load_default_adblock_rules(&mut adblocker);

    let blocked_urls = [
        "https://doubleclick.net/track",
        "https://googlesyndication.com/ad",
        "https://googleadservices.com/pagead",
        "https://adnxs.com/bid",
        "https://adsrvr.org/ad",
        "https://amazon-adsystem.com/ad",
    ];

    for url_str in &blocked_urls {
        let url = url::Url::parse(url_str).unwrap();
        assert!(
            adblocker.should_block(&url, None, None),
            "default rules should block: {url_str}"
        );
    }
}

#[test]
fn test_default_adblock_rules_allow_normal_browsing() {
    let mut adblocker = AdBlocker::new();
    frame_tasks::load_default_adblock_rules(&mut adblocker);

    let safe_urls = [
        "https://github.com/user/repo",
        "https://en.wikipedia.org/wiki/Main_Page",
        "https://rust-lang.org/learn",
        "https://example.com/page",
        "https://docs.rs/serde",
        "https://news.ycombinator.com",
    ];

    for url_str in &safe_urls {
        let url = url::Url::parse(url_str).unwrap();
        assert!(
            !adblocker.should_block(&url, None, None),
            "normal browsing should be allowed: {url_str}"
        );
    }
}

#[test]
fn test_default_adblock_rules_do_not_block_subresources_of_safe_domains() {
    let mut adblocker = AdBlocker::new();
    frame_tasks::load_default_adblock_rules(&mut adblocker);

    let safe_subresources = [
        "https://github.com/assets/script.js",
        "https://en.wikipedia.org/static/images/logo.png",
        "https://rust-lang.org/css/style.css",
    ];

    for url_str in &safe_subresources {
        let url = url::Url::parse(url_str).unwrap();
        assert!(
            !adblocker.should_block(&url, None, None),
            "subresources of safe domains should be allowed: {url_str}"
        );
    }
}

#[test]
fn test_default_adblock_rules_with_https_safe_list() {
    let mut adblocker = AdBlocker::new();
    frame_tasks::load_default_adblock_rules(&mut adblocker);

    // Verify the blocker was actually loaded (has rules)
    // by checking that a known-blocked domain is blocked
    let url = url::Url::parse("https://doubleclick.net/track").unwrap();
    assert!(
        adblocker.should_block(&url, None, None),
        "after loading defaults, doubleclick.net should be blocked"
    );
}

// ─── 4. handle_pending_import ──────────────────────────────────────

#[test]
fn test_handle_pending_import_none_does_nothing() {
    let mut state = make_state();
    let original_message = state.ui.status_message.clone();
    assert!(state.pending_import.is_none());

    frame_tasks::handle_pending_import(&mut state);

    // Should not change status message when no import is pending
    assert_eq!(
        state.ui.status_message, original_message,
        "no import pending should not change status message"
    );
}

#[test]
fn test_handle_pending_import_unknown_source() {
    let mut state = make_state();
    state.pending_import = Some("safari".into());

    frame_tasks::handle_pending_import(&mut state);

    assert!(
        state.ui.status_message.contains("Unknown import source"),
        "unknown import source should set error status"
    );
    assert!(
        state.pending_import.is_none(),
        "pending_import should be consumed"
    );
}

#[test]
fn test_handle_pending_import_no_database() {
    let mut state = make_state();
    state.pending_import = Some("firefox".into());
    state.db = None;

    frame_tasks::handle_pending_import(&mut state);

    assert!(
        state.ui.status_message.contains("No database"),
        "import without database should set error status"
    );
    assert!(
        state.pending_import.is_none(),
        "pending_import should be consumed"
    );
}

// ─── 5. Cross-module: split + close + split again ──────────────────

#[test]
fn test_split_close_split_cycle() {
    let mut state = make_state();

    // Initial state: 1 pane
    assert_eq!(state.wm.leaf_count(), 1);
    let initial_id = state.wm.active_pane_id();

    // Split -> 2 panes
    let id_a = state
        .wm
        .split(initial_id, SplitDirection::Vertical, 0.5)
        .unwrap();
    assert_eq!(state.wm.leaf_count(), 2);

    // Close one -> 1 pane
    frame_tasks::handle_pending_tab_close(&mut state, id_a);
    assert_eq!(state.wm.leaf_count(), 1);

    // Split again -> 2 panes (verify tree is still usable)
    let remaining_id = state.wm.active_pane_id();
    let id_b = state
        .wm
        .split(remaining_id, SplitDirection::Horizontal, 0.5)
        .unwrap();
    assert_eq!(state.wm.leaf_count(), 2);

    // Clean up
    frame_tasks::handle_pending_tab_close(&mut state, id_b);
    assert_eq!(state.wm.leaf_count(), 1);
}

#[test]
fn test_repeated_split_and_close_stress() {
    let mut state = make_state();

    for i in 0..10 {
        let active = state.wm.active_pane_id();
        let new_id = state
            .wm
            .split(
                active,
                if i % 2 == 0 {
                    SplitDirection::Vertical
                } else {
                    SplitDirection::Horizontal
                },
                0.5,
            )
            .unwrap();

        frame_tasks::handle_pending_tab_close(&mut state, new_id);
    }

    // After 10 split+close cycles, should still have exactly 1 pane
    assert_eq!(
        state.wm.leaf_count(),
        1,
        "after 10 split+close cycles, should have 1 pane"
    );
}
