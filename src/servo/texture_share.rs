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
    #[must_use = "ignoring this value may lead to unexpected behavior"]
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

/// A pool of recycled RGBA pixel buffers for multi-pane scenarios.
///
/// Avoids per-frame heap allocation by reusing buffers across frames.
/// Buffers are keyed by `(width, height)` so only same-dimension buffers
/// are reused. The pool evicts oldest entries when it exceeds `max_pool_size`.
pub struct TexturePool {
    /// Recycled RGBA buffers indexed by `(width, height)`.
    pool: std::collections::HashMap<(u32, u32), Vec<Vec<u8>>>,
    /// Maximum number of buffers to retain across all dimensions.
    max_pool_size: usize,
}

impl TexturePool {
    /// Create a new texture pool with the default max size of 16 buffers.
    pub fn new() -> Self {
        Self {
            pool: std::collections::HashMap::new(),
            max_pool_size: 16,
        }
    }

    /// Create a new texture pool with a custom max buffer count.
    pub fn with_max_size(max_pool_size: usize) -> Self {
        Self {
            pool: std::collections::HashMap::new(),
            max_pool_size,
        }
    }

    /// Acquire a buffer suitable for the given dimensions.
    ///
    /// Returns a recycled buffer if one with matching dimensions exists,
    /// otherwise allocates a new buffer. The returned buffer is cleared
    /// but its underlying allocation is retained.
    pub fn acquire(&mut self, width: u32, height: u32) -> Vec<u8> {
        let needed = (width as usize) * (height as usize) * 4;
        if let Some(buffers) = self.pool.get_mut(&(width, height))
            && let Some(mut buf) = buffers.pop()
            && buf.capacity() >= needed
        {
            buf.clear();
            return buf;
        }
        Vec::with_capacity(needed)
    }

    /// Release a buffer back into the pool for later reuse.
    ///
    /// If the pool is at capacity, the oldest buffer (from the non-full
    /// dimension bucket) is evicted first.
    pub fn release(&mut self, width: u32, height: u32, mut buffer: Vec<u8>) {
        let total: usize = self.pool.values().map(|v| v.len()).sum();
        if total >= self.max_pool_size {
            self.evict_oldest();
        }
        buffer.clear();
        self.pool.entry((width, height)).or_default().push(buffer);
    }

    /// Evict one buffer to make room. Prefers the dimension bucket with
    /// the fewest buffers (least likely to be reused soon).
    fn evict_oldest(&mut self) {
        if let Some(key) = self
            .pool
            .iter()
            .min_by_key(|(_, v)| v.len())
            .map(|(k, _)| *k)
            && let Some(buffers) = self.pool.get_mut(&key)
        {
            buffers.pop();
            if buffers.is_empty() {
                self.pool.remove(&key);
            }
        }
    }

    /// Total number of buffers currently pooled.
    #[must_use]
    pub fn len(&self) -> usize {
        self.pool.values().map(|v| v.len()).sum()
    }

    /// Whether the pool is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pool.values().all(|v| v.is_empty())
    }

    /// Clear all pooled buffers.
    pub fn clear(&mut self) {
        self.pool.clear();
    }
}

impl Default for TexturePool {
    fn default() -> Self {
        Self::new()
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

    #[test]
    fn test_texture_pool_acquire_creates_buffer() {
        let mut pool = TexturePool::new();
        let buf = pool.acquire(100, 100);
        assert_eq!(buf.len(), 0);
        assert!(buf.capacity() >= 100 * 100 * 4);
    }

    #[test]
    fn test_texture_pool_reuse() {
        let mut pool = TexturePool::new();
        let buf = pool.acquire(100, 100);
        pool.release(100, 100, buf);
        let buf2 = pool.acquire(100, 100);
        // Reused buffer should have the same capacity
        assert!(buf2.capacity() >= 100 * 100 * 4);
    }

    #[test]
    fn test_texture_pool_eviction() {
        let mut pool = TexturePool::with_max_size(2);
        let b1 = pool.acquire(10, 10);
        pool.release(10, 10, b1);
        let b2 = pool.acquire(20, 20);
        pool.release(20, 20, b2);
        let b3 = pool.acquire(30, 30);
        pool.release(30, 30, b3);
        assert!(pool.len() <= 2);
    }

    #[test]
    fn test_texture_pool_different_dimensions() {
        let mut pool = TexturePool::new();
        let b1 = pool.acquire(100, 100);
        pool.release(100, 100, b1);
        let b2 = pool.acquire(200, 200);
        pool.release(200, 200, b2);
        assert_eq!(pool.len(), 2);
        // Acquiring different dimensions should not reuse
        let buf = pool.acquire(100, 200);
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn test_texture_pool_clear() {
        let mut pool = TexturePool::new();
        let b1 = pool.acquire(10, 10);
        pool.release(10, 10, b1);
        assert!(!pool.is_empty());
        pool.clear();
        assert!(pool.is_empty());
    }
}
