//! egui rendering for native terminal panes.
//!
//! Draws the terminal grid into an egui::Painter area.

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Cell;
use alacritty_terminal::vte::ansi::{Color, NamedColor};

use super::grid::{CellMetrics, TerminalColors};

/// Render the terminal grid into an egui area.
pub fn render_terminal(
    painter: &egui::Painter,
    term: &alacritty_terminal::term::Term<super::TermEventListener>,
    screen_rect: egui::Rect,
    colors: &TerminalColors,
    metrics: &CellMetrics,
    selection: Option<&super::Selection>,
) {
    // Fill background
    painter.rect_filled(screen_rect, 0.0, colors.background);

    let cols = term.columns();
    let screen_lines = term.screen_lines();
    let display_offset = term.grid().display_offset();

    let font_id = egui::FontId::monospace(metrics.font_size);

    // Iterate visible lines
    for row in 0..screen_lines {
        let grid_line_i32 = (display_offset + row) as i32;
        let y = screen_rect.min.y + row as f32 * metrics.cell_height;

        for col in 0..cols {
            let cell = &term.grid()[Line(grid_line_i32)][Column(col)];
            let fg_color = resolve_fg_color(cell, colors);
            let bg_color = resolve_bg_color(cell, colors);

            let x = screen_rect.min.x + col as f32 * metrics.cell_width;

            // Draw cell background if different from default
            if bg_color != colors.background {
                let cell_rect = egui::Rect::from_min_size(
                    egui::pos2(x, y),
                    egui::vec2(metrics.cell_width, metrics.cell_height),
                );
                painter.rect_filled(cell_rect, 0.0, bg_color);
            }

            // Draw the character (skip empty cells)
            if cell.c != ' ' && cell.c != '\0' {
                let text_pos = egui::pos2(x, y + (metrics.cell_height - metrics.font_size) * 0.5);

                let text_color = apply_cell_flags(fg_color, cell.flags);

                painter.text(
                    text_pos,
                    egui::Align2::LEFT_TOP,
                    cell.c.to_string(),
                    font_id.clone(),
                    text_color,
                );
            }
        }
    }

    // Draw cursor (bar style)
    let cursor_point = term.grid().cursor.point;
    let cursor_line = cursor_point.line.0;
    let cursor_col = cursor_point.column.0;

    if cursor_line >= display_offset as i32 && cursor_line < (display_offset + screen_lines) as i32
    {
        let visible_row = (cursor_line - display_offset as i32) as usize;
        let cursor_x = screen_rect.min.x + cursor_col as f32 * metrics.cell_width;
        let cursor_y = screen_rect.min.y + visible_row as f32 * metrics.cell_height;

        // Bar cursor: thin vertical line
        painter.rect_filled(
            egui::Rect::from_min_size(
                egui::pos2(cursor_x, cursor_y),
                egui::vec2(2.0, metrics.cell_height),
            ),
            0.0,
            colors.cursor,
        );
    }

    // Draw selection overlay
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
