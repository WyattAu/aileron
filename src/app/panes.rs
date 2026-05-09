use std::sync::Arc;
use tracing::warn;

use super::instance::{AileronApp, STATUS_BAR_HEIGHT, URL_BAR_HEIGHT};
use crate::servo::{EmbedMode, bsp_rect_to_wry_rect};

impl AileronApp {
    pub(crate) fn create_wry_pane_for(&mut self, pane_id: uuid::Uuid, url: &url::Url) {
        if self.config.is_offscreen() {
            self.create_offscreen_pane_for(pane_id, url);
            return;
        }

        let window = match &self.window {
            Some(w) => Arc::clone(w),
            None => return,
        };

        let is_terminal = {
            let app_state = match &self.app_state {
                Some(s) => s,
                None => return,
            };
            app_state.terminal_pane_ids.contains(&pane_id)
        };

        let wm_rect = {
            let app_state = match &self.app_state {
                Some(s) => s,
                None => return,
            };
            let panes = app_state.wm.panes();
            match panes.iter().find(|(id, _)| *id == pane_id) {
                Some((_, rect)) => *rect,
                None => {
                    warn!("BSP rect not found for pane {}", &pane_id.to_string()[..8]);
                    return;
                }
            }
        };

        let wry_rect = {
            let app_state = match &self.app_state {
                Some(s) => s,
                None => return,
            };
            let tab_layout = app_state.config.tab_layout.as_str();
            let sidebar_width = if tab_layout == "sidebar" {
                app_state.config.tab_sidebar_width as f64
            } else {
                0.0
            };
            let sidebar_on_right = app_state.config.tab_sidebar_right;
            bsp_rect_to_wry_rect(
                &wm_rect,
                STATUS_BAR_HEIGHT,
                URL_BAR_HEIGHT,
                sidebar_width,
                sidebar_on_right,
            )
        };

        let blocked_domains: Vec<String> = self.adblocker.blocked_domains_iter();

        let https_safe_list = if self.config.https_upgrade_enabled {
            self.app_state
                .as_mut()
                .map(|s| s.get_cached_https_safe_list())
                .unwrap_or_default()
        } else {
            std::collections::HashSet::new()
        };

        let interceptor_registry = self
            .app_state
            .as_ref()
            .map(|s| s.extension_manager.read().interceptor_registry.clone());

        match self.wry_panes.create_pane(
            &*window,
            pane_id,
            url.clone(),
            wry_rect,
            blocked_domains,
            https_safe_list,
            self.config.devtools,
            self.config.popup_blocker_enabled,
            interceptor_registry,
        ) {
            Ok(()) => {
                if is_terminal {
                    match self.terminal_manager.create_terminal(pane_id, 80, 24) {
                        Ok(_size) => {
                            if let Some(app_state) = &mut self.app_state
                                && let Some(cmd) = app_state.pending_terminal_command.take()
                            {
                                self.terminal_manager.write_input(&pane_id, &cmd);
                            }
                        }
                        Err(e) => warn!("Failed to create terminal: {}", e),
                    }
                }

                let mode = self.wry_panes.get(&pane_id).map(|p| p.embed_mode());
                let mode_str = match mode {
                    Some(EmbedMode::ChildWindow) => "X11 child",
                    Some(EmbedMode::GtkWindow) => "GTK window (Wayland)",
                    None => "unknown",
                };
                tracing::info!(
                    "WryPane {} created ({}) -> {}",
                    &pane_id.to_string()[..8],
                    mode_str,
                    url
                );
            }
            Err(e) => {
                warn!("Failed to create WryPane: {}", e);
                if let Some(app_state) = &mut self.app_state {
                    app_state.ui.status_message = format!("Pane creation failed: {e}");
                }
            }
        }
    }

    pub(crate) fn create_offscreen_pane_for(&mut self, pane_id: uuid::Uuid, url: &url::Url) {
        let is_terminal = {
            let app_state = match &self.app_state {
                Some(s) => s,
                None => return,
            };
            app_state.terminal_pane_ids.contains(&pane_id)
        };

        let wm_rect = {
            let app_state = match &self.app_state {
                Some(s) => s,
                None => return,
            };
            let panes = app_state.wm.panes();
            match panes.iter().find(|(id, _)| *id == pane_id) {
                Some((_, rect)) => *rect,
                None => {
                    warn!("BSP rect not found for pane {}", &pane_id.to_string()[..8]);
                    return;
                }
            }
        };

        let wry_rect = {
            let app_state = match &self.app_state {
                Some(s) => s,
                None => return,
            };
            let tab_layout = app_state.config.tab_layout.as_str();
            let sidebar_width = if tab_layout == "sidebar" {
                app_state.config.tab_sidebar_width as f64
            } else {
                0.0
            };
            let sidebar_on_right = app_state.config.tab_sidebar_right;
            bsp_rect_to_wry_rect(
                &wm_rect,
                STATUS_BAR_HEIGHT,
                URL_BAR_HEIGHT,
                sidebar_width,
                sidebar_on_right,
            )
        };

        let (width, height) = match wry_rect.size {
            winit::dpi::Size::Logical(s) => (s.width as i32, s.height as i32),
            winit::dpi::Size::Physical(s) => (s.width as i32, s.height as i32),
        };

        let blocked_domains: Vec<String> = self.adblocker.blocked_domains_iter();

        let https_safe_list = if self.config.https_upgrade_enabled {
            self.app_state
                .as_mut()
                .map(|s| s.get_cached_https_safe_list())
                .unwrap_or_default()
        } else {
            std::collections::HashSet::new()
        };

        let interceptor_registry = self
            .app_state
            .as_ref()
            .map(|s| s.extension_manager.read().interceptor_registry.clone());

        #[cfg(target_os = "linux")]
        match self.offscreen_panes.create_pane_with_privacy(
            pane_id,
            url,
            width,
            height,
            blocked_domains,
            https_safe_list,
            true,
            true,
            self.config.devtools,
            self.config.popup_blocker_enabled,
            interceptor_registry,
        ) {
            Ok(()) => {
                if is_terminal {
                    match self.terminal_manager.create_terminal(pane_id, 80, 24) {
                        Ok(_size) => {
                            if let Some(app_state) = &mut self.app_state
                                && let Some(cmd) = app_state.pending_terminal_command.take()
                            {
                                self.terminal_manager.write_input(&pane_id, &cmd);
                            }
                        }
                        Err(e) => warn!("Failed to create terminal: {}", e),
                    }
                }

                tracing::info!(
                    "OffscreenWebView {} created -> {}",
                    &pane_id.to_string()[..8],
                    url
                );
            }
            Err(e) => {
                warn!("Failed to create OffscreenWebView: {}", e);
                if let Some(app_state) = &mut self.app_state {
                    app_state.ui.status_message = format!("Pane creation failed: {e}");
                }
            }
        }

        #[cfg(not(target_os = "linux"))]
        {
            let _ = (
                pane_id,
                url,
                width,
                height,
                blocked_domains,
                interceptor_registry,
            );
            warn!("Offscreen webview not supported on this platform");
        }
    }

