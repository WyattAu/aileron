# Aileron

**v0.15.0** — 806 tests, ~38,000 Rust LOC

**The terminal for the web.** A keyboard-driven, tiling web environment with an embedded native terminal, built for developers who live in terminals. Written in Rust with wry (WebKitGTK) for web rendering and egui for the UI overlay.

## Features

### Core

- **Vim-style keybindings** — modal editing (Normal, Insert, Command) with `h/j/k/l` navigation
- **Tiling pane management** — BSP tree layout with vertical/horizontal splits, pane navigation, and close
- **Command palette** — `Ctrl+P` fuzzy search (Nucleo engine) across history, bookmarks, commands, and custom Lua commands
- **Lua scripting** — `init.lua` for custom keybindings, commands, URL redirect rules, and themes
- **Keybinding customization** — override default keybindings via `config.toml` `[keybindings]` section
- **Workspace persistence** — save/restore pane layouts with URLs via `:ws-save` / `:ws-load`; auto-save every 30s for crash recovery
- **Tab management** — `:tab-restore` to reopen closed tabs, `:tab-unload` to free LRU background pane, `:tabs` to search open tabs
- **Help panel** — `:help` shows full keybinding reference with collapsible sections
- **Crash recovery** — `:crash-reload` reloads a pane after web content crash
- **MCP bridge** — built-in Model Context Protocol server (stdio transport) so LLMs can browse the web
- **ARP (Aileron Remote Protocol)** — WebSocket server for mobile clients, tab sync, clipboard sharing
- **Sync protocol** — WebDAV + E2EE specification ready for implementation
- **Cross-platform** — PlatformOps trait for Linux, macOS, Windows

### Internationalization

- **9 languages** — EN, ZH, JA, KO, DE, FR, ES, PT, RU with `:language` command
- **Runtime switching** — `:language <code>` changes UI language instantly; `:language-list` shows available languages

### Accessibility

- **ARIA labels** — all UI chrome elements include ARIA labels for screen reader support

### Browsing

- **Ad blocking** — EasyList parser with network + cosmetic CSS rules, `$redirect`/`$important`/`$badfilter` support, filter list auto-update, per-site toggle (`:adblock-toggle`, `:adblock-update`)
- **HTTPS upgrade** — auto-upgrade HTTP for known-safe domains (EasyList HTTPS list)
- **Tracking protection** — blocks known tracker domains, sends DNT/GPC headers, strict referrer policy
- **Popup blocker** — blocks unwanted `window.open()` calls, configurable via `:popups`
- **Cookie management** — view (`:cookies`), clear (`:cookies-clear`), per-site allow/block
- **Per-site settings** — zoom, adblock, JS, cookies, autoplay per domain (exact/wildcard/regex patterns)
- **Download manager** — progress tracking, open file, open directory
- **Browser import** — `:import-firefox` / `:import-chrome` for bookmarks and history
- **Multiple search engines** — pre-configured Google, DuckDuckGo, GitHub, YouTube, Wikipedia; switch with `:engine <name>`
- **Engine selection** — choose rendering engine (auto/servo/webkit) with `:engine auto|servo|webkit`
- **Adaptive quality** — auto-reduces rendering quality when over frame budget (`:adaptive-quality`)
- **Custom CSS** — inject custom stylesheets via config or `aileron://settings`
- **Link hints** — press `f` to reveal clickable hints on all links, type digits to follow
- **Find in page** — `Ctrl+F` incremental search with next/previous navigation
- **Smooth scrolling** — native smooth scroll behavior
- **Print support** — `:print` triggers system print dialog
- **PDF viewer** — `:pdf <path>` opens PDFs via system viewer (WebEngine PDF planned)
- **Reader mode** — `Ctrl+Shift+R` strips CSS and extracts article text; toggle per pane
- **Minimal mode** — `Ctrl+Shift+M` hides images/media and removes scripts; toggle per pane

### Terminal

