//! Input routing integration tests.
//!
//! End-to-end verification of the key event pipeline:
//!   KeyEvent -> route_event / keybinding lookup -> Action -> ActionEffect
//!
//! Tests cover mode transitions, keybinding dispatch, mode isolation,
//! command palette interaction, config overrides, and router totality --
//! all without requiring a GUI.

use aileron::app::AppState;
use aileron::app::WryAction;
use aileron::config::Config;
use aileron::input::{Key, KeyEvent, KeybindingRegistry, Mode, Modifiers, route_event};
use aileron::wm::Rect;

fn test_viewport() -> Rect {
    Rect::new(0.0, 0.0, 1920.0, 1080.0)
}

fn make_state() -> AppState {
    AppState::new(test_viewport(), Config::default()).unwrap()
}

fn key_event(key: Key, modifiers: Modifiers) -> KeyEvent {
    KeyEvent {
        key,
        modifiers,
        physical_key: None,
    }
}

// ─── 1. Scroll keybindings produce correct WryActions ──────────────

#[test]
fn test_j_key_in_normal_produces_scroll_down() {
    let mut state = make_state();
    state.process_key_event(key_event(Key::Character('j'), Modifiers::none()));
    assert!(
        !state.pending_wry_actions.is_empty(),
        "'j' in Normal mode should queue a WryAction"
    );
    match &state.pending_wry_actions[0] {
        WryAction::SmoothScroll { y, .. } => assert_eq!(*y, 120.0),
        other => panic!("Expected SmoothScroll(y=120), got {other:?}"),
    }
}

#[test]
fn test_k_key_in_normal_produces_scroll_up() {
    let mut state = make_state();
    state.process_key_event(key_event(Key::Character('k'), Modifiers::none()));
    match &state.pending_wry_actions[0] {
        WryAction::SmoothScroll { y, .. } => assert_eq!(*y, -120.0),
        other => panic!("Expected SmoothScroll(y=-120), got {other:?}"),
    }
}

#[test]
fn test_ctrl_d_produces_half_page_down() {
    let mut state = make_state();
    state.process_key_event(key_event(Key::Character('d'), Modifiers::ctrl()));
    match &state.pending_wry_actions[0] {
        WryAction::SmoothScroll { y, .. } => assert_eq!(*y, 400.0),
        other => panic!("Expected SmoothScroll(y=400), got {other:?}"),
    }
}

#[test]
fn test_ctrl_u_produces_half_page_up() {
    let mut state = make_state();
    state.process_key_event(key_event(Key::Character('u'), Modifiers::ctrl()));
    match &state.pending_wry_actions[0] {
        WryAction::SmoothScroll { y, .. } => assert_eq!(*y, -400.0),
        other => panic!("Expected SmoothScroll(y=-400), got {other:?}"),
    }
}

#[test]
fn test_g_capital_produces_scroll_to_bottom() {
    let mut state = make_state();
    state.process_key_event(key_event(Key::Character('G'), Modifiers::none()));
    match &state.pending_wry_actions[0] {
        WryAction::SmoothScroll { y, .. } => assert_eq!(*y, 999999.0),
        other => panic!("Expected SmoothScroll(y=999999), got {other:?}"),
    }
}

#[test]
fn test_ctrl_g_produces_scroll_to_top() {
    let mut state = make_state();
    state.process_key_event(key_event(Key::Character('g'), Modifiers::ctrl()));
    match &state.pending_wry_actions[0] {
        WryAction::SmoothScroll { y, .. } => assert_eq!(*y, -999999.0),
        other => panic!("Expected SmoothScroll(y=-999999), got {other:?}"),
    }
}

// ─── 2. Navigation keybindings ─────────────────────────────────────

#[test]
fn test_r_key_reloads_page() {
    let mut state = make_state();
    state.process_key_event(key_event(Key::Character('r'), Modifiers::none()));
    assert!(
        state.pending_wry_actions.contains(&WryAction::Reload),
        "'r' should queue Reload"
    );
}

