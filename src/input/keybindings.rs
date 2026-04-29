use crate::input::mode::{Key, KeyCombo, Mode, Modifiers};
use std::collections::HashMap;

/// Actions that can be bound to key combinations.
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// Copy current page URL to clipboard.
    CopyUrl,
    /// Open find-in-page bar (Ctrl+F).
    Find,
    /// Find next match.
    FindNext,
    /// Find previous match.
    FindPrev,
    /// Close find bar (Escape).
    FindClose,
    /// Toggle link hints overlay (vimium-style).
    ToggleLinkHints,
    /// Open hinted link in a new background tab (vimium-style, F key).
    FollowLinkNewTab,
    /// Save current pane layout as a named workspace.
    SaveWorkspace,
    /// Open an embedded terminal pane.
    OpenTerminal,
    /// Open a new standalone browser window.
    NewWindow,
    /// Custom action (Lua-defined).
    Custom(String),
    /// Zoom in (increase page scale).
    ZoomIn,
    /// Zoom out (decrease page scale).
    ZoomOut,
    /// Reset zoom to 100%.
    ZoomReset,
    /// Resize pane: grow/shrink in a direction.
    ResizePane(crate::wm::Direction),
    /// Set a scroll mark (followed by a letter key).
    SetMark(char),
    /// Jump to a scroll mark (followed by a letter key).
    GoToMark(char),
    /// Toggle reader mode (strip CSS, show article text).
    ToggleReaderMode,
    /// Toggle minimal mode (disable JS, block images).
    ToggleMinimalMode,
    /// Show network request log.
    ToggleNetworkLog,
    /// Show JS console log.
    ToggleConsoleLog,
    /// Detach current pane to a standalone popup window.
    DetachPane,
    /// Close all panes except the current one.
    CloseOtherPanes,
    /// Print the current page.
    Print,
    /// Pin/unpin the active pane.
    PinPane,
}

/// Registry of keybindings, organized by mode.
pub struct KeybindingRegistry {
    bindings: HashMap<KeyCombo, Action>,
}

impl KeybindingRegistry {
    pub fn new() -> Self {
        Self {
            bindings: HashMap::new(),
        }
    }

