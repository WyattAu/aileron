# Aileron — Version & State Tracking

## Current State
- **Phase:** v5 Dogfood & Ship (Track A: Stability + Track B: Polish)
- **Version:** 0.16.0
- **Status**: Stability hardening, polish, release engineering
- **Last Updated:** 2026-04-24
- **Test Count:** 806 lib tests
- **Zero clippy warnings**
- **13 legitimate unsafe blocks** (3 X11 FFI, 6 cairo/gtk, 4 GLib)
- **Release profile: LTO + strip + panic=abort**

## Implementation Phases

| Phase | Status | Description |
|-------|--------|-------------|
| A–E | Complete | 24 tasks: scaffolding, tiling, history, palette, Lua, adblock, MCP, passwords |
| F | Complete | Architecture B: offscreen webview rendering, 7 bug fixes |
| G | Complete | Native terminal (alacritty_terminal + portable_pty), TASK-G06, TASK-G07 |
| H | Complete | Make It Polished: devtools, new tab, better internal pages |
| I | Complete | Make It Distributable: SVG icon, desktop entry, flake.nix |
| I.1 | Complete | Privacy & security: HTTPS upgrade, tracking protection, DNT/GPC |
| I.2 | Complete | Settings GUI, download manager, browser import, session recovery |
| I.3 | Complete | Per-site settings, print, popup blocker, cookies, audio mute, themes, password manager, PDF viewer |
| I.4 | Complete | Enhanced content scripts (@run-at, @match-regexp) |
| J | Complete | Make It Actually Work: full scrolling, mouse wheel, clipboard |
| K | Complete | Test the Testable + Hardening: keymap extraction, action queue, TOCTOU fix |
| L | Complete | Pure action dispatch: ActionEffect enum, 30 dispatch tests |
| M | Complete | Daily Driver Minimum: find-in-page, URL bar editing, download handler, link hints |
| J.0 | Complete | Polish: nucleo search, smooth scroll, tab pinning, visual bell, Servo stub |
| K.1 | Complete | WebExtensions API trait definitions (6 traits, full type system) |
| K.2 | Complete | Cross-platform traits (PlatformOps), Linux/macOS/Windows impls, native dialogs, notifications |
| K.3 | Complete | Advanced ad blocking ($csp, $removeheader, $redirect) |
| K.4 | Complete | System keyring integration + save-on-submit observer, OAuth detection, multi-step flows |
| K.5 | Complete | Sync protocol spec (WebDAV, E2EE, CRDT conflict resolution) |
| K.6 | Complete | Frame time profiling, adaptive quality, lazy init, texture caching |
| K.7 | Complete | Servo evaluation, pane design, texture sharing, engine selection, compat overrides |
| K.8 | Complete | Accessibility (ARIA labels, keyboard nav, screen reader, focus management) |
| K.9 | Complete | i18n framework (32 strings, 9 locales, TOML translation files, :language command) |
| K.10 | Complete | CI/CD (GitHub Actions: Linux test, macOS/Windows check, fmt, clippy) |
| P | Complete | Settings completion: sync UI, theme picker, search engines, expanded :set commands |
| U | Complete | Keyboard nav, keybinding config, mode indicator, omnibox frecency, inline bookmarks, drag resize, tab swap, undo close, find-replace |
| V | Complete | Bookmarks panel, folders, reader mode, per-site settings, workspace cycling |
| W | Complete | Crash-reload, tab-unload LRU, adaptive framerate, startup optimization, GPU fallback, input latency tracker |
| X | Complete | MCP tools, extension API docs, Lua scripting guide, bookmark import UI |
| Y | Complete | README, help panel, config reference, landing page, v1.0 roadmap |
| v5 Track A | In Progress | Stability: navigation error detection, crash watchdog, keyup, popup blocker |
| v5 Track B | In Progress | Polish: new tab page, download progress, g<url> quick navigate |
| v5 Track C | Pending | Release: VERSION.md, CHANGELOG, AUR PKGBUILD, man page, desktop entry |

## Benchmark Results (criterion --quick)
| Benchmark | Time | Notes |
|-----------|------|-------|
| bsp_create | 137 ns | BSP tree creation |
| bsp_split_vertical | 406 ns | Vertical pane split |
| bsp_split_horizontal | 331 ns | Horizontal pane split |
| bsp_navigate_4pane_grid | 61 ns | 4-pane grid iteration |
| bsp_close | 890 ns | Pane close (cleanup) |
| bsp_resize | 60 ns | Pane resize |
| fuzzy_search_short | 42 µs | Nucleo pattern match (100 items) |
| fuzzy_search_long | 132 µs | Nucleo pattern match (100 items) |
| fuzzy_search_no_match | 18 µs | Nucleo no-match (100 items) |
| pane_state_create | 1.38 µs | Pane state creation |
| pane_state_navigate | 98 ns | Pane URL navigation |
| dispatch_all_actions | 524 ns | Dispatch 10 actions |
| filter_list_parse_easylist | 1.17 µs | EasyList filter parse |
| site_settings_url_match_exact | 673 ns | Exact URL pattern match |
| site_settings_url_match_wildcard | 495 ns | Wildcard pattern match |
| site_settings_url_match_regex | 8.36 µs | Regex URL pattern match |
| content_script_match_100 | 19 µs | 100 scripts URL match |
| adblock_check_allowed | 53 ns | Domain block check |
| dispatch_print_action | 14 ns | Single action dispatch |

## Binary Size
- **Release binary:** ~21 MB (stripped)
- **Target architecture:** x86_64 Linux
- **Total Rust code:** ~39,000 lines across 104 files

## Key Discoveries
1. wry cannot render to wgpu texture — always paints to its own native surface
2. wry `build_as_child` only supports X11 — GTK fallback for Wayland
3. wry `!Send + !Sync` — must live on main thread
4. wgpu `Backers::all()` crashes on Wayland via EGL — use `VULKAN` only
5. Nix `vulkan-loader` has no ICD files — system Vulkan at `/usr/lib`
6. MCP tools are `Send+Sync` but wry panes are `!Send+!Sync` — bridged via `Arc<RwLock<>>` + `mpsc`
7. Bitwarden CLI `search()` needed ID extraction — fixed with `VaultItem` struct
8. wry `PageLoadEvent` has no failure variant — error detection requires JS init script + IPC
9. wry `with_new_window_req_handler` takes `(String, NewWindowFeatures) -> NewWindowResponse`

## Architecture Decisions
- **ADR-001:** Servo dependency broken — use `WebEngine` trait abstraction, wry now
- **ADR-002:** Dual-engine strategy: wry now, Servo later
- **ADR-003:** GTK fallback for Wayland (standalone gtk::Window + gtk::Fixed)
- **ADR-004:** Native terminal emulator
- **ADR-005:** Architecture D — Hybrid Servo+WebKitGTK
- **ADR-006:** Error detection via JS init script + IPC (no wry load failure event)
- **ADR-007:** Crash watchdog via activity timestamp tracking on offscreen panes