#[test]
fn test_h_capital_navigates_back() {
    let mut state = make_state();
    state.process_key_event(key_event(Key::Character('H'), Modifiers::none()));
    assert!(
        state.pending_wry_actions.contains(&WryAction::Back),
        "'H' should queue Back"
    );
}

#[test]
fn test_l_capital_navigates_forward() {
    let mut state = make_state();
    state.process_key_event(key_event(Key::Character('L'), Modifiers::none()));
    assert!(
        state.pending_wry_actions.contains(&WryAction::Forward),
        "'L' should queue Forward"
    );
}

// ─── 3. Mode transition and isolation ──────────────────────────────

#[test]
fn test_insert_mode_suppresses_normal_keybindings() {
    let mut state = make_state();
    // Enter insert mode
    state.process_key_event(key_event(Key::Character('i'), Modifiers::none()));
    assert_eq!(state.mode, Mode::Insert);
    state.pending_wry_actions.clear();

    // 'j' in Insert should NOT scroll
    state.process_key_event(key_event(Key::Character('j'), Modifiers::none()));
    assert!(
        state.pending_wry_actions.is_empty(),
        "'j' in Insert mode should produce no WryActions"
    );

    // 'k' in Insert should NOT scroll
    state.process_key_event(key_event(Key::Character('k'), Modifiers::none()));
    assert!(
        state.pending_wry_actions.is_empty(),
        "'k' in Insert mode should produce no WryActions"
    );

    // 'r' in Insert should NOT reload
    state.process_key_event(key_event(Key::Character('r'), Modifiers::none()));
    assert!(
        state.pending_wry_actions.is_empty(),
        "'r' in Insert mode should produce no WryActions"
    );
}

#[test]
fn test_mode_round_trip_restores_keybindings() {
    let mut state = make_state();
    // Normal -> Insert -> Normal
    state.process_key_event(key_event(Key::Character('i'), Modifiers::none()));
    assert_eq!(state.mode, Mode::Insert);
    state.process_key_event(key_event(Key::Escape, Modifiers::none()));
    assert_eq!(state.mode, Mode::Normal);

    // 'j' should scroll again
    state.pending_wry_actions.clear();
    state.process_key_event(key_event(Key::Character('j'), Modifiers::none()));
    assert!(
        !state.pending_wry_actions.is_empty(),
        "After Normal -> Insert -> Normal, 'j' should scroll again"
    );
}

// ─── 4. Palette interaction ────────────────────────────────────────

#[test]
fn test_ctrl_p_opens_palette_and_routes_keys_there() {
    let mut state = make_state();
    state.process_key_event(key_event(Key::Character('p'), Modifiers::ctrl()));
    assert!(state.palette.open);

    // Typing into palette should not produce wry actions
    state.pending_wry_actions.clear();
    state.process_key_event(key_event(Key::Character('x'), Modifiers::none()));
    state.process_key_event(key_event(Key::Character('y'), Modifiers::none()));
    state.process_key_event(key_event(Key::Character('z'), Modifiers::none()));
    assert!(
        state.pending_wry_actions.is_empty(),
        "Keys typed into palette should not produce WryActions"
    );
}

#[test]
fn test_colon_opens_palette() {
    let mut state = make_state();
    state.process_key_event(key_event(Key::Character(':'), Modifiers::none()));
    assert!(state.palette.open, "':' should open the command palette");
}

#[test]
fn test_escape_closes_palette() {
    let mut state = make_state();
    state.process_key_event(key_event(Key::Character('p'), Modifiers::ctrl()));
    assert!(state.palette.open);

    state.process_key_event(key_event(Key::Escape, Modifiers::none()));
    assert!(!state.palette.open, "Escape should close the palette");
}