    /// Load default keybindings.
    pub fn load_defaults(&mut self) {
        // Navigation (Normal mode)
        self.register(KeyCombo::normal(Key::Character('j')), Action::ScrollDown);
        self.register(KeyCombo::normal(Key::Character('k')), Action::ScrollUp);
        self.register(KeyCombo::normal(Key::Character('h')), Action::ScrollLeft);
        self.register(KeyCombo::normal(Key::Character('l')), Action::ScrollRight);
        self.register(
            KeyCombo::normal(Key::Character('i')),
            Action::EnterInsertMode,
        );
        self.register(
            KeyCombo::normal(Key::Character(':')),
            Action::OpenCommandPalette,
        );

        // Pane management
        self.register(KeyCombo::normal(Key::Character('q')), Action::ClosePane);
        self.register(
            KeyCombo::with_ctrl(Key::Character('w')),
            Action::SplitVertical,
        );
        self.register(
            KeyCombo::with_ctrl(Key::Character('s')),
            Action::SplitHorizontal,
        );

        // Navigation between panes
        let ctrl = Modifiers::ctrl();
        self.register(
            KeyCombo::new(Mode::Normal, ctrl, Key::Character('h')),
            Action::NavigateLeft,
        );
        self.register(
            KeyCombo::new(Mode::Normal, ctrl, Key::Character('j')),
            Action::NavigateDown,
        );
        self.register(
            KeyCombo::new(Mode::Normal, ctrl, Key::Character('k')),
            Action::NavigateUp,
        );
        self.register(
            KeyCombo::new(Mode::Normal, ctrl, Key::Character('l')),
            Action::NavigateRight,
        );

        // History navigation (Normal mode)
        self.register(KeyCombo::normal(Key::Character('H')), Action::NavigateBack);
        self.register(
            KeyCombo::normal(Key::Character('L')),
            Action::NavigateForward,
        );
        self.register(KeyCombo::normal(Key::Character('r')), Action::Reload);

        // Scrolling (Normal mode)
        // j/k = line scroll, Ctrl+D/Ctrl+U = half-page, G/gg = top/bottom
        self.register(
            KeyCombo::with_ctrl(Key::Character('d')),
            Action::HalfPageDown,
        );
        self.register(KeyCombo::with_ctrl(Key::Character('u')), Action::HalfPageUp);
        self.register(KeyCombo::normal(Key::Character('G')), Action::ScrollBottom);
        self.register(KeyCombo::with_ctrl(Key::Character('g')), Action::ScrollTop);

        // Bookmark toggle (Ctrl+B for Bookmark — Ctrl+D is HalfPageDown)
        self.register(
            KeyCombo::with_ctrl(Key::Character('b')),
            Action::BookmarkToggle,
        );

        // System
        self.register(
            KeyCombo::with_ctrl(Key::Character('e')),
            Action::OpenExternalBrowser,
        );
        self.register(
            KeyCombo::with_ctrl(Key::Character('p')),
            Action::OpenCommandPalette,
        );
        self.register(KeyCombo::with_ctrl(Key::Character('t')), Action::NewTab);

        // DevTools (F12)
        self.register(KeyCombo::normal(Key::F(12)), Action::ToggleDevTools);

        // Clipboard
        self.register(KeyCombo::normal(Key::Character('y')), Action::CopyUrl);

        // Zoom
        self.register(KeyCombo::with_ctrl(Key::Character('=')), Action::ZoomIn);
        self.register(KeyCombo::with_ctrl(Key::Character('-')), Action::ZoomOut);
        self.register(KeyCombo::with_ctrl(Key::Character('0')), Action::ZoomReset);

        // Reader mode (strip CSS, show article text)
        self.register(
            KeyCombo::new(
                Mode::Normal,
                Modifiers {
                    ctrl: true,
                    shift: true,
                    alt: false,
                    super_key: false,
                },
                Key::Character('R'),
            ),
            Action::ToggleReaderMode,
        );
        // Minimal mode (disable JS, block images)
        self.register(
            KeyCombo::new(
                Mode::Normal,
                Modifiers {
                    ctrl: true,
                    shift: true,
                    alt: false,
                    super_key: false,
                },
                Key::Character('M'),
            ),
            Action::ToggleMinimalMode,
        );

        // Find-in-page (vim-style / and Ctrl+F)
        self.register(KeyCombo::normal(Key::Character('/')), Action::Find);
        self.register(KeyCombo::with_ctrl(Key::Character('f')), Action::Find);

        // Link hints (vimium-style: f=foreground, F=background tab)
        self.register(
            KeyCombo::normal(Key::Character('f')),
            Action::ToggleLinkHints,
        );
        self.register(
            KeyCombo::normal(Key::Character('F')),
            Action::FollowLinkNewTab,
        );

        // Embedded terminal (backtick, like vim's :terminal)
        self.register(KeyCombo::normal(Key::Character('`')), Action::OpenTerminal);

        // Network log (Ctrl+Shift+N)
        self.register(
            KeyCombo::new(
                Mode::Normal,
                Modifiers {
                    ctrl: true,
                    shift: true,
                    alt: false,
                    super_key: false,
                },
                Key::Character('N'),
            ),
            Action::ToggleNetworkLog,
        );
        // JS console log (Ctrl+Shift+J)
        self.register(
            KeyCombo::new(
                Mode::Normal,
                Modifiers {
                    ctrl: true,
                    shift: true,
                    alt: false,
                    super_key: false,
                },
                Key::Character('J'),
            ),
            Action::ToggleConsoleLog,
        );

        // New standalone window (Ctrl+N)
        self.register(KeyCombo::with_ctrl(Key::Character('n')), Action::NewWindow);

        // Detach pane to popup (Ctrl+Shift+D)
        self.register(
            KeyCombo::new(
                Mode::Normal,
                Modifiers {
                    ctrl: true,
                    shift: true,
                    alt: false,
                    super_key: false,
                },
                Key::Character('D'),
            ),
            Action::DetachPane,
        );

        // Pin pane (Ctrl+Shift+P — Ctrl+P is command palette)
        self.register(
            KeyCombo::new(
                Mode::Normal,
                Modifiers {
                    ctrl: true,
                    shift: true,
                    alt: false,
                    super_key: false,
                },
                Key::Character('P'),
            ),
            Action::PinPane,
        );

        // Pane resize (Ctrl+Alt+H/J/K/L like tmux)
        let ctrl_alt = Modifiers {
            ctrl: true,
            alt: true,
            ..Modifiers::none()
        };
        self.register(
            KeyCombo::new(Mode::Normal, ctrl_alt, Key::Character('h')),
            Action::ResizePane(crate::wm::Direction::Left),
        );
        self.register(
            KeyCombo::new(Mode::Normal, ctrl_alt, Key::Character('l')),
            Action::ResizePane(crate::wm::Direction::Right),
        );
        self.register(
            KeyCombo::new(Mode::Normal, ctrl_alt, Key::Character('j')),
            Action::ResizePane(crate::wm::Direction::Down),
        );
        self.register(
            KeyCombo::new(Mode::Normal, ctrl_alt, Key::Character('k')),
            Action::ResizePane(crate::wm::Direction::Up),
        );
    }

