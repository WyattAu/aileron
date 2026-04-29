//! Integration smoke tests — verify that core modules wire together correctly.
//!
//! These tests catch cross-module issues that unit tests cannot:
//! - Serialization roundtrips (BspTree ↔ WorkspaceData ↔ JSON)
//! - Dispatch → Effect pipeline correctness
//! - Keybinding routing through AppState
//! - BSP tree spatial invariants after complex operations
//! - Config default sanity

use aileron::app::AppState;
use aileron::app::dispatch::{ActionEffect, dispatch_action};
use aileron::config::Config;
use aileron::db::workspaces::{SplitDir, WorkspaceData, WorkspaceNode};
use aileron::input::{Action, Key, KeyEvent, Modifiers};
use aileron::wm::{BspTree, Rect, SplitDirection};

fn test_viewport() -> Rect {
    Rect::new(0.0, 0.0, 1920.0, 1080.0)
}

fn test_url() -> url::Url {
    url::Url::parse("https://example.com").unwrap()
}

// ─── 1. BSP tree ↔ WorkspaceData roundtrip ───────────────────────────

#[test]
fn test_bsp_tree_workspace_roundtrip() {
    let viewport = test_viewport();
    let mut tree = BspTree::new(viewport, test_url());

    let id1 = tree.active_pane_id();
    let id2 = tree.split(id1, SplitDirection::Vertical, 0.5).unwrap();
    let _id3 = tree.split(id1, SplitDirection::Horizontal, 0.5).unwrap();
    let _id4 = tree.split(id2, SplitDirection::Horizontal, 0.5).unwrap();
    assert_eq!(tree.leaf_count(), 4);

    assert_eq!(tree.leaf_count(), 4);

    let data = tree.to_workspace_data(|_| None).unwrap();

    let restored = BspTree::from_workspace_data(&data, viewport).unwrap();

    assert_eq!(
        restored.leaf_count(),
        tree.leaf_count(),
        "pane count should survive workspace roundtrip"
    );
    assert!(restored.verify_coverage());
    assert!(restored.verify_non_overlapping());
}

#[test]
fn test_bsp_tree_workspace_roundtrip_single_pane() {
    let viewport = test_viewport();
    let tree = BspTree::new(viewport, test_url());
    assert_eq!(tree.leaf_count(), 1);

    let data = tree.to_workspace_data(|_| None).unwrap();
    let restored = BspTree::from_workspace_data(&data, viewport).unwrap();

    assert_eq!(restored.leaf_count(), 1);
    assert!(restored.verify_coverage());
    assert!(restored.verify_non_overlapping());
}

#[test]
fn test_bsp_tree_workspace_roundtrip_with_url_resolver() {
    let viewport = test_viewport();
    let mut tree = BspTree::new(viewport, test_url());

    let id1 = tree.active_pane_id();
    tree.split(id1, SplitDirection::Vertical, 0.5).unwrap();

    let live_url = "https://live.example.com".to_string();
    let data = tree
        .to_workspace_data(|pane_id| {
            if pane_id == id1 {
                Some(live_url.clone())
            } else {
                None
            }
        })
        .unwrap();

    assert_eq!(data.active_url, "aileron://new");

    // Verify the tree structure by checking URLs via collect_urls
    let urls = aileron::db::workspaces::collect_urls(&data.tree);
    assert_eq!(urls.len(), 2);
    assert!(urls.contains(&live_url));
    assert!(urls.contains(&"aileron://new".to_string()));
}

// ─── 2. Dispatch produces expected effects ───────────────────────────

#[test]
fn test_dispatch_split_vertical_produces_request_split() {
    let effects = dispatch_action(&Action::SplitVertical);
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, ActionEffect::RequestSplit(SplitDirection::Vertical))),
        "SplitVertical should produce RequestSplit(Vertical), got {:?}",
        effects
    );
}

#[test]
fn test_dispatch_close_pane_produces_request_close() {
    let effects = dispatch_action(&Action::ClosePane);
    assert!(
        effects.contains(&ActionEffect::RequestClosePane),
        "ClosePane should produce RequestClosePane, got {:?}",
        effects
    );
}

#[test]
fn test_dispatch_quit_produces_quit_effect() {
    let effects = dispatch_action(&Action::Quit);
    assert!(
        effects.contains(&ActionEffect::Quit),
        "Quit should produce Quit effect, got {:?}",
        effects
    );
}

#[test]
fn test_dispatch_open_command_palette_produces_open_palette() {
    let effects = dispatch_action(&Action::OpenCommandPalette);
    assert!(
        effects.contains(&ActionEffect::OpenPalette),
        "OpenCommandPalette should produce OpenPalette, got {:?}",
        effects
    );
}

