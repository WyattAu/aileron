//! Pure action dispatch — maps `Action` → `Vec<ActionEffect>` with no side effects.
//!
//! This module is the testable core of Aileron's action system. Every keybinding
//! ultimately flows through `dispatch_action()`, which returns a list of effects
//! that the caller (AppState) applies. Because this function is pure (no I/O,
//! no mutation), it can be exhaustively unit-tested.

use crate::app::WryAction;
use crate::input::Action;
use crate::input::Mode;
use crate::wm::{Direction, SplitDirection};

/// An effect that should be applied by the caller after dispatching an action.
///
/// Effects are pure data — they describe *what should happen*, not *how*.
/// The caller (AppState) interprets each variant and applies it to its own state.
#[derive(Debug, Clone, PartialEq)]
pub enum ActionEffect {
    /// Queue a wry action for the rendering backend.
    Wry(WryAction),

    /// Set the status bar message (replaces current message).
    Status(String),

    /// Change the input mode.
    SetMode(Mode),

    /// Request application quit.
    Quit,

    /// Open the command palette.
    OpenPalette,

    /// Request a pane split in the given direction.
    /// Caller must: call `wm.split()`, create engine for new pane, set status.
    RequestSplit(SplitDirection),

    /// Request closing the active pane.
    /// Caller must: call `wm.close()`, remove engine, set status.
    RequestClosePane,

    /// Request navigating to an adjacent pane in the given direction.
    /// Caller must: call `wm.navigate()`, set active pane, update status.
    RequestNavigatePane(Direction),

    /// Open the active pane's URL in the system browser.
    /// Caller must: look up engine URL, call `open_in_system_browser()`, set status.
    RequestExternalBrowser,

    /// Toggle find-in-page bar.
    OpenFindBar,

    /// Close find-in-page bar.
    CloseFindBar,

    /// Search for next/previous match in the page.
    FindInPage { query: String, forward: bool },

    /// Toggle link hints overlay.
    ToggleLinkHints,

    /// Save current pane layout as a named workspace.
    SaveWorkspace,
    /// Open an embedded terminal pane.
    OpenTerminal,
    /// Open a new standalone browser window.
    NewWindow,
    /// Copy the active pane's URL to the system clipboard.
    CopyUrl,
    /// Resize the active pane in a direction.
    ResizePane(Direction),
    /// Enter reader mode (toggle).
    EnterReaderMode,
    /// Exit reader mode.
    ExitReaderMode,
    /// Enter minimal mode (toggle).
    EnterMinimalMode,
    /// Exit minimal mode.
    ExitMinimalMode,
    /// Get network request log.
    GetNetworkLog,
    /// Clear network request log.
    ClearNetworkLog,
    /// Get JS console log.
    GetConsoleLog,
    /// Clear JS console log.
    ClearConsoleLog,
    /// Detach current pane to a standalone popup window.
    DetachPane,
    /// Close all panes except the current one.
    CloseOtherPanes,
    /// Print the current page.
    Print,
}

