# Aileron Architecture

## Overview

Aileron is a keyboard-driven, tiling web browser built in Rust. It uses a BSP (Binary Space Partitioning) tree for window management, WebKitGTK (via wry) for web rendering, and egui for the native UI layer.

## High-Level Architecture

```
+------------------+     +------------------+     +------------------+
|     egui/wgpu     |     |    wry/WebKitGTK   |     |    Offscreen     |
|  (native UI)      |     |  (web rendering)   |     |  (Architecture B)  |
+------------------+     +------------------+     +------------------+
         |                          |                        |
         v                          v                        v
+------------------------------------------------------------+
|                    AppState                          |
|  (mode machine, config, tabs, history, bookmarks) |
+------------------------------------------------------------+
         |                          |
         v                          v
+------------------+     +------------------+
|   BSP Tree (WM)  |     |  Frame Tasks     |
| (pane layout)    |     | (per-frame ops)  |
+------------------+     +------------------+
         |                          |
         v                          v
+------------------+     +------------------+
|  Lua Scripts    |     |  Extensions      |
|  (user macros)   |     |  (MV3 partial)   |
+------------------+     +------------------+
         |
         v
+------------------+
|    ARP Server    |
| (mobile sync)    |
+------------------+
```

## Core Systems

### Window Manager (`src/wm/`)

A BSP tree partitions the viewport into non-overlapping rectangular regions. Each leaf node holds one pane. The tree supports split, close, resize, navigate, and swap operations.

- **Data structures:** `BspTree` (tree), `BspNode` (leaf or split), `Pane` (tab list + metadata), `Rect` (axis-aligned rectangle)
- **Caching:** Pane list and pane IDs are cached via `RefCell<Vec>` with a dirty flag. `iter_panes()` and `iter_pane_ids()` avoid cloning the cache Vec on read.
- **Complexity:** Split/Close are O(log n) where n = number of panes.

### Input System (`src/input/`)

A mode machine with four modes: Normal, Insert, Command, and Find. Key events are routed through a `KeybindingRegistry` that maps `(mode, physical_key, modifiers)` to `Action` enums.

- **Keymap:** `KeyMap` converts winit `PhysicalKey` + `Modifiers` to `KeyEvent` (action + key string), then `KeybindingRegistry` looks up the binding.
- **Routing:** `Router` dispatches `KeyEvent` to the appropriate subsystem (keybind handler, egui, webview, terminal) based on the current mode.

### Web Rendering (`src/servo/`)

Two rendering architectures:

- **Architecture A (default):** wry manages native WKWebView windows, composited via winit/egui.
- **Architecture B (offscreen):** WebKitGTK renders to an offscreen buffer, captured as RGBA pixels, and displayed as an egui `Image` widget. This avoids transparency issues with Wayland compositors.

### UI Layer (`src/ui/`)

egui-based panels for the tab bar (horizontal or sidebar), status bar, URL bar, command palette, and various utility panels (history, bookmarks, tab search, site settings, workspace).

- **Performance:** Tab display data (title, URL, truncated variants) is cached in `TabDisplayInfo` structs, rebuilt only when dirty. The `truncate_str` helper returns `Cow<str>` to avoid allocation on the common (no-truncation) path.

### Command System (`src/app/`)

Commands are dispatched through `app::dispatch::dispatch_action()`, which maps `Action` enums to `WryAction` effects. Commands are available via:
- Keybindings (Normal/Command mode)
- Lua scripting (`aileron.cmd.create`)
- MCP bridge (`tools/browser_*`)
- ARP mobile clients (`tabs.create`, `tabs.navigate`)

### Extension System (`src/extensions/`)

Partial MV3 implementation:
- Manifest parsing (v2 and v3 formats)
- Content scripts (URL matching, CSS/JS injection)
- Storage API (per-extension localStorage)
- Tabs API (query, create, update, remove)
- WebRequest API (interceptor-based blocking/modifying)
- Runtime API (getManifest, getURL)
- Messaging (message bus between content scripts)
- Permissions model (host patterns, optional permissions)

### Lua Scripting (`src/lua/`)

