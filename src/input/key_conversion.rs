use crate::input::Key;
#[cfg(feature = "terminal")]
use crate::input::Modifiers;

pub(crate) fn key_to_js(key: &Key) -> (String, String) {
    match key {
        Key::Enter => ("Enter".into(), "Enter".into()),
        Key::Backspace => ("Backspace".into(), "Backspace".into()),
        Key::Tab => ("Tab".into(), "Tab".into()),
        Key::Escape => ("Escape".into(), "Escape".into()),
        Key::Up => ("ArrowUp".into(), "ArrowUp".into()),
        Key::Down => ("ArrowDown".into(), "ArrowDown".into()),
        Key::Left => ("ArrowLeft".into(), "ArrowLeft".into()),
        Key::Right => ("ArrowRight".into(), "ArrowRight".into()),
        Key::Home => ("Home".into(), "Home".into()),
        Key::End => ("End".into(), "End".into()),
        Key::PageUp => ("PageUp".into(), "PageUp".into()),
        Key::PageDown => ("PageDown".into(), "PageDown".into()),
        Key::F(n) => (format!("F{n}"), format!("F{n}")),
        _ => ("".into(), "".into()),
    }
}

#[cfg(feature = "terminal")]
pub(crate) fn key_to_escape_sequence(key: &Key, mods: Modifiers) -> String {
    let ctrl = mods.ctrl;
    let shift = mods.shift;
    let alt = mods.alt;

    if ctrl && let Key::Character(c) = key {
        let lower = c.to_ascii_lowercase();
        let byte = lower as u32;
        if (0x61..=0x7a).contains(&byte) {
            return String::from_utf8_lossy(&[(byte - 0x60) as u8]).to_string();
        }
    }

    if alt && let Key::Character(c) = key {
        return format!("\x1b{c}");
    }

    match key {
        Key::Enter => "\r".into(),
        Key::Backspace => "\x7f".into(),
        Key::Tab => "\t".into(),
        Key::Escape => "\x1b".into(),
        Key::Up => {
            if shift {
                "\x1b[1;2A".into()
            } else {
                "\x1b[A".into()
            }
        }
        Key::Down => {
            if shift {
                "\x1b[1;2B".into()
            } else {
                "\x1b[B".into()
            }
        }
        Key::Right => {
            if shift {
                "\x1b[1;2C".into()
            } else {
                "\x1b[C".into()
            }
        }
        Key::Left => {
            if shift {
                "\x1b[1;2D".into()
            } else {
                "\x1b[D".into()
            }
        }
        Key::Home => "\x1b[H".into(),
        Key::End => "\x1b[F".into(),
        Key::PageUp => "\x1b[5~".into(),
        Key::PageDown => "\x1b[6~".into(),
        Key::F(1) => "\x1bOP".into(),
        Key::F(2) => "\x1bOQ".into(),
        Key::F(3) => "\x1bOR".into(),
        Key::F(4) => "\x1bOS".into(),
        Key::F(5) => "\x1b[15~".into(),
        Key::F(6) => "\x1b[17~".into(),
        Key::F(7) => "\x1b[18~".into(),
        Key::F(8) => "\x1b[19~".into(),
        Key::F(9) => "\x1b[20~".into(),
        Key::F(10) => "\x1b[21~".into(),
        Key::F(11) => "\x1b[23~".into(),
        Key::F(12) => "\x1b[24~".into(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::Modifiers;

    fn shift_mods() -> Modifiers {
        Modifiers {
            ctrl: false,
            shift: true,
            alt: false,
            super_key: false,
        }
    }

    fn alt_mods() -> Modifiers {
        Modifiers {
            ctrl: false,
            shift: false,
            alt: true,
            super_key: false,
        }
    }

    // ─── key_to_js tests ────────────────────────────────────────

    #[test]
    fn test_js_names_special_keys() {
        let cases = [
            (Key::Enter, "Enter"),
            (Key::Escape, "Escape"),
            (Key::Backspace, "Backspace"),
            (Key::Tab, "Tab"),
            (Key::Up, "ArrowUp"),
            (Key::Down, "ArrowDown"),
            (Key::Left, "ArrowLeft"),
            (Key::Right, "ArrowRight"),
            (Key::Home, "Home"),
            (Key::End, "End"),
            (Key::PageUp, "PageUp"),
            (Key::PageDown, "PageDown"),
        ];
        for (key, expected) in &cases {
            let (code, key_name) = key_to_js(key);
            assert_eq!(code, *expected, "key_to_js code mismatch for {key:?}");
            assert_eq!(
                key_name, *expected,
                "key_to_js key_name mismatch for {key:?}"
            );
        }
    }

    #[test]
    fn test_js_names_function_keys() {
        for n in 1..=12u8 {
            let (code, key_name) = key_to_js(&Key::F(n));
            let expected = format!("F{n}");
            assert_eq!(code, expected);
            assert_eq!(key_name, expected);
        }
    }

    #[test]
    fn test_js_names_character_returns_empty() {
        let (code, key_name) = key_to_js(&Key::Character('a'));
        assert!(code.is_empty());
        assert!(key_name.is_empty());
    }

    #[test]
    fn test_js_names_unknown_returns_empty() {
        let (code, key_name) = key_to_js(&Key::Unknown);
        assert!(code.is_empty());
        assert!(key_name.is_empty());
    }

    // ─── key_to_escape_sequence tests ───────────────────────────

    #[test]
    fn test_escape_special_keys() {
        assert_eq!(
            key_to_escape_sequence(&Key::Escape, Modifiers::none()),
            "\x1b"
        );
        assert_eq!(key_to_escape_sequence(&Key::Enter, Modifiers::none()), "\r");
        assert_eq!(
            key_to_escape_sequence(&Key::Backspace, Modifiers::none()),
            "\x7f"
        );
        assert_eq!(key_to_escape_sequence(&Key::Tab, Modifiers::none()), "\t");
    }

    #[test]
    fn test_escape_arrows() {
        assert_eq!(
            key_to_escape_sequence(&Key::Up, Modifiers::none()),
            "\x1b[A"
        );
        assert_eq!(
            key_to_escape_sequence(&Key::Down, Modifiers::none()),
            "\x1b[B"
        );
        assert_eq!(
            key_to_escape_sequence(&Key::Right, Modifiers::none()),
            "\x1b[C"
        );
        assert_eq!(
            key_to_escape_sequence(&Key::Left, Modifiers::none()),
            "\x1b[D"
        );
    }

    #[test]
    fn test_escape_shift_arrows() {
        assert_eq!(key_to_escape_sequence(&Key::Up, shift_mods()), "\x1b[1;2A");
        assert_eq!(
            key_to_escape_sequence(&Key::Down, shift_mods()),
            "\x1b[1;2B"
        );
        assert_eq!(
            key_to_escape_sequence(&Key::Right, shift_mods()),
            "\x1b[1;2C"
        );
        assert_eq!(
            key_to_escape_sequence(&Key::Left, shift_mods()),
            "\x1b[1;2D"
        );
    }

    #[test]
    fn test_escape_navigation_keys() {
        assert_eq!(
            key_to_escape_sequence(&Key::Home, Modifiers::none()),
            "\x1b[H"
        );
        assert_eq!(
            key_to_escape_sequence(&Key::End, Modifiers::none()),
            "\x1b[F"
        );
        assert_eq!(
            key_to_escape_sequence(&Key::PageUp, Modifiers::none()),
            "\x1b[5~"
        );
        assert_eq!(
            key_to_escape_sequence(&Key::PageDown, Modifiers::none()),
            "\x1b[6~"
        );
    }

    #[test]
    fn test_escape_function_keys() {
        assert_eq!(
            key_to_escape_sequence(&Key::F(1), Modifiers::none()),
            "\x1bOP"
        );
        assert_eq!(
            key_to_escape_sequence(&Key::F(2), Modifiers::none()),
            "\x1bOQ"
        );
        assert_eq!(
            key_to_escape_sequence(&Key::F(3), Modifiers::none()),
            "\x1bOR"
        );
        assert_eq!(
            key_to_escape_sequence(&Key::F(4), Modifiers::none()),
            "\x1bOS"
        );
        assert_eq!(
            key_to_escape_sequence(&Key::F(5), Modifiers::none()),
            "\x1b[15~"
        );
        assert_eq!(
            key_to_escape_sequence(&Key::F(6), Modifiers::none()),
            "\x1b[17~"
        );
        assert_eq!(
            key_to_escape_sequence(&Key::F(7), Modifiers::none()),
            "\x1b[18~"
        );
        assert_eq!(
            key_to_escape_sequence(&Key::F(8), Modifiers::none()),
            "\x1b[19~"
        );
        assert_eq!(
            key_to_escape_sequence(&Key::F(9), Modifiers::none()),
            "\x1b[20~"
        );
        assert_eq!(
            key_to_escape_sequence(&Key::F(10), Modifiers::none()),
            "\x1b[21~"
        );
        assert_eq!(
            key_to_escape_sequence(&Key::F(11), Modifiers::none()),
            "\x1b[23~"
        );
        assert_eq!(
            key_to_escape_sequence(&Key::F(12), Modifiers::none()),
            "\x1b[24~"
        );
    }

    #[test]
    fn test_escape_ctrl_letters() {
        let ctrl = Modifiers::ctrl();
        assert_eq!(key_to_escape_sequence(&Key::Character('a'), ctrl), "\x01");
        assert_eq!(key_to_escape_sequence(&Key::Character('b'), ctrl), "\x02");
        assert_eq!(key_to_escape_sequence(&Key::Character('c'), ctrl), "\x03");
        assert_eq!(key_to_escape_sequence(&Key::Character('z'), ctrl), "\x1a");
    }

    #[test]
    fn test_escape_ctrl_uppercase_maps_to_lowercase() {
        let ctrl = Modifiers::ctrl();
        // Ctrl+A and Ctrl+a should produce the same sequence
        let lower = key_to_escape_sequence(&Key::Character('a'), ctrl);
        let upper = key_to_escape_sequence(&Key::Character('A'), ctrl);
        assert_eq!(lower, upper, "Ctrl should be case-insensitive");
    }

    #[test]
    fn test_escape_ctrl_non_alpha_returns_empty() {
        let ctrl = Modifiers::ctrl();
        // Ctrl+0, Ctrl+1, etc. are not a-z, so they fall through to the match
        let result = key_to_escape_sequence(&Key::Character('0'), ctrl);
        assert!(result.is_empty(), "Ctrl+0 should return empty sequence");
    }

    #[test]
    fn test_escape_alt_prefix() {
        assert_eq!(
            key_to_escape_sequence(&Key::Character('a'), alt_mods()),
            "\x1ba"
        );
        assert_eq!(
            key_to_escape_sequence(&Key::Character('1'), alt_mods()),
            "\x1b1"
        );
        assert_eq!(
            key_to_escape_sequence(&Key::Character('Z'), alt_mods()),
            "\x1bZ"
        );
    }

    #[test]
    fn test_escape_unknown_and_character_return_empty() {
        assert_eq!(key_to_escape_sequence(&Key::Unknown, Modifiers::none()), "");
        assert_eq!(
            key_to_escape_sequence(&Key::Character('x'), Modifiers::none()),
            ""
        );
    }
}
