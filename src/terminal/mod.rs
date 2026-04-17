//! Native terminal emulator using alacritty_terminal + egui rendering.
//!
//! Replaces the xterm.js-in-webview approach with a pure Rust terminal:
//! - PTY managed by portable_pty (existing)
//! - VT sequence parsing by alacritty_terminal::Term + vte::ansi::Processor
//! - Rendering via egui::Painter (direct text drawing)
//!
//! Architecture:
//!   winit key event → write_input() → PTY master
//!   PTY slave output → read thread → vte Processor → Term grid
//!   Term grid → egui Painter (each frame, dirty cells only)

pub mod grid;
pub mod pty;
pub mod render;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use alacritty_terminal::term::TermDamage;

pub struct DamageInfo<'a> {
    pub full: bool,
    pub lines: &'a [(usize, usize, usize)],
}

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::Processor;
use tracing::{info, warn};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Selection {
    pub start: (i32, usize),
    pub end: (i32, usize),
    pub active: bool,
}

impl Selection {
    pub fn new() -> Self {
        Self {
            start: (0, 0),
            end: (0, 0),
            active: false,
        }
    }

    pub fn normalized(&self) -> ((i32, usize), (i32, usize)) {
        if !self.active {
            return ((0, 0), (0, 0));
        }
        if (self.start.0, self.start.1) <= (self.end.0, self.end.1) {
            (self.start, self.end)
        } else {
            (self.end, self.start)
        }
    }

    pub fn clear(&mut self) {
        self.active = false;
    }
}

impl Default for Selection {
    fn default() -> Self {
        Self::new()
    }
}

use crate::terminal::pty::PtyHandle;

/// Event listener for alacritty_terminal.
/// Routes terminal events (PtyWrite, Title, Bell) back to the PTY or app state.
#[derive(Clone, Default)]
pub struct TermEventListener {
    pty_write_tx: Arc<Mutex<Vec<u8>>>,
    title_tx: Arc<Mutex<Option<String>>>,
}

impl EventListener for TermEventListener {
    fn send_event(&self, event: Event) {
        match event {
            Event::PtyWrite(s) => {
                if let Ok(mut buf) = self.pty_write_tx.lock() {
                    buf.extend_from_slice(s.as_bytes());
                }
            }
            Event::Title(title) => {
                if let Ok(mut t) = self.title_tx.lock() {
                    *t = Some(title);
                }
            }
            Event::ResetTitle => {
                if let Ok(mut t) = self.title_tx.lock() {
                    *t = None;
                }
            }
            Event::Bell => {
                // TODO: visual bell support
            }
            _ => {
                // ClipboardStore, ClipboardLoad, etc. — handle later
            }
        }
    }
}

/// Dimensions wrapper for alacritty_terminal::Term.
/// Implements the Dimensions trait required by Term::new() and Term::resize().
struct TermDimensions {
    cols: usize,
    screen_lines: usize,
    history_size: usize,
}

impl TermDimensions {
    fn new(cols: u16, rows: u16, history_size: usize) -> Self {
        Self {
            cols: cols as usize,
            screen_lines: rows as usize,
            history_size,
        }
    }
}

impl Dimensions for TermDimensions {
    fn total_lines(&self) -> usize {
        self.screen_lines + self.history_size
    }

    fn screen_lines(&self) -> usize {
        self.screen_lines
    }

    fn columns(&self) -> usize {
        self.cols
    }

    fn history_size(&self) -> usize {
        self.history_size
    }
}

/// A single native terminal pane.
///
/// Owns:
/// - PTY handle (spawning, writing, reading, resizing)
/// - alacritty_terminal::Term (VT state machine, grid, cursor)
/// - vte::ansi::Processor (feeds bytes into Term)
pub struct NativeTerminalPane {
    pty: PtyHandle,
    term: Term<TermEventListener>,
    parser: Processor,
    event_listener: TermEventListener,
    shutdown: Arc<AtomicBool>,
    title: Arc<Mutex<Option<String>>>,
    dirty: Arc<AtomicBool>,
    selection: Selection,
    selecting: bool,
    damage_lines: Vec<(usize, usize, usize)>,
    damage_full: bool,
}

