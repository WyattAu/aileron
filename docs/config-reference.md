# Aileron Configuration Reference

Aileron reads configuration from a TOML file. If no file exists, all values fall back to built-in defaults. The config is loaded at startup, auto-migrated to the current schema version, and saved back to disk after migration.

## Config File Locations

| Platform | Config File | Fallback |
|----------|-------------|----------|
| Linux | `$XDG_CONFIG_HOME/aileron/config.toml` | `~/.config/aileron/config.toml` |
| macOS | `$HOME/Library/Application Support/Aileron/config.toml` | |
| Windows | `%APPDATA%\Aileron\config.toml` | `~\AppData\Roaming\Aileron\config.toml` |

The `directories` crate resolves these paths via `ProjectDirs::from("com", "aileron", "Aileron")`.

### Data and Cache Directories

| Platform | Data Dir | Cache Dir |
|----------|----------|-----------|
| Linux | `~/.local/share/aileron/` | `~/.cache/aileron/` |
| macOS | `~/Library/Application Support/Aileron/` | `~/Library/Caches/Aileron/` |
| Windows | `%LOCALAPPDATA%\Aileron\` | `%LOCALAPPDATA%\Aileron\Cache\` |

The data directory stores the workspace database, logs, and the debug log. The init.lua script lives in the config directory by default.

## Environment Variables

| Variable | Description | Platforms |
|----------|-------------|-----------|
| `AILERON_TESTING` | When set, disables native file open dialogs (returns `None`). Used in CI/headless environments. | Linux |
| `AILERON_SPELLCHECK` | Controls WebKitGTK spell checking. Set to `0` or `false` to disable. Defaults to enabled. | Linux |
| `AILERON_DEBUG` | Enables extended debug mode. Affects HTTPS safe list caching behavior and activates the debug capturer. | All |
| `AILERON_DEBUG_FILE` | Overrides the debug log file path. Defaults to `<data_dir>/debug.log`. | All |
| `all_proxy` | Set automatically at startup from the `proxy` config option. Not intended for manual use. | All |
| `RUST_LOG` | Tracing log filter. Default: `aileron=debug,wgpu=warn,wry=debug,webkit2gtk=debug,gdk=debug,gtk=debug,egui=info` | All |

## General Options

### `homepage`

- **Type:** string
- **Default:** `"aileron://welcome"`
- **Description:** URL loaded in the first pane when Aileron starts.
- **Example:**
  ```toml
  homepage = "https://duckduckgo.com"
  ```

### `window_width`

- **Type:** integer
- **Default:** `1280`
- **Description:** Initial window width in logical pixels.
- **Range:** Positive integer.
- **Example:**
  ```toml
  window_width = 1920
  ```

### `window_height`

- **Type:** integer
- **Default:** `800`
- **Description:** Initial window height in logical pixels.
- **Range:** Positive integer.
- **Example:**
  ```toml
  window_height = 1080
  ```

### `devtools`

- **Type:** boolean
- **Default:** `false`
- **Description:** Enable web developer tools in webview panes.
- **Example:**
  ```toml
  devtools = true
  ```

### `config_version`

- **Type:** integer
- **Default:** `2`
- **Description:** Schema version for automatic migration. Do not set manually. Aileron upgrades older configs on load.
- **Range:** Positive integer.

## Rendering

### `render_mode`

- **Type:** string (enum)
- **Default:** `"offscreen"`
- **Description:** Controls how webview content is composited into the egui UI.
  - `"offscreen"` -- Architecture B. WebViews render into offscreen GTK buffers; pixels are captured and uploaded as egui textures each frame. Default on Linux and macOS.
  - `"native"` -- Wry creates visible child windows (XWayland on Wayland). Default on Windows.
- **Platform overrides:** Windows forces `"native"` by default.
- **Example:**
  ```toml
  render_mode = "offscreen"
  ```

### `adaptive_quality`

- **Type:** boolean
- **Default:** `true`
- **Description:** Enable adaptive quality rendering. Reduces texture capture rate and skips non-active background panes when frame times are high.
- **Example:**
  ```toml
  adaptive_quality = true
  ```

## Privacy and Security

### `adblock_enabled`

- **Type:** boolean
- **Default:** `true`
- **Description:** Enable network-level ad blocking. Blocks requests to domains found in filter lists.
- **Example:**
  ```toml
  adblock_enabled = true
  ```

### `adblock_filter_lists`

- **Type:** array of strings
- **Default:**
  ```toml
  adblock_filter_lists = [
      "https://easylist.to/easylist/easylist.txt",
      "https://easylist.to/easylist/easyprivacy.txt",
  ]
  ```
- **Description:** URLs of filter lists in EasyList format. Downloaded on startup and periodically.
- **Example:**
  ```toml
  adblock_filter_lists = [
      "https://easylist.to/easylist/easylist.txt",
      "https://easylist.to/easylist/easyprivacy.txt",
      "https://pgl.yoyo.org/aserverlist.php?hostformat=adblock&showintro=1&mimetype=plaintext",
  ]
  ```

### `adblock_update_interval_hours`

- **Type:** integer
- **Default:** `24`
- **Description:** How often to re-download filter lists in the background, in hours.
- **Range:** Positive integer.
- **Example:**
  ```toml
  adblock_update_interval_hours = 12
  ```

### `adblock_cosmetic_filtering`

- **Type:** boolean
- **Default:** `true`
- **Description:** Enable cosmetic CSS injection (element hiding) based on filter list rules.
- **Example:**
  ```toml
  adblock_cosmetic_filtering = true
  ```

### `https_upgrade_enabled`

- **Type:** boolean
- **Default:** `true`
- **Description:** Automatically upgrade HTTP requests to HTTPS for domains on the HTTPS Everywhere safe list.
- **Example:**
  ```toml
  https_upgrade_enabled = true
  ```

### `tracking_protection_enabled`

- **Type:** boolean
- **Default:** `true`
- **Description:** Enable tracking protection. Blocks known tracker domains, strips referrer headers, and sends DNT/GPC signals.
- **Example:**
  ```toml
  tracking_protection_enabled = true
  ```

### `popup_blocker_enabled`

- **Type:** boolean
- **Default:** `true`
- **Description:** Block `window.open()` calls except those triggered by user gestures (clicks, key presses).
- **Example:**
  ```toml
  popup_blocker_enabled = true
  ```

### `spellcheck_enabled`

- **Type:** boolean
- **Default:** `true`
- **Description:** Enable WebKitGTK spell checking. Can also be controlled at runtime via the `AILERON_SPELLCHECK` environment variable.
- **Platforms:** Linux only.
- **Example:**
  ```toml
  spellcheck_enabled = true
  ```

## Tabs and Layout

### `tab_layout`

- **Type:** string (enum)
- **Default:** `"sidebar"`
- **Description:** Tab bar layout style.
  - `"sidebar"` -- Vertical tab list in a sidebar panel.
  - `"topbar"` -- Horizontal tab bar at the top.
  - `"none"` -- No tab bar shown.
- **Example:**
  ```toml
  tab_layout = "sidebar"
  ```

### `tab_sidebar_width`

- **Type:** float
- **Default:** `180.0`
- **Description:** Width of the tab sidebar in logical pixels. Only used when `tab_layout` is `"sidebar"`.
- **Range:** Positive float.
- **Example:**
  ```toml
  tab_sidebar_width = 200.0
  ```

### `tab_sidebar_right`

- **Type:** boolean
- **Default:** `false`
- **Description:** Show the tab sidebar on the right side of the window instead of the left.
- **Platform overrides:** macOS defaults to `true`.
- **Example:**
  ```toml
  tab_sidebar_right = true
  ```

## Session and Workspaces

### `restore_session`

- **Type:** boolean
- **Default:** `false`
- **Description:** Auto-restore the most recent workspace on startup. If the previous session ended uncleanly (crash), the `_autosave` workspace is restored instead.
- **Example:**
  ```toml
  restore_session = true
  ```

### `auto_save`

- **Type:** boolean
- **Default:** `true`
- **Description:** Periodically save the current workspace for crash recovery.
- **Example:**
  ```toml
  auto_save = true
  ```

### `auto_save_interval`

- **Type:** integer
- **Default:** `30`
- **Description:** Auto-save interval in seconds. Only effective when `auto_save` is `true`.
- **Range:** Positive integer.
- **Example:**
  ```toml
  auto_save_interval = 60
  ```

## Search

### `search_engine`

- **Type:** string
- **Default:** `"https://duckduckgo.com/?q={query}"`
- **Description:** Default search engine URL template. The literal `{query}` placeholder is replaced with URL-encoded search terms when navigating from the URL bar.
- **Example:**
  ```toml
  search_engine = "https://www.google.com/search?q={query}"
  ```

### `search_engines`

- **Type:** map of string to string
- **Default:**
  ```toml
  [search_engines]
  google = "https://www.google.com/search?q={query}"
  ddg = "https://duckduckgo.com/?q={query}"
  gh = "https://github.com/search?q={query}"
  yt = "https://www.youtube.com/results?search_query={query}"
  wiki = "https://en.wikipedia.org/w/index.php?search={query}"
  ```
- **Description:** Additional named search engines. Switch between them at runtime with the `:engine <name>` command. Each value is a URL template with `{query}` placeholder.
- **Example:**
  ```toml
  [search_engines]
  google = "https://www.google.com/search?q={query}"
  ddg = "https://duckduckgo.com/?q={query}"
  ```

## Networking

### `proxy`

- **Type:** optional string
- **Default:** `null` (no proxy)
- **Description:** HTTP, HTTPS, or SOCKS5 proxy URL. Set at startup to the `all_proxy` environment variable, which WebKit and other components honor.
- **Format:** `"http://host:port"`, `"https://host:port"`, or `"socks5://host:port"`.
- **Example:**
  ```toml
  proxy = "socks5://127.0.0.1:1080"
  ```

## Scripting and Customization

### `init_lua_path`

- **Type:** optional string
- **Default:** `null` (uses `<config_dir>/init.lua`)
- **Description:** Path to a custom init.lua script. Overrides the default XDG config directory location.
- **Example:**
  ```toml
  init_lua_path = "/home/user/.config/aileron/init.lua"
  ```

### `custom_css`

- **Type:** optional string
- **Default:** `null`
- **Description:** Raw CSS injected into every loaded page. For advanced users.
- **Example:**
  ```toml
  custom_css = "body { background: #000 !important; }"
  ```

### `keybindings`

- **Type:** map of string to string
- **Default:** `{}` (empty)
- **Description:** Custom keybinding overrides. Keys use crossterm notation, values are action names from the `Action` enum in `src/input/keybindings.rs`.
- **Key format:** `<C-p>` for Ctrl+P, `<A-S>` for Alt+Shift+S, `<S-C-f>` for Ctrl+Shift+F.
- **Example:**
  ```toml
  [keybindings]
  "<C-p>" = "OpenCommandPalette"
  "j" = "ScrollDown"
  "k" = "ScrollUp"
  ```

## Theming

### `theme`

- **Type:** string
- **Default:** `"dark"`
- **Description:** Active UI theme. Can be a built-in name or a custom theme defined in the `themes` section. Falls back to `"dark"` if the named theme is not found.
- **Built-in themes:** `dark`, `light`, `gruvbox-dark`, `nord`, `dracula`, `solarized-dark`, `solarized-light`.
- **Example:**
  ```toml
  theme = "nord"
  ```

### `themes`

- **Type:** map of string to `ThemeColors`
- **Default:** All seven built-in themes are included.
- **Description:** Theme definitions. Each theme is a table of color overrides. Colors are hex strings without alpha (6-character `#RRGGBB`). Any omitted field inherits from the `"dark"` theme defaults.

