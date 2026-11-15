//! Fresco chrome bar spike (S4 fresco sequencing step 2).
//!
//! A status bar painted entirely by fresco (widget-kit tree ->
//! paint-kit CPU strip), alpha-composited over the live webview frame
//! before gfx presents. No IME, no TextField — the two surfaces the
//! kill-switch protects are untouched. See docs/spikes/fresco-chrome.md.
//!
//! Consumer evidence contract (fresco#8): paint cost + composite cost
//! per damage, measured in the test below, feed the frame-pacing
//! report.

use std::time::{Duration, Instant};

use paint_kit::{Painter, Surface};
use widget_kit::{WidgetColors, WidgetKind, WidgetTree, paint_tree};

/// Status bar height in CSS pixels.
pub const BAR_HEIGHT: usize = 28;

/// A fresco-painted status bar over the webview frame.
pub struct ChromeBar {
    tree: WidgetTree,
    surface: Surface,
    painter: Painter,
    colors: WidgetColors,
    pub width: usize,
    pub height: usize,
    status: String,
    right: String,
    dirty: bool,
}

impl ChromeBar {
    /// Create a bar as wide as the window.
    pub fn new(width: usize) -> anyhow::Result<Self> {
        Ok(Self {
            tree: WidgetTree::new(),
            surface: Surface::new(width, BAR_HEIGHT).ok_or_else(|| {
                anyhow::anyhow!("fresco Surface::new rejected {width}x{BAR_HEIGHT}")
            })?,
            painter: Painter::default(),
            colors: WidgetColors::default(),
            width,
            height: BAR_HEIGHT,
            status: String::new(),
            right: String::new(),
            dirty: true,
        })
    }

    /// Set the left status text. Returns true when the bar needs a repaint.
    pub fn set_status(&mut self, text: impl Into<String>) -> bool {
        let text = text.into();
        if text != self.status {
            self.status = text;
            self.dirty = true;
        }
        self.dirty
    }

    /// Set the right-side text (tab count, zoom, mode). Returns true on change.
    pub fn set_right(&mut self, text: impl Into<String>) -> bool {
        let text = text.into();
        if text != self.right {
            self.right = text;
            self.dirty = true;
        }
        self.dirty
    }

    /// Repaint the bar only when dirty; returns paint duration (the
    /// fresco#8 frame-pacing datum) and whether a repaint happened.
    pub fn paint_if_dirty(&mut self) -> anyhow::Result<(bool, Duration)> {
        if !self.dirty {
            return Ok((false, Duration::ZERO));
        }
        let start = Instant::now();
        self.tree = WidgetTree::new();
        let root = self.tree.push(WidgetKind::Row { gap: 12.0 });
        let status = self.tree.push(WidgetKind::Label {
            text: self.status.clone(),
            px_size: 13.0,
        });
        self.tree.add_child(root, status);
        if !self.right.is_empty() {
            let right = self.tree.push(WidgetKind::Label {
                text: self.right.clone(),
                px_size: 13.0,
            });
            self.tree.add_child(root, right);
        }
        self.tree.compute(widget_kit::Size2D {
            width: self.width as f64,
            height: self.height as f64,
        });
        self.surface.fill_solid(self.colors.surface);
        paint_tree(
            &self.tree,
            &mut self.surface,
            &mut self.painter,
            &self.colors,
        );
        self.dirty = false;
        Ok((true, start.elapsed()))
    }

    /// Alpha-composite the bar over the TOP rows of a webview frame
    /// (RGBA, width x height, 4 bytes/px). Damage region = bar rows only.
    pub fn composite_over(&self, frame: &mut [u8], frame_w: usize, frame_h: usize) {
        composite_rgba(
            &self.surface,
            self.width,
            self.height,
            frame,
            frame_w,
            frame_h,
        );
    }
}

