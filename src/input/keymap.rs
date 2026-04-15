//! Key mapping from winit physical/virtual keys to Aileron's internal `Key` enum.
//!
//! This module is intentionally decoupled from the event loop so it can be
//! unit-tested without a winit display or wgpu surface.

use crate::input::mode::Key;
use winit::keyboard::{KeyCode, PhysicalKey};

/// Map a winit `KeyCode` (physical key) to Aileron's `Key` enum.
///
/// For alphanumeric keys, the physical `KeyCode` is used to ensure layout-
/// independent mapping (QWERTY `KeyA` always maps to `Key::Character('a')`
/// regardless of the user's keyboard layout). For function keys and special
/// keys, the mapping is direct.
///
/// Falls back to parsing the `logical_key` string for any unmapped codes
/// (e.g., international characters, numpad keys).
pub fn map_winit_key(code: KeyCode, logical_key: &winit::keyboard::Key) -> Key {
    match code {
        KeyCode::Escape => Key::Escape,
        KeyCode::Enter => Key::Enter,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Tab => Key::Tab,
        KeyCode::ArrowUp => Key::Up,
        KeyCode::ArrowDown => Key::Down,
        KeyCode::ArrowLeft => Key::Left,
        KeyCode::ArrowRight => Key::Right,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::KeyA => Key::Character('a'),
        KeyCode::KeyB => Key::Character('b'),
        KeyCode::KeyC => Key::Character('c'),
        KeyCode::KeyD => Key::Character('d'),
        KeyCode::KeyE => Key::Character('e'),
        KeyCode::KeyF => Key::Character('f'),
        KeyCode::KeyG => Key::Character('g'),
        KeyCode::KeyH => Key::Character('h'),
        KeyCode::KeyI => Key::Character('i'),
        KeyCode::KeyJ => Key::Character('j'),
        KeyCode::KeyK => Key::Character('k'),
        KeyCode::KeyL => Key::Character('l'),
        KeyCode::KeyM => Key::Character('m'),
        KeyCode::KeyN => Key::Character('n'),
        KeyCode::KeyO => Key::Character('o'),
        KeyCode::KeyP => Key::Character('p'),
        KeyCode::KeyQ => Key::Character('q'),
        KeyCode::KeyR => Key::Character('r'),
        KeyCode::KeyS => Key::Character('s'),
        KeyCode::KeyT => Key::Character('t'),
        KeyCode::KeyU => Key::Character('u'),
        KeyCode::KeyV => Key::Character('v'),
        KeyCode::KeyW => Key::Character('w'),
        KeyCode::KeyX => Key::Character('x'),
        KeyCode::KeyY => Key::Character('y'),
        KeyCode::KeyZ => Key::Character('z'),
        KeyCode::Space => Key::Character(' '),
        KeyCode::BracketLeft => Key::Character('['),
        KeyCode::F1 => Key::F(1),
        KeyCode::F2 => Key::F(2),
        KeyCode::F3 => Key::F(3),
        KeyCode::F4 => Key::F(4),
        KeyCode::F5 => Key::F(5),
        KeyCode::F6 => Key::F(6),
        KeyCode::F7 => Key::F(7),
        KeyCode::F8 => Key::F(8),
        KeyCode::F9 => Key::F(9),
        KeyCode::F10 => Key::F(10),
        KeyCode::F11 => Key::F(11),
        KeyCode::F12 => Key::F(12),
        _ => {
            if let winit::keyboard::Key::Character(c) = logical_key {
                c.chars().next().map(Key::Character).unwrap_or(Key::Unknown)
            } else {
                Key::Unknown
            }
        }
    }
}

