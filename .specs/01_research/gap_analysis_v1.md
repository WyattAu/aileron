# Aileron Competitive Gap Analysis v1.0
# Date: 2026-04-17
# Comparing Aileron vs Floorp, Zen, Arc, Brave, Vivaldi, qutebrowser

---

## A. Feature Comparison Matrix

Legend: **Y** = Full support, **P** = Partial, **N** = No support, **-** = N/A

| Feature | Aileron | Floorp | Zen | Arc | Brave | Vivaldi | qutebrowser |
|---|---|---|---|---|---|---|---|
| **Tiling (split panes)** | **Y** (BSP tree, resize, multi-split) | N | P (split view, 2 tabs) | P (split view, 2 tabs) | N | P (tab tiling, grid) | N |
| **Keyboard-driven nav** | **Y** (vim-like modes, j/k/h/l) | P (Firefox shortcuts) | P (Firefox shortcuts) | P (Cmd shortcuts) | P (Chromium shortcuts) | P (customizable) | **Y** (vim-like) |
| **Custom keybindings** | **Y** (Lua init.lua + registry) | P (limited) | P (limited) | N | N | **Y** (full) | **Y** (full config) |
| **Tab/workspace mgmt** | **Y** (workspaces, save/restore) | P (tab groups) | **Y** (workspaces, containers) | **Y** (spaces, profiles) | P (tab groups) | **Y** (workspaces, tab stacks) | P (sessions) |
| **Built-in terminal** | **Y** (native, alacritty_terminal) | N | N | N | N | N | N |
| **Vertical/side tab bar** | **Y** (sidebar, topbar, none) | P (vertical tabs) | **Y** (vertical tabs) | **Y** (sidebar) | N | P (panel) | N |
| **Command palette** | **Y** (omnibox + :commands) | N | N | **Y** (Cmd+T) | N | **Y** (Ctrl+E) | **Y** (command mode) |
| **Ad blocking** | P (domain-based, EasyList subset) | P (uBlock add-on) | P (uBlock add-on) | N | **Y** (built-in Brave Shields) | **Y** (built-in) | P (adblock lib) |
| **Content scripts** | **Y** (Lua-defined JS injection) | **Y** (WebExtensions) | **Y** (WebExtensions) | **Y** (Chrome extensions) | **Y** (Chrome extensions) | **Y** (Chrome extensions) | P (userscripts) |
| **Extension ecosystem** | N (no WebExtensions API) | **Y** (full Firefox) | **Y** (full Firefox) | **Y** (full Chrome) | **Y** (full Chrome) | **Y** (full Chrome) | N |
| **Bookmark management** | **Y** (SQLite CRUD, search) | **Y** | **Y** | **Y** | **Y** | **Y** | **Y** |
| **History** | **Y** (SQLite, search, visit count) | **Y** | **Y** | **Y** | **Y** | **Y** | **Y** |
| **Download management** | P (DB record, basic save) | **Y** | **Y** | **Y** | **Y** | **Y** | **Y** (download manager) |
| **Find in page** | **Y** (Ctrl+F, forward/back) | **Y** | **Y** | **Y** | **Y** | **Y** | **Y** |
| **Reader mode** | **Y** (strip CSS, extract article) | **Y** | **Y** | **Y** | **Y** | **Y** | **Y** |
| **DevTools integration** | **Y** (F12, WebKit inspector) | **Y** | **Y** | **Y** | **Y** | **Y** | **Y** |
| **Session restore** | **Y** (workspace save/load, auto-save) | **Y** | **Y** | **Y** | **Y** | **Y** | **Y** |
| **Multiple search engines** | **Y** (configurable, :engine cmd) | **Y** | **Y** | **Y** | **Y** (Brave Search default) | **Y** | **Y** |
| **Privacy (tracking)** | P (adblock only) | **Y** (ETP) | **Y** (ETP) | **Y** | **Y** (Shields, fingerprinting) | **Y** (tracker blocker) | P |
| **Privacy (fingerprinting)** | N | P | P | N | **Y** | **Y** | N |
| **VPN** | N | N | N | N | **Y** (premium) | **Y** (Proton VPN) | N |
| **Password manager** | **Y** (Bitwarden CLI integration) | **Y** (Firefox PM) | **Y** (Firefox PM) | **Y** (Keychain) | **Y** (built-in) | **Y** (built-in) | P |
| **Email client** | N | N | N | N | N | **Y** (built-in) | N |
| **Calendar** | N | N | N | N | N | **Y** (built-in) | N |
| **Feed reader** | N | N | N | N | N | **Y** (built-in) | N |
| **Notes** | N | N | N | N | N | **Y** (built-in) | N |
| **AI assistant** | N | N | N | P (Arc Max) | **Y** (Leo AI) | N | N |
| **Mouse gestures** | N | N | N | N | N | **Y** | N |
| **Picture-in-Picture** | N | **Y** | **Y** | **Y** | **Y** | **Y** | N |
| **Tab stacking** | N | N | N | N | N | **Y** | N |
| **Sync across devices** | N | **Y** (Firefox Sync) | **Y** (Firefox Sync) | **Y** (Arc Sync) | **Y** (Brave Sync) | **Y** (E2E Sync) | P (third-party) |
| **Quickmarks** | **Y** (m<letter>, g<letter>) | N | N | N | N | N | **Y** (bmarks) |
| **Scroll marks** | **Y** (m<letter>, '<letter>) | N | N | N | N | N | **Y** |
| **Link hints** | **Y** (vimium-style) | N | N | N | N | N | **Y** |
| **SSH shortcut** | **Y** (:ssh <host>) | N | N | N | N | N | N |
| **Git integration** | **Y** (:gs, :gl, :gd, :grep) | N | N | N | N | N | N |
| **Shell commands** | **Y** (!<cmd> in palette) | N | N | N | N | N | **Y** (:spawn) |
| **MCP/AI bridge** | **Y** (MCP server) | N | N | N | N | N | N |
| **Lua scripting** | **Y** (init.lua, custom commands) | N | N | N | N | N | N |
| **Custom CSS injection** | **Y** (config) | P (userChrome) | P (userChrome) | N | N | P (custom CSS) | N |
| **Proxy support** | **Y** (config + runtime :proxy) | **Y** | **Y** | **Y** | **Y** | **Y** | **Y** |
| **Custom protocols** | **Y** (aileron://) | N | N | N | N | N | N |

---

## B. Aileron's Current Feature Set (from source)

### Core Actions (src/input/keybindings.rs — Action enum)
- Navigation: ScrollUp/Down/Left/Right, HalfPageUp/Down, ScrollTop/Bottom
- Tiling: SplitHorizontal, SplitVertical, ClosePane, NavigateUp/Down/Left/Right
- History: NavigateBack, NavigateForward, Reload
- Bookmarking: BookmarkToggle
- Search: Find, FindNext, FindPrev, FindClose
- Modes: EnterInsertMode, OpenCommandPalette
- Tools: ToggleDevTools, ToggleReaderMode, ToggleMinimalMode, ToggleNetworkLog, ToggleConsoleLog
- Window: NewTab, NewWindow, OpenTerminal, DetachPane, CloseOtherPanes
- Clipboard: CopyUrl, Yank, Paste
- Zoom: ZoomIn, ZoomOut, ZoomReset
- Resize: ResizePane(Direction)
- Marks: SetMark(char), GoToMark(char)
- Hints: ToggleLinkHints
- Workspaces: SaveWorkspace

### Wry Actions (src/app/mod.rs — WryAction enum)
- Navigate, Back, Forward, Reload
- ToggleBookmark, Autofill (Bitwarden JS injection)
- ToggleDevTools, ScrollBy, ScrollTo, RunJs
- SaveWorkspace, EnterReaderMode, ExitReaderMode
- EnterMinimalMode, ExitMinimalMode
- ShowPaneError, ListContentScripts
- GetNetworkLog, ClearNetworkLog, GetConsoleLog, ClearConsoleLog
- SaveConfig

### Configuration (src/config.rs)
- homepage, window_width, window_height, devtools, adblock_enabled
- restore_session, auto_save, auto_save_interval
- init_lua_path, palette.max_results
- search_engine, search_engines (hashmap)
- custom_css, proxy, tab_layout, tab_sidebar_width, tab_sidebar_right
- render_mode ("offscreen"/"native"), config_version

### UI Panels (src/ui/panels.rs)
- Side panel (left/right) or top bar tab list
- Status bar (mode, pane count, git status, URL, hints, messages)
- URL bar with omnibox (bookmarks + history search)
- Find bar (search in page, forward/back)
- Command palette with categorized results
- Central panel: offscreen webview textures + native terminal rendering

### Database (src/db/)
- SQLite with WAL mode
- Tables: history, bookmarks, workspaces, downloads
- History: record_visit, search, recent_entries, prune_old, clear
- Bookmarks: add/remove/search/clear with upsert
- Workspaces: save/load/list/delete with BSP tree serialization
- Downloads: record, mark_completed, recent, clear

### Special Features
- **Lua scripting** (src/lua/): init.lua, custom commands, keybinds, hooks, URL redirects
- **Content scripts** (src/scripts.rs): Lua-defined JS with @match patterns, @grant metadata
- **Bitwarden integration** (src/passwords/): unlock, search, autofill via CLI
- **MCP server** (src/mcp/): bridge, tools, transport for AI integration
- **Native terminal** (src/terminal/): alacritty_terminal + portable_pty + egui painter
- **Ad blocking** (src/net/adblock.rs): domain-based, EasyList-compatible, cosmetic CSS
- **Git integration** (src/git.rs): repo root detection, status bar display

---

## C. Architecture Comparison

| Aspect | Aileron | Floorp | Zen | Arc | Brave | Vivaldi | qutebrowser |
|---|---|---|---|---|---|---|---|
| **Language** | Rust | C++/JS | C++/JS | C++/JS | C++/JS | C++/JS | Python |
| **UI Framework** | egui + winit | XUL/HTML | XUL/HTML | C++ (custom) | C++ (Chromium) | C++ (Blink) | Qt/QML |
| **Rendering Engine** | WebKitGTK (wry) | Gecko | Gecko | Chromium (Blink) | Chromium (Blink) | Chromium (Blink) | QtWebEngine (Blink) |
| **Extension System** | Custom (Lua scripts) | WebExtensions (Firefox) | WebExtensions (Firefox) | Chrome Extensions | Chrome Extensions | Chrome Extensions | None (userscripts) |
| **Multi-process** | Single-process (wry in-process) | Multiprocess (e10s) | Multiprocess (e10s) | Multiprocess | Multiprocess (sandbox) | Multiprocess (sandbox) | Single-process (QtWebEngine) |
| **GPU Acceleration** | wgpu (egui) + WebKitGTK | Gecko GPU | Gecko GPU | Skia + GPU | Skia + GPU | Skia + GPU | Qt RHI |
| **Memory Mgmt** | Manual (Rust ownership) | Firefox GC | Firefox GC | Chromium partition alloc | Chromium partition alloc | Chromium partition alloc | Python GC + Qt |
| **Update Mechanism** | Manual (git build) | Built-in updater | Built-in updater | Built-in updater | Built-in updater | Built-in updater | Package manager |
| **Platform Support** | Linux only (WebKitGTK) | Win/Mac/Linux | Win/Mac/Linux | Win/Mac (Linux beta) | Win/Mac/Linux | Win/Mac/Linux | Win/Mac/Linux/BSD |
| **Binary Size** | ~15-25MB | ~80MB | ~80MB | ~200MB | ~150MB | ~150MB | ~50MB + Qt deps |

---

## D. Competitive Advantages

### What Aileron Does BETTER

1. **True BSP Tiling** — No other browser offers binary space partition tiling with arbitrary splits. Vivaldi has tab tiling (grid only), Zen/Arc have split view (2 tabs max). Aileron supports unlimited recursive splits with resize.

2. **Native Terminal Integration** — Only Aileron embeds a terminal directly in the browser window as a first-class pane. No other browser has this. The terminal uses alacritty_terminal for ~2-5MB/pane vs xterm.js's 30-50MB/pane.

3. **Unified Keyboard-Driven Workflow** — Aileron combines vim-like modal editing, tiling, terminal, and browser navigation in a single keybinding system. qutebrowser has keyboard nav but no tiling or terminal.

4. **Lua Scripting Engine** — init.lua allows custom keybindings, commands, URL redirects, and hooks (mode_change, navigate). No other browser offers a general-purpose scripting layer.

5. **Developer-First Features** — Git integration (:gs, :gl, :gd, :grep), SSH shortcut (:ssh), shell commands (!<cmd>), MCP/AI bridge, network/console log inspection. These are unique to Aileron.

6. **Zero-Config Tiling WM in a Browser** — BSP tree with automatic layout, resize, workspace save/restore. No plugins or extensions needed.

7. **Lightweight** — Rust + egui + wry architecture. ~15-25MB binary vs 80-200MB for competitors.

8. **Offscreen Rendering Architecture** — Web views render offscreen and are composited by egui. This enables pixel-perfect tiling control impossible with native window management.

### What is UNIQUE About Aileron

- **BSP tiling + native terminal + keyboard-driven** combination (no other product has all three)
- **Lua scripting** for browser customization
- **MCP (Model Context Protocol) server** for AI tool integration
- **aileron:// custom protocol** for internal pages (welcome, files, terminal, error)
- **Git awareness** in the status bar
- **Quickmarks + scroll marks** (vim-like navigation primitives)
- **Pane detach** to standalone popup windows
- **Minimal mode** (JS disabled, images blocked) per-pane

---

## E. Critical Gaps (Prioritized by Switch Impact)

### CRITICAL — Users Will Not Switch Without These

#### E1. WebExtensions / Extension Ecosystem
- **Impact:** HIGH — Most users rely on extensions (uBlock Origin, 1Password, LastPass, Dark Reader, etc.)
- **Who has it:** Everyone except Aileron and qutebrowser
- **Current state:** Aileron has Lua content scripts (JS injection) but no WebExtensions API
- **Migration friction:** EXTREME — users would lose all their extensions
- **Note:** qutebrowser survives without it because it targets keyboard-driven purists

#### E2. Cross-Platform Support (Windows/macOS)
- **Impact:** HIGH — Linux-only limits adoption severely
- **Who has it:** All competitors support Win/Mac/Linux
- **Current state:** Aileron depends on WebKitGTK (Linux only)
- **Migration friction:** HIGH — dual-boot users can't use Aileron on all systems

#### E3. Robust Ad Blocking
- **Impact:** HIGH — Ad blocking is a baseline expectation in 2026
- **Who has it:** Brave (Shields), Vivaldi (built-in), all others via uBlock Origin extension
- **Current state:** Domain-based blocking with EasyList subset. No element hiding API, no $CSP/$popup/$media rules
- **Migration friction:** HIGH — ads visible = instant dealbreaker for most users

### HIGH — Users Strongly Expect These

#### E4. Password Manager (Native, Not CLI)
- **Impact:** HIGH — Password auto-fill is critical daily workflow
- **Who has it:** All competitors (built-in or via extension)
- **Current state:** Bitwarden CLI integration works but requires CLI unlock, not browser-native
- **Migration friction:** MEDIUM — workaround exists but is clunky

#### E5. Download Manager with Progress UI
- **Impact:** HIGH — Users expect download progress bars, pause/resume, open folder
- **Who has it:** All competitors
- **Current state:** Downloads save to ~/Downloads/ with DB record, no UI feedback
- **Migration friction:** MEDIUM

#### E6. HTTPS Everywhere / Secure Connection Upgrade
- **Impact:** HIGH — Security baseline
- **Who has it:** Brave, Firefox-based (Floorp, Zen), Vivaldi
- **Current state:** No HTTPS upgrade mechanism
- **Migration friction:** MEDIUM

#### E7. Tab Management Beyond BSP (Close-All-Except, Pin, Mute)
- **Impact:** MEDIUM-HIGH — Power users need pin/mute/close-others as single actions
- **Who has it:** All competitors
- **Current state:** Has CloseOtherPanes but no pin or mute audio
- **Migration friction:** LOW-MEDIUM

### MEDIUM — Users Notice the Absence

#### E8. Cookie Management (Per-Site)
- **Impact:** MEDIUM — Cookie clearing exists but no per-site UI
- **Who has it:** All competitors (settings UI)
- **Current state:** Clear all cookies via :clear cookies, no per-site control

#### E9. Settings/Preferences GUI
- **Impact:** MEDIUM — Most users expect a settings page, not TOML editing
- **Who has it:** All competitors
- **Current state:** Config via TOML file + :set command (runtime)

#### E10. Import from Other Browsers
- **Impact:** MEDIUM — Switching requires importing bookmarks/history/passwords
- **Who has it:** All competitors
- **Current state:** No import mechanism

#### E11. Print Support
- **Impact:** MEDIUM — Occasional need
- **Who has it:** All competitors
- **Current state:** No print support

#### E12. PDF Viewer
- **Impact:** MEDIUM — PDFs open in external viewer
- **Who has it:** All competitors (built-in)
- **Current state:** No built-in PDF viewer

#### E13. Sync Across Devices
- **Impact:** MEDIUM — Important for multi-device users
- **Who has it:** All major competitors
- **Current state:** No sync mechanism

#### E14. Popup/Notification Permission Control
- **Impact:** LOW-MEDIUM
- **Who has it:** All competitors
- **Current state:** No popup blocking beyond adblock

### LOW — Nice-to-Have

#### E15. Picture-in-Picture
- **Current state:** Not implemented

#### E16. Reading List / Read Later
- **Current state:** Bookmarks serve this purpose

#### E17. Dark/Light Theme Toggle
- **Current state:** Hardcoded dark theme

#### E18. Font Size / Page Zoom Persistence
- **Current state:** Zoom exists per-session, not persisted per-site

---

## F. Recommended Roadmap Additions

### Phase I: Critical Survival (must-have before any public release)

#### TASK-I01: Hardened Ad Blocking
- **Priority:** CRITICAL
- **Estimated effort:** 16-24 hours
- **Dependencies:** None
- **Description:**
  - Integrate `brave-adblock` or `adblock-rs` crate for proper ABP filter list parsing
  - Support network-level rules ($third-party, $popup, $media, $image)
  - Support element hiding rules (##selector, domain##selector)
  - Support $CSP, $redirect, $removeheader rules
  - Download and auto-update filter lists (EasyList, EasyPrivacy, uBlock filters)
  - Show blocked count in status bar
  - Per-site whitelist toggle (:adblock-toggle)
  - Cosmetic CSS injection for hidden elements
- **Verification:** adblock-testpages.com passes all tests

#### TASK-I02: HTTPS Upgrade / Tracking Protection
- **Priority:** CRITICAL
- **Estimated effort:** 8-12 hours
- **Dependencies:** None
- **Description:**
  - Auto-upgrade HTTP to HTTPS for known-safe domains (HTTPS Everywhere list)
  - Block known tracking domains (Disconnect list)
  - Referrer header stripping (send only origin)
  - DNT header (Do Not Track)
  - GPC header (Global Privacy Control)
  - Configurable per-site via :privacy command
- **Verification:** No mixed content on HTTPS sites; tracking domains blocked

#### TASK-I03: Settings GUI (aileron://settings)
- **Priority:** HIGH
- **Estimated effort:** 12-16 hours
- **Dependencies:** None
- **Description:**
  - Create aileron://settings custom protocol page
  - Sections: General (homepage, search engine, startup), Appearance (theme, tab layout, sidebar), Privacy (adblock, tracking, HTTPS), Advanced (devtools, proxy, custom CSS)
  - All changes persist to config.toml via IPC
  - Keyboard-navigable (vim-style)
  - Runtime config changes without restart
- **Verification:** All config values editable from GUI; changes persist

#### TASK-I04: Download Manager UI
- **Priority:** HIGH
- **Estimated effort:** 8-12 hours
- **Dependencies:** None
- **Description:**
  - Status bar notification on download start/complete
  - :downloads command shows list with status
  - Open downloaded file (:downloads-open <id>)
  - Open download directory (:downloads-dir)
  - Cancel active downloads
  - Progress indication in status bar (% downloaded)
- **Verification:** Downloads show progress, can be opened after completion

#### TASK-I05: Import from Other Browsers
- **Priority:** HIGH
- **Estimated effort:** 8-12 hours
- **Dependencies:** None
- **Description:**
  - Parse Firefox bookmarks.html and places.sqlite
  - Parse Chrome/Chromium Bookmarks and History JSON files
  - Import bookmarks into Aileron DB (skip duplicates)
  - Import history into Aileron DB
  - :import-firefox and :import-chrome commands
  - Auto-detect browser data directories (XDG paths)
- **Verification:** Bookmarks and history imported from Firefox/Chrome

### Phase II: Core Experience Polish

#### TASK-I06: Per-Site Settings
- **Priority:** HIGH
- **Estimated effort:** 12-16 hours
- **Dependencies:** None
- **Description:**
  - Per-site configuration stored in SQLite (site_settings table)
  - Settings per site: zoom level, adblock on/off, JS on/off, cookie allow/block, auto-play allow/block
  - :site-settings command to view/edit current site
  - Persist across sessions
  - URL pattern matching (exact, wildcard, regex)
- **Verification:** Settings persist per site across restarts

#### TASK-I07: Improved Password Manager Integration
- **Priority:** HIGH
- **Estimated effort:** 8-12 hours
- **Dependencies:** None
- **Description:**
  - Auto-detect login forms and offer credential fill (via content script)
  - Credential search from command palette (:bw-search integrated)
  - Save new credentials (capture form submit)
  - Support for password-only unlock (not full vault unlock)
  - Integration with system keyring for vault password caching
- **Verification:** Auto-fill works on login forms without manual bw-search

#### TASK-I08: Print Support
- **Priority:** MEDIUM
- **Estimated effort:** 4-6 hours
- **Dependencies:** None
- **Description:**
  - :print command triggers system print dialog
  - Use wry's print API if available
  - Fallback: generate PDF via headless approach
  - Ctrl+P keybinding
- **Verification:** Current page prints via system dialog

#### TASK-I09: Built-in PDF Viewer
- **Priority:** MEDIUM
- **Estimated effort:** 8-12 hours
- **Dependencies:** TASK-I02 (adblock, to avoid blocking PDF resources)
- **Description:**
  - Detect PDF content-type responses
  - Render PDF using pdf.js in a custom aileron:// viewer
  - Support zoom, page navigation, text selection
  - :pdf <path> command to open local PDFs
- **Verification:** PDF files render in browser without external app

#### TASK-I10: Popup Blocker + Notification Control
- **Priority:** MEDIUM
- **Estimated effort:** 4-6 hours
- **Dependencies:** None
- **Description:**
  - Block unwanted popup windows (allow from user gestures only)
  - Per-site notification permission control
  - Status bar indicator when popup blocked
  - :popups command to manage blocked/allowed sites
- **Verification:** Unwanted popups blocked; allowed when user-initiated

#### TASK-I11: Cookie Management UI
- **Priority:** MEDIUM
- **Estimated effort:** 6-8 hours
- **Dependencies:** TASK-I06 (per-site settings)
- **Description:**
  - :cookies command to view cookies for current site
  - :cookies-clear-site to clear current site cookies
  - Per-site cookie policy (allow, block, session-only)
  - Third-party cookie blocking option
- **Verification:** Per-site cookie control works

### Phase III: Differentiation Deepening

#### TASK-I12: Tab Audio Mute/Indicator
- **Priority:** MEDIUM
- **Estimated effort:** 4-6 hours
- **Dependencies:** None
- **Description:**
  - Detect audio-playing panes (via WebKit API)
  - Show audio indicator icon in tab sidebar
  - :mute / :unmute commands for active pane
  - Global mute-all (:mute-all)
- **Verification:** Audio indicator appears; mute works

#### TASK-I13: Theme System
- **Priority:** MEDIUM
- **Estimated effort:** 8-12 hours
- **Dependencies:** None
- **Description:**
  - Dark/Light/System theme toggle
  - Custom theme support via TOML config
  - Color scheme for: background, foreground, accent, tab bar, status bar, URL bar
  - Built-in themes: default-dark, default-light, gruvbox, nord, dracula, solarized
  - :theme <name> command
  - Per-workspace theme
- **Verification:** Multiple themes switchable at runtime

#### TASK-I14: Enhanced Content Script System
- **Priority:** MEDIUM
- **Estimated effort:** 12-16 hours
- **Dependencies:** None
- **Description:**
  - @match URL pattern improvements (regex support)
  - @run-at (document_start, document_end, document_idle)
  - Script persistence toggle
  - Script management UI (aileron://scripts)
  - Script error reporting in console log
  - Multiple scripts per URL (execution order)
  - Shared storage between scripts (per-domain)
- **Verification:** Content scripts execute at correct lifecycle stage

#### TASK-I15: Session Auto-Complete (Crash Recovery)
- **Priority:** HIGH
- **Estimated effort:** 6-8 hours
- **Dependencies:** None
- **Description:**
  - Auto-save workspace every N seconds (configurable, default 30s)
  - On startup, detect if previous session was unclean (pid file or crash flag)
  - Offer to restore last session or start fresh
  - Separate auto-save from user-named workspaces
  - _autosave workspace that overwrites on each auto-save
- **Verification:** After crash, Aileron offers session restore

### Phase IV: Platform Expansion (Long-Term)

#### TASK-I16: macOS Support via WKWebView
- **Priority:** LOW (long-term)
- **Estimated effort:** 40-60 hours
- **Dependencies:** Architecture B completion (Phase F)
- **Description:**
  - Replace wry's Linux-specific code with macOS WKWebView backend
  - Use wry's cocoa feature for macOS webview
  - Native macOS window management (NSWindow)
  - Code signing and notarization
- **Verification:** Aileron builds and runs on macOS

#### TASK-I17: Windows Support via WebView2
- **Priority:** LOW (long-term)
- **Estimated effort:** 40-60 hours
- **Dependencies:** Architecture B completion (Phase F)
- **Description:**
  - Use wry's webview2 feature for Windows
  - Native Windows window management
  - MSIX packaging for Microsoft Store
- **Verification:** Aileron builds and runs on Windows 10/11

#### TASK-I18: Extension Compatibility Layer (Partial WebExtensions)
- **Priority:** LOW (long-term)
- **Estimated effort:** 80-120 hours
- **Dependencies:** TASK-I14 (enhanced content scripts)
- **Description:**
  - Implement minimal WebExtensions API subset:
    - browser.tabs (basic)
    - browser.storage (local)
    - browser.runtime (messaging)
    - browser.webRequest (for adblockers — delegate to native adblock)
  - Focus on supporting uBlock Origin and Dark Reader
  - manifest.json v2/v3 parsing
  - Extension installation from AMO/Chrome Web Store (download + verify)
  - aileron://extensions management page
- **Verification:** uBlock Origin loads and blocks ads; Dark Reader applies themes

---

## Summary Priority Matrix

| ID | Feature | Priority | Effort (hrs) | Impact | Phase |
|---|---|---|---|---|---|
| I01 | Hardened Ad Blocking | CRITICAL | 16-24 | Users won't switch without ad blocking | I |
| I02 | HTTPS Upgrade + Tracking Protection | CRITICAL | 8-12 | Security baseline expectation | I |
| I03 | Settings GUI | HIGH | 12-16 | Non-technical users can't use TOML | I |
| I04 | Download Manager UI | HIGH | 8-12 | Missing feedback on downloads | I |
| I05 | Import from Other Browsers | HIGH | 8-12 | Switching requires migration | I |
| I06 | Per-Site Settings | HIGH | 12-16 | Power user expectation | II |
| I07 | Password Manager Improvement | HIGH | 8-12 | Daily workflow friction | II |
| I08 | Print Support | MEDIUM | 4-6 | Occasional hard requirement | II |
| I09 | PDF Viewer | MEDIUM | 8-12 | Common use case | II |
| I10 | Popup Blocker | MEDIUM | 4-6 | Annoyance prevention | II |
| I11 | Cookie Management UI | MEDIUM | 6-8 | Privacy expectation | II |
| I12 | Tab Audio Mute | MEDIUM | 4-6 | Quality of life | III |
| I13 | Theme System | MEDIUM | 8-12 | Personalization | III |
| I14 | Enhanced Content Scripts | MEDIUM | 12-16 | Power user extensibility | III |
| I15 | Session Auto-Complete | HIGH | 6-8 | Crash recovery | III |
| I16 | macOS Support | LOW | 40-60 | Market expansion | IV |
| I17 | Windows Support | LOW | 40-60 | Market expansion | IV |
| I18 | Partial WebExtensions API | LOW | 80-120 | Extension ecosystem access | IV |

### Total Estimated Effort
- Phase I (Critical): 52-76 hours (~2 weeks)
- Phase II (Polish): 42-58 hours (~1.5 weeks)
- Phase III (Differentiation): 30-42 hours (~1 week)
- Phase IV (Platform): 160-260 hours (~6-10 weeks)

### Recommended Implementation Order
1. **TASK-I01** (Ad blocking) — do this first, it's the #1 user expectation
2. **TASK-I02** (HTTPS/tracking) — pairs well with ad blocking work
3. **TASK-I15** (Session auto-complete) — quick win, already partially done (auto_save config exists)
4. **TASK-I03** (Settings GUI) — enables non-dev users
5. **TASK-I04** (Download UI) — quick win
6. **TASK-I05** (Import) — enables switching
7. **TASK-I06** (Per-site settings) — enables power users
8. **TASK-I07** (Password manager) — daily workflow
9. Remaining Phase II/III items in priority order

---

## Target User Positioning

Aileron should NOT try to compete as a general-purpose browser replacement for Chrome/Firefox users. Instead, it should position as:

> **"The terminal for the web"** — A keyboard-driven, tiling web environment for developers who live in terminals and want their browser to feel like tmux/neovim.

**Primary target:** Developers who use vim/neovim, tmux, and terminal-first workflows.
**Secondary target:** Keyboard-driven purists (qutebrowser users) who want tiling + terminal.
**NOT targeting:** General consumers, enterprise users, or users who depend on specific Chrome/Firefox extensions.

This positioning means:
- Phase I items (ad blocking, HTTPS, settings) are still critical because they're hygiene factors
- Extension ecosystem (TASK-I18) can be deferred because target users prefer scripts over GUI extensions
- Cross-platform (TASK-I16/I17) is important but Linux-first is acceptable for v1
- The unique features (tiling, terminal, Lua, git) are the primary selling points
