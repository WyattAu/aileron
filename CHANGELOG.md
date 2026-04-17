# Changelog

All notable changes to Aileron will be documented in this file.

## v0.3.1 (2026-04-18)

### Search
- **Nucleo fuzzy search** — replaced substring matcher with nucleo pattern-based fuzzy matching for better command palette and URL bar results

### Scrolling
- **Smooth scrolling** — keyboard scrolls (j/k, Ctrl+D/U, gg/G) now use CSS smooth behavior; mouse wheel remains instant

### Tab Management
- **Tab pinning** — `Ctrl+Shift+P` or `:pin` to pin/unpin panes; pinned panes cannot be accidentally closed; 📌 indicator in sidebar

### Terminal
- **Visual bell** — terminal bell triggers a 200ms white flash overlay instead of audio

### Privacy & Settings
- **Per-site zoom on page load** — zoom override from site_settings DB now applied automatically when pages load

### Usability
- **Middle-click link following** — middle-click on web panes opens link under cursor in new tab
- **Did-you-mean suggestions** — unknown commands suggest closest match via Levenshtein distance (e.g., "Unknown command: qit (did you mean :quit?)")

### Architecture D Preparation
- **ServoPane skeleton** — stub implementation of PaneRenderer trait for future Servo integration
- **EngineType enum** — `WebKit`/`Servo` on PaneState for per-pane engine tracking
- **`:engine` command** — query and plan engine selection

### Code Quality
- Fixed 3 concerning `unwrap()` calls with safe early-return patterns
- Updated welcome page with all current keybindings and commands

### Stats
- 428 total tests (388 lib + 26 integration + 13 startup + 1 offscreen)
- Zero clippy warnings

## v0.3.0 (2026-04-17)

### Native Terminal (Phase G)
- Native Rust terminal using alacritty_terminal + portable_pty
- ~1-2ms keystroke latency, ~2-5MB per pane
- 256-color ANSI, mouse selection, clipboard copy
- Dirty-region rendering optimization

### Architecture B (Phase F)
- Offscreen webview rendering via GTK OffscreenWindow
- CPU readback → wgpu texture → egui Image widget
- 7 critical/medium bug fixes

### Privacy & Security (Phase I.1)
- Hardened ad blocking with EasyList parser
- HTTPS upgrade + tracking protection
- DNT/GPC headers, referrer policy

