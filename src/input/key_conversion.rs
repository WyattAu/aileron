use crate::input::{Key, Modifiers};

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