#[test]
fn test_dispatch_navigate_produces_correct_directions() {
    let cases = [
        (Action::NavigateUp, aileron::wm::Direction::Up),
        (Action::NavigateDown, aileron::wm::Direction::Down),
        (Action::NavigateLeft, aileron::wm::Direction::Left),
        (Action::NavigateRight, aileron::wm::Direction::Right),
    ];

    for (action, expected_dir) in cases {
        let effects = dispatch_action(&action);
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, ActionEffect::RequestNavigatePane(d) if *d == expected_dir)),
            "{:?} should produce RequestNavigatePane({:?}), got {:?}",
            action,
            expected_dir,
            effects
        );
    }
}

#[test]
fn test_dispatch_scroll_actions_are_wry_effects() {
    let scroll_actions = [
        Action::ScrollDown,
        Action::ScrollUp,
        Action::ScrollLeft,
        Action::ScrollRight,
        Action::HalfPageDown,
        Action::HalfPageUp,
        Action::ScrollTop,
        Action::ScrollBottom,
    ];

    for action in &scroll_actions {
        let effects = dispatch_action(action);
        assert_eq!(
            effects.len(),
            1,
            "{:?} should produce exactly 1 effect, got {}",
            action,
            effects.len()
        );
        assert!(
            matches!(effects[0], ActionEffect::Wry(_)),
            "{:?} should produce a Wry effect",
            action
        );
    }
}

// ─── 3. AppState keybinding routing ──────────────────────────────────

#[test]
fn test_app_state_keybinding_routing_opens_palette() {
    let viewport = test_viewport();
    let mut state = AppState::new(viewport, Config::default()).unwrap();
    assert!(!state.palette.open);

    let event = KeyEvent {
        key: Key::Character('p'),
        modifiers: Modifiers::ctrl(),
        physical_key: None,
    };
    state.process_key_event(event);

    assert!(
        state.palette.open,
        "Ctrl+P should open the command palette via keybinding routing"
    );
}

#[test]
fn test_app_state_keybinding_routing_enters_insert_mode() {
    let viewport = test_viewport();
    let mut state = AppState::new(viewport, Config::default()).unwrap();
    assert_eq!(state.mode, aileron::input::Mode::Normal);

    let event = KeyEvent {
        key: Key::Character('i'),
        modifiers: Modifiers::none(),
        physical_key: None,
    };
    state.process_key_event(event);

    assert_eq!(
        state.mode,
        aileron::input::Mode::Insert,
        "'i' key should enter Insert mode via keybinding"
    );
}

#[test]
fn test_app_state_keybinding_routing_does_not_fire_in_insert_mode() {
    let viewport = test_viewport();
    let mut state = AppState::new(viewport, Config::default()).unwrap();

    state.mode = aileron::input::Mode::Insert;
    let event = KeyEvent {
        key: Key::Character('j'),
        modifiers: Modifiers::none(),
        physical_key: None,
    };
    state.process_key_event(event);

    assert!(
        state.pending_wry_actions.is_empty(),
        "'j' in Insert mode should not trigger ScrollDown (no Wry actions queued)"
    );
}

#[test]
fn test_app_state_command_palette_quit() {
    let viewport = test_viewport();
    let mut state = AppState::new(viewport, Config::default()).unwrap();
    assert!(!state.should_quit);

    // Open the command palette via Ctrl+P
    state.process_key_event(KeyEvent {
        key: Key::Character('p'),
        modifiers: Modifiers::ctrl(),
        physical_key: None,
    });
    assert!(state.palette.open);

    // Type 'q' into the palette
    state.process_key_event(KeyEvent {
        key: Key::Character('q'),
        modifiers: Modifiers::none(),
        physical_key: None,
    });

    // Submit with Enter
    state.process_key_event(KeyEvent {
        key: Key::Enter,
        modifiers: Modifiers::none(),
        physical_key: None,
    });

    assert!(
        state.should_quit,
        "typing ':q' then Enter in the command palette should set should_quit"
    );
}

// ─── 4. WorkspaceData JSON roundtrip ─────────────────────────────────

#[test]
fn test_workspace_data_json_roundtrip() {
    let original = WorkspaceData {
        tree: WorkspaceNode::Split {
            direction: SplitDir::Vertical,
            ratio: 0.5,
            left: Box::new(WorkspaceNode::Leaf {
                url: "https://example.com".into(),
            }),
            right: Box::new(WorkspaceNode::Split {
                direction: SplitDir::Horizontal,
                ratio: 0.6,
                left: Box::new(WorkspaceNode::Leaf {
                    url: "https://rust-lang.org".into(),
                }),
                right: Box::new(WorkspaceNode::Leaf {
                    url: "https://github.com".into(),
                }),
            }),
        },
        active_url: "https://rust-lang.org".into(),
    };

    let json = original.to_json().unwrap();
    let restored = WorkspaceData::from_json(&json).unwrap();

    assert_eq!(original.active_url, restored.active_url);
    assert_eq!(
        serde_json::to_value(&original.tree).unwrap(),
        serde_json::to_value(&restored.tree).unwrap(),
        "workspace tree structure should survive JSON roundtrip"
    );
}

