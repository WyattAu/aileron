use aileron::input::Modifiers;

pub(crate) fn key_to_js(key: &aileron::input::Key) -> (String, String) {
    match key {
        aileron::input::Key::Enter => ("Enter".into(), "Enter".into()),
        aileron::input::Key::Backspace => ("Backspace".into(), "Backspace".into()),
        aileron::input::Key::Tab => ("Tab".into(), "Tab".into()),
        aileron::input::Key::Escape => ("Escape".into(), "Escape".into()),
        aileron::input::Key::Up => ("ArrowUp".into(), "ArrowUp".into()),
        aileron::input::Key::Down => ("ArrowDown".into(), "ArrowDown".into()),
        aileron::input::Key::Left => ("ArrowLeft".into(), "ArrowLeft".into()),
        aileron::input::Key::Right => ("ArrowRight".into(), "ArrowRight".into()),
        aileron::input::Key::Home => ("Home".into(), "Home".into()),
        aileron::input::Key::End => ("End".into(), "End".into()),
        aileron::input::Key::PageUp => ("PageUp".into(), "PageUp".into()),
        aileron::input::Key::PageDown => ("PageDown".into(), "PageDown".into()),
        aileron::input::Key::F(n) => (format!("F{n}"), format!("F{n}")),
        _ => ("".into(), "".into()),
    }
}

/// Convert an aileron Key + modifiers to a terminal escape sequence.
/// This is the native terminal equivalent of key_to_js — it sends
/// the appropriate VT100/xterm escape sequence to the PTY.
pub(crate) fn key_to_escape_sequence(key: &aileron::input::Key, mods: Modifiers) -> String {
    use aileron::input::Key;

    let ctrl = mods.ctrl;
    let shift = mods.shift;
    let alt = mods.alt;

    // Control letter: Ctrl+A through Ctrl+Z → \x01 through \x1A
    if ctrl && let Key::Character(c) = key {
        let lower = c.to_ascii_lowercase();
        let byte = lower as u32;
        if (0x61..=0x7a).contains(&byte) {
            // a=0x61 → Ctrl+A = 0x01
            return String::from_utf8_lossy(&[(byte - 0x60) as u8]).to_string();
        }
    }

    // Alt+letter: ESC followed by the character
    if alt && let Key::Character(c) = key {
        return format!("\x1b{c}");
    }

    match key {
        Key::Enter => "\r".into(),
        Key::Backspace => "\x7f".into(), // DEL
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

/// Detect NVIDIA GPU by checking DRM subsystem vendor IDs.
/// Returns true if any card0..card9 reports vendor 0x10de (NVIDIA).
#[cfg(target_os = "linux")]
pub(crate) fn is_nvidia_gpu() -> bool {
    (0..=9).any(|i| {
        let path = format!("/sys/class/drm/card{i}/device/vendor");
        std::fs::read_to_string(&path)
            .map(|v| v.trim() == "0x10de")
            .unwrap_or(false)
    })
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn is_nvidia_gpu() -> bool {
    false
}