/// Map a `PhysicalKey` + `Key` pair (as received from winit's `KeyboardInput` event)
/// to Aileron's `Key` enum.
///
/// This is the public entry point used by the event loop. It extracts the
/// `KeyCode` from the `PhysicalKey` and delegates to `map_winit_key`.
pub fn map_key(physical: PhysicalKey, logical: &winit::keyboard::Key) -> Key {
    match physical {
        PhysicalKey::Code(code) => map_winit_key(code, logical),
        // Unhandled: native key codes (e.g., macOS Fn layer)
        PhysicalKey::Unidentified(_) => Key::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use smol_str::SmolStr;
    use winit::keyboard::PhysicalKey;

    // Helper to construct a winit logical key from a character
    fn char_key(c: char) -> winit::keyboard::Key {
        let s: String = c.into();
        winit::keyboard::Key::Character(SmolStr::from(s))
    }

    fn empty_logical() -> winit::keyboard::Key {
        winit::keyboard::Key::Character(SmolStr::from(""))
    }

    fn unknown_logical() -> winit::keyboard::Key {
        winit::keyboard::Key::Unidentified(winit::keyboard::NativeKey::Unidentified)
    }

    // ─── Special keys ──────────────────────────────────────────────

    #[test]
    fn test_escape() {
        assert_eq!(
            map_winit_key(KeyCode::Escape, &unknown_logical()),
            Key::Escape
        );
    }

    #[test]
    fn test_enter() {
        assert_eq!(
            map_winit_key(KeyCode::Enter, &unknown_logical()),
            Key::Enter
        );
    }

    #[test]
    fn test_backspace() {
        assert_eq!(
            map_winit_key(KeyCode::Backspace, &unknown_logical()),
            Key::Backspace
        );
    }

    #[test]
    fn test_tab() {
        assert_eq!(map_winit_key(KeyCode::Tab, &unknown_logical()), Key::Tab);
    }

    // ─── Arrow keys ────────────────────────────────────────────────

    #[test]
    fn test_arrow_keys() {
        assert_eq!(map_winit_key(KeyCode::ArrowUp, &unknown_logical()), Key::Up);
        assert_eq!(
            map_winit_key(KeyCode::ArrowDown, &unknown_logical()),
            Key::Down
        );
        assert_eq!(
            map_winit_key(KeyCode::ArrowLeft, &unknown_logical()),
            Key::Left
        );
        assert_eq!(
            map_winit_key(KeyCode::ArrowRight, &unknown_logical()),
            Key::Right
        );
    }

    // ─── Home / End / PageUp / PageDown ───────────────────────────

    #[test]
    fn test_navigation_keys() {
        assert_eq!(map_winit_key(KeyCode::Home, &unknown_logical()), Key::Home);
        assert_eq!(map_winit_key(KeyCode::End, &unknown_logical()), Key::End);
        assert_eq!(
            map_winit_key(KeyCode::PageUp, &unknown_logical()),
            Key::PageUp
        );
        assert_eq!(
            map_winit_key(KeyCode::PageDown, &unknown_logical()),
            Key::PageDown
        );
    }

    // ─── A-Z mapping (layout-independent via physical key) ─────────

    #[test]
    fn test_az_lowercase() {
        let letters = "abcdefghijklmnopqrstuvwxyz";
        let codes = [
            KeyCode::KeyA,
            KeyCode::KeyB,
            KeyCode::KeyC,
            KeyCode::KeyD,
            KeyCode::KeyE,
            KeyCode::KeyF,
            KeyCode::KeyG,
            KeyCode::KeyH,
            KeyCode::KeyI,
            KeyCode::KeyJ,
            KeyCode::KeyK,
            KeyCode::KeyL,
            KeyCode::KeyM,
            KeyCode::KeyN,
            KeyCode::KeyO,
            KeyCode::KeyP,
            KeyCode::KeyQ,
            KeyCode::KeyR,
            KeyCode::KeyS,
            KeyCode::KeyT,
            KeyCode::KeyU,
            KeyCode::KeyV,
            KeyCode::KeyW,
            KeyCode::KeyX,
            KeyCode::KeyY,
            KeyCode::KeyZ,
        ];

        for (c, code) in letters.chars().zip(codes.iter()) {
            assert_eq!(
                map_winit_key(*code, &unknown_logical()),
                Key::Character(c),
                "KeyCode::{:?} should map to Key::Character('{c}')",
                code
            );
        }
    }

    // ─── Space and bracket ────────────────────────────────────────

    #[test]
    fn test_space() {
        assert_eq!(
            map_winit_key(KeyCode::Space, &unknown_logical()),
            Key::Character(' ')
        );
    }

    #[test]
    fn test_bracket_left() {
        assert_eq!(
            map_winit_key(KeyCode::BracketLeft, &unknown_logical()),
            Key::Character('[')
        );
    }

    // ─── Function keys F1–F12 ─────────────────────────────────────

    #[test]
    fn test_function_keys() {
        for i in 1..=12u8 {
            let code = match i {
                1 => KeyCode::F1,
                2 => KeyCode::F2,
                3 => KeyCode::F3,
                4 => KeyCode::F4,
                5 => KeyCode::F5,
                6 => KeyCode::F6,
                7 => KeyCode::F7,
                8 => KeyCode::F8,
                9 => KeyCode::F9,
                10 => KeyCode::F10,
                11 => KeyCode::F11,
                12 => KeyCode::F12,
                _ => unreachable!(),
            };
            assert_eq!(
                map_winit_key(code, &unknown_logical()),
                Key::F(i),
                "F{i} should map to Key::F({i})"
            );
        }
    }

    // ─── Unknown / fallback ────────────────────────────────────────

    #[test]
    fn test_unknown_physical_key_returns_unknown() {
        // A KeyCode that doesn't match any arm, with a non-character logical key
        assert_eq!(
            map_winit_key(KeyCode::Convert, &unknown_logical()),
            Key::Unknown
        );
    }

    #[test]
    fn test_fallback_to_logical_key_character() {
        // An unmapped physical code with a character logical key should use the character
        assert_eq!(
            map_winit_key(KeyCode::Convert, &char_key('ü')),
            Key::Character('ü')
        );
    }

    #[test]
    fn test_fallback_empty_logical_key() {
        // An unmapped physical code with an empty character string
        assert_eq!(
            map_winit_key(KeyCode::Convert, &empty_logical()),
            Key::Unknown
        );
    }

    // ─── map_key (public entry point) ─────────────────────────────

    #[test]
    fn test_map_key_with_code() {
        assert_eq!(
            map_key(PhysicalKey::Code(KeyCode::KeyJ), &char_key('j')),
            Key::Character('j')
        );
    }

    #[test]
    fn test_map_key_with_unidentified() {
        assert_eq!(
            map_key(
                PhysicalKey::Unidentified(winit::keyboard::NativeKeyCode::Unidentified),
                &unknown_logical()
            ),
            Key::Unknown
        );
    }

    // ─── Physical key is layout-independent ───────────────────────

    #[test]
    fn test_layout_independence() {
        // Even if the logical key says 'h' (because user has Dvorak layout),
        // the physical KeyCode::KeyJ should still map to 'j'
        assert_eq!(
            map_key(PhysicalKey::Code(KeyCode::KeyJ), &char_key('h')),
            Key::Character('j')
        );
    }
}