#[test]
fn test_palette_quit_sequence() {
    let mut state = make_state();
    assert!(!state.session.should_quit);

    // Open palette via Ctrl+P
    state.process_key_event(key_event(Key::Character('p'), Modifiers::ctrl()));
    assert!(state.palette.open);

    // Type 'quit'
    state.process_key_event(key_event(Key::Character('q'), Modifiers::none()));
    state.process_key_event(key_event(Key::Character('u'), Modifiers::none()));
    state.process_key_event(key_event(Key::Character('i'), Modifiers::none()));
    state.process_key_event(key_event(Key::Character('t'), Modifiers::none()));

    // Submit
    state.process_key_event(key_event(Key::Enter, Modifiers::none()));
    assert!(
        state.session.should_quit,
        "typing 'quit' + Enter in palette should set should_quit"
    );
}

#[test]
fn test_colon_quit_sequence() {
    let mut state = make_state();
    assert!(!state.session.should_quit);

    // Open palette with ':'
    state.process_key_event(key_event(Key::Character(':'), Modifiers::none()));
    assert!(state.palette.open);

    // Type 'q' and Enter
    state.process_key_event(key_event(Key::Character('q'), Modifiers::none()));
    state.process_key_event(key_event(Key::Enter, Modifiers::none()));
    assert!(
        state.session.should_quit,
        "typing ':q' + Enter should set should_quit"
    );
}

// ─── 5. Unbound keys ───────────────────────────────────────────────

#[test]
fn test_unbound_key_in_normal_produces_no_effects() {
    let mut state = make_state();
    state.pending_wry_actions.clear();

    let unbound_keys = ['z', 'x', '1', '2', '3'];
    for key in &unbound_keys {
        state.process_key_event(key_event(Key::Character(*key), Modifiers::none()));
    }
    assert!(
        state.pending_wry_actions.is_empty(),
        "unbound keys should produce no WryActions"
    );
}

// ─── 6. Split keybindings ──────────────────────────────────────────

#[test]
fn test_ctrl_w_splits_vertical() {
    let mut state = make_state();
    state.process_key_event(key_event(Key::Character('w'), Modifiers::ctrl()));
    assert_eq!(
        state.wm.leaf_count(),
        2,
        "Ctrl+W should split the tree into 2 panes"
    );
}

#[test]
fn test_ctrl_s_splits_horizontal() {
    let mut state = make_state();
    state.process_key_event(key_event(Key::Character('s'), Modifiers::ctrl()));
    assert_eq!(
        state.wm.leaf_count(),
        2,
        "Ctrl+S should split the tree into 2 panes"
    );
}

#[test]
fn test_ctrl_t_creates_new_tab() {
    let mut state = make_state();
    state.process_key_event(key_event(Key::Character('t'), Modifiers::ctrl()));
    // Ctrl+T = NewTab, which now creates a new tab within the active pane.
    // The pane should now have 2 tabs.
    let active_id = state.wm.active_pane_id();
    if let Some(root) = state.wm.root_mut()
        && let Some(pane) = aileron::wm::BspTree::find_pane_mut(root, active_id)
    {
        assert_eq!(
            pane.tabs.len(),
            2,
            "Ctrl+T should create a second tab in the active pane"
        );
    }
}

// ─── 7. Config keybinding override ─────────────────────────────────

#[test]
fn test_config_override_remapping() {
    let mut config = Config::default();
    config.keybindings.insert("<C-j>".into(), "ScrollUp".into());

    let mut state = AppState::new(test_viewport(), config).unwrap();
    state.process_key_event(key_event(Key::Character('j'), Modifiers::ctrl()));
    match &state.pending_wry_actions[0] {
        WryAction::SmoothScroll { y, .. } => {
            assert_eq!(*y, -120.0, "Ctrl+J remapped to ScrollUp should scroll up")
        }
        other => panic!("Expected SmoothScroll(y=-120), got {other:?}"),
    }
}

#[test]
fn test_config_override_new_binding() {
    let mut config = Config::default();
    config.keybindings.insert("<C-S-q>".into(), "Quit".into());

    let mut state = AppState::new(test_viewport(), config).unwrap();
    state.process_key_event(key_event(
        Key::Character('q'),
        Modifiers {
            ctrl: true,
            shift: true,
            alt: false,
            super_key: false,
        },
    ));
    assert!(
        state.session.should_quit,
        "Ctrl+Shift+Q remapped to Quit should set should_quit"
    );
}

