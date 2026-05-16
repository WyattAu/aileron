# Aileron Keybindings Reference

Source: `src/input/keybindings.rs`, `src/input/mode.rs`, `src/app/dispatch.rs`

---

## Mode Overview

Aileron uses a modal input system. The current mode determines how keystrokes are interpreted and where they are routed.

| Mode | Status bar label | Behavior |
|------|-----------------|----------|
| **Normal** | `NORMAL` | Keystrokes execute keybindings (scroll, navigate, split, etc.). No text is sent to the page. |
| **Insert** | `INSERT` | Keystrokes are forwarded to the web content for typing and interaction. |
| **Command** | `COMMAND` | Keystrokes are sent to the command palette for entering commands and URLs. |

### Mode Transitions

| From | Key | To |
|------|-----|----|
| Normal | `i` / `I` | Insert |
| Normal | `:` | Command |
| Insert | `Escape` | Normal |
| Insert | `Ctrl+[` | Normal |
| Command | `Escape` | Normal |

### Event Routing by Mode

| Mode | Character keys | Escape/Enter/Backspace/Tab | Arrow keys / F-keys / Ctrl / Alt |
|------|---------------|---------------------------|-------------------------------|
| Normal | Keybinding handler | Keybinding handler | egui (UI) |
| Insert | Web content (wry/WebKitGTK) | Web content (wry/WebKitGTK) | egui (UI) |
| Command | Command palette | Command palette | egui (UI) |

---

## Scrolling

| Key | Mode | Action | Description |
|-----|------|--------|-------------|
| `j` | Normal | `ScrollDown` | Scroll down one line (smooth, 120px) |
| `k` | Normal | `ScrollUp` | Scroll up one line (smooth, 120px) |
| `h` | Normal | `ScrollLeft` | Scroll left (120px) |
| `l` | Normal | `ScrollRight` | Scroll right (120px) |
| `Ctrl+d` | Normal | `HalfPageDown` | Scroll down half a page (smooth, 400px) |
| `Ctrl+u` | Normal | `HalfPageUp` | Scroll up half a page (smooth, 400px) |
| `G` | Normal | `ScrollBottom` | Jump to bottom of page |
| `Ctrl+g` | Normal | `ScrollTop` | Jump to top of page |

---

## Page Navigation

| Key | Mode | Action | Description |
|-----|------|--------|-------------|
| `H` | Normal | `NavigateBack` | Go back in browser history |
| `L` | Normal | `NavigateForward` | Go forward in browser history |
| `r` | Normal | `Reload` | Reload the current page |

---

## Pane Management

| Key | Mode | Action | Description |
|-----|------|--------|-------------|
| `Ctrl+w` | Normal | `SplitVertical` | Split the current pane vertically (side by side) |
| `Ctrl+s` | Normal | `SplitHorizontal` | Split the current pane horizontally (stacked) |
| `q` | Normal | `ClosePane` | Close the active pane |
| `Ctrl+Shift+W` | Normal | `CloseTab` | Close the active tab within the pane |
| `Ctrl+Shift+D` | Normal | `DetachPane` | Detach current pane to a standalone popup window |
| `Ctrl+Shift+P` | Normal | `PinPane` | Pin/unpin the active pane |

---

## Pane Navigation

| Key | Mode | Action | Description |
|-----|------|--------|-------------|
| `Ctrl+h` | Normal | `NavigateLeft` | Move focus to the pane on the left |
| `Ctrl+j` | Normal | `NavigateDown` | Move focus to the pane below |
| `Ctrl+k` | Normal | `NavigateUp` | Move focus to the pane above |
| `Ctrl+l` | Normal | `NavigateRight` | Move focus to the pane on the right |

---

## Pane Resize

Resizes the active pane in a direction (tmux-style).

| Key | Mode | Action | Description |
|-----|------|--------|-------------|
| `Ctrl+Alt+h` | Normal | `ResizePane(Left)` | Shrink pane width from the right |
| `Ctrl+Alt+l` | Normal | `ResizePane(Right)` | Grow pane width to the right |
| `Ctrl+Alt+j` | Normal | `ResizePane(Down)` | Grow pane height downward |
| `Ctrl+Alt+k` | Normal | `ResizePane(Up)` | Shrink pane height from the bottom |

---

## Tab Management

| Key | Mode | Action | Description |
|-----|------|--------|-------------|
| `Ctrl+t` | Normal | `NewTab` | Open a new tab within the active pane |

---

## Zoom