impl NativeTerminalPane {
    /// Create a new terminal pane with the given grid size.
    pub fn new(cols: u16, rows: u16) -> anyhow::Result<Self> {
        let history_size = 10_000;
        let pty = PtyHandle::new(cols, rows)?;

        let pty_write_tx: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
        let title_tx: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

        let event_listener = TermEventListener {
            pty_write_tx: pty_write_tx.clone(),
            title_tx: title_tx.clone(),
        };

        let config = Config {
            scrolling_history: history_size,
            ..Config::default()
        };

        let dimensions = TermDimensions::new(cols, rows, history_size);
        let term = Term::new(config, &dimensions, event_listener.clone());
        let parser = Processor::new();

        Ok(Self {
            pty,
            term,
            parser,
            event_listener,
            shutdown: Arc::new(AtomicBool::new(false)),
            title: title_tx,
            dirty: Arc::new(AtomicBool::new(true)),
            selection: Selection::new(),
            selecting: false,
            damage_lines: Vec::new(),
            damage_full: true,
        })
    }

    /// Write input bytes to the PTY (user keystrokes, paste, etc.).
    pub fn write_input(&mut self, data: &str) {
        if let Err(e) = self.pty.write(data) {
            warn!("failed to write to PTY: {e}");
        }
    }

    /// Poll PTY output and feed it through the VT parser.
    /// Must be called from the main thread (Term is !Send).
    /// Returns true if new data was processed (grid may have changed).
    pub fn tick(&mut self) -> bool {
        // Drain pending output from PTY read thread
        let bytes = self.pty.drain_output();

        if bytes.is_empty() {
            // Still check for PtyWrite events (responses from Term back to PTY)
            let pty_write = {
                let mut buf = self.event_listener.pty_write_tx.lock().unwrap();
                std::mem::take(&mut *buf)
            };
            if !pty_write.is_empty() {
                let _ = self.pty.write_bytes(&pty_write);
            }
            return false;
        }

        // Feed bytes through VTE parser → Term state machine
        self.parser.advance(&mut self.term, &bytes);

        self.collect_damage();
        self.term.reset_damage();

        // Drain any PtyWrite responses (e.g., device attribute replies)
        let pty_write = {
            let mut buf = self.event_listener.pty_write_tx.lock().unwrap();
            std::mem::take(&mut *buf)
        };
        if !pty_write.is_empty() {
            let _ = self.pty.write_bytes(&pty_write);
        }

        self.dirty.store(true, Ordering::Relaxed);
        true
    }

    /// Check if the terminal content has changed since last render.
    pub fn is_dirty(&self) -> bool {
        self.dirty.load(Ordering::Relaxed)
    }

    /// Mark the terminal as clean (after rendering).
    pub fn clear_dirty(&self) {
        self.dirty.store(false, Ordering::Relaxed);
    }

    fn collect_damage(&mut self) {
        self.damage_lines.clear();
        self.damage_full = false;
        match self.term.damage() {
            TermDamage::Full => {
                self.damage_full = true;
            }
            TermDamage::Partial(iter) => {
                for bounds in iter {
                    self.damage_lines
                        .push((bounds.line, bounds.left, bounds.right));
                }
            }
        }
    }

