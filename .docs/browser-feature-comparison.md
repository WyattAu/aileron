# Feature Implementation Comparison: Aileron vs Alternatives

## 1. Tiling and Window Management

### Aileron Implementation

**Approach:** BSP (Binary Space Partition) tree data structure in `src/wm/tree.rs`.

- Each leaf node is a pane (webview or terminal)
- Internal nodes represent horizontal or vertical splits
- Supports unlimited nesting depth
- Proportional resize via edge dragging
- Pane metadata: title, URL, session state, type (web/terminal)
- Area-preserving splits (no lost pixels)
- Minimum pane size enforcement (rejects tiny splits)

**Code footprint:** ~500 LOC across `tree.rs`, `pane.rs`, `rect.rs`

**Key data structures:**
```
enum BspTree { Leaf(Pane), Split { direction, ratio, left, right } }
```

### Zen Browser Implementation

**Approach:** `split-view/` module with `ZenViewSplitter.mjs`.

- Maximum 2-pane split (no nesting)
- Horizontal or vertical
- Implemented as a browser chrome overlay
- Drag-to-resize divider
- No workspace-aware tiling

**Code footprint:** ~800 LOC (JS/MJS)

### Comparison

| Capability | Aileron | Zen |
|------------|---------|-----|
| Max panes | Unlimited | 2 |
| Nesting depth | Unlimited | 1 |
| Resize granularity | Proportional (any ratio) | Divider drag |
| Terminal panes | Yes | No |
| Pane types | Web + Terminal | Web only |
| Session persistence | Yes (save/load workspace) | Yes (session store) |

**Aileron advantage:** Full BSP tree with unlimited nesting, terminal embedding, session persistence.

**Missing in Aileron:** No visual split indicator during drag, no predefined layouts (e.g., "3-column"), no tab-within-pane (multiple tabs per pane).

---

## 2. Extension Systems

### Aileron Implementation

**Approach:** WebExtensions API subset implemented as Rust traits in `src/extensions/`.

- 6 API traits: `runtime`, `tabs`, `storage`, `scripting`, `webRequest`, `permissions`
- Manifest V3 primary, MV2 backward-compatible
- Extensions loaded from filesystem (`~/.config/aileron/extensions/<name>/`)
- Content scripts via Lua -> JS injection pipeline
- Message bus for inter-extension communication
- Persistent storage (JSON files per extension)
- Background scripts loaded but not yet executed in JS runtime

**Code footprint:** ~4,000 LOC across 15 files

### qutebrowser Implementation

**Approach:** Python-based extension API.

- Userscripts (JavaScript injection with `@match` patterns)
- Host blocking via adblock-compatible lists
- Greasemonkey compatibility layer
- Python command API (`:spawn`, `:debug`)

### Nyxt Implementation

**Approach:** Full Common Lisp extensibility.

- 40+ built-in modes (no-script, no-image, blocker, etc.)
- Macros via Lisp functions
- REPL for live development
- Configuration is Lisp code (not data)

### Comparison

| Capability | Aileron | qutebrowser | Nyxt | Luakit |
|------------|---------|-------------|------|--------|
| API standard | WebExtensions (partial) | Custom Python | Custom Lisp | Custom Lua |
| Chrome store compat | No | No | No | No |
| AMO/Firefox compat | No | No | No | No |
| Content scripts | Yes (Lua->JS) | Yes (JS userscripts) | Yes (Lisp) | Yes (Lua) |
| Background scripts | Loaded, not executed | N/A | Yes (Lisp) | No |
| Storage API | Yes (JSON) | No | No | No |
| Message passing | Yes (port + broadcast) | No | Yes (Lisp) | No |
| MV3 declarativeNetRequest | No | No | No | No |
| Native ad blocking | Yes (EasyList parser) | Yes (braveadblock compat) | Yes (built-in) | Yes (noscript) |

**Aileron advantage:** WebExtensions API alignment (future Chrome/Firefox compat), persistent storage, message bus architecture.

**Missing in Aileron:** JS runtime for background scripts (currently loaded but not evaluated), cookies API, alarms API, contextMenus API, notifications API, declarativeNetRequest, permissions.request(), i18n API, webNavigation API, sidePanel, theme API.

---

## 3. Ad Blocking

### Aileron Implementation

**Approach:** Built-in EasyList-compatible parser in `src/net/adblock.rs`.

- Network filter rules (block, whitelist, important, badfilter)
- Cosmetic CSS rules (hide elements by selector)
- Cosmetic JS injection (remove elements)
- Aho-Corasick automaton for fast multi-pattern domain matching
- Per-site toggle, filter list auto-update
- Advanced: `$redirect`, `$csp`, `$removeheader`, `$important`, `$badfilter`
- Resource type filtering (image, script, font, media, etc.)
- Third-party filtering

**Code footprint:** ~1,500 LOC across `adblock.rs`, `filter_list.rs`

### Nyxt Implementation

**Approach:** Built-in blocker mode.

- URL-based blocking
- No cosmetic filtering
- No filter list format support

### Luakit Implementation

**Approach:** noscript module.

- JavaScript blocking per domain
- No cosmetic filtering
- No filter list format support

