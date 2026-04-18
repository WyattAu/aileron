# Changelog

All notable changes to Aileron will be documented in this file.

## v0.10.0 (2026-04-18) — Phase N: Feature Completion

### Settings Page (N.1)
- Added engine_selection dropdown (auto/servo/webkit)
- Added language dropdown with native display names (9 languages)
- Added popup_blocker_enabled checkbox
- Added adblock_update_interval_hours number input
- Added adaptive_quality checkbox
- All fields wired to IPC config save handler

### Extension Content Script Injection (N.2)
- Implemented ExtensionContentScriptRegistry with URL matching
- Extension manifests' content_scripts are now registered on load
- Extension JS/CSS injected into matching pages (document_start + document_idle)
- AileronScriptingApi.register_content_scripts() fully implemented
- 11 new tests for registry, matching, dedup, loader integration

### Internal Pages (N.3)
- Added proper aileron://404 page (was silent redirect to welcome)
- Added aileron://terminal placeholder with keyboard shortcut info
- Unknown aileron:// URLs now show 404 with requested URL

### Stats
- 692 total tests (+11 from v0.9.0)
- Zero clippy warnings
- 6 production unwrap() calls (all provably safe)

## v0.9.0 (2026-04-18) — Phase M: Critical Bug Fixes

### Security & Correctness
- Fixed use-after-free UB in i18n locale override (AtomicPtr → RwLock, eliminated 25 unsafe blocks)
- Fixed ad-blocker exception filters never evaluated in should_block() — @@|| rules now work correctly
- Fixed MCP transport serde_json::to_string().unwrap() panics on unserializable data
- Replaced curl shell-out with attohttpc for filter list downloads (no command injection risk)

### Performance
- Added release profile: LTO (thin), strip, codegen-units=1, panic=abort
- Expected 15-25% binary size reduction in release builds

### Tests
- 3 new adblock exception filter tests
- Total: 641 lib tests + 40 integration/startup/offscreen = 681

### Stats
- 681 total tests (+43 from v0.8.1)
- Zero clippy warnings
- Zero unsafe blocks in production code

## v0.8.1 (2026-04-18) — Phase L continued

### Bug Fixes
- Fixed: config.devtools now actually controls webview devtools (was hardcoded to debug builds only)
- Fixed: custom_css is now injected into web pages on load (was stored but never applied)
- Fixed: adblock_update_interval_hours now triggers periodic filter list updates

### WebExtensions Wiring (L.8)
- Concrete AileronExtensionApi implementing all 6 WebExtensions traits
- ExtensionManager with directory scanning and manifest.json loading
- :extensions, :extension-load, :extension-info commands
- Extension loading on startup from data_dir/extensions/

### Dead Code Cleanup (L.5)
- Removed unused MCP tool state fields (4 structs)
- Removed unused ParsedFilter::Ignore variant
- Removed never-accessed PopupWindow.window field
- Changed AuthCredentials.password to Zeroizing<String> for consistency

### Test Coverage (L.6)
- 22 new tests for i18n/loader (10), workspace_restore (6), wm/pane (6)
- Total: 638 lib tests + 40 integration/startup/offscreen = 678

### Housekeeping (L.7)
- Trimmed tokio features from "full" to "rt-multi-thread,macros" (smaller binary)
- Added **/.lake/ to .gitignore
- Removed 5 unnecessary #[allow(dead_code)] annotations

### Hardening Audit (L.2-L.4)
- Database layer: already properly hardened (all unwraps in test code only)
- lua/api.rs: already properly hardened (63/64 unwraps in test code)
- wm/tree.rs: already properly hardened (all unwraps in test code)

### Stats
- 678 total tests (+40 from v0.7.0)
- Zero clippy warnings
- Production unwrap() audit: db (0), lua (1 infallible), wm (1 infallible)

## v0.7.0 (2026-04-18) — Phase K Complete

Phase K is now 100% complete (42/42 tasks). This is the final planned development phase.

### Cross-Platform Abstraction (K.2, K.8, K.9 complete)
- PlatformOps trait with 13 methods for platform-specific operations
- LinuxPlatform: zenity/kdialog file dialogs, notify-send notifications
- MacOSPlatform: stub implementations (compiles, sidebar-right default, "Cmd" key)
- WindowsPlatform: stub implementations (compiles, native render mode, "Win" key)
- platform() factory function with cfg(target_os) dispatch
- GitHub Actions CI: Linux (test+clippy+fmt), macOS/Windows (compile-check)

### Servo Integration Architecture (K.7 complete)
- Servo embedder API evaluation spec (servo v0.1.0 LTS, OpenGL rendering, conditional go for Q3 2026)
- Servo pane architecture design spec (wgpu sharing strategies, thread model, migration path)
- Texture sharing infrastructure: ShareStrategy enum, TextureShareHandle, CpuReadback/DmaBuf/DirectWgpu
- ServoPane enhanced with texture share handle and resize support
- Engine selection: EngineSelection enum (auto/servo/webkit), select_engine() with domain lists
- :engine command to switch engines at runtime
- :compat-override command for per-site engine overrides
- Built-in WebKit override list (Google Docs, Meet, WhatsApp, Twitter/X)
- Built-in Servo prefer list (MDN, Rust-Lang, GitHub, StackOverflow)

