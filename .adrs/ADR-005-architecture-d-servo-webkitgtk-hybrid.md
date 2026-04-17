# ADR-005: Architecture D — Hybrid Servo+WebKitGTK

## Status
Proposed

## Context
Aileron currently uses wry (WebKitGTK on Linux) for all web rendering via offscreen 
capture (Architecture B, ADR-003). The long-term goal is to integrate Servo as the 
primary rendering engine for performance, security, and embeddability. However, Servo 
is not yet mature enough for full web browsing.

Architecture D proposes a hybrid approach:
- **Servo** for first-party, developer-focused content (documentation, code review, 
  dashboards, terminal-adjacent workflows)
- **WebKitGTK** (via wry) for legacy/general web content that Servo cannot yet handle

## Decision
Implement a dual-engine architecture where:
1. A `WebEngine` trait abstracts rendering backends
2. Servo embedder provides Servo-based panes (rendering directly to wgpu textures)
3. WebKitGTK (wry) continues as fallback for general web content
4. Per-pane engine selection via config or heuristics
5. Shared UI layer (egui) composites both engine outputs

## Consequences
### Positive
- Servo eliminates the CPU readback bottleneck (~5-8ms/pane at 1080p)
- Direct wgpu texture sharing avoids BGRA→RGBA conversion
- Servo's Rust-native architecture aligns with Aileron's goals
- Progressive migration: one pane type at a time
- No big-bang rewrite risk

### Negative
- Two rendering paths to maintain
- Servo's CSS/JS compatibility is incomplete (estimated 85-90%)
- Servo binary size adds ~15-20MB
- Engine switching UX needs careful design
- Servo build from source takes 15-30 minutes

### Risks
- Servo embedder API may change (tracked in ADR-001)
- Performance regression if engine switching is slow
- User confusion about which engine is active

## Timeline
- **Q3 2026:** Servo embedder proof-of-concept (single pane, no navigation)
- **Q4 2026:** Servo with basic navigation, form support, and CSS
- **Q1 2027:** Hybrid mode: per-pane engine selection
- **Q2 2027:** Servo as default for developer content, WebKit fallback

## Alternatives Considered
1. **WebKit-only (current):** Works but CPU readback limits performance
2. **Servo-only:** Too immature, would break many sites
3. **Chromium (CEF):** Bloated, non-Rust, licensing concerns
4. **Electron:** Defeats the purpose of a lightweight terminal-like browser

## Related ADRs
- ADR-001: Servo dependency broken (WebEngine trait abstraction)
- ADR-002: Dual-engine strategy
- ADR-003: Offscreen webview rendering (current Architecture B)
- ADR-004: Native terminal emulator
