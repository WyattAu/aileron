//! egui rendering for native terminal panes.
//!
//! Draws the terminal grid into an egui::Painter area.

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Cell;
use alacritty_terminal::vte::ansi::{Color, NamedColor};

use super::DamageInfo;
use super::grid::{CellMetrics, TerminalColors};

#[allow(clippy::too_many_arguments)]
pub fn render_terminal(
    painter: &egui::Painter,
    term: &alacritty_terminal::term::Term<super::TermEventListener>,
    screen_rect: egui::Rect,
    colors: &TerminalColors,
    metrics: &CellMetrics,
    selection: Option<&super::Selection>,
    damage: &DamageInfo,
    bell_flashing: bool,
) {
    if !damage.full && damage.lines.is_empty() {
        return;
    }

    painter.rect_filled(screen_rect, 0.0, colors.background);

    let cols = term.columns();
    let screen_lines = term.screen_lines();
    let display_offset = term.grid().display_offset();
    let font_id = egui::FontId::monospace(metrics.font_size);

    if damage.full {
        render_cells(
            painter,
            term,
            screen_rect,
            colors,
            metrics,
            display_offset,
            &font_id,
            0..screen_lines,
            0..cols,
        );
    } else {
        for &(line, left, right) in damage.lines {
            if line < display_offset {
                continue;
            }
            let viewport_row = line - display_offset;
            if viewport_row >= screen_lines {
                continue;
            }
            let col_end = (right + 1).min(cols);
            render_cells(
                painter,
                term,
                screen_rect,
                colors,
                metrics,
                display_offset,
                &font_id,
                viewport_row..viewport_row + 1,
                left..col_end,
            );
        }
    }

    draw_cursor(painter, term, screen_rect, metrics, colors, display_offset);
    draw_selection(
        painter,
        term,
        screen_rect,
        metrics,
        selection,
        cols,
        screen_lines,
    );

    if bell_flashing {
        painter.rect_filled(
            screen_rect,
            0.0,
            egui::Color32::from_rgba_premultiplied(255, 255, 255, 30),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn render_cells(
    painter: &egui::Painter,
    term: &alacritty_terminal::term::Term<super::TermEventListener>,
    screen_rect: egui::Rect,
    colors: &TerminalColors,
    metrics: &CellMetrics,
    display_offset: usize,
    font_id: &egui::FontId,
    rows: std::ops::Range<usize>,
    cols_range: std::ops::Range<usize>,
) {
    let row_font_id = font_id.clone();
    let mut text_buf = String::with_capacity(4);

    for row in rows {
        let grid_line_i32 = (display_offset + row) as i32;
        let y = screen_rect.min.y + row as f32 * metrics.cell_height;

        for col in cols_range.clone() {
            let cell = &term.grid()[Line(grid_line_i32)][Column(col)];
            let fg_color = resolve_fg_color(cell, colors);
            let bg_color = resolve_bg_color(cell, colors);

            let x = screen_rect.min.x + col as f32 * metrics.cell_width;

            if bg_color != colors.background {
                let cell_rect = egui::Rect::from_min_size(
                    egui::pos2(x, y),
                    egui::vec2(metrics.cell_width, metrics.cell_height),
                );
                painter.rect_filled(cell_rect, 0.0, bg_color);
            }

            if cell.c != ' ' && cell.c != '\0' {
                let text_pos = egui::pos2(x, y + (metrics.cell_height - metrics.font_size) * 0.5);
                let text_color = apply_cell_flags(fg_color, cell.flags);
                text_buf.clear();
                text_buf.push(cell.c);
                let galley =
                    painter.layout_no_wrap(text_buf.clone(), row_font_id.clone(), text_color);
                let rect = egui::Align2::LEFT_TOP.anchor_size(text_pos, galley.size());
                painter.galley(rect.min, galley, text_color);
            }
        }
    }
}

fn draw_cursor(
    painter: &egui::Painter,
    term: &alacritty_terminal::term::Term<super::TermEventListener>,
    screen_rect: egui::Rect,
    metrics: &CellMetrics,
    colors: &TerminalColors,
    display_offset: usize,
) {
    let cursor_point = term.grid().cursor.point;
    let cursor_line = cursor_point.line.0;
    let cursor_col = cursor_point.column.0;
    let screen_lines = term.screen_lines();

    if cursor_line >= display_offset as i32 && cursor_line < (display_offset + screen_lines) as i32
    {
        let visible_row = (cursor_line - display_offset as i32) as usize;
        let cursor_x = screen_rect.min.x + cursor_col as f32 * metrics.cell_width;
        let cursor_y = screen_rect.min.y + visible_row as f32 * metrics.cell_height;

        painter.rect_filled(
            egui::Rect::from_min_size(
                egui::pos2(cursor_x, cursor_y),
                egui::vec2(2.0, metrics.cell_height),
            ),
            0.0,
            colors.cursor,
        );
    }
}

fn draw_selection(
    painter: &egui::Painter,
    term: &alacritty_terminal::term::Term<super::TermEventListener>,
    screen_rect: egui::Rect,
    metrics: &CellMetrics,
    selection: Option<&super::Selection>,
    cols: usize,
    screen_lines: usize,
) {
    if let Some(sel) = selection
        && sel.active
    {
        let ((start_line, start_col), (end_line, end_col)) = sel.normalized();
        let display_offset = term.grid().display_offset() as i32;

        for grid_line in start_line..=end_line {
            let screen_row = (grid_line - display_offset) as isize;
            if screen_row < 0 || screen_row as usize >= screen_lines {
                continue;
            }

            let row_start = if grid_line == start_line {
                start_col
            } else {
                0
            };
            let row_end = if grid_line == end_line {
                end_col
            } else {
                cols.saturating_sub(1)
            };

            for col in row_start..=row_end {
                let x = screen_rect.min.x + col as f32 * metrics.cell_width;
                let y = screen_rect.min.y + screen_row as f32 * metrics.cell_height;
                let cell_rect = egui::Rect::from_min_size(
                    egui::pos2(x, y),
                    egui::vec2(metrics.cell_width, metrics.cell_height),
                );
                painter.rect_filled(
                    cell_rect,
                    0.0,
                    egui::Color32::from_rgba_premultiplied(77, 180, 255, 60),
                );
            }
        }
    }
}

/// Resolve a cell's foreground color to egui Color32.
fn resolve_fg_color(cell: &Cell, colors: &TerminalColors) -> egui::Color32 {
    match cell.fg {
        Color::Named(named) => {
            let idx = named as usize;
            colors.ansi.get(idx).copied().unwrap_or(colors.foreground)
        }
        Color::Spec(rgb) => egui::Color32::from_rgb(rgb.r, rgb.g, rgb.b),
        Color::Indexed(idx) => colors
            .ansi
            .get(idx as usize)
            .copied()
            .unwrap_or(colors.foreground),
    }
}

/// Resolve a cell's background color to egui Color32.
fn resolve_bg_color(cell: &Cell, colors: &TerminalColors) -> egui::Color32 {
    match cell.bg {
        Color::Named(NamedColor::Background) => colors.background,
        Color::Named(named) => {
            let idx = named as usize;
            colors.ansi.get(idx).copied().unwrap_or(colors.background)
        }
        Color::Spec(rgb) => egui::Color32::from_rgb(rgb.r, rgb.g, rgb.b),
        Color::Indexed(idx) => colors
            .ansi
            .get(idx as usize)
            .copied()
            .unwrap_or(colors.background),
    }
}

/// Apply cell flags (bold, dim, hidden) to modify text color.
fn apply_cell_flags(
    color: egui::Color32,
    flags: alacritty_terminal::term::cell::Flags,
) -> egui::Color32 {
    use alacritty_terminal::term::cell::Flags;

    let mut c = color;
    if flags.contains(Flags::BOLD) {
        c = egui::Color32::from_rgb(
            c.r().saturating_add(30),
            c.g().saturating_add(30),
            c.b().saturating_add(30),
        );
    }
    if flags.contains(Flags::DIM) {
        c = egui::Color32::from_rgb(c.r() / 2, c.g() / 2, c.b() / 2);
    }
    if flags.contains(Flags::HIDDEN) {
        c = egui::Color32::TRANSPARENT;
    }
    c
}

#[cfg(test)]
mod tests {
    use alacritty_terminal::term::cell::Flags;
    use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb};

    use super::*;

    fn test_cell() -> Cell {
        Cell::default()
    }

    fn test_colors() -> TerminalColors {
        let mut ansi = [egui::Color32::BLACK; 256];
        ansi[0] = egui::Color32::from_rgb(1, 1, 1); // Black
        ansi[1] = egui::Color32::from_rgb(2, 2, 2); // Red
        ansi[2] = egui::Color32::from_rgb(3, 3, 3); // Green
        ansi[3] = egui::Color32::from_rgb(4, 4, 4); // Yellow
        ansi[4] = egui::Color32::from_rgb(5, 5, 5); // Blue
        ansi[5] = egui::Color32::from_rgb(6, 6, 6); // Magenta
        ansi[6] = egui::Color32::from_rgb(7, 7, 7); // Cyan
        ansi[7] = egui::Color32::from_rgb(8, 8, 8); // White
        ansi[8] = egui::Color32::from_rgb(9, 9, 9); // BrightBlack
        ansi[9] = egui::Color32::from_rgb(10, 10, 10); // BrightRed
        ansi[10] = egui::Color32::from_rgb(11, 11, 11); // BrightGreen
        ansi[11] = egui::Color32::from_rgb(12, 12, 12); // BrightYellow
        ansi[12] = egui::Color32::from_rgb(13, 13, 13); // BrightBlue
        ansi[13] = egui::Color32::from_rgb(14, 14, 14); // BrightMagenta
        ansi[14] = egui::Color32::from_rgb(15, 15, 15); // BrightCyan
        ansi[15] = egui::Color32::from_rgb(16, 16, 16); // BrightWhite
        TerminalColors {
            background: egui::Color32::from_rgb(10, 20, 30),
            foreground: egui::Color32::from_rgb(200, 210, 220),
            cursor: egui::Color32::from_rgb(255, 0, 0),
            cursor_text: egui::Color32::WHITE,
            selection_bg: egui::Color32::from_rgba_premultiplied(77, 180, 255, 60),
            ansi,
        }
    }

    #[test]
    fn resolve_fg_color_named() {
        let mut cell = test_cell();
        cell.fg = Color::Named(NamedColor::Red);
        let colors = test_colors();
        assert_eq!(
            resolve_fg_color(&cell, &colors),
            egui::Color32::from_rgb(2, 2, 2)
        );
    }

    #[test]
    fn resolve_fg_color_spec() {
        let mut cell = test_cell();
        cell.fg = Color::Spec(Rgb {
            r: 100,
            g: 150,
            b: 200,
        });
        let colors = test_colors();
        assert_eq!(
            resolve_fg_color(&cell, &colors),
            egui::Color32::from_rgb(100, 150, 200)
        );
    }

    #[test]
    fn resolve_fg_color_indexed() {
        let mut cell = test_cell();
        cell.fg = Color::Indexed(5);
        let colors = test_colors();
        assert_eq!(
            resolve_fg_color(&cell, &colors),
            egui::Color32::from_rgb(6, 6, 6)
        );
    }

    #[test]
    fn resolve_fg_color_out_of_range_falls_back() {
        let mut cell = test_cell();
        cell.fg = Color::Named(NamedColor::Foreground);
        let colors = test_colors();
        assert_eq!(
            resolve_fg_color(&cell, &colors),
            egui::Color32::from_rgb(200, 210, 220)
        );
    }

    #[test]
    fn resolve_bg_color_named_background() {
        let cell = test_cell();
        let colors = test_colors();
        assert_eq!(
            resolve_bg_color(&cell, &colors),
            egui::Color32::from_rgb(10, 20, 30)
        );
    }

    #[test]
    fn resolve_bg_color_named_non_background() {
        let mut cell = test_cell();
        cell.bg = Color::Named(NamedColor::Blue);
        let colors = test_colors();
        assert_eq!(
            resolve_bg_color(&cell, &colors),
            egui::Color32::from_rgb(5, 5, 5)
        );
    }

    #[test]
    fn resolve_bg_color_spec() {
        let mut cell = test_cell();
        cell.bg = Color::Spec(Rgb {
            r: 50,
            g: 60,
            b: 70,
        });
        let colors = test_colors();
        assert_eq!(
            resolve_bg_color(&cell, &colors),
            egui::Color32::from_rgb(50, 60, 70)
        );
    }

    #[test]
    fn resolve_bg_color_indexed() {
        let mut cell = test_cell();
        cell.bg = Color::Indexed(9);
        let colors = test_colors();
        assert_eq!(
            resolve_bg_color(&cell, &colors),
            egui::Color32::from_rgb(10, 10, 10)
        );
    }

    #[test]
    fn resolve_bg_color_out_of_range_falls_back() {
        let mut cell = test_cell();
        cell.bg = Color::Named(NamedColor::Cursor);
        let colors = test_colors();
        assert_eq!(
            resolve_bg_color(&cell, &colors),
            egui::Color32::from_rgb(10, 20, 30)
        );
    }

    #[test]
    fn apply_cell_flags_no_flags() {
        let color = egui::Color32::from_rgb(100, 150, 200);
        assert_eq!(apply_cell_flags(color, Flags::empty()), color);
    }

    #[test]
    fn apply_cell_flags_bold() {
        let color = egui::Color32::from_rgb(100, 150, 200);
        assert_eq!(
            apply_cell_flags(color, Flags::BOLD),
            egui::Color32::from_rgb(130, 180, 230)
        );
    }

    #[test]
    fn apply_cell_flags_bold_saturates() {
        let color = egui::Color32::from_rgb(240, 250, 255);
        assert_eq!(
            apply_cell_flags(color, Flags::BOLD),
            egui::Color32::from_rgb(255, 255, 255)
        );
    }

    #[test]
    fn apply_cell_flags_dim() {
        let color = egui::Color32::from_rgb(100, 150, 201);
        assert_eq!(
            apply_cell_flags(color, Flags::DIM),
            egui::Color32::from_rgb(50, 75, 100)
        );
    }

    #[test]
    fn apply_cell_flags_hidden() {
        let color = egui::Color32::from_rgb(100, 150, 200);
        assert_eq!(
            apply_cell_flags(color, Flags::HIDDEN),
            egui::Color32::TRANSPARENT
        );
    }

    #[test]
    fn apply_cell_flags_bold_dim() {
        let color = egui::Color32::from_rgb(100, 150, 200);
        assert_eq!(
            apply_cell_flags(color, Flags::BOLD | Flags::DIM),
            egui::Color32::from_rgb(65, 90, 115)
        );
    }
}
