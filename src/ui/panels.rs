use crate::app::{AppState, WryAction};
use crate::config::ThemeColors;
use crate::git::GitStatus;
use crate::input::Mode;
use crate::servo::WryPaneManager;
use crate::terminal::grid::{CellMetrics, TerminalColors};
use crate::terminal::render::render_terminal;
use crate::terminal::NativeTerminalManager;
use crate::ui::search::SearchCategory;

pub fn build_ui(
    ctx: &egui::Context,
    app_state: &mut AppState,
    wry_panes: &WryPaneManager,
    git_status: &GitStatus,
    status_bar_height: f64,
    webview_textures: &std::collections::HashMap<uuid::Uuid, egui::TextureId>,
    terminal_manager: &NativeTerminalManager,
) {
    let tab_layout = app_state.config.tab_layout.as_str();
    let theme = app_state.config.active_theme();
    let tab_bg = ThemeColors::resolve(&theme.tab_bar_bg, "#19191e");
    let tab_fg = ThemeColors::resolve(&theme.tab_bar_fg, "#cccccc");
    let _status_bg = ThemeColors::resolve(&theme.status_bar_bg, "#1a1a20");
    let _status_fg = ThemeColors::resolve(&theme.status_bar_fg, "#cccccc");
    let _url_bg = ThemeColors::resolve(&theme.url_bar_bg, "#1a1a20");
    let _url_fg = ThemeColors::resolve(&theme.url_bar_fg, "#e0e0e0");
    let accent = ThemeColors::resolve(&theme.accent, "#4db4ff");
    let bg = ThemeColors::resolve(&theme.bg, "#191920");
    let border_color_default = ThemeColors::resolve(&theme.border, "#3c3c3c");

    if tab_layout == "sidebar" {
        let panel = if app_state.config.tab_sidebar_right {
            egui::SidePanel::right("tab-sidebar")
        } else {
            egui::SidePanel::left("tab-sidebar")
        };
        panel
            .default_width(app_state.config.tab_sidebar_width)
            .resizable(true)
            .frame(egui::Frame::new().fill(tab_bg))
            .show(ctx, |ui| {
                build_tab_list(ui, app_state, wry_panes, false, &theme);
            });
    } else if tab_layout == "topbar" {
        egui::TopBottomPanel::top("tab-bar").show(ctx, |ui| {
            build_tab_list(ui, app_state, wry_panes, true, &theme);
        });
    }

    egui::TopBottomPanel::top("status-bar").show(ctx, |ui| {
        egui::menu::bar(ui, |ui| {
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                let mode_color = match app_state.mode {
                    Mode::Normal => egui::Color32::from_rgb(100, 200, 100),
                    Mode::Insert => accent,
                    Mode::Command => egui::Color32::from_rgb(255, 200, 100),
                };
                ui.colored_label(mode_color, app_state.mode.as_str());

                ui.separator();

                let pane_count = app_state.wm.leaf_count();
                ui.label(format!("panes: {}", pane_count));

                let git_text = git_status.status_bar_text();
                if !git_text.is_empty() {
                    ui.separator();
                    let git_color = if git_status.is_dirty {
                        egui::Color32::from_rgb(255, 200, 100)
                    } else {
                        tab_fg
                    };
                    ui.colored_label(git_color, git_text);
                }

                ui.separator();

                let active_id = app_state.wm.active_pane_id();
                if let Some(wry_pane) = wry_panes.get(&active_id) {
                    let url_str = wry_pane.url().as_str();
                    let display_url = if url_str.len() > 60 {
                        format!("{}...", &url_str[..57])
                    } else {
                        url_str.to_string()
                    };
                    ui.label(display_url);
                }

                ui.separator();

                if app_state.hint_mode {
                    ui.colored_label(accent, format!("hint: {}", app_state.hint_buffer));
                } else if !app_state.status_message.is_empty() {
                    ui.label(&app_state.status_message);
                }
            });
        });
    });

    egui::TopBottomPanel::bottom("url-bar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            if app_state.palette.open {
                ui.label(":");
                let response = ui.add(
                    egui::TextEdit::singleline(&mut app_state.palette.query)
                        .desired_width(f32::INFINITY)
                        .hint_text("Search commands, history, bookmarks..."),
                );
                response.request_focus();

                let query_snapshot = app_state.palette.query.clone();
                app_state.palette.update_query(&query_snapshot);
                app_state.command_palette_input = app_state.palette.query.clone();
            } else if app_state.command_palette_open {
                ui.label(":");
                let response = ui.add(
                    egui::TextEdit::singleline(&mut app_state.command_palette_input)
                        .desired_width(f32::INFINITY)
                        .hint_text("Enter command..."),
                );
                response.request_focus();
            } else if app_state.url_bar_focused {
                ui.colored_label(accent, "URL>");
                let response = ui.add(
                    egui::TextEdit::singleline(&mut app_state.url_bar_input)
                        .desired_width(f32::INFINITY)
                        .hint_text("Search or enter URL..."),
                );
                response.request_focus();

                let query_snapshot = app_state.url_bar_input.clone();
                if query_snapshot != app_state.last_omnibox_query {
                    app_state.update_omnibox(&query_snapshot);
                }

                if !app_state.omnibox_results.is_empty() {
                    let popup_id = egui::Id::new("omnibox_popup");
                    let popup_height = (app_state.omnibox_results.len() as f32 * 24.0).min(200.0);

                    let bar_rect = ui.clip_rect();
                    egui::Area::new(popup_id)
                        .fixed_pos(egui::pos2(
                            bar_rect.left(),
                            bar_rect.top() - popup_height - 4.0,
                        ))
                        .order(egui::Order::Foreground)
                        .show(ui.ctx(), |ui| {
                            egui::Frame::popup(ui.style()).show(ui, |ui| {
                                ui.set_width(ui.available_width().max(400.0));
                                let mut clicked_index: Option<usize> = None;
                                for (i, item) in app_state.omnibox_results.iter().enumerate() {
                                    let selected = i == app_state.omnibox_selected;
                                    let category_prefix = match item.category {
                                        SearchCategory::Bookmark => "\u{2606}",
                                        SearchCategory::History => "\u{25CE}",
                                        _ => "\u{2192}",
                                    };

                                    if ui
                                        .selectable_label(
                                            selected,
                                            format!("{} {}", category_prefix, item.label),
                                        )
                                        .clicked()
                                    {
                                        clicked_index = Some(i);
                                    }
                                }
                                if let Some(idx) = clicked_index {
                                    app_state.handle_omnibox_select(idx);
                                    app_state.url_bar_focused = false;
                                    app_state.omnibox_results.clear();
                                    app_state.last_omnibox_query.clear();
                                }
                            });
                        });
                }

                if ui.input(|i| i.key_pressed(egui::Key::ArrowDown))
                    && app_state.omnibox_selected
                        < app_state.omnibox_results.len().saturating_sub(1)
                {
                    app_state.omnibox_selected += 1;
                }
                if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                    app_state.omnibox_selected = app_state.omnibox_selected.saturating_sub(1);
                }

                if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    if !app_state.omnibox_results.is_empty() {
                        app_state.handle_omnibox_select(app_state.omnibox_selected);
                    } else {
                        let input = app_state.url_bar_input.trim().to_string();
                        if !input.is_empty() {
                            let url = if input.starts_with("aileron://") || input.contains("://") {
                                url::Url::parse(&input).ok()
                            } else {
                                app_state.config.search_url(&input)
                            };
                            if let Some(url) = url {
                                app_state
                                    .pending_wry_actions
                                    .push_back(WryAction::Navigate(url));
                                app_state.status_message = format!("Navigating to {}", input);
                            }
                        }
                    }
                    app_state.url_bar_focused = false;
                    app_state.omnibox_results.clear();
                    app_state.last_omnibox_query.clear();
                }

                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    app_state.url_bar_focused = false;
                    app_state.url_bar_input.clear();
                    app_state.omnibox_results.clear();
                    app_state.last_omnibox_query.clear();
                }
            } else {
                let (mode_label, mode_color) = match app_state.mode {
                    Mode::Normal => ("NORMAL", egui::Color32::from_rgb(100, 200, 100)),
                    Mode::Insert => ("INSERT", accent),
                    Mode::Command => ("COMMAND", egui::Color32::from_rgb(200, 200, 100)),
                };
                ui.colored_label(mode_color, mode_label);
                ui.separator();

                let active_id = app_state.wm.active_pane_id();
                let url_str = if let Some(wry_pane) = wry_panes.get(&active_id) {
                    wry_pane.url().to_string()
                } else {
                    "aileron://welcome".to_string()
                };

                let url_label = ui.strong(&url_str);

                if url_label.clicked() {
                    app_state.url_bar_focused = true;
                    app_state.url_bar_input = url_str.clone();
                }
            }
        });
    });

    if app_state.find_bar_open {
        let area = egui::Area::new(egui::Id::new("find-bar"))
            .anchor(
                egui::Align2::LEFT_BOTTOM,
                egui::vec2(0.0, -(status_bar_height as f32)),
            )
            .order(egui::Order::Foreground);
        area.show(ctx, |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(
                    bg.r().saturating_add(20),
                    bg.g().saturating_add(20),
                    bg.b().saturating_add(20),
                ))
                .inner_margin(4.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.colored_label(accent, "Find:");
                        let response = ui.add(
                            egui::TextEdit::singleline(&mut app_state.find_query)
                                .desired_width(300.0)
                                .hint_text("Search in page..."),
                        );
                        response.request_focus();

                        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            let active_id = app_state.wm.active_pane_id();
                            if let Some(wry_pane) = wry_panes.get(&active_id) {
                                let q = app_state.find_query.replace('\'', "\\'");
                                wry_pane.execute_js(&format!(
                                    "window.find('{}', false, true, true, false, false)",
                                    q
                                ));
                            }
                        }

                        if ui.button("\u{2193}").clicked() {
                            let active_id = app_state.wm.active_pane_id();
                            if let Some(wry_pane) = wry_panes.get(&active_id) {
                                let q = app_state.find_query.replace('\'', "\\'");
                                wry_pane.execute_js(&format!(
                                    "window.find('{}', false, true, true, false, false)",
                                    q
                                ));
                            }
                        }
                        if ui.button("\u{2191}").clicked() {
                            let active_id = app_state.wm.active_pane_id();
                            if let Some(wry_pane) = wry_panes.get(&active_id) {
                                let q = app_state.find_query.replace('\'', "\\'");
                                wry_pane.execute_js(&format!(
                                    "window.find('{}', false, true, false, false, false)",
                                    q
                                ));
                            }
                        }
                        if ui.button("\u{2715}").clicked() {
                            app_state.find_bar_open = false;
                            app_state.find_query.clear();
                            let active_id = app_state.wm.active_pane_id();
                            if let Some(wry_pane) = wry_panes.get(&active_id) {
                                wry_pane.execute_js("window.getSelection().removeAllRanges()");
                            }
                        }
                    });
                });
        });
    }

    if app_state.palette.open {
        let results = app_state.palette.results().to_vec();
        if !results.is_empty() {
            let area = egui::Area::new(egui::Id::new("command-palette-results"))
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 40.0))
                .order(egui::Order::Foreground);
            area.show(ctx, |ui| {
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(
                        bg.r().saturating_add(10),
                        bg.g().saturating_add(10),
                        bg.b().saturating_add(10),
                    ))
                    .inner_margin(8.0)
                    .corner_radius(4.0)
                    .show(ui, |ui| {
                        ui.set_width(500.0);
                        ui.set_max_height(300.0);

                        egui::ScrollArea::vertical()
                            .max_height(280.0)
                            .show(ui, |ui| {
                                for item in results.iter() {
                                    let is_selected = app_state
                                        .palette
                                        .selected_item()
                                        .map(|s| s.id == item.id)
                                        .unwrap_or(false);

                                    let response = ui.selectable_label(
                                        is_selected,
                                        format!(
                                            "[{}] {} \u{2014} {}",
                                            match item.category {
                                                SearchCategory::History => "H",
                                                SearchCategory::Bookmark => "B",
                                                SearchCategory::Command => ">",
                                                SearchCategory::OpenTab => "T",
                                                SearchCategory::Setting => "S",
                                                SearchCategory::Credential => "\u{1f511}",
                                                SearchCategory::Custom => "\u{03bb}",
                                            },
                                            item.label,
                                            item.description
                                        ),
                                    );

                                    if response.clicked() {
                                        let selected = item.clone();
                                        app_state.palette.close();
                                        app_state.command_palette_open = false;
                                        app_state.command_palette_input.clear();
                                        app_state.execute_palette_selection(&selected);
                                    }

                                    if is_selected && response.hovered() {}
                                }
                            });
                    });
            });
        }
    }

    egui::CentralPanel::default().show(ctx, |ui| {
        let panes = app_state.wm.panes();
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
                    let is_terminal = terminal_manager.is_terminal(id);

                    if is_terminal {
                        // Native terminal rendering: draw grid directly with egui
                        if let Some(pane) = terminal_manager.get(id) {
                            let colors = TerminalColors::default();
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
                    } else if let Some(&tex_id) = webview_textures.get(id) {
                        // Web content: show captured webview texture
                        let image = egui::Image::new(egui::load::SizedTexture::new(
                            tex_id,
                            screen_rect.size(),
                        ));
                        ui.put(screen_rect, image);
                    } else {
                        ui.painter().rect_filled(screen_rect, 0.0, bg);
                        ui.painter().rect_stroke(
                            screen_rect,
                            0.0,
                            egui::Stroke::new(2.0, border_color),
                            egui::epaint::StrokeKind::Middle,
                        );
                    }
                }

                ui.painter().rect_stroke(
                    screen_rect,
                    0.0,
                    egui::Stroke::new(2.0, border_color),
                    egui::epaint::StrokeKind::Middle,
                );
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
                ui.heading("Aileron");
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
        }
    });
}

