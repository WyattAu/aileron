use crate::app::AppState;
use crate::app::WryAction;
use crate::servo::WryPaneManager;
use egui::WidgetType;

use super::{a11y_info, truncate_str};

pub fn build_tab_list(
    ui: &mut egui::Ui,
    app_state: &mut AppState,
    wry_panes: &WryPaneManager,
    horizontal: bool,
    tab_bar_bg: egui::Color32,
    border_color: egui::Color32,
) {
    let panes: Vec<_> = app_state.wm.iter_panes().collect();
    let active_id = app_state.wm.active_pane_id();

    // Populate tab display cache when dirty
    if app_state.tabs.tab_display_dirty {
        app_state.tabs.tab_display_dirty = false;
        let mut fresh = std::collections::HashMap::new();
        for (pane_id, _) in &panes {
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
            fresh.insert(
                *pane_id,
                crate::app::TabDisplayInfo {
                    truncated_title_horizontal: truncate_str(&title, 21).into_owned(),
                    truncated_title_sidebar: truncate_str(&title, 17).into_owned(),
                    truncated_url: truncate_str(&url, 19).into_owned(),
                    title,
                    url,
                },
            );
        }
        app_state.tabs.tab_display_cache = fresh;
    }

    if horizontal {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
            for (pane_id, _rect) in &panes {
                let is_active = *pane_id == active_id;
                let is_terminal = app_state.is_terminal_pane(pane_id);

                // Extract needed data from cache before any mutable borrows of app_state.
                // The egui closures below also capture &mut app_state, so all shared
                // borrows of app_state must end before those closures run.
                let trunc_h: std::borrow::Cow<'_, str>;
                let a11y_base: String;
                {
                    let entry = app_state.tabs.tab_display_cache.get(pane_id);
                    trunc_h = entry
                        .map(|i| std::borrow::Cow::Borrowed(i.truncated_title_horizontal.as_str()))
                        .unwrap_or_else(|| std::borrow::Cow::Borrowed(""));
                    let info_title = entry.map(|i| i.title.as_str()).unwrap_or("");
                    let info_url = entry.map(|i| i.url.as_str()).unwrap_or("");
                    a11y_base = if info_title.is_empty() {
                        String::from("Tab: New Tab")
                    } else {
                        format!("Tab: {info_title} - {info_url}")
                    };
                }

                // Use custom tab name if set
                // Force into owned String so the borrow of app_state.tabs is released
                // before the egui closures below that also use &mut app_state.
                let display_title: String = {
                    let custom = app_state.tabs.tab_names.get(pane_id).map(|s| s.as_str());
                    match custom {
                        Some(name) => truncate_str(name, 21).into_owned(),
                        None => {
                            if trunc_h.is_empty() {
                                String::from("New Tab")
                            } else {
                                trunc_h.into_owned()
                            }
                        }
                    }
                };

                let frame_color = if is_active {
                    egui::Color32::from_rgb(40, 60, 90)
                } else {
                    tab_bar_bg
                };

                let close_label = format!("Close tab: {display_title}");
                let is_pinned = app_state.tabs.pinned_pane_ids.contains(pane_id);
                let is_muted = app_state.tabs.muted_pane_ids.contains(pane_id);
                let is_private = app_state.tabs.private_pane_ids.contains(pane_id);
                let muted_prefix = if is_muted { "\u{1f507} " } else { "" };
                let pinned_prefix = if is_pinned { "\u{1f4cc} " } else { "" };
                let private_prefix = if is_private { "\u{1f512} " } else { "" };

                // Pre-format the full a11y label so the Fn closure only captures a &String.
                let mut a11y_label = a11y_base;
                if is_pinned {
                    a11y_label.push_str(" (Pinned)");
                }
                if is_muted {
                    a11y_label.push_str(" (Muted)");
                }
                if is_private {
                    a11y_label.push_str(" (Private)");
                }

                egui::Frame::new().fill(frame_color).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.add_space(8.0);

                        let icon = if is_terminal { "\u{2328} " } else { "  " };
                        ui.label(icon);

                        let response = ui.selectable_label(
                            is_active,
                            format!("{pinned_prefix}{muted_prefix}{private_prefix}{display_title}"),
                        );
                        response
                            .widget_info(|| a11y_info(WidgetType::SelectableLabel, &a11y_label));
                        if response.clicked() && !is_active {
                            app_state.wm.set_active_pane(*pane_id);
                            app_state.update_status();
                        }

                        let close_btn = ui.small_button("\u{00d7}");
                        close_btn.widget_info(move || a11y_info(WidgetType::Button, &close_label));
                        if close_btn.clicked() {
                            app_state.pending_tab_close = Some(*pane_id);
                        }

                        ui.add_space(4.0);
                    });
                });
            }
        });
    } else {
        // "+" new tab button at the top of the sidebar.
        if ui.button("+").clicked() {
            app_state.pending_new_tab = true;
        }
        egui::ScrollArea::vertical().show(ui, |ui| {
            // Collect tab display data upfront to avoid borrow conflicts.
            struct TabEntry {
                pane_id: uuid::Uuid,
                tab_idx: usize,
                title: String,
                url: String,
                is_active: bool,
                is_terminal: bool,
                is_pinned: bool,
                is_muted: bool,
                is_private: bool,
            }
            let mut entries: Vec<TabEntry> = Vec::new();
            for (pane_id, _rect) in &panes {
                let is_active_pane = *pane_id == active_id;
                let is_terminal = app_state.is_terminal_pane(pane_id);
                let pane = app_state.wm.find_pane(*pane_id);
                let tab_count = pane.map(|p| p.tabs.len()).unwrap_or(1);
                let active_tab_idx = pane.map(|p| p.tabs.active_index()).unwrap_or(0);
                for tab_idx in 0..tab_count {
                    let (tab_title, tab_url) = pane
                        .and_then(|p| p.tabs.get(tab_idx))
                        .map(|t| {
                            let url_str = t.url.as_str();
                            let title = if t.title.is_empty() || t.title == "about:blank" {
                                url_str.rsplit('/').next().unwrap_or("New Tab").to_string()
                            } else {
                                t.title.clone()
                            };
                            (title, url_str.to_string())
                        })
                        .unwrap_or_else(|| ("New Tab".into(), "aileron://new".into()));
                    entries.push(TabEntry {
                        pane_id: *pane_id,
                        tab_idx,
                        title: tab_title,
                        url: tab_url,
                        is_active: is_active_pane && tab_idx == active_tab_idx,
                        is_terminal,
                        is_pinned: app_state.tabs.pinned_pane_ids.contains(pane_id),
                        is_muted: app_state.tabs.muted_pane_ids.contains(pane_id),
                        is_private: app_state.tabs.private_pane_ids.contains(pane_id),
                    });
                }
            }

            for entry in &entries {
                let trunc_s = truncate_str(&entry.title, 17);
                let trunc_u = truncate_str(&entry.url, 19);

                let frame_color = if entry.is_active {
                    egui::Color32::from_rgb(40, 60, 90)
                } else {
                    tab_bar_bg
                };

                let muted_prefix = if entry.is_muted { "\u{1f507} " } else { "" };
                let pinned_prefix = if entry.is_pinned { "\u{1f4cc} " } else { "" };
                let private_prefix = if entry.is_private { "\u{1f512} " } else { "" };

                egui::Frame::new().fill(frame_color).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let icon = if entry.is_terminal {
                            "\u{2328}"
                        } else {
                            "\u{1f310}"
                        };
                        ui.label(icon);

                        let response = ui.selectable_label(
                            entry.is_active,
                            format!("{pinned_prefix}{muted_prefix}{private_prefix}{trunc_s}"),
                        );
                        response.widget_info(|| {
                            a11y_info(
                                WidgetType::SelectableLabel,
                                format!("Tab: {} - {}", entry.title, entry.url),
                            )
                        });
                        if response.clicked() && !entry.is_active {
                            let pid = entry.pane_id;
                            let tidx = entry.tab_idx;
                            if let Some(p) = app_state
                                .wm
                                .root_mut()
                                .and_then(|root| crate::wm::BspTree::find_pane_mut(root, pid))
                                .filter(|p| p.tabs.get(tidx).is_some())
                            {
                                let url = p.tabs.get(tidx).unwrap().url.clone();
                                p.tabs.switch_to(tidx);
                                app_state
                                    .pending_wry_actions
                                    .push_back(WryAction::Navigate(url));
                                app_state.tabs.tab_display_dirty = true;
                                app_state.ui.status_message =
                                    format!("Tab {}/{}", p.tabs.active_index() + 1, p.tabs.len());
                            }
                        }

                        let close_label = format!("Close tab: {}", entry.title);
                        let close_btn = ui.small_button("\u{00d7}");
                        close_btn.widget_info(move || a11y_info(WidgetType::Button, &close_label));
                        if close_btn.clicked() {
                            app_state.pending_tab_close = Some(entry.pane_id);
                        }
                    });

                    if !entry.is_terminal {
                        ui.label(egui::RichText::new(trunc_u).small().color(border_color));
                    } else {
                        ui.label(egui::RichText::new("Terminal").small().color(border_color));
                    }
                });
                ui.add_space(2.0);
            }
        });
    }
}