### Stats
- 638 total tests (+50 from v0.6.0)
- 42/42 Phase K tasks complete (100%)
- Zero clippy warnings
- 816-line master plan, all tasks closed

## v0.6.0 (2026-04-18)

### Sync Protocol Design (K.5 complete)
- Complete sync protocol specification (.specs/02_architecture/sync_protocol_design.md)
- 7 sync collections with CRDT conflict resolution and delta sync
- Transport evaluation: WebDAV (recommended), Git, Custom HTTPS, Matrix, SQLite/SSH
- E2E encryption: Argon2id key derivation, XChaCha20-Poly1305, Ed25519 signing, BIP-39 recovery

### Performance Optimization (K.6 expanded)
- Adaptive quality rendering: auto-reduces texture capture rate when over 16.7ms budget
- Lazy pane initialization: background panes created one-per-frame, active pane prioritized
- Texture caching: reuse GPU textures via TextureHandle.set(), only reallocate on resize
- :adaptive-quality toggle command

### Enhanced Password Manager (K.4 expanded)
- Periodic form re-scan via MutationObserver (catches JS-rendered forms)
- OAuth/SSO detection: skips credential saving for Google, Microsoft, Facebook, Apple OAuth
- Multi-step login flow handling via sessionStorage
- Hidden form detection (display:none, visibility:hidden, offscreen positioning)

### Accessibility (K.8 complete)
- ARIA labels on all egui UI chrome via widget_info()
- Status bar, tab bar, URL bar, find bar, command palette all labeled
- Screen reader compatible (egui AccessKit integration)

### Internationalization (K.9 expanded)
- 9 locales: English, Chinese, Japanese, Korean, German, French, Spanish, Portuguese, Russian
- TOML translation files with compile-time embedding (include_str!)
- :language <code> command for runtime language switching
- :language-list command shows available languages
- Language preference persisted in config.toml

### Stats
- 588 total tests (+42 from v0.5.0)
- 33/42 Phase K tasks complete (78%)
- Zero clippy warnings

## v0.5.0 (2026-04-18)

### Advanced Ad Blocking (K.3 complete)
- $redirect filter rules with inline data URI stubs (1x1.gif, empty.css, empty.js)
- $badfilter detection (skip broken rules with warning)
- $important modifier (important rules override exceptions/whitelist)
- $generichide generic element hiding
- $document and $all resource type modifiers
- Peter Lowe's Ad & Tracking Server list as default
- Filter list update mechanism with ETag/304 conditional HTTP
- :adblock-update command

### Password Manager (K.4 complete)
- :credentials command lists Bitwarden vault items for current site
- :credentials-save saves pending form submission to system keyring
- Save-on-submit observer JS injected on page load
- Ctrl+Shift+K for credential search

### Performance & Monitoring (K.6)
- Frame time profiler: 1000-sample ring buffer with p50/p95/p99 stats
- :perf / :perf-on / :perf-off commands
- Dropped frame counter (frames exceeding 16.7ms budget)
- Memory monitoring via /proc/self/status
- :memory command shows RSS + per-pane estimates

### Internationalization (K.9 expanded)
- 29 UI strings externalized (was 7)
- register() helper for clean key registration
- Coverage: mode names, status messages, commands, errors

### Stats
- 546 total tests (+37 from v0.4.0)
- Zero clippy warnings

## v0.4.0 (2026-04-18)

### WebExtensions API (K.1)
- **Extension traits**: ExtensionApi, TabsApi, StorageApi, RuntimeApi, WebRequestApi, ScriptingApi
- **Manifest V3**: JSON parsing with permissions, content_scripts, background scripts
- **Full type system**: TabInfo, RequestFilter, BlockingResponse, InjectionTarget, etc.

### Advanced Ad Blocking (K.3)
- **$csp rules**: Content-Security-Policy header injection from filter lists
- **$removeheader rules**: Strip headers from requests
- **$redirect rules**: Resource redirection (parsed, not yet applied)
- **Block counter**: `[AB: N]` in status bar shows blocked requests per session

### Password Manager (K.4)
- **System keyring**: Store/retrieve credentials via OS keyring (GNOME Keyring/KWallet/Keychain)
- **Save-on-submit**: Form submission observer JS detects login forms
- **:keyring-test** command to verify keyring availability

### Cross-Platform Abstraction (K.2)
- **Platform module**: config_dir, data_dir, cache_dir, downloads_dir with per-OS cfg
- **OS detection**: is_wayland, is_x11, desktop_environment, os_name
- **Platform defaults**: macOS sidebar right, Windows native render mode
- **Refactored**: Config path construction uses platform module

### Internationalization (K.9)
- **i18n framework**: Locale detection, TrKey, tr()/tr_locale() static string table
- **Locale enum**: English (extensible)
- **OnceLock initialization**: Zero-cost after first access

### Performance (K.6)
- **Frame time profiling**: Logs frames exceeding 16.7ms budget

### Accessibility (K.8)
- **ARIA labels**: All internal pages (welcome, new tab, settings) have roles and labels
- **Keyboard navigation**: Settings form is fully keyboard-navigable
- **Screen reader**: aria-live regions for status updates

### Code Quality
- Fixed unsafe `set_var`/`remove_var` calls in tests

### Stats
- 469 total tests (+81 from v0.3.1)
- Zero clippy warnings
- ~18,500 lines of Rust

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
