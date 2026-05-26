use std::sync::Arc;
use winit::event::WindowEvent;
use winit::window::{Window, WindowId};

use super::instance::AileronApp;

impl AileronApp {
    pub(crate) fn init_popup_window(&mut self, window_id: WindowId, window: Arc<Window>) {
        let url = self
            .app_state
            .as_mut()
            .and_then(|s| s.pending_detach_url.take())
            .unwrap_or_else(|| url::Url::parse("aileron://new").unwrap());
        let blocked_domains: Vec<String> = self.adblocker.blocked_domains_iter();
        let https_safe_list = if self.config.https_upgrade_enabled {
            self.app_state
                .as_mut()
                .map(|s| s.get_cached_https_safe_list())
                .unwrap_or_default()
        } else {
            std::sync::Arc::new(std::collections::HashSet::new())
        };

        self.popup.init_popup_window(
            window_id,
            window,
            url,
            blocked_domains,
            https_safe_list,
            self.config.devtools,
        );
    }

    pub(crate) fn handle_popup_event(&mut self, window_id: WindowId, event: &WindowEvent) {
        self.popup.handle_popup_event(window_id, event);
    }
}