#### ThemeColors fields

| Field | Default (dark) | Description |
|-------|----------------|-------------|
| `bg` | `#191920` | Main background |
| `fg` | `#e0e0e0` | Main foreground text |
| `accent` | `#4db4ff` | Accent color (highlights, links) |
| `tab_bar_bg` | `#19191e` | Tab bar background |
| `tab_bar_fg` | `#cccccc` | Tab bar text |
| `status_bar_bg` | `#1a1a20` | Status bar background |
| `status_bar_fg` | `#cccccc` | Status bar text |
| `url_bar_bg` | `#1a1a20` | URL bar background |
| `url_bar_fg` | `#e0e0e0` | URL bar text |
| `border` | `#3c3c3c` | Border color |

- **Example:**
  ```toml
  [themes.mytheme]
  bg = "#1e1e2e"
  fg = "#cdd6f4"
  accent = "#89b4fa"
  tab_bar_bg = "#181825"
  tab_bar_fg = "#bac2de"
  status_bar_bg = "#181825"
  status_bar_fg = "#a6adc8"
  url_bar_bg = "#1e1e2e"
  url_bar_fg = "#cdd6f4"
  border = "#45475a"
  ```

### `language`

- **Type:** optional string
- **Default:** `null` (auto-detect from `LANG` environment variable)
- **Description:** Preferred UI language as an ISO 639-1 code (e.g., `"en"`, `"zh"`, `"ja"`).
- **Example:**
  ```toml
  language = "en"
  ```