    pub fn damage_info(&self) -> DamageInfo<'_> {
        DamageInfo {
            full: self.damage_full,
            lines: &self.damage_lines,
        }
    }

    /// Resize the terminal grid and PTY.
    pub fn resize(&mut self, cols: u16, rows: u16) {
        let dimensions = TermDimensions::new(cols, rows, 10_000);
        self.term.resize(dimensions);
        self.pty.resize(cols, rows);
        self.dirty.store(true, Ordering::Relaxed);
        self.damage_full = true;
        self.damage_lines.clear();
    }

    /// Get the terminal title (if set by the child process).
    pub fn title(&self) -> Option<String> {
        self.title.lock().unwrap().clone()
    }

    /// Get the current grid size.
    pub fn size(&self) -> (u16, u16) {
        (self.term.columns() as u16, self.term.screen_lines() as u16)
    }

    /// Check if the child process is still alive.
    pub fn is_alive(&mut self) -> bool {
        self.pty.is_alive()
    }

    /// Access the underlying Term for rendering.
    pub fn term(&self) -> &Term<TermEventListener> {
        &self.term
    }

    /// Access the underlying Term mutably.
    pub fn term_mut(&mut self) -> &mut Term<TermEventListener> {
        &mut self.term
    }

    /// Scroll the terminal view by `delta` lines (positive = up, negative = down).
    pub fn scroll(&mut self, delta: i32) {
        self.term.grid_mut().scroll_display(Scroll::Delta(delta));
        self.dirty.store(true, Ordering::Relaxed);
        self.damage_full = true;
        self.damage_lines.clear();
    }

    /// Get the current scrollback offset (0 = bottom of screen).
    pub fn scroll_offset(&self) -> usize {
        self.term.grid().display_offset()
    }

    pub fn start_selection(&mut self, line: i32, col: usize) {
        self.selection.start = (line, col);
        self.selection.end = (line, col);
        self.selection.active = true;
        self.selecting = true;
    }

    pub fn extend_selection(&mut self, line: i32, col: usize) {
        if self.selecting {
            self.selection.end = (line, col);
        }
    }

    pub fn end_selection(&mut self) {
        self.selecting = false;
    }

    pub fn selection_text(&self) -> Option<String> {
        if !self.selection.active {
            return None;
        }
        let ((start_line, start_col), (end_line, end_col)) = self.selection.normalized();
        let mut text = String::new();
        let cols = self.term.columns();
        for line in start_line..=end_line {
            let row_start = if line == start_line { start_col } else { 0 };
            let row_end = if line == end_line {
                end_col.min(cols.saturating_sub(1))
            } else {
                cols.saturating_sub(1)
            };
            for col in row_start..=row_end {
                let cell = &self.term.grid()[Line(line)][Column(col)];
                if cell.c != '\0' {
                    text.push(cell.c);
                }
            }
            if line < end_line {
                text.push('\n');
            }
        }
        if text.is_empty() {
            None
        } else {
            Some(text.trim_end().to_string())
        }
    }

    pub fn clear_selection(&mut self) {
        self.selection.clear();
        self.selecting = false;
    }

    pub fn selection(&self) -> &Selection {
        &self.selection
    }

    pub fn is_selecting(&self) -> bool {
        self.selecting
    }

    pub fn pixel_to_grid(
        &self,
        pixel_x: f32,
        pixel_y: f32,
        cell_width: f32,
        cell_height: f32,
    ) -> (i32, usize) {
        let col = (pixel_x / cell_width).floor().max(0.0) as usize;
        let row = (pixel_y / cell_height).floor().max(0.0) as usize;
        let display_offset = self.term.grid().display_offset() as i32;
        let grid_line = display_offset + row as i32;
        let clamped_col = col.min(self.term.columns().saturating_sub(1));
        (grid_line, clamped_col)
    }
}

impl Drop for NativeTerminalPane {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        info!("native terminal pane dropped");
    }
}

/// Manages all native terminal panes, keyed by pane UUID.
pub struct NativeTerminalManager {
    panes: HashMap<Uuid, NativeTerminalPane>,
}

impl NativeTerminalManager {
    pub fn new() -> Self {
        Self {
            panes: HashMap::new(),
        }
    }

    /// Create a new native terminal pane.
    pub fn create_terminal(
        &mut self,
        pane_id: Uuid,
        cols: u16,
        rows: u16,
    ) -> anyhow::Result<(u16, u16)> {
        let pane = NativeTerminalPane::new(cols, rows)?;
        let size = pane.size();
        self.panes.insert(pane_id, pane);
        Ok(size)
    }

    /// Write input to a specific terminal pane.
    pub fn write_input(&mut self, pane_id: &Uuid, data: &str) {
        if let Some(pane) = self.panes.get_mut(pane_id) {
            pane.write_input(data);
        }
    }

    /// Poll all terminals for new output. Call once per frame.
    pub fn tick_all(&mut self) {
        for pane in self.panes.values_mut() {
            pane.tick();
        }
    }

    /// Remove a terminal pane.
    pub fn remove(&mut self, pane_id: &Uuid) {
        self.panes.remove(pane_id);
    }

    /// Check if a pane ID is a native terminal.
    pub fn is_terminal(&self, pane_id: &Uuid) -> bool {
        self.panes.contains_key(pane_id)
    }

    /// Resize a terminal pane.
    pub fn resize(&mut self, pane_id: &Uuid, cols: u16, rows: u16) {
        if let Some(pane) = self.panes.get_mut(pane_id) {
            pane.resize(cols, rows);
        }
    }

