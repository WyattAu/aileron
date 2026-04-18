//! WebExtensions API — Rust trait definitions for browser extension support.
//!
//! This module provides trait-based API surface compatible with a subset of
//! the WebExtensions standard. Implementations are decoupled through trait objects.
//! See `.specs/02_architecture/webextensions_api_design.md` for full design.

pub mod api;
pub mod manifest;
pub mod tabs;
pub mod storage;
pub mod runtime;
pub mod web_request;
pub mod scripting;
pub mod types;

pub use api::*;
pub use manifest::*;
pub use tabs::*;
pub use storage::*;
pub use runtime::*;
pub use web_request::*;
pub use scripting::*;
pub use types::*;
