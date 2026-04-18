//! Texture sharing infrastructure for Servo integration.
//!
//! Provides abstractions for sharing rendered content between
//! the browser engine and the egui compositor.

/// Strategy for sharing rendered content between engine and compositor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShareStrategy {
    /// Direct wgpu texture sharing (ideal, requires engine support).
    DirectWgpu,
    /// DMA-BUF sharing on Linux (zero-copy via kernel).
    DmaBuf,
    /// CPU readback fallback (copy pixels through shared memory).
    CpuReadback,
}

/// Metadata about a shared texture.
#[derive(Debug, Clone)]
pub struct SharedTexture {
    /// Unique identifier for this texture.
    pub id: uuid::Uuid,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Current sharing strategy.
    pub strategy: ShareStrategy,
    /// Whether the texture content has changed since last read.
    pub dirty: bool,
}

/// Handle to a shared texture that the compositor can consume.
/// In the real implementation, this would hold wgpu texture IDs or DMA-BUF fds.
#[derive(Debug)]
pub struct TextureShareHandle {
    /// The shared texture metadata.
    pub texture: SharedTexture,
    /// Pixel data for CPU readback mode (RGBA8).
    /// None when using DirectWgpu or DmaBuf strategies.
    pub pixel_data: Option<Vec<u8>>,
}

impl TextureShareHandle {
    /// Create a new texture share handle.
    pub fn new(width: u32, height: u32, strategy: ShareStrategy) -> Self {
        Self {
            texture: SharedTexture {
                id: uuid::Uuid::new_v4(),
                width,
                height,
                strategy,
                dirty: true,
            },
            pixel_data: if strategy == ShareStrategy::CpuReadback {
                Some(vec![0u8; (width * height * 4) as usize])
            } else {
                None
            },
        }
    }

    /// Mark the texture as clean (content has been consumed).
    pub fn mark_clean(&mut self) {
        self.texture.dirty = false;
    }

    /// Update pixel data (CPU readback mode only).
    pub fn update_pixels(&mut self, rgba_data: Vec<u8>) -> Result<(), TextureShareError> {
        if self.texture.strategy != ShareStrategy::CpuReadback {
            return Err(TextureShareError::InvalidStrategy);
        }
        let expected_size = (self.texture.width * self.texture.height * 4) as usize;
        if rgba_data.len() != expected_size {
            return Err(TextureShareError::SizeMismatch {
                expected: expected_size,
                actual: rgba_data.len(),
            });
        }
        self.pixel_data = Some(rgba_data);
        self.texture.dirty = true;
        Ok(())
    }

    /// Resize the shared texture.
    pub fn resize(&mut self, new_width: u32, new_height: u32) {
        self.texture.width = new_width;
        self.texture.height = new_height;
        if self.texture.strategy == ShareStrategy::CpuReadback {
            self.pixel_data = Some(vec![0u8; (new_width * new_height * 4) as usize]);
        }
        self.texture.dirty = true;
    }
}

/// Errors that can occur during texture sharing.
#[derive(Debug, thiserror::Error)]
pub enum TextureShareError {
    #[error("Invalid sharing strategy for this operation")]
    InvalidStrategy,
    #[error("Pixel data size mismatch: expected {expected}, got {actual}")]
    SizeMismatch { expected: usize, actual: usize },
    #[error("DMA-BUF not available on this platform")]
    DmaBufUnavailable,
    #[error("wgpu texture sharing failed: {0}")]
    WgpuError(String),
}

/// Detect the best available sharing strategy for the current platform.
pub fn detect_best_strategy() -> ShareStrategy {
    #[cfg(target_os = "linux")]
    {
        ShareStrategy::CpuReadback
    }
    #[cfg(not(target_os = "linux"))]
    {
        ShareStrategy::CpuReadback
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shared_texture_creation() {
        let handle = TextureShareHandle::new(800, 600, ShareStrategy::CpuReadback);
        assert_eq!(handle.texture.width, 800);
        assert_eq!(handle.texture.height, 600);
        assert!(handle.texture.dirty);
        assert!(handle.pixel_data.is_some());
    }

    #[test]
    fn test_direct_wgpu_no_pixel_data() {
        let handle = TextureShareHandle::new(800, 600, ShareStrategy::DirectWgpu);
        assert!(handle.pixel_data.is_none());
    }

    #[test]
    fn test_update_pixels_success() {
        let mut handle = TextureShareHandle::new(100, 100, ShareStrategy::CpuReadback);
        let pixels = vec![255u8; 100 * 100 * 4];
        assert!(handle.update_pixels(pixels).is_ok());
        assert!(handle.texture.dirty);
    }

    #[test]
    fn test_update_pixels_wrong_size() {
        let mut handle = TextureShareHandle::new(100, 100, ShareStrategy::CpuReadback);
        let pixels = vec![0u8; 50];
        let result = handle.update_pixels(pixels);
        assert!(result.is_err());
    }

    #[test]
    fn test_update_pixels_wrong_strategy() {
        let mut handle = TextureShareHandle::new(100, 100, ShareStrategy::DirectWgpu);
        let pixels = vec![0u8; 100 * 100 * 4];
        let result = handle.update_pixels(pixels);
        assert!(result.is_err());
    }

    #[test]
    fn test_mark_clean() {
        let mut handle = TextureShareHandle::new(100, 100, ShareStrategy::CpuReadback);
        handle.mark_clean();
        assert!(!handle.texture.dirty);
    }

    #[test]
    fn test_resize() {
        let mut handle = TextureShareHandle::new(100, 100, ShareStrategy::CpuReadback);
        handle.resize(200, 150);
        assert_eq!(handle.texture.width, 200);
        assert_eq!(handle.texture.height, 150);
        assert!(handle.texture.dirty);
    }

    #[test]
    fn test_detect_strategy() {
        let strategy = detect_best_strategy();
        match strategy {
            ShareStrategy::CpuReadback | ShareStrategy::DirectWgpu | ShareStrategy::DmaBuf => {}
        }
    }
}