- **Native terminal** — press `` ` `` to open a terminal pane using `alacritty_terminal` + `portable_pty` + egui rendering (no xterm.js, ~2-5MB/pane)
- **Terminal mouse selection** — click and drag to select text, copy to clipboard
- **Visual bell** — terminal bell triggers a 200ms visual flash
- **SSH quick-connect** — `ssh user@host` in command palette opens terminal pane and auto-connects
- **Terminal search** — `:terminal-search <pattern>` searches scrollback buffer

### Tabs & Panes

- **Tab system** — sidebar (default), topbar, or none; shows title, URL, pane type, audio/mute/pin indicators
- **Tab pinning** — `Ctrl+Shift+P` or `:pin` pins a pane (prevents accidental close)
- **Audio mute** — `:mute` / `:unmute` to silence a pane
- **Pane swap** — `:swap` swaps URLs between active and previously active pane
- **Detach pane** — `Ctrl+Shift+D` detaches pane to standalone popup window
- **Popup windows** — `Ctrl+N` opens standalone webview (no tiling, no egui overlay)

### Theming

- **7 built-in themes** — dark, light, gruvbox-dark, nord, dracula, solarized-dark, solarized-light
- **Custom themes** — define themes in `config.toml` under `[themes.<name>]`
- **Runtime switching** — `:theme <name>` to switch instantly

### Privacy & Settings

- **Settings GUI** — `aileron://settings` page for keyboard-navigable configuration
- **Privacy dashboard** — `:privacy` shows HTTPS upgrade, tracking protection, adblock status
- **Bitwarden integration** — `bw-unlock` / `bw-search` / `bw-autofill` / `bw-detect` for credential management
- **Password manager** — built-in credential storage with OAuth detection, multi-step login flow support (`:credentials`, `:credentials-save`)

### Developer Tools

- **DevTools** — `F12` opens WebKit inspector
- **Network request log** — `Ctrl+Shift+N` or `:network` shows intercepted fetch/XHR with status codes
- **Console capture** — `Ctrl+Shift+J` or `:console` shows console.log/warn/error output
- **Git integration** — status bar shows branch with dirty indicator; `:gs`, `:gl`, `:gd`, `:grep`
- **Project search** — `:grep <pattern>` via ripgrep/grep
- **File browser** — `:files` opens project root (auto-detects git root)
- **Shell commands** — `:! <command>` runs shell commands from the command line

### Scripting & Extensibility

- **Content scripts** — `.lua` files in `~/.config/aileron/scripts/` with `@match` and `@match-regexp` patterns, `@run-at` (document_start/end/idle)
- **Lua hooks** — `aileron.on("navigate", fn)` and `aileron.on("mode_change", fn)` in init.lua
- **Command chaining** — `:open github.com && mg` chains ex-commands
- **Did-you-mean** — fuzzy Levenshtein suggestions for mistyped commands
- **Custom protocols** — `aileron://` for internal pages (welcome, settings, files, error)
- **WebExtensions** — 6 API traits, extension loading from disk, `:extensions`/`:extension-load`/`:extension-info` commands

## Prerequisites

- **Linux** (x86_64) — tested on CachyOS (Wayland + NVIDIA), should work on any distro with WebKitGTK 4.1
- **macOS** (aarch64, x86_64) — WebKit built-in; requires Xcode command line tools
- **Windows** (x86_64) — WebView2 (Edge); experimental, see `com.github.WyattAu.aileron.yaml` for Flatpak parity
- **Vulkan-capable GPU** — required by wgpu for egui rendering
- **Rust toolchain** — `rustc`, `cargo`, `pkg-config`

## Build

```bash
cargo build

# Run (with runtime library path for WebKitGTK)
LD_LIBRARY_PATH="/usr/lib:$LD_LIBRARY_PATH" ./target/debug/aileron
```

## Test

```bash
# Unit tests (806 tests)
cargo test --lib -- --test-threads=4

# Integration tests
cargo test --test integration_smoke

# All tests + clippy (zero warnings)
cargo clippy --lib -- -D warnings
cargo test -- --test-threads=4
```

## Install

### Cargo

```bash
cargo install --path .
```

### AUR

```bash
paru -S aileron-git
```

### Flatpak (experimental)

A Flatpak manifest is provided at `com.github.WyattAu.aileron.yaml`:

```bash
flatpak-builder build-dir com.github.WyattAu.aileron.yaml --force-clean
flatpak-builder --user --install build-dir com.github.WyattAu.aileron.yaml
flatpak run com.github.WyattAu.aileron
```

> **Note:** Flatpak support is experimental. Targets Freedesktop 23.08 with Rust and LLVM SDK extensions.

## Key Bindings

