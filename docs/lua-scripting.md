# Aileron Lua Scripting Guide

## Overview

Aileron supports Lua 5.4 scripting via [mlua](https://github.com/mlua-rs/mlua). Scripts are loaded from `~/.config/aileron/init.lua` (configurable via `config.init_lua_path`). The Lua environment is sandboxed — only `string`, `table`, `math`, `utf8`, and `coroutine` stdlib modules are available. Blocked: `os`, `io`, `debug`, `package`, `dofile`, `loadfile`, `load`, `require`, `dostring`.

## API Reference

### `aileron.version`

Returns the Aileron version string.

```lua
print(aileron.version)
```

### `aileron.keymap.set(mode, key, action)`

Register a keybinding.

**Modes:** `"normal"`, `"insert"`, `"command"`

**Key format:** Modifier+key combinations like `"ctrl+a"`, `"alt+shift+t"`, `"super+k"`. Unmodified keys use their literal name.

**Special keys:** `enter`, `escape`, `backspace`, `tab`, `space`, `up`, `down`, `left`, `right`, `home`, `end`, `pageup`, `page_down`

**Supported actions:**

| Action | Description |
|---|---|
| `quit` | Quit the browser |
| `scroll_up` / `scroll_down` | Scroll vertically |
| `scroll_left` / `scroll_right` | Scroll horizontally |
| `split_horizontal` / `sp` | Split pane horizontally |
| `split_vertical` / `vs` | Split pane vertically |
| `close_pane` | Close current pane |
| `navigate_back` | Go back in history |
| `navigate_forward` | Go forward in history |
| `reload` | Reload current page |
| `open_command_palette` | Open the command palette |
| `open_external_browser` | Open URL in external browser |
| `enter_insert_mode` / `insert` | Enter insert mode |
| `pin_pane` / `pin` | Pin the current pane |

```lua
aileron.keymap.set("normal", "ctrl+q", "quit")
aileron.keymap.set("normal", "ctrl+w", "close_pane")
aileron.keymap.set("normal", "H", "navigate_back")
aileron.keymap.set("normal", "L", "navigate_forward")
```

### `aileron.cmd.create(name, description, callback)`

Register a custom command. Appears in the command palette as `:cmd <name>`.

```lua
aileron.cmd.create("hello", "Print a greeting", function()
    aileron.log("Hello from Aileron!")
end)
```

### `aileron.on(event, callback)`

Register an event hook.

**Events:**

| Event | Callback arguments |
|---|---|
| `"navigate"` | `url` — the URL being navigated to |
| `"mode_change"` | `mode` — the new mode name |

```lua
aileron.on("navigate", function(url)
    aileron.log("Navigated to: " .. url)
end)

aileron.on("mode_change", function(mode)
    aileron.log("Switched to mode: " .. mode)
end)
```

### `aileron.url.add_redirect(pattern, replacement)`

Add a URL redirect rule. If the host contains `pattern` (case-insensitive match), the first match of `pattern` in the host is replaced with `replacement`.

```lua
aileron.url.add_redirect("twitter.com", "nitter.net")
```

### `aileron.theme.set(name)`

Set the UI theme. Currently a placeholder — logs the theme name.

```lua
aileron.theme.set("dark")
```

### `aileron.info()`

Returns a table with Aileron build info.

```lua
local info = aileron.info()
aileron.log("Version: " .. info.version)
aileron.log("Engine: " .. info.engine)
```

### `aileron.log(message)` / `aileron.warn(message)`

Print a log or warning message to Aileron's log output.

```lua
aileron.log("Something happened")
aileron.warn("Something seems off")
```

### `aileron.extensions.list()`

Returns an array of loaded extensions. Each entry is a table with keys: `id`, `name`, `version`, `description`, `has_background`.

```lua
local exts = aileron.extensions.list()
for _, ext in ipairs(exts) do
    aileron.log(ext.id .. ": " .. ext.name .. " v" .. ext.version)
end
```

### `aileron.extensions.info(id)`

Returns detailed info about a specific extension by its ID.

```lua
local info = aileron.extensions.info("my-extension")
aileron.log(info.name .. " — " .. info.description)
```

### `aileron.extensions.reload(id)`

Stubbed — not yet implemented. Will reload an extension by ID in a future release.

## Examples

### Example 1: Custom Keybindings

Vim-style navigation and pane management:

```lua
aileron.keymap.set("normal", "ctrl+h", "scroll_left")
aileron.keymap.set("normal", "ctrl+j", "scroll_down")
aileron.keymap.set("normal", "ctrl+k", "scroll_up")
aileron.keymap.set("normal", "ctrl+l", "scroll_right")

aileron.keymap.set("normal", "ctrl+|", "split_vertical")
aileron.keymap.set("normal", "ctrl+-", "split_horizontal")
aileron.keymap.set("normal", "ctrl+x", "close_pane")

aileron.keymap.set("normal", "i", "insert")
aileron.keymap.set("normal", "r", "reload")
aileron.keymap.set("normal", "p", "pin_pane")
```

### Example 2: Custom Commands

Open frequently used sites with short commands:

```lua
aileron.cmd.create("gh", "Open GitHub", function()
    aileron.log("Opening github.com")
end)

aileron.cmd.create("news", "Open Hacker News", function()
    aileron.log("Opening news.ycombinator.com")
end)

aileron.cmd.create("rc", "Reload current page", function()
    aileron.log("Reloading...")
end)
```

### Example 3: URL Redirects

Redirect privacy-unfriendly sites to privacy-respecting frontends:

```lua
aileron.url.add_redirect("twitter.com", "nitter.net")
aileron.url.add_redirect("x.com", "nitter.net")
aileron.url.add_redirect("reddit.com", "old.reddit.com")
aileron.url.add_redirect("youtube.com", "yewtu.be")
```

### Example 4: Navigation Hooks

Log all navigations and auto-pin work-related domains:

```lua
local work_domains = { "github.com", "gitlab.com", "linear.app" }

aileron.on("navigate", function(url)
    aileron.log("Navigate: " .. url)

    for _, domain in ipairs(work_domains) do
        if string.find(url, domain, 1, true) then
            aileron.log("Work domain detected — pinning pane")
            break
        end
    end
end)

aileron.on("mode_change", function(mode)
    aileron.warn("Mode changed to: " .. mode)
end)
```

### Example 5: Track Visited URLs with a History Command

Keep an in-memory history and create a command to display it:

```lua
local history = {}
local max_history = 50

aileron.on("navigate", function(url)
    table.insert(history, 1, url)
    if #history > max_history then
        table.remove(history)
    end
end)

aileron.cmd.create("history", "Show recent navigation history", function()
    aileron.log("=== Recent History ===")
    for i, url in ipairs(history) do
        aileron.log(string.format("  %d. %s", i, url))
    end
    aileron.log(string.format("=== %d entries ===", #history))
end)

aileron.cmd.create("clear-history", "Clear navigation history", function()
    history = {}
    aileron.log("History cleared")
end)
```
