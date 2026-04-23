//! Aileron Remote Protocol (ARP) — WebSocket server for mobile clients.
//!
//! Implements JSON-RPC 2.0 over WebSocket (tokio-tungstenite).
//! Specification: `.specs/02_architecture/arp_protocol_spec.md`

mod commands;
mod server;

pub use commands::ArpCommand;
pub use server::{ArpConfig, ArpServer};