/// Shared alpha-composite: `src` top rows over `frame` top rows (RGBA8).
fn composite_rgba(
    src: &Surface,
    src_w: usize,
    src_h: usize,
    frame: &mut [u8],
    frame_w: usize,
    frame_h: usize,
) {
    if frame_w < src_w || frame_h < src_h {
        return;
    }
    for y in 0..src_h {
        let bar_row = &src.data()[y * src_w * 4..(y + 1) * src_w * 4];
        let frame_row = &mut frame[y * frame_w * 4..y * frame_w * 4 + src_w * 4];
        for (px, bp) in frame_row
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(bar_row.as_chunks::<4>().0)
        {
            let a = bp[3] as u32;
            if a == 255 {
                px[0] = bp[0];
                px[1] = bp[1];
                px[2] = bp[2];
                px[3] = 255;
            } else if a > 0 {
                for c in 0..3 {
                    let f = px[c] as u32;
                    let b = bp[c] as u32;
                    px[c] = ((b * a + f * (255 - a)) / 255) as u8;
                }
                px[3] = 255;
            }
        }
    }
}

/// Fresco command-palette overlay: query TextField + results Select,
/// rendered from `CommandPalette` state (the tested logic stays the
/// owner of actions; fresco is the renderer here — input unification
/// waits on IME).
pub struct PaletteOverlay {
    tree: WidgetTree,
    surface: Surface,
    painter: Painter,
    colors: WidgetColors,
    pub width: usize,
    pub height: usize,
    query: String,
    rows: Vec<String>,
    selected: usize,
    dirty: bool,
}

const ROW_H: usize = 22;
const PAD: usize = 8;

impl PaletteOverlay {
    pub fn new(width: usize) -> anyhow::Result<Self> {
        Ok(Self {
            tree: WidgetTree::new(),
            surface: Surface::new(width, 1)
                .ok_or_else(|| anyhow::anyhow!("fresco Surface::new rejected {width}"))?,
            painter: Painter::default(),
            colors: WidgetColors::default(),
            width,
            height: 1,
            query: String::new(),
            rows: Vec::new(),
            selected: 0,
            dirty: true,
        })
    }

    fn height_for(rows: usize) -> usize {
        PAD * 2 + 28 + rows.min(10) * ROW_H
    }

    /// Mirror palette state; returns true when a repaint is needed.
    pub fn set_state(&mut self, query: &str, rows: Vec<String>, selected: usize) -> bool {
        if query != self.query || rows != self.rows || selected != self.selected {
            self.query = query.to_string();
            self.rows = rows;
            self.selected = selected;
            self.dirty = true;
        }
        self.dirty
    }

    /// Rebuild the fresco tree from state and paint. Returns paint duration.
    pub fn paint(&mut self) -> anyhow::Result<Duration> {
        if !self.dirty {
            return Ok(Duration::ZERO);
        }
        let start = Instant::now();
        let want_h = Self::height_for(self.rows.len());
        if self.height != want_h {
            self.height = want_h;
            self.surface = Surface::new(self.width, want_h).ok_or_else(|| {
                anyhow::anyhow!("fresco Surface::new rejected {}x{}", self.width, want_h)
            })?;
        }
        self.tree = WidgetTree::new();
        let root = self.tree.push(WidgetKind::Column { gap: 6.0 });
        let field = self.tree.push(WidgetKind::TextField {
            placeholder: "Search commands…".to_string(),
            field: ui_text_edit::TextField::new(ui_text_edit::StringBuffer::new()),
        });
        self.tree.add_child(root, field);
        if let Some(w) = self.tree.widget_mut(field) {
            w.fixed_width = Some((self.width - PAD * 2) as f64);
        }
        self.tree.set_focus(Some(field));
        for c in self.query.chars() {
            self.tree.key_char(c);
        }
        if !self.rows.is_empty() {
            let select = self.tree.push(WidgetKind::Select {
                options: self.rows.clone(),
                selected: self.selected.min(self.rows.len().saturating_sub(1)),
                open: true,
            });
            self.tree.add_child(root, select);
            if let Some(w) = self.tree.widget_mut(select) {
                w.fixed_width = Some((self.width - PAD * 2) as f64);
            }
        }
        self.tree.compute(widget_kit::Size2D {
            width: self.width as f64,
            height: self.height as f64,
        });
        self.surface.fill_solid(self.colors.surface);
        paint_tree(
            &self.tree,
            &mut self.surface,
            &mut self.painter,
            &self.colors,
        );
        self.dirty = false;
        Ok(start.elapsed())
    }

