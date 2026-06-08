#[cfg(feature = "terminal")]
use aileron::input::Modifiers;

/// Convert an aileron Key + modifiers to a terminal escape sequence.
/// This is the native terminal equivalent of key_to_js — it sends
/// the appropriate VT100/xterm escape sequence to the PTY.
#[cfg(feature = "terminal")]
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

#[allow(dead_code)]
pub(crate) fn screen_to_pane_local(
    pos: (f32, f32),
    rect: &aileron::wm::Rect,
    tab_sidebar_right: bool,
    tab_layout_is_sidebar: bool,
    tab_sidebar_width: f64,
    pane_width: f64,
    pane_height: f64,
) -> Option<(f64, f64)> {
    const STATUS_BAR_HEIGHT: f32 = 32.0;
    let top_offset = STATUS_BAR_HEIGHT;
    let sidebar_offset = if tab_layout_is_sidebar && !tab_sidebar_right {
        tab_sidebar_width
    } else {
        0.0
    };
    let local_x = pos.0 - rect.x as f32 - sidebar_offset as f32;
    let local_y = pos.1 - rect.y as f32 - top_offset;
    if local_x >= 0.0
        && local_y >= 0.0
        && local_x < pane_width as f32
        && local_y < pane_height as f32
    {
        Some((local_x as f64, local_y as f64))
    } else {
        None
    }
}

pub(crate) fn clear_hints_js() -> &'static str {
    r#"
        (function() {
            var style = document.getElementById('__aileron_hints');
            if (style) style.remove();
            document.querySelectorAll('[data-aileron-hint]').forEach(el => {
                el.removeAttribute('data-aileron-hint');
            });
        })();
    "#
}

pub(crate) fn hint_click_js(hint_buf: &str, new_tab: bool) -> String {
    if new_tab {
        format!(
            "(function() {{ \
                var el = document.querySelector('[data-aileron-hint=\"{hint_buf}\"]'); \
                if (el && el.href) {{ window.open(el.href, '_blank'); window.ipc.postMessage(JSON.stringify({{t:'hint-clicked'}})); return; }} \
                if (el) {{ el.click(); window.ipc.postMessage(JSON.stringify({{t:'hint-clicked'}})); return; }} \
                var all = document.querySelectorAll('[data-aileron-hint]'); \
                var matches = []; \
                all.forEach(function(e) {{ \
                    if (e.getAttribute('data-aileron-hint').startsWith('{hint_buf}')) matches.push(e); \
                }}); \
                if (matches.length === 1 && matches[0].href) {{ window.open(matches[0].href, '_blank'); window.ipc.postMessage(JSON.stringify({{t:'hint-clicked'}})); return; }} \
                if (matches.length === 1) {{ matches[0].click(); window.ipc.postMessage(JSON.stringify({{t:'hint-clicked'}})); return; }} \
            }})()"
        )
    } else {
        format!(
            "(function() {{ \
                var el = document.querySelector('[data-aileron-hint=\"{hint_buf}\"]'); \
                if (el) {{ el.click(); window.ipc.postMessage(JSON.stringify({{t:'hint-clicked'}})); return; }} \
                var all = document.querySelectorAll('[data-aileron-hint]'); \
                var matches = []; \
                all.forEach(function(e) {{ \
                    if (e.getAttribute('data-aileron-hint').startsWith('{hint_buf}')) matches.push(e); \
                }}); \
                if (matches.length === 1) {{ matches[0].click(); window.ipc.postMessage(JSON.stringify({{t:'hint-clicked'}})); return; }} \
            }})()"
        )
    }
}