| Key | Mode | Action | Description |
|-----|------|--------|-------------|
| `Ctrl+=` | Normal | `ZoomIn` | Increase page zoom by 10% |
| `Ctrl+-` | Normal | `ZoomOut` | Decrease page zoom by 10% (minimum 30%) |
| `Ctrl+0` | Normal | `ZoomReset` | Reset zoom to 100% |

---

## Mode and UI

| Key | Mode | Action | Description |
|-----|------|--------|-------------|
| `i` / `I` | Normal | `EnterInsertMode` | Switch to Insert mode (pass keystrokes to page) |
| `:` | Normal | `OpenCommandPalette` | Open the command palette |
| `Ctrl+p` | Normal | `OpenCommandPalette` | Open the command palette (alternate binding) |
| `F12` | Normal | `ToggleDevTools` | Toggle web developer tools |

---

## Find in Page

| Key | Mode | Action | Description |
|-----|------|--------|-------------|
| `/` | Normal | `Find` | Open find-in-page bar |
| `Ctrl+f` | Normal | `Find` | Open find-in-page bar (alternate binding) |
| `Escape` | Find bar | `FindClose` | Close find bar |

The `FindNext` and `FindPrev` actions navigate between matches within the page using `window.find()`.

---

## Link Hints (Vimium-style)

| Key | Mode | Action | Description |
|-----|------|--------|-------------|
| `f` | Normal | `ToggleLinkHints` | Toggle link hint overlay (clickable links, buttons, inputs get letter hints) |
| `F` | Normal | `FollowLinkNewTab` | Show link hints; followed links open in a new background tab (orange hints) |

When hints are shown, type the displayed letters to activate the element. Press `Escape` to dismiss hints.

Hint labels use the home row (`asdfghjklqwertyuiopzxcvbnm`), expanding to multi-character labels (`aa`, `ab`, ...) as needed.

---

## Clipboard

| Key | Mode | Action | Description |
|-----|------|--------|-------------|
| `y` | Normal | `CopyUrl` | Copy the current page URL to the system clipboard |

The `Yank` action copies the current text selection to the clipboard. The `Paste` action inserts clipboard text at the cursor. Neither has a default keybinding in Normal mode.

---

## Bookmarks

| Key | Mode | Action | Description |
|-----|------|--------|-------------|
| `Ctrl+b` | Normal | `BookmarkToggle` | Toggle bookmark on the current page |

---

## External Browser and Windows

| Key | Mode | Action | Description |
|-----|------|--------|-------------|
| `Ctrl+e` | Normal | `OpenExternalBrowser` | Open the current page URL in the system default browser |
| `Ctrl+n` | Normal | `NewWindow` | Open a new standalone Aileron window |

---

## Developer Tools and Logs

| Key | Mode | Action | Description |
|-----|------|--------|-------------|
| `F12` | Normal | `ToggleDevTools` | Toggle web developer tools |
| `Ctrl+Shift+N` | Normal | `ToggleNetworkLog` | Show network request log |
| `Ctrl+Shift+J` | Normal | `ToggleConsoleLog` | Show JavaScript console log |

---

## Reading and Minimal Modes

| Key | Mode | Action | Description |
|-----|------|--------|-------------|
| `Ctrl+Shift+R` | Normal | `ToggleReaderMode` | Toggle reader mode (strips CSS, shows article text) |
| `Ctrl+Shift+M` | Normal | `ToggleMinimalMode` | Toggle minimal mode (disables JS, blocks images) |

---

## Terminal

| Key | Mode | Action | Description |
|-----|------|--------|-------------|
| `` ` `` | Normal | `OpenTerminal` | Open an embedded terminal pane |

---

## Scroll Marks

| Action | Description |
|--------|-------------|
| `SetMark(char)` | Set a scroll mark at the current position (followed by a letter key) |
| `GoToMark(char)` | Jump to a previously set scroll mark |

No default keybindings are assigned to these actions. They can be bound via configuration or Lua scripts.

---

## Actions Without Default Keybindings

The following actions are defined in the `Action` enum but have no default keybinding:

| Action | Description |
|--------|-------------|
| `Quit` | Quit the application |
| `Yank` | Copy text selection to clipboard |
| `Paste` | Paste clipboard text at cursor |
| `SaveWorkspace` | Save current pane layout as a named workspace |
| `CloseOtherPanes` | Close all panes except the current one |
| `Print` | Print the current page |
| `NewTabInPane` | Open a new tab navigating to `aileron://new` |
| `NextTab` | Switch to the next tab in the active pane |
| `PrevTab` | Switch to the previous tab in the active pane |
| `SetMark(char)` | Set a scroll mark |
| `GoToMark(char)` | Jump to a scroll mark |
| `Custom(String)` | User-defined action via Lua |

