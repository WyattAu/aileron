# Spike: Native GTK Overlay Chrome (`native-chrome` feature)

**Decision date:** 2026-10-29 · **Timebox:** 2–3 weeks · **Owner:** @WyattAu

## Why

The 2026-10-24..29 reliability campaign traced every field defect (50GB shmem
OOM, ~1000fps idle spin, 8-minute snapshot callbacks, wgpu validation hazards,
chrome-pane MCP hijacks, trunk-build startup degradation) to one architectural
choice: rendering the browser chrome as a Leptos-WASM app in an offscreen
WebKitGTK webview, composited over page content via a hand-built wgpu pipeline.

## Windowing constraint (2026-10-30 correction — material)

On Wayland, the primary window is winit-owned; GTK widgets cannot be hosted
in it. A GTK HeaderBar chrome therefore requires replacing winit with a
GTK-primary window — an architecture change beyond this spike's scope. Two
viable native paths remain:

1. **egui chrome layer (recommended):** draw the chrome in the existing wgpu
   pass (`gfx::render_frame`), exactly as the terminal panes already render
   (egui + portable_pty are proven in this codebase). No windowing change;
   the chrome webview, its capture, and the updateChromeState IPC are
   deleted. Fits YP-GFX-SCHED-002's gate unchanged.
2. **GTK-primary window:** full GTK chrome + native WebKitGTK widgets (the
   classic GTK browser shape). Deletes winit. Bigger rewrite; deferred
   until egui proves insufficient.

The compositing of WEB CONTENT stays in both paths — only the chrome
layer changes substrate.

## Ecosystem note (2026-10-30): egui was here before

`src/gfx/renderer.rs` is documented as "a simplified version of the old
egui-based renderer" — an egui pipeline existed and was removed when the
current wgpu compositor landed. The spike is therefore a re-introduction:
the wgpu surface, the composited frame path, and the terminal grid's
rendering all remain; only the chrome draws through a re-added egui pass.
Risk: re-learning the removal's reasons (git history in the renderer
docs) before committing to the dependency re-add.

## Spike hypothesis (revised)

Two native paths, both replacing the Leptos-WASM chrome webview:

- **egui chrome layer (recommended):** the chrome drawn as an egui pass in
  the existing wgpu compositing — the same mechanism the terminal panes
  already render through (egui + portable_pty are proven in this codebase).
  No windowing change; deletes the chrome webview, its capture, and the
  updateChromeState IPC. Fits YP-GFX-SCHED-002's gate unchanged.
- **GTK-primary chrome:** native GTK widgets and a GTK-owned window (the
  classic GTK browser shape). Deletes winit — an architecture change
  beyond this spike; revisit if the egui path proves insufficient.

Both deliver equivalent UX with:

- idle memory flat (vs ~3GB baseline that grew to 50GB)
- idle CPU ~0 (no compositing loop, no capture)
- MCP same-batch read latency <1s after navigate (no capture lag, no settle)
- zero WebKitGTK-snapshot / wgpu / NVIDIA-GL code in the chrome path

## Scaffold (merged behind `native-chrome` feature)

- `src/chrome_native.rs` — `NativeChrome` shell: `gtk::Overlay` +
  `HeaderBar` chrome stack + content container slot for BSP-positioned
  WebKitGTK panes (compiles, unwired).

## Spike steps

1. Window wiring: `NativeChrome::build()` replaces the chrome webview
   bootstrap in `bootstrap.rs` behind the feature flag.
2. Content: BSP reposition targets `content_container` with real
   `webkit2gtk::WebView` children (replacing `create_offscreen_pane_for` on
   this path; MCP dispatch settle-wait can shrink to ~1s).
3. Omnibox/tab-strip signals routed through the WryAction queue.
4. Measure the three gates over one week of daily use.

## Decision rule

Adopt when all three gates pass for a week. Reject if any gate regresses vs
the compositing baseline; revisit with servo instead.
