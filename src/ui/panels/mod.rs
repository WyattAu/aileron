use crate::app::{AppState, WryAction};
use crate::git::GitStatus;
use crate::input::Mode;
use crate::servo::WryPaneManager;
#[cfg(feature = "terminal")]
use crate::terminal::NativeTerminalManager;
use crate::ui::search::SearchCategory;
use egui::{WidgetInfo, WidgetType};
use tracing::warn;

pub(crate) fn a11y_info(typ: WidgetType, label: impl Into<String>) -> WidgetInfo {
    WidgetInfo {
        typ,
        label: Some(label.into()),
        ..WidgetInfo::new(typ)
    }
}

/// Truncate a string to at most `max_chars` characters without splitting multi-byte UTF-8.
/// Returns `Cow::Borrowed` when no truncation is needed (avoids allocation on the common path).
pub(crate) fn truncate_str<'a>(s: &'a str, max_chars: usize) -> std::borrow::Cow<'a, str> {
    if s.chars().count() <= max_chars {
        std::borrow::Cow::Borrowed(s)
    } else {
        let truncated: String = s.chars().take(max_chars).collect();
        std::borrow::Cow::Owned(format!("{truncated}..."))
    }
}

/// Format a byte count as a human-readable size string.
fn format_size(bytes: u64) -> String {
    if bytes == 0 {
        return "0 B".into();
    }
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
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
    #[cfg(feature = "terminal")] terminal_manager: &mut NativeTerminalManager,
    offscreen_panes: &crate::offscreen_webview::OffscreenWebViewManager,
) {
    build_ui_inner(
        ctx,
        app_state,
        wry_panes,
        git_status,
        status_bar_height,
        webview_textures,
        offscreen_panes,
        #[cfg(feature = "terminal")]
        terminal_manager,
    )
}