pub fn build_tab_list(
    ui: &mut egui::Ui,
    app_state: &mut AppState,
    wry_panes: &WryPaneManager,
    horizontal: bool,
    theme: &ThemeColors,
) {
    let panes = app_state.wm.panes();
    let active_id = app_state.wm.active_pane_id();
    let _accent = ThemeColors::resolve(&theme.accent, "#4db4ff");
    let _tab_fg = ThemeColors::resolve(&theme.tab_bar_fg, "#cccccc");
    let tab_bar_bg = ThemeColors::resolve(&theme.tab_bar_bg, "#19191e");
    let border_color = ThemeColors::resolve(&theme.border, "#3c3c3c");

    if horizontal {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
            for (pane_id, _rect) in &panes {
                let is_active = *pane_id == active_id;
                let is_terminal = app_state.terminal_pane_ids.contains(pane_id);

                let (title, _url) = wry_panes
                    .get(pane_id)
                    .map(|p| {
                        let t = p.title();
                        let u = p.url().to_string();
                        (
                            if t.is_empty() || t == "about:blank" {
                                u.rsplit('/').next().unwrap_or("New Tab").to_string()
                            } else {
                                t.to_string()
                            },
                            u,
                        )
                    })
                    .unwrap_or_else(|| ("New Tab".into(), "aileron://new".into()));

                let display_title = if title.len() > 24 {
                    format!("{}...", &title[..21])
                } else {
                    title.clone()
                };

                let frame_color = if is_active {
                    egui::Color32::from_rgb(40, 60, 90)
                } else {
                    tab_bar_bg
                };

                egui::Frame::new().fill(frame_color).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.add_space(8.0);

                        let icon = if is_terminal { "\u{2328} " } else { "  " };
                        ui.label(icon);

                        let muted_prefix = if app_state.muted_pane_ids.contains(pane_id) {
                            "\u{1f507} "
                        } else {
                            ""
                        };
                        let pinned_prefix = if app_state.pinned_pane_ids.contains(pane_id) {
                            "\u{1f4cc} "
                        } else {
                            ""
                        };

                        let response = ui.selectable_label(
                            is_active,
                            format!("{}{}{}", pinned_prefix, muted_prefix, display_title),
                        );
                        if response.clicked() && !is_active {
                            app_state.wm.set_active_pane(*pane_id);
                            app_state.update_status();
                        }

                        if ui.small_button("\u{00d7}").clicked() {
                            app_state.pending_tab_close = Some(*pane_id);
                        }

                        ui.add_space(4.0);
                    });
                });
            }
        });
    } else {
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (pane_id, _rect) in &panes {
                let is_active = *pane_id == active_id;
                let is_terminal = app_state.terminal_pane_ids.contains(pane_id);

                let (title, url) = wry_panes
                    .get(pane_id)
                    .map(|p| {
                        let t = p.title();
                        let u = p.url().to_string();
                        (
                            if t.is_empty() || t == "about:blank" {
                                u.rsplit('/').next().unwrap_or("New Tab").to_string()
                            } else {
                                t.to_string()
                            },
                            u,
                        )
                    })
                    .unwrap_or_else(|| ("New Tab".into(), "aileron://new".into()));

                let display_title = if title.len() > 20 {
                    format!("{}...", &title[..17])
                } else {
                    title.clone()
                };

                let frame_color = if is_active {
                    egui::Color32::from_rgb(40, 60, 90)
                } else {
                    tab_bar_bg
                };

                egui::Frame::new().fill(frame_color).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let icon = if is_terminal { "\u{2328}" } else { "\u{1f310}" };
                        ui.label(icon);

                        let muted_prefix = if app_state.muted_pane_ids.contains(pane_id) {
                            "\u{1f507} "
                        } else {
                            ""
                        };
                        let pinned_prefix = if app_state.pinned_pane_ids.contains(pane_id) {
                            "\u{1f4cc} "
                        } else {
                            ""
                        };

                        let response = ui.selectable_label(
                            is_active,
                            format!("{}{}{}", pinned_prefix, muted_prefix, display_title),
                        );
                        if response.clicked() && !is_active {
                            app_state.wm.set_active_pane(*pane_id);
                            app_state.update_status();
                        }

                        if ui.small_button("\u{00d7}").clicked() {
                            app_state.pending_tab_close = Some(*pane_id);
                        }
                    });

                    if !is_terminal {
                        let display_url = if url.len() > 22 {
                            format!("{}...", &url[..19])
                        } else {
                            url
                        };
                        ui.label(egui::RichText::new(display_url).small().color(border_color));
                    } else {
                        ui.label(egui::RichText::new("Terminal").small().color(border_color));
                    }
                });
                ui.add_space(2.0);
            }
        });
    }
}
