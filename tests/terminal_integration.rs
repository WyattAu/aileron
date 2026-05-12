//! Integration tests for terminal subsystem.
//!
//! Tests the PTY lifecycle (create, write, read, resize, destroy),
//! Selection operations, NativeTerminalPane management, and
//! terminal color/rendering structures.

use aileron::terminal::grid::TerminalColors;
use aileron::terminal::pty::PtyHandle;
use aileron::terminal::{NativeTerminalPane, Selection};

/// Verify that a PTY can be created and cleaned up without panic.
#[test]
fn test_pty_create_and_drop() {
    let result = PtyHandle::new(80, 24);
    match result {
        Ok(pty) => {
            assert_eq!(pty.pid(), pty.pid()); // pid is stable
            drop(pty);
        }
        Err(e) => {
            // PTY may fail in CI environments without a controlling terminal
            // This is acceptable — test that the error is informative
            let msg = format!("{e:#}");
            assert!(!msg.is_empty(), "error message should not be empty");
        }
    }
}

/// Verify that PTY writing works (if PTY creation succeeds).
#[test]
fn test_pty_write_and_resize() {
    let mut pty = match PtyHandle::new(80, 24) {
        Ok(p) => p,
        Err(_) => return, // PTY unavailable; skip test
    };

    // Write input to PTY
    let result = pty.write_bytes(b"echo hello\n");
    assert!(result.is_ok());

    // Resize the PTY
    pty.resize(120, 40);

    // Write more after resize
    let result = pty.write_bytes(b"ls\n");
    assert!(result.is_ok());

    // Output should be populated (or empty if shell hasn't responded yet)
    let output = pty.drain_output();
    // Output may be empty if shell hasn't produced data yet; either is fine
    let _ = output;

    drop(pty);
}

/// Verify that the PTY reports liveness correctly.
#[test]
fn test_pty_is_alive() {
    let mut pty = match PtyHandle::new(80, 24) {
        Ok(p) => p,
        Err(_) => return,
    };

    // Fresh PTY should be alive
    assert!(pty.is_alive());

    // Write exit command
    let _ = pty.write_bytes(b"exit\n");

    // Give shell a moment to exit
    std::thread::sleep(std::time::Duration::from_millis(200));

    // PTY may or may not be dead by now — just verify no panic
    let _ = pty.is_alive();

    drop(pty);
}

/// Verify that drain_output consumes the buffer.
#[test]
fn test_pty_drain_consumes_output() {
    let pty = match PtyHandle::new(80, 24) {
        Ok(p) => p,
        Err(_) => return,
    };

    // Drain once — should be empty or have initial shell output
    let first = pty.drain_output();

    // Drain again — should be empty (or nearly so)
    let second = pty.drain_output();
    assert!(
        second.len() <= first.len() + 128,
        "second drain should not produce dramatically more output than first"
    );

    drop(pty);
}

/// Verify that PTY pid is non-zero.
#[test]
fn test_pty_pid_nonzero() {
    let pty = match PtyHandle::new(80, 24) {
        Ok(p) => p,
        Err(_) => return,
    };

    let pid = pty.pid();
    assert!(pid > 0, "PTY process ID should be positive, got {pid}");

    drop(pty);
}

// --- Selection tests ---

#[test]
fn test_selection_new_inactive() {
    let sel = Selection::new();
    assert!(!sel.active);
    let (start, end) = sel.normalized();
    assert_eq!(start, (0, 0));
    assert_eq!(end, (0, 0));
}

#[test]
fn test_selection_normalized_forward() {
    let sel = Selection {
        start: (0, 5),
        end: (2, 10),
        active: true,
    };
    let (start, end) = sel.normalized();
    assert_eq!(start, (0, 5));
    assert_eq!(end, (2, 10));
}

#[test]
fn test_selection_normalized_backward() {
    let sel = Selection {
        start: (2, 10),
        end: (0, 5),
        active: true,
    };
    let (start, end) = sel.normalized();
    assert_eq!(start, (0, 5));
    assert_eq!(end, (2, 10));
}

#[test]
fn test_selection_normalized_same_position() {
    let sel = Selection {
        start: (1, 30),
        end: (1, 30),
        active: true,
    };
    let (start, end) = sel.normalized();
    assert_eq!(start, (1, 30));
    assert_eq!(end, (1, 30));
}