| Key | Mode | Action |
|-----|------|--------|
| `i` | Normal | Enter Insert mode |
| `Esc` | Insert/Command | Return to Normal mode |
| `Ctrl+P` | Normal | Open command palette |
| `Ctrl+W` | Normal | Split vertical |
| `Ctrl+S` | Normal | Split horizontal |
| `q` | Normal | Close pane |
| `Ctrl+H/J/K/L` | Normal | Navigate panes |
| `Ctrl+Alt+H/J/K/L` | Normal | Resize pane (shrink/grow in direction) |
| `j` / `k` | Normal | Scroll down/up |
| `Ctrl+D` / `Ctrl+U` | Normal | Scroll half page down/up |
| `H` / `L` | Normal | Go back / forward |
| `r` | Normal | Reload page |
| `Ctrl+B` | Normal | Toggle bookmark |
| `Ctrl+F` | Normal | Find in page |
| `f` | Normal | Toggle link hints |
| `Ctrl+E` | Normal | Open in system browser |
| `F12` | Normal | Toggle dev tools |
| `y` | Normal | Copy current URL to clipboard |
| `` ` `` | Normal | Open terminal pane |
| `Ctrl+=` / `Ctrl+-` / `Ctrl+0` | Normal | Zoom in / out / reset |
| `Ctrl+N` | Normal | Open popup window (standalone webview) |
| `Ctrl+Shift+R` | Normal | Toggle reader mode |
| `Ctrl+Shift+M` | Normal | Toggle minimal mode |
| `Ctrl+Shift+N` | Normal | Show network request log |
| `Ctrl+Shift+J` | Normal | Show captured console output |
| `Ctrl+Shift+D` | Normal | Detach pane to popup window |
| `Ctrl+Shift+P` | Normal | Pin/unpin active pane |
| `m` + letter | Normal | Set a scroll mark (a-z) |
| `'` + letter | Normal | Jump to mark (a-z) |

## Commands