#[allow(clippy::too_many_arguments)]
fn build_ui_inner(
    ctx: &egui::Context,
    app_state: &mut AppState,
    wry_panes: &WryPaneManager,
    git_status: &GitStatus,
    status_bar_height: f64,
    webview_textures: &std::collections::HashMap<uuid::Uuid, egui::TextureId>,
    offscreen_panes: &crate::offscreen_webview::OffscreenWebViewManager,
    #[cfg(feature = "terminal")] terminal_manager: &mut NativeTerminalManager,
) {
    let tab_layout = app_state.config.tab_layout.as_str();
    // Extract only the Copy Color32 fields we need from CachedThemeColors,
    // avoiding a clone of the entire struct (which contains Strings) every frame.
    let tc = app_state.config.cached_theme_colors();
    let tab_bg = tc.tab_bar_bg;
    let tab_fg = tc.tab_bar_fg;
    let _status_bg = tc.status_bar_bg;
    let _status_fg = tc.status_bar_fg;
    let _url_bg = tc.url_bar_bg;
    let _url_fg = tc.url_bar_fg;
    let accent = tc.accent;
    let bg = tc.bg;
    let border_color_default = tc.border;
    drop(tc);

    // CRITICAL: In modes without a TextEdit widget, release egui keyboard focus.
    // Otherwise egui retains focus on the last TextEdit and consumes ALL
    // character key events, preventing our keybind system from receiving them.
    let needs_text_edit_focus = app_state.palette.open
        || app_state.ui.url_bar_focused
        || app_state.mode == crate::input::Mode::Insert
        || app_state.ui.find_bar_open
        || app_state.panels.tab_search_open;
    if !needs_text_edit_focus {
        ctx.memory_mut(|mem| {
            if let Some(focused_id) = mem.focused() {
                mem.surrender_focus(focused_id);
            }
        });
    }

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
                build_tab_list(
                    ui,
                    app_state,
                    wry_panes,
                    false,
                    tab_bg,
                    border_color_default,
                );
            });
    } else if tab_layout == "topbar" {
        egui::TopBottomPanel::top("tab-bar").show(ctx, |ui| {
            build_tab_list(ui, app_state, wry_panes, true, tab_bg, border_color_default);
        });
    }

    egui::TopBottomPanel::top("status-bar").show(ctx, |ui| {
        egui::menu::bar(ui, |ui| {
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                // Accessibility live region: hidden label that screen readers announce
                // on important state changes (mode, navigation, error, pane close).
                // IMPORTANT: Do NOT call request_focus() here — that would steal
                // keyboard focus from egui's keybind system every frame, blocking
                // all keyboard input. The WidgetInfo metadata is sufficient for
                // screen readers; a transparent label never needs keyboard focus.
                if !app_state.ui.accessibility_text.is_empty() {
                    let a11y_text = app_state.ui.accessibility_text.clone();
                    ui.allocate_ui_with_layout(
                        egui::vec2(0.0, 0.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            let resp = ui.label(
                                egui::RichText::new(&a11y_text).color(egui::Color32::TRANSPARENT),
                            );
                            resp.widget_info(|| WidgetInfo {
                                typ: WidgetType::Label,
                                label: Some(a11y_text.clone()),
                                ..WidgetInfo::new(WidgetType::Label)
                            });
                        },
                    );
                }
                let mode_color = match app_state.mode {
                    Mode::Normal => egui::Color32::from_rgb(100, 200, 100),
                    Mode::Insert => accent,
                    Mode::Command => egui::Color32::from_rgb(255, 200, 100),
                };
                let mut mode_str = app_state.mode.as_str().to_string();

                // Show sub-mode indicators
                if app_state.ui.hint_mode {
                    mode_str = format!("{} HINT[{}]", mode_str, &app_state.ui.hint_buffer);
                } else if app_state.ui.hint_new_tab {
                    mode_str = format!("{} HINT-TAB[{}]", mode_str, &app_state.ui.hint_buffer);
                } else if app_state.ui.find_bar_open {
                    mode_str = format!("{mode_str} FIND");
                } else if app_state.ui.url_bar_focused {
                    mode_str = format!("{mode_str} URL");
                } else if app_state.panels.tab_search_open {
                    mode_str = format!("{mode_str} TABS");
                } else if app_state.panels.history_panel_open {
                    mode_str = format!("{mode_str} HIST");
                } else if app_state.panels.bookmarks_panel_open {
                    mode_str = format!("{mode_str} BM");
                } else if app_state.panels.help_panel_open {
                    mode_str = format!("{mode_str} HELP");
                } else if app_state.panels.workspace_panel_open {
                    mode_str = format!("{mode_str} WS");
                } else if app_state.panels.sync_status_panel_open {
                    mode_str = format!("{mode_str} SYNC");
                } else if app_state.panels.sync_conflicts_panel_open {
                    mode_str = format!("{mode_str} CONFLICTS");
                }

                ui.colored_label(mode_color, &mode_str).widget_info(|| {
                    a11y_info(WidgetType::Label, format!("Current mode: {mode_str}"))
                });

                ui.separator();

                let current_count = app_state.wm.leaf_count();
                let pane_count = if app_state.cache.pane_count_dirty
                    || current_count != app_state.cache.cached_pane_count
                {
                    app_state.cache.cached_pane_count = current_count;
                    app_state.cache.pane_count_dirty = false;
                    current_count
                } else {
                    app_state.cache.cached_pane_count
                };
                ui.label(format!("panes: {pane_count}"))
                    .widget_info(|| a11y_info(WidgetType::Label, format!("Panes: {pane_count}")));

                // Private mode indicator
                if app_state
                    .tabs
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
                        format!("[{ws_name}]"),
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
                        format!("[AB: {blocked}]"),
                    )
                    .widget_info(|| {
                        a11y_info(WidgetType::Label, format!("Blocked ads: {blocked}"))
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
                    ui.colored_label(engine_color, &engine_text)
                        .widget_info(|| {
                            a11y_info(WidgetType::Label, format!("Engine: {}", &engine_text))
                        });
                }

                let git_text = git_status.status_bar_text();
                if !git_text.is_empty() {
                    ui.separator();
                    let git_color = if git_status.is_dirty {
                        egui::Color32::from_rgb(255, 200, 100)
                    } else {
                        tab_fg
                    };
                    ui.colored_label(git_color, &git_text).widget_info(|| {
                        a11y_info(WidgetType::Label, format!("Git: {}", &git_text))
                    });
                }

                // Sync status indicator (when sync feature is enabled and target is set)
                #[cfg(feature = "sync")]
                if !app_state.config.sync_target.is_empty() {
                    ui.separator();
                    let watcher_running = app_state.sync_watcher.is_running();
                    let (sync_label, sync_color) = if watcher_running {
                        ("[SYNC:ON]", egui::Color32::from_rgb(100, 200, 100))
                    } else {
                        ("[SYNC]", egui::Color32::from_rgb(180, 180, 100))
                    };
                    ui.colored_label(sync_color, sync_label).widget_info(|| {
                        a11y_info(
                            WidgetType::Label,
                            format!("Sync: target {}", app_state.config.sync_target),
                        )
                    });
                }

                ui.separator();

                let active_id = app_state.wm.active_pane_id();
                if let Some(wry_pane) = wry_panes.get(&active_id) {
                    let url_str = wry_pane.url().as_str();
                    let display_url = truncate_str(url_str, 57);
                    let a11y_label = format!("Current URL: {url_str}");
                    let url_resp = ui.label(display_url.as_ref());
                    url_resp.widget_info(move || a11y_info(WidgetType::Label, &a11y_label));
                    if url_resp.clicked() {
                        app_state.ui.url_bar_focused = true;
                        app_state.ui.url_bar_input = url_str.to_string();
                    }
                } else if let Some(pane) = offscreen_panes.get(&active_id) {
                    let url_str = pane.url().as_str();
                    let display_url = truncate_str(url_str, 57);
                    let a11y_label = format!("Current URL: {url_str}");
                    let url_resp = ui.label(display_url.as_ref());
                    url_resp.widget_info(move || a11y_info(WidgetType::Label, &a11y_label));
                    if url_resp.clicked() {
                        app_state.ui.url_bar_focused = true;
                        app_state.ui.url_bar_input = url_str.to_string();
                    }
                }

                ui.separator();

                // Show zoom level if non-default
                if let Some(zoom) = app_state.panels.site_settings_zoom
                    && (zoom - 1.0).abs() > 0.01
                {
                    let pct = (zoom * 100.0).round() as u32;
                    let zoom_text = format!("{pct}%");
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
                            a11y_info(WidgetType::Label, format!("Download: {dl_text}"))
                        });
                    }
                    ui.separator();
                }

                if app_state.autofill.available {
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
                        && let Some(js) = app_state.autofill.js.take()
                    {
                        app_state
                            .pending_wry_actions
                            .push_back(WryAction::RunJs(js));
                        app_state.ui.status_message = app_state.autofill.status_msg.clone();
                        app_state.autofill.available = false;
                    }
                }

                if app_state.ui.hint_mode {
                    let hint_text = format!("hint: {}", app_state.ui.hint_buffer);
                    ui.colored_label(accent, &hint_text)
                        .widget_info(|| a11y_info(WidgetType::Label, &hint_text));
                } else if !app_state.ui.status_message.is_empty() {
                    let msg = app_state.ui.status_message.clone();
                    ui.label(&msg)
                        .widget_info(|| a11y_info(WidgetType::Label, format!("Status: {msg}")));
                }

                // Version + build hash (far right)
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let ver = format!(
                        "v{} ({})",
                        env!("CARGO_PKG_VERSION"),
                        env!("AILERON_GIT_HASH"),
                    );
                    ui.colored_label(egui::Color32::from_rgb(120, 120, 120), &ver)
                        .widget_info(|| a11y_info(WidgetType::Label, format!("Version: {ver}")));
                });
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

                // Sync: egui's TextEdit receives characters from IME commits
                // directly (fed at line 1056 in main.rs). Push its content into
                // command_palette_input (the keybind-handler string) so both
                // paths stay in sync, and update search results.
                let query_snapshot = app_state.palette.query.clone();
                app_state.ui.command_palette_input = query_snapshot;
                app_state
                    .palette
                    .update_query(&app_state.ui.command_palette_input);
            } else if app_state.ui.url_bar_focused {
                ui.colored_label(accent, "URL>").widget_info(|| {
                    a11y_info(WidgetType::Label, "URL bar mode indicator: editing")
                });
                let response = ui.add(
                    egui::TextEdit::singleline(&mut app_state.ui.url_bar_input)
                        .desired_width(f32::INFINITY)
                        .hint_text("Search or enter URL..."),
                );
                response.widget_info(|| a11y_info(WidgetType::TextEdit, "URL bar"));
                response.request_focus();

                let query_snapshot = app_state.ui.url_bar_input.clone();
                if query_snapshot != app_state.ui.last_omnibox_query {
                    app_state.update_omnibox(&query_snapshot);
                }

                if !app_state.ui.omnibox_results.is_empty() {
                    let popup_id = egui::Id::new("omnibox_popup");
                    let popup_height =
                        (app_state.ui.omnibox_results.len() as f32 * 24.0).min(200.0);

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
                                for (i, item) in app_state.ui.omnibox_results.iter().enumerate() {
                                    let selected = i == app_state.ui.omnibox_selected;
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
                                    app_state.ui.url_bar_focused = false;
                                    app_state.ui.omnibox_results.clear();
                                    app_state.ui.last_omnibox_query.clear();
                                }
                            });
                        });
                }

                // Help panel overlay
                if app_state.panels.help_panel_open {
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
                                (":ws-delete <name>", "Delete workspace"),
                                (":ws-panel", "Workspace panel"),
                                (":ws-next / :ws-prev", "Cycle workspaces"),
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
                    && app_state.ui.omnibox_selected
                        < app_state.ui.omnibox_results.len().saturating_sub(1)
                {
                    app_state.ui.omnibox_selected += 1;
                }
                if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                    app_state.ui.omnibox_selected = app_state.ui.omnibox_selected.saturating_sub(1);
                }

                if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    if !app_state.ui.omnibox_results.is_empty() {
                        app_state.handle_omnibox_select(app_state.ui.omnibox_selected);
                    } else {
                        let input = app_state.ui.url_bar_input.trim().to_string();
                        if !input.is_empty() {
                            // Check for go/<name> quickmark navigation
                            if let Some(name) = input.strip_prefix("go/") {
                                let name = name.trim();
                                if !name.is_empty() {
                                    if let Some(url) = app_state.quickmarks_get(name) {
                                        app_state
                                            .pending_wry_actions
                                            .push_back(WryAction::Navigate(url));
                                        app_state.ui.status_message = format!("Quickmark: {name}");
                                    } else {
                                        app_state.ui.status_message =
                                            format!("Quickmark '{name}' not found");
                                    }
                                }
                            } else {
                                let url =
                                    if input.starts_with("aileron://") || input.contains("://") {
                                        url::Url::parse(&input).ok()
                                    } else {
                                        app_state.config.search_url(&input)
                                    };
                                if let Some(url) = url {
                                    app_state
                                        .pending_wry_actions
                                        .push_back(WryAction::Navigate(url));
                                    app_state.ui.status_message = format!("Navigating to {input}");
                                }
                            }
                        }
                    }
                    app_state.ui.url_bar_focused = false;
                    app_state.ui.omnibox_results.clear();
                    app_state.ui.last_omnibox_query.clear();
                }

                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    app_state.ui.url_bar_focused = false;
                    app_state.ui.url_bar_input.clear();
                    app_state.ui.omnibox_results.clear();
                    app_state.ui.last_omnibox_query.clear();
                }
            } else {
                let (mode_label, mode_color) = match app_state.mode {
                    Mode::Normal => ("NORMAL", egui::Color32::from_rgb(100, 200, 100)),
                    Mode::Insert => ("INSERT", accent),
                    Mode::Command => ("COMMAND", egui::Color32::from_rgb(200, 200, 100)),
                };
                let ml = mode_label;
                ui.colored_label(mode_color, mode_label)
                    .widget_info(|| a11y_info(WidgetType::Label, format!("URL bar mode: {ml}")));
                ui.separator();

                let active_id = app_state.wm.active_pane_id();
                let url_str = if let Some(wry_pane) = wry_panes.get(&active_id) {
                    wry_pane.url().as_str()
                } else {
                    "aileron://welcome"
                };

                let url_label = ui.strong(url_str);
                url_label.widget_info(|| a11y_info(WidgetType::Label, format!("URL: {url_str}")));

                if url_label.clicked() {
                    app_state.ui.url_bar_focused = true;
                    app_state.ui.url_bar_input = url_str.to_string();
                }
            }
        });
    });

    if app_state.ui.find_bar_open {
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
                            egui::TextEdit::singleline(&mut app_state.ui.find_query)
                                .desired_width(300.0)
                                .hint_text("Search in page..."),
                        );
                        response.widget_info(|| a11y_info(WidgetType::TextEdit, "Find in page"));
                        response.request_focus();

                        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            let active_id = app_state.wm.active_pane_id();
                            if let Some(wry_pane) = wry_panes.get(&active_id) {
                                let q = app_state.ui.find_query.replace('\'', "\\'");
                                // Store query in JS for FindNext/FindPrev reuse
                                wry_pane.execute_js(&format!("window._aileronFindQuery='{q}'"));
                                wry_pane.execute_js(&format!(
                                    "window.find('{q}', false, false, true, true, false)"
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
                            app_state.ui.find_bar_open = false;
                            app_state.ui.find_query.clear();
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
                                        app_state.ui.command_palette_input.clear();
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
    if app_state.panels.history_panel_open {
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
                            app_state.panels.history_panel_open = false;
                            app_state.panels.history_entries.clear();
                        }
                    });
                });
                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);

                egui::ScrollArea::vertical()
                    .max_height(430.0)
                    .show(ui, |ui| {
                        if app_state.panels.history_entries.is_empty() {
                            ui.label(
                                egui::RichText::new("No history entries")
                                    .color(egui::Color32::GRAY),
                            );
                        }
                        let mut navigate_to: Option<url::Url> = None;
                        for (i, entry) in app_state.panels.history_entries.iter().enumerate() {
                            let is_selected = i == app_state.panels.history_selected;
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
                                app_state.panels.history_selected = i;
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
                            app_state.panels.history_panel_open = false;
                            app_state.panels.history_entries.clear();
                        }
                    });
            });
    }

    // ─── Tab Search Panel ───
    if app_state.panels.tab_search_open {
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
                            app_state.panels.tab_search_open = false;
                        }
                    });
                });

                // Search filter
                let search_response = ui.add(
                    egui::TextEdit::singleline(&mut app_state.panels.tab_search_query)
                        .hint_text("Filter tabs...")
                        .desired_width(f32::INFINITY)
                        .text_color(text),
                );
                search_response.request_focus();

                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);

                // Tab list with fuzzy filter
                let query = app_state.panels.tab_search_query.to_lowercase();
                let pane_ids: Vec<_> = app_state.wm.iter_pane_ids().collect();
                let active_id = app_state.wm.active_pane_id();

                let mut switch_to: Option<uuid::Uuid> = None;
                let mut close_tab: Option<uuid::Uuid> = None;

                egui::ScrollArea::vertical()
                    .max_height(300.0)
                    .show(ui, |ui| {
                        let mut visible_index = 0usize;
                        for id in &pane_ids {
                            // Use tab_display_cache instead of direct wry_panes lookups
                            // to avoid per-frame String allocations.
                            let (title, url) = app_state
                                .tabs
                                .tab_display_cache
                                .get(id)
                                .map(|i| (i.title.as_str(), i.url.as_str()))
                                .unwrap_or(("New Tab", "aileron://new"));

                            // Simple substring filter (not fuzzy, but good enough)
                            if !query.is_empty() {
                                let matches = title.to_lowercase().contains(&query)
                                    || url.to_lowercase().contains(&query)
                                    || id.simple().to_string().starts_with(&query);
                                if !matches {
                                    continue;
                                }
                            }

                            let is_active = *id == active_id;
                            let is_selected = visible_index == app_state.panels.tab_search_selected;
                            let is_terminal = app_state.is_terminal_pane(id);
                            let prefix = if is_terminal { "[term] " } else { "" };
                            let marker = if is_active { " ●" } else { "" };

                            ui.horizontal(|ui| {
                                let label = format!("{prefix}{title}{marker}  {url}");
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
                                    app_state.panels.tab_search_selected = visible_index;
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
                        if visible_index > 0
                            && app_state.panels.tab_search_selected >= visible_index
                        {
                            app_state.panels.tab_search_selected = visible_index - 1;
                        }

                        if pane_ids.is_empty() {
                            ui.label(egui::RichText::new("No open tabs").color(dim));
                        }
                    });

                if let Some(id) = switch_to {
                    app_state.wm.set_active_pane(id);
                }
                if let Some(id) = close_tab {
                    if let Err(e) = app_state.wm.close(id) {
                        warn!(%e, "Failed to close pane");
                    }
                    app_state.session.session_dirty = true;
                }
            });
    }

    // ─── Bookmarks Panel ───
    if app_state.panels.bookmarks_panel_open {
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
                            app_state.ui.status_message = "Importing Chrome bookmarks...".into();
                        }
                        if ui.small_button("Import Firefox").clicked() {
                            app_state.pending_import = Some("firefox".into());
                            app_state.ui.status_message = "Importing Firefox bookmarks...".into();
                        }
                        ui.add_space(8.0);
                        if ui.button("✕").clicked() {
                            app_state.panels.bookmarks_panel_open = false;
                            app_state.panels.bookmarks_entries.clear();
                        }
                    });
                });
                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);

                egui::ScrollArea::vertical()
                    .max_height(380.0)
                    .show(ui, |ui| {
                        if app_state.panels.bookmarks_entries.is_empty() {
                            ui.label(
                                egui::RichText::new("No bookmarks").color(egui::Color32::GRAY),
                            );
                        }
                        let mut navigate_to: Option<url::Url> = None;
                        let mut delete_id: Option<i64> = None;

                        // Group bookmarks by folder for display
                        let mut last_folder = String::new();
                        for (i, bm) in app_state.panels.bookmarks_entries.iter().enumerate() {
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
                                    egui::RichText::new(format!("  {folder_label}"))
                                        .size(12.0)
                                        .strong(),
                                );
                                ui.add_space(2.0);
                            }

                            let is_selected = i == app_state.panels.bookmarks_selected;
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
                                    app_state.panels.bookmarks_selected = i;
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
                            app_state.panels.bookmarks_panel_open = false;
                            app_state.panels.bookmarks_entries.clear();
                        }
                        if let Some(id) = delete_id
                            && let Some(db) = app_state.db.as_ref()
                        {
                            if let Err(e) = crate::db::bookmarks::remove_bookmark_by_id(db, id) {
                                tracing::warn!("Failed to remove bookmark by id: {}", e);
                            }
                            app_state.panels.bookmarks_entries.retain(|b| b.id != id);
                            if app_state.panels.bookmarks_selected
                                >= app_state.panels.bookmarks_entries.len()
                            {
                                app_state.panels.bookmarks_selected =
                                    app_state.panels.bookmarks_entries.len().saturating_sub(1);
                            }
                        }
                    });
            });
    }

    // ─── Workspace Panel ───
    if app_state.panels.workspace_panel_open {
        let bg = egui::Color32::from_rgb(0x19, 0x19, 0x20);
        let accent = egui::Color32::from_rgb(0x4d, 0xb4, 0xff);
        let text = egui::Color32::from_rgb(0xd4, 0xd4, 0xd4);
        let dim = egui::Color32::from_rgb(0x88, 0x88, 0x88);

        egui::Window::new("workspaces")
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
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Workspaces")
                            .size(16.0)
                            .color(accent)
                            .strong(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Save Current").clicked() {
                            let name = if app_state.current_workspace_name != "default" {
                                app_state.current_workspace_name.clone()
                            } else {
                                format!("ws-{}", chrono::Local::now().format("%m%d-%H%M"))
                            };
                            app_state
                                .pending_wry_actions
                                .push_back(WryAction::SaveWorkspace {
                                    name: name.clone(),
                                    pane_urls: std::collections::HashMap::new(),
                                });
                            app_state.current_workspace_name = name.clone();
                            app_state.ui.status_message = format!("Saving workspace: {name}...");
                        }
                        if ui.button("Split Pane").clicked() {
                            let active = app_state.wm.active_pane_id();
                            if let Err(e) =
                                app_state
                                    .wm
                                    .split(active, crate::wm::SplitDirection::Vertical, 0.5)
                            {
                                warn!(%e, "Failed to split pane");
                            }
                            app_state.session.session_dirty = true;
                        }
                        ui.add_space(8.0);
                        if ui.button("X").clicked() {
                            app_state.panels.workspace_panel_open = false;
                            app_state.panels.workspace_entries.clear();
                        }
                    });
                });
                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);

                egui::ScrollArea::vertical()
                    .max_height(300.0)
                    .show(ui, |ui| {
                        if app_state.panels.workspace_entries.is_empty() {
                            ui.label(
                                egui::RichText::new("No saved workspaces")
                                    .color(egui::Color32::GRAY),
                            );
                        }
                        let mut switch_to: Option<String> = None;
                        let mut delete_name: Option<String> = None;

                        for (i, ws) in app_state.panels.workspace_entries.iter().enumerate() {
                            if ws.name == "_autosave" {
                                continue;
                            }
                            let is_current = ws.name == app_state.current_workspace_name;
                            let is_selected = i == app_state.panels.workspace_selected;

                            ui.horizontal(|ui| {
                                let marker = if is_current { " * " } else { "   " };
                                let response = ui.selectable_label(
                                    is_selected || is_current,
                                    egui::RichText::new(format!("{}{}", marker, ws.name))
                                        .size(13.0)
                                        .color(if is_selected || is_current {
                                            accent
                                        } else {
                                            text
                                        }),
                                );
                                if response.clicked() {
                                    switch_to = Some(ws.name.clone());
                                    app_state.panels.workspace_selected = i;
                                }
                                if is_selected {
                                    response.scroll_to_me(Some(egui::Align::Center));
                                }

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(
                                            egui::RichText::new(&ws.updated_at).small().color(dim),
                                        );
                                        if ui.small_button("X").clicked() {
                                            delete_name = Some(ws.name.clone());
                                        }
                                    },
                                );
                            });
                        }

                        if let Some(name) = switch_to {
                            app_state.pending_workspace_restore = Some(name.clone());
                            app_state.current_workspace_name = name.clone();
                            app_state.ui.status_message = format!("Restoring workspace: {name}...");
                            app_state.panels.workspace_panel_open = false;
                            app_state.panels.workspace_entries.clear();
                        }
                        if let Some(name) = delete_name
                            && let Some(db) = app_state.db.as_ref()
                            && let Ok(true) = crate::db::workspaces::delete_workspace(db, &name)
                        {
                            app_state
                                .panels
                                .workspace_entries
                                .retain(|w| w.name != name);
                            if name == app_state.current_workspace_name {
                                app_state.current_workspace_name = "default".into();
                            }
                            app_state.ui.status_message = format!("Workspace deleted: {name}");
                        }
                    });
            });
    }

    // ─── Sync Status Panel ───
    #[cfg(feature = "sync")]
    if app_state.panels.sync_status_panel_open {
        let bg = egui::Color32::from_rgb(0x19, 0x19, 0x20);
        let accent = egui::Color32::from_rgb(0x4d, 0xb4, 0xff);
        let text = egui::Color32::from_rgb(0xd4, 0xd4, 0xd4);
        let dim = egui::Color32::from_rgb(0x88, 0x88, 0x88);

        egui::Window::new("sync-status")
            .title_bar(false)
            .collapsible(false)
            .resizable(true)
            .default_width(480.0)
            .default_height(320.0)
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
                        egui::RichText::new("Sync Status")
                            .size(16.0)
                            .color(accent)
                            .strong(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("X").clicked() {
                            app_state.panels.sync_status_panel_open = false;
                        }
                    });
                });
                ui.add_space(4.0);
                ui.separator();
                ui.add_space(8.0);

                let config_dir = crate::config::Config::config_dir();
                let sm = crate::sync::SyncManager::new(config_dir);
                let file_count = match sm.compute_manifest() {
                    Ok(m) => m.files.len(),
                    Err(_) => 0,
                };
                let watcher_running = app_state.sync_watcher.is_running();
                let has_target = !app_state.config.sync_target.is_empty();

                // Status rows
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Target:").color(dim));
                    if has_target {
                        ui.label(egui::RichText::new(&app_state.config.sync_target).color(text));
                    } else {
                        ui.label(
                            egui::RichText::new("not configured")
                                .color(egui::Color32::from_rgb(255, 100, 100)),
                        );
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Encryption:").color(dim));
                    ui.label(
                        egui::RichText::new(if app_state.config.sync_encrypted {
                            "enabled"
                        } else {
                            "disabled"
                        })
                        .color(text),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Watcher:").color(dim));
                    let (watcher_text, watcher_color) = if watcher_running {
                        ("running", egui::Color32::from_rgb(100, 200, 100))
                    } else {
                        ("stopped", egui::Color32::from_rgb(200, 100, 100))
                    };
                    ui.colored_label(watcher_color, watcher_text);
                });
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Local files:").color(dim));
                    ui.label(egui::RichText::new(format!("{file_count}")).color(text));
                });

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    if ui.button("Sync Push").clicked() {
                        app_state.ui.status_message = crate::app::cmd::sync::execute_sync_push(
                            &app_state.config.sync_target,
                            app_state.config.sync_encrypted,
                        );
                    }
                    if ui.button("Sync Pull").clicked() {
                        app_state.ui.status_message = crate::app::cmd::sync::execute_sync_pull(
                            &app_state.config.sync_target,
                            app_state.config.sync_encrypted,
                        );
                    }
                    if watcher_running {
                        if ui.button("Stop Watcher").clicked() {
                            app_state.sync_watcher.stop();
                            app_state.ui.status_message = "Sync watcher stopped".into();
                        }
                    } else if ui.button("Start Watcher").clicked()
                        && crate::app::cmd::sync::execute_sync_watch(&app_state.config.sync_target)
                            .is_ok()
                    {
                        let config_dir = crate::config::Config::config_dir();
                        match app_state.sync_watcher.start(&config_dir) {
                            Ok(()) => app_state.ui.status_message = "Sync watcher started".into(),
                            Err(e) => {
                                app_state.ui.status_message =
                                    format!("Failed to start watcher: {e}");
                            }
                        }
                    }
                });
            });
    }

    // ─── Sync Conflicts Panel ───
    #[cfg(feature = "sync")]
    if app_state.panels.sync_conflicts_panel_open {
        let bg = egui::Color32::from_rgb(0x19, 0x19, 0x20);
        let accent = egui::Color32::from_rgb(0x4d, 0xb4, 0xff);
        let dim = egui::Color32::from_rgb(0x88, 0x88, 0x88);
        let warn_color = egui::Color32::from_rgb(255, 200, 100);

        let conflict_count = app_state.panels.sync_conflict_entries.len();
        egui::Window::new("sync-conflicts")
            .title_bar(false)
            .collapsible(false)
            .resizable(true)
            .default_width(560.0)
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
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!("Sync Conflicts ({conflict_count})"))
                            .size(16.0)
                            .color(accent)
                            .strong(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("X").clicked() {
                            app_state.panels.sync_conflicts_panel_open = false;
                            app_state.panels.sync_conflict_entries.clear();
                        }
                    });
                });
                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);

                egui::ScrollArea::vertical()
                    .max_height(300.0)
                    .show(ui, |ui| {
                        if app_state.panels.sync_conflict_entries.is_empty() {
                            ui.label(
                                egui::RichText::new(
                                    "No conflicts detected. All files are in sync.",
                                )
                                .color(egui::Color32::GRAY),
                            );
                        }

                        let mut resolve_keep_local: Option<usize> = None;
                        let mut resolve_keep_remote: Option<usize> = None;

                        for (i, conflict) in
                            app_state.panels.sync_conflict_entries.iter().enumerate()
                        {
                            let is_selected = i == app_state.panels.sync_conflict_selected;
                            let deleted = conflict.local_hash.is_empty();

                            ui.horizontal(|ui| {
                                let response = ui.selectable_label(
                                    is_selected,
                                    egui::RichText::new(if deleted {
                                        format!("  [DELETED] {}", conflict.path)
                                    } else {
                                        format!("  {}", conflict.path)
                                    })
                                    .size(13.0)
                                    .color(if deleted {
                                        egui::Color32::from_rgb(255, 100, 100)
                                    } else if is_selected {
                                        accent
                                    } else {
                                        warn_color
                                    }),
                                );
                                if response.clicked() {
                                    app_state.panels.sync_conflict_selected = i;
                                }
                                if is_selected {
                                    response.scroll_to_me(Some(egui::Align::Center));
                                }

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if !deleted && ui.small_button("Keep Remote").clicked() {
                                            resolve_keep_remote = Some(i);
                                        }
                                        if !deleted && ui.small_button("Keep Local").clicked() {
                                            resolve_keep_local = Some(i);
                                        }
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "{} -> {}",
                                                format_size(conflict.remote_size),
                                                format_size(conflict.local_size),
                                            ))
                                            .small()
                                            .color(dim),
                                        );
                                    },
                                );
                            });
                        }

                        // Handle conflict resolution
                        if let Some(idx) = resolve_keep_local {
                            app_state.panels.sync_conflict_entries.remove(idx);
                            app_state.ui.status_message = "Kept local version".into();
                            if app_state.panels.sync_conflict_selected
                                >= app_state.panels.sync_conflict_entries.len()
                            {
                                app_state.panels.sync_conflict_selected = app_state
                                    .panels
                                    .sync_conflict_entries
                                    .len()
                                    .saturating_sub(1);
                            }
                        }
                        if let Some(idx) = resolve_keep_remote {
                            // For keep-remote: revert local file to the synced version
                            if let Some(conflict) = app_state.panels.sync_conflict_entries.get(idx)
                            {
                                app_state.ui.status_message = format!(
                                    "Keep remote selected for: {} (re-run :sync --pull to restore)",
                                    conflict.path
                                );
                            }
                            app_state.panels.sync_conflict_entries.remove(idx);
                            if app_state.panels.sync_conflict_selected
                                >= app_state.panels.sync_conflict_entries.len()
                            {
                                app_state.panels.sync_conflict_selected = app_state
                                    .panels
                                    .sync_conflict_entries
                                    .len()
                                    .saturating_sub(1);
                            }
                        }
                    });
            });
    }

    // Per-site settings panel
    if app_state.panels.site_settings_panel_open {
        egui::Window::new("site-settings")
            .title_bar(true)
            .resizable(true)
            .default_width(320.0)
            .default_height(350.0)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("URL Pattern:");
                    let mut pat = app_state.panels.site_settings_url_pattern.clone();
                    ui.text_edit_singleline(&mut pat);
                    app_state.panels.site_settings_url_pattern = pat;
                });
                ui.add_space(4.0);

                // Zoom level
                ui.horizontal(|ui| {
                    ui.label("Zoom:");
                    let mut zoom = app_state.panels.site_settings_zoom.unwrap_or(100.0);
                    if ui
                        .add(egui::Slider::new(&mut zoom, 25.0..=300.0).suffix("%"))
                        .changed()
                    {
                        app_state.panels.site_settings_zoom = Some(zoom);
                    }
                    if ui.small_button("reset").clicked() {
                        app_state.panels.site_settings_zoom = None;
                    }
                });

                // Toggles
                ui.add_space(4.0);
                egui::Grid::new("site_settings_grid")
                    .num_columns(2)
                    .spacing([8.0, 4.0])
                    .show(ui, |ui| {
                        ui.label("JavaScript:");
                        let mut js = app_state.panels.site_settings_js.unwrap_or(true);
                        if ui.checkbox(&mut js, "").changed() {
                            app_state.panels.site_settings_js = Some(js);
                        }
                        ui.end_row();

                        ui.label("Cookies:");
                        let mut cookies = app_state.panels.site_settings_cookies.unwrap_or(true);
                        if ui.checkbox(&mut cookies, "").changed() {
                            app_state.panels.site_settings_cookies = Some(cookies);
                        }
                        ui.end_row();

                        ui.label("AdBlock:");
                        let mut adblock = app_state.panels.site_settings_adblock.unwrap_or(true);
                        if ui.checkbox(&mut adblock, "").changed() {
                            app_state.panels.site_settings_adblock = Some(adblock);
                        }
                        ui.end_row();
                    });

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        let pattern = if app_state.panels.site_settings_url_pattern.is_empty() {
                            // Use current active pane URL as wildcard pattern
                            let active_id = app_state.wm.active_pane_id();
                            wry_panes
                                .url_for(&active_id)
                                .map(|u| {
                                    if let Some(host) = u.host_str() {
                                        format!("*://{host}*")
                                    } else {
                                        u.to_string()
                                    }
                                })
                                .unwrap_or_else(|| "*".into())
                        } else {
                            app_state.panels.site_settings_url_pattern.clone()
                        };
                        if let Some(db) = app_state.db.as_ref() {
                            macro_rules! save_field {
                                ($field:expr, $val:expr) => {
                                    if let Err(e) = crate::db::site_settings::set_site_field(
                                        db, &pattern, "wildcard", $field, $val,
                                    ) {
                                        tracing::warn!(
                                            "Failed to save site setting '{}': {}",
                                            $field,
                                            e
                                        );
                                    }
                                };
                            }
                            save_field!(
                                "zoom",
                                app_state
                                    .panels
                                    .site_settings_zoom
                                    .map(|z| z.to_string())
                                    .as_deref()
                            );
                            save_field!(
                                "adblock",
                                app_state.panels.site_settings_adblock.map(|v| if v {
                                    "1"
                                } else {
                                    "0"
                                })
                            );
                            save_field!(
                                "javascript",
                                app_state.panels.site_settings_js.map(|v| if v {
                                    "1"
                                } else {
                                    "0"
                                })
                            );
                            save_field!(
                                "cookies",
                                app_state.panels.site_settings_cookies.map(|v| if v {
                                    "1"
                                } else {
                                    "0"
                                })
                            );
                            app_state.ui.status_message =
                                format!("Saved site settings for: {pattern}");
                        }
                    }
                    if ui.button("Close").clicked() {
                        app_state.panels.site_settings_panel_open = false;
                    }
                });
            });
    }

    // ── Permission prompt dialog ──
    // Take ownership of the pending request to avoid borrow conflicts.
    let pending_perm = app_state.panels.pending_permission_request.take();
    if app_state.panels.permission_prompt_open {
        if let Some(req) = pending_perm {
            egui::Window::new("permission-prompt")
                .title_bar(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .frame(
                    egui::Frame::new()
                        .fill(bg)
                        .stroke(egui::Stroke::new(1.0, border_color_default))
                        .corner_radius(6)
                        .inner_margin(egui::Margin::same(16)),
                )
                .show(ctx, |ui| {
                    // Header
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Permission Request")
                                .size(14.0)
                                .strong(),
                        );
                    });
                    ui.add_space(8.0);

                    // Extension name
                    ui.label(format!(
                        "\"{}\" requests additional permissions:",
                        req.extension_name
                    ));
                    ui.add_space(6.0);

                    // Permission list
                    for perm in &req.permissions {
                        ui.horizontal(|ui| {
                            ui.add_space(12.0);
                            ui.label(egui::RichText::new(format!("- {}", perm)).size(12.0));
                        });
                    }
                    ui.add_space(12.0);

                    // Action buttons
                    ui.horizontal(|ui| {
                        let allow_btn = ui.button(egui::RichText::new("Allow").strong());
                        let deny_btn = ui.button("Deny");

                        if allow_btn.clicked() || deny_btn.clicked() {
                            let granted = allow_btn.clicked();
                            let ext_id_str = req.extension_id.clone();
                            let perms = req.permissions.clone();
                            let request_id = req.request_id;

                            app_state.panels.permission_prompt_open = false;

                            if granted {
                                let ext_id =
                                    crate::extensions::types::ExtensionId(ext_id_str.clone());
                                let mut em = app_state.extension_manager.write();
                                for perm in &perms {
                                    if let Err(e) = em.grant_optional_permission(&ext_id, perm) {
                                        tracing::warn!(
                                            target: "extensions",
                                            "Failed to grant permission '{}': {}",
                                            perm,
                                            e
                                        );
                                    }
                                }
                                tracing::info!(
                                    target: "extensions",
                                    "User granted permissions {:?} to extension '{}'",
                                    perms,
                                    ext_id_str
                                );
                            }

                            // Resolve the JS Promise in the active pane
                            let js = format!(
                                "window.__aileron_resolve_permission_request({}, {})",
                                request_id, granted
                            );
                            app_state
                                .pending_wry_actions
                                .push_back(WryAction::RunJs(js));
                        } else {
                            // Not clicked yet — put the request back
                            app_state.panels.pending_permission_request = Some(req);
                        }
                    });
                });
        } else {
            // Prompt open but no request data — close it
            app_state.panels.permission_prompt_open = false;
        }
    }

    central_panel::render_central_panel(
        ctx,
        app_state,
        wry_panes,
        webview_textures,
        #[cfg(feature = "terminal")]
        terminal_manager,
        accent,
        bg,
        border_color_default,
    );
}

mod central_panel;
mod tab_list;
pub use tab_list::build_tab_list;

#[cfg(test)]
mod tests;
