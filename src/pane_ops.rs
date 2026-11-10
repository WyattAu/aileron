//! `PaneOps` implementation for the headed/engine side (S4 stage 2c).
//!
//! `CombinedPaneOps` bundles the two pane managers behind the core
//! `PaneOps` trait so `process_mcp_commands` can live in aileron-core.
//! The settle budgets, the GTK pump, and the two-path screenshot
//! capture are behavior-locked (GATE-3 soak) — this impl reproduces the
//! exact mechanics the MCP arms used inline.

use std::sync::{Arc, Mutex};

use image::ImageEncoder;
use url::Url;
use uuid::Uuid;

use aileron_core::agent::PaneOps;

use crate::frame_tasks::pump_gtk_loop;
use crate::offscreen_webview::OffscreenWebViewManager;
use crate::servo::WryPaneManager;

pub struct CombinedPaneOps<'a> {
    pub wry: &'a mut WryPaneManager,
    pub off: &'a mut OffscreenWebViewManager,
}

impl PaneOps for CombinedPaneOps<'_> {
    fn first_content_pane(&self) -> Option<Uuid> {
        self.off
            .iter()
            .map(|(id, _)| *id)
            .find(|id| *id != Uuid::nil())
    }

    fn pane_exists(&self, id: &Uuid) -> bool {
        self.wry.get(id).is_some() || self.off.get(id).is_some()
    }

    fn pane_url_title(&self, id: &Uuid) -> Option<(String, String)> {
        self.off
            .get(id)
            .map(|p| (p.url().as_str().to_string(), p.title().to_string()))
            .or_else(|| {
                self.wry
                    .get(id)
                    .map(|p| (p.url().as_str().to_string(), p.title().to_string()))
            })
    }

    fn pane_url(&self, id: &Uuid) -> Option<String> {
        self.off
            .get(id)
            .map(|p| p.url().to_string())
            .or_else(|| self.wry.get(id).map(|p| p.url().to_string()))
    }

    fn all_panes(&self) -> Vec<(Uuid, String, String)> {
        self.off
            .iter()
            .map(|(id, p)| (*id, p.url().to_string(), p.title().to_string()))
            .collect()
    }

    fn navigate_settled(&mut self, id: &Uuid, url: &Url) -> Option<bool> {
        if let Some(wry_pane) = self.wry.get_mut(id) {
            // X11 / GTK-window-embedded native pane.
            wry_pane.navigate(url);
            // Settle: pump until the webview's URL property reflects the
            // target (navigation committed), so same-batch reads do not
            // evaluate the outgoing document. Bounded at 15s.
            let nav_deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
            while std::time::Instant::now() < nav_deadline {
                pump_gtk_loop();
                if wry_pane.url().as_str() == url.as_str() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            Some(false)
        } else if let Some(pane) = self.off.get_mut(id) {
            // Wayland: direct navigation + the dispatch settle-wait
            // (pane.is_loading()) keeps same-batch reads correct.
            // Navigate + deterministic settle: flush stale load events,
            // then wait for THIS load's Started->Complete pair while
            // pumping GTK. Bounded at 60s; slow pages fall through
            // best-effort.
            pane.navigate_settled(url, std::time::Duration::from_secs(60));
            Some(true)
        } else {
            None
        }
    }

    fn set_user_agent(&mut self, id: &Uuid, ua: &str) -> bool {
        self.off
            .get_mut(id)
            .map(|p| p.set_user_agent(ua))
            .unwrap_or(false)
    }

    fn set_viewport(&mut self, id: &Uuid, w: i32, h: i32) -> bool {
        self.off.get_mut(id).map(|p| p.set_viewport(w, h)).is_some()
    }

    fn screenshot_data_uri(&mut self, id: &Uuid) -> String {
        if let Some(pane) = self.off.get_mut(id) {
            let dims = pane.frame().map(|f| (f.width, f.height));
            if pane.capture_frame().is_none() {
                tracing::warn!("Screenshot frame capture returned no data");
            }
            let rgba = pane.frame_rgba().map(|r| r.to_vec());
            let dims = dims.or_else(|| pane.frame().map(|f| (f.width, f.height)));
            match (dims, rgba) {
                (Some((w, h)), Some(rgba)) => match image::RgbaImage::from_raw(w, h, rgba) {
                    Some(img) => {
                        let mut png_bytes = Vec::new();
                        let encoder = image::codecs::png::PngEncoder::new(&mut png_bytes);
                        if encoder
                            .write_image(img.as_raw(), w, h, image::ExtendedColorType::Rgba8)
                            .is_ok()
                        {
                            use base64::Engine;
                            let b64 = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
                            format!("data:image/png;base64,{b64}")
                        } else {
                            "Error: failed to encode PNG".into()
                        }
                    }
                    None => "Error: invalid frame dimensions".into(),
                },
                _ => "Error: no frame available".into(),
            }
        } else if self.wry.get(id).is_some() {
            // Native wry pane (X11 / non-composited Wayland): Canvas API
            // capture. The result arrives via the GTK main loop, so wait
            // with the same pump used for JS results.
            let buffer: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
            if let Some(pane) = self.wry.get(id) {
                pane.start_capture_screenshot(Arc::clone(&buffer));
            }
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            let mut out = "Error: screenshot capture timed out".into();
            while std::time::Instant::now() < deadline {
                if let Some(data) =
                    crate::servo::wry_engine::WryPane::pending_screenshot_result(&buffer)
                {
                    out = data;
                    break;
                }
                pump_gtk_loop();
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            out
        } else {
            "Error: no active pane".into()
        }
    }

    fn execute_js(&mut self, id: &Uuid, js: &str, on_result: Box<dyn Fn(String) + Send>) -> bool {
        crate::frame_tasks::dispatch_active_js_with_callback(self.wry, self.off, id, js, on_result)
    }

    fn reset_offscreen(&mut self) {
        *self.off = OffscreenWebViewManager::new();
    }

    fn webview_is_loading(&self, id: &Uuid) -> bool {
        self.off
            .get(id)
            .map(|p| p.webview_is_loading())
            .unwrap_or(false)
    }

    fn pump(&mut self) {
        pump_gtk_loop();
    }
}