// ─── 8. KeybindingRegistry dispatch consistency ────────────────────

#[test]
fn test_all_default_keybindings_are_registered() {
    let registry = KeybindingRegistry::default();
    let known_combos: Vec<(Mode, Modifiers, Key)> = vec![
        // Scrolling
        (Mode::Normal, Modifiers::none(), Key::Character('j')),
        (Mode::Normal, Modifiers::none(), Key::Character('k')),
        (Mode::Normal, Modifiers::ctrl(), Key::Character('d')),
        (Mode::Normal, Modifiers::ctrl(), Key::Character('u')),
        (Mode::Normal, Modifiers::none(), Key::Character('G')),
        (Mode::Normal, Modifiers::ctrl(), Key::Character('g')),
        // Navigation
        (Mode::Normal, Modifiers::none(), Key::Character('r')),
        (Mode::Normal, Modifiers::none(), Key::Character('H')),
        (Mode::Normal, Modifiers::none(), Key::Character('L')),
        // Pane management
        (Mode::Normal, Modifiers::none(), Key::Character('q')),
        (Mode::Normal, Modifiers::ctrl(), Key::Character('w')),
        (Mode::Normal, Modifiers::ctrl(), Key::Character('s')),
        (Mode::Normal, Modifiers::ctrl(), Key::Character('t')),
        // Mode
        (Mode::Normal, Modifiers::none(), Key::Character('i')),
        (Mode::Normal, Modifiers::ctrl(), Key::Character('p')),
        (Mode::Normal, Modifiers::none(), Key::Character(':')),
        // DevTools
        (Mode::Normal, Modifiers::none(), Key::F(12)),
        // Find
        (Mode::Normal, Modifiers::none(), Key::Character('/')),
        (Mode::Normal, Modifiers::ctrl(), Key::Character('f')),
    ];

    for (mode, mods, key) in &known_combos {
        let action = registry.lookup(*mode, *mods, key.clone());
        assert!(
            action.is_some(),
            "No action registered for {mode:?} + {mods:?} + {key:?}"
        );
    }
}

// ─── 9. Router totality ────────────────────────────────────────────

#[test]
fn test_router_returns_destination_for_all_mode_key_combinations() {
    let keys = [
        Key::Character('a'),
        Key::Character('z'),
        Key::Character('0'),
        Key::Escape,
        Key::Enter,
        Key::Backspace,
        Key::Tab,
        Key::Up,
        Key::Down,
        Key::Left,
        Key::Right,
        Key::Home,
        Key::End,
        Key::PageUp,
        Key::PageDown,
        Key::F(1),
        Key::F(12),
        Key::Unknown,
    ];
    let modes = [Mode::Normal, Mode::Insert, Mode::Command];
    let mods_list = [Modifiers::none(), Modifiers::ctrl()];

    for mode in &modes {
        for key in &keys {
            for modifiers in &mods_list {
                let event = key_event(key.clone(), *modifiers);
                let _dest = route_event(*mode, &event);
                // Should never panic -- route_event is total
            }
        }
    }
}

// ─── 10. Find bar ──────────────────────────────────────────────────

#[test]
fn test_slash_opens_find_bar() {
    let mut state = make_state();
    state.process_key_event(key_event(Key::Character('/'), Modifiers::none()));
    assert!(state.ui.find_bar_open, "'/' should open the find bar");
}

#[test]
fn test_ctrl_f_opens_find_bar() {
    let mut state = make_state();
    state.process_key_event(key_event(Key::Character('f'), Modifiers::ctrl()));
    assert!(state.ui.find_bar_open, "Ctrl+F should open the find bar");
}

// ─── 11. DevTools toggle ───────────────────────────────────────────

#[test]
fn test_f12_toggles_devtools() {
    let mut state = make_state();
    state.process_key_event(key_event(Key::F(12), Modifiers::none()));
    assert!(
        state
            .pending_wry_actions
            .contains(&WryAction::ToggleDevTools),
        "F12 should toggle devtools"
    );
}

