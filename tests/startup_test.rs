//! Headless startup test — verifies all subsystems initialize without panicking.
//!
//! This test cannot create a window (no display), but it verifies that
//! AppState, Config, DB, and other core subsystems can be constructed
//! and used without errors.

use aileron::app::AppState;
use aileron::config::Config;
use aileron::git::GitStatus;
use aileron::input::KeybindingRegistry;
use aileron::net::AdBlocker;
use aileron::popup::PopupManager;
use aileron::scripts::ContentScriptManager;
use aileron::servo::PaneStateManager;
use aileron::terminal::NativeTerminalManager;
use aileron::ui::CommandPalette;
use aileron::ui::{FuzzySearch, SearchCategory, SearchItem};
use aileron::wm::BspTree;
use aileron::wm::Rect;

#[test]
fn test_app_state_creates_with_defaults() {
    let config = Config::default();
    let viewport = Rect::new(0.0, 0.0, 1280.0, 800.0);
    let state = AppState::new(viewport, config).unwrap();
    assert!(state.wm.active_pane_id() != uuid::Uuid::nil());
}

#[test]
fn test_config_loads_without_panic() {
    let config = Config::load();
    assert!(!config.homepage.is_empty());
    assert!(config.adblock_enabled);
    assert!(!config.restore_session);
    assert!(config.auto_save);
}

#[test]
fn test_pane_state_manager_operations() {
    let mut mgr = PaneStateManager::new();
    let id = uuid::Uuid::new_v4();
    let url = url::Url::parse("https://example.com").unwrap();
    mgr.create_pane(id, url.clone(), None);
    assert!(mgr.get(&id).is_some());
    assert!(mgr.get(&id).unwrap().current_url().is_some());
    mgr.remove_pane(&id);
    assert!(mgr.get(&id).is_none());
}

#[test]
fn test_bsp_tree_roundtrip() {
    let viewport = Rect::new(0.0, 0.0, 1280.0, 800.0);
    let initial_url = url::Url::parse("https://example.com").unwrap();
    let mut tree = BspTree::new(viewport, initial_url);
    let active = tree.active_pane_id();
    let pane = tree
        .split(active, aileron::wm::SplitDirection::Vertical, 0.5)
        .unwrap();
    assert!(tree.get_rect(pane).is_some());
}

#[test]
fn test_terminal_manager_lifecycle() {
    let mut mgr = NativeTerminalManager::new();
    let id = uuid::Uuid::new_v4();
    let size = mgr.create_terminal(id, 80, 24).unwrap();
    assert_eq!(size.0, 80);
    assert_eq!(size.1, 24);
    assert!(mgr.is_terminal(&id));
    mgr.remove(&id);
    assert!(!mgr.is_terminal(&id));
}

#[test]
fn test_keybinding_registry_all_defaults() {
    let registry = KeybindingRegistry::default();
    let count = registry.len();
    assert!(
        count > 20,
        "Expected at least 20 keybindings, got {}",
        count
    );
}

#[test]
fn test_command_palette() {
    let mut palette = CommandPalette::new();
    palette.open();
    assert!(palette.open);
    palette.update_query("test");
    palette.close();
    assert!(!palette.open);
}

#[test]
fn test_fuzzy_search() {
    let mut engine = FuzzySearch::new();
    engine.upsert(SearchItem {
        id: "hello world".into(),
        label: "hello world".into(),
        description: "hello world desc".into(),
        category: SearchCategory::Bookmark,
    });
    let results = engine.search("hel", 10);
    assert!(!results.is_empty());
}

#[test]
fn test_adblocker_default() {
    let _blocker = AdBlocker::new();
    let url = url::Url::parse("https://example.com").unwrap();
    let mut blocker = AdBlocker::new();
    assert!(!blocker.should_block(&url));
    assert!(blocker.is_enabled());
}

#[test]
fn test_content_script_manager() {
    let mgr = ContentScriptManager::new();
    let _ = mgr.all_scripts();
}

#[test]
fn test_popup_manager() {
    let mgr = PopupManager::new();
    assert!(!mgr.pending_new_window);
    assert!(mgr.pending_popup_window.is_none());
}

#[test]
fn test_git_status_parsing() {
    let status = GitStatus {
        branch: Some("main".into()),
        modified_count: 3,
        untracked_count: 2,
        is_dirty: true,
    };
    assert_eq!(status.status_bar_text(), "main *5");

    let clean = GitStatus {
        branch: Some("main".into()),
        modified_count: 0,
        untracked_count: 0,
        is_dirty: false,
    };
    assert_eq!(clean.status_bar_text(), "main");
}

#[test]
fn test_workspace_restore_module() {
    use aileron::workspace_restore;
    use std::collections::HashSet;

    let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
    let mut terminal_pane_ids = HashSet::new();
    let mut pane_mgr = PaneStateManager::new();
    let initial_url = url::Url::parse("https://example.com").unwrap();
    let mut wm = BspTree::new(viewport, initial_url);
    let mut terminal_mgr = NativeTerminalManager::new();

    let result = workspace_restore::restore_workspace(
        "nonexistent",
        viewport,
        None,
        &mut terminal_pane_ids,
        &mut pane_mgr,
        &mut wm,
        &mut terminal_mgr,
    );
    assert!(matches!(
        result,
        workspace_restore::RestoreOutcome::NoDatabase | workspace_restore::RestoreOutcome::NotFound
    ));
}
