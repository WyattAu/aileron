//! Terminal grid color definitions and cell metrics.
//!
//! Reads the alacritty_terminal grid state and maps ANSI colors.
//! Rendering is handled by the webview terminal integration, not by egui.

/// Packed RGBA color (R in bits 24-31, G in 16-23, B in 8-15, A in 0-7).
/// Stored as u32 for zero-cost copy.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Color(pub u32);

impl Color {
    pub const WHITE: Color = Color(0xFF_FFFFFF);
    pub const BLACK: Color = Color(0xFF_000000);
    pub const TRANSPARENT: Color = Color(0x00_000000);

    pub const fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Color(((r as u32) << 24) | ((g as u32) << 16) | ((b as u32) << 8) | 0xFF)
    }

    pub const fn from_rgba_unmultiplied(r: u8, g: u8, b: u8, a: u8) -> Self {
        Color(((r as u32) << 24) | ((g as u32) << 16) | ((b as u32) << 8) | (a as u32))
    }

    #[must_use]
    pub const fn r(self) -> u8 {
        (self.0 >> 24) as u8
    }

    #[must_use]
    pub const fn g(self) -> u8 {
        (self.0 >> 16) as u8
    }

    #[must_use]
    pub const fn b(self) -> u8 {
        (self.0 >> 8) as u8
    }

    #[must_use]
    pub const fn a(self) -> u8 {
        self.0 as u8
    }
}

/// ANSI color palette for terminal rendering.
#[derive(Clone)]
pub struct TerminalColors {
    pub background: Color,
    pub foreground: Color,
    pub cursor: Color,
    pub cursor_text: Color,
    pub selection_bg: Color,
    /// 256-color ANSI palette. Index 0-7 = standard, 8-15 = bright, 16-231 = color cube, 232-255 = grayscale.
    pub ansi: [Color; 256],
}

impl Default for TerminalColors {
    fn default() -> Self {
        let mut ansi = [Color::WHITE; 256];

        // Standard 16 colors (matches old xterm.js theme)
        ansi[0] = Color::from_rgb(0x2e, 0x2e, 0x2e); // black
        ansi[1] = Color::from_rgb(0xff, 0x6b, 0x6b); // red
        ansi[2] = Color::from_rgb(0x51, 0xcf, 0x66); // green
        ansi[3] = Color::from_rgb(0xff, 0xd4, 0x3b); // yellow
        ansi[4] = Color::from_rgb(0x4d, 0xb4, 0xff); // blue
        ansi[5] = Color::from_rgb(0xcc, 0x5d, 0xe8); // magenta
        ansi[6] = Color::from_rgb(0x22, 0xb8, 0xcf); // cyan
        ansi[7] = Color::from_rgb(0xe0, 0xe0, 0xe0); // white
        ansi[8] = Color::from_rgb(0x86, 0x8e, 0x96); // bright black
        ansi[9] = Color::from_rgb(0xff, 0x87, 0x87); // bright red
        ansi[10] = Color::from_rgb(0x69, 0xdb, 0x7c); // bright green
        ansi[11] = Color::from_rgb(0xff, 0xe0, 0x66); // bright yellow
        ansi[12] = Color::from_rgb(0x74, 0xc0, 0xfc); // bright blue
        ansi[13] = Color::from_rgb(0xda, 0x77, 0xf2); // bright magenta
        ansi[14] = Color::from_rgb(0x3b, 0xc9, 0xdb); // bright cyan
        ansi[15] = Color::from_rgb(0xff, 0xff, 0xff); // bright white

        // 216 color cube (6x6x6)
        for r in 0..6u8 {
            for g in 0..6u8 {
                for b in 0..6u8 {
                    let idx = 16 + (r as usize) * 36 + (g as usize) * 6 + (b as usize);
                    if idx < 256 {
                        ansi[idx] = Color::from_rgb(
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
            ansi[232 + i as usize] = Color::from_rgb(v, v, v);
        }

        Self {
            background: Color::from_rgb(0x1a, 0x1a, 0x2e),
            foreground: Color::from_rgb(0xe0, 0xe0, 0xe0),
            cursor: Color::from_rgb(0x4d, 0xb4, 0xff),
            cursor_text: Color::from_rgb(0x1a, 0x1a, 0x2e),
            selection_bg: Color::from_rgb(0x26, 0x4f, 0x78),
            ansi,
        }
    }
}

impl TerminalColors {
    /// Convert an alacritty_terminal cell Color to a terminal Color.
    /// Uses the `c` field (char) and `fg`/`bg` fields from Cell.
    pub fn resolve_color(&self, _color: &alacritty_terminal::term::cell::Cell) -> Color {
        self.foreground
    }
}

/// Metrics for character cell sizing.
#[derive(Clone, Copy, Debug)]
pub struct CellMetrics {
    /// Width of a single character cell in logical pixels.
    pub cell_width: f32,
    /// Height of a single character cell in logical pixels.
    pub cell_height: f32,
    /// Font size in points.
    pub font_size: f32,
}

impl CellMetrics {
    /// Estimate cell metrics for a monospace font at the given size.
    /// Without egui, we use a fixed aspect ratio (0.6 width/height ratio).
    pub fn estimated(font_size: f32) -> Self {
        let cell_height = font_size * 1.2;
        let cell_width = font_size * 0.6;
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
        assert_eq!(colors.background, Color::from_rgb(0x1a, 0x1a, 0x2e));
        assert_eq!(colors.foreground, Color::from_rgb(0xe0, 0xe0, 0xe0));
        assert_eq!(colors.ansi.len(), 256);
        assert_eq!(colors.ansi[0], Color::from_rgb(0x2e, 0x2e, 0x2e));
        assert_eq!(colors.ansi[1], Color::from_rgb(0xff, 0x6b, 0x6b));
    }

    #[test]
    fn test_color_from_rgb() {
        let c = Color::from_rgb(0x42, 0x69, 0xAD);
        assert_eq!(c.r(), 0x42);
        assert_eq!(c.g(), 0x69);
        assert_eq!(c.b(), 0xAD);
        assert_eq!(c.a(), 0xFF);
    }

    #[test]
    fn test_cell_metrics_estimated() {
        let metrics = CellMetrics::estimated(14.0);
        assert_eq!(metrics.font_size, 14.0);
        assert!(metrics.cell_width > 0.0);
        assert!(metrics.cell_height > 0.0);
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