## Command Palette

### `palette.max_results`

- **Type:** integer
- **Default:** `20`
- **Description:** Maximum number of results displayed in the command palette.
- **Range:** Positive integer.
- **Example:**
  ```toml
  [palette]
  max_results = 50
  ```

## Rendering Engine

### `engine_selection`

- **Type:** string (enum)
- **Default:** `"webkit"`
- **Description:** Controls which rendering engine to use.
  - `"auto"` -- Choose per-URL based on compatibility lists.
  - `"servo"` -- Use Servo with WebKit fallback on failure.
  - `"webkit"` -- Always use WebKit (most compatible).
- **Example:**
  ```toml
  engine_selection = "webkit"
  ```

### `compat_overrides`

- **Type:** map of string to string
- **Default:** `{}` (empty)
- **Description:** Per-domain engine compatibility overrides. Key is the domain, value is `"webkit"` or `"servo"`. Custom overrides take highest priority in `"auto"` mode.
- **Example:**
  ```toml
  [compat_overrides]
  "github.com" = "webkit"
  "example.com" = "servo"
  ```

## Sync

### `sync_target`

- **Type:** string
- **Default:** `""` (disabled)
- **Description:** Sync target -- an SSH target or local path. Empty string disables sync.
- **Example:**
  ```toml
  sync_target = "user@server:~/.aileron-sync"
  ```

