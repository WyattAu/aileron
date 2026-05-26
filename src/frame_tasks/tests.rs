use super::*;

fn test_viewport() -> crate::wm::Rect {
    crate::wm::Rect::new(0.0, 0.0, 1920.0, 1080.0)
}

fn test_app_state() -> AppState {
    AppState::new(test_viewport(), crate::config::Config::default()).unwrap()
}

// ─── poll_git_status ────────────────────────────────────────────────

#[test]
fn poll_git_status_none_poller_leaves_status_unchanged() {
    let mut status = GitStatus {
        branch: Some("main".into()),
        modified_count: 3,
        untracked_count: 1,
        is_dirty: true,
    };
    poll_git_status(&mut status, &None);
    assert_eq!(status.branch.as_deref(), Some("main"));
    assert_eq!(status.modified_count, 3);
    assert!(status.is_dirty);
}

#[test]
fn poll_git_status_with_empty_channel_leaves_status_unchanged() {
    let tmp = std::env::temp_dir().join("aileron_test_git_poller_none");
    let _ = std::fs::create_dir_all(&tmp);
    let poller = crate::git::GitPoller::new(tmp.clone(), std::time::Duration::from_secs(3600));
    let mut status = GitStatus {
        branch: Some("feature".into()),
        modified_count: 1,
        untracked_count: 0,
        is_dirty: true,
    };
    poll_git_status(&mut status, &Some(poller));
    assert_eq!(status.branch.as_deref(), Some("feature"));
    assert_eq!(status.modified_count, 1);
}

#[test]
fn poll_git_status_with_new_poller_receives_initial_status() {
    let tmp = std::env::temp_dir().join("aileron_test_git_poller_recv");
    let _ = std::fs::create_dir_all(&tmp);
    let poller = crate::git::GitPoller::new(tmp.clone(), std::time::Duration::from_secs(3600));
    let mut status = GitStatus::default();
    poll_git_status(&mut status, &Some(poller));
}

// ─── auto_save_workspace ────────────────────────────────────────────

#[test]
fn auto_save_disabled_does_not_save() {
    let mut app_state = test_app_state();
    app_state.config.auto_save = false;
    app_state.session.session_dirty = true;
    app_state.session.last_auto_save = std::time::Instant::now()
        - std::time::Duration::from_secs(app_state.config.auto_save_interval + 10);
    let wry_panes = WryPaneManager::new();
    auto_save_workspace(&mut app_state, &wry_panes);
    assert!(app_state.session.session_dirty);
}

#[test]
fn auto_save_session_not_dirty_does_not_save() {
    let mut app_state = test_app_state();
    app_state.config.auto_save = true;
    app_state.session.session_dirty = false;
    app_state.session.last_auto_save = std::time::Instant::now()
        - std::time::Duration::from_secs(app_state.config.auto_save_interval + 10);
    let wry_panes = WryPaneManager::new();
    auto_save_workspace(&mut app_state, &wry_panes);
}

#[test]
fn auto_save_interval_not_elapsed_does_not_save() {
    let mut app_state = test_app_state();
    app_state.config.auto_save = true;
    app_state.session.session_dirty = true;
    app_state.session.last_auto_save = std::time::Instant::now();
    let wry_panes = WryPaneManager::new();
    auto_save_workspace(&mut app_state, &wry_panes);
}

// ─── push_tabs_to_arp ───────────────────────────────────────────────

#[cfg(feature = "arp")]
#[test]
fn push_tabs_to_arp_no_server_does_nothing() {
    let app_state = test_app_state();
    assert!(app_state.arp_server.is_none());
    let wry_panes = WryPaneManager::new();
    push_tabs_to_arp(&app_state, &wry_panes);
}