    /// Register a keybinding. Overwrites any existing binding for the same combo.
    pub fn register(&mut self, combo: KeyCombo, action: Action) {
        self.bindings.insert(combo, action);
    }

    /// Look up the action for a key combo in the given mode.
    pub fn lookup(&self, mode: Mode, modifiers: Modifiers, key: Key) -> Option<&Action> {
        let combo = KeyCombo::new(mode, modifiers, key);
        self.bindings.get(&combo)
    }

    /// Apply custom keybinding overrides from config.
    /// Format: `"<key>" = "<Action>"` where key uses crossterm notation:
    /// - `"j"` - bare character key (Normal mode)
    /// - `"<C-p>"` - Ctrl+P (Normal mode)
    /// - `"<A-S>"` - Alt+Shift+S
    /// - `"<C-S-i>"` - Ctrl+Shift+I
    /// - `"<F1>"` - function keys
    ///
    /// Returns the number of bindings applied.
    pub fn apply_config_overrides(
        &mut self,
        overrides: &std::collections::HashMap<String, String>,
    ) -> usize {
        let mut applied = 0usize;
        for (key_str, action_str) in overrides {
            let combo = match Self::parse_key_combo(key_str) {
                Some(c) => c,
                None => {
                    tracing::warn!(target: "keybindings", "Failed to parse key: {}", key_str);
                    continue;
                }
            };
            let action = match Self::parse_action(action_str) {
                Some(a) => a,
                None => {
                    tracing::warn!(target: "keybindings", "Unknown action: {}", action_str);
                    continue;
                }
            };
            self.register(combo, action);
            applied += 1;
        }
        applied
    }

    /// Parse a key string in crossterm-like notation.
    fn parse_key_combo(s: &str) -> Option<KeyCombo> {
        let s = s.trim();
        // Strip angle brackets: <C-p> → C-p
        let inner = s
            .strip_prefix('<')
            .and_then(|r| r.strip_suffix('>'))
            .unwrap_or(s);

        let mut modifiers = Modifiers::none();
        let mut key_part = inner;

        // Parse modifier prefixes: C- for Ctrl, A- for Alt, S- for Shift, F1-F12
        loop {
            if let Some(rest) = key_part.strip_prefix("C-") {
                modifiers.ctrl = true;
                key_part = rest;
            } else if let Some(rest) = key_part.strip_prefix("A-") {
                modifiers.alt = true;
                key_part = rest;
            } else if let Some(rest) = key_part.strip_prefix("S-") {
                modifiers.shift = true;
                key_part = rest;
            } else {
                break;
            }
        }

        let key = if key_part.starts_with('F') && key_part.len() <= 3 {
            // Function key: F1-F12
            let num: u8 = key_part[1..].parse().ok()?;
            if num == 0 || num > 12 {
                return None;
            }
            Key::F(num)
        } else if key_part.len() == 1 {
            Key::Character(key_part.chars().next()?)
        } else {
            // Special keys
            match key_part {
                "Enter" | "Return" => Key::Enter,
                "Escape" | "Esc" => Key::Escape,
                "Backspace" | "BS" => Key::Backspace,
                "Tab" => Key::Tab,
                "Space" | "SPC" => Key::Character(' '),
                _ => return None,
            }
        };

        Some(KeyCombo::new(Mode::Normal, modifiers, key))
    }

