//! WebExtensions API — Rust trait definitions for browser extension support.
//!
//! This module provides trait-based API surface compatible with a subset of
//! the WebExtensions standard. Implementations are decoupled through trait objects.
//! See `.specs/02_architecture/webextensions_api_design.md` for full design.

pub mod alarms;
pub mod api;
pub mod builtin_adblock;
pub mod context_menus;
pub mod cookies;
pub mod declarative_net_request;
pub mod impls;
pub mod loader;
pub mod manifest;
pub mod message_bus;
pub mod permissions;
pub mod runtime;
pub mod scripting;
pub mod storage;
pub mod tabs;
pub mod types;
pub mod web_request;

pub use alarms::*;
pub use api::*;
pub use builtin_adblock::builtin_adblock_id;
pub use context_menus::*;
pub use cookies::*;
pub use declarative_net_request::*;
pub use impls::AileronExtensionApi;
pub use loader::ExtensionManager;
pub use manifest::*;
pub use message_bus::{MessageBus, RoutedMessage};
pub use permissions::*;
pub use runtime::*;
pub use scripting::*;
pub use storage::*;
pub use tabs::*;
pub use types::*;
pub use web_request::*;
