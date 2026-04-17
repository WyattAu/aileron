# Aileron Competitive Gap Analysis v2.0
# Date: 2026-04-18
# Comparing Aileron v0.3.1 vs Floorp, Zen, Arc, Brave, Vivaldi, qutebrowser

---

## A. Feature Comparison Matrix

Legend: **Y** = Full support, **P** = Partial, **N** = No support, **-** = N/A

| Feature | Aileron | Floorp | Zen | Arc | Brave | Vivaldi | qutebrowser |
|---|---|---|---|---|---|---|---|
| **Tiling (split panes)** | **Y** (BSP tree, resize, multi-split) | N | P (split view, 2 tabs) | P (split view, 2 tabs) | N | P (tab tiling, grid) | N |
| **Keyboard-driven nav** | **Y** (vim-like modes, j/k/h/l) | P (Firefox shortcuts) | P (Firefox shortcuts) | P (Cmd shortcuts) | P (Chromium shortcuts) | P (customizable) | **Y** (vim-like) |
| **Custom keybindings** | **Y** (Lua init.lua + registry) | P (limited) | P (limited) | N | N | **Y** (full) | **Y** (full config) |
| **Tab/workspace mgmt** | **Y** (workspaces, save/restore) | P (tab groups) | **Y** (workspaces, containers) | **Y** (spaces, profiles) | P (tab groups) | **Y** (workspaces, tab stacks) | P (sessions) |
| **Built-in terminal** | **Y** (native, alacritty_terminal, ~2-5MB/pane) | N | N | N | N | N | N |
| **Vertical/side tab bar** | **Y** (sidebar, topbar, none) | P (vertical tabs) | **Y** (vertical tabs) | **Y** (sidebar) | N | P (panel) | N |
| **Command palette** | **Y** (omnibox + :commands, Nucleo fuzzy search) | N | N | **Y** (Cmd+T) | N | **Y** (Ctrl+E) | **Y** (command mode) |
| **Ad blocking** | **Y** (EasyList parser, network + cosmetic CSS, per-site toggle) | P (uBlock add-on) | P (uBlock add-on) | N | **Y** (Shields) | **Y** (built-in) | P (adblock lib) |
| **Content scripts** | **Y** (Lua-defined JS, @match, @match-regexp, @run-at) | **Y** (WebExtensions) | **Y** (WebExtensions) | **Y** (Chrome extensions) | **Y** (Chrome extensions) | **Y** (Chrome extensions) | P (userscripts) |
| **Extension ecosystem** | N (no WebExtensions API) | **Y** (full Firefox) | **Y** (full Firefox) | **Y** (full Chrome) | **Y** (full Chrome) | **Y** (full Chrome) | N |
| **Bookmark management** | **Y** (SQLite CRUD, search, import) | **Y** | **Y** | **Y** | **Y** | **Y** | **Y** |
| **History** | **Y** (SQLite, search, visit count, import) | **Y** | **Y** | **Y** | **Y** | **Y** | **Y** |
| **Download management** | **Y** (progress tracking, open file/dir, DB) | **Y** | **Y** | **Y** | **Y** | **Y** | **Y** |
| **Find in page** | **Y** (Ctrl+F, forward/back) | **Y** | **Y** | **Y** | **Y** | **Y** | **Y** |
| **Reader mode** | **Y** (strip CSS, extract article) | **Y** | **Y** | **Y** | **Y** | **Y** | **Y** |
| **DevTools integration** | **Y** (F12, WebKit inspector) | **Y** | **Y** | **Y** | **Y** | **Y** | **Y** |
| **Session restore** | **Y** (workspace save/load, auto-save, crash recovery) | **Y** | **Y** | **Y** | **Y** | **Y** | **Y** |
| **Multiple search engines** | **Y** (configurable, :engine cmd) | **Y** | **Y** | **Y** | **Y** | **Y** | **Y** |
| **Privacy (tracking)** | **Y** (tracker blocking, DNT/GPC, strict referrer) | **Y** (ETP) | **Y** (ETP) | **Y** | **Y** (Shields) | **Y** (tracker blocker) | P |
| **Privacy (fingerprinting)** | N | P | P | N | **Y** | **Y** | N |
| **Privacy (HTTPS upgrade)** | **Y** (EasyList HTTPS safe list) | **Y** (HTTPS-Only) | **Y** (HTTPS-Only) | N | **Y** | **Y** | N |
| **VPN** | N | N | N | N | **Y** (premium) | **Y** (Proton VPN) | N |
| **Password manager** | **Y** (Bitwarden CLI, auto-fill, form detection) | **Y** (Firefox PM) | **Y** (Firefox PM) | **Y** (Keychain) | **Y** (built-in) | **Y** (built-in) | P |
| **Email client** | N | N | N | N | N | **Y** | N |
| **Calendar** | N | N | N | N | N | **Y** | N |
| **Feed reader** | N | N | N | N | N | **Y** | N |
| **Notes** | N | N | N | N | N | **Y** | N |
| **AI assistant** | N | N | N | P (Arc Max) | **Y** (Leo AI) | N | N |
| **Mouse gestures** | N | N | N | N | N | **Y** | N |
| **Picture-in-Picture** | N | **Y** | **Y** | **Y** | **Y** | **Y** | N |
| **Tab stacking** | N | N | N | N | N | **Y** | N |
| **Sync across devices** | N | **Y** (Firefox Sync) | **Y** (Firefox Sync) | **Y** (Arc Sync) | **Y** (Brave Sync) | **Y** (E2E Sync) | P |
| **Quickmarks** | **Y** (m<letter>, g<letter>) | N | N | N | N | N | **Y** |
| **Scroll marks** | **Y** (m<letter>, '<letter>) | N | N | N | N | N | **Y** |
| **Link hints** | **Y** (vimium-style) | N | N | N | N | N | **Y** |
| **SSH shortcut** | **Y** (:ssh <host>) | N | N | N | N | N | N |
| **Git integration** | **Y** (:gs, :gl, :gd, :grep) | N | N | N | N | N | N |
| **Shell commands** | **Y** (!<cmd> in palette) | N | N | N | N | N | **Y** (:spawn) |
| **MCP/AI bridge** | **Y** (MCP server) | N | N | N | N | N | N |
| **Lua scripting** | **Y** (init.lua, custom commands) | N | N | N | N | N | N |
| **Custom CSS injection** | **Y** (config) | P (userChrome) | P (userChrome) | N | N | P | N |
| **Proxy support** | **Y** (config + runtime :proxy) | **Y** | **Y** | **Y** | **Y** | **Y** | **Y** |
| **Custom protocols** | **Y** (aileron://) | N | N | N | N | N | N |
| **Tab pinning** | **Y** (Ctrl+Shift+P, :pin) | **Y** | **Y** | **Y** | **Y** | **Y** | N |
| **Audio mute** | **Y** (:mute/:unmute) | **Y** | **Y** | **Y** | **Y** | **Y** | N |
| **Popup blocker** | **Y** (configurable) | **Y** | **Y** | **Y** | **Y** | **Y** | P |
| **Cookie management** | **Y** (view, clear, per-site allow/block) | **Y** | **Y** | **Y** | **Y** | **Y** | P |
| **Per-site settings** | **Y** (zoom, adblock, JS, cookies, autoplay) | P | P | P | **Y** | **Y** | P |
| **Settings GUI** | **Y** (aileron://settings) | **Y** | **Y** | **Y** | **Y** | **Y** | P |
| **Browser import** | **Y** (Firefox + Chrome) | **Y** | **Y** | **Y** | **Y** | **Y** | P |
| **Print support** | **Y** (:print) | **Y** | **Y** | **Y** | **Y** | **Y** | **Y** |
| **PDF viewer** | P (external viewer, built-in planned) | **Y** | **Y** | **Y** | **Y** | **Y** | **Y** |
| **Theme system** | **Y** (7 built-in + custom TOML, runtime switch) | **Y** | **Y** | **Y** | **Y** | **Y** | P |
| **Dark/Light toggle** | **Y** (:theme) | **Y** | **Y** | **Y** | **Y** | **Y** | P |

---

## B. Aileron's Current Feature Set (v0.3.1)

### Core Actions (src/input/keybindings.rs — Action enum)
- Navigation: ScrollUp/Down/Left/Right, HalfPageUp/Down, ScrollTop/Bottom
- Tiling: SplitHorizontal, SplitVertical, ClosePane, NavigateUp/Down/Left/Right
- History: NavigateBack, NavigateForward, Reload
- Bookmarking: BookmarkToggle
- Search: Find, FindNext, FindPrev, FindClose
- Modes: EnterInsertMode, OpenCommandPalette
- Tools: ToggleDevTools, ToggleReaderMode, ToggleMinimalMode, ToggleNetworkLog, ToggleConsoleLog
- Window: NewTab, NewWindow, OpenTerminal, DetachPane, CloseOtherPanes, PinPane
- Clipboard: CopyUrl, Yank, Paste
- Zoom: ZoomIn, ZoomOut, ZoomReset
- Resize: ResizePane(Direction)
- Marks: SetMark(char), GoToMark(char)
- Hints: ToggleLinkHints
- Print: Print
- Workspaces: SaveWorkspace

### Wry Actions (src/app/mod.rs — WryAction enum)
- Navigate, Back, Forward, Reload
- ToggleBookmark, Autofill (Bitwarden JS injection)
- ToggleDevTools, ScrollBy, SmoothScroll, ScrollTo, RunJs
- SaveWorkspace, EnterReaderMode, ExitReaderMode
- EnterMinimalMode, ExitMinimalMode
- ShowPaneError, ListContentScripts
- GetNetworkLog, ClearNetworkLog, GetConsoleLog, ClearConsoleLog
- SaveConfig, Print, ToggleMute

### Configuration (src/config.rs)
- homepage, window_width, window_height, devtools
- adblock_enabled, adblock_filter_lists (EasyList URLs), adblock_cosmetic_filtering, adblock_update_interval_hours
- https_upgrade_enabled, tracking_protection_enabled
- popup_blocker_enabled
- restore_session, auto_save, auto_save_interval
- init_lua_path, palette.max_results
- search_engine, search_engines (hashmap)
- custom_css, proxy
- tab_layout, tab_sidebar_width, tab_sidebar_right
- theme, themes (built-in + custom ThemeColors)
- render_mode ("offscreen"/"native"), config_version

### UI Panels (src/ui/panels.rs)
- Side panel (left/right) or top bar tab list with pin/mute indicators
- Status bar (mode, pane count, git status, URL, hints, messages)
- URL bar with omnibox (bookmarks + history + search, Nucleo fuzzy search)
- Find bar (search in page, forward/back)
- Command palette with categorized results + did-you-mean suggestions
- Central panel: offscreen webview textures + native terminal rendering (alacritty_terminal)

### Database (src/db/)
- SQLite with WAL mode
- Tables: history, bookmarks, workspaces, downloads, site_settings
- History: record_visit, search, recent_entries, prune_old, clear
- Bookmarks: add/remove/search/clear with upsert
- Workspaces: save/load/list/delete with BSP tree serialization
- Downloads: record, mark_completed, recent, clear, progress tracking
- Site Settings: per-domain settings (exact/wildcard/regex patterns), upsert, list, delete

### Network Layer (src/net/)
- **AdBlocker** (net/adblock.rs): EasyList-compatible parser, network + cosmetic CSS filtering, per-site toggle
- **FilterList** (net/filter_list.rs): NetworkFilter (pattern, resource types, third-party, domain-specific), CosmeticFilter (CSS selectors)
- **Privacy** (net/privacy.rs): HTTPS safe list (EasyList HTTPSEasy), tracking domain blocklist (Disconnect), DNT/GPC headers, referrer policy

### Terminal (src/terminal/)
- **NativeTerminalPane**: alacritty_terminal Term + vte::ansi::Processor + portable_pty PTY
- **NativeTerminalManager**: multi-pane terminal management keyed by UUID
- **Selection**: mouse-based text selection with start/extend/end/clipboard copy
- **Visual bell**: Event::Bell → 200ms flash via AtomicBool
- **Rendering**: egui Painter with dirty-cell-only redraw, damage tracking (full/partial)
- **PTY**: read thread → drain output → VTE parser → Term grid → render loop

### Special Features
- **Lua scripting** (src/lua/): init.lua, custom commands, keybinds, hooks, URL redirects
- **Content scripts** (src/scripts.rs): Lua-defined JS with @match/@match-regexp, @run-at (document_start/end/idle)
- **Bitwarden integration** (src/passwords/): unlock, search, autofill, login form detection (bw-detect)
- **MCP server** (src/mcp/): bridge, tools, transport for AI integration
- **Git integration** (src/git.rs): repo root detection, status bar display
- **ServoPane skeleton** (src/servo/servo_engine.rs): placeholder for future Servo engine integration
- **Browser import** (src/app/mod.rs): import-firefox, import-chrome (bookmarks + history)
- **Fuzzy search** (src/ui/search.rs): Nucleo matcher for command palette and omnibox
- **Did-you-mean**: Levenshtein distance suggestion for unknown commands

---

## C. Architecture Comparison

| Aspect | Aileron | Floorp | Zen | Arc | Brave | Vivaldi | qutebrowser |
|---|---|---|---|---|---|---|---|
| **Language** | Rust | C++/JS | C++/JS | C++/JS | C++/JS | C++/JS | Python |
| **UI Framework** | egui + winit | XUL/HTML | XUL/HTML | C++ (custom) | C++ (Chromium) | C++ (Blink) | Qt/QML |
| **Rendering Engine** | WebKitGTK (wry) | Gecko | Gecko | Chromium (Blink) | Chromium (Blink) | Chromium (Blink) | QtWebEngine (Blink) |
| **Terminal Engine** | alacritty_terminal + portable_pty | - | - | - | - | - | - |
| **Extension System** | Custom (Lua scripts) | WebExtensions (Firefox) | WebExtensions (Firefox) | Chrome Extensions | Chrome Extensions | Chrome Extensions | None |
| **Multi-process** | Single-process (wry in-process) | Multiprocess (e10s) | Multiprocess (e10s) | Multiprocess | Multiprocess | Multiprocess | Single-process |
| **GPU Acceleration** | wgpu (egui) + WebKitGTK | Gecko GPU | Gecko GPU | Skia + GPU | Skia + GPU | Skia + GPU | Qt RHI |
| **Memory Mgmt** | Manual (Rust ownership) | Firefox GC | Firefox GC | Chromium alloc | Chromium alloc | Chromium alloc | Python GC + Qt |
| **Update Mechanism** | Manual (cargo/git) | Built-in updater | Built-in updater | Built-in updater | Built-in updater | Built-in updater | Package manager |
| **Platform Support** | Linux only (WebKitGTK) | Win/Mac/Linux | Win/Mac/Linux | Win/Mac | Win/Mac/Linux | Win/Mac/Linux | Win/Mac/Linux/BSD |
| **Binary Size** | ~15-25MB | ~80MB | ~80MB | ~200MB | ~150MB | ~150MB | ~50MB + Qt deps |

---

## D. Competitive Advantages

### What Aileron Does BETTER

1. **True BSP Tiling** — No other browser offers binary space partition tiling with arbitrary splits. Vivaldi has tab tiling (grid only), Zen/Arc have split view (2 tabs max). Aileron supports unlimited recursive splits with resize.

2. **Native Terminal Integration** — Only Aileron embeds a terminal directly in the browser window as a first-class pane. Uses alacritty_terminal for ~2-5MB/pane (vs xterm.js's 30-50MB/pane). Full PTY, mouse selection, visual bell.

3. **Unified Keyboard-Driven Workflow** — Aileron combines vim-like modal editing, tiling, terminal, and browser navigation in a single keybinding system. qutebrowser has keyboard nav but no tiling or terminal.

4. **Lua Scripting Engine** — init.lua allows custom keybindings, commands, URL redirects, and hooks. No other browser offers a general-purpose scripting layer.

5. **Developer-First Features** — Git integration, SSH shortcut, shell commands, MCP/AI bridge, network/console log inspection. These are unique to Aileron.

6. **Zero-Config Tiling WM in a Browser** — BSP tree with automatic layout, resize, workspace save/restore. No plugins or extensions needed.

7. **Lightweight** — Rust + egui + wry architecture. ~15-25MB binary vs 80-200MB for competitors.

8. **Offscreen Rendering Architecture** — Web views render offscreen and are composited by egui. Pixel-perfect tiling control impossible with native window management.

9. **Privacy Without Extensions** — Ad blocking (EasyList), HTTPS upgrade, tracking protection, popup blocker, cookie management — all built-in, no extensions required.

10. **Per-Site Configuration** — Zoom, adblock, JS, cookies, autoplay configurable per domain with exact/wildcard/regex patterns, persisted across sessions.

### What is UNIQUE About Aileron

- **BSP tiling + native terminal + keyboard-driven** combination (no other product has all three)
- **Lua scripting** for browser customization
- **MCP (Model Context Protocol) server** for AI tool integration
- **aileron:// custom protocol** for internal pages (welcome, settings, files, terminal, error)
- **Git awareness** in the status bar + git commands
- **Quickmarks + scroll marks** (vim-like navigation primitives)
- **Pane detach** to standalone popup windows
- **Minimal mode** (JS disabled, images blocked) per-pane
- **Nucleo fuzzy search** in command palette and omnibox

---

## E. Gap Status (Updated from v1)

### CLOSED (Completed in v0.3.0/v0.3.1)

| ID | Feature | Status | How Implemented |
|---|---|---|---|
| **E3** | Robust Ad Blocking | **CLOSED** | EasyList parser (net/filter_list.rs), network + cosmetic CSS filtering, per-site toggle |
| **E5** | Download Manager with Progress UI | **CLOSED** | Progress tracking in DB (progress_percent, total/received bytes), :downloads with status, :downloads-open, :downloads-dir |
| **E6** | HTTPS Everywhere | **CLOSED** | EasyList HTTPSEasy safe list (net/privacy.rs), https_upgrade_enabled config, :https-toggle |
| **E7** | Tab Management (Pin, Mute) | **CLOSED (partial)** | Tab pinning (Ctrl+Shift+P, :pin), audio mute (:mute/:unmute). Missing: tab stacking, drag-reorder |
| **E8** | Cookie Management | **CLOSED** | :cookies (view), :cookies-clear, :cookies-block/:cookies-allow (per-site via site_settings) |
| **E9** | Settings GUI | **CLOSED** | aileron://settings page, keyboard-navigable, all config sections |
| **E10** | Import from Other Browsers | **CLOSED** | :import-firefox, :import-chrome (bookmarks + history) |
| **E11** | Print Support | **CLOSED** | :print command, WryAction::Print |
| **E12** | PDF Viewer | **CLOSED** | :pdf <path> opens via system viewer (built-in WebEngine PDF planned) |
| **E17** | Dark/Light Theme Toggle | **CLOSED** | 7 built-in themes + custom TOML, :theme <name>, runtime switching |

### REMAINING GAPS (Re-ranked by Priority)

#### CRITICAL — Users Will Not Switch Without These

##### E1. WebExtensions / Extension Ecosystem
- **Impact:** HIGH
- **Who has it:** Everyone except Aileron and qutebrowser
- **Current state:** Aileron has Lua content scripts but no WebExtensions API
- **Mitigation:** Built-in ad blocking, tracking protection, and content scripts cover the top use cases (uBlock Origin, Dark Reader)
- **Recommended action:** Defer. Target users (terminal-first developers) prefer scripts. Consider minimal subset (tabs API, storage) if demand arises post-v1.

##### E2. Cross-Platform Support (Windows/macOS)
- **Impact:** HIGH (but Linux-first is acceptable for target users)
- **Who has it:** All competitors
- **Current state:** Aileron depends on WebKitGTK (Linux only)
- **Mitigation:** wry supports WKWebView (macOS) and WebView2 (Windows) backends
- **Recommended action:** Medium-term. macOS via WKWebView is most feasible (wry cocoa feature). Windows via WebView2. Requires testing and packaging.

##### E13. Sync Across Devices
- **Impact:** MEDIUM
- **Who has it:** All major competitors
- **Current state:** No sync mechanism
- **Mitigation:** Workspace save/load + browser import covers initial migration
- **Recommended action:** Low priority. Could implement file-based sync (syncthing-friendly workspace export) before building a custom sync server.

##### E14. Privacy (Fingerprinting Protection)
- **Impact:** MEDIUM
- **Who has it:** Brave (full), Vivaldi (partial), Firefox-based (partial)
- **Current state:** No fingerprinting protection
- **Recommended action:** Medium priority. Could block known fingerprinting APIs (Canvas, WebGL, AudioContext) via content scripts or WebKit user-content filters.

#### MEDIUM — Users Notice the Absence

##### E15. Picture-in-Picture
- **Impact:** LOW-MEDIUM
- **Current state:** Not implemented
- **Recommended action:** Low priority. Can delegate to WebKit's native PiP support when available.

##### E16. Reading List / Read Later
- **Impact:** LOW
- **Current state:** Bookmarks serve this purpose
- **Recommended action:** Skip. Bookmarks + reader mode covers this.

##### E18. Font Size / Page Zoom Persistence
- **Impact:** LOW
- **Current state:** Per-site settings support zoom_level but it's not auto-applied on navigation
- **Recommended action:** Quick win — auto-apply site_settings zoom when navigating to a URL.

---

## F. Architecture D Readiness Assessment

Aileron has begun preparation for Architecture D (Servo engine integration):

### What Exists
- **ServoPane** (src/servo/servo_engine.rs): Skeleton struct implementing PaneRenderer trait with stub methods (navigate, execute_js, reload, back, forward)
- **PaneRenderer trait** (src/servo/engine.rs): Abstract trait allowing engine swapping — WryPane and ServoPane both implement it
- **PaneStateManager**: Engine-agnostic state management, keyed by UUID, supports arbitrary PaneRenderer implementations
- **EngineType enum**: Discriminant for engine selection

### What's Needed for Architecture D
1. **Servo embedding**: Embed Servo's `servo` crate as a library (not subprocess). Servo provides its own compositing and rendering.
2. **wgpu surface sharing**: Servo uses wgpu for rendering — need to share the wgpu surface between egui (UI overlay) and Servo (web content).
3. **Input routing**: Route winit events to both egui (when UI focused) and Servo (when web content focused).
4. **Async message passing**: Servo communicates via channels — need async bridge to Aileron's sync main loop.
5. **Feature parity**: Servo doesn't support all WebKit features (e.g., devtools, print, cookies API). Need fallback paths.
6. **Testing strategy**: Integration tests must work with both WryPane and ServoPane.

### Estimated Timeline
- Servo embedding prototype: Q3 2026 (as noted in `:engine servo` output)
- Feature parity with WryPane: Q4 2026
- Production-ready dual-engine: Q1 2027

### Risk Assessment
- **HIGH**: Servo is still under active development; API stability is not guaranteed
- **MEDIUM**: wgpu surface sharing between egui and Servo may require custom compositing
- **LOW**: PaneRenderer trait abstraction makes engine swapping clean

---

## G. Recommended Roadmap (Updated)

### Completed (v0.3.0 — v0.3.1)

| Task | Status |
|---|---|
| Hardened Ad Blocking (EasyList parser) | Done |
| HTTPS Upgrade + Tracking Protection | Done |
| Settings GUI (aileron://settings) | Done |
| Download Manager with Progress | Done |
| Import from Other Browsers | Done |
| Per-Site Settings | Done |
| Tab Pinning + Audio Mute | Done |
| Print Support | Done |
| PDF Viewer (external) | Done |
| Popup Blocker | Done |
| Cookie Management | Done |
| Theme System (7 built-in + custom) | Done |
| Enhanced Content Scripts (@match-regexp, @run-at) | Done |
| Native Terminal (alacritty_terminal) | Done |
| Terminal Mouse Selection | Done |
| Visual Bell | Done |
| Smooth Scrolling | Done |
| Nucleo Fuzzy Search | Done |
| Did-You-Mean Suggestions | Done |
| ServoPane Skeleton | Done |

### Next Priorities

#### Phase I: Quality & Polish (v0.4.0)

| ID | Task | Priority | Effort | Rationale |
|---|---|---|---|---|
| N01 | Auto-apply per-site zoom on navigation | LOW | 2-4h | Quick win, E18 gap |
| N02 | Built-in PDF viewer (pdf.js or WebKit PDF) | MEDIUM | 8-12h | Complete E12 (currently external viewer) |
| N03 | Fingerprinting protection (block Canvas/WebGL/Audio APIs) | MEDIUM | 6-8h | E14 gap, pairs with existing privacy stack |
| N04 | Content script management UI (aileron://scripts) | LOW | 4-6h | Power user feature |
| N05 | Keyboard-driven settings navigation improvements | LOW | 4-6h | Polish aileron://settings |

#### Phase II: Platform & Engine (v0.5.0)

| ID | Task | Priority | Effort | Rationale |
|---|---|---|---|---|
| N06 | macOS support (WKWebView via wry) | HIGH | 40-60h | E2 gap, largest market expansion |
| N07 | Servo embedding prototype | HIGH | 60-80h | Architecture D, long-term differentiator |
| N08 | File-based workspace sync (syncthing-friendly export) | MEDIUM | 8-12h | E13 gap without server infrastructure |
| N09 | Session/tab restore improvements | LOW | 4-6h | Polish crash recovery UX |

#### Phase III: Ecosystem (v1.0)

| ID | Task | Priority | Effort | Rationale |
|---|---|---|---|---|
| N10 | Minimal WebExtensions API (tabs, storage, webRequest) | MEDIUM | 80-120h | E1 gap, enables uBlock Origin / Dark Reader |
| N11 | Windows support (WebView2 via wry) | MEDIUM | 40-60h | E2 gap |
| N12 | Extension management UI (aileron://extensions) | LOW | 12-16h | Companion to N10 |

### Total Estimated Remaining Effort
- Phase I (Polish): 24-44 hours (~1 week)
- Phase II (Platform): 112-164 hours (~3-4 weeks)
- Phase III (Ecosystem): 132-196 hours (~3-5 weeks)

### Recommended Implementation Order
1. **N01** (auto-apply zoom) — trivial, closes E18
2. **N03** (fingerprinting) — extends privacy stack, differentiator
3. **N02** (PDF viewer) — completes E12
4. **N06** (macOS) — largest user impact
5. **N07** (Servo prototype) — long-term strategic bet
6. Remaining items based on user feedback

---

## Target User Positioning

> **"The terminal for the web"** — A keyboard-driven, tiling web environment for developers who live in terminals and want their browser to feel like tmux/neovim.

**Primary target:** Developers who use vim/neovim, tmux, and terminal-first workflows.
**Secondary target:** Keyboard-driven purists (qutebrowser users) who want tiling + terminal.
**NOT targeting:** General consumers, enterprise users, or users who depend on specific Chrome/Firefox extensions.

With v0.3.1, Aileron has closed 11 of the 18 gaps identified in v1. The remaining gaps (WebExtensions, cross-platform, sync, fingerprinting) are either lower priority for the target audience or long-term strategic investments. The unique combination of BSP tiling, native terminal, Lua scripting, and built-in privacy features is unmatched by any competitor.
