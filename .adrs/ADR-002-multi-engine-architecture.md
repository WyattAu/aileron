# ADR-002: Multi-Engine Architecture (Servo + System WebView Fallback)

## Status
Accepted

## Context
Servo is a modern web engine but may not render all websites correctly (complex SPAs, WebGL apps, sites with Chrome-specific quirks). Users need a fallback when Servo fails to render a page properly (Fatal Flaw #1 from init_discussions.md).

## Decision
Architect the rendering engine as a trait:
```rust
trait WebEngine {
    fn load_url(&self, url: Url);
    fn get_texture(&self) -> Option<TextureView>;
    fn send_event(&self, event: InputEvent);
    fn execute_js(&self, script: &str) -> Option<String>;
}
```

Two implementations:
1. **ServoEngine:** Primary; uses Servo Embedder API with wgpu texture output
2. **SystemWebViewEngine:** Fallback; uses `wry` (Tauri's system WebView wrapper)

The user presses `Ctrl+E` to reload the current pane using the system WebView engine.

## Consequences
- **Positive:** Users can always access any website; no "dead ends" due to engine limitations
- **Negative:** Two engine implementations to maintain; system WebView doesn't support wgpu texture sharing (uses CPU texture copy)
- **Risks:** System WebView may have different event model; texture copy adds latency

## Alternatives Considered
1. **Servo-only (no fallback):** Rejected — users would abandon Aileron when hitting broken sites
2. **Three engines (Servo + Chromium + WebKit):** Rejected — maintenance burden too high
3. **Embed full Chromium (like Electron):** Rejected — binary size, license (BSD vs MIT), defeats purpose of using Servo

## Related Standards
N/A

## Related ADRs
ADR-001 (Servo Embedder API Risk)

## Date
2026-04-11

## Author
Nexus (Principal Systems Architect)
