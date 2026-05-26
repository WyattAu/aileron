use crate::app::AppState;
use crate::servo::WryPaneManager;
#[cfg(feature = "terminal")]
use crate::terminal::grid::CellMetrics;
#[cfg(feature = "terminal")]
use crate::terminal::render::render_terminal;
use egui::WidgetType;
use tracing::warn;

use super::a11y_info;

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_central_panel(
    ctx: &egui::Context,
    app_state: &mut AppState,
    wry_panes: &WryPaneManager,
    webview_textures: &std::collections::HashMap<uuid::Uuid, egui::TextureId>,
    #[cfg(feature = "terminal")] terminal_manager: &mut crate::terminal::NativeTerminalManager,
    accent: egui::Color32,
    bg: egui::Color32,
    border_color_default: egui::Color32,
) {
    egui::CentralPanel::default().show(ctx, |ui| {
        let panes: Vec<_> = app_state.wm.iter_panes().collect();
        let active_id = app_state.wm.active_pane_id();
        let offscreen = app_state.config.is_offscreen();

        if panes.len() > 1 {
            let available = ui.available_rect_before_wrap();
            for (id, wm_rect) in &panes {
                let screen_rect = egui::Rect::from_min_max(
                    egui::pos2(
                        available.min.x + wm_rect.x as f32,
                        available.min.y + wm_rect.y as f32,
                    ),
                    egui::pos2(
                        available.min.x + (wm_rect.x + wm_rect.w) as f32,
                        available.min.y + (wm_rect.y + wm_rect.h) as f32,
                    ),
                );

                let is_active = *id == active_id;
                let border_color = if is_active {
                    accent
                } else {
                    border_color_default
                };

                if offscreen {
                    #[cfg(feature = "terminal")]
                    let is_terminal = terminal_manager.is_terminal(id);

                    #[cfg(feature = "terminal")]
                    if is_terminal {
                        let colors = terminal_manager.get_colors();
                        if let Some(pane) = terminal_manager.get(id) {
                            let metrics = CellMetrics::from_egui(ctx, 14.0);
                            let selection = pane.selection();
                            let damage = pane.damage_info();
                            let bell_flashing = pane.is_bell_flashing();
                            render_terminal(
                                ui.painter(),
                                pane.term(),
                                screen_rect,
                                &colors,
                                &metrics,
                                Some(selection),
                                &damage,
                                bell_flashing,
                            );
                        } else {
                            ui.painter().rect_filled(screen_rect, 0.0, bg);
                        }
                    }

                    #[cfg(feature = "terminal")]
                    let show_webview = !is_terminal;
                    #[cfg(not(feature = "terminal"))]
                    let show_webview = true;

                    if show_webview {
                        if let Some(&tex_id) = webview_textures.get(id) {
                            let image = egui::Image::new(egui::load::SizedTexture::new(
                                tex_id,
                                screen_rect.size(),
                            ));
                            ui.put(screen_rect, image);
                        } else {
                            tracing::debug!(
                                "render pane {}: NO texture ({} total)",
                                &id.to_string()[..8],
                                webview_textures.len(),
                            );
                            ui.painter().rect_filled(screen_rect, 0.0, bg);
                            ui.painter().rect_stroke(
                                screen_rect,
                                0.0,
                                egui::Stroke::new(2.0, border_color),
                                egui::epaint::StrokeKind::Middle,
                            );
                        }
                    }
                }

                ui.painter().rect_stroke(
                    screen_rect,
                    0.0,
                    egui::Stroke::new(2.0, border_color),
                    egui::epaint::StrokeKind::Middle,
                );
            }

            // Draw interactive resize handles on split borders
            let borders = app_state.wm.split_borders();
            let available = ui.available_rect_before_wrap();
            let handle_thickness = 6.0;
            for (pos, direction, pane_a_id, _pane_b_id) in &borders {
                let handle_rect = match direction {
                    crate::wm::rect::SplitDirection::Horizontal => {
                        let x = available.min.x + *pos as f32;
                        egui::Rect::from_min_max(
                            egui::pos2(x - handle_thickness / 2.0, available.min.y),
                            egui::pos2(x + handle_thickness / 2.0, available.max.y),
                        )
                    }
                    crate::wm::rect::SplitDirection::Vertical => {
                        let y = available.min.y + *pos as f32;
                        egui::Rect::from_min_max(
                            egui::pos2(available.min.x, y - handle_thickness / 2.0),
                            egui::pos2(available.max.x, y + handle_thickness / 2.0),
                        )
                    }
                };

                let response = ui.allocate_rect(handle_rect, egui::Sense::drag());
                let hovering = response.hovered();
                if hovering {
                    ui.ctx().set_cursor_icon(match direction {
                        crate::wm::rect::SplitDirection::Horizontal => {
                            egui::CursorIcon::ResizeColumn
                        }
                        crate::wm::rect::SplitDirection::Vertical => egui::CursorIcon::ResizeRow,
                    });
                }
                if response.drag_started() {
                    ui.ctx().set_cursor_icon(match direction {
                        crate::wm::rect::SplitDirection::Horizontal => {
                            egui::CursorIcon::ResizeColumn
                        }
                        crate::wm::rect::SplitDirection::Vertical => egui::CursorIcon::ResizeRow,
                    });
                }
                if response.dragged() {
                    let delta = response.drag_delta();
                    let amount = match direction {
                        crate::wm::rect::SplitDirection::Horizontal => delta.x,
                        crate::wm::rect::SplitDirection::Vertical => delta.y,
                    };
                    let viewport = app_state
                        .wm
                        .iter_panes()
                        .find_map(|(id, r)| if id == *pane_a_id { Some(r) } else { None });
                    if let Some(viewport) = viewport {
                        let resize_amount = match direction {
                            crate::wm::rect::SplitDirection::Horizontal => {
                                (amount as f64) / viewport.w.max(1.0)
                            }
                            crate::wm::rect::SplitDirection::Vertical => {
                                (amount as f64) / viewport.h.max(1.0)
                            }
                        };
                        if let Err(e) = app_state.wm.resize_pane(*pane_a_id, resize_amount as f64) {
                            warn!(%e, "Failed to resize pane");
                        }
                    }
                }

                if hovering || response.dragged() {
                    let highlight = egui::Color32::from_rgba_premultiplied(
                        accent.r(),
                        accent.g(),
                        accent.b(),
                        60,
                    );
                    ui.painter().rect_filled(handle_rect, 0.0, highlight);
                }
            }
        } else if wry_panes.is_empty() && (!offscreen || webview_textures.is_empty()) {
            let available = ui.available_rect_before_wrap();
            ui.painter().rect_stroke(
                available,
                0.0,
                egui::Stroke::new(2.0, accent),
                egui::epaint::StrokeKind::Middle,
            );

            ui.vertical_centered(|ui| {
                ui.add_space(ui.available_height() / 4.0);
                ui.heading("Aileron").widget_info(|| {
                    a11y_info(
                        WidgetType::Label,
                        "Aileron welcome screen - keyboard shortcuts",
                    )
                });
                ui.label("Keyboard-Driven Web Environment");
                ui.add_space(16.0);
                ui.label("Controls:");
                ui.monospace("i           Enter Insert mode");
                ui.monospace("Esc         Return to Normal mode");
                ui.monospace(":           Enter Command mode");
                ui.monospace("Ctrl+W      Split vertical");
                ui.monospace("Ctrl+S      Split horizontal");
                ui.monospace("q           Close pane");
                ui.monospace("Ctrl+H/J/K/L  Navigate panes");
                ui.monospace("Ctrl+P      Command palette");
                ui.monospace("Ctrl+E      Open in system browser");
            });
        } else if offscreen && panes.len() == 1 {
            let available = ui.available_rect_before_wrap();

            #[cfg(feature = "terminal")]
            let is_terminal = panes.iter().any(|(id, _)| terminal_manager.is_terminal(id));

            #[cfg(feature = "terminal")]
            if is_terminal {
                let colors = terminal_manager.get_colors();
                for (id, _) in &panes {
                    if let Some(pane) = terminal_manager.get(id) {
                        let metrics = CellMetrics::from_egui(ctx, 14.0);
                        let selection = pane.selection();
                        let damage = pane.damage_info();
                        let bell_flashing = pane.is_bell_flashing();
                        render_terminal(
                            ui.painter(),
                            pane.term(),
                            available,
                            &colors,
                            &metrics,
                            Some(selection),
                            &damage,
                            bell_flashing,
                        );
                    }
                }
            }

            #[cfg(feature = "terminal")]
            let show_webview = !is_terminal;
            #[cfg(not(feature = "terminal"))]
            let show_webview = true;

            if show_webview {
                if let Some((_, tex_id)) = panes
                    .iter()
                    .find_map(|(id, _)| webview_textures.get_key_value(id))
                {
                    let image =
                        egui::Image::new(egui::load::SizedTexture::new(*tex_id, available.size()));
                    ui.put(available, image);
                } else {
                    ui.painter().rect_filled(available, 0.0, bg);
                }
            }
        }
    });
}
