# Spike: Native GTK Overlay Chrome (`native-chrome` feature)

**Decision date:** 2026-10-29 · **Timebox:** 2–3 weeks · **Owner:** @WyattAu

## Why

The 2026-10-24..29 reliability campaign traced every field defect (50GB shmem
OOM, ~1000fps idle spin, 8-minute snapshot callbacks, wgpu validation hazards,
chrome-pane MCP hijacks, trunk-build startup degradation) to one architectural
choice: rendering the browser chrome as a Leptos-WASM app in an offscreen
WebKitGTK webview, composited over page content via a hand-built wgpu pipeline.

## Spike hypothesis

Native GTK widgets (Overlay: live WebKitGTK webviews as base, native chrome
stack on top) deliver equivalent UX with:

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