// ─── 12. Multi-step workflows ──────────────────────────────────────

#[test]
fn test_split_then_close_via_frame_tasks() {
    let mut state = make_state();
    assert_eq!(state.wm.leaf_count(), 1);

    // Split
    state.process_key_event(key_event(Key::Character('w'), Modifiers::ctrl()));
    assert_eq!(state.wm.leaf_count(), 2);
    let new_id = state.wm.active_pane_id();

    // Close via frame_tasks handle_pending_tab_close
    aileron::frame_tasks::handle_pending_tab_close(&mut state, new_id);
    assert_eq!(state.wm.leaf_count(), 1, "closed pane should be removed");
}

#[test]
fn test_multiple_splits_and_navigations() {
    let mut state = make_state();
    assert_eq!(state.wm.leaf_count(), 1);

    // Split twice for 3 panes
    state.process_key_event(key_event(Key::Character('w'), Modifiers::ctrl()));
    assert_eq!(state.wm.leaf_count(), 2);

    state.process_key_event(key_event(Key::Character('w'), Modifiers::ctrl()));
    assert_eq!(state.wm.leaf_count(), 3);

    // Navigate between panes (hjkl direction keys)
    state.process_key_event(key_event(Key::Character('h'), Modifiers::none()));
    // h/j/k/l navigation modifies AppState.wm.active_pane, not WryActions
    assert_eq!(
        state.wm.leaf_count(),
        3,
        "navigation should not change pane count"
    );
}

#[test]
fn test_full_workflow_split_navigate_quit() {
    let mut state = make_state();

    // Split vertically
    state.process_key_event(key_event(Key::Character('w'), Modifiers::ctrl()));
    assert_eq!(state.wm.leaf_count(), 2);

    // Scroll down in the active pane
    state.process_key_event(key_event(Key::Character('j'), Modifiers::none()));
    assert!(
        !state.pending_wry_actions.is_empty(),
        "should have scroll action"
    );

    // Open palette and quit
    state.process_key_event(key_event(Key::Character('p'), Modifiers::ctrl()));
    assert!(state.palette.open);
    state.process_key_event(key_event(Key::Character('q'), Modifiers::none()));
    state.process_key_event(key_event(Key::Enter, Modifiers::none()));
    assert!(state.session.should_quit);
}

// ─── 13. Ctrl+B toggles bookmark ───────────────────────────────────

#[test]
fn test_ctrl_b_toggles_bookmark() {
    let mut state = make_state();
    state.process_key_event(key_event(Key::Character('b'), Modifiers::ctrl()));
    assert!(
        state
            .pending_wry_actions
            .contains(&WryAction::ToggleBookmark),
        "Ctrl+B should toggle bookmark"
    );
}

// ─── 14. Zoom keybindings ──────────────────────────────────────────

#[test]
fn test_ctrl_equals_zooms_in() {
    let mut state = make_state();
    state.process_key_event(key_event(Key::Character('='), Modifiers::ctrl()));
    assert!(
        state
            .pending_wry_actions
            .iter()
            .any(|a| matches!(a, WryAction::RunJs(js) if js.contains("zoom"))),
        "Ctrl+= should produce a zoom JS action"
    );
}

#[test]
fn test_ctrl_minus_zooms_out() {
    let mut state = make_state();
    state.process_key_event(key_event(Key::Character('-'), Modifiers::ctrl()));
    assert!(
        state
            .pending_wry_actions
            .iter()
            .any(|a| matches!(a, WryAction::RunJs(js) if js.contains("zoom"))),
        "Ctrl+- should produce a zoom JS action"
    );
}

#[test]
fn test_ctrl_zero_resets_zoom() {
    let mut state = make_state();
    state.process_key_event(key_event(Key::Character('0'), Modifiers::ctrl()));
    assert!(
        state
            .pending_wry_actions
            .iter()
            .any(|a| matches!(a, WryAction::RunJs(js) if js.contains("zoom"))),
        "Ctrl+0 should produce a zoom JS action"
    );
}