### `sync_encrypted`

- **Type:** boolean
- **Default:** `false`
- **Description:** Enable end-to-end encryption for sync data.
- **Example:**
  ```toml
  sync_encrypted = true
  ```

### `sync_passphrase`

- **Type:** string
- **Default:** `""`
- **Description:** Sync encryption passphrase. Written to config but excluded from serialization. Set at runtime via the `:sync-passphrase` command.

### `sync_auto`

- **Type:** boolean
- **Default:** `false`
- **Description:** Enable automatic real-time sync via filesystem watcher.
- **Example:**
  ```toml
  sync_auto = true
  ```

### `sync_auto_interval_sec`

- **Type:** integer
- **Default:** `60`
- **Description:** Auto-sync polling interval in seconds. Used as fallback when filesystem watcher is unavailable.
- **Range:** Positive integer.
- **Example:**
  ```toml
  sync_auto_interval_sec = 30
  ```

## ARP (Aileron Remote Protocol)

### `arp_port`

- **Type:** optional integer
- **Default:** `null` (uses port 19743)
- **Description:** TCP port for the ARP server. None uses the default port.
- **Range:** Valid port number (1-65535).
- **Example:**
  ```toml
  arp_port = 19743
  ```

### `arp_token`

- **Type:** optional string
- **Default:** `null` (no auth)
- **Description:** ARP authentication token. Generated at runtime via `:arp-token`. None means no authentication (warning logged). Excluded from serialization.

## Memory

### `memory_limit_mb`

- **Type:** integer
- **Default:** `0` (no limit)
- **Description:** Memory limit in megabytes. When process RSS exceeds this, the least-recently active background tab is evicted (webview destroyed, tree node kept). Zero disables memory limit enforcement.
- **Range:** Non-negative integer. Recommended minimum: `512`.
- **Example:**
  ```toml
  memory_limit_mb = 2048
  ```

## Complete Example Config

```toml
# Aileron Configuration

# General
homepage = "aileron://welcome"
window_width = 1280
window_height = 800
devtools = false

# Rendering
render_mode = "offscreen"
adaptive_quality = true

# Privacy
adblock_enabled = true
adblock_filter_lists = [
    "https://easylist.to/easylist/easylist.txt",
    "https://easylist.to/easylist/easyprivacy.txt",
]
adblock_update_interval_hours = 24
adblock_cosmetic_filtering = true
https_upgrade_enabled = true
tracking_protection_enabled = true
popup_blocker_enabled = true
spellcheck_enabled = true

# Tabs
tab_layout = "sidebar"
tab_sidebar_width = 180.0
tab_sidebar_right = false

# Session
restore_session = false
auto_save = true
auto_save_interval = 30

# Search
search_engine = "https://duckduckgo.com/?q={query}"

[search_engines]
google = "https://www.google.com/search?q={query}"
ddg = "https://duckduckgo.com/?q={query}"
gh = "https://github.com/search?q={query}"
yt = "https://www.youtube.com/results?search_query={query}"
wiki = "https://en.wikipedia.org/w/index.php?search={query}"

# Networking
# proxy = "socks5://127.0.0.1:1080"

# Scripting
# init_lua_path = "/home/user/.config/aileron/init.lua"
# custom_css = "body { background: #000 !important; }"

# Keybindings
# [keybindings]
# "<C-p>" = "OpenCommandPalette"
# "j" = "ScrollDown"

# Theming
theme = "dark"
# language = "en"

# Command palette
[palette]
max_results = 20

# Engine selection
engine_selection = "webkit"

# [compat_overrides]
# "github.com" = "webkit"

# Sync (disabled by default)
# sync_target = ""
# sync_encrypted = false
# sync_auto = false
# sync_auto_interval_sec = 60

# ARP
# arp_port = 19743

# Memory
memory_limit_mb = 0
```

## Platform-Specific Defaults

### macOS

| Option | Override |
|--------|----------|
| `tab_sidebar_right` | `true` |

### Windows

| Option | Override |
|--------|----------|
| `render_mode` | `"native"` |

### Linux

No config value overrides. Linux defaults are the base defaults documented above.

## Config Migration

Aileron automatically migrates configs from older schema versions. The current schema version is `2`. Migration runs on load and the updated file is written back to disk. The `config_version` field should not be edited manually.