### Comparison

| Capability | Aileron | uBlock Origin | Nyxt | Luakit | Brave |
|------------|---------|--------------|------|--------|-------|
| EasyList parsing | Yes (full) | Yes | No | No | Yes |
| Cosmetic CSS | Yes | Yes | No | No | Yes |
| Cosmetic JS | Yes | No | No | No | No |
| $redirect | Yes | Yes | No | No | No |
| $csp | Yes | Yes | No | No | Yes |
| $removeheader | Yes | Yes | No | No | No |
| $badfilter | Yes | Yes | No | No | No |
| $important | Yes | Yes | No | No | Yes |
| Aho-Corasick | Yes | Yes | No | No | No |
| Per-site toggle | Yes | Yes | No | No | Yes |
| Filter list update | Yes (auto) | Yes | No | No | Yes |

**Aileron's ad blocker is near feature-complete** compared to uBlock Origin for network/cosmetic filtering. The main gap is missing cosmetic filters that use `:has()`, `:not()`, and other complex CSS selectors, and no support for scriptlet injection.

---

## 4. Terminal Emulation

### Aileron Implementation

**Approach:** Embedded native terminal using `alacritty_terminal` + `portable_pty` + egui rendering.

- Full VT100/xterm terminal emulation (alacritty_terminal)
- PTY process management (portable_pty)
- egui-based pixel-perfect rendering (no xterm.js overhead)
- Mouse selection and clipboard copy
- Visual bell (200ms flash)
- Terminal search in scrollback buffer
- SSH quick-connect (`:ssh user@host`)
- ~2-5 MB per pane (no Electron overhead)

**Code footprint:** ~800 LOC across `terminal/`, `input/`

### Comparison

| Capability | Aileron | Nyxt | qutebrowser | Luakit |
|------------|---------|------|-------------|--------|
| Embedded terminal | Yes (native) | No | No | No |
| External terminal | Via `:! cmd` | Via `expose` | Via `:spawn` | Via `:terminal` |
| PTY support | Yes | N/A | N/A | N/A |
| Mouse selection | Yes | N/A | N/A | N/A |
| SSH integration | Yes (`:ssh`) | N/A | N/A | No |
| Scrollback search | Yes | N/A | N/A | No |

**Aileron is the only browser with an embedded native terminal.** This is a significant differentiator for developer workflows.

---

## 5. AI/MCP Integration

### Aileron Implementation

**Approach:** Built-in Model Context Protocol (MCP) server.

- JSON-RPC over stdio transport
- Tools: browser_navigate, browser_get_text, browser_fill_form, click, screenshot, get_cookies, execute_js, wait_for, search_web, create_tab, close_tab
- Bridge pattern: `Arc<RwLock<>>` + `mpsc` channels
- tokio::sync::oneshot for response channels (no async runtime blocking)
- Integration with wry panes via IPC (JS init script)

**Code footprint:** ~1,200 LOC across `mcp/`

### Comparison

| Capability | Aileron | Zen | Floorp | Thorium | qutebrowser |
|------------|---------|-----|--------|---------|-------------|
| MCP server | Yes (built-in) | No | No | No | No |
| Browser automation API | Yes (MCP tools) | No | No | Chrome DevTools | No |
| Screenshot tool | Yes (via MCP) | No | No | Yes (headless) | No |
| Form filling | Yes (via MCP) | No | No | No | No |
| LLM integration | Yes (via MCP stdio) | No | No | No | No |

**Aileron is the only browser with built-in MCP support.** This enables LLM agents (Claude, GPT, etc.) to browse the web, fill forms, take screenshots, and extract page content programmatically.

---

## 6. Sync Protocol

### Aileron Implementation

**Approach:** Spec-level implementation with core primitives.

- Content-addressed manifest computation (blake3 chunk hashing)
- Delta detection (new, modified, deleted files)
- Age encryption for E2EE
- Filesystem watcher (notify crate)
- WebDAV transport: **spec only** (not implemented)
- CRDT conflict resolution: **spec only**
- Sync execution loop: **not implemented**

**Code footprint:** ~800 LOC across `sync/`

### Comparison

| Capability | Aileron | Firefox Sync | Chrome Sync | qutebrowser |
|------------|---------|-------------|-------------|-------------|
| Bookmarks sync | Spec | Yes | Yes | No |
| History sync | Spec | Yes | Yes | No |
| Settings sync | Spec | Yes | Yes | No |
| E2EE | Yes (age) | Yes (kXcrypt) | Yes (Google keys) | N/A |
| Self-hosted | Yes (WebDAV planned) | No | No | N/A |
| Conflict resolution | Spec (CRDT) | Server-side | Server-side | N/A |

**Aileron has the cryptographic and delta primitives** but lacks the actual transport layer (WebDAV) and sync execution loop. This is a significant gap -- Zen and Floorp get sync "for free" via Firefox Sync.

---

## 7. Keyboard-Driven Interface

### Aileron Implementation

**Approach:** Native modal editing with three modes (Normal, Insert, Command).

