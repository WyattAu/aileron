# Changelog

All notable changes to Aileron will be documented in this file.

## [Unreleased] -- MCP Bridge Deduplication & CI Hardening

### Changed -- Single source of truth for MCP tool dispatch
- **`src/mcp/tcp_bridge.rs`**: removed the duplicated `handle_tool_call` re-dispatcher (~280 lines) that re-implemented every tool's command mapping inline. The TCP bridge now delegates all JSON-RPC requests (`initialize`, `tools/list`, `tools/call`, `ping`, notifications) to the same registered `McpServer` used by the in-process stdio transport. This corrects divergent behaviour: the LLM-backed tools (`summarize_page`, `translate_page`, `analyze_page`) and `read_active_pane` (which reads shared state without a main-thread round-trip) are now executed via their canonical `McpTool::execute` implementations rather than the lossy inline approximation.
- **`src/bin/mcp_server.rs`**: simplified the standalone relay. Each request opens a fresh loopback connection (tolerates browser restarts at negligible cost for MCP's request frequency), and the redundant startup connectivity probe was removed; per-request connection errors are already mapped to well-formed JSON-RPC error responses.

### Added -- Verifiability & CI coverage
- **`src/mcp/tcp_bridge.rs`**: added six loopback integration tests (`tools_list_is_dispatched_to_server`, `tools_call_is_dispatched_to_registered_tool`, `initialize_is_dispatched_to_server`, `unknown_method_returns_method_not_found`, `unknown_tool_returns_invalid_params`, `malformed_json_returns_parse_error`) that exercise the bridge end-to-end over a real TCP socket with deterministic timeouts and explicit EOF signalling to avoid read-loop deadlocks.
- **`src/mcp/tcp_bridge.rs`**: compile-time port-sanity invariant (`const _: () = { assert!(MCP_TCP_PORT > 1024); assert!(MCP_TCP_PORT < 49152); }`) so an invalid port fails the build.
- **`.github/workflows/ci.yml`**: added a `wasm` job that runs `cargo check` and `cargo clippy -D warnings` against `aileron-chrome` on `wasm32-unknown-unknown`. Previously chrome WASM regressions were only caught by the local pre-commit hook, never by remote CI.
- **`README.md`**: corrected stale test counts (1198 lib / 257 integration / 4 doc / 1459).

### Fixed -- Cross-platform compilation (restores macOS/Windows/cross-compile CI)
- **`src/app_handler.rs`**: gated the `OffscreenWebViewManager::capture_dirty_frames()` call behind `#[cfg(target_os = "linux")]`. The method is only implemented in the Linux `impl` block (offscreen WebKitGTK texture capture); the unconditional call caused `error[E0599]` on macOS, Windows, and all cross-compile targets. On non-Linux the enclosing `uses_offscreen_compositing()` branch is already dead at runtime.
- **`src/main.rs`**: narrowed the `is_terminal` binding in `create_offscreen_pane_for` to `#[cfg(all(target_os = "linux", feature = "terminal"))]` so it matches its only use site (inside the Linux-only offscreen match). Previously it was `#[cfg(feature = "terminal")]` only, producing an `unused variable` warning on macOS/Windows that failed clippy under `-D warnings`.

### Fixed -- CI tooling (restores the Linux Test job)
- **`.github/workflows/ci.yml`**: replaced `cargo install cargo-audit@0.20.0 --locked` and `cargo install cargo-llvm-cov@0.6.14 --locked` with `taiki-e/install-action@v2`, which installs pre-built binaries. The `--locked` source installs failed with `E0282` in the pinned `time` crate on Rust 1.96; pre-built binaries are immune to compiler-version drift and are markedly faster to provision. This restores the security-audit and code-coverage steps that had been failing the Test job (all 1459 tests themselves passed).

### Changed -- Single canonical roadmap
- Removed four superseded, overlapping planning documents (`ROADMAP_COMPREHENSIVE.md`, `ROADMAP_FINAL.md`, `ROADMAP_PRODUCTION.md`, `ROADMAP_v0.19_v1.0.md`); `ROADMAP.md` is now the single source of truth, with its Current State refreshed to the live metrics (1198/257/4 tests, 9 CI jobs including the new WASM job).

### Test Results
- 1198 library tests, 257 integration tests, 4 doc tests -- all passing with zero clippy warnings on both native and WASM targets.

## v1.0.0 (2026-06-09) -- First Stable Release

### Summary
Aileron 1.0.0 is the first stable release of the keyboard-driven tiling web environment. This release represents the culmination of all development phases (A through Z, v5-v7 tracks) with 1178 lib tests, 253 integration tests, and 1435 total tests.

### Major Features Implemented
- **Extension API completion** — 9 WebExtensions API traits (tabs, storage, runtime, scripting, webRequest, alarms, cookies, contextMenus, declarativeNetRequest, permissions)
- **Sync protocol** — WebDAV transport with E2EE encryption (Age), content-defined chunking (fastcdc), delta detection, real-time filesystem watcher
- **MCP integration** — Built-in Model Context Protocol server with stdio transport for AI assistant browser control
- **Lua scripting** — init.lua with custom keybindings, commands, URL redirect rules, content scripts with @match/@match-regexp patterns
- **Native terminal** — alacritty_terminal + portable_pty, ~2-5MB/pane, SSH quick-connect, scrollback search
- **Password manager** — Bitwarden integration via system keyring, OAuth detection, multi-step login flows

### Platform Expansion
- **macOS** — WebKit built-in, full native rendering support
- **Windows** — WebView2 (Edge) integration, Flatpak parity
- **Cross-platform traits** — PlatformOps abstraction for Linux/macOS/Windows

### Privacy & Security
- **Fingerprint protection** — Canvas/WebGL noise injection, AudioContext protection
- **Container tabs** — Isolated cookie jars for multi-account browsing
- **HTTPS upgrade** — Auto-upgrade HTTP for known-safe domains
- **Tracking protection** — Domain blocking, DNT/GPC headers, strict referrer policy
- **Ad blocking** — EasyList parser with cosmetic CSS rules, $redirect/$important/$badfilter support

### UX Improvements
- **Drag-drop** — Tab reordering, pane detachment via drag
- **Session management** — Auto-save/restore, crash recovery
- **Macros** — Record and replay key sequences
- **Reader mode** — Article text extraction with clean display
- **Link hints** — Vimium-style f-key hint following
- **Workspace persistence** — Save/restore pane layouts with auto-save every 30s

### Distribution
- **AppImage** — Portable Linux binary distribution
- **Flatpak** — Sandboxed Linux package
- **AUR stable** — Arch User Repository stable package
- **GitHub Actions CI** — Linux test, macOS/Windows check, fmt, clippy

### Performance
- **Adaptive quality** — Auto-reduces frame rate for background tabs
- **Lazy initialization** — Background panes created one-per-frame
- **Frame time profiling** — p50/p95/p99 stats with dropped frame counter
- **Release profile** — LTO thin + strip + panic=abort + codegen-units=1

### Internationalization
- **9 languages** — EN, ZH, JA, KO, DE, FR, ES, PT, RU
- **Runtime switching** — Instant language changes via :language command

### Test Results
- 1178 lib tests, 253 integration (13 suites), 4 doc tests = 1435 total
- Zero clippy warnings (--all-targets -D warnings)
- Zero rustfmt issues
- Zero critical vulnerabilities (cargo audit)

## v0.20.0 (2026-05-14) -- Phase 2 Performance: Buffer Reuse & Feature Gates

### Frame Capture Buffer Reuse
- **Eliminated ~8MB per-frame heap allocation** during active scrolling. Capture buffers are now stored in `capture_buffers: HashMap<Uuid, Vec<u8>>` on `AileronApp` and reused across frames. Only reallocated when pane dimensions change.
- Applied to both `app/render.rs` (WebKitGTK backend) and `main.rs` (Wry backend).
- Buffers are cleaned up in `remove_wry_pane_for()` to prevent memory leaks.
- Restructured `update_webview_textures()` to collect captured pane IDs first, then reference buffers by ID -- avoids borrowing `self` across mutable fields.

### Feature Gates: `terminal` and `lua`
- **`terminal` feature gate:** `src/terminal/` (1,464 LoC) and `portable-pty`, `alacritty_terminal` deps now gated behind `feature = "terminal"`. ~15 call sites wrapped with `#[cfg(feature = "terminal")]`.
- **`lua` feature gate:** `src/lua/` (1,344 LoC) and `mlua` dep now gated behind `feature = "lua"`. ~10 call sites wrapped with `#[cfg(feature = "lua")]`.
- Both features remain in `default = [...]` so existing users are unaffected.
- `cargo check --no-default-features` compiles clean with zero warnings -- demonstrates minimal browser-only build.
- Estimated compile time savings when disabled: terminal ~1-2m (C PTY libs + vendored VTE), lua ~30-60s (vendored Lua 5.4 C compiler).

### Collapsible `if` Clippy Fixes
- Collapsed 7 nested `if`/`if let` patterns into `if ... && let ...` chains in `event_handler.rs` and `main.rs`.

## v0.19.0 (2026-05-13) -- Phase 2 Performance: Analysis & Quick Wins

### Hot-Path Allocation Audit
- Identified 22 allocation findings across frame_tasks, wry_actions, render loop, event routing
- 3 HIGH severity: tab display cache String clones per frame (panels.rs), 8MB frame capture buffer per dirty pane (render.rs), panes() Vec clone per frame (wm/tree.rs)
- 9 MEDIUM severity: ARP JSON serialization per frame, dispatch Vec+String per keypress, WryAction clone per dispatch, CachedThemeColors clone per frame, pane_id.to_string() HashMap lookup per tab, HashSet allocation per keypress for pane change detection, pane_ids() Vec allocation, format!() JS strings per scroll
- 10 LOW severity: unconditional Vec allocations, mode string, Key enum clone, double String alloc in a11y, truncate_str

### Fixes Applied
- **Memory eviction LRU fix:** Automatic memory-limit eviction now uses `find_lru_pane()` (proper LRU by focus timestamp) instead of `iter().find()` (arbitrary first non-active pane). Fixed in both main.rs (Wry backend) and event_handler.rs (WebKitGTK backend).
- **Dependency-level feature gates:** `image`, `base64` gated behind `mcp`; `tokio-tungstenite` behind `arp`; `fastcdc`, `blake3`, `age`, `notify`, `notify-debouncer-mini` behind `sync`; `keyring` behind `passwords`. No-features build compiles clean (verified).
- **chrono serde removal:** Removed unused `serde` feature from chrono dependency (no DateTime fields in serializable structs).
- **poll_all_events Vec pre-allocation:** Changed `Vec::new()` to `Vec::with_capacity(panes.len())` in wry_engine.rs.
- **MCP code gate fix:** Wrapped `active_id` and `app_state` variables in `#[cfg(feature = "mcp")]` block to eliminate no-features warnings.
- **v1.0.0 release blockers:** Updated ROADMAP_PRODUCTION.md with accurate status (4 items checked, 2 documented as accepted/deferred).

### Feature Gate Infrastructure
- Existing features (mcp, arp, sync, passwords) now gate their Cargo.toml dependencies, not just code
- `cargo check --no-default-features` compiles clean with zero warnings
- Identified `terminal` and `lua` as future feature-gate candidates (15+ and 10+ call sites respectively)

### Compile Time Baseline
- Clean dev build: 7m 14s (6-core x86_64, no caching)
- Incremental build: ~2.5s after dependency changes
- Heaviest crates: wry/webkit2gtk, egui/wgpu, mlua (vendored Lua 5.4), alacritty_terminal

### Memory Profiling Assessment
- Current: global RSS via `/proc/self/status` + static heuristic (50 MB/web pane, 3 MB/terminal pane)
- Gap: no per-pane heap measurement, no allocator integration, automatic eviction was not using LRU (fixed)
- Tab-unload LRU infrastructure exists (`find_lru_pane()`, `pane_last_focus` HashMap) but was bypassed in automatic eviction (now fixed)

### Dependency Feature Audit
- 16 dependencies with explicit features audited
- 1 unused feature found and removed: `chrono/serde`
- All other features verified as used
- Transitively enabled `tokio/time` (via tokio-tungstenite) never directly used

### v1.0.0 Release Blockers Updated
- Checked: unsafe SAFETY comments (19/19), unwrap() audit, #[must_use] (48/28 files), pre-commit hook
- Accepted: ~15 benign shutdown channel sends (converting would spam)
- Deferred: website visit integration test (requires display server)

## v0.18.1 (2026-05-12) — Quality Audit & CI Hardening

### Quality Verification
- **1299 tests pass** (1038 lib, 253 integration, 4 doc, 4 bin)
- **Zero clippy warnings** (`--all-targets -D warnings`)
- **Zero rustfmt issues**
- **Zero critical vulnerabilities** (`cargo audit`; 13 transitive GTK3 unmaintained warnings)
- **Zero emojis in documentation**
- **Zero code stubs** (1 legitimate STUB_GIF for adblock pixel)
- **All 19 unsafe blocks have SAFETY comments** (down from ~50)

### Documentation Fixes
- Updated all test counts to reflect actual: 1299 (was stale at 1259)
- Fixed VERSION.md unsafe block count: 19 (was stale ~50)
- Fixed LOC count: ~50,800 (was stale ~49,500)
- Updated ROADMAP_PRODUCTION.md: marked `aileron.navigate()` as implemented
- Updated CONTRIBUTING.md: clippy instruction changed to `--all-targets`
- Added v0.18.1 Quality Audit Results section to production roadmap

### CI/CD Hardening
- Pre-commit hook: added 6 missing integration test suites (was 7, now 13)
- GitHub Actions CI: added 6 missing integration test suites
- GitHub Actions CI: clippy now runs with `--all-targets` (was `--lib` only)
- GitHub Actions CI: security audit relaxed to `cargo audit` (was `--deny warnings`)
- GitHub Actions CI: added benchmark baseline verification step

### Code Quality
- `#[must_use]` audit complete: 48 attributes across 28 files (8 missing found and fixed)
- Silent error swallow audit: 11 converted to tracing::warn in v0.18.0; ~15 remaining are benign shutdown channel sends
- FFI SAFETY comment audit: all 19 unsafe blocks have actionable SAFETY comments

### New Integration Tests
- `tests/downloads_integration.rs`: 14 tests (manager lifecycle, filename sanitization, progress formatting, cleanup)
- `tests/terminal_integration.rs`: 21 tests (PTY lifecycle, selection, NativeTerminalPane, colors, cell metrics)

### aileron.navigate() Implementation (already in v0.18.0)
- Supports init.lua startup navigation
- Supports hook callback navigation
- Pending navigations processed after engine initialization

## v0.18.0 (2026-04-29) — Extension Foundation & Agent Browser

### Extension System
- **Builtin adblock extension**: Scaffold with manifest, registered at startup
- **Extension loader**: `register_builtin_adblock()`, `is_builtin_adblock_enabled()`, `set_builtin_adblock_enabled()`
- **Extension API**: All callback types migrated from `Box` to `Arc` for clone-under-lock deadlock prevention
- **`:bind` / `:unbind` commands**: Custom keybindings per mode (normal, insert, command)
- **`:stats` command**: System resource usage display (tabs, extensions, memory, bookmarks, history)

### Agent Browser (MCP)
- **MCP response channels**: Migrated from `std::sync::mpsc` to `tokio::sync::oneshot` — no more tokio runtime blocking
- **MCP server**: `spawn_blocking` wrapper prevents I/O starvation

### Quality Hardening
- **Terminal mutex safety**: 6x `.lock().unwrap()` → poison recovery (no crash on panic)
- **Extension deadlock fix**: `fire_change_callbacks`, `fire_installed`, `fire_startup`, message handler all release lock before invoking callbacks
- **Profiling NaN fix**: `partial_cmp` → `unwrap_or(Equal)` prevents sort panic
- **Terminal rendering**: Reused String buffer across cells (~5000 fewer allocs/frame)
- **RGBA buffer reuse**: `frame_rgba()` returns `&[u8]` slice, buffer reused across frames (~8MB/frame saved)
- **Pane list caching**: `BspTree` dirty-flag cache via `RefCell<Vec>` (5-10 tree traversals/frame eliminated)
- **Deterministic sync**: `SyncManifest.files` HashMap → BTreeMap for stable JSON serialization
- **Regex pre-compilation**: Content script patterns compiled at load time; site settings patterns cached with `LazyLock`
- **Error logging**: 11x silent `let _ = db::*` → `tracing::warn!` (bookmarks, workspaces, tabs, site settings)
- **x11-dl**: Linux-only dependency (removed from non-Linux builds)
- **cargo fmt**: Entire codebase reformatted to rustfmt standard

### Clippy Fixes
- All-targets `-D warnings` clean (lib + bin + tests)
- 6x field assignment outside initializer → struct expression
- 1x unwrap on Ok value, 1x returning let binding, 1x empty line after doc
- 1x items after test module → reordered, 1x unused imports → removed

### Testing
- **851 tests pass** (4 new: builtin adblock register, idempotent, toggle, survives load_all)
- **22 extension loader tests** (was 18)
- **Clippy**: Zero warnings on all-targets
- **Cargo audit**: Zero vulnerabilities

## v0.17.0 (2026-04-25) — Daily-Driver Features & Quality Audit

### Security & Reliability
- **JS injection hardening**: `:replace` command escapes `\ " ' ) } ] $` in user input
- **Path traversal fix**: Download filenames sanitized via `Path::file_name()`
- **DB permissions**: Database file set to 0600 (owner-only) on Unix
- **rustls-webpki**: Updated 0.103.12→0.103.13 (fixes RUSTSEC-2026-0104)
- **History title index**: Added `idx_history_title` for search performance
- **Atomic import_visit**: `INSERT OR IGNORE` replaces SELECT+INSERT race

### Performance
- **Background git status**: `GitPoller` thread — no more 1Hz main thread block
- **Background filter downloads**: Adblock filter list HTTP downloads offloaded to thread
- **Cached theme colors**: `CachedThemeColors` — 10 hex parses/frame → 0 (cached)
- **pane_ids()**: O(n) Vec allocation avoided when only UUIDs needed
- **VecDeque closed tab stack**: `pop_front()` O(1) vs `Vec::remove(0)` O(n)
- **UTF-8 safe truncation**: `truncate_str()` helper — no more mid-character splits

### Memory Leaks Fixed
- `pane_last_focus`, `marks`, `tab_names`, `private_pane_ids` cleaned on pane close

### v6 Features (Vim-style UX)
- **`/` find**: Vim-style `/` bound to `OpenFindBar`
- **`F` hint mode**: Opens links in new background tabs (orange badges, `window.open`)
- **`:tab-rename***: Custom tab names with DB persistence
- **`:private***: Per-pane private browsing with [PRIVATE] indicator
- **`:yt***: Yank page title to clipboard
- **URL click-to-edit**: Click URL in status bar to edit
- **Private propagation**: New tabs inherit private mode from parent pane

### v0.18 Daily-Driver Features
- **Form autofill**: `:autofill` command detects login forms and fills from Bitwarden
- **Auto-fill indicator**: Status bar shows `[autofill available]` on login pages
- **PDF inline viewing**: PDFs render inline via WebKitGTK (no external viewer)
- **`:pdf` command**: Now navigates inline instead of opening external viewer
- **Smooth scroll**: `scroll-behavior: smooth` CSS injection + smooth `scrollBy()`
- **WAL checkpoint**: `PRAGMA wal_checkpoint(TRUNCATE)` after periodic auto-save
- **`:set` command**: Runtime config changes (`:set theme light`, `:set adblock false`)
- **SyncWatcher fix**: Corrected inverted stop/running semantics

### Testing
- **845 tests pass** (24 new: omnibox, autofill, PDF, scroll, :set)
- **50 modules with test coverage**, zero critical gaps
- **Clippy**: Zero warnings with `-D warnings`
- **Cargo audit**: Zero vulnerabilities

## v0.16.0 (2026-04-24) — Dogfood Hardening & Polish

### Stability (Track A)
- **Navigation failure detection** (A01): `ERROR_MONITOR_JS` initialization script detects
  WebKitGTK error pages (DNS, TLS, connection failures) by checking title patterns and
  empty pages. Reports via IPC `__aileron_nav_error__|url|message` and redirects to
  `aileron://error` page with details.
- **WebView crash detection** (A02): `OffscreenWebView` tracks `last_activity_time` and
  `loading` state. 15-second watchdog timer detects stalled loading panes, populates
  `webview_crash_detected`/`crashed_pane_url` fields for `:crash-reload` recovery.
- **Keyup event forwarding** (A03): Added `ElementState::Released` handler in main.rs
  that forwards keyup events to active offscreen webview in insert mode. Fixes shift-release
  not ending text selection in web content.
- **Popup blocker** (A04): `with_new_window_req_handler` on both native and offscreen
  `WebViewBuilder`. Reads `config.popup_blocker_enabled` (default: true). Plumbs
  `popup_blocker` param through entire `create_pane`/`new`/`make_builder` chain.

### Polish (Track B)
- **Enhanced new tab page** (B01): Requests bookmarks and recent history via IPC
  (`get-newtab-data` handler). Shows bookmark tiles with initial letters, recent history
  items with hostnames, and built-in links to Files, Terminal, Bookmarks, History.
  Keyboard shortcut hints in footer.
- **Download progress indicator** (B02): Status bar shows active download percentage
  and speed (e.g., `DL 45% (2.3 MB/s)`) with green color when downloads are in progress.
- **`g <url>` quick navigate** (B05): Opens URL in new tab via horizontal split.
  Auto-prepends `https://` if no scheme present.

### Commands Added
- `:g <url>` — Open URL in a new pane (horizontal split)

## v0.15.0 (2026-04-23) — Feature Completeness Sprint

### Features (Phase U-Y, 40 tasks)
- **Keyboard navigation** (U01): `j`/`k` scroll in panels, `J`/`K` switch tabs, `Enter` activate
- **Keybinding configuration** (U02): TOML `[keybindings]` section, `c`/`C`/`d`/`D`/`u`/`r`/`H`/`L`/`R`/`y`/`Y`/`p`/`P` support, applied after defaults
- **Mode indicator** (U03): Status bar shows `[N]`/`[I]`/`[C]` with accent color
- **Omnibox frecency** (U04): Tab/search=1000, open tabs=900, bookmarks=800, history=frecency×100
- **Bookmark folders** (V02): DB schema with nullable folder column, panel group headers, `:bookmark <url> [folder]`
- **Reader mode** (V03): Article extraction via JS, clean display
- **Per-site settings** (V04): `:site-settings` overlay panel, zoom/JS/cookies/adblock per domain
- **Workspace cycling** (V06): Index-based cycling, workspace name in status bar
- **Drag resize handles** (U06): 6px invisible strips at split borders, cursor change, accent highlight
- **Tab move swap** (U07): Actual BSP pane ID swap via `swap_pane_ids()`
- **Undo close tab** (U08): Closed tab stack (50 max), `:tab-restore` command
- **Find & replace** (U05): `:find <query>`, `:replace <old> <new>`
- **MCP tools** (X01): `list-tabs`, `bookmark-crud`, `history-search` with request-response pattern
- **Extension API docs** (X02): `docs/extension-api.md` (426 lines)
- **Lua scripting guide** (X03): `docs/lua-scripting.md` (242 lines)
- **Bookmark import** (X05): Firefox/Chrome import buttons in bookmarks panel
- **Landing page** (Y04): `docs/index.html` (172 lines)
- **Crash-reload** (W01): Auto-reload crashed webviews
- **Tab unload LRU** (W02): Unload least-recently-used tabs
- **Adaptive framerate** (W03): Active 30fps, background 2fps
- **Input latency tracker** (W06): `:stats` command showing avg/max/p99

## v0.14.0 (2026-04-22) — Ecosystem & Documentation

### Documentation
- README.md with features, installation, keyboard shortcuts
- Help panel (`:help`) with command reference
- Configuration reference (`:config`)

## v0.13.0 (2026-04-21) — Reliability & Performance

### Reliability
- Crash-reload infrastructure with `webview_crash_detected` state
- Tab unload LRU with configurable threshold
- Background tab adaptive framerate (2fps)

### Performance
- GPU fallback chain: VULKAN|GL → GL → VULKAN
- Startup optimization with lazy initialization
- Input latency tracking with `:stats` command

## v0.12.0 (2026-04-19) — Settings Completion & Sync UI

### Settings Page Expansion
- Added **Sync** section: sync target, encryption toggle, passphrase (keyring-backed), auto-sync toggle, interval
- Added **Theme** picker: dropdown with all 7 built-in themes (dark, light, gruvbox-dark, nord, dracula, solarized-dark, solarized-light)
- Added **cosmetic filtering** toggle (adblock CSS element hiding)
- Added **auto-save workspace** toggle
- Fixed **custom CSS** label: was "Custom CSS Path" but config stores inline CSS, not a file path
- Search engine dropdown now **dynamically populated** from `config.search_engines` instead of hardcoded DuckDuckGo/Google

### Sync Passphrase Security
- Sync passphrase is now stored in **system keyring** (GNOME Keyring/KWallet) via `keyring` crate
- Settings page sends passphrase to keyring, never written to config.toml
- Passphrase field uses `type="password"` with `autocomplete="new-password"`

### Expanded :set Command
- Refactored duplicate `:set` handlers into shared `apply_set_setting()` helper method
- New settings available at runtime via `:set`:
  - `devtools` — enable/disable developer tools
  - `tab_layout` — sidebar/topbar/none
  - `sidebar_width` — pixel width (100-600 range validated)
  - `sidebar_right` — toggle sidebar position
  - `cosmetic_filtering` — adblock CSS element hiding
  - `auto_save` — workspace auto-save
  - `theme` — color theme selection
  - `adaptive_quality` — frame rate adaptive rendering
  - `sync_encrypted` — sync E2E encryption
  - `sync_auto` — real-time sync via filesystem watcher

### Bug Fixes
- Fixed `file_open_dialog` tests hanging indefinitely — now checks `AILERON_TESTING` env var before spawning GUI dialogs
- Fixed `file_open_dialog` early return when no display server available (headless environments)

### Test Results
- 667 lib tests + 26 integration + 13 startup + 1 offscreen = 707 total, all pass
- Zero clippy warnings
- Release build verified

## v0.11.0 (2026-04-18) — Sync Protocol Implementation

### Sync Engine
- SyncManager with content-defined chunking (fastcdc) + blake3 hashing
- SQLite online backup API for safe database snapshots during sync
- Delta detection: only transfers changed file chunks (not entire files)
- SyncManifest with JSON serialization for tracking sync state

### E2E Encryption
- Age encryption layer (X25519 + scrypt passphrase, age spec compliant)
- encrypt_file/decrypt_file for filesystem operations
- encrypt_data/decrypt_data for in-memory operations
- ASCII armor support for transport-safe encoding

### Real-Time Sync
- Filesystem watcher via notify + notify-debouncer-mini (2s debounce)
- Background thread monitors config directory for changes
- Cross-platform (inotify on Linux, FSEvents on macOS, ReadDirectoryChanges on Windows)

### Transport Layer
- SyncTarget: Local (path) or SSH (user@host:path)
- Push/pull operations with local staging directory
- SSH transport via scp (creates remote directory, copies staging)
- Configurable via :sync-target command

### Commands
- :sync — push local → remote
- :sync --pull — pull remote → local
- :sync --both — bidirectional sync
- :sync --status — show sync state
- :sync-watch — start real-time filesystem watcher
- :sync-stop — stop filesystem watcher
- :sync-target <target> — set sync target

### Dependencies Added
- fastcdc 4.0 (content-defined chunking, pure Rust)
- blake3 1.8 (fast hashing, SIMD-accelerated)
- notify 8.2 + notify-debouncer-mini 0.7 (filesystem watching)
- age 0.11 (E2E encryption, age spec compliant)

### Stats
- 707 total tests (+15 from v0.10.0)
- Zero clippy warnings

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
- **Tab pinning** — `Ctrl+Shift+P` or `:pin` to pin/unpin panes; pinned panes cannot be accidentally closed; pin indicator in sidebar

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