### Settings & UI (Phase I.2)
- Settings GUI (aileron://settings)
- Download manager with progress
- Browser import (Firefox/Chrome)
- Session auto-complete with crash recovery

### Per-Site & Advanced (Phase I.3-I.4)
- Per-site settings (zoom, adblock, JS, cookies, autoplay)
- Print support (:print)
- Popup blocker
- Cookie management
- Tab audio mute
- Theme system (7 built-in themes + custom TOML)
- Enhanced content scripts (@run-at, @match-regexp)

### Password Manager (Phase I.3)
- Login form auto-detection
- URL-based credential search
- :bw-autofill and :bw-detect commands

### PDF Viewer
- :pdf command for system PDF viewer

### New Commands
`:print`, `:pdf`, `:settings`, `:import-firefox`, `:import-chrome`, `:mute`, `:unmute`, `:popup-block`, `:cookies-manage`, `:site-settings`, `:theme`, `:bw-autofill`, `:bw-detect`, `:https-upgrade`, `:tracking-protect`

### Stats
- 426 total tests (386 lib + 26 integration + 13 startup + 1 offscreen)
- 16,423 lines of Rust
- Zero clippy warnings

## v0.2.0 (2026-04-15)

### Architecture
- **PaneRenderer trait** — clean abstraction for rendering backends; WryPane implements it, making future engine swaps (Servo, etc.) trivial
- **PaneState** — renamed from PlaceholderEngine; honest naming for per-pane URL/title metadata tracker

### Daily-Driver Hardening
- **Auto-save workspace** — saves layout to `_autosave` every 30s for crash recovery
- **Auto-restore on startup** — when `restore_session = true`, prefers `_autosave` for crash recovery
- **Omnibox URL bar** — fuzzy search across bookmarks, history, and search engines with dropdown
- **Error recovery** — `aileron://error` protocol page; pane failures don't crash the app
- **Config migration** — `config_version` field; old configs auto-upgrade on load

### Content Modes
- **Reader mode** (`Ctrl+Shift+R`, `:reader`) — strips CSS, extracts article text, dark reading view
- **Minimal mode** (`Ctrl+Shift+M`, `:minimal`) — hides images/media, removes scripts

### Developer Tools
- **Network request log** (`Ctrl+Shift+N`, `:network`) — intercepts fetch/XHR, shows method + URL + status
- **Console capture** (`Ctrl+Shift+J`, `:console`) — captures console.log/warn/error output
- **Proxy support** — `proxy = "socks5://..."` in config, `:proxy <url>` command

### Content Scripts
- **Lua content scripts** — `.lua` files in `~/.config/aileron/scripts/` with `@match` URL patterns
- **Greasemonkey-compatible metadata** — `==UserScript==` blocks with `@name`, `@match`, `@grant`

### Window Management
- **Detach pane** (`Ctrl+Shift+D`) — move current pane to a standalone popup window
- **Close others** (`:only`) — close all panes except current

### Navigation
- **Multiple search engines** — `:engine google|ddg|gh|yt|wiki` to quick-switch
- **Nav commands** — `:back`, `:forward`, `:reload` ex-commands
- **Scroll restore** — scroll position preserved on back/forward navigation

### Privacy
- **Cookie management** — `:cookies-clear` and `:clear cookies` per pane
- **Clear browsing data** — `:clear history|bookmarks|workspaces|cookies|all`
- **Download history** — `:downloads` and `:downloads-clear` commands

### New Commands
`:engine`, `:back`, `:forward`, `:reload`, `:only`, `:reader`, `:minimal`, `:network`, `:network-clear`, `:console`, `:console-clear`, `:scripts`, `:downloads`, `:downloads-clear`, `:cookies-clear`, `:inspect`, `:proxy`, `:config-save`, `:clear`

### Stats
- 307 unit tests + 26 integration tests = 333 total
- Zero clippy warnings

## [0.1.0-alpha] - 2026-04-14

### Added
- Tiling window manager with horizontal/vertical splits
- Keyboard-driven navigation (vim-style: hjkl, gg/G, Ctrl+D/U)
- Embedded terminal pane (xterm.js + PTY via portable-pty)
- File browser with dark theme and keyboard navigation
- Git branch/status indicator in status bar
- Configurable search engine (default: DuckDuckGo)
- Command palette (Ctrl+P) with fuzzy search
- Tab sidebar (default) and topbar layouts
- Quickmarks (`:m<a> <url>` to set, `:g<a>` to go)
- Pane resize (Ctrl+Alt+H/J/K/L)
- Zoom in/out/reset (Ctrl+=/-/0)
- URL copy to clipboard (y key)
- Shell command execution (`:! <cmd>`)
- Runtime config changes (`:set <key> <value>`)
- SSH quick-connect (`:ssh <host>`)
- Workspace save/restore (`:ws-save`, `:ws-load`, `:ws-list`)
- Session auto-restore on startup
- Lua scripting support (init.lua)
- MCP (Model Context Protocol) bridge
- Bitwarden password manager integration
- Link hints (vimium-style, f key)
- Find-in-page (Ctrl+F)
- Ad-blocking via filter lists
- URL redirect rules (Lua)
- Custom keybindings (Lua)
- New tab page with search bar and quick links
- Internal pages: welcome, file browser, terminal

### Key Bindings
- `i` — Insert mode | `Esc` — Normal mode | `:` — Command mode
- `Ctrl+P` — Command palette | `` ` `` — Terminal
- `Ctrl+W` — Split vertical | `Ctrl+S` — Split horizontal
- `Ctrl+H/J/K/L` — Navigate panes | `Ctrl+Alt+H/J/K/L` — Resize panes
- `j/k` — Scroll | `Ctrl+D/U` — Half page | `gg/G` — Top/bottom
- `H/L` — Back/forward | `r` — Reload | `Ctrl+B` — Bookmark
- `Ctrl+F` — Find | `f` — Link hints | `y` — Copy URL
- `Ctrl+=/-/0` — Zoom | `Ctrl+E` — External browser
- `Ctrl+T` — New tab | `F12` — DevTools

### Commands
- `:q` — Quit | `:vs` — Split vertical | `:sp` — Split horizontal
- `:files` — File browser | `:ssh <host>` — SSH connect
- `:! <cmd>` — Shell command | `:set <key> <val>` — Runtime config
- `:open <url>` — Navigate | `:m<a> <url>` — Set quickmark
- `:g<a>` — Go to quickmark | `:ws-save/load/list` — Workspaces

### Configuration
- `~/.config/aileron/config.toml` — see README for all options
- `tab_layout` — "sidebar" (default), "topbar", or "none"
- `search_engine` — URL template with `{query}` placeholder
- `homepage` — Default homepage URL
- `restore_session` — Auto-restore last workspace on startup

### Technical
- 306 tests (280 lib + 26 integration)
- Clippy-clean with `-D warnings`
- Nix-reproducible build
- CI via GitHub Actions

## [Unreleased]

### Added
- Initial R&D lifecycle infrastructure (.specs directory structure with 50+ specification files)
- VERSION.md state tracking
- Initial project scaffolding with Cargo.toml, flake.nix

### Phase 5: Prototype Implementation
- **TASK-001:** Module structure — `src/{lib,main,app}.rs` with `wm/`, `input/`, `db/` submodules
- **TASK-002/003:** winit window creation + wgpu surface + egui-wgpu-winit integration + event loop
- **TASK-007:** BSP tree data structure (`BspTree`, `BspNode`, `Rect`, `SplitDirection`, `Direction`)
  - `split()`, `close()`, `resize()`, `navigate()`, `panes()`, `get_rect()`
  - Axiom verification: `verify_coverage()` and `verify_non_overlapping()`
  - 12 unit tests (TV-BSP-001 through TV-BSP-008 coverage)
- **TASK-008:** Modal state machine (`Mode` enum: Normal/Insert/Command, `transition()` function)
  - 8 unit tests (mode transitions, rapid switching, determinism)
- **TASK-009:** Keybinding registry (`KeybindingRegistry` with HashMap-based lookup)
  - Default keybindings: j/k/h/l navigation, i for Insert, : for Command, q to close
  - Ctrl+w/v/s, Ctrl+e, Ctrl+p shortcuts
  - 6 unit tests (lookup, override, mode isolation)
- **TASK-011:** Input event router (`route_event()` function per DEF-MODE-003)
  - Normal→KeybindingHandler, Insert→Servo, Command→CommandPalette, mouse→Egui
  - 8 unit tests (routing correctness, total coverage property)
- **TASK-013:** SQLite database with history, bookmarks, and workspaces tables
  - `record_visit()` with upsert on URL, `recent_entries()`, `search()`
  - WAL mode, indexed queries
  - 5 unit tests (CRUD, search, ordering, deduplication)
- **AppState:** Application core with mode machine, action execution, command palette, DB integration
- **45 unit tests passing, 0 failures**

### Changed
- Removed broken `servo` and `servo_embedder_traits` git dependencies (CPR-001: Servo Embedder API not resolvable)
- Removed `adblock` crate (transitive `rmp-serde` version conflict)
- Updated dependency versions: wgpu 23.0.0, winit 0.30.8, egui 0.31.1

### Technical Debt
- Servo integration not yet implemented — needs WebEngine trait abstraction (ADR-001)
- egui rendering pass not yet wired into the main loop (compositor bridge pending)
- No actual Servo pane rendering (placeholder URLs: `aileron://new`, `aileron://welcome`)
- Command palette UI not yet rendered (state machine works, no egui overlay)
- Lua scripting not yet integrated
- MCP server not yet implemented
- Clippy: 5 minor warnings (redundant closures, collapsible ifs, `Copy` trait usage)
