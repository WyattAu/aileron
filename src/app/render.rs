use tracing::warn;

use super::instance::{AileronApp, STATUS_BAR_HEIGHT};
use crate::ui::panels;

impl AileronApp {
    pub(crate) fn render(&mut self) {
        let window = match &self.window {
            Some(w) => w,
            None => return,
        };
        let winit_state = match &mut self.egui_winit {
            Some(s) => s,
            None => return,
        };
        let gfx = match &mut self.gfx {
            Some(g) => g,
            None => return,
        };
        let app_state = match &mut self.app_state {
            Some(s) => s,
            None => return,
        };

        let raw_input = winit_state.take_egui_input(window);

        let full_output = winit_state.egui_ctx().run(raw_input, |egui_ctx| {
            panels::build_ui(
                egui_ctx,
                app_state,
                &self.wry_panes,
                &self.git_status,
                STATUS_BAR_HEIGHT,
                &self.webview_textures,
                #[cfg(feature = "terminal")]
                &mut self.terminal_manager,
                &self.offscreen_panes,
            );
        });

        winit_state.handle_platform_output(window, full_output.platform_output);

        let egui_ctx = winit_state.egui_ctx();
        let paint_jobs = egui_ctx.tessellate(full_output.shapes, full_output.pixels_per_point);
        let textures_delta = &full_output.textures_delta;

        let screen_descriptor = gfx.screen_descriptor(window);

        for (id, image_delta) in &textures_delta.set {
            gfx.egui_renderer
                .update_texture(&gfx.device, &gfx.queue, *id, image_delta);
        }

        let mut encoder = gfx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("egui-encoder"),
            });

        let user_cmd_bufs = gfx.egui_renderer.update_buffers(
            &gfx.device,
            &gfx.queue,
            &mut encoder,
            &paint_jobs,
            &screen_descriptor,
        );

        let output = match gfx.surface.get_current_texture() {
            Ok(tex) => tex,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                let size = window.inner_size();
                gfx.resize(size.width, size.height);
                return;
            }
            Err(e) => {
                warn!("Surface error (skipping frame): {:?}", e);
                return;
            }
        };
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        {
            let render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui-main-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.08,
                            g: 0.08,
                            b: 0.08,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            let mut render_pass = render_pass.forget_lifetime();
            gfx.egui_renderer
                .render(&mut render_pass, &paint_jobs, &screen_descriptor);
        }

        gfx.queue.submit(
            user_cmd_bufs
                .into_iter()
                .chain(std::iter::once(encoder.finish())),
        );

        for id in &textures_delta.free {
            gfx.egui_renderer.free_texture(id);
        }

        output.present();
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn update_webview_textures(&mut self) -> bool {
        if self.offscreen_panes.is_empty() {
            return false;
        }

        let capture_interval = self.adaptive_quality.capture_interval_ms();
        let bg_capture_interval = self.adaptive_quality.background_capture_interval_ms();
        let skip_non_active = self.adaptive_quality.should_skip_non_active();
        let active_id = self.app_state.as_ref().map(|s| s.wm.active_pane_id());

        // Collect IDs of panes that need capture (avoid holding mutable borrows across texture updates).
        let mut captured: Vec<(uuid::Uuid, u32, u32)> = Vec::new();

        for (id, pane) in self.offscreen_panes.iter_mut() {
            if skip_non_active && active_id.is_some_and(|aid| aid != *id) {
                continue;
            }

            let is_active = active_id.is_some_and(|aid| aid == *id);
            let interval_ms = if is_active {
                capture_interval
            } else {
                bg_capture_interval
            };

            let last = self
                .offscreen_last_capture
                .get(id)
                .copied()
                .unwrap_or_else(|| std::time::Instant::now() - std::time::Duration::from_secs(10));
            let dirty = pane.is_dirty();
            let elapsed = last.elapsed();
            if dirty && elapsed >= std::time::Duration::from_millis(interval_ms as u64) {
                tracing::debug!(
                    "capture: pane {} dirty={} elapsed={:?} active={} interval={}ms",
                    &id.to_string()[..8],
                    dirty,
                    elapsed,
                    is_active,
                    interval_ms,
                );
                if pane.capture_frame().is_some()
                    && let Some(frame) = pane.frame()
                {
                    let fw = frame.width;
                    let fh = frame.height;
                    let needed = (fw as usize) * (fh as usize) * 4;
                    // Reuse existing buffer; only reallocate when pane size grows.
                    let buf = self
                        .capture_buffers
                        .entry(*id)
                        .or_insert_with(|| Vec::with_capacity(needed));
                    if buf.len() < needed {
                        buf.resize(needed, 0);
                    } else {
                        buf[..needed].fill(0);
                    }
                    if let Some(rgba) = pane.frame_rgba() {
                        let copy_len = rgba.len().min(needed);
                        buf[..copy_len].copy_from_slice(&rgba[..copy_len]);
                    }
                    captured.push((*id, fw, fh));
                }
                self.offscreen_last_capture
                    .insert(*id, std::time::Instant::now());
            }
        }

        let mut updated = false;
        for (pane_id, width, height) in captured {
            let rgba = self.capture_buffers.get(&pane_id);
            let Some(rgba) = rgba else {
                continue;
            };
            let color_image =
                egui::ColorImage::from_rgba_unmultiplied([width as usize, height as usize], rgba);

            if let Some(ws) = self.egui_winit.as_ref() {
                let ctx = ws.egui_ctx();

                if let Some(handle) = self.webview_texture_handles.get_mut(&pane_id) {
                    if handle.size() == [width as usize, height as usize] {
                        handle.set(color_image, egui::TextureOptions::LINEAR);
                    } else {
                        let new_handle = ctx.load_texture(
                            format!("webview-{pane_id}"),
                            color_image,
                            egui::TextureOptions::LINEAR,
                        );
                        self.webview_textures.insert(pane_id, new_handle.id());
                        self.webview_texture_handles.insert(pane_id, new_handle);
                    }
                } else {
                    let handle = ctx.load_texture(
                        format!("webview-{pane_id}"),
                        color_image,
                        egui::TextureOptions::LINEAR,
                    );
                    self.webview_textures.insert(pane_id, handle.id());
                    self.webview_texture_handles.insert(pane_id, handle);
                }
            }
            updated = true;
        }
        updated
    }
}
