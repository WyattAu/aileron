//! Local LLM integration via Ollama HTTP API.
//!
//! Ollama runs as a separate process (default `localhost:11434`).
//! This module provides an HTTP client — no embedded LLM runtime.

pub mod ollama;

pub use ollama::{OllamaClient, OllamaConfig, OllamaError, OllamaModel, OllamaResponse};
