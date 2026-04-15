# Aileron — Version & State Tracking

## Current State
- **Phase:** K (Complete) — Test the Testable + Hardening
- **Version:** 0.1.0-pre-alpha
- **Status:** In Progress — All core features working, scrolling functional, action queue hardened
- **Last Updated:** 2026-04-13T00:00:00Z
- **Current Error Level:** None
- **Test Count:** 224 passing

## Implementation Phases (beyond specification)

| Phase | Status | Description |
|-------|--------|-------------|
| A–E | ✅ Complete | 24 tasks: scaffolding, tiling, history, palette, Lua, adblock, MCP, passwords |
| F | ✅ Complete | Make It Launch: channel sharing, URL bar, resize, GTK fallback |
| G | ✅ Complete | Make It Useful: title tracking, history, back/forward/reload, Lua keymaps, config, bookmarks, MCP bridge, password manager |
| H | ✅ Complete | Make It Polished: devtools, new tab, better internal pages |
| I | ✅ Complete | Make It Distributable: SVG icon, desktop entry, flake.nix rewrite |
| J | ✅ Complete | Make It Actually Work: full scrolling, mouse wheel, clipboard |
| K | ✅ Complete | Test the Testable + Hardening: keymap extraction, action queue, TOCTOU fix |
| L | ✅ Complete | Pure action dispatch: ActionEffect enum, 30 dispatch tests, execute_action refactored |
| M | ✅ Complete | Daily Driver Minimum: find-in-page, URL bar editing, download handler, link hints |

## Phase J Details (Make It Actually Work)
| Task | Status | Description |
|------|--------|-------------|
| J-1 | ✅ | ScrollDown/Up via j/k (120px per press) |
| J-2 | ✅ | ScrollLeft/Right via h/l (120px per press) |
| J-3 | ✅ | HalfPageDown/Up via Ctrl+D/Ctrl+U |
| J-4 | ✅ | ScrollTop/Bottom via Ctrl+G/G |
| J-5 | ✅ | Mouse wheel forwarding to wry in Insert mode (LineDelta + PixelDelta) |
| J-6 | ✅ | Yank: copies selected text via JS `getSelection()` |
| J-7 | ✅ | Paste: triggers browser paste via JS `execCommand('paste')` |
| J-8 | ✅ | Fixed Ctrl+D conflict (HalfPageDown vs BookmarkToggle) — bookmark now Ctrl+B |
| J-9 | ✅ | Added `WryAction::RunJs(String)` for arbitrary JS execution |

## Phase K Details (Test the Testable + Hardening)
| Task | Status | Description |
|------|--------|-------------|
| K-1 | ✅ | Extracted `map_winit_key()` → `src/input/keymap.rs` (18 new tests: A-Z, F1-F12, arrows, fallback, layout independence) |
| K-2 | ✅ | Tests for `bsp_rect_to_wry_rect()` — 5 tests covering origin, clamping, zero/large bars |
| K-3 | ✅ | Tests for HTML page generators — 4 tests validating welcome + new tab pages |
| K-4 | ✅ | Tests for `db/mod.rs` schema init — 8 tests: table/column existence, UNIQUE constraint, idempotency |
| K-5 | ✅ | Fixed TOCTOU panic risk in `handle_raw_command()` (replaced `is_ok()`+`unwrap()` with `if let Ok`) |
| K-6 | ✅ | Replaced `Option<WryAction>` with `VecDeque<WryAction>` — no more silently dropped actions |
| K-7 | ✅ | Added `WryAction::RunJs(String)` for arbitrary JS execution from actions |
| K-8 | ✅ | Added `smol_str` direct dependency for winit Key::Character construction in tests |

