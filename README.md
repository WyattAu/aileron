# Aileron

A keyboard-driven, tiling web environment for developers. Built in Rust with wry (WebKitGTK) for web rendering and egui for the UI overlay.

## Features

- **Vim-style keybindings** — modal editing (Normal, Insert, Command) with `h/j/k/l` navigation
- **Tiling pane management** — BSP tree layout with vertical/horizontal splits, pane navigation, and close
- **Command palette** — `Ctrl+P` fuzzy search across history, bookmarks, commands, and custom Lua commands
- **Lua scripting** — `init.lua` for custom keybindings, commands, URL redirect rules, and themes
- **Workspace persistence** — save/restore pane layouts with URLs via `:ws-save` / `:ws-load`
- **MCP bridge** — built-in Model Context Protocol server (stdio transport) so LLMs can browse the web
- **Ad blocking** — domain blocking + cosmetic CSS rules (no external extension needed)
- **Link hints** — press `f` to reveal clickable hints on all links, type digits to follow
- **Find in page** — `Ctrl+F` incremental search with next/previous navigation
- **Bitwarden integration** — `bw-unlock` / `bw-search` to autofill credentials from your vault
- **Developer tools** — `F12` to open WebKit inspector on the active pane
- **Embedded terminal** — press `` ` `` to open a terminal pane with full PTY, rendered via xterm.js with Aileron dark theme, resizes with pane repositioning
- **File browser** — `files` or `browse` in the command palette (or `:files` in ex-command mode); navigate with `j`/`k`, Enter to open, Backspace to go up
- **Git integration** — status bar shows current branch with dirty indicator (yellow) and modified file count, auto-detected from cwd
- **SSH quick-connect** — `ssh user@host` in the command palette or `:ssh user@host` in ex-command mode; opens a terminal pane and auto-connects
- **Web search** — search from the command palette (`Ctrl+P`, type a query and press Enter) or from the new tab page search bar; uses DuckDuckGo by default, configurable via `search_engine` in config.toml
- **Tab system** — switch between open panes via a sidebar (default) or topbar; sidebar shows page title, URL, and pane type (🌐 web / ⌨ terminal), with × to close and drag to resize
- **Vim-style marks** — `m` then a letter (a-z) to set a mark on the current URL, `'` then a letter to jump back to it
- **Command chaining** — use `&&` to chain ex-commands: `:open github.com && m g`
- **Pane swap** — `:swap` or `:tab-swap` swaps URLs between the active and previously active pane
- **Lua hooks** — `aileron.on("navigate", fn(url) ...)` and `aileron.on("mode_change", fn(mode) ...)` in init.lua
- **Popup windows** — `Ctrl+N` opens a standalone webview window (no tiling, no egui overlay) for content that needs its own window
- **Reader mode** — `Ctrl+Shift+R` strips CSS and extracts article text for clean reading; toggle per pane
- **Minimal mode** — `Ctrl+Shift+M` hides images/media and removes scripts for lightweight browsing; toggle per pane
- **Auto-save workspace** — saves layout every 30 seconds for crash recovery; auto-restores on startup when `restore_session = true`
- **Omnibox URL bar** — click the URL bar or focus it to search across bookmarks, history, and search engines as you type
- **Content scripts** — place `.lua` files in `~/.config/aileron/scripts/` with `@match` URL patterns to inject custom JavaScript on matching pages (like Greasemonkey)
- **Multiple search engines** — pre-configured Google, DuckDuckGo, GitHub, YouTube, Wikipedia; switch with `:engine <name>`
- **Detach pane** — `Ctrl+Shift+D` detaches the current pane to a standalone popup window
- **Clear browsing data** — `:clear history|bookmarks|workspaces|all` to clear stored data
- **Network request log** — `Ctrl+Shift+N` or `:network` shows intercepted fetch/XHR requests with status codes
- **Console capture** — `Ctrl+Shift+J` or `:console` shows captured console.log/warn/error output
- **Error recovery** — pane crashes show an error page instead of killing the app
- **Config migration** — config format changes are handled automatically; old configs get upgraded
- **Download history** — `:downloads` shows recent downloads, tracked in database
- **Proxy support** — configure SOCKS5/HTTP proxy in config or `:proxy <url>` command
- **Cookie management** — `:cookies-clear` to clear cookies for the active pane
- **Project search** — `:grep <pattern>` searches codebase via ripgrep, falls back to grep
- **Git integration** — `:git-status`/`:gs`, `:git-log`/`:gl`, `:git-diff`/`:gd` for quick git info
- **File browser** — `:files` opens project root (auto-detects git root); click files to open via `xdg-open`
- **Terminal search** — `:terminal-search <pattern>` searches scrollback buffer
- **Layout presets** — `:layout-save <name>` and `:layout-load <name>` for window arrangements