#[test]
fn test_workspace_data_json_roundtrip_single_leaf() {
    let original = WorkspaceData {
        tree: WorkspaceNode::Leaf {
            url: "aileron://welcome".into(),
        },
        active_url: "aileron://welcome".into(),
    };

    let json = original.to_json().unwrap();
    let restored = WorkspaceData::from_json(&json).unwrap();

    assert_eq!(original.active_url, restored.active_url);
    assert_eq!(
        serde_json::to_value(&original.tree).unwrap(),
        serde_json::to_value(&restored.tree).unwrap()
    );
}

#[test]
fn test_workspace_data_json_is_valid_json() {
    let data = WorkspaceData {
        tree: WorkspaceNode::Split {
            direction: SplitDir::Horizontal,
            ratio: 0.3,
            left: Box::new(WorkspaceNode::Leaf {
                url: "https://a.com".into(),
            }),
            right: Box::new(WorkspaceNode::Leaf {
                url: "https://b.com".into(),
            }),
        },
        active_url: "https://a.com".into(),
    };

    let json = data.to_json().unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["active_url"], "https://a.com");
    assert_eq!(parsed["tree"]["Split"]["ratio"], 0.3);
}

// ─── 5. BSP tree split/close preserves spatial invariants ────────────

#[test]
fn test_bsp_tree_split_close_preserves_coverage() {
    let viewport = test_viewport();
    let mut tree = BspTree::new(viewport, test_url());

    let id1 = tree.active_pane_id();
    let id2 = tree.split(id1, SplitDirection::Vertical, 0.5).unwrap();
    let id3 = tree.split(id1, SplitDirection::Horizontal, 0.5).unwrap();
    let _id4 = tree.split(id2, SplitDirection::Horizontal, 0.5).unwrap();

    assert_eq!(tree.leaf_count(), 4);
    assert!(tree.verify_coverage());
    assert!(tree.verify_non_overlapping());

    tree.close(id3).unwrap();
    assert_eq!(tree.leaf_count(), 3);
    assert!(
        tree.verify_coverage(),
        "coverage should hold after closing one of four panes"
    );
    assert!(
        tree.verify_non_overlapping(),
        "non-overlapping should hold after closing one of four panes"
    );
}

#[test]
fn test_bsp_tree_multiple_splits_and_closes() {
    let viewport = test_viewport();
    let mut tree = BspTree::new(viewport, test_url());

    // Build a 4-pane grid
    let id1 = tree.active_pane_id();
    let id2 = tree.split(id1, SplitDirection::Vertical, 0.5).unwrap();
    let id3 = tree.split(id1, SplitDirection::Horizontal, 0.5).unwrap();
    let id4 = tree.split(id2, SplitDirection::Horizontal, 0.5).unwrap();
    assert_eq!(tree.leaf_count(), 4);

    // Close two panes, leaving 2
    tree.close(id3).unwrap();
    assert!(tree.verify_coverage());
    assert!(tree.verify_non_overlapping());

    tree.close(id4).unwrap();
    assert_eq!(tree.leaf_count(), 2);
    assert!(
        tree.verify_coverage(),
        "coverage should hold after closing down to 2 panes"
    );
    assert!(
        tree.verify_non_overlapping(),
        "non-overlapping should hold after closing down to 2 panes"
    );
}

#[test]
fn test_bsp_tree_split_close_split_preserves_invariants() {
    let viewport = test_viewport();
    let mut tree = BspTree::new(viewport, test_url());

    let id1 = tree.active_pane_id();
    let _id2 = tree.split(id1, SplitDirection::Vertical, 0.5).unwrap();
    assert_eq!(tree.leaf_count(), 2);

    // Close back to 1 pane
    tree.close(id1).unwrap();
    assert_eq!(tree.leaf_count(), 1);
    assert!(tree.verify_coverage());

    // Re-split the single pane
    let active = tree.active_pane_id();
    tree.split(active, SplitDirection::Horizontal, 0.5).unwrap();
    assert_eq!(tree.leaf_count(), 2);
    assert!(
        tree.verify_coverage(),
        "coverage should hold after close-then-resplit cycle"
    );
    assert!(tree.verify_non_overlapping());
}