| Command | Description |
|---------|-------------|
| `:open <url>` | Navigate to URL |
| `:help` / `:?` | Show keybinding reference panel |
| `:! <command>` | Run shell command, show output |
| `:set <key> <value>` | Change config at runtime (search_engine, homepage, adblock, https_upgrade, tracking_protection, popup_blocker) |
| `:m<letter> <url>` | Set quickmark |
| `:g<letter>` | Go to quickmark |
| `:swap` / `:tab-swap` | Swap URLs between active and previously active pane |
| `:only` | Close all panes except current |
| `:back` / `:forward` / `:reload` | History navigation |
| `:reader` / `:minimal` | Toggle reader/minimal mode |
| `:settings` | Open aileron://settings |
| `:site-settings` | View per-site settings for current URL |
| `:site-settings set <key> <value>` | Set per-site setting (zoom, adblock, js, cookies, autoplay) |
| `:site-settings list` | List all per-site settings |
| `:site-settings delete <id>` | Delete a site setting |
| `:site-settings clear <domain>` | Clear all settings for a domain |
| `:theme <name>` | Switch theme |
| `:theme list` | List available themes |
| `:pin` | Pin/unpin active pane |
| `:mute` / `:unmute` | Mute/unmute active pane |
| `:print` | Print current page |
| `:pdf <path>` | Open PDF |
| `:popups [on\|off]` | Toggle popup blocker |
| `:cookies` | View cookies for current site |
| `:cookies-clear` | Clear cookies for current pane |
| `:cookies-block <domain>` | Block cookies for a domain |
| `:cookies-allow <domain>` | Allow cookies for a domain |
| `:adblock-toggle` | Toggle adblock for current site |
| `:adblock-count` | Show blocked request count |
| `:privacy` | Show privacy settings (HTTPS, tracking, adblock) |
| `:https-toggle` | Check HTTPS upgrade status for current domain |
| `:engine <name>` | Switch search engine (google, ddg, gh, yt, wiki) |
| `:bw-unlock <password>` | Unlock Bitwarden vault |
| `:bw-search <query>` | Search vault for credentials |
| `:bw-autofill` | Auto-fill credentials for current site |
| `:bw-detect` | Detect login forms on current page |
| `:bw-lock` | Lock Bitwarden vault |
| `:network` / `:network-clear` | Show/clear network request log |
| `:console` / `:console-clear` | Show/clear console output |
| `:downloads` / `:downloads-clear` | Show/clear download history |
| `:downloads-open [id]` | Open downloaded file |
| `:downloads-dir` | Open downloads directory |
| `:import-firefox` / `:import-chrome` | Import bookmarks/history from browser |
| `:inspect` | Open WebKit inspector |
| `:proxy <url>` | Set proxy (socks5://, http://) |
| `:clear history\|bookmarks\|workspaces\|cookies\|all` | Clear stored data |
| `:grep <pattern>` | Search project with ripgrep/grep |
| `:gs` / `:gl` / `:gd` | Git status/log/diff |
| `:files` / `:browse` | Open file browser |
| `:ssh <host>` | SSH in terminal pane |
| `:scripts` | List loaded content scripts |
| `:config-save` | Save config to disk |
| `:terminal-clear` / `:cls` | Clear terminal pane |
| `:terminal-search <pattern>` | Search terminal scrollback |
| `:layout-save <name>` / `:layout-load <name>` | Save/load window layout preset |
| `:ws-save <name>` / `:ws-load <name>` / `:ws-list` | Workspace management |
| `:cmd1 && :cmd2` | Chain commands |
| `:extensions` | List loaded extensions |
| `:extension-load <path>` | Load extension from path |
| `:extension-info <id>` | Show extension details |
| `:language <code>` | Set UI language (en, zh, ja, ko, de, fr, es, pt, ru) |
| `:language-list` | List available languages |
| `:engine auto\|servo\|webkit` | Select rendering engine |
| `:compat-override add\|remove\|list` | Manage compatibility overrides |
| `:adaptive-quality` | Toggle adaptive quality rendering |
| `:adblock-update` | Update adblock filter lists |
| `:credentials` | Manage stored credentials |
| `:credentials-save` | Save credentials for current site |
| `:perf` | Show performance overlay |
| `:perf-on` / `:perf-off` | Toggle performance overlay |
| `:memory` | Show memory usage statistics |
| `:tab-restore [n]` | Restore recently closed tab |
| `:tab-unload` | Unload least-recently-used background pane |
| `:crash-reload` | Reload pane after web content crash |
| `:replace <old> <new> [case]` | Find and replace in page content |
| `:arp-start` / `:arp-stop` / `:arp-status` | Start/stop/status of Aileron Remote Protocol |

## Configuration

### config.toml

Aileron looks for `~/.config/aileron/config.toml`:

```toml
# ── General ──────────────────────────────────────
homepage = "aileron://welcome"         # Start page URL
window_width = 1280                    # Default window width (px)
window_height = 800                   # Default window height (px)
devtools = false                      # Enable WebKit devtools

# ── Privacy ─────────────────────────────────────
adblock_enabled = true                # Block ads from filter lists
adblock_filter_lists = ["https://easylist.to/easylist/easylist.txt"]
adblock_cosmetic_filtering = true     # Hide ad elements via CSS
adblock_update_interval_hours = 24    # Filter list update frequency
https_upgrade_enabled = true          # Auto-upgrade HTTP to HTTPS
tracking_protection_enabled = true    # Block tracker domains + DNT headers
popup_blocker_enabled = true          # Block unwanted window.open()

# ── Appearance ──────────────────────────────────
theme = "dark"                        # "dark" | "light" | custom name
tab_layout = "sidebar"                # "sidebar" | "topbar" | "none"
tab_sidebar_width = 180.0             # Sidebar width in pixels
tab_sidebar_right = false             # Sidebar on right side
language = "en"                       # UI language (ISO 639-1)
custom_css = ""                       # Inline CSS or path to CSS file

# ── Rendering ──────────────────────────────────
render_mode = "offscreen"             # "offscreen" (texture) | "native" (window)
engine_selection = "auto"             # "auto" | "webkit" | "servo"
adaptive_quality = true               # Reduce frame rate when slow

# ── Search ──────────────────────────────────────
search_engine = "https://duckduckgo.com/?q={query}"
palette_max_results = 20              # Command palette result limit

# ── Session ─────────────────────────────────────
restore_session = false               # Restore last workspace on startup
auto_save = true                      # Auto-save workspace for crash recovery
auto_save_interval = 30               # Auto-save interval (seconds)
init_lua_path = ""                    # Path to custom init.lua

# ── Proxy ───────────────────────────────────────
# proxy = "socks5://127.0.0.1:1080"

# ── Sync ────────────────────────────────────────
sync_target = ""                      # SSH target or local path
sync_encrypted = false                # E2EE for sync
sync_auto = false                     # Auto-sync via filesystem watcher
sync_auto_interval_sec = 300          # Auto-sync interval

# ── ARP (Remote Protocol) ──────────────────────
arp_port = 19743                      # WebSocket server port

# ── Keybinding overrides (applied on top of defaults) ──
[keybindings]
# Format: "<key>" = "<Action>"
# Keys: crossterm notation — j, <C-p>, <A-S>, <C-S-i>, <F1>-<F12>
# Actions: PascalCase or shorthand (ScrollDown, vs, sp, Hints)
# "k" = "ScrollUp"
# "<C-S-k>" = "ScrollUp"

# ── Per-domain engine overrides ────────────────
[compat_overrides]
# "example.com" = "webkit"

# ── Custom themes ──────────────────────────────
[themes.mytheme]
bg = "#1a1a2e"
fg = "#e0e0e0"
accent = "#4db4ff"
```

### init.lua

```lua
aileron.keymap.set("normal", "Ctrl+Shift+R", "reload")

aileron.cmd.create("open-rust", "Open Rust documentation", function()
    aileron.navigate("https://doc.rust-lang.org")
end)

aileron.url.add_redirect("github.com", "ghproxy.com")

aileron.on("navigate", function(url)
    print("Navigating to: " .. url)
end)
```

### Content Scripts

Place `.lua` files in `~/.config/aileron/scripts/`:

```lua
-- ==UserScript==
-- @name        Dark Mode for GitHub
-- @match       https://*.github.com/*
-- @match-regexp    ^https://.*\.github\.com/.*
-- @run-at      document_idle
-- @grant       none
-- ==/UserScript==

return [[
  document.body.style.background = '#1a1a1a';
  document.body.style.color = '#d4d4d4';
]]
```

Use `:scripts` to list loaded scripts.

## Architecture

```
src/
  main.rs              — Event loop, wry pane management, egui UI
  app/mod.rs           — AppState (mode machine, palette, keybindings, dispatch bridge)
  app/dispatch.rs      — Pure action dispatch (Action → ActionEffect)
  app/commands.rs      — Ex-command handler (60+ commands)
  app/events.rs        — Key event routing, panel keyboard navigation
  input/               — Key mapping, mode transitions, keybinding registry with config overrides
  wm/                  — BSP tree (tiling), rectangle math, pane metadata
  servo/               — PaneStateManager, PaneRenderer trait, WryPaneManager, ServoPane (skeleton)
  terminal/            — Native terminal (alacritty_terminal + portable_pty + egui painter)
  ui/                  — Command palette (Nucleo fuzzy search), panels, omnibox, find bar, help panel
  db/                  — SQLite: history, bookmarks, workspaces, downloads, site_settings
  lua/                 — Lua sandbox, API bindings (cmd, keymap, theme, url)
  arp/                 — Aileron Remote Protocol (WebSocket server, tab sync, clipboard sharing)
  mcp/                 — MCP JSON-RPC server, tools, stdio transport
  net/                 — AdBlocker (EasyList parser), filter_list (network/cosmetic), privacy (HTTPS upgrade, tracking protection)
  downloads/           — Download manager (async, resume, progress tracking)
  extensions/          — WebExtensions support (6 API traits, extension loading, lifecycle)
  i18n/                — Internationalization (9 languages, locale resolution, message catalog)
  passwords/           — Password manager (credential storage, OAuth detection, multi-step login flows, Bitwarden client)
  platform/            — PlatformOps trait, Linux/macOS/Windows platform implementations
  scripts/             — Content script manager (Lua → JS injection, @match/@match-regexp, @run-at)
  gfx/                 — wgpu surface + egui renderer setup
  git.rs               — Git repo detection, status bar integration
  popup.rs             — Standalone popup window management
  frame_tasks.rs       — Frame-level task execution, auto-save, ARP sync
  offscreen_webview.rs — Offscreen webview texture compositing
  wry_actions.rs       — WryAction queue utilities
  sync.rs              — Sync protocol specification (WebDAV + E2EE)
  profiling/           — Memory profiling, performance monitoring
tests/
  integration_smoke.rs — Cross-module integration tests
```

### Action Dispatch Pattern

```
Key event → AppState.process_key_event()
         → KeybindingRegistry.lookup() → Action
         → dispatch_action(Action) → Vec<ActionEffect>
         → execute_action() applies effects to AppState
         → WryAction queue consumed by main.rs
```

## License

MIT