    /// Parse an action string into an Action variant.
    fn parse_action(s: &str) -> Option<Action> {
        match s.trim() {
            "ScrollDown" => Some(Action::ScrollDown),
            "ScrollUp" => Some(Action::ScrollUp),
            "ScrollLeft" => Some(Action::ScrollLeft),
            "ScrollRight" => Some(Action::ScrollRight),
            "HalfPageDown" => Some(Action::HalfPageDown),
            "HalfPageUp" => Some(Action::HalfPageUp),
            "ScrollTop" => Some(Action::ScrollTop),
            "ScrollBottom" => Some(Action::ScrollBottom),
            "SplitVertical" | "vs" => Some(Action::SplitVertical),
            "SplitHorizontal" | "sp" => Some(Action::SplitHorizontal),
            "ClosePane" | "CloseTab" => Some(Action::ClosePane),
            "CloseOtherPanes" => Some(Action::CloseOtherPanes),
            "NavigateUp" => Some(Action::NavigateUp),
            "NavigateDown" => Some(Action::NavigateDown),
            "NavigateLeft" => Some(Action::NavigateLeft),
            "NavigateRight" => Some(Action::NavigateRight),
            "NavigateBack" => Some(Action::NavigateBack),
            "NavigateForward" => Some(Action::NavigateForward),
            "Reload" => Some(Action::Reload),
            "BookmarkToggle" => Some(Action::BookmarkToggle),
            "OpenCommandPalette" | "CommandPalette" => Some(Action::OpenCommandPalette),
            "OpenExternalBrowser" => Some(Action::OpenExternalBrowser),
            "EnterInsertMode" | "InsertMode" => Some(Action::EnterInsertMode),
            "ToggleDevTools" | "DevTools" => Some(Action::ToggleDevTools),
            "NewTab" => Some(Action::NewTab),
            "Yank" => Some(Action::Yank),
            "Paste" => Some(Action::Paste),
            "CopyUrl" => Some(Action::CopyUrl),
            "Find" => Some(Action::Find),
            "FindNext" => Some(Action::FindNext),
            "FindPrev" => Some(Action::FindPrev),
            "FindClose" => Some(Action::FindClose),
            "ToggleLinkHints" | "Hints" => Some(Action::ToggleLinkHints),
            "FollowLinkNewTab" | "FollowNewTab" => Some(Action::FollowLinkNewTab),
            "SaveWorkspace" => Some(Action::SaveWorkspace),
            "OpenTerminal" => Some(Action::OpenTerminal),
            "NewWindow" => Some(Action::NewWindow),
            "ZoomIn" => Some(Action::ZoomIn),
            "ZoomOut" => Some(Action::ZoomOut),
            "ZoomReset" => Some(Action::ZoomReset),
            "ToggleReaderMode" | "Reader" => Some(Action::ToggleReaderMode),
            "ToggleMinimalMode" | "Minimal" => Some(Action::ToggleMinimalMode),
            "ToggleNetworkLog" => Some(Action::ToggleNetworkLog),
            "ToggleConsoleLog" => Some(Action::ToggleConsoleLog),
            "DetachPane" | "Detach" => Some(Action::DetachPane),
            "Print" => Some(Action::Print),
            "PinPane" | "Pin" => Some(Action::PinPane),
            "Quit" => Some(Action::Quit),
            _ => None,
        }
    }

    /// Count of registered bindings.
    pub fn len(&self) -> usize {
        self.bindings.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }

    /// Parse a mode string ("normal", "insert", "command") into a Mode.
    pub fn parse_mode(s: &str) -> Option<Mode> {
        match s.trim().to_lowercase().as_str() {
            "normal" | "n" => Some(Mode::Normal),
            "insert" | "i" => Some(Mode::Insert),
            "command" | "c" => Some(Mode::Command),
            _ => None,
        }
    }
}

impl Default for KeybindingRegistry {
    fn default() -> Self {
        let mut registry = Self::new();
        registry.load_defaults();
        registry
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_keybindings_loaded() {
        let registry = KeybindingRegistry::default();
        assert!(!registry.is_empty());
        assert!(registry.len() > 10);
    }

    #[test]
    fn test_lookup_scroll_down() {
        let registry = KeybindingRegistry::default();
        assert_eq!(
            registry.lookup(Mode::Normal, Modifiers::none(), Key::Character('j')),
            Some(&Action::ScrollDown)
        );
    }

    #[test]
    fn test_lookup_scroll_up() {
        let registry = KeybindingRegistry::default();
        assert_eq!(
            registry.lookup(Mode::Normal, Modifiers::none(), Key::Character('k')),
            Some(&Action::ScrollUp)
        );
    }

    #[test]
    fn test_lookup_unbound_returns_none() {
        let registry = KeybindingRegistry::default();
        assert_eq!(
            registry.lookup(Mode::Normal, Modifiers::none(), Key::Character('z')),
            None
        );
    }

    #[test]
    fn test_lookup_mode_isolated() {
        let registry = KeybindingRegistry::default();
        // 'j' in Normal mode is ScrollDown, but in Insert mode should be None
        assert_eq!(
            registry.lookup(Mode::Insert, Modifiers::none(), Key::Character('j')),
            None
        );
    }

    #[test]
    fn test_register_overrides_default() {
        let mut registry = KeybindingRegistry::default();
        registry.register(
            KeyCombo::normal(Key::Character('j')),
            Action::Custom("custom_scroll".into()),
        );
        assert_eq!(
            registry.lookup(Mode::Normal, Modifiers::none(), Key::Character('j')),
            Some(&Action::Custom("custom_scroll".into()))
        );
    }

    #[test]
    fn test_ctrl_combo_lookup() {
        let registry = KeybindingRegistry::default();
        assert_eq!(
            registry.lookup(Mode::Normal, Modifiers::ctrl(), Key::Character('e')),
            Some(&Action::OpenExternalBrowser)
        );
    }
}
