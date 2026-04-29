use std::fmt;

/// Input mode (Normal, Insert, Command) — the core of the modal editing system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode {
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

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Apply a key event to determine if a mode transition should occur.
/// Returns Some(new_mode) if a transition happens, None otherwise.
pub fn transition(mode: Mode, event: &KeyEvent) -> Option<Mode> {
    match (mode, &event.key) {
        // Enter Insert mode
        (Mode::Normal, Key::Character('i')) => Some(Mode::Insert),
        (Mode::Normal, Key::Character('I')) => Some(Mode::Insert),

        // Enter Command mode
        (Mode::Normal, Key::Character(':')) => Some(Mode::Command),

        // Return to Normal mode
        (Mode::Insert, Key::Escape) => Some(Mode::Normal),
        (Mode::Command, Key::Escape) => Some(Mode::Normal),
        // Ctrl+[ also exits to Normal (vim-style)
        (Mode::Insert, Key::CtrlBracket) => Some(Mode::Normal),

        _ => None,
    }
}

/// Key identifier for routing.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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

impl Key {
    pub fn from_char(c: char) -> Self {
        Key::Character(c)
    }
}

/// Key event with modifier state.
#[derive(Debug, Clone)]
pub struct KeyEvent {
    pub key: Key,
    pub modifiers: Modifiers,
    pub physical_key: Option<PhysicalKey>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub super_key: bool,
}

impl Modifiers {
    pub fn none() -> Self {
        Self::default()
    }

    pub fn ctrl() -> Self {
        Self {
            ctrl: true,
            ..Self::default()
        }
    }
}

/// A combination of modifiers + key, used as a HashMap key for keybindings.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyCombo {
    pub mode: Mode,
    pub modifiers: Modifiers,
    pub key: Key,
}

impl KeyCombo {
    pub fn new(mode: Mode, modifiers: Modifiers, key: Key) -> Self {
        Self {
            mode,
            modifiers,
            key,
        }
    }

    pub fn normal(key: Key) -> Self {
        Self::new(Mode::Normal, Modifiers::none(), key)
    }

    pub fn with_ctrl(key: Key) -> Self {
        Self::new(Mode::Normal, Modifiers::ctrl(), key)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PhysicalKey {
    pub code: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_mode_is_none() {
        // transition returns None when no transition should happen
        assert!(
            transition(
                Mode::Normal,
                &KeyEvent {
                    key: Key::Character('j'),
                    modifiers: Modifiers::none(),
                    physical_key: None
                }
            )
            .is_none()
        );
    }

    #[test]
    fn test_i_enters_insert() {
        assert_eq!(
            transition(
                Mode::Normal,
                &KeyEvent {
                    key: Key::Character('i'),
                    modifiers: Modifiers::none(),
                    physical_key: None
                }
            ),
            Some(Mode::Insert)
        );
    }

    #[test]
    fn test_esc_returns_to_normal_from_insert() {
        assert_eq!(
            transition(
                Mode::Insert,
                &KeyEvent {
                    key: Key::Escape,
                    modifiers: Modifiers::none(),
                    physical_key: None
                }
            ),
            Some(Mode::Normal)
        );
    }

    #[test]
    fn test_esc_returns_to_normal_from_command() {
        assert_eq!(
            transition(
                Mode::Command,
                &KeyEvent {
                    key: Key::Escape,
                    modifiers: Modifiers::none(),
                    physical_key: None
                }
            ),
            Some(Mode::Normal)
        );
    }

    #[test]
    fn test_colon_enters_command() {
        assert_eq!(
            transition(
                Mode::Normal,
                &KeyEvent {
                    key: Key::Character(':'),
                    modifiers: Modifiers::none(),
                    physical_key: None
                }
            ),
            Some(Mode::Command)
        );
    }

    #[test]
    fn test_rapid_mode_switching() {
        let mut mode = Mode::Normal;
        for _ in 0..1000 {
            mode = transition(
                mode,
                &KeyEvent {
                    key: Key::Character('i'),
                    modifiers: Modifiers::none(),
                    physical_key: None,
                },
            )
            .unwrap();
            assert_eq!(mode, Mode::Insert);
            mode = transition(
                mode,
                &KeyEvent {
                    key: Key::Escape,
                    modifiers: Modifiers::none(),
                    physical_key: None,
                },
            )
            .unwrap();
            assert_eq!(mode, Mode::Normal);
        }
    }

    #[test]
    fn test_insert_mode_no_transition_on_j() {
        assert!(
            transition(
                Mode::Insert,
                &KeyEvent {
                    key: Key::Character('j'),
                    modifiers: Modifiers::none(),
                    physical_key: None
                }
            )
            .is_none()
        );
    }

    #[test]
    fn test_mode_display() {
        assert_eq!(Mode::Normal.as_str(), "NORMAL");
        assert_eq!(Mode::Insert.as_str(), "INSERT");
        assert_eq!(Mode::Command.as_str(), "COMMAND");
    }
}