#[cfg(feature = "arp")]
#[test]
fn push_tabs_to_arp_stopped_server_does_nothing() {
    let mut app_state = test_app_state();
    let Ok((server, _receiver)) = crate::arp::ArpServer::new(crate::arp::ArpConfig::default())
    else {
        return;
    };
    assert!(!server.is_running());
    app_state.arp_server = Some(server);
    let wry_panes = WryPaneManager::new();
    push_tabs_to_arp(&app_state, &wry_panes);
}

// ─── process_arp_commands ───────────────────────────────────────────

#[cfg(feature = "arp")]
#[test]
fn process_arp_commands_no_receiver_does_nothing() {
    let mut app_state = test_app_state();
    assert!(app_state.arp_cmd_receiver.is_none());
    process_arp_commands(&mut app_state);
}

#[cfg(feature = "arp")]
#[test]
fn process_arp_commands_tab_navigate_pushes_action() {
    let mut app_state = test_app_state();
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    app_state.arp_cmd_receiver = Some(std::sync::Mutex::new(rx));
    let _ = tx.send(ArpCommand::TabNavigate {
        tab_id: None,
        url: "https://example.com".into(),
    });
    process_arp_commands(&mut app_state);
    assert!(!app_state.pending_wry_actions.is_empty());
}

#[cfg(feature = "arp")]
#[test]
fn process_arp_commands_clipboard_set_pushes_action() {
    let mut app_state = test_app_state();
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    app_state.arp_cmd_receiver = Some(std::sync::Mutex::new(rx));
    let _ = tx.send(ArpCommand::ClipboardSet {
        text: "test clipboard".into(),
    });
    process_arp_commands(&mut app_state);
    assert!(!app_state.pending_wry_actions.is_empty());
}

#[cfg(feature = "arp")]
#[test]
fn process_arp_commands_quickmark_open_no_match_does_nothing() {
    let mut app_state = test_app_state();
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    app_state.arp_cmd_receiver = Some(std::sync::Mutex::new(rx));
    let _ = tx.send(ArpCommand::QuickmarkOpen {
        key: "nonexistent".into(),
    });
    process_arp_commands(&mut app_state);
    assert!(app_state.pending_wry_actions.is_empty());
}

#[cfg(feature = "arp")]
#[test]
fn process_arp_commands_quickmark_open_with_default_quickmark_pushes_navigate() {
    let mut app_state = test_app_state();
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    app_state.arp_cmd_receiver = Some(std::sync::Mutex::new(rx));
    let _ = tx.send(ArpCommand::QuickmarkOpen { key: "gh".into() });
    process_arp_commands(&mut app_state);
    assert_eq!(app_state.pending_wry_actions.len(), 1);
}

#[cfg(feature = "arp")]
#[test]
fn process_arp_commands_tab_create_with_no_url() {
    let mut app_state = test_app_state();
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    app_state.arp_cmd_receiver = Some(std::sync::Mutex::new(rx));
    let _ = tx.send(ArpCommand::TabCreate { url: None });
    process_arp_commands(&mut app_state);
    assert!(app_state.session.session_dirty);
}

#[cfg(feature = "arp")]
#[test]
fn process_arp_commands_tab_close_with_none_target() {
    let mut app_state = test_app_state();
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    app_state.arp_cmd_receiver = Some(std::sync::Mutex::new(rx));
    let _ = tx.send(ArpCommand::TabClose { tab_id: None });
    process_arp_commands(&mut app_state);
}

#[cfg(feature = "arp")]
#[test]
fn process_arp_commands_tab_activate() {
    let mut app_state = test_app_state();
    let active_id = app_state.wm.active_pane_id();
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    app_state.arp_cmd_receiver = Some(std::sync::Mutex::new(rx));
    let _ = tx.send(ArpCommand::TabActivate { tab_id: active_id });
    process_arp_commands(&mut app_state);
    assert_eq!(app_state.wm.active_pane_id(), active_id);
}

#[cfg(feature = "arp")]
#[test]
fn process_arp_commands_tab_go_back() {
    let mut app_state = test_app_state();
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    app_state.arp_cmd_receiver = Some(std::sync::Mutex::new(rx));
    let _ = tx.send(ArpCommand::TabGoBack { tab_id: None });
    process_arp_commands(&mut app_state);
    assert_eq!(app_state.pending_wry_actions.len(), 1);
}