/// Pure dispatch: map an `Action` to a list of effects.
///
/// This function contains no I/O, no mutation, and no access to AppState.
/// It is the single source of truth for what each action does.
///
/// # Why a `Vec`?
///
/// Some actions produce multiple effects. For example, `ScrollDown` produces
/// both a `Wry(ScrollBy)` and a `Status` effect. Returning a `Vec` allows
/// the caller to apply them in order without any branching.
pub fn dispatch_action(action: &Action) -> Vec<ActionEffect> {
    match action {
        // ─── Lifecycle ────────────────────────────────────────────
        Action::Quit => vec![ActionEffect::Quit],

        // ─── Scrolling (pure Wry actions) ─────────────────────────
        Action::ScrollDown => vec![ActionEffect::Wry(WryAction::ScrollBy { x: 0.0, y: 120.0 })],
        Action::ScrollUp => vec![ActionEffect::Wry(WryAction::ScrollBy { x: 0.0, y: -120.0 })],
        Action::HalfPageDown => vec![ActionEffect::Wry(WryAction::ScrollTo { fraction: 0.5 })],
        Action::HalfPageUp => vec![ActionEffect::Wry(WryAction::ScrollTo { fraction: 0.3 })],
        Action::ScrollTop => vec![ActionEffect::Wry(WryAction::ScrollTo { fraction: 0.0 })],
        Action::ScrollBottom => vec![ActionEffect::Wry(WryAction::ScrollTo { fraction: 1.0 })],
        Action::ScrollLeft => vec![ActionEffect::Wry(WryAction::ScrollBy { x: -120.0, y: 0.0 })],
        Action::ScrollRight => vec![ActionEffect::Wry(WryAction::ScrollBy { x: 120.0, y: 0.0 })],

        // ─── Clipboard ────────────────────────────────────────────
        Action::Yank => vec![
            ActionEffect::Wry(WryAction::RunJs("window.getSelection().toString()".into())),
            ActionEffect::Status("Selection copied".into()),
        ],
        Action::Paste => vec![
            ActionEffect::Wry(WryAction::RunJs("document.execCommand('paste')".into())),
            ActionEffect::Status("Paste".into()),
        ],

        // ─── Pane management (state-dependent, delegated to caller) ─
        Action::SplitVertical => vec![ActionEffect::RequestSplit(SplitDirection::Vertical)],
        Action::SplitHorizontal => vec![ActionEffect::RequestSplit(SplitDirection::Horizontal)],
        Action::ClosePane => vec![ActionEffect::RequestClosePane],

        // ─── Pane navigation (state-dependent) ─────────────────────
        Action::NavigateUp => vec![ActionEffect::RequestNavigatePane(Direction::Up)],
        Action::NavigateDown => vec![ActionEffect::RequestNavigatePane(Direction::Down)],
        Action::NavigateLeft => vec![ActionEffect::RequestNavigatePane(Direction::Left)],
        Action::NavigateRight => vec![ActionEffect::RequestNavigatePane(Direction::Right)],

        // ─── Mode changes ─────────────────────────────────────────
        Action::EnterInsertMode => vec![ActionEffect::SetMode(Mode::Insert)],

        // ─── UI ───────────────────────────────────────────────────
        Action::OpenCommandPalette => vec![ActionEffect::OpenPalette],

        // ─── History navigation (pure Wry actions) ────────────────
        Action::NavigateBack => vec![
            ActionEffect::Wry(WryAction::Back),
            ActionEffect::Status("Back".into()),
        ],
        Action::NavigateForward => vec![
            ActionEffect::Wry(WryAction::Forward),
            ActionEffect::Status("Forward".into()),
        ],
        Action::Reload => vec![
            ActionEffect::Wry(WryAction::Reload),
            ActionEffect::Status("Reloading...".into()),
        ],

        // ─── Bookmarks ────────────────────────────────────────────
        Action::BookmarkToggle => vec![
            ActionEffect::Wry(WryAction::ToggleBookmark),
            ActionEffect::Status("Toggling bookmark...".into()),
        ],

        // ─── DevTools ─────────────────────────────────────────────
        Action::ToggleDevTools => vec![
            ActionEffect::Wry(WryAction::ToggleDevTools),
            ActionEffect::Status("DevTools".into()),
        ],

        // ─── New tab ──────────────────────────────────────────────
        Action::NewTab => {
            let url = url::Url::parse("aileron://new")
                .unwrap_or_else(|_| url::Url::parse("aileron://welcome").unwrap());
            vec![
                ActionEffect::Wry(WryAction::Navigate(url)),
                ActionEffect::Status("New tab".into()),
            ]
        }

        // ─── External browser (state-dependent) ───────────────────
        Action::OpenExternalBrowser => vec![ActionEffect::RequestExternalBrowser],

        // ─── Find-in-page ─────────────────────────────────────────
        Action::Find => vec![ActionEffect::OpenFindBar],
        Action::FindNext => vec![ActionEffect::Status("Find next".into())],
        Action::FindPrev => vec![ActionEffect::Status("Find prev".into())],
        Action::FindClose => vec![ActionEffect::CloseFindBar],

        // ─── Link hints ───────────────────────────────────────────
        Action::ToggleLinkHints => vec![
            ActionEffect::Wry(WryAction::RunJs(
                r#"
                (function() {
                    if (document.getElementById('__aileron_hints')) {
                        document.getElementById('__aileron_hints').remove();
                        document.querySelectorAll('[data-aileron-hint]').forEach(el => {
                            el.removeAttribute('data-aileron-hint');
                        });
                        return 'hints_removed';
                    }
                    var style = document.createElement('style');
                    style.id = '__aileron_hints';
                    style.textContent = '[data-aileron-hint]::after { content: attr(data-aileron-hint); position: absolute; background: #4db4ff; color: #000; font-size: 11px; font-weight: bold; padding: 1px 4px; border-radius: 3px; z-index: 999999; pointer-events: none; font-family: monospace; }';
                    document.head.appendChild(style);
                    var links = document.querySelectorAll('a[href], button, input[type="submit"], [role="link"]');
                    links.forEach(function(el, i) {
                        el.setAttribute('data-aileron-hint', String(i));
                    });
                    return 'hints_shown_' + links.length;
                })();
                "#.into(),
            )),
            ActionEffect::Status("Link hints: type number, Enter to follow".into()),
        ],

        // ─── Workspace ───────────────────────────────────────────
        Action::SaveWorkspace => vec![ActionEffect::SaveWorkspace],

        // ─── Terminal ───────────────────────────────────────────
        Action::OpenTerminal => vec![ActionEffect::OpenTerminal],

        // ─── New window ────────────────────────────────────────
        Action::NewWindow => vec![ActionEffect::NewWindow],

        // ─── Copy URL ──────────────────────────────────────────
        Action::CopyUrl => vec![ActionEffect::CopyUrl],

        // ─── Pane resize ──────────────────────────────────────
        Action::ResizePane(direction) => vec![ActionEffect::ResizePane(*direction)],

        // ─── Marks ──────────────────────────────────────────
        Action::SetMark(c) => vec![ActionEffect::Status(format!("Mark {} set", c))],
        Action::GoToMark(c) => vec![ActionEffect::Status(format!("Go to mark {}", c))],

        // ─── Zoom ──────────────────────────────────────────────
        Action::ZoomIn => vec![
            ActionEffect::Wry(WryAction::RunJs(
                "document.body.style.zoom = (parseFloat(document.body.style.zoom || '1') + 0.1).toFixed(1)"
                    .into(),
            )),
            ActionEffect::Status("Zoom in".into()),
        ],
        Action::ZoomOut => vec![
            ActionEffect::Wry(WryAction::RunJs(
                "document.body.style.zoom = Math.max(0.3, (parseFloat(document.body.style.zoom || '1') - 0.1)).toFixed(1)"
                    .into(),
            )),
            ActionEffect::Status("Zoom out".into()),
        ],
        Action::ZoomReset => vec![
            ActionEffect::Wry(WryAction::RunJs("document.body.style.zoom = '1'".into())),
            ActionEffect::Status("Zoom reset".into()),
        ],

        // ─── Custom / unknown ─────────────────────────────────────
        Action::Custom(name) => vec![ActionEffect::Status(format!("Action: {}", name))],

        // ─── Reader mode ──────────────────────────────────────────
        Action::ToggleReaderMode => {
            vec![ActionEffect::EnterReaderMode]
        }
        Action::ToggleMinimalMode => {
            vec![ActionEffect::EnterMinimalMode]
        }
        Action::ToggleNetworkLog => {
            vec![ActionEffect::GetNetworkLog]
        }
        Action::ToggleConsoleLog => {
            vec![ActionEffect::GetConsoleLog]
        }
        Action::DetachPane => vec![ActionEffect::DetachPane],
        Action::CloseOtherPanes => vec![ActionEffect::CloseOtherPanes],
        Action::Print => vec![ActionEffect::Print],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper: extract Wry effects from a dispatch result
    fn wry_effects(effects: &[ActionEffect]) -> Vec<&WryAction> {
        effects
            .iter()
            .filter_map(|e| match e {
                ActionEffect::Wry(a) => Some(a),
                _ => None,
            })
            .collect()
    }

    // Helper: extract status message from dispatch result
    fn status_msg(effects: &[ActionEffect]) -> Option<&str> {
        effects.iter().find_map(|e| match e {
            ActionEffect::Status(s) => Some(s.as_str()),
            _ => None,
        })
    }

    // ─── Lifecycle ────────────────────────────────────────────────

    #[test]
    fn test_quit() {
        let effects = dispatch_action(&Action::Quit);
        assert!(effects.contains(&ActionEffect::Quit));
    }

    // ─── Scrolling ────────────────────────────────────────────────

    #[test]
    fn test_scroll_down() {
        let effects = dispatch_action(&Action::ScrollDown);
        let wry = wry_effects(&effects);
        assert_eq!(wry.len(), 1);
        assert_eq!(wry[0], &WryAction::ScrollBy { x: 0.0, y: 120.0 });
    }

    #[test]
    fn test_scroll_up() {
        let effects = dispatch_action(&Action::ScrollUp);
        let wry = wry_effects(&effects);
        assert_eq!(wry.len(), 1);
        assert_eq!(wry[0], &WryAction::ScrollBy { x: 0.0, y: -120.0 });
    }

    #[test]
    fn test_scroll_left() {
        let effects = dispatch_action(&Action::ScrollLeft);
        let wry = wry_effects(&effects);
        assert_eq!(wry.len(), 1);
        assert_eq!(wry[0], &WryAction::ScrollBy { x: -120.0, y: 0.0 });
    }

    #[test]
    fn test_scroll_right() {
        let effects = dispatch_action(&Action::ScrollRight);
        let wry = wry_effects(&effects);
        assert_eq!(wry.len(), 1);
        assert_eq!(wry[0], &WryAction::ScrollBy { x: 120.0, y: 0.0 });
    }

    #[test]
    fn test_half_page_down() {
        let effects = dispatch_action(&Action::HalfPageDown);
        let wry = wry_effects(&effects);
        assert_eq!(wry.len(), 1);
        assert_eq!(wry[0], &WryAction::ScrollTo { fraction: 0.5 });
    }

    #[test]
    fn test_half_page_up() {
        let effects = dispatch_action(&Action::HalfPageUp);
        let wry = wry_effects(&effects);
        assert_eq!(wry.len(), 1);
        assert_eq!(wry[0], &WryAction::ScrollTo { fraction: 0.3 });
    }

    #[test]
    fn test_scroll_top() {
        let effects = dispatch_action(&Action::ScrollTop);
        let wry = wry_effects(&effects);
        assert_eq!(wry.len(), 1);
        assert_eq!(wry[0], &WryAction::ScrollTo { fraction: 0.0 });
    }

    #[test]
    fn test_scroll_bottom() {
        let effects = dispatch_action(&Action::ScrollBottom);
        let wry = wry_effects(&effects);
        assert_eq!(wry.len(), 1);
        assert_eq!(wry[0], &WryAction::ScrollTo { fraction: 1.0 });
    }

    // ─── Clipboard ────────────────────────────────────────────────

    #[test]
    fn test_yank_produces_js_and_status() {
        let effects = dispatch_action(&Action::Yank);
        assert_eq!(effects.len(), 2);
        assert_eq!(status_msg(&effects), Some("Selection copied"));
        let wry = wry_effects(&effects);
        match &wry[0] {
            WryAction::RunJs(js) => assert!(js.contains("getSelection")),
            other => panic!("Expected RunJs, got {:?}", other),
        }
    }

    #[test]
    fn test_paste_produces_js_and_status() {
        let effects = dispatch_action(&Action::Paste);
        assert_eq!(effects.len(), 2);
        assert_eq!(status_msg(&effects), Some("Paste"));
        let wry = wry_effects(&effects);
        match &wry[0] {
            WryAction::RunJs(js) => assert!(js.contains("paste")),
            other => panic!("Expected RunJs, got {:?}", other),
        }
    }

    // ─── Pane management ──────────────────────────────────────────

    #[test]
    fn test_split_vertical() {
        let effects = dispatch_action(&Action::SplitVertical);
        assert_eq!(effects.len(), 1);
        assert_eq!(
            effects[0],
            ActionEffect::RequestSplit(SplitDirection::Vertical)
        );
    }

    #[test]
    fn test_split_horizontal() {
        let effects = dispatch_action(&Action::SplitHorizontal);
        assert_eq!(effects.len(), 1);
        assert_eq!(
            effects[0],
            ActionEffect::RequestSplit(SplitDirection::Horizontal)
        );
    }

    #[test]
    fn test_close_pane() {
        let effects = dispatch_action(&Action::ClosePane);
        assert_eq!(effects.len(), 1);
        assert_eq!(effects[0], ActionEffect::RequestClosePane);
    }

    // ─── Pane navigation ──────────────────────────────────────────

    #[test]
    fn test_navigate_up() {
        let effects = dispatch_action(&Action::NavigateUp);
        assert_eq!(effects.len(), 1);
        assert_eq!(effects[0], ActionEffect::RequestNavigatePane(Direction::Up));
    }

    #[test]
    fn test_navigate_down() {
        let effects = dispatch_action(&Action::NavigateDown);
        assert_eq!(effects.len(), 1);
        assert_eq!(
            effects[0],
            ActionEffect::RequestNavigatePane(Direction::Down)
        );
    }

    #[test]
    fn test_navigate_left() {
        let effects = dispatch_action(&Action::NavigateLeft);
        assert_eq!(effects.len(), 1);
        assert_eq!(
            effects[0],
            ActionEffect::RequestNavigatePane(Direction::Left)
        );
    }

    #[test]
    fn test_navigate_right() {
        let effects = dispatch_action(&Action::NavigateRight);
        assert_eq!(effects.len(), 1);
        assert_eq!(
            effects[0],
            ActionEffect::RequestNavigatePane(Direction::Right)
        );
    }

    // ─── Mode changes ─────────────────────────────────────────────

    #[test]
    fn test_enter_insert_mode() {
        let effects = dispatch_action(&Action::EnterInsertMode);
        assert_eq!(effects.len(), 1);
        assert_eq!(effects[0], ActionEffect::SetMode(Mode::Insert));
    }

    // ─── UI ───────────────────────────────────────────────────────

    #[test]
    fn test_open_command_palette() {
        let effects = dispatch_action(&Action::OpenCommandPalette);
        assert_eq!(effects.len(), 1);
        assert_eq!(effects[0], ActionEffect::OpenPalette);
    }

    // ─── History navigation ───────────────────────────────────────

    #[test]
    fn test_navigate_back() {
        let effects = dispatch_action(&Action::NavigateBack);
        let wry = wry_effects(&effects);
        assert!(wry.contains(&&WryAction::Back));
        assert_eq!(status_msg(&effects), Some("Back"));
    }

    #[test]
    fn test_navigate_forward() {
        let effects = dispatch_action(&Action::NavigateForward);
        let wry = wry_effects(&effects);
        assert!(wry.contains(&&WryAction::Forward));
        assert_eq!(status_msg(&effects), Some("Forward"));
    }

    #[test]
    fn test_reload() {
        let effects = dispatch_action(&Action::Reload);
        let wry = wry_effects(&effects);
        assert!(wry.contains(&&WryAction::Reload));
        assert_eq!(status_msg(&effects), Some("Reloading..."));
    }

    // ─── Bookmarks ────────────────────────────────────────────────

    #[test]
    fn test_bookmark_toggle() {
        let effects = dispatch_action(&Action::BookmarkToggle);
        let wry = wry_effects(&effects);
        assert!(wry.contains(&&WryAction::ToggleBookmark));
        assert_eq!(status_msg(&effects), Some("Toggling bookmark..."));
    }

    // ─── DevTools ─────────────────────────────────────────────────

    #[test]
    fn test_toggle_devtools() {
        let effects = dispatch_action(&Action::ToggleDevTools);
        let wry = wry_effects(&effects);
        assert!(wry.contains(&&WryAction::ToggleDevTools));
        assert_eq!(status_msg(&effects), Some("DevTools"));
    }

    // ─── New tab ──────────────────────────────────────────────────

    #[test]
    fn test_new_tab_navigates_to_aileron_new() {
        let effects = dispatch_action(&Action::NewTab);
        let wry = wry_effects(&effects);
        assert_eq!(wry.len(), 1);
        match &wry[0] {
            WryAction::Navigate(url) => {
                assert_eq!(url.as_str(), "aileron://new");
            }
            other => panic!("Expected Navigate, got {:?}", other),
        }
        assert_eq!(status_msg(&effects), Some("New tab"));
    }

    // ─── External browser ─────────────────────────────────────────

    #[test]
    fn test_open_external_browser() {
        let effects = dispatch_action(&Action::OpenExternalBrowser);
        assert_eq!(effects.len(), 1);
        assert_eq!(effects[0], ActionEffect::RequestExternalBrowser);
    }

    // ─── Find-in-page ─────────────────────────────────────────────

    #[test]
    fn test_find_opens_find_bar() {
        let effects = dispatch_action(&Action::Find);
        assert_eq!(effects.len(), 1);
        assert_eq!(effects[0], ActionEffect::OpenFindBar);
    }

    #[test]
    fn test_find_next() {
        let effects = dispatch_action(&Action::FindNext);
        assert_eq!(status_msg(&effects), Some("Find next"));
    }

    #[test]
    fn test_find_prev() {
        let effects = dispatch_action(&Action::FindPrev);
        assert_eq!(status_msg(&effects), Some("Find prev"));
    }

    #[test]
    fn test_find_close() {
        let effects = dispatch_action(&Action::FindClose);
        assert_eq!(effects.len(), 1);
        assert_eq!(effects[0], ActionEffect::CloseFindBar);
    }

    // ─── Link hints ───────────────────────────────────────────────

    #[test]
    fn test_toggle_link_hints_produces_js() {
        let effects = dispatch_action(&Action::ToggleLinkHints);
        let wry = wry_effects(&effects);
        assert_eq!(wry.len(), 1);
        match &wry[0] {
            WryAction::RunJs(js) => {
                assert!(
                    js.contains("__aileron_hints"),
                    "JS should define hint styles"
                );
                assert!(
                    js.contains("data-aileron-hint"),
                    "JS should set hint attributes"
                );
            }
            other => panic!("Expected RunJs, got {:?}", other),
        }
        assert_eq!(
            status_msg(&effects),
            Some("Link hints: type number, Enter to follow")
        );
    }

    // ─── Custom actions ───────────────────────────────────────────

    #[test]
    fn test_custom_action_produces_status() {
        let effects = dispatch_action(&Action::Custom("my-action".into()));
        assert_eq!(effects.len(), 1);
        assert_eq!(status_msg(&effects), Some("Action: my-action"));
    }

    // ─── Exhaustiveness: every Action variant is handled ──────────

    #[test]
    fn test_every_action_produces_effects() {
        // Verify that every Action variant dispatches to at least one effect
        let actions = [
            Action::Quit,
            Action::ScrollDown,
            Action::ScrollUp,
            Action::ScrollLeft,
            Action::ScrollRight,
            Action::HalfPageDown,
            Action::HalfPageUp,
            Action::ScrollTop,
            Action::ScrollBottom,
            Action::Yank,
            Action::Paste,
            Action::SplitVertical,
            Action::SplitHorizontal,
            Action::ClosePane,
            Action::NavigateUp,
            Action::NavigateDown,
            Action::NavigateLeft,
            Action::NavigateRight,
            Action::OpenCommandPalette,
            Action::OpenExternalBrowser,
            Action::EnterInsertMode,
            Action::NavigateBack,
            Action::NavigateForward,
            Action::Reload,
            Action::BookmarkToggle,
            Action::ToggleDevTools,
            Action::NewTab,
            Action::Find,
            Action::FindNext,
            Action::FindPrev,
            Action::FindClose,
            Action::ToggleLinkHints,
            Action::SaveWorkspace,
            Action::CopyUrl,
            Action::ZoomIn,
            Action::ZoomOut,
            Action::ZoomReset,
            Action::ResizePane(Direction::Right),
            Action::SetMark('a'),
            Action::GoToMark('a'),
            Action::NewWindow,
            Action::ToggleReaderMode,
            Action::ToggleMinimalMode,
            Action::ToggleNetworkLog,
            Action::ToggleConsoleLog,
            Action::DetachPane,
            Action::CloseOtherPanes,
            Action::Custom("test".into()),
        ];

        for action in &actions {
            let effects = dispatch_action(action);
            assert!(
                !effects.is_empty(),
                "Action {:?} should produce at least one effect",
                action
            );
        }
    }

    // ─── No duplicate effect types that shouldn't be duplicated ──

    #[test]
    fn test_scroll_actions_produce_exactly_one_wry_effect() {
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
                "Scroll action {:?} should produce exactly 1 effect, got {}",
                action,
                effects.len()
            );
            assert!(
                matches!(effects[0], ActionEffect::Wry(_)),
                "Scroll action {:?} should produce a Wry effect",
                action
            );
        }
    }
}
