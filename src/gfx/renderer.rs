//! GPU renderer for displaying offscreen webview frames.
//!
//! Uses wgpu to render captured RGBA pixel data as textures on the main window.
//! This is a simplified version of the old egui-based renderer, adapted for
//! the Leptos WASM chrome architecture.

use std::sync::Arc;
use tracing::{info, warn};
use winit::window::Window;

/// Holds all wgpu rendering state.
pub struct GfxState {
    pub instance: wgpu::Instance,
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub surface_format: wgpu::TextureFormat,
    /// Last successfully configured surface size.
    surface_size: (u32, u32),
    /// Reusable BGRA buffer to avoid per-frame allocation.
    bgra_buffer: Vec<u8>,
}

/// GPU backend combinations to try, in order of preference.
fn backend_options() -> [wgpu::Backends; 3] {
    [
        wgpu::Backends::VULKAN | wgpu::Backends::GL,
        wgpu::Backends::GL,
        wgpu::Backends::VULKAN,
    ]
}

impl GfxState {
    /// Initialize wgpu renderer for the given window.
    /// Tries multiple GPU backend combinations with graceful fallback.
    pub fn new(window: Arc<Window>) -> anyhow::Result<Self> {
        let mut last_err = String::new();
        let mut result: Option<(wgpu::Instance, wgpu::Surface, wgpu::Adapter)> = None;

        for backends in backend_options() {
            let inst = wgpu::Instance::new(&wgpu::InstanceDescriptor {
                backends,
                ..Default::default()
            });

            let surf = match inst.create_surface(Arc::clone(&window)) {
                Ok(s) => s,
                Err(e) => {
                    last_err = format!("Surface creation failed (backends {backends:?}): {e}");
                    warn!("{}", last_err);
                    continue;
                }
            };

            let adapter = pollster::block_on(async {
                let opts = wgpu::RequestAdapterOptions {
                    compatible_surface: Some(&surf),
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    ..Default::default()
                };
                inst.request_adapter(&opts).await
            });

            // Fallback to low power adapter
            let adapter = adapter.or_else(|| {
                pollster::block_on(async {
                    let opts = wgpu::RequestAdapterOptions {
                        compatible_surface: Some(&surf),
                        power_preference: wgpu::PowerPreference::LowPower,
                        ..Default::default()
                    };
                    inst.request_adapter(&opts).await
                })
            });

            if let Some(a) = adapter {
                result = Some((inst, surf, a));
                break;
            }
            last_err = format!(
                "No adapter found (backends {:?}). VK_ICD_FILENAMES={} WAYLAND_DISPLAY={}",
                backends,
                std::env::var("VK_ICD_FILENAMES").unwrap_or_default(),
                std::env::var("WAYLAND_DISPLAY").unwrap_or_default(),
            );
            warn!("{}", last_err);
        }

        let (instance, surface, adapter) = result.ok_or_else(|| {
            anyhow::anyhow!(
                "No suitable GPU adapter found after trying all backend combinations.\n\
                 Last error: {last_err}\n\
                 Hints:\n  - Ensure Vulkan or OpenGL drivers are installed\n  \
                 - Try: WINIT_UNIX_BACKEND=x11\n  \
                 - Check: vulkaninfo | head -20"
            )
        })?;

        info!("GPU adapter: {:?}", adapter.get_info());

        let (device, queue) = pollster::block_on(async {
            let adapter_limits = adapter.limits();
            adapter
                .request_device(
                    &wgpu::DeviceDescriptor {
                        label: Some("aileron-device"),
                        required_features: wgpu::Features::empty(),
                        required_limits: adapter_limits,
                        ..Default::default()
                    },
                    None,
                )
                .await
        })?;

        let surface_capabilities = surface.get_capabilities(&adapter);

        // Prefer sRGB formats for gamma-correct rendering
        let surface_format = surface_capabilities
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_capabilities.formats[0]);

        // Prefer Opaque alpha mode
        let alpha_mode = surface_capabilities
            .alpha_modes
            .iter()
            .find(|m| **m == wgpu::CompositeAlphaMode::Opaque)
            .copied()
            .unwrap_or(surface_capabilities.alpha_modes[0]);

        let initial_size = window.inner_size();
        let initial_w = initial_size.width;
        let initial_h = initial_size.height;

        surface.configure(
            &device,
            &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_DST,
                format: surface_format,
                width: initial_w,
                height: initial_h,
                present_mode: wgpu::PresentMode::AutoVsync,
                alpha_mode,
                view_formats: vec![],
                desired_maximum_frame_latency: 1,
            },
        );

        info!("Graphics initialized (format: {:?})", surface_format);

        Ok(Self {
            instance,
            surface,
            device,
            queue,
            surface_format,
            surface_size: (initial_w, initial_h),
            bgra_buffer: Vec::new(),
        })
    }

    /// Resize the surface after a window resize event.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 && (width, height) != self.surface_size {
            self.surface.configure(
                &self.device,
                &wgpu::SurfaceConfiguration {
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_DST,
                    format: self.surface_format,
                    width,
                    height,
                    present_mode: wgpu::PresentMode::AutoVsync,
                    alpha_mode: wgpu::CompositeAlphaMode::Opaque,
                    view_formats: vec![],
                    desired_maximum_frame_latency: 1,
                },
            );
            self.surface_size = (width, height);
        }
    }

    /// Render an RGBA pixel buffer to the surface.
    ///
    /// Writes RGBA data directly to the surface texture via write_texture.
    pub fn render_frame(&mut self, rgba: &[u8], width: u32, height: u32) {
        let output = match self.surface.get_current_texture() {
            Ok(tex) => tex,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                let size = (self.surface_size.0, self.surface_size.1);
                self.resize(size.0, size.1);
                return;
            }
            Err(e) => {
                warn!("Surface error (skipping frame): {:?}", e);
                return;
            }
        };

        let buffer_size = (width * height * 4) as usize;
        if rgba.len() < buffer_size {
            return;
        }

        // Convert RGBA to BGRA for the surface format, reusing the buffer.
        self.bgra_buffer.clear();
        self.bgra_buffer.reserve(buffer_size);
        for chunk in rgba.chunks_exact(4) {
            self.bgra_buffer.push(chunk[2]); // B
            self.bgra_buffer.push(chunk[1]); // G
            self.bgra_buffer.push(chunk[0]); // R
            self.bgra_buffer.push(chunk[3]); // A
        }

        // Write directly to the surface texture
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &output.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &self.bgra_buffer,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * width),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width: output.texture.width().min(width),
                height: output.texture.height().min(height),
                depth_or_array_layers: 1,
            },
        );

        output.present();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_exactly_three_elements() {
        let opts = backend_options();
        assert_eq!(opts.len(), 3);
    }

    #[test]
    fn first_element_contains_vulkan_and_gl() {
        let opts = backend_options();
        assert_eq!(opts[0], wgpu::Backends::VULKAN | wgpu::Backends::GL);
    }

    #[test]
    fn no_duplicate_combinations() {
        let opts = backend_options();
        let unique: std::collections::HashSet<_> = opts.iter().collect();
        assert_eq!(unique.len(), 3);
    }
}