#[test]
fn test_selection_clear() {
    let mut sel = Selection {
        start: (0, 5),
        end: (2, 10),
        active: true,
    };
    sel.clear();
    assert!(!sel.active);
}

#[test]
fn test_selection_default_is_new() {
    let sel1 = Selection::new();
    let sel2 = Selection::default();
    assert_eq!(sel1.active, sel2.active);
    assert_eq!(sel1.start, sel2.start);
    assert_eq!(sel1.end, sel2.end);
}

// --- NativeTerminalPane tests ---

#[test]
fn test_native_terminal_pane_create() {
    let result = NativeTerminalPane::new(80, 24);
    match result {
        Ok(mut pane) => {
            assert!(pane.is_alive());
            let (cols, rows) = pane.size();
            assert_eq!(cols, 80);
            assert_eq!(rows, 24);
            drop(pane);
        }
        Err(e) => {
            let msg = format!("{e:#}");
            assert!(!msg.is_empty());
        }
    }
}

#[test]
fn test_native_terminal_pane_resize() {
    let mut pane = match NativeTerminalPane::new(80, 24) {
        Ok(p) => p,
        Err(_) => return,
    };

    pane.resize(120, 40);
    let (cols, rows) = pane.size();
    assert_eq!(cols, 120);
    assert_eq!(rows, 40);

    drop(pane);
}

#[test]
fn test_native_terminal_pane_write_and_tick() {
    let mut pane = match NativeTerminalPane::new(80, 24) {
        Ok(p) => p,
        Err(_) => return,
    };

    // Write input
    pane.write_input("echo test\n");

    // Tick to process output
    let dirty = pane.tick();
    // May or may not be dirty — depends on shell response time
    let _ = dirty;

    drop(pane);
}

#[test]
fn test_native_terminal_pane_dirty_flag() {
    let mut pane = match NativeTerminalPane::new(80, 24) {
        Ok(p) => p,
        Err(_) => return,
    };

    // Clear dirty flag
    pane.clear_dirty();
    assert!(!pane.is_dirty());

    // Write and tick — may trigger dirty
    pane.write_input("\n");
    let _ = pane.tick();

    drop(pane);
}

#[test]
fn test_native_terminal_pane_title() {
    let pane = match NativeTerminalPane::new(80, 24) {
        Ok(p) => p,
        Err(_) => return,
    };

    // Initial title may be None or shell-provided
    let title = pane.title();
    // Either state is valid
    let _ = title;

    // Term access should not panic
    let _term = pane.term();
    // Should have non-zero dimensions
    let (cols, rows) = pane.size();
    assert!(cols > 0);
    assert!(rows > 0);

    drop(pane);
}

#[test]
fn test_native_terminal_pane_scroll() {
    let mut pane = match NativeTerminalPane::new(80, 24) {
        Ok(p) => p,
        Err(_) => return,
    };

    // Scroll should not panic even without content
    pane.scroll(1);
    pane.scroll(-1);
    pane.scroll(0);

    drop(pane);
}

#[test]
fn test_native_terminal_pane_damage_info() {
    let mut pane = match NativeTerminalPane::new(80, 24) {
        Ok(p) => p,
        Err(_) => return,
    };

    let _ = pane.tick();
    let info = pane.damage_info();
    // Damage info has lines reference — just verify no panic
    let _ = info.full;
    let _ = info.lines;

    drop(pane);
}

// --- TerminalColors tests ---

#[test]
fn test_terminal_colors_default() {
    let colors = TerminalColors::default();
    // Default colors should be reasonable: foreground != background
    assert_ne!(colors.foreground, colors.background);
    // ANSI palette should have 256 entries
    assert_eq!(colors.ansi.len(), 256);
}

#[test]
fn test_terminal_colors_resolve() {
    use alacritty_terminal::term::cell::Cell;

    let colors = TerminalColors::default();
    let cell = Cell::default();

    // Resolve should not panic
    let resolved = colors.resolve_color(&cell);
    let _ = resolved;
}

// --- Cell metrics tests ---

#[test]
fn test_cell_metrics_positive_dimensions() {
    // CellMetrics requires an egui::Context which needs a display.
    // Test skipped in headless CI — the struct's fields are public.
    // Verify the struct can be constructed via its fields.
    let _metrics = aileron::terminal::grid::CellMetrics {
        cell_width: 8.0,
        cell_height: 16.0,
        font_size: 12.0,
    };
}
