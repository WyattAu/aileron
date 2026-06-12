//! GPU rendering pipeline for displaying offscreen webview frames.
//!
//! Uses wgpu to render captured BGRA pixel data as textures on the main window.
//! This replaces the old egui-based rendering pipeline that was removed when
//! chrome was replaced with Leptos WASM.

pub mod renderer;

pub use renderer::GfxState;