These can be triggered from the command palette (`:`) or bound to keys via configuration.

---

## Command Palette

The command palette (`:`) is the primary interface for entering commands, URLs, and search queries.

**Opening:** Press `:` in Normal mode, or `Ctrl+p`.

**Behavior:**
- Type a URL (starting with `http://` or `https://`) to navigate directly.
- Type a search query (no scheme) to search using the configured search engine.
- Type a command prefixed with `:` (e.g., `:quit`, `:set theme gruvbox-dark`) to execute it.

**Closing:** Press `Escape` to return to Normal mode without executing. Press `Enter` to submit.

The palette shows up to `palette.max_results` items (default: 20, configurable in `config.toml`).

---

## Customizing Keybindings

Keybindings are customized in `~/.config/aileron/config.toml` under the `[keybindings]` table.

### Format

```toml
[keybindings]
"<key>" = "<Action>"
```

- **Key notation** uses angle-bracketed crossterm-style strings (angle brackets are optional):
  - `"j"` -- bare character key (Normal mode only)
  - `"<C-p>"` -- Ctrl+P
  - `"<A-S-i>"` -- Alt+Shift+I
  - `"<C-S-R>"` -- Ctrl+Shift+R
  - `"<F1>"` through `"<F12>"` -- function keys
  - `"Enter"`, `"Escape"` / `"Esc"`, `"Backspace"` / `"BS"`, `"Tab"`, `"Space"` / `"SPC"` -- special keys

- **Modifier prefixes:** `C-` (Ctrl), `A-` (Alt), `S-` (Shift)
- **All custom bindings apply in Normal mode.**

### Action Names

The full action name from the enum, or a shorthand alias where available:

| Full Name | Aliases |
|-----------|---------|
| `ScrollDown` | -- |
| `ScrollUp` | -- |
| `ScrollLeft` | -- |
| `ScrollRight` | -- |
| `HalfPageDown` | -- |
| `HalfPageUp` | -- |
| `ScrollTop` | -- |
| `ScrollBottom` | -- |
| `SplitVertical` | `vs` |
| `SplitHorizontal` | `sp` |
| `ClosePane` | `CloseTab` |
| `CloseOtherPanes` | -- |
| `NavigateUp` | -- |
| `NavigateDown` | -- |
| `NavigateLeft` | -- |
| `NavigateRight` | -- |
| `NavigateBack` | -- |
| `NavigateForward` | -- |
| `Reload` | -- |
| `BookmarkToggle` | -- |
| `OpenCommandPalette` | `CommandPalette` |
| `OpenExternalBrowser` | -- |
| `EnterInsertMode` | `InsertMode` |
| `ToggleDevTools` | `DevTools` |
| `NewTab` | -- |
| `Yank` | -- |
| `Paste` | -- |
| `CopyUrl` | -- |
| `Find` | -- |
| `FindNext` | -- |
| `FindPrev` | -- |
| `FindClose` | -- |
| `ToggleLinkHints` | `Hints` |
| `FollowLinkNewTab` | `FollowNewTab` |
| `SaveWorkspace` | -- |
| `OpenTerminal` | -- |
| `NewWindow` | -- |
| `ZoomIn` | -- |
| `ZoomOut` | -- |
| `ZoomReset` | -- |
| `ToggleReaderMode` | `Reader` |
| `ToggleMinimalMode` | `Minimal` |
| `ToggleNetworkLog` | -- |
| `ToggleConsoleLog` | -- |
| `DetachPane` | `Detach` |
| `Print` | -- |
| `PinPane` | `Pin` |
| `Quit` | -- |

### Example Configuration

```toml
[keybindings]
"<C-q>" = "Quit"
"<S-h>" = "NavigateBack"
"<S-l>" = "NavigateForward"
"<C-S-t>" = "NewTabInPane"
"gt" = "NextTab"
"gT" = "PrevTab"
"<C-a>" = "ToggleLinkHints"
```

Custom bindings overwrite defaults for the same key combination. Invalid keys or unknown actions are logged as warnings and silently skipped.

---

## Key Mapping Notes

Aileron maps physical key codes (QWERTY layout positions) rather than logical characters. This means keybindings are **layout-independent**: pressing the physical `J` key always triggers the `j` binding regardless of keyboard layout (e.g., Dvorak, Colemak). This is implemented in `src/input/keymap.rs` via winit `KeyCode` mapping.