    pub(crate) fn remove_wry_pane_for(&mut self, pane_id: &uuid::Uuid) {
        self.terminal_manager.remove(pane_id);
        self.wry_panes.remove_pane(pane_id);
        self.offscreen_panes.remove_pane(pane_id);
        self.webview_textures.remove(pane_id);
        self.webview_texture_handles.remove(pane_id);
        self.offscreen_last_capture.remove(pane_id);
        self.pending_pane_creates.retain(|(id, _)| id != pane_id);
        if let Some(app_state) = &mut self.app_state {
            app_state.cleanup_pane_state(pane_id);
        }
    }

    pub(crate) fn drain_pending_pane_creates(&mut self) {
        if self.pending_pane_creates.is_empty() {
            return;
        }

        let active_id = self.app_state.as_ref().map(|s| s.wm.active_pane_id());

        let current_pane_ids: std::collections::HashSet<uuid::Uuid> = self
            .app_state
            .as_ref()
            .map(|s| s.wm.panes().iter().map(|(id, _)| *id).collect())
            .unwrap_or_default();

        let has_active = self
            .pending_pane_creates
            .iter()
            .any(|(pid, _)| Some(*pid) == active_id && current_pane_ids.contains(pid));

        let to_create = if has_active {
            self.pending_pane_creates
                .iter()
                .position(|(pid, _)| Some(*pid) == active_id && current_pane_ids.contains(pid))
        } else {
            self.pending_pane_creates
                .iter()
                .position(|(pid, _)| current_pane_ids.contains(pid))
        };

        if let Some(idx) = to_create {
            let (pid, url) = self.pending_pane_creates.remove(idx).unwrap();
            self.create_wry_pane_for(pid, &url);
        }

        self.pending_pane_creates
            .retain(|(pid, _)| current_pane_ids.contains(pid));
    }

    pub(crate) fn reposition_all_panes(&mut self) {
        let app_state = match &self.app_state {
            Some(s) => s,
            None => return,
        };

        let tab_layout = app_state.config.tab_layout.as_str();
        let sidebar_width = if tab_layout == "sidebar" {
            app_state.config.tab_sidebar_width as f64
        } else {
            0.0
        };
        let sidebar_on_right = app_state.config.tab_sidebar_right;

        let panes = app_state.wm.panes();
        for (pane_id, wm_rect) in &panes {
            if let Some(wry_pane) = self.wry_panes.get(pane_id) {
                let wry_rect = bsp_rect_to_wry_rect(
                    wm_rect,
                    STATUS_BAR_HEIGHT,
                    URL_BAR_HEIGHT,
                    sidebar_width,
                    sidebar_on_right,
                );
                wry_pane.set_bounds(wry_rect);
            }
        }

        if self.config.is_offscreen() {
            use crate::terminal::grid::CellMetrics;

            for (pane_id, wm_rect) in &panes {
                let wry_rect = bsp_rect_to_wry_rect(
                    wm_rect,
                    STATUS_BAR_HEIGHT,
                    URL_BAR_HEIGHT,
                    sidebar_width,
                    sidebar_on_right,
                );
                let (w, h) = match wry_rect.size {
                    winit::dpi::Size::Logical(s) => (s.width as i32, s.height as i32),
                    winit::dpi::Size::Physical(s) => (s.width as i32, s.height as i32),
                };

                if self.terminal_manager.is_terminal(pane_id) {
                    if let Some(ws) = self.egui_winit.as_ref() {
                        let ctx = ws.egui_ctx();
                        let metrics = CellMetrics::from_egui(ctx, 14.0);
                        let cols = (w as f32 / metrics.cell_width).max(2.0) as u16;
                        let rows = (h as f32 / metrics.cell_height).max(1.0) as u16;
                        self.terminal_manager.resize(pane_id, cols, rows);
                    }
                } else {
                    if w > 0 && h > 0 {
                        self.offscreen_panes.resize(pane_id, w, h);
                    }
                }
            }
        }
    }
}