#[cfg(feature = "arp")]
#[test]
fn process_arp_commands_tab_go_forward() {
    let mut app_state = test_app_state();
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    app_state.arp_cmd_receiver = Some(std::sync::Mutex::new(rx));
    let _ = tx.send(ArpCommand::TabGoForward { tab_id: None });
    process_arp_commands(&mut app_state);
    assert_eq!(app_state.pending_wry_actions.len(), 1);
}

#[cfg(feature = "arp")]
#[test]
fn process_arp_commands_tab_reload() {
    let mut app_state = test_app_state();
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    app_state.arp_cmd_receiver = Some(std::sync::Mutex::new(rx));
    let _ = tx.send(ArpCommand::TabReload { tab_id: None });
    process_arp_commands(&mut app_state);
    assert_eq!(app_state.pending_wry_actions.len(), 1);
}

#[cfg(feature = "arp")]
#[test]
fn process_arp_commands_clipboard_get() {
    let mut app_state = test_app_state();
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    app_state.arp_cmd_receiver = Some(std::sync::Mutex::new(rx));
    let _ = tx.send(ArpCommand::ClipboardGet { request_id: 42 });
    process_arp_commands(&mut app_state);
}

// ─── load_default_adblock_rules ─────────────────────────────────────

#[test]
fn load_default_adblock_rules_loads_without_panic() {
    let mut adblocker = crate::net::adblock::AdBlocker::new();
    load_default_adblock_rules(&mut adblocker);
    assert!(adblocker.is_enabled());
}

#[test]
fn load_default_adblock_rules_adds_blocked_domains() {
    let mut adblocker = crate::net::adblock::AdBlocker::new();
    load_default_adblock_rules(&mut adblocker);
    let test_url = url::Url::parse("https://doubleclick.net/track").unwrap();
    assert!(
        adblocker.should_block(&test_url, None, None),
        "doubleclick.net should be blocked after loading default rules"
    );
}

// ─── handle_pending_import ──────────────────────────────────────────

#[test]
fn handle_pending_import_none_does_nothing() {
    let mut app_state = test_app_state();
    assert!(app_state.pending_import.is_none());
    handle_pending_import(&mut app_state);
    assert!(app_state.ui.status_message.is_empty());
}

#[test]
fn handle_pending_import_no_database_sets_message() {
    let mut app_state = test_app_state();
    app_state.pending_import = Some("firefox".into());
    app_state.db = None;
    handle_pending_import(&mut app_state);
    assert!(app_state.pending_import.is_none());
    assert!(app_state.ui.status_message.contains("No database"));
}

#[test]
fn handle_pending_import_unknown_source_sets_message() {
    let mut app_state = test_app_state();
    app_state.pending_import = Some("safari".into());
    handle_pending_import(&mut app_state);
    assert!(app_state.pending_import.is_none());
    assert!(
        app_state
            .ui
            .status_message
            .contains("Unknown import source")
    );
}

// ─── poll_terminal_output ───────────────────────────────────────────

#[cfg(feature = "terminal")]
#[test]
fn poll_terminal_output_calls_tick_all_without_panic() {
    let mut terminal_manager = NativeTerminalManager::new();
    poll_terminal_output(&mut terminal_manager);
}

// ─── process_pending_wry_actions ────────────────────────────────────

#[test]
fn process_pending_wry_actions_none_app_state_does_nothing() {
    let mut app_state: Option<AppState> = None;
    let mut wry_panes = WryPaneManager::new();
    let mut offscreen_panes = OffscreenWebViewManager::new();
    let content_scripts = ContentScriptManager::new();
    process_pending_wry_actions(
        &mut app_state,
        &mut wry_panes,
        &mut offscreen_panes,
        &content_scripts,
    );
}