## Prerequisites

- **Nix** (with flakes enabled) — [install guide](https://nixos.org/download)
- **Linux** (x86_64) — tested on CachyOS (Wayland + NVIDIA), should work on any distro with WebKitGTK 4.1
- **Vulkan-capable GPU** — required by wgpu for egui rendering

## Build

```bash
# Enter the Nix dev shell (installs all build dependencies)
nix develop

# Build
cargo build

# Run (with runtime library path for WebKitGTK)
LD_LIBRARY_PATH="/usr/lib:$LD_LIBRARY_PATH" ./target/debug/aileron
```

## Test

```bash
# Unit tests (307 tests)
nix develop --command cargo test --lib -- --test-threads=4

# Integration tests (26 tests)
nix develop --command cargo test --test integration_smoke

# All tests + clippy (zero warnings)
nix develop --command cargo clippy --lib -- -D warnings
nix develop --command cargo test -- --test-threads=4
```

## Install

### Nix (recommended)

```bash
nix build
./result/bin/aileron
```

The Nix build produces a wrapper script that sets `LD_LIBRARY_PATH`, `WINIT_UNIX_BACKEND`, and `VK_ICD_FILENAMES` automatically.

### AUR

```bash
# Build from source (uses nix)
paru -S aileron-git
```

### Flatpak (experimental)

A Flatpak manifest is provided at `com.github.WyattAu.aileron.yaml`. Build with:

```bash
flatpak-builder build-dir com.github.WyattAu.aileron.yaml --force-clean
flatpak-builder --user --install build-dir com.github.WyattAu.aileron.yaml
flatpak run com.github.WyattAu.aileron
```

> **Note:** Flatpak support is experimental. The manifest targets Freedesktop 23.08 with Rust and LLVM SDK extensions.

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
| `Ctrl+=` | Normal | Zoom in |
| `Ctrl+-` | Normal | Zoom out |
| `Ctrl+0` | Normal | Reset zoom |
| `Ctrl+N` | Normal | Open popup window (standalone webview) |
| `Ctrl+Shift+R` | Normal | Toggle reader mode (extract article text) |
| `Ctrl+Shift+M` | Normal | Toggle minimal mode (hide images/scripts) |
| `Ctrl+Shift+N` | Normal | Show network request log |
| `Ctrl+Shift+J` | Normal | Show captured console output |
| `Ctrl+Shift+D` | Normal | Detach pane to popup window |
| `files` / `browse` | Palette | Open file browser |
| `ssh <host>` | Palette | SSH to remote host |
| `:url` | Palette | Navigate to URL |
| `:ws-save <name>` | Palette | Save workspace |
| `:ws-load <name>` | Palette | Restore workspace |
| `:ws-list` | Palette | List saved workspaces |
| `:files` / `:browse` | Ex-command | Open file browser |
| `:ssh <host>` | Ex-command | SSH connection in terminal pane |
| `:open <url>` | Ex-command | Navigate to a URL explicitly |
| `:! <command>` | Ex-command | Run shell command, show output in status bar |
| `:set <key> <value>` | Ex-command | Change config at runtime (search_engine, homepage, adblock) |
| `:m<letter> <url>` | Ex-command | Set a quickmark (e.g., `:mg https://github.com`) |
| `:g<letter>` | Ex-command | Go to quickmark (e.g., `:gg` opens the URL saved in mark `g`) |
| `m` + letter | Normal | Set a mark on current pane URL (a-z) |
| `'` + letter | Normal | Jump to mark (a-z) |
| `:swap` / `:tab-swap` | Ex-command | Swap URLs between active and previously active pane |
| `:cmd1 && :cmd2` | Ex-command | Chain two ex-commands (e.g., `:open github.com && m g`) |
| `:reader` | Ex-command | Toggle reader mode (strip CSS, show article text) |
| `:minimal` | Ex-command | Toggle minimal mode (hide images, remove scripts) |
| `:network` | Ex-command | Show intercepted network requests |
| `:network-clear` | Ex-command | Clear network request log |
| `:console` | Ex-command | Show captured console output |
| `:console-clear` | Ex-command | Clear console capture |
| `:scripts` | Ex-command | List loaded content scripts |
| `:config-save` | Ex-command | Save current config to disk |
| `:only` | Ex-command | Close all panes except current |
| `:back` | Ex-command | Go back in history |
| `:forward` | Ex-command | Go forward in history |
| `:reload` | Ex-command | Reload current page |
| `:engine <name>` | Ex-command | Switch search engine (google, ddg, gh, yt, wiki) |
| `:clear history` | Ex-command | Clear browsing history |
| `:clear bookmarks` | Ex-command | Clear all bookmarks |
| `:clear workspaces` | Ex-command | Clear all saved workspaces |
| `:clear all` | Ex-command | Clear all stored data |
| `:downloads` | Ex-command | Show recent downloads |
| `:downloads-clear` | Ex-command | Clear download history |
| `:cookies-clear` | Ex-command | Clear cookies for current pane |
| `:inspect` | Ex-command | Open WebKit inspector |
| `:proxy <url>` | Ex-command | Set proxy (socks5://, http://) |
| `:grep <pattern>` | Ex-command | Search project with ripgrep/grep |
| `:git-status` / `:gs` | Ex-command | Show git status |
| `:git-log` / `:gl` | Ex-command | Show recent commits |
| `:git-diff` / `:gd` | Ex-command | Show diff summary |
| `:terminal-clear` / `:cls` | Ex-command | Clear terminal pane |
| `:terminal-search <pat>` | Ex-command | Search terminal scrollback |
| `:layout-save <name>` | Ex-command | Save window layout preset |
| `:layout-load <name>` | Ex-command | Load window layout preset |

## Configuration

### Config file

Aileron looks for `~/.config/aileron/config.toml`:

```toml
homepage = "https://example.com"
window_width = 1280
window_height = 800
adblock_enabled = true
palette_max_results = 20
search_engine = "https://duckduckgo.com/?q={query}"

# Tab bar layout: "sidebar", "topbar", or "none"
tab_layout = "sidebar"

# Tab sidebar width in pixels (sidebar layout only)
tab_sidebar_width = 180.0

# Show tab sidebar on the right side instead of left
tab_sidebar_right = false

# Search engines — use :engine <name> to switch
# Pre-configured: google, ddg, gh, yt, wiki

# Auto-save workspace for crash recovery
auto_save = true
auto_save_interval = 30

# Proxy URL (supports http, https, socks5)
# proxy = "socks5://127.0.0.1:1080"
```

### init.lua

Place `~/.config/aileron/init.lua` for custom keybindings and commands:

```lua
-- Custom keybinding
aileron.keymap.set("normal", "Ctrl+Shift+R", "reload")

-- Custom command (appears in palette with category "Custom")
aileron.cmd.create("open-rust", "Open Rust documentation", function()
    aileron.navigate("https://doc.rust-lang.org")
end)

-- URL redirect rule (case-insensitive host matching)
aileron.url.add_redirect("github.com", "ghproxy.com")

-- Lifecycle hooks
aileron.on("navigate", function(url)
    print("Navigating to: " .. url)
end)

aileron.on("mode_change", function(mode)
    print("Mode changed to: " .. mode)
end)
```

### Content Scripts

Place `.lua` files in `~/.config/aileron/scripts/` to inject JavaScript on matching pages:

```lua
-- ==UserScript==
-- @name        Dark Mode for GitHub
-- @match       https://*.github.com/*
-- @grant       none
-- ==/UserScript==

return [[
  document.body.style.background = '#1a1a1a';
  document.body.style.color = '#d4d4d4';
]]
```

The `@match` patterns use `*` as a wildcard. Scripts are loaded at startup and injected automatically on matching page loads. Use `:scripts` to list loaded scripts.

## Architecture

```
src/
  main.rs          — Event loop, wry pane management, egui UI
  app/mod.rs       — AppState (mode machine, palette, keybindings, dispatch bridge)
  app/dispatch.rs  — Pure action dispatch (Action → ActionEffect), 36 tests
  input/           — Key mapping, mode transitions, keybinding registry
  wm/              — BSP tree (tiling), rectangle math, pane metadata
  servo/           — WryPaneManager (wry webview), PaneRenderer trait, PaneState (metadata), xterm.js terminal (PTY + IPC bridge)
  ui/              — Command palette, fuzzy search, search items
  db/              — SQLite: history, bookmarks, workspaces
  lua/             — Lua sandbox, API bindings (cmd, keymap, theme, url)
  mcp/             — MCP JSON-RPC server, tools, stdio transport
  net/             — Ad blocker (domain blocking + cosmetic CSS)
  scripts/         — Content script manager (Lua → JS injection, @match URL patterns)
  gfx/             — wgpu surface + egui renderer setup
  passwords/       — Bitwarden client (credential fetch + autofill JS)
  config.rs        — Config struct (TOML)
tests/
  integration_smoke.rs  — 26 cross-module integration tests
```

### Action Dispatch Pattern

All user input flows through a pure dispatch function:

```
Key event → AppState.process_key_event()
         → KeybindingRegistry.lookup() → Action
         → dispatch_action(Action) → Vec<ActionEffect>
         → execute_action() applies effects to AppState
         → WryAction queue consumed by main.rs
```

## License

MIT
