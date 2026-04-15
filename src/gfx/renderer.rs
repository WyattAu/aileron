use egui_wgpu::ScreenDescriptor;
use std::sync::Arc;
use tracing::info;
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

impl GfxState {
    /// Initialize wgpu + egui renderer for the given window.
    /// Must be called from a context where blocking is acceptable (e.g., `resumed`).
    pub fn new(window: Arc<Window>) -> anyhow::Result<Self> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..Default::default()
        });

        let surface = instance.create_surface(Arc::clone(&window))?;

        let adapter = pollster::block_on(async {
            instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    compatible_surface: Some(&surface),
                    ..Default::default()
                })
                .await
        })
        .ok_or_else(|| {
            // Provide helpful error message for common issues
            let vk_icd = std::env::var("VK_ICD_FILENAMES").unwrap_or_default();
            let wayland_display = std::env::var("WAYLAND_DISPLAY").unwrap_or_default();
            anyhow::anyhow!(
                "No suitable GPU adapter found. \
                 Vulkan surface creation failed.\n\
                 Hints:\n  - VK_ICD_FILENAMES={}\n  - WAYLAND_DISPLAY={}\n  - Try: WINIT_UNIX_BACKEND=x11",
                if vk_icd.is_empty() { "(not set)" } else { &vk_icd },
                if wayland_display.is_empty() { "(not set)" } else { &wayland_display },
            )
        })?;

        info!("GPU adapter: {:?}", adapter.get_info());

        let (device, queue) = pollster::block_on(async {
            adapter
                .request_device(
                    &wgpu::DeviceDescriptor {
                        label: Some("aileron-device"),
                        required_features: wgpu::Features::empty(),
                        required_limits: wgpu::Limits::default(),
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

        surface.configure(
            &device,
            &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: surface_format,
                width: window.inner_size().width,
                height: window.inner_size().height,
                present_mode: wgpu::PresentMode::AutoVsync,
                alpha_mode: surface_capabilities.alpha_modes[0],
                view_formats: vec![],
                desired_maximum_frame_latency: 2,
            },
        );

        let egui_renderer = egui_wgpu::Renderer::new(
            &device,
            surface_format,
            None,  // depth_format
            1,     // msaa_samples
            false, // dithering
        );

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
                    alpha_mode: wgpu::CompositeAlphaMode::Auto,
                    view_formats: vec![],
                    desired_maximum_frame_latency: 2,
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
