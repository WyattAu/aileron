use crate::app::{AppState, WryAction};
use crate::git::GitStatus;
use crate::input::Mode;
use crate::servo::WryPaneManager;
use crate::terminal::NativeTerminalManager;
use crate::terminal::grid::{CellMetrics, TerminalColors};
use crate::terminal::render::render_terminal;
use crate::ui::search::SearchCategory;
use egui::{WidgetInfo, WidgetType};

fn a11y_info(typ: WidgetType, label: impl Into<String>) -> WidgetInfo {
    WidgetInfo {
        typ,
        label: Some(label.into()),
        ..WidgetInfo::new(typ)
    }
}

/// Truncate a string to at most `max_chars` characters without splitting multi-byte UTF-8.
fn truncate_str(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars).collect();
        format!("{truncated}...")
    }
}

#[allow(clippy::too_many_arguments)]
pub fn build_ui(
    ctx: &egui::Context,
    app_state: &mut AppState,
    wry_panes: &WryPaneManager,
    git_status: &GitStatus,
    status_bar_height: f64,
    webview_textures: &std::collections::HashMap<uuid::Uuid, egui::TextureId>,
    terminal_manager: &NativeTerminalManager,
    offscreen_panes: &crate::offscreen_webview::OffscreenWebViewManager,
) {
    let tab_layout = app_state.config.tab_layout.as_str();
    let tc = app_state.config.cached_theme_colors().clone();
    let tab_bg = tc.tab_bar_bg;
    let tab_fg = tc.tab_bar_fg;
    let _status_bg = tc.status_bar_bg;
    let _status_fg = tc.status_bar_fg;
    let _url_bg = tc.url_bar_bg;
    let _url_fg = tc.url_bar_fg;
    let accent = tc.accent;
    let bg = tc.bg;
    let border_color_default = tc.border;

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
                build_tab_list(ui, app_state, wry_panes, false, &tc.clone());
            });
    } else if tab_layout == "topbar" {
        egui::TopBottomPanel::top("tab-bar").show(ctx, |ui| {
            build_tab_list(ui, app_state, wry_panes, true, &tc.clone());
        });
    }

    egui::TopBottomPanel::top("status-bar").show(ctx, |ui| {
        egui::menu::bar(ui, |ui| {
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                // TODO(a11y): egui status bar equivalent of aria-live="polite"
                // for screen reader announcements on mode/status changes
                let mode_color = match app_state.mode {
                    Mode::Normal => egui::Color32::from_rgb(100, 200, 100),
                    Mode::Insert => accent,
                    Mode::Command => egui::Color32::from_rgb(255, 200, 100),
                };
                let mut mode_str = app_state.mode.as_str().to_string();

                // Show sub-mode indicators
                if app_state.hint_mode {
                    mode_str = format!("{} HINT[{}]", mode_str, &app_state.hint_buffer);
                } else if app_state.hint_new_tab {
                    mode_str = format!("{} HINT-TAB[{}]", mode_str, &app_state.hint_buffer);
                } else if app_state.find_bar_open {
                    mode_str = format!("{} FIND", mode_str);
                } else if app_state.url_bar_focused {
                    mode_str = format!("{} URL", mode_str);
                } else if app_state.tab_search_open {
                    mode_str = format!("{} TABS", mode_str);
                } else if app_state.history_panel_open {
                    mode_str = format!("{} HIST", mode_str);
                } else if app_state.bookmarks_panel_open {
                    mode_str = format!("{} BM", mode_str);
                } else if app_state.help_panel_open {
                    mode_str = format!("{} HELP", mode_str);
                }

                ui.colored_label(mode_color, &mode_str).widget_info(|| {
                    a11y_info(WidgetType::Label, format!("Current mode: {}", mode_str))
                });

                ui.separator();

                let pane_count = app_state.wm.leaf_count();
                ui.label(format!("panes: {}", pane_count))
                    .widget_info(|| a11y_info(WidgetType::Label, format!("Panes: {}", pane_count)));

                // Private mode indicator
                if app_state
                    .private_pane_ids
                    .contains(&app_state.wm.active_pane_id())
                {
                    ui.separator();
                    ui.colored_label(egui::Color32::from_rgb(255, 100, 100), "[PRIVATE]");
                }

                if app_state.current_workspace_name != "default" {
                    ui.separator();
                    let ws_name = app_state.current_workspace_name.clone();
                    ui.colored_label(
                        egui::Color32::from_rgb(180, 180, 255),
                        format!("[{}]", ws_name),
                    )
                    .widget_info(|| {
                        a11y_info(
                            WidgetType::Label,
                            format!("Workspace: {}", app_state.current_workspace_name),
                        )
                    });
                }

                if app_state.adblock_blocked_count > 0 {
                    ui.separator();
                    let blocked = app_state.adblock_blocked_count;
                    ui.colored_label(
                        egui::Color32::from_rgb(255, 100, 100),
                        format!("[AB: {}]", blocked),
                    )
                    .widget_info(|| {
                        a11y_info(WidgetType::Label, format!("Blocked ads: {}", blocked))
                    });
                }

                if app_state.config.engine_selection != "webkit" {
                    ui.separator();
                    let engine_text = format!("[{}]", app_state.config.engine_selection);
                    let engine_color = if app_state.config.engine_selection == "servo" {
                        egui::Color32::from_rgb(100, 200, 255)
                    } else {
                        egui::Color32::from_rgb(200, 200, 100)
                    };
                    let et = engine_text.clone();
                    ui.colored_label(engine_color, engine_text)
                        .widget_info(|| a11y_info(WidgetType::Label, format!("Engine: {}", et)));
                }

                let git_text = git_status.status_bar_text();
                if !git_text.is_empty() {
                    ui.separator();
                    let git_color = if git_status.is_dirty {
                        egui::Color32::from_rgb(255, 200, 100)
                    } else {
                        tab_fg
                    };
                    let gt = git_text.clone();
                    ui.colored_label(git_color, git_text)
                        .widget_info(|| a11y_info(WidgetType::Label, format!("Git: {}", gt)));
                }

                ui.separator();

                let active_id = app_state.wm.active_pane_id();
                if let Some(wry_pane) = wry_panes.get(&active_id) {
                    let url_str = wry_pane.url().as_str();
                    let display_url = truncate_str(url_str, 57);
                    let full_url = url_str.to_string();
                    let url_resp = ui.label(display_url.clone());
                    url_resp.widget_info(|| {
                        a11y_info(WidgetType::Label, format!("Current URL: {}", full_url))
                    });
                    if url_resp.clicked() {
                        app_state.url_bar_focused = true;
                        app_state.url_bar_input = full_url;
                    }
                } else if let Some(pane) = offscreen_panes.get(&active_id) {
                    let url_str = pane.url().as_str();
                    let display_url = truncate_str(url_str, 57);
                    let full_url = url_str.to_string();
                    let url_resp = ui.label(display_url.clone());
                    url_resp.widget_info(|| {
                        a11y_info(WidgetType::Label, format!("Current URL: {}", full_url))
                    });
                    if url_resp.clicked() {
                        app_state.url_bar_focused = true;
                        app_state.url_bar_input = full_url;
                    }
                }

                ui.separator();

                // Show zoom level if non-default
                if let Some(zoom) = app_state.site_settings_zoom
                    && (zoom - 1.0).abs() > 0.01
                {
                    let pct = (zoom * 100.0).round() as u32;
                    let zoom_text = format!("{}%", pct);
                    ui.colored_label(egui::Color32::from_rgb(180, 180, 100), zoom_text);
                    ui.separator();
                }

                // Show download progress if any active downloads
                if app_state.download_manager.has_active() {
                    let progress = app_state.download_manager.progress_all();
                    let active: Vec<_> = progress
                        .iter()
                        .filter(|p| {
                            matches!(p.state, crate::downloads::DownloadState::Downloading)
                                && p.fraction < 1.0
                        })
                        .take(2)
                        .collect();
                    if let Some(dl) = active.first() {
                        let dl_text = format!(
                            "DL {:.0}% ({}/s)",
                            dl.fraction * 100.0,
                            crate::downloads::DownloadProgress::format_bytes(
                                dl.speed_bytes_per_sec as u64
                            ),
                        );
                        let dl_color = egui::Color32::from_rgb(100, 200, 100);
                        ui.colored_label(dl_color, &dl_text).widget_info(|| {
                            a11y_info(WidgetType::Label, format!("Download: {}", dl_text))
                        });
                    }
                    ui.separator();
                }

                if app_state.autofill_available {
                    ui.separator();
                    let autofill_resp = ui.colored_label(
                        egui::Color32::from_rgb(100, 200, 255),
                        "[autofill available]",
                    );
                    autofill_resp.widget_info(|| {
                        a11y_info(
                            WidgetType::Label,
                            "Auto-fill credentials available - click to fill",
                        )
                    });
                    if autofill_resp.clicked()
                        && let Some(js) = app_state.autofill_js.take()
                    {
                        app_state
                            .pending_wry_actions
                            .push_back(WryAction::RunJs(js));
                        app_state.status_message = app_state.autofill_status_msg.clone();
                        app_state.autofill_available = false;
                    }
                }

                if app_state.hint_mode {
                    let hint_text = format!("hint: {}", app_state.hint_buffer);
                    ui.colored_label(accent, hint_text.clone())
                        .widget_info(|| a11y_info(WidgetType::Label, hint_text.clone()));
                } else if !app_state.status_message.is_empty() {
                    let msg = app_state.status_message.clone();
                    ui.label(&msg)
                        .widget_info(|| a11y_info(WidgetType::Label, format!("Status: {}", msg)));
                }
            });
        });
    });

    egui::TopBottomPanel::bottom("url-bar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            if app_state.palette.open {
                ui.label(":")
                    .widget_info(|| a11y_info(WidgetType::Label, "Command palette prompt"));
                let response = ui.add(
                    egui::TextEdit::singleline(&mut app_state.palette.query)
                        .desired_width(f32::INFINITY)
                        .hint_text("Search commands, history, bookmarks..."),
                );
                response.widget_info(|| a11y_info(WidgetType::TextEdit, "Command palette"));
                response.request_focus();

                let query_snapshot = app_state.palette.query.clone();
                app_state.palette.update_query(&query_snapshot);
                app_state.command_palette_input = app_state.palette.query.clone();
            } else if app_state.url_bar_focused {
                ui.colored_label(accent, "URL>").widget_info(|| {
                    a11y_info(WidgetType::Label, "URL bar mode indicator: editing")
                });
                let response = ui.add(
                    egui::TextEdit::singleline(&mut app_state.url_bar_input)
                        .desired_width(f32::INFINITY)
                        .hint_text("Search or enter URL..."),
                );
                response.widget_info(|| a11y_info(WidgetType::TextEdit, "URL bar"));
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

                // Help panel overlay
                if app_state.help_panel_open {
                    let help_sections: &[(&str, &[(&str, &str)])] = &[
                        (
                            "Navigation",
                            &[
                                ("j / k", "Scroll down / up"),
                                ("Ctrl+D / Ctrl+U", "Half page down / up"),
                                ("Ctrl+F", "Find in page"),
                                ("G / gg", "Scroll to bottom / top"),
                                ("H / L", "Go back / forward"),
                                ("f", "Toggle link hints"),
                                ("r", "Reload page"),
                                ("m' / 'a", "Set / jump to scroll mark"),
                            ],
                        ),
                        (
                            "Panes & Tabs",
                            &[
                                ("Ctrl+W / Ctrl+S", "Split vertical / horizontal"),
                                ("Ctrl+H/J/K/L", "Navigate panes"),
                                ("q", "Close pane"),
                                ("w", "Close all panes except current"),
                                ("Ctrl+Shift+D", "Detach pane to popup"),
                                ("Ctrl+Shift+P", "Pin / unpin pane"),
                                (":tab-restore", "Reopen closed tab"),
                                (":tabs", "Search open tabs"),
                            ],
                        ),
                        (
                            "Modes",
                            &[
                                ("i", "Enter Insert mode"),
                                ("Esc", "Return to Normal mode"),
                                ("Ctrl+P", "Open command palette"),
                                (":help", "Show this help panel"),
                            ],
                        ),
                        (
                            "URL & Search",
                            &[
                                ("o <url>", "Open URL"),
                                ("O <url>", "Open in new tab"),
                                ("y", "Copy URL to clipboard"),
                                ("Ctrl+E", "Open in system browser"),
                                (":engine <name>", "Switch search engine"),
                                ("a-s / a-S", "Save / search quickmark"),
                            ],
                        ),
                        (
                            "Privacy & Security",
                            &[
                                ("Ctrl+B", "Toggle bookmark"),
                                (":bookmarks", "View bookmarks"),
                                (":adblock-toggle", "Toggle ad block"),
                                (":privacy", "Privacy dashboard"),
                                (":cookies", "View cookies"),
                                (":site-settings", "Per-site settings"),
                            ],
                        ),
                        (
                            "Terminal",
                            &[
                                ("`", "Open terminal pane"),
                                (":ssh <host>", "SSH quick-connect"),
                                (":terminal-clear", "Clear terminal"),
                                (":terminal-search", "Search scrollback"),
                                (":! <cmd>", "Run shell command"),
                            ],
                        ),
                        (
                            "Developer",
                            &[
                                ("F12", "Toggle dev tools"),
                                ("Ctrl+Shift+N", "Network log"),
                                ("Ctrl+Shift+J", "Console log"),
                                (":inspect", "WebKit inspector"),
                                (":gs / :gl / :gd", "Git status / log / diff"),
                                (":grep <pat>", "Search project (ripgrep)"),
                            ],
                        ),
                        (
                            "Sessions",
                            &[
                                (":ws-save <name>", "Save workspace"),
                                (":ws-load <name>", "Load workspace"),
                                (":ws-list", "List workspaces"),
                                (":layout-save <n>", "Save layout"),
                                (":layout-load <n>", "Load layout"),
                            ],
                        ),
                    ];

                    egui::Window::new("Help")
                        .default_width(640.0)
                        .default_height(520.0)
                        .resizable(true)
                        .collapsible(false)
                        .frame(egui::Frame::new().fill(bg))
                        .pivot(egui::Align2::CENTER_CENTER)
                        .default_pos(ctx.screen_rect().center())
                        .show(ctx, |ui| {
                            ui.strong("Aileron Keybindings");
                            ui.label("Press Esc or ? to close");
                            ui.separator();

                            egui::ScrollArea::vertical().show(ui, |ui| {
                                for (section, bindings) in help_sections {
                                    ui.collapsing(*section, |ui| {
                                        egui::Grid::new(format!("help_grid_{section}"))
                                            .num_columns(2)
                                            .striped(true)
                                            .min_col_width(140.0)
                                            .show(ui, |ui| {
                                                for (key, desc) in *bindings {
                                                    ui.label(
                                                        egui::RichText::new(*key).color(accent),
                                                    );
                                                    ui.label(*desc);
                                                    ui.end_row();
                                                }
                                            });
                                    });
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
                let ml = mode_label;
                ui.colored_label(mode_color, mode_label)
                    .widget_info(|| a11y_info(WidgetType::Label, format!("URL bar mode: {}", ml)));
                ui.separator();

                let active_id = app_state.wm.active_pane_id();
                let url_str = if let Some(wry_pane) = wry_panes.get(&active_id) {
                    wry_pane.url().to_string()
                } else {
                    "aileron://welcome".to_string()
                };

                let url_clone = url_str.clone();
                let url_label = ui.strong(&url_str);
                url_label
                    .widget_info(|| a11y_info(WidgetType::Label, format!("URL: {}", url_clone)));

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
                        response.widget_info(|| a11y_info(WidgetType::TextEdit, "Find in page"));
                        response.request_focus();

                        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            let active_id = app_state.wm.active_pane_id();
                            if let Some(wry_pane) = wry_panes.get(&active_id) {
                                let q = app_state.find_query.replace('\'', "\\'");
                                // Store query in JS for FindNext/FindPrev reuse
                                wry_pane.execute_js(&format!("window._aileronFindQuery='{}'", q));
                                wry_pane.execute_js(&format!(
                                    "window.find('{}', false, false, true, true, false)",
                                    q
                                ));
                            }
                        }

                        let find_next = ui.button("\u{2193}");
                        find_next.widget_info(|| a11y_info(WidgetType::Button, "Find next"));
                        if find_next.clicked() {
                            let active_id = app_state.wm.active_pane_id();
                            if let Some(wry_pane) = wry_panes.get(&active_id) {
                                // find(query, caseSensitive, backwards, findNext, matchCount, wrapAround)
                                wry_pane.execute_js("window.find(window._aileronFindQuery||'',false,false,true,true,false)");
                            }
                        }
                        let find_prev = ui.button("\u{2191}");
                        find_prev.widget_info(|| a11y_info(WidgetType::Button, "Find previous"));
                        if find_prev.clicked() {
                            let active_id = app_state.wm.active_pane_id();
                            if let Some(wry_pane) = wry_panes.get(&active_id) {
                                wry_pane.execute_js("window.find(window._aileronFindQuery||'',false,true,false,true,false)");
                            }
                        }
                        let find_close = ui.button("\u{2715}");
                        find_close.widget_info(|| a11y_info(WidgetType::Button, "Close find bar"));
                        if find_close.clicked() {
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

    // ─── History Panel ───
    if app_state.history_panel_open {
        let bg = egui::Color32::from_rgb(0x19, 0x19, 0x20);
        let accent = egui::Color32::from_rgb(0x4d, 0xb4, 0xff);
        let text = egui::Color32::from_rgb(0xd4, 0xd4, 0xd4);

        egui::Window::new("History")
            .title_bar(false)
            .collapsible(false)
            .resizable(true)
            .default_width(600.0)
            .default_height(500.0)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .frame(
                egui::Frame::new()
                    .fill(bg)
                    .inner_margin(12.0)
                    .corner_radius(6.0)
                    .stroke(egui::Stroke::new(
                        1.0,
                        egui::Color32::from_rgb(0x40, 0x40, 0x50),
                    )),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("History")
                            .size(16.0)
                            .color(accent)
                            .strong(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("✕").clicked() {
                            app_state.history_panel_open = false;
                            app_state.history_entries.clear();
                        }
                    });
                });
                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);

                egui::ScrollArea::vertical()
                    .max_height(430.0)
                    .show(ui, |ui| {
                        if app_state.history_entries.is_empty() {
                            ui.label(
                                egui::RichText::new("No history entries")
                                    .color(egui::Color32::GRAY),
                            );
                        }
                        let mut navigate_to: Option<url::Url> = None;
                        for (i, entry) in app_state.history_entries.iter().enumerate() {
                            let is_selected = i == app_state.history_selected;
                            let response =
                                ui.selectable_label(
                                    is_selected,
                                    egui::RichText::new(format!(
                                        "{}  {}  [{}×]",
                                        entry.title, entry.url, entry.visit_count,
                                    ))
                                    .size(13.0)
                                    .color(if is_selected { accent } else { text }),
                                );
                            if response.clicked() {
                                navigate_to = url::Url::parse(&entry.url).ok();
                                app_state.history_selected = i;
                            }
                            // Scroll selected item into view
                            if is_selected {
                                response.scroll_to_me(Some(egui::Align::Center));
                            }
                            // Tooltip with full URL and timestamp
                            response.on_hover_text(format!(
                                "{}\nVisited: {}\nVisits: {}",
                                entry.url, entry.visited_at, entry.visit_count
                            ));
                        }
                        if let Some(url) = navigate_to {
                            app_state
                                .pending_wry_actions
                                .push_back(crate::app::WryAction::Navigate(url));
                            app_state.history_panel_open = false;
                            app_state.history_entries.clear();
                        }
                    });
            });
    }

    // ─── Tab Search Panel ───
    if app_state.tab_search_open {
        let bg = egui::Color32::from_rgb(0x19, 0x19, 0x20);
        let accent = egui::Color32::from_rgb(0x4d, 0xb4, 0xff);
        let text = egui::Color32::from_rgb(0xd4, 0xd4, 0xd4);
        let dim = egui::Color32::from_rgb(0x88, 0x88, 0x88);

        egui::Window::new("tab-search")
            .title_bar(false)
            .collapsible(false)
            .resizable(true)
            .default_width(500.0)
            .default_height(400.0)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .frame(
                egui::Frame::new()
                    .fill(bg)
                    .inner_margin(12.0)
                    .corner_radius(6.0)
                    .stroke(egui::Stroke::new(
                        1.0,
                        egui::Color32::from_rgb(0x40, 0x40, 0x50),
                    )),
            )
            .show(ctx, |ui| {
                // Header
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Tabs")
                            .size(16.0)
                            .color(accent)
                            .strong(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("✕").clicked() {
                            app_state.tab_search_open = false;
                        }
                    });
                });

                // Search filter
                let search_response = ui.add(
                    egui::TextEdit::singleline(&mut app_state.tab_search_query)
                        .hint_text("Filter tabs...")
                        .desired_width(f32::INFINITY)
                        .text_color(text),
                );
                search_response.request_focus();

                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);

                // Tab list with fuzzy filter
                let query = app_state.tab_search_query.to_lowercase();
                let pane_ids = app_state.wm.pane_ids();
                let active_id = app_state.wm.active_pane_id();

                let mut switch_to: Option<uuid::Uuid> = None;
                let mut close_tab: Option<uuid::Uuid> = None;

                egui::ScrollArea::vertical()
                    .max_height(300.0)
                    .show(ui, |ui| {
                        let mut visible_index = 0usize;
                        for id in &pane_ids {
                            let url = wry_panes
                                .url_for(id)
                                .map(|u| u.to_string())
                                .unwrap_or_default();
                            let title = wry_panes
                                .get(id)
                                .map(|p| p.title().to_string())
                                .unwrap_or_default();

                            // Simple substring filter (not fuzzy, but good enough)
                            if !query.is_empty() {
                                let matches = title.to_lowercase().contains(&query)
                                    || url.to_lowercase().contains(&query)
                                    || id.to_string().starts_with(&query);
                                if !matches {
                                    continue;
                                }
                            }

                            let is_active = *id == active_id;
                            let is_selected = visible_index == app_state.tab_search_selected;
                            let is_terminal = app_state.terminal_pane_ids.contains(id);
                            let prefix = if is_terminal { "[term] " } else { "" };
                            let marker = if is_active { " ●" } else { "" };

                            ui.horizontal(|ui| {
                                let label = format!("{}{}{}  {}", prefix, title, marker, url);
                                let response = ui.selectable_label(
                                    is_selected || is_active,
                                    egui::RichText::new(label).size(13.0).color(if is_selected {
                                        accent
                                    } else if is_active {
                                        text
                                    } else {
                                        dim
                                    }),
                                );
                                if response.clicked() {
                                    switch_to = Some(*id);
                                    app_state.tab_search_selected = visible_index;
                                }
                                if is_selected {
                                    response.scroll_to_me(Some(egui::Align::Center));
                                }

                                if ui.small_button("✕").clicked() {
                                    close_tab = Some(*id);
                                }
                            });
                            visible_index += 1;
                        }

                        // Clamp selection to visible count
                        if visible_index > 0 && app_state.tab_search_selected >= visible_index {
                            app_state.tab_search_selected = visible_index - 1;
                        }

                        if pane_ids.is_empty() {
                            ui.label(egui::RichText::new("No open tabs").color(dim));
                        }
                    });

                if let Some(id) = switch_to {
                    app_state.wm.set_active_pane(id);
                }
                if let Some(id) = close_tab {
                    let _ = app_state.wm.close(id);
                    app_state.session_dirty = true;
                }
            });
    }

    // ─── Bookmarks Panel ───
    if app_state.bookmarks_panel_open {
        let bg = egui::Color32::from_rgb(0x19, 0x19, 0x20);
        let accent = egui::Color32::from_rgb(0x4d, 0xb4, 0xff);
        let text = egui::Color32::from_rgb(0xd4, 0xd4, 0xd4);

        egui::Window::new("bookmarks")
            .title_bar(false)
            .collapsible(false)
            .resizable(true)
            .default_width(550.0)
            .default_height(450.0)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .frame(
                egui::Frame::new()
                    .fill(bg)
                    .inner_margin(12.0)
                    .corner_radius(6.0)
                    .stroke(egui::Stroke::new(
                        1.0,
                        egui::Color32::from_rgb(0x40, 0x40, 0x50),
                    )),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Bookmarks")
                            .size(16.0)
                            .color(accent)
                            .strong(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("Import Chrome").clicked() {
                            app_state.pending_import = Some("chrome".into());
                            app_state.status_message = "Importing Chrome bookmarks...".into();
                        }
                        if ui.small_button("Import Firefox").clicked() {
                            app_state.pending_import = Some("firefox".into());
                            app_state.status_message = "Importing Firefox bookmarks...".into();
                        }
                        ui.add_space(8.0);
                        if ui.button("✕").clicked() {
                            app_state.bookmarks_panel_open = false;
                            app_state.bookmarks_entries.clear();
                        }
                    });
                });
                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);

                egui::ScrollArea::vertical()
                    .max_height(380.0)
                    .show(ui, |ui| {
                        if app_state.bookmarks_entries.is_empty() {
                            ui.label(
                                egui::RichText::new("No bookmarks").color(egui::Color32::GRAY),
                            );
                        }
                        let mut navigate_to: Option<url::Url> = None;
                        let mut delete_id: Option<i64> = None;

                        // Group bookmarks by folder for display
                        let mut last_folder = String::new();
                        for (i, bm) in app_state.bookmarks_entries.iter().enumerate() {
                            // Show folder header when folder changes
                            if last_folder != bm.folder {
                                last_folder.clone_from(&bm.folder);
                                let folder_label = if bm.folder.is_empty() {
                                    "📌 Unsorted"
                                } else {
                                    bm.folder.as_str()
                                };
                                ui.colored_label(
                                    egui::Color32::from_rgb(140, 180, 255),
                                    egui::RichText::new(format!("  {}", folder_label))
                                        .size(12.0)
                                        .strong(),
                                );
                                ui.add_space(2.0);
                            }

                            let is_selected = i == app_state.bookmarks_selected;
                            ui.horizontal(|ui| {
                                let label = format!("{}  {}", bm.title, bm.url);
                                let response =
                                    ui.selectable_label(
                                        is_selected,
                                        egui::RichText::new(label)
                                            .size(13.0)
                                            .color(if is_selected { accent } else { text }),
                                    );
                                if response.clicked() {
                                    navigate_to = url::Url::parse(&bm.url).ok();
                                    app_state.bookmarks_selected = i;
                                }
                                if is_selected {
                                    response.scroll_to_me(Some(egui::Align::Center));
                                }
                                response.on_hover_text(format!(
                                    "Folder: {} | Created: {}\nID: {}",
                                    if bm.folder.is_empty() {
                                        "(unsorted)"
                                    } else {
                                        &bm.folder
                                    },
                                    bm.created_at,
                                    bm.id
                                ));
                                if ui.small_button("✕").clicked() {
                                    delete_id = Some(bm.id);
                                }
                            });
                        }
                        if let Some(url) = navigate_to {
                            app_state
                                .pending_wry_actions
                                .push_back(crate::app::WryAction::Navigate(url));
                            app_state.bookmarks_panel_open = false;
                            app_state.bookmarks_entries.clear();
                        }
                        if let Some(id) = delete_id
                            && let Some(db) = app_state.db.as_ref()
                        {
                            let _ = crate::db::bookmarks::remove_bookmark_by_id(db, id);
                            app_state.bookmarks_entries.retain(|b| b.id != id);
                            if app_state.bookmarks_selected >= app_state.bookmarks_entries.len() {
                                app_state.bookmarks_selected =
                                    app_state.bookmarks_entries.len().saturating_sub(1);
                            }
                        }
                    });
            });
    }

    // Per-site settings panel
    if app_state.site_settings_panel_open {
        egui::Window::new("site-settings")
            .title_bar(true)
            .resizable(true)
            .default_width(320.0)
            .default_height(350.0)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("URL Pattern:");
                    let mut pat = app_state.site_settings_url_pattern.clone();
                    ui.text_edit_singleline(&mut pat);
                    app_state.site_settings_url_pattern = pat;
                });
                ui.add_space(4.0);

                // Zoom level
                ui.horizontal(|ui| {
                    ui.label("Zoom:");
                    let mut zoom = app_state.site_settings_zoom.unwrap_or(100.0);
                    if ui
                        .add(egui::Slider::new(&mut zoom, 25.0..=300.0).suffix("%"))
                        .changed()
                    {
                        app_state.site_settings_zoom = Some(zoom);
                    }
                    if ui.small_button("reset").clicked() {
                        app_state.site_settings_zoom = None;
                    }
                });

                // Toggles
                ui.add_space(4.0);
                egui::Grid::new("site_settings_grid")
                    .num_columns(2)
                    .spacing([8.0, 4.0])
                    .show(ui, |ui| {
                        ui.label("JavaScript:");
                        let mut js = app_state.site_settings_js.unwrap_or(true);
                        if ui.checkbox(&mut js, "").changed() {
                            app_state.site_settings_js = Some(js);
                        }
                        ui.end_row();

                        ui.label("Cookies:");
                        let mut cookies = app_state.site_settings_cookies.unwrap_or(true);
                        if ui.checkbox(&mut cookies, "").changed() {
                            app_state.site_settings_cookies = Some(cookies);
                        }
                        ui.end_row();

                        ui.label("AdBlock:");
                        let mut adblock = app_state.site_settings_adblock.unwrap_or(true);
                        if ui.checkbox(&mut adblock, "").changed() {
                            app_state.site_settings_adblock = Some(adblock);
                        }
                        ui.end_row();
                    });

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        let pattern = if app_state.site_settings_url_pattern.is_empty() {
                            // Use current active pane URL as wildcard pattern
                            let active_id = app_state.wm.active_pane_id();
                            wry_panes
                                .url_for(&active_id)
                                .map(|u| {
                                    if let Some(host) = u.host_str() {
                                        format!("*://{}*", host)
                                    } else {
                                        u.to_string()
                                    }
                                })
                                .unwrap_or_else(|| "*".into())
                        } else {
                            app_state.site_settings_url_pattern.clone()
                        };
                        if let Some(db) = app_state.db.as_ref() {
                            macro_rules! save_field {
                                ($field:expr, $val:expr) => {
                                    let _ = crate::db::site_settings::set_site_field(
                                        db, &pattern, "wildcard", $field, $val,
                                    );
                                };
                            }
                            save_field!(
                                "zoom",
                                app_state
                                    .site_settings_zoom
                                    .map(|z| z.to_string())
                                    .as_deref()
                            );
                            save_field!(
                                "adblock",
                                app_state
                                    .site_settings_adblock
                                    .map(|v| if v { "1" } else { "0" })
                            );
                            save_field!(
                                "javascript",
                                app_state
                                    .site_settings_js
                                    .map(|v| if v { "1" } else { "0" })
                            );
                            save_field!(
                                "cookies",
                                app_state
                                    .site_settings_cookies
                                    .map(|v| if v { "1" } else { "0" })
                            );
                            app_state.status_message =
                                format!("Saved site settings for: {}", pattern);
                        }
                    }
                    if ui.button("Close").clicked() {
                        app_state.site_settings_panel_open = false;
                    }
                });
            });
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
            let handle_thickness = 6.0; // pixels of draggable area
            for (pos, direction, pane_a_id, _pane_b_id) in &borders {
                let handle_rect = match direction {
                    crate::wm::rect::SplitDirection::Horizontal => {
                        // Vertical border line at x=pos
                        let x = available.min.x + *pos as f32;
                        egui::Rect::from_min_max(
                            egui::pos2(x - handle_thickness / 2.0, available.min.y),
                            egui::pos2(x + handle_thickness / 2.0, available.max.y),
                        )
                    }
                    crate::wm::rect::SplitDirection::Vertical => {
                        // Horizontal border line at y=pos
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
                    // Change cursor
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
                    // Apply resize to both adjacent panes
                    let viewport = app_state
                        .wm
                        .panes()
                        .iter()
                        .find_map(|(id, r)| if *id == *pane_a_id { Some(*r) } else { None });
                    if let Some(viewport) = viewport {
                        let resize_amount = match direction {
                            crate::wm::rect::SplitDirection::Horizontal => {
                                (amount as f64) / viewport.w.max(1.0)
                            }
                            crate::wm::rect::SplitDirection::Vertical => {
                                (amount as f64) / viewport.h.max(1.0)
                            }
                        };
                        let _ = app_state.wm.resize_pane(*pane_a_id, resize_amount as f64);
                    }
                }

                // Draw subtle visual indicator on hover
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
            // Single-pane offscreen rendering: show the captured webview texture.
            let available = ui.available_rect_before_wrap();
            let is_terminal = panes.iter().any(|(id, _)| terminal_manager.is_terminal(id));

            if is_terminal {
                for (id, _) in &panes {
                    if let Some(pane) = terminal_manager.get(id) {
                        let colors = TerminalColors::default();
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
            } else if let Some((_, tex_id)) = panes
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
    });
}

pub fn build_tab_list(
    ui: &mut egui::Ui,
    app_state: &mut AppState,
    wry_panes: &WryPaneManager,
    horizontal: bool,
    cached: &crate::config::CachedThemeColors,
) {
    let panes = app_state.wm.panes();
    let active_id = app_state.wm.active_pane_id();
    let tab_bar_bg = cached.tab_bar_bg;
    let border_color = cached.border;

    if horizontal {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
            for (pane_id, _rect) in &panes {
                let is_active = *pane_id == active_id;
                let is_terminal = app_state.terminal_pane_ids.contains(pane_id);

                let (title, tab_url) = wry_panes
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

                // Use custom tab name if set
                let display_title = {
                    let custom = app_state.tab_names.get(&pane_id.to_string()).cloned();
                    match custom {
                        Some(name) => truncate_str(&name, 21),
                        None => truncate_str(&title, 21),
                    }
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
                        let private_prefix = if app_state.private_pane_ids.contains(pane_id) {
                            "\u{1f512} "
                        } else {
                            ""
                        };

                        let is_pinned = app_state.pinned_pane_ids.contains(pane_id);
                        let is_muted = app_state.muted_pane_ids.contains(pane_id);
                        let is_private = app_state.private_pane_ids.contains(pane_id);
                        let a11y_title = title.clone();
                        let a11y_url = tab_url.clone();

                        let response = ui.selectable_label(
                            is_active,
                            format!(
                                "{}{}{}{}",
                                pinned_prefix, muted_prefix, private_prefix, display_title
                            ),
                        );
                        response.widget_info(|| {
                            let mut label = format!("Tab: {} - {}", a11y_title, a11y_url);
                            if is_pinned {
                                label.push_str(" (Pinned)");
                            }
                            if is_muted {
                                label.push_str(" (Muted)");
                            }
                            if is_private {
                                label.push_str(" (Private)");
                            }
                            a11y_info(WidgetType::SelectableLabel, label)
                        });
                        if response.clicked() && !is_active {
                            app_state.wm.set_active_pane(*pane_id);
                            app_state.update_status();
                        }

                        let close_title = display_title.clone();
                        let close_btn = ui.small_button("\u{00d7}");
                        close_btn.widget_info(|| {
                            a11y_info(WidgetType::Button, format!("Close tab: {}", close_title))
                        });
                        if close_btn.clicked() {
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

                // Use custom tab name if set
                let display_title = {
                    let custom = app_state.tab_names.get(&pane_id.to_string()).cloned();
                    match custom {
                        Some(name) => truncate_str(&name, 17),
                        None => truncate_str(&title, 17),
                    }
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
                        let private_prefix = if app_state.private_pane_ids.contains(pane_id) {
                            "\u{1f512} "
                        } else {
                            ""
                        };

                        let is_pinned = app_state.pinned_pane_ids.contains(pane_id);
                        let is_muted = app_state.muted_pane_ids.contains(pane_id);
                        let is_private = app_state.private_pane_ids.contains(pane_id);
                        let a11y_title = title.clone();
                        let a11y_url = url.clone();

                        let response = ui.selectable_label(
                            is_active,
                            format!(
                                "{}{}{}{}",
                                pinned_prefix, muted_prefix, private_prefix, display_title
                            ),
                        );
                        response.widget_info(|| {
                            let mut label = format!("Tab: {} - {}", a11y_title, a11y_url);
                            if is_pinned {
                                label.push_str(" (Pinned)");
                            }
                            if is_muted {
                                label.push_str(" (Muted)");
                            }
                            if is_private {
                                label.push_str(" (Private)");
                            }
                            a11y_info(WidgetType::SelectableLabel, label)
                        });
                        if response.clicked() && !is_active {
                            app_state.wm.set_active_pane(*pane_id);
                            app_state.update_status();
                        }

                        let close_title = display_title.clone();
                        let close_btn = ui.small_button("\u{00d7}");
                        close_btn.widget_info(|| {
                            a11y_info(WidgetType::Button, format!("Close tab: {}", close_title))
                        });
                        if close_btn.clicked() {
                            app_state.pending_tab_close = Some(*pane_id);
                        }
                    });

                    if !is_terminal {
                        let display_url = truncate_str(&url, 19);
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
