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
        if frame_w < self.width || frame_h < self.height {
            return;
        }
        for y in 0..self.height {
            let bar_row = &self.surface.data()[y * self.width * 4..(y + 1) * self.width * 4];
            let frame_row = &mut frame[y * frame_w * 4..y * frame_w * 4 + self.width * 4];
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