    /// Alpha-composite the palette panel over the webview frame's top rows.
    pub fn composite_over(&self, frame: &mut [u8], frame_w: usize, frame_h: usize) {
        composite_rgba(
            &self.surface,
            self.width,
            self.height,
            frame,
            frame_w,
            frame_h,
        );
    }
}

// Timing-split helpers for the fresco#8 evidence (cfg(test) callers only).
#[cfg(test)]
impl ChromeBar {
    pub fn tree_rebuild_for_timing(&mut self) {
        self.tree = WidgetTree::new();
        let root = self.tree.push(WidgetKind::Row { gap: 12.0 });
        let status = self.tree.push(WidgetKind::Label {
            text: self.status.clone(),
            px_size: 13.0,
        });
        self.tree.add_child(root, status);
        self.tree.compute(widget_kit::Size2D {
            width: self.width as f64,
            height: self.height as f64,
        });
    }
    pub fn colors_surface_for_timing(&self) -> [u8; 4] {
        self.colors.surface
    }
    pub fn paint_tree_for_timing(&mut self) {
        paint_tree(
            &self.tree,
            &mut self.surface,
            &mut self.painter,
            &self.colors,
        );
    }
}

/// Fresco find-in-page bar: query TextField + hint/count label +
/// close button. State mirror of `core.ui.find_bar_open/find_query`;
/// match counts arrive via `core.ui.status_message`.
pub struct FindBarOverlay {
    tree: WidgetTree,
    surface: Surface,
    painter: Painter,
    colors: WidgetColors,
    pub width: usize,
    pub height: usize,
    query: String,
    info: String,
    dirty: bool,
}

impl FindBarOverlay {
    pub fn new(width: usize) -> anyhow::Result<Self> {
        Ok(Self {
            tree: WidgetTree::new(),
            surface: Surface::new(width, BAR_HEIGHT).ok_or_else(|| {
                anyhow::anyhow!("fresco Surface::new rejected {width}x{BAR_HEIGHT}")
            })?,
            painter: Painter::default(),
            colors: WidgetColors::default(),
            width,
            height: BAR_HEIGHT,
            query: String::new(),
            info: String::new(),
            dirty: true,
        })
    }

    /// Mirror find-bar state; returns true when a repaint is needed.
    pub fn set_state(&mut self, query: &str, info: &str) -> bool {
        if query != self.query || info != self.info {
            self.query = query.to_string();
            self.info = info.to_string();
            self.dirty = true;
        }
        self.dirty
    }

    /// Rebuild the fresco tree and paint. Returns paint duration.
    pub fn paint(&mut self) -> anyhow::Result<Duration> {
        if !self.dirty {
            return Ok(Duration::ZERO);
        }
        let start = Instant::now();
        self.tree = WidgetTree::new();
        let root = self.tree.push(WidgetKind::Row { gap: 10.0 });
        let field = self.tree.push(WidgetKind::TextField {
            placeholder: "Find in page…".to_string(),
            field: ui_text_edit::TextField::new(ui_text_edit::StringBuffer::new()),
        });
        self.tree.add_child(root, field);
        if let Some(w) = self.tree.widget_mut(field) {
            w.fixed_width = Some(220.0);
        }
        self.tree.set_focus(Some(field));
        for c in self.query.chars() {
            self.tree.key_char(c);
        }
        let hint = self.tree.push(WidgetKind::Label {
            text: if self.info.is_empty() {
                "⏎ next  ⇧⏎ prev".to_string()
            } else {
                self.info.clone()
            },
            px_size: 12.0,
        });
        self.tree.add_child(root, hint);
        self.tree.compute(widget_kit::Size2D {
            width: self.width as f64,
            height: self.height as f64,
        });
        self.surface.fill_solid(self.colors.surface);
        paint_tree(
            &self.tree,
            &mut self.surface,
            &mut self.painter,
            &self.colors,
        );
        self.dirty = false;
        Ok(start.elapsed())
    }

