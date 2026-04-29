use egui_wgpu::ScreenDescriptor;
use std::sync::Arc;
use tracing::{info, warn};
use winit::window::Window;

/// Holds all wgpu + egui rendering state.
pub struct GfxState {
    pub instance: wgpu::Instance,
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub egui_renderer: egui_wgpu::Renderer,
    pub surface_format: wgpu::TextureFormat,
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
    /// Initialize wgpu + egui renderer for the given window.
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
                    last_err = format!("Surface creation failed (backends {:?}): {}", backends, e);
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
                 Last error: {}\n\
                 Hints:\n  - Ensure Vulkan or OpenGL drivers are installed\n  \
                 - Try: WINIT_UNIX_BACKEND=x11\n  \
                 - Check: vulkaninfo | head -20",
                last_err
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

        surface.configure(
            &device,
            &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: surface_format,
                width: window.inner_size().width,
                height: window.inner_size().height,
                present_mode: wgpu::PresentMode::AutoVsync,
                alpha_mode,
                view_formats: vec![],
                desired_maximum_frame_latency: 1,
            },
        );

        let egui_renderer = egui_wgpu::Renderer::new(&device, surface_format, None, 1, false);

        info!("Graphics initialized (format: {:?})", surface_format);

        Ok(Self {
            instance,
            surface,
            device,
            queue,
            egui_renderer,
            surface_format,
        })
    }

    /// Resize the surface after a window resize event.
    pub fn resize(&self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.surface.configure(
                &self.device,
                &wgpu::SurfaceConfiguration {
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    format: self.surface_format,
                    width,
                    height,
                    present_mode: wgpu::PresentMode::AutoVsync,
                    alpha_mode: wgpu::CompositeAlphaMode::Opaque,
                    view_formats: vec![],
                    desired_maximum_frame_latency: 1,
                },
            );
        }
    }

    /// Build a ScreenDescriptor from the current window size.
    pub fn screen_descriptor(&self, window: &Window) -> ScreenDescriptor {
        let size = window.inner_size();
        ScreenDescriptor {
            size_in_pixels: [size.width, size.height],
            pixels_per_point: window.scale_factor() as f32,
        }
    }
}