Sandboxed Lua 5.4 environment via `mlua`. Available APIs: `aileron.version`, `aileron.navigate`, `aileron.cmd.create`, `aileron.on`, `aileron.keymap.set`, `aileron.url.add_redirect`, `aileron.theme.set`, `aileron.info`, `aileron.log`, `aileron.warn`, `aileron.extensions.list`, `aileron.extensions.info`, `aileron.extensions.reload`.

### Ad Blocking (`src/net/`)

Aho-Corasick multi-pattern matcher against EasyList-compatible filter lists. Supports network filters (domain blocking, exception rules, `$third-party`, `$popup`, `$redirect`, `$csp`, `$removeheader`, `$important`) and cosmetic filters (CSS/JS injection per domain).

### Sync Protocol (`src/sync/`)

Delta-based sync with E2EE via age encryption. Transports: Local filesystem and SSH (WebDAV planned). Core: chunk-based content-addressable manifest, delta detection, conflict flagging.

### ARP Protocol (`src/arp/`)

JSON-RPC over WebSocket protocol for mobile companion apps. Provides real-time tab state, quickmark access, clipboard sharing, and remote command execution.

## Module Map

| Module | Responsibility |
|--------|---------------|
| `src/app/` | Application state, command dispatch, event handling, pane/render management |
| `src/wm/` | BSP tree window manager, pane/tab data structures |
| `src/input/` | Keymap, keybinding registry, mode machine, event routing |
| `src/ui/` | egui panels (tabs, status bar, URL bar, palette, search) |
| `src/config.rs` | Configuration loading, theme management, feature flags |
| `src/servo/` | Wry pane manager, offscreen rendering, engine selection |
| `src/offscreen_webview.rs` | Offscreen webview capture, BGRA-to-RGBA conversion |
| `src/extensions/` | MV3 manifest parsing, content scripts, storage, messaging |
| `src/lua/` | Lua sandbox, API bindings, keymap/hooks |
| `src/net/` | Ad blocking (Aho-Corasick + EasyList), privacy lists |
| `src/frame_tasks.rs` | Per-frame operations (wry events, adblock, sync, tab close) |
| `src/arp/` | ARP WebSocket server, JSON-RPC protocol |
| `src/sync/` | Sync core (delta, manifest, chunking), crypto (age), transports |
| `src/passwords/` | Bitwarden-compatible credential storage (keyring backend) |
| `src/profiling/` | Frame time profiling, adaptive quality, input latency tracking |
| `src/git.rs` | Git status polling ( porcelain v2 format) |
| `src/i18n/` | Internationalization (9 locales, runtime switching) |
| `src/terminal/` | Integrated terminal emulator (alacritty_terminal + portable-pty) |
| `src/gfx/` | GPU rendering state (wgpu surface, swap chain) |
| `src/popup/` | Popup window management |
| `src/scripts/` | Content script URL matching and injection |
| `src/debug_capturer.rs` | Crash report capture for post-mortem analysis |
| `src/platform/` | Platform-specific code (Linux X11/Wayland, macOS, Windows) |
| `src/bootstrap.rs` | Panic hook, environment diagnostics |

## Feature Flags

All feature flags are dependency-gated -- disabling them removes the transitive dependency entirely from `--no-default-features` builds:

| Flag | Removes | Adds |
|------|---------|-------|
| `terminal` | portable-pty, alacritty_terminal | ~15 call sites |
| `lua` | vendored Lua 5.4 C compilation | ~10 call sites |
| `mcp` | image, base64 | MCP bridge |
| `arp` | tokio-tungstenite | ARP server |
| `sync` | fastcdc, blake3, age, notify | Sync protocol |
| `passwords` | keyring | Password manager |

## Concurrency Model

Single-threaded winit event loop with `Arc<RwLock<>>` bridges for shared state. The wry WebView is `!Send + !Sync`, so all web operations go through an mpsc channel (`WryAction` queue) processed in the frame callback.

## Performance Budgets (v1.0.0 targets)

| Metric | Target |
|--------|--------|
| 1 pane @ 60 fps | frame time <= 16.67ms |
| 4 panes @ 30 fps | frame time <= 33.33ms |
| Cold start to first paint | < 2s |
| Warm start to first paint | < 500ms |
| Input latency p95 | < 33ms |
| Memory per web pane | < 100 MB RSS |
| Memory per terminal pane | < 10 MB RSS |