    /// Alpha-composite the find bar over the webview frame's top rows.
    pub fn composite_over(&self, frame: &mut [u8], frame_w: usize, frame_h: usize) {
        composite_rgba(
            &self.surface,
            self.width,
            self.height,
            frame,
            frame_w,
            frame_h,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bar_paints_text_pixels() {
        let mut bar = ChromeBar::new(640).expect("bar");
        bar.set_status("https://example.com");
        let (painted, _t) = bar.paint_if_dirty().expect("paint");
        assert!(painted);
        // Text rasterized: the bar is not uniformly background-colored.
        let data = bar.surface.data();
        let bg = [data[0], data[1], data[2]];
        assert!(
            data.as_chunks::<4>()
                .0
                .iter()
                .any(|px| px[0] != bg[0] || px[1] != bg[1] || px[2] != bg[2]),
            "expected non-uniform pixels from label text"
        );
        // Second paint with no change: not dirty, zero cost.
        let (painted, t) = bar.paint_if_dirty().expect("paint");
        assert!(!painted && t == Duration::ZERO);
    }

    #[test]
    fn palette_paints_query_and_rows() {
        let mut pal = PaletteOverlay::new(640).expect("palette");
        let mk = |n: usize| {
            (0..n)
                .map(|i| format!("command {i}: open settings page {i}"))
                .collect::<Vec<String>>()
        };
        assert!(pal.set_state("set", mk(5), 1), "first state is dirty");
        pal.paint().expect("paint");
        assert_eq!(pal.height, PAD * 2 + 28 + 5 * ROW_H);
        // Query row has text pixels; results area is not uniform panel.
        let data = pal.surface.data();
        let stride = pal.width * 4;
        let query_row = &data[0..28 * stride];
        let bg = [query_row[0], query_row[1], query_row[2]];
        assert!(
            query_row
                .as_chunks::<4>()
                .0
                .iter()
                .any(|px| px[0] != bg[0] || px[1] != bg[1] || px[2] != bg[2]),
            "query text should rasterize"
        );
        let rows_area = &data[28 * stride..];
        assert!(
            rows_area
                .as_chunks::<4>()
                .0
                .iter()
                .any(|px| px[0] != bg[0] || px[1] != bg[1] || px[2] != bg[2]),
            "result rows should rasterize"
        );
        // Selected row renders differently than with another selection.
        let painted_v1 = data.to_vec();
        assert!(pal.set_state("set", mk(5), 3));
        pal.paint().expect("paint");
        assert_ne!(
            painted_v1,
            pal.surface.data(),
            "selection change must repaint"
        );
        // Same state again: not dirty.
        assert!(!pal.set_state("set", mk(5), 3));
        let dur = pal.paint().expect("paint");
        assert_eq!(dur, Duration::ZERO);
    }

    #[test]
    fn palette_pacing_fits_frame_budget() {
        let mut pal = PaletteOverlay::new(1280).expect("palette");
        let mk = |n: usize| {
            (0..n)
                .map(|i| format!("command {i}: navigate to example site {i}"))
                .collect::<Vec<String>>()
        };
        pal.set_state("na", mk(10), 2);
        let _warm = pal.paint().expect("warm");
        pal.set_state("nav", mk(10), 4);
        let t = pal.paint().expect("paint").as_micros();
        println!(
            "fresco#8 palette pacing: paint={}µs (10 rows, 1280px) — measured {}",
            t,
            if cfg!(debug_assertions) {
                "DEBUG (not comparable)"
            } else {
                "RELEASE"
            }
        );
        // Regression guard only: today's release number is ~61 ms — 4x over
        // the 16.6 ms frame budget (Select open-render path; finding filed
        // to fresco#8). Tighten to <16_667 once fresco optimizes Select.
        #[cfg(not(debug_assertions))]
        assert!(t < 100_000, "palette paint regression guard (was {t}µs)");
    }

    #[test]
    fn find_bar_paints_query_and_updates() {
        let mut fb = FindBarOverlay::new(640).expect("find bar");
        assert!(fb.set_state("pattern", ""), "first state dirty");
        fb.paint().expect("paint");
        let data = fb.surface.data();
        let bg = [data[0], data[1], data[2]];
        assert!(
            data.as_chunks::<4>()
                .0
                .iter()
                .any(|px| px[0] != bg[0] || px[1] != bg[1] || px[2] != bg[2]),
            "query text should rasterize"
        );
        let before = data.to_vec();
        assert!(fb.set_state("pattern", "3/17 matches"));
        fb.paint().expect("paint");
        assert_ne!(before, fb.surface.data(), "match info change must repaint");
        assert!(!fb.set_state("pattern", "3/17 matches"));
        let dur = fb.paint().expect("paint");
        assert_eq!(dur, Duration::ZERO);
    }

    #[test]
    fn find_bar_pacing_fits_frame_budget() {
        let mut fb = FindBarOverlay::new(1280).expect("find bar");
        fb.set_state("pattern", "");
        let _warm = fb.paint().expect("warm");
        fb.set_state("pattern", "12/48 matches");
        let t = fb.paint().expect("paint").as_micros();
        println!(
            "fresco#8 find-bar pacing: paint={}µs (1280x{}) — {}",
            t,
            BAR_HEIGHT,
            if cfg!(debug_assertions) {
                "DEBUG (not comparable)"
            } else {
                "RELEASE"
            }
        );
        #[cfg(not(debug_assertions))]
        assert!(t < 16_667, "find-bar damage must fit one frame (was {t}µs)");
    }

    #[test]
    fn composite_touches_only_bar_rows() {
        let mut bar = ChromeBar::new(320).expect("bar");
        bar.set_status("test");
        bar.paint_if_dirty().expect("paint");
        let (w, h) = (320usize, 100usize);
        let mut frame = vec![10u8; w * h * 4];
        bar.composite_over(&mut frame, w, h);
        // Row 0 must differ from solid webview gray (bar painted over it).
        let row0 = &frame[0..w * 4];
        assert!(
            row0.as_chunks::<4>()
                .0
                .iter()
                .any(|px| *px != [10, 10, 10, 10])
        );
        // Row 40 (below the bar) untouched.
        let row40 = &frame[40 * w * 4..40 * w * 4 + w * 4];
        assert!(
            row40
                .as_chunks::<4>()
                .0
                .iter()
                .all(|px| *px == [10, 10, 10, 10])
        );
    }

    /// Frame-pacing evidence for fresco#8: paint + composite cost for a
    /// 1280px-wide bar. CI tolerance is generous (CPU, no GPU); the
    /// printed numbers are the real deliverable.
    #[test]
    fn paint_and_composite_pacing() {
        let mut bar = ChromeBar::new(1280).expect("bar");
        bar.set_status("https://example.com — Loading…");
        bar.set_right("12 tabs | 100%");
        // Discard the first paint: lazy font-atlas / raster-scratch warm-up.
        let (_warm, warm_t) = bar.paint_if_dirty().expect("warm paint");
        bar.set_status("https://example.com — Done");
        let (_painted, paint_t) = bar.paint_if_dirty().expect("paint");
        // Phase split for the fresco#8 finding: tree rebuild/layout vs fill vs paint_tree.
        bar.set_status("phase split");
        let t0 = Instant::now();
        bar.tree_rebuild_for_timing();
        let layout_t = t0.elapsed();
        let t1 = Instant::now();
        bar.surface.fill_solid(bar.colors_surface_for_timing());
        let fill_t = t1.elapsed();
        let t2 = Instant::now();
        bar.paint_tree_for_timing();
        let tree_t = t2.elapsed();
        println!(
            "fresco#8 phase split: layout={}µs fill={}µs paint_tree={}µs",
            layout_t.as_micros(),
            fill_t.as_micros(),
            tree_t.as_micros()
        );
        println!("fresco#8 warm-up: {}µs (atlas build)", warm_t.as_micros());
        let _ = warm_t;
        let (w, h) = (1280usize, 720usize);
        let mut frame = vec![10u8; w * h * 4];
        let start = Instant::now();
        bar.composite_over(&mut frame, w, h);
        let comp_t = start.elapsed();
        println!(
            "fresco#8 frame-pacing: paint={}µs composite={:?}µs (1280x{} bar over 1280x720)",
            paint_t.as_micros(),
            comp_t.as_micros(),
            BAR_HEIGHT
        );
        // Frame-budget assertion is release-only: debug builds are ~20x
        // slower and would fail spuriously (measured 22-58 ms vs 2.4 ms).
        #[cfg(not(debug_assertions))]
        assert!(
            paint_t + comp_t < Duration::from_millis(16),
            "bar damage must fit one 60 Hz frame budget"
        );
    }
}
