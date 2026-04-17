# ADR-004: Native Terminal Emulator (Replace xterm.js)

## Status
Accepted

## Date
2026-04-17

## Context

The current approach embeds xterm.js inside an offscreen webview (per ADR-003). This introduces several problems:

1. **7 handoff points per keystroke** — egui event → JS dispatchEvent → xterm.js → JS IPC → Rust → PTY write → PTY read → Rust → JS → xterm.js render → pixbuf readback → wgpu texture
2. **CPU readback overhead** — every terminal frame goes through `get_pixbuf()` + wgpu upload (ADR-003 negative consequence)
3. **~30-50MB per terminal pane** — full WebKitGTK process + xterm.js DOM + offscreen buffer
4. **CDN dependency** — xterm.js add-ons (fit, webgl, ligatures) loaded from unpkg/jsDelivr
5. **Input forwarding bugs** — keystrokes lost or duplicated when webview is offscreen or resizing; IME composition broken across the egui→JS boundary

Terminal output is fundamentally a grid of characters — a full web engine is overkill.

## Decision

**Replace xterm.js with a native Rust terminal emulator.**

Stack:
- `alacritty_terminal` (v0.26) — VT state machine (parser, grid, scrollbar, selection)
- `portable_pty` — PTY spawn and I/O (Linux/macOS/Windows)
- egui Painter — render terminal grid directly as egui primitives (no wgpu texture upload)

Architecture:
```
winit Window → wgpu → egui
├── Pane 1: TerminalWidget (native, no webview)
│   ├── alacritty_terminal: Grid<TerminalCell> + VT parser
│   ├── portable_pty: raw fd read/write
│   └── egui Painter: grid → egui rectangles/text
├── Pane 2: egui::Image(webview_texture)  ← OffscreenWindow (web content only)
└── Pane 3: egui::Image(webview_texture)
```

Terminal panes bypass the webview pipeline entirely. Keystrokes write directly to the PTY master fd; PTY output updates the in-memory grid; the grid renders via egui's immediate-mode painter.

## Alternatives

### Keep xterm.js (status quo)
- Retains ligatures, sixel image support, and mature accessibility
- All 5 problems above remain; performance degrades further with more panes

### Use `egui_term` crate
- Tight egui integration, quick to adopt
- Per-cell `egui::RichText` rendering has severe performance issues at 80×24+ grids
- Tightly coupled to egui internals, hard to customize
- Unmaintained, not suitable for production use

## Consequences

### Positive
- **~1-2ms keystroke latency** (was 15-30ms) — direct PTY write, no JS round-trips
- **~2-5MB per terminal pane** (was 30-50MB) — in-memory grid only, no web engine
- **No CDN dependency** — fully offline, no network required at runtime
- **No input forwarding bugs** — egui events map directly to PTY writes, no offscreen webview edge cases
- **Eliminates frame capture overhead for terminals** — no pixbuf readback or wgpu texture upload

### Negative
- **Lost xterm.js features** — font ligatures and sixel image support not available initially; can be added later via custom rendering passes
- **More Rust code to maintain** — VT parser integration, selection handling, scrollback, and accessibility are handled in-process rather than delegated to xterm.js

### Architecture Impact
- Simplifies **Architecture D**: terminal panes require no browser engine at all
- Reduces the number of offscreen webviews needed — only web content panes use the ADR-003 pipeline
- PTY lifecycle is now managed by Aileron directly, not by a JS terminal emulator

## Related ADRs
- ADR-003: Offscreen Webview Rendering via wgpu Textures (terminal was previously a webview consumer of this pipeline)
- ADR-002: Multi-engine Architecture (native terminal is orthogonal to web engine choice)