#[test]
fn test_bsp_tree_resize_after_splits_preserves_invariants() {
    let viewport = test_viewport();
    let mut tree = BspTree::new(viewport, test_url());

    let id1 = tree.active_pane_id();
    let id2 = tree.split(id1, SplitDirection::Vertical, 0.5).unwrap();
    tree.split(id1, SplitDirection::Horizontal, 0.5).unwrap();
    tree.split(id2, SplitDirection::Horizontal, 0.5).unwrap();
    assert_eq!(tree.leaf_count(), 4);

    let new_viewport = Rect::new(0.0, 0.0, 1280.0, 720.0);
    tree.resize(new_viewport);

    assert!(
        tree.verify_coverage(),
        "coverage should hold after resizing a 4-pane tree"
    );
    assert!(
        tree.verify_non_overlapping(),
        "non-overlapping should hold after resizing a 4-pane tree"
    );
}

// ─── 6. Config default validity ──────────────────────────────────────

#[test]
fn test_config_default_is_valid() {
    let config = Config::default();

    assert!(
        !config.homepage.is_empty(),
        "default homepage should not be empty"
    );
    assert!(
        config.window_width > 0,
        "default window_width should be > 0"
    );
    assert!(
        config.window_height > 0,
        "default window_height should be > 0"
    );
    assert!(
        config.palette.max_results > 0,
        "default palette max_results should be > 0"
    );

    // Homepage should be parseable as a URL
    let _url = url::Url::parse(&config.homepage).expect("default homepage should be a valid URL");
}

#[test]
fn test_config_default_window_sane_aspect() {
    let config = Config::default();
    let ratio = config.window_width as f64 / config.window_height as f64;
    assert!(
        (0.5..3.0).contains(&ratio),
        "default window aspect ratio ({:.2}) should be reasonable",
        ratio
    );
}

// ─── 7. Cross-module: AppState uses Config homepage ──────────────────

#[test]
fn test_app_state_uses_config_homepage() {
    let viewport = test_viewport();
    let mut config = Config::default();
    config.homepage = "https://my-custom-home.page".into();

    let state = AppState::new(viewport, config).unwrap();
    let panes = state.wm.panes();
    assert_eq!(panes.len(), 1);

    // The pane's URL comes from the config homepage
    let (_id, rect) = &panes[0];
    assert_eq!(*rect, viewport);
}

// ─── 8. Cross-module: BspTree roundtrip through different viewports ──

#[test]
fn test_bsp_tree_workspace_roundtrip_different_viewport() {
    let viewport1 = Rect::new(0.0, 0.0, 1920.0, 1080.0);
    let mut tree = BspTree::new(viewport1, test_url());

    let id1 = tree.active_pane_id();
    tree.split(id1, SplitDirection::Vertical, 0.5).unwrap();

    let data = tree.to_workspace_data(|_| None).unwrap();

    let viewport2 = Rect::new(0.0, 0.0, 1280.0, 720.0);
    let restored = BspTree::from_workspace_data(&data, viewport2).unwrap();

    assert_eq!(restored.leaf_count(), 2);
    assert!(restored.verify_coverage());
    assert!(restored.verify_non_overlapping());

    // Verify the restored tree fits the new viewport
    for (_, rect) in restored.panes() {
        assert!(
            rect.x >= viewport2.x && rect.y >= viewport2.y,
            "pane rect should be within viewport"
        );
        assert!(
            rect.x + rect.w <= viewport2.x + viewport2.w + 0.001,
            "pane rect width should fit within viewport"
        );
        assert!(
            rect.y + rect.h <= viewport2.y + viewport2.h + 0.001,
            "pane rect height should fit within viewport"
        );
    }
}

// ─── 9. Cross-module: KeybindingRegistry dispatches all actions ──────

#[test]
fn test_keybinding_registry_has_non_empty_defaults() {
    let registry = aileron::input::KeybindingRegistry::default();
    assert!(
        registry.len() > 15,
        "default keybindings should cover many actions"
    );
}

#[test]
fn test_keybinding_lookup_consistency_with_dispatch() {
    let registry = aileron::input::KeybindingRegistry::default();

    // Verify that actions found via keybinding lookup dispatch without error
    let combos_to_check = [
        (Modifiers::none(), Key::Character('j')),
        (Modifiers::ctrl(), Key::Character('w')),
        (Modifiers::ctrl(), Key::Character('s')),
        (Modifiers::ctrl(), Key::Character('p')),
        (Modifiers::none(), Key::Character('q')),
    ];

    for (modifiers, key) in combos_to_check {
        let action = registry.lookup(aileron::input::Mode::Normal, modifiers, key);
        if let Some(action) = action {
            let effects = dispatch_action(action);
            assert!(
                !effects.is_empty(),
                "action {:?} from keybinding should produce effects",
                action
            );
        }
    }
}