## Phase L Details (Pure Action Dispatch)
| Task | Status | Description |
|------|--------|-------------|
| L-1 | ✅ | Designed `ActionEffect` enum — pure data describing what each action does (10 variants) |
| L-2 | ✅ | Extracted `dispatch_action()` free function — maps every `Action` → `Vec<ActionEffect>` with zero I/O |
| L-3 | ✅ | Refactored `execute_action()` to call `dispatch_action()` + apply effects (thin 50-line wrapper) |
| L-4 | ✅ | 30 tests for `dispatch_action()`: every action variant, WryAction parameters, status messages, mode changes, exhaustiveness |
| L-5 | ✅ | Converted `app.rs` → `app/mod.rs` + `app/dispatch.rs` module directory |
| L-6 | ✅ | Added `PartialEq` to `WryAction` for test assertions |

## Phase M Details (Daily Driver Minimum)
| Task | Status | Description |
|------|--------|-------------|
| M-1 | ✅ | Find-in-page: Ctrl+F opens egui bar, Enter/↓/↑ search via JS `window.find()`, Escape closes |
| M-2 | ✅ | URL bar editing: click URL in bottom bar to focus, type + Enter navigates, Escape unfocuses |
| M-3 | ✅ | Download handler: wry `download_started_handler` saves to `~/Downloads/` with filename extraction |
| M-4 | ✅ | Welcome page: fixed Ctrl+D→Ctrl+B, added Scroll/Find/Link hints to keybinding reference |
| M-5 | ✅ | Link hints (O-1): press `f` toggles numbered blue badges over clickable elements via JS injection |
| M-6 | ✅ | New actions: Find, FindNext, FindPrev, FindClose, ToggleLinkHints + 5 dispatch tests |

## Phase O Details (Developer Power Features — In Progress)
| Task | Status | Description |
|------|--------|-------------|
| O-1 | ✅ | Link hints toggle: `f` injects numbered badges via CSS+JS, second `f` removes |
| O-1b | ✅ | Link hints follow: digit input accumulates, prefix-matches, clicks on exact match, Escape cancels |
| O-2 | 🔲 | Lua cmd.create execution |
| O-3 | 🔲 | MCP tool expansion |
| O-4 | 🔲 | Workspace persistence |
| O-5 | 🔲 | URL transformation rules |

## Phase G Details (Make It Useful)
| Task | Status | Description |
|------|--------|-------------|
| G-1 | ✅ | Title tracking via `with_document_title_changed_handler` |
| G-2 | ✅ | History recording to SQLite on page load |
| G-3 | ✅ | Back/Forward/Reload via JS workaround (wry has no native API) |
| G-4 | ✅ | Lua init.lua with `aileron.keymap.set()` |
| G-5 | ✅ | Config file at `~/.config/aileron/config.toml` |
| G-6 | ✅ | MCP tools wired to wry panes via bridge (state + command channel) |
| G-7 | ✅ | Password manager: `bw-unlock`, `bw-search`, `bw-lock`, credential autofill |
| G-8 | ✅ | Bookmarks: CRUD, Ctrl+B toggle, palette integration |

## Phase H Details (Make It Polished)
| Task | Status | Description |
|------|--------|-------------|
| H-1 | ✅ | Devtools enabled in debug builds |
| H-2 | ✅ | `aileron://` protocol URL-aware (welcome vs new tab pages) |
| H-3 | ✅ | F12 devtools toggle, Ctrl+T new tab |

## Key Discoveries
1. wry cannot render to wgpu texture — always paints to its own native surface
2. wry `build_as_child` only supports X11 — GTK fallback for Wayland
3. wry `!Send + !Sync` — must live on main thread
4. wgpu `Backends::all()` crashes on Wayland via EGL — use `VULKAN` only
5. Nix `vulkan-loader` has no ICD files — system Vulkan at `/usr/lib`
6. MCP tools are `Send+Sync` but wry panes are `!Send+!Sync` — bridged via `Arc<RwLock<>>` + `mpsc`
7. Bitwarden CLI `search()` needed ID extraction — fixed with `VaultItem` struct

## Architecture Decisions
- **ADR-001:** Servo dependency broken — use `WebEngine` trait abstraction, wry now
- **ADR-002:** Dual-engine strategy: wry now, Servo later
- **ADR-003:** GTK fallback for Wayland (standalone gtk::Window + gtk::Fixed)