- Vim-style keybindings (h/j/k/l, H/L for history)
- Command palette (Ctrl+P) with Nucleo fuzzy search
- Ex-command parser (60+ commands)
- Pure action dispatch pattern (Action -> ActionEffect)
- Link hints mode (f to reveal, digits to follow)
- Find in page (Ctrl+F)
- Mode indicator in status bar

### qutebrowser Implementation

**Approach:** Vi-like keybindings with command mode.

- Full vi modal editing
- Command completion (partial match)
- Hint mode (similar to Aileron link hints)
- `:set`/`:bind` configuration
- Marks (similar to Aileron's scroll marks)

### Comparison

| Capability | Aileron | qutebrowser | Nyxt | Luakit |
|------------|---------|-------------|------|--------|
| Modal editing | Yes (3 modes) | Yes (3 modes) | Yes (multiple modes) | Yes (normal/insert) |
| Command palette | Yes (Nucleo) | Yes (partial match) | Yes (prompt-buffer) | Yes |
| Link hints | Yes | Yes | Yes | No |
| Ex-commands | 60+ | 200+ | Lisp functions | Lua API |
| Custom keybindings | Yes (config.toml) | Yes (config.py) | Yes (Lisp) | Yes (rc.lua) |
| Macro recording | No | Yes | No | No |
| Marks | Yes (a-z) | Yes | No | No |
| Did-you-mean | Yes (fuzzy Levenshtein) | Yes | No | No |

**qutebrowser leads in command count** (200+ vs 60+). **Aileron leads in architecture** (pure dispatch pattern, no side effects in command parsing).

---

## 8. Privacy and Security

### Aileron Implementation

- HTTPS upgrade for known-safe domains (EasyList HTTPS list)
- Tracking protection (Disconnect list domain blocking)
- DNT and GPC headers sent with every request
- Strict referrer policy
- Popup blocker
- Cookie management (view, clear, per-site allow/block)
- Per-site settings (zoom, adblock, JS, cookies, autoplay)
- Content Security Policy headers ($csp filter option)

### LibreWolf Implementation

- All Firefox telemetry removed
- Pocket removed
- DRM disabled by default
- uBlock Origin bundled
- Privacy-respecting search defaults
- Signed builds

### Comparison

| Capability | Aileron | LibreWolf | Zen | Thorium |
|------------|---------|-----------|-----|---------|
| Telemetry removal | N/A (none) | Yes | Firefox default | Chromium default |
| HTTPS upgrade | Yes | No | No | No |
| Tracking list blocking | Yes | No | No | No |
| Ad blocking | Built-in | Bundled uBO | No | No |
| Per-site settings | Yes | No | No | No |
| Container/isolated tabs | No | No | No | No |
| Fingerprint protection | No | Partial | No | No |
| Proxy support | Yes | Yes | Yes | Yes |

**LibreWolf leads in privacy-by-default configuration.** **Aileron leads in built-in privacy tools** (ad blocker, HTTPS upgrade, tracking protection, per-site settings). Aileron lacks container tabs and fingerprint protection.

---

## 9. Internationalization

### Aileron Implementation

- 9 languages: EN, ZH, JA, KO, DE, FR, ES, PT, RU
- Runtime language switching (`:language <code>`)
- TOML-based translation files per locale
- `tr()` and `tr_locale()` functions with English fallback
- `:language-list` command

### Comparison

| Capability | Aileron | Zen | Floorp | qutebrowser |
|------------|---------|-----|--------|-------------|
| Languages supported | 9 | ~40+ (Firefox) | ~40+ (Firefox) | N/A (English UI) |
| Runtime switching | Yes | Yes | Yes | No |
| Translation format | TOML | Fluent (Firefox) | Fluent (Firefox) | N/A |
| Right-to-left | No | Yes | Yes | No |

**Firefox-based browsers lead in i18n** by leveraging Mozilla's Fluent translation system and community translations. Aileron's 9 languages are manually maintained TOML files.

---

## 10. Distribution and Packaging

### Aileron Implementation

- Nix flake (hermetic build environment)
- AUR package (`aileron-git`)
- Flatpak manifest (experimental)
- PKGBUILD for Arch Linux
- Cargo install
- Manual build (cargo build)

### Comparison

| Platform | Aileron | Zen | Floorp | Thorium | qutebrowser | Nyxt | Luakit |
|----------|---------|-----|--------|---------|-------------|------|--------|
| AUR | Yes | Yes | Yes | Yes | Yes | Yes | Yes |
| Flatpak | Experimental | Yes | Yes | No | Yes | Yes | No |
| Nix | Yes | No | No | No | No | No | No |
| Homebrew | No | Yes | Yes | No | Yes | Yes | No |
| Windows | Compile | Yes | Yes | Yes | Yes | Yes | No |
| macOS | Compile | Yes | Yes | Yes | Yes | Yes | Yes |
| AppImage | No | No | No | Yes | Yes | No | No |
| Signed builds | No | Yes | Yes | No | No | No | No |
| Auto-update | No | Yes | Yes | Yes | No | No | No |

**Distribution is Aileron's weakest area.** No signed builds, no auto-update, limited packaging. Firefox/Chromium forks benefit from massive build infrastructure and distribution channels.
