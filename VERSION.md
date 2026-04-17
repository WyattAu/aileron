# Aileron — Version & State Tracking

## Current State
- **Phase:** v0.3.0 (Complete)
- **Version:** 0.3.0
- **Status:** Complete
- **Last Updated:** 2026-04-17
- **Test Count:** 386 lib + 26 integration + 13 startup + 1 offscreen = 426 total
- **Zero clippy warnings**

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

## Benchmark Results
| Benchmark | Time |
|-----------|------|
| bsp_create | ~1.5 µs |
| bsp_split_vertical | ~3 µs |
| bsp_split_horizontal | ~3 µs |
| fuzzy_search_short | ~15 µs |
| fuzzy_search_long | ~25 µs |
| pane_state_create | ~5 µs |
| dispatch_all_actions | ~2 µs |

## Key Discoveries
1. wry cannot render to wgpu texture — always paints to its own native surface
2. wry `build_as_child` only supports X11 — GTK fallback for Wayland
3. wry `!Send + !Sync` — must live on main thread
4. wgpu `Backers::all()` crashes on Wayland via EGL — use `VULKAN` only
5. Nix `vulkan-loader` has no ICD files — system Vulkan at `/usr/lib`
6. MCP tools are `Send+Sync` but wry panes are `!Send+!Sync` — bridged via `Arc<RwLock<>>` + `mpsc`
7. Bitwarden CLI `search()` needed ID extraction — fixed with `VaultItem` struct

## Architecture Decisions
- **ADR-001:** Servo dependency broken — use `WebEngine` trait abstraction, wry now
- **ADR-002:** Dual-engine strategy: wry now, Servo later
- **ADR-003:** GTK fallback for Wayland (standalone gtk::Window + gtk::Fixed)
- **ADR-004:** Native terminal emulator
- **ADR-005:** Architecture D — Hybrid Servo+WebKitGTK
