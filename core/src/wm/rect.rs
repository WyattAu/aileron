use std::fmt;

/// A 2D rectangle with f64 coordinates.
/// Used by the BSP tree to define pane positions on screen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub const MIN_W: f64 = 100.0;
    pub const MIN_H: f64 = 100.0;

    pub fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self { x, y, w, h }
    }

    pub fn area(&self) -> f64 {
        self.w * self.h
    }

    pub fn valid(&self) -> bool {
        self.w > 0.0 && self.h > 0.0
    }

    pub fn contains_point(&self, px: f64, py: f64) -> bool {
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }

    /// Split this rectangle into two sub-rectangles.
    pub fn partition(&self, dir: SplitDirection, ratio: f64) -> (Rect, Rect) {
        match dir {
            SplitDirection::Horizontal => {
                let top_h = self.h * ratio;
                let bot_h = self.h * (1.0 - ratio);
                (
                    Rect::new(self.x, self.y, self.w, top_h),
                    Rect::new(self.x, self.y + top_h, self.w, bot_h),
                )
            }
            SplitDirection::Vertical => {
                let left_w = self.w * ratio;
                let right_w = self.w * (1.0 - ratio);
                (
                    Rect::new(self.x, self.y, left_w, self.h),
                    Rect::new(self.x + left_w, self.y, right_w, self.h),
                )
            }
        }
    }

    /// Check if two rectangles are interior-disjoint (no overlap).
    pub fn disjoint(&self, other: &Rect) -> bool {
        self.x + self.w <= other.x
            || other.x + other.w <= self.x
            || self.y + self.h <= other.y
            || other.y + other.h <= self.y
    }

    /// Check if this rectangle is adjacent to another in the given direction.
    pub fn adjacent_to(&self, other: &Rect, dir: Direction) -> bool {
        match dir {
            Direction::Up => {
                other.y + other.h == self.y
                    && self.x < other.x + other.w
                    && other.x < self.x + self.w
            }
            Direction::Down => {
                self.y + self.h == other.y
                    && self.x < other.x + other.w
                    && other.x < self.x + self.w
            }
            Direction::Left => {
                other.x + other.w == self.x
                    && self.y < other.y + other.h
                    && other.y < self.y + self.h
            }
            Direction::Right => {
                self.x + self.w == other.x
                    && self.y < other.y + other.h
                    && other.y < self.y + self.h
            }
        }
    }
}

impl fmt::Display for Rect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{:.0}, {:.0}, {:.0}, {:.0}]",
            self.x, self.y, self.w, self.h
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rect_area() {
        let r = Rect::new(0.0, 0.0, 1920.0, 1080.0);
        assert!((r.area() - 2073600.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_rect_valid() {
        assert!(Rect::new(0.0, 0.0, 100.0, 100.0).valid());
        assert!(!Rect::new(0.0, 0.0, -1.0, 100.0).valid());
        assert!(!Rect::new(0.0, 0.0, 100.0, 0.0).valid());
    }

    #[test]
    fn test_partition_horizontal() {
        let r = Rect::new(0.0, 0.0, 1920.0, 1080.0);
        let (top, bot) = r.partition(SplitDirection::Horizontal, 0.5);
        assert!((top.h - 540.0).abs() < f64::EPSILON);
        assert!((bot.h - 540.0).abs() < f64::EPSILON);
        assert!((bot.y - 540.0).abs() < f64::EPSILON);
        assert!((top.area() + bot.area() - r.area()).abs() < f64::EPSILON);
    }

    #[test]
    fn test_partition_vertical() {
        let r = Rect::new(0.0, 0.0, 1920.0, 1080.0);
        let (left, right) = r.partition(SplitDirection::Vertical, 0.5);
        assert!((left.w - 960.0).abs() < f64::EPSILON);
        assert!((right.w - 960.0).abs() < f64::EPSILON);
        assert!((left.area() + right.area() - r.area()).abs() < f64::EPSILON);
    }

    #[test]
    fn test_partition_area_preservation() {
        let r = Rect::new(100.0, 50.0, 800.0, 600.0);
        for ratio in [0.1, 0.3, 0.5, 0.7, 0.9] {
            let (a, b) = r.partition(SplitDirection::Horizontal, ratio);
            assert!((a.area() + b.area() - r.area()).abs() < 0.001);
            let (a, b) = r.partition(SplitDirection::Vertical, ratio);
            assert!((a.area() + b.area() - r.area()).abs() < 0.001);
        }
    }

    #[test]
    fn test_disjoint() {
        let a = Rect::new(0.0, 0.0, 100.0, 100.0);
        let b = Rect::new(100.0, 0.0, 100.0, 100.0);
        assert!(a.disjoint(&b));
        let c = Rect::new(50.0, 50.0, 100.0, 100.0);
        assert!(!a.disjoint(&c));
    }
}
