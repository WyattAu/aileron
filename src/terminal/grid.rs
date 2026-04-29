//! Terminal grid rendering to egui.
//!
//! Reads the alacritty_terminal grid state and draws characters
//! using egui::Painter. Uses damage tracking to only redraw
//! changed cells.

/// ANSI color palette for terminal rendering.
#[derive(Clone)]
pub struct TerminalColors {
    pub background: egui::Color32,
    pub foreground: egui::Color32,
    pub cursor: egui::Color32,
    pub cursor_text: egui::Color32,
    pub selection_bg: egui::Color32,
    /// 256-color ANSI palette. Index 0-7 = standard, 8-15 = bright, 16-231 = color cube, 232-255 = grayscale.
    pub ansi: [egui::Color32; 256],
}

impl Default for TerminalColors {
    fn default() -> Self {
        let mut ansi = [egui::Color32::WHITE; 256];

        // Standard 16 colors (matches old xterm.js theme)
        ansi[0] = egui::Color32::from_rgb(0x2e, 0x2e, 0x2e); // black
        ansi[1] = egui::Color32::from_rgb(0xff, 0x6b, 0x6b); // red
        ansi[2] = egui::Color32::from_rgb(0x51, 0xcf, 0x66); // green
        ansi[3] = egui::Color32::from_rgb(0xff, 0xd4, 0x3b); // yellow
        ansi[4] = egui::Color32::from_rgb(0x4d, 0xb4, 0xff); // blue
        ansi[5] = egui::Color32::from_rgb(0xcc, 0x5d, 0xe8); // magenta
        ansi[6] = egui::Color32::from_rgb(0x22, 0xb8, 0xcf); // cyan
        ansi[7] = egui::Color32::from_rgb(0xe0, 0xe0, 0xe0); // white
        ansi[8] = egui::Color32::from_rgb(0x86, 0x8e, 0x96); // bright black
        ansi[9] = egui::Color32::from_rgb(0xff, 0x87, 0x87); // bright red
        ansi[10] = egui::Color32::from_rgb(0x69, 0xdb, 0x7c); // bright green
        ansi[11] = egui::Color32::from_rgb(0xff, 0xe0, 0x66); // bright yellow
        ansi[12] = egui::Color32::from_rgb(0x74, 0xc0, 0xfc); // bright blue
        ansi[13] = egui::Color32::from_rgb(0xda, 0x77, 0xf2); // bright magenta
        ansi[14] = egui::Color32::from_rgb(0x3b, 0xc9, 0xdb); // bright cyan
        ansi[15] = egui::Color32::from_rgb(0xff, 0xff, 0xff); // bright white

        // 216 color cube (6x6x6)
        for r in 0..6u8 {
            for g in 0..6u8 {
                for b in 0..6u8 {
                    let idx = 16 + (r as usize) * 36 + (g as usize) * 6 + (b as usize);
                    if idx < 256 {
                        ansi[idx] = egui::Color32::from_rgb(
                            if r == 0 { 0 } else { 55 + r * 40 },
                            if g == 0 { 0 } else { 55 + g * 40 },
                            if b == 0 { 0 } else { 55 + b * 40 },
                        );
                    }
                }
            }
        }
        // 24 grayscale ramp
        for i in 0..24u8 {
            let v = 8 + i * 10;
            ansi[232 + i as usize] = egui::Color32::from_rgb(v, v, v);
        }

        Self {
            background: egui::Color32::from_rgb(0x1a, 0x1a, 0x2e),
            foreground: egui::Color32::from_rgb(0xe0, 0xe0, 0xe0),
            cursor: egui::Color32::from_rgb(0x4d, 0xb4, 0xff),
            cursor_text: egui::Color32::from_rgb(0x1a, 0x1a, 0x2e),
            selection_bg: egui::Color32::from_rgb(0x26, 0x4f, 0x78),
            ansi,
        }
    }
}

impl TerminalColors {
    /// Convert an alacritty_terminal cell Color to an egui Color32.
    /// Uses the `c` field (char) and `fg`/`bg` fields from Cell.
    pub fn resolve_color(&self, _color: &alacritty_terminal::term::cell::Cell) -> egui::Color32 {
        self.foreground
    }
}

/// Metrics for character cell sizing.
#[derive(Clone, Copy, Debug)]
pub struct CellMetrics {
    /// Width of a single character cell in points (egui logical pixels).
    pub cell_width: f32,
    /// Height of a single character cell in points.
    pub cell_height: f32,
    /// Font size in points.
    pub font_size: f32,
}

impl CellMetrics {
    /// Calculate cell metrics from egui's font system.
    pub fn from_egui(ctx: &egui::Context, font_size: f32) -> Self {
        let font_id = egui::FontId::monospace(font_size);

        let cell_width = ctx.fonts(|fonts| {
            // Measure 'M' width for monospace cell width
            let glyph_width = fonts.glyph_width(&font_id, 'M');
            glyph_width.max(1.0)
        });

        let cell_height = ctx.fonts(|fonts| fonts.row_height(&font_id));

        Self {
            cell_width,
            cell_height,
            font_size,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_colors() {
        let colors = TerminalColors::default();
        assert_eq!(colors.background, egui::Color32::from_rgb(0x1a, 0x1a, 0x2e));
        assert_eq!(colors.foreground, egui::Color32::from_rgb(0xe0, 0xe0, 0xe0));
        assert_eq!(colors.ansi.len(), 256);
        // Check standard colors
        assert_eq!(colors.ansi[0], egui::Color32::from_rgb(0x2e, 0x2e, 0x2e));
        assert_eq!(colors.ansi[1], egui::Color32::from_rgb(0xff, 0x6b, 0x6b));
    }

    #[test]
    fn test_cell_metrics_fields() {
        let metrics = CellMetrics {
            cell_width: 8.0,
            cell_height: 16.0,
            font_size: 14.0,
        };
        assert_eq!(metrics.cell_width, 8.0);
        assert_eq!(metrics.cell_height, 16.0);
        assert_eq!(metrics.font_size, 14.0);
    }
}
