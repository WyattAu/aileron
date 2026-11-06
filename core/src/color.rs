/// RGBA color stored as packed 0xRRGGBBAA (u32) - core data type shared by
/// the terminal grid, config themes, and the headed compositor (GATE-1 S1:
/// moved out of terminal::grid so headless builds don't need the terminal).
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
