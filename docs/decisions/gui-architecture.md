# Decision Record: Browser Chrome Architecture

**Date:** 2026-10-30 · **Status:** OPEN — decision requested
**Author:** @WyattAu with the MCP reliability campaign
**Supersedes:** the unrecorded rationale of `33549af` (egui+wgpu → Leptos WASM chrome pivot)

## The decision on the table

The chrome (toolbar, omnibox, tab strip, palette, panels) renders today as a
Leptos WASM app inside an offscreen WebKitGTK webview, captured to frames and
composited over page content via a hand-built wgpu pipeline. Three paths
forward:

| Path | Substrate | Rewrite cost | Kills from the 2026-10 campaign |
|---|---|---|---|
| **Status quo** | Leptos WASM chrome webview + offscreen capture + wgpu composite | 0 | nothing |
| **egui layer** | egui pass in the existing wgpu compositing (chrome de-webviewed; web capture stays) | medium (re-introduce egui/egui-wgpu; one draw pass) | the chrome-capture OOM contribution, the updateChromeState IPC, trunk dependency, chrome-pane MCP hijacks |
| **GTK-primary** | GTK-owned window, native GTK chrome, WebKitGTK webviews as real widgets | large (replaces winit; GTK on Win/Mac is weak) | the entire compositing pipeline incl. the WebKitGTK UI-process segfault exposure |

## Measured costs of the status quo (2026-10-24..30 campaign)

- Browser RSS grew to ~50GB (24GB shmem) in 65 minutes idle — the chrome
  webview's frame capture was the dominant contributor.
- The redraw loop spun at ~1000fps idle (~720k INFO log records).
- One UI-process SIGSEGV in libwebkit2gtk under shared-GL (NVIDIA) took the
  browser down twice; both crashes occurred during chrome/webview interleave.
- The chrome overlay pane hijacked MCP reads during page loads.
- Startup degrades without `trunk build` (chrome/dist missing).
- The Leptos chrome requires the `chrome_bridge` IPC protocol and JS state
  pushes to survive at all.

## What the egui pivot's history says

`33549af` removed egui/egui-wgpu/egui-winit/wgpu and moved all chrome to the
Leptos WASM webview. The commit records the *mechanics* but not the *reason*.
Before deciding, recover it: was the pivot driven by rendering quality,
developer ergonomics (HTML/CSS), input handling, or WASM tooling? If the
reasons still hold, the egui layer re-introduces them; if they were
NVIDIA-rendering artifacts, the shared-GL fallback has since changed the
picture.

## The deciding questions

1. What drove `33549af`? (Recover from design notes/memory; if it was
   rendering quality on NVIDIA, test the current shared-GL path first.)
2. Is chrome-as-HTML (Leptos) a product requirement — CSS-styled panels,
   web-grade typography — or an implementation preference?
3. Is cross-platform (Windows/macOS) first-class for aileron v1? If yes,
   GTK-primary narrows to egui-layer or status quo (GTK on Windows is weak).

## Recommendation (if forced today)

egui layer (Path: de-webview the chrome, keep the wgpu composite) — it
deletes the highest-cost subsystem measured in the campaign at medium
rewrite cost, and the terminal panes prove the wgpu+egui rendering pattern.
But this reverses a deliberate prior decision; recover its rationale first.

## Follow-ups either way

- The MCP layer (session-typed relay, epochs, lifecycle invariants, QA
  matrix) is chrome-independent — it ships unchanged on all paths.
- The WebKitGTK segfault evidence package is committed and ready for the
  upstream report on any path.
