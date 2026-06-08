use std::sync::Arc;
use tracing::warn;

use super::instance::{AileronApp, STATUS_BAR_HEIGHT, URL_BAR_HEIGHT};
use crate::servo::{EmbedMode, bsp_rect_to_wry_rect};

impl AileronApp {
    pub(crate) fn create_wry_pane_for(&mut self, pane_id: uuid::Uuid, url: &url::Url) {
        let window = match &self.window {
            Some(w) => Arc::clone(w),
            None => return,
        };

        #[cfg(all(target_os = "linux", feature = "terminal"))]
        let is_terminal = {
            let app_state = match &self.app_state {
                Some(s) => s,
                None => return,
            };
            app_state.is_terminal_pane(&pane_id)
        };

        let wm_rect = {
            let app_state = match &self.app_state {
                Some(s) => s,
                None => return,
            };
            let panes = app_state.wm.panes_ref();
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

        #[cfg(target_os = "linux")]
        let https_safe_list = if self.config.https_upgrade_enabled {
            self.app_state
                .as_mut()
                .map(|s| s.get_cached_https_safe_list())
                .unwrap_or_default()
        } else {
            std::sync::Arc::new(std::collections::HashSet::new())
        };
        #[cfg(not(target_os = "linux"))]
        let https_safe_list: std::sync::Arc<std::collections::HashSet<String>> =
            std::sync::Arc::new(std::collections::HashSet::new());

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
                #[cfg(all(target_os = "linux", feature = "terminal"))]
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

    pub(crate) fn remove_wry_pane_for(&mut self, pane_id: &uuid::Uuid) {
        #[cfg(feature = "terminal")]
        self.terminal_manager.remove(pane_id);
        self.wry_panes.remove_pane(pane_id);
        if let Some(app_state) = &mut self.app_state {
            app_state.cleanup_pane_state(pane_id);
        }
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

        let panes = app_state.wm.panes_ref();
        for (pane_id, wm_rect) in panes.iter() {
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
    }
}