    /// Get a terminal pane by ID.
    pub fn get(&self, pane_id: &Uuid) -> Option<&NativeTerminalPane> {
        self.panes.get(pane_id)
    }

    /// Get a terminal pane by ID (mutable).
    pub fn get_mut(&mut self, pane_id: &Uuid) -> Option<&mut NativeTerminalPane> {
        self.panes.get_mut(pane_id)
    }

    /// Get all terminal pane IDs.
    pub fn pane_ids(&self) -> Vec<Uuid> {
        self.panes.keys().copied().collect()
    }

    /// Scroll a terminal pane.
    pub fn scroll(&mut self, pane_id: &Uuid, delta: i32) {
        if let Some(pane) = self.panes.get_mut(pane_id) {
            pane.scroll(delta);
        }
    }
}

impl Default for NativeTerminalManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_native_terminal_create_and_remove() {
        let mut manager = NativeTerminalManager::new();
        let id = Uuid::new_v4();

        assert!(!manager.is_terminal(&id));
        assert!(manager.pane_ids().is_empty());

        manager.create_terminal(id, 80, 24).unwrap();
        assert!(manager.is_terminal(&id));
        assert_eq!(manager.pane_ids().len(), 1);

        manager.remove(&id);
        assert!(!manager.is_terminal(&id));
        assert!(manager.pane_ids().is_empty());
    }

    #[test]
    fn test_native_terminal_write_and_size() {
        let mut manager = NativeTerminalManager::new();
        let id = Uuid::new_v4();

        manager.create_terminal(id, 80, 24).unwrap();
        let pane = manager.get(&id).unwrap();
        assert_eq!(pane.size(), (80, 24));
        assert!(pane.is_dirty()); // New terminals start dirty
    }

    #[test]
    fn test_native_terminal_resize() {
        let mut manager = NativeTerminalManager::new();
        let id = Uuid::new_v4();

        manager.create_terminal(id, 80, 24).unwrap();
        manager.resize(&id, 120, 40);
        let pane = manager.get(&id).unwrap();
        assert_eq!(pane.size(), (120, 40));
    }

    #[test]
    fn test_native_terminal_unknown_pane() {
        let mut manager = NativeTerminalManager::new();
        let unknown_id = Uuid::new_v4();

        // These should all be no-ops
        manager.write_input(&unknown_id, "test");
        manager.resize(&unknown_id, 80, 24);
        manager.scroll(&unknown_id, 5);
        manager.remove(&unknown_id);
        manager.tick_all();
    }

    #[test]
    fn test_term_dimensions() {
        let dims = TermDimensions::new(80, 24, 10_000);
        assert_eq!(dims.columns(), 80);
        assert_eq!(dims.screen_lines(), 24);
        assert_eq!(dims.total_lines(), 10_024);
        assert_eq!(dims.history_size(), 10_000);
    }

    #[test]
    fn test_selection_normalized_forward() {
        let sel = Selection {
            start: (2, 5),
            end: (4, 10),
            active: true,
        };
        let (top, bottom) = sel.normalized();
        assert_eq!(top, (2, 5));
        assert_eq!(bottom, (4, 10));
    }

    #[test]
    fn test_selection_normalized_backward() {
        let sel = Selection {
            start: (4, 10),
            end: (2, 5),
            active: true,
        };
        let (top, bottom) = sel.normalized();
        assert_eq!(top, (2, 5));
        assert_eq!(bottom, (4, 10));
    }

    #[test]
    fn test_selection_normalized_inactive() {
        let sel = Selection {
            start: (4, 10),
            end: (2, 5),
            active: false,
        };
        let (top, bottom) = sel.normalized();
        assert_eq!(top, (0, 0));
        assert_eq!(bottom, (0, 0));
    }

    #[test]
    fn test_selection_clear() {
        let mut sel = Selection {
            start: (1, 2),
            end: (3, 4),
            active: true,
        };
        sel.clear();
        assert!(!sel.active);
    }

    #[test]
    fn test_selection_same_point() {
        let sel = Selection {
            start: (5, 10),
            end: (5, 10),
            active: true,
        };
        let (top, bottom) = sel.normalized();
        assert_eq!(top, (5, 10));
        assert_eq!(bottom, (5, 10));
    }
}
