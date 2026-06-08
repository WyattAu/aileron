# GUI Refactor Analysis: Technology Evaluation & Migration Roadmap

**Date:** 2026-06-08
**Scope:** Aileron v0.22+ frontend architecture
**Status:** DRAFT

---

## 1. Current Architecture Assessment

### 1.1 What We Have

Aileron's GUI is a hybrid of three rendering systems layered on top of each other:

| Layer | Technology | Purpose | Lines of Code |
|-------|-----------|---------|---------------|
| Window Manager | winit 0.30 | Main window, event loop | ~1,100 (main.rs) |
| UI Chrome | egui 0.31 + wgpu 24 | Status bar, URL bar, sidebar, tabs, command palette | ~2,750 (panels/mod.rs, tab_list.rs, central_panel.rs) |
| Web Content | wry 0.55 (WebKitGTK) | Web page rendering | ~2,560 (wry_engine.rs, offscreen_webview.rs) |
| Internal Pages | Inline HTML/JS strings | New tab, settings, reader, file browser, error pages | ~1,400 (wry_pages.rs) |

**Total UI surface:** ~8,810 LOC across 7 files.

### 1.2 Current Rendering Pipeline

```
winit Window
├── wgpu Render Pass (via egui)
│   ├── Status Bar (TopBottomPanel::top)
│   ├── Tab List / Sidebar (SidePanel::left)
│   ├── Central Panel (Area)
│   │   ├── Offscreen textures (wry snapshot → RGBA → TextureHandle)
│   │   └── Resize handles (drag interaction)
│   ├── URL Bar / Command Palette (TopBottomPanel::bottom)
│   ├── Find Bar (Area)
│   ├── History / Bookmark / Help overlays
│   └── Version string
├── wry Child Windows (X11 native mode, behind egui overlay)
│   └── Per-pane WebKitGTK webviews positioned via set_bounds()
└── Offscreen WebKitGTK (offscreen mode)
    └── Per-pane capture → pixel transfer → texture upload
```

### 1.3 Key Pain Points

| # | Pain Point | Location | Severity |
|---|-----------|----------|----------|
| P1 | God-class event handler (`window_event` = 950 lines) | app_handler.rs:98-1044 | High |
| P2 | `about_to_wait` = 370 lines of frame orchestration | app_handler.rs:1055-1423 | High |
| P3 | Coordinate transform duplication (5+ places) | app_handler.rs:662, 743, 837, 882 | Medium |
| P4 | Custom protocol handler duplicated in two files | wry_engine.rs:349, offscreen_webview.rs:197 | Medium |
| P5 | Focus management is fragile and platform-specific | app_handler.rs:226-258, 459-473 | High |
| P6 | Dual-mode branching everywhere (native vs offscreen) | app_handler.rs, central_panel.rs | Medium |
| P7 | 30-field struct (`AileronApp`) mixing 4 concerns | main.rs:38-130 | Medium |
| P8 | Internal pages are HTML strings embedded in Rust | wry_pages.rs (~1,400 LOC) | Low |
| P9 | Settings page duplicates config.rs structure | wry_pages.rs:966 | Low |
| P10 | No accessibility support (egui limitation) | All egui panels | Medium |

### 1.4 What egui Does Well

- Immediate mode: no retained state, easy to reason about
- Zero-cost: no DOM, no layout engine, direct wgpu rendering
- Rust-native: full type system, no JS boundary
- Keyboard-driven: natural fit for vim-style keybindings
- Custom rendering: terminal output, resize handles, overlays

### 1.5 What egui Does Poorly

- Text editing: URL bar autocomplete, command palette search are basic `TextEdit`
- Styling: no CSS, no animations, no transitions, manual color/layout
- Accessibility: no ARIA, no screen reader support, no semantic HTML
- Component library: no rich dropdown, no context menu, no tooltips with HTML
- Theming: limited to color/font overrides, no dark/light mode transitions
- Internal pages: already implemented as HTML -- egui can't render them natively

### 1.6 Critical Observation

**Items 7-14 of the UI are ALREADY HTML** (new tab page, settings, reader mode, file browser, error pages, welcome page). These are rendered inside the wry webview as injected HTML. The migration path is to extend this: move the remaining chrome (status bar, URL bar, sidebar, command palette, find bar) from egui into the same HTML rendering context.

---

## 2. Technology Evaluation

### 2.1 Option A: Migrate to Tauri (Web Frontend)

**Architecture:**
```
Tauri Window (wry webview = full window)
├── HTML/CSS/JS Chrome (React/Svelte/Vue/Leptos)
│   ├── Status bar, URL bar, sidebar, tabs
│   ├── Command palette, find bar, overlays
│   ├── New tab page, settings, reader mode (already HTML)
│   └── Internal pages (already HTML)
├── Child WebViews (wry build_as_child, per pane)
│   └── Web content (same as current native mode)
└── Rust Backend (Tauri commands)
    ├── BSP tree, keybinding system, navigation
    ├── Bookmark/history management
    ├── Ad blocking, content scripts
    └── Configuration
```

**Relevant Tauri Ecosystem:**

| Component | Why Relevant | Source |
|-----------|-------------|--------|
| Tauri v2 core | Window management, IPC, plugins | tauri-apps/tauri |
| tauri-plugin-context-menu | Native right-click menus | c2r0b/tauri-plugin-context-menu |
| tauri-plugin-clipboard | Clipboard read/write | CrossCopy/tauri-plugin-clipboard |
| Tauri Specta / taurpc | Type-safe IPC (Rust types → TypeScript/Leptos types) | oscartebeaumont/tauri-specta, MatsDK/TauRPC |
| tauri-plugin-aptabase | Privacy-first analytics | aptabase/tauri-plugin-aptabase |
| tauri-plugin-tracing | Structured logging bridge | fltsci/tauri-plugin-tracing |
| vite-plugin-tauri | Build integration | amrbashir/vite-plugin-tauri |

**Pros:**
- Rich UI toolkit (CSS animations, transitions, accessibility, semantic HTML)
- Mature component libraries (shadcn/ui, Radix, Headless UI)
- Settings/new tab pages already HTML -- natural fit
- Type-safe IPC via Specta/taurpc
- Large ecosystem and community
- Cross-platform consistency (one rendering context)
- Theming via CSS custom properties
- Accessibility (ARIA, screen readers)
- Command palette can leverage `<input>` with rich autocomplete
- URL bar can use real browser-like autocomplete with debounce
- No more coordinate transform duplication (CSS layout handles it)

**Cons:**
- Introduces JavaScript or WASM dependency for chrome rendering
- IPC overhead between Rust backend and web frontend (~microseconds per call)
- Focus management still complex (wry child webviews steal focus)
- The 8,810 LOC of egui UI must be rewritten
- Event routing (keybindings) needs JS-side implementation or IPC bridge
- winit-level keyboard capture no longer direct (Tauri owns the window)
- Build system more complex (frontend build + Rust build)
- Larger binary (frontend assets bundled)
- Memory increase (V8/JSContext for chrome rendering)
- egui's immediate-mode simplicity lost (state management needed)

### 2.2 Option B: Migrate to Tauri + Leptos (All-Rust)

**Architecture:** Same as Option A, but the chrome is Leptos compiled to WASM instead of a JS framework.

**Relevant Leptos Ecosystem:**

| Component | Why Relevant | Source |
|-----------|-------------|--------|
| leptos 0.8 | Reactive signals, components, view macro | leptos-rs/leptos |
| leptos-use | Reactive primitives (use_event_listener, use_window) | leptos-use.rs |
| leptos-hotkeys | Declarative keybinding system | gaucho-labs/leptos-hotkeys |
| thaw | Component library (buttons, inputs, tabs, dropdowns) | thaw-ui/thaw |
| leptix | Accessible components (Radix UI port) | RantAI-dev/leptix-ui |
| tailwind-fuse | Tailwind class conflict resolution | gaucho-labs/tailwind-fuse |
| stylance | Scoped CSS modules | basro/stylance-rs |
| tauri-leptos-ssr | Tauri + Leptos integration template | codeitlikemiley/tauri-leptos-ssr |
| Rust shadcn/ui | Copy-paste Leptos components | shadcn-ui.rustforweb.org |

**Pros:**
- All Rust: share types between frontend and backend with zero FFI
- Type-safe IPC: Rust command definitions → Leptos signal bindings
- No JS runtime for chrome (WASM is lighter)
- Leptos signals map naturally to reactive UI (mode changes, URL updates)
- `leptos-hotkeys` could map to our keybinding system
- Growing ecosystem with shadcn/ui port
- Compile-time CSS via stylance or turf

**Cons:**
- Leptos WASM in Tauri is an unusual combo (less community precedent)
- WASM debugging is harder than JS debugging
- Leptos ecosystem much smaller than React/Svelte/Vue
- Component library less mature (thaw is good but limited compared to Radix/shadcn)
- Build complexity: Leptos compilation + WASM + Tauri
- No hot reload for WASM without trunk/cargo-leptos
- Leptos learning curve (signals, fine-grained reactivity)
- Less hiring flexibility (Rust/WASM devs rarer than JS devs)
- Some Leptos crates are immature (leptos-hotkeys has limited features)

### 2.3 Option C: Keep egui, Refactor Architecture

**Architecture:** Keep egui as the chrome renderer but decompose the god-classes.

**Plan:**
- Split `app_handler.rs` into separate handler modules (keyboard, mouse, focus, resize)
- Extract coordinate transforms to utility functions
- Merge custom protocol handlers into shared builder
- Reduce `AileronApp` fields by grouping into sub-structs
- Extract `about_to_wait` tasks into named methods or a task queue

**Pros:**
- Minimal disruption, no architecture change
- Keep all native rendering work
- Keep Rust-native performance
- Keep direct keybinding system (no JS/IPC bridge needed)
- Fastest path to code quality improvement

**Cons:**
- egui's limitations remain (accessibility, styling, text editing)
- Internal pages remain dual-rendered (HTML in webview, chrome in egui)
- Command palette/URL bar remain basic egui TextEdit
- No CSS animations/transitions
- Long-term ceiling on UI quality

### 2.4 Option D: Hybrid (Tauri window + egui overlay + wry content)

**Architecture:** Keep egui rendering but move the window management to Tauri.

This doesn't solve the fundamental problem -- egui is still the chrome. Tauri would just replace winit as the window manager, adding IPC overhead without benefit. **Rejected.**

---

## 3. Decision Matrix

| Criterion | Weight | Tauri+Leptos | Tauri+JS | Keep egui |
|-----------|--------|-------------|----------|-----------|
| UI quality ceiling | 9 | 8 | 9 | 5 |
| Accessibility support | 8 | 7 | 9 | 2 |
| Keybinding fidelity | 10 | 7 | 6 | 10 |
| Memory efficiency | 6 | 7 | 5 | 9 |
| Build simplicity | 5 | 3 | 5 | 9 |
| Type safety end-to-end | 7 | 10 | 6 | 9 |
| Ecosystem maturity | 6 | 5 | 9 | 7 |
| Migration effort | 7 | 3 | 4 | 8 |
| Long-term maintainability | 8 | 7 | 8 | 5 |
| Internal pages unification | 5 | 9 | 9 | 3 |
| Rust ecosystem alignment | 8 | 10 | 4 | 9 |
| **Weighted Total** | | **663** | **635** | **641** |

### Decision: Tauri + Leptos WASM

**Rationale:**

1. **Internal pages are already HTML.** The new tab page, settings, reader mode, file browser, and error pages total ~1,400 LOC of HTML/JS already embedded in Rust strings. The chrome (status bar, URL bar, sidebar, command palette) is the remaining ~2,750 LOC to migrate. Unifying everything into one rendering context eliminates the dual-rendering problem.

2. **Leptos keeps everything in Rust.** The keybinding system (Action enum, KeyCombo, Mode, KeybindingRegistry) is shared between backend and frontend via `aileron-shared` crate. No serde at runtime for type-safe lookups. Keybinding lookup is a WASM HashMap get -- near-native speed, zero IPC per keystroke.

3. **leptos-hotkeys scope management is a perfect fit for vim modes.** Normal/Insert/Command scopes map directly to Aileron's mode machine. Mode transitions are reactive signal updates, not mutable bool flags.

4. **Accessibility matters for a browser.** A webview chrome (even WASM-rendered) gives semantic HTML, ARIA, screen reader support -- none of which egui provides.

5. **Tauri v2 supports child webviews** via `Window::add_child()`. Content child webviews coexist with the Leptos chrome webview in the same native window. This preserves the native rendering approach we've already built.

6. **Keyboard events are handled in WASM, not Rust.** Tauri v2 does not expose pre-webview keyboard interception. This is not a problem -- leptos-hotkeys handles document-level keydown events, looks up the action in the WASM keybinding registry, and only invokes Tauri IPC for actions that need backend execution. The current 950-line `window_event` match statement is replaced by declarative scope definitions.

### Frontend Framework Decision

For the web chrome frontend within Tauri, the choice is between:

| Framework | Verdict |
|-----------|---------|
| Leptos (Rust WASM) | **Selected** -- all-Rust types, shared keybinding system, reactive signals |
| Svelte | Good DX but introduces JS dependency |
| React + TypeScript | Most ecosystem but heavy, no type sharing with Rust |
| Vanilla HTML/CSS/JS | Simplest but loses Rust type safety and reactive system |

**Recommendation: Leptos WASM.**

Rationale:
- Aileron's chrome is keyboard-driven with a complex mode system -- leptos-hotkeys scope management maps directly to this
- Keybinding types (Action enum, KeyCombo, Mode) are shared between backend and frontend without serde at runtime
- Leptos fine-grained reactivity handles state changes (mode, URL, pane focus) efficiently
- Thaw provides 10/16 required components; the remaining 6 are straightforward to build
- Stylance gives compile-time scoped CSS with Rust-generated class constants
- Build pipeline (trunk + cargo tauri) is officially documented by the Tauri project

---

## 4. Migration Roadmap

### Phase Overview

```
Phase 1: Architecture Decomposition (keep egui)
Phase 2: Tauri Migration (replace winit, keep egui chrome)
Phase 3: Chrome Migration to Leptos WASM (replace egui with Leptos + Thaw)
Phase 4: Integration & Polish
Phase 5: Feature Parity & Release
```

### Phase 1: Architecture Decomposition (2-3 weeks)

**Goal:** Fix the god-classes and eliminate duplication WITHOUT changing the rendering technology. This establishes clean module boundaries that make the Tauri migration straightforward.

**Tasks:**

1. **Split `app_handler.rs` into handler modules:**
   ```
   src/input/
   ├── mod.rs
   ├── keyboard_handler.rs      # window_event keyboard section (~350 lines)
   ├── mouse_handler.rs         # mouse input + cursor movement (~270 lines)
   ├── focus_handler.rs         # mode transitions, webview focus (~100 lines)
   └── ime_handler.rs           # Wayland IME workaround (~113 lines)
   ```
   Keep `app_handler.rs` as a thin dispatcher (~200 lines) that calls into handlers.

2. **Extract coordinate transforms:**
   ```
   src/ui/
   ├── coord.rs                 # screen_to_pane_local(rect, pos, sidebar_w, top_h) -> (f64, f64)
   ```
   Replace all 5+ duplicate calculations with a single function.

3. **Merge custom protocol handlers:**
   ```
   src/servo/
   └── protocol.rs              # shared custom protocol builder
   ```
   Called by both `wry_engine.rs` and `offscreen_webview.rs`.

4. **Group `AileronApp` fields into sub-structs:**
   ```
   main.rs:
   struct AileronApp {
       window: WindowState,
       gpu: GpuState,
       app: AppState,           // existing
       wry: WryState,            // panes, events, focus
       offscreen: OffscreenState, // textures, buffers, captures
       terminal: TerminalState,  // if feature="terminal"
       ui: UiState,              // pending_new_tab, url_bar_focused, etc.
   }
   ```

5. **Decompose `about_to_wait` into frame tasks:**
   ```
   src/frame_tasks/
   ├── mod.rs
   ├── workspace.rs             # workspace restore
   ├── filter.rs                 # filter list updates
   ├── wry_events.rs            # wry event processing (existing)
   ├── offscreen_events.rs      # offscreen event processing (existing)
   ├── tab_close.rs             # tab close queue
   ├── terminal.rs               # terminal output polling
   ├── bookmark.rs              # bookmark import
   ├── scroll.rs                 # scroll mark jumps
   ├── memory.rs                 # memory limit enforcement
   └── texture.rs               # texture capture + upload
   ```

**Exit Criteria:**
- `app_handler.rs` < 300 lines
- Zero coordinate transform duplication
- Single custom protocol implementation
- All 1,143 lib tests pass
- Zero clippy warnings

### Phase 2: Tauri Migration (3-4 weeks)

**Goal:** Replace winit with Tauri as the window manager while keeping egui as the chrome renderer. This is an intermediate step that validates the Tauri integration before the full chrome migration.

**Tasks:**

1. **Add Tauri v2 dependency:**
   ```toml
   [dependencies]
   tauri = { version = "2", features = ["wry"] }
   tauri-build = { version = "2" }
   ```

2. **Convert `main.rs` to Tauri plugin architecture:**
   - Replace `winit::ApplicationHandler` with `tauri::Builder`
   - Move app state into Tauri managed state
   - Register Tauri commands for IPC

3. **Preserve egui rendering in Tauri context:**
   - Tauri v2 supports custom rendering via `WebviewBuilder::with_asynchronous_plugin` or by keeping the egui wgpu surface on the same window
   - Alternative: render egui chrome into an offscreen texture, display in the Tauri webview as a CSS background-image (hacky but works for transition)
   - Best approach: Use Tauri's `WindowEvent` to feed events into the existing egui+wgpu pipeline

4. **Preserve wry child window embedding:**
   - Tauri uses wry internally -- the child window approach must work within Tauri's window management
   - May need to access the underlying wry `Webview` to call `build_as_child`

5. **Keyboard event routing:**
   - Tauri v2 intercepts window events before the webview
   - Route to keybinding system in Rust, only forward to webview in Insert mode

6. **Keep dual rendering mode:**
   - Native (child windows) and offscreen both supported during transition

**Exit Criteria:**
- Tauri window launches with egui chrome rendered correctly
- Keybindings work identically to pre-migration
- Web content renders in child windows (native mode)
- All tests pass

### Phase 3: Chrome Migration to Leptos WASM (4-6 weeks)

**Goal:** Replace egui chrome with Leptos WASM components rendered in the Tauri webview. This is the core refactor.

**Tasks:**

1. **Create the shared types crate:**
   ```
   aileron-shared/
   ├── Cargo.toml              # [dependencies] serde, uuid, url
   └── src/
       ├── lib.rs
       ├── action.rs           # Action enum (50+ variants)
       ├── keybinding.rs       # KeyCombo, KeybindingRegistry (from current input/keybindings.rs)
       ├── mode.rs             # Mode enum (Normal, Insert, Command)
       ├── pane.rs             # PaneInfo, Tab, Rect (for IPC)
       └── config.rs           # Config types (subset for chrome display)
   ```

2. **Create the Leptos chrome crate:**
   ```
   chrome/
   ├── Cargo.toml              # [dependencies] leptos, thaw, leptos-use, leptos-hotkeys, aileron-shared
   ├── Trunk.toml              # WASM build config
   ├── index.html              # WASM entry point
   └── src/
       ├── main.rs             # wasm_main()
       ├── app.rs              # ChromeApp root component
       ├── tauri_bridge.rs     # invoke/listen wrappers (wasm-bindgen → Tauri API)
       ├── components/
       │   ├── mod.rs
       │   ├── status_bar.rs   # Mode indicator, pane count, URL, git hash
       │   ├── sidebar.rs      # Pane list, tab list, new tab button
       │   ├── url_bar.rs      # Thaw AutoComplete with history/bookmark suggestions
       │   ├── command_palette.rs  # Custom: Thaw Popover + Input + fuzzy filter
       │   ├── find_bar.rs     # Thaw Input + next/prev/close
       │   ├── history_panel.rs    # Thaw Drawer or Modal
       │   ├── bookmark_panel.rs   # Thaw Drawer or Modal
       │   ├── help_dialog.rs      # Thaw Modal
       │   ├── tab_search.rs       # Thaw Modal + Input
       │   └── resize_handle.rs    # Custom drag interaction
       ├── keybindings.rs      # leptos-hotkeys scope definitions per mode
       └── styles/
           ├── status_bar.module.css
           ├── sidebar.module.css
           ├── url_bar.module.css
           ├── command_palette.module.css
           └── theme.css        # CSS custom properties (colors, dimensions)
   ```

3. **Port keybinding system to Leptos WASM:**
   - Move `KeybindingRegistry::load_defaults()` into `aileron-shared`
   - Compile keybinding lookup into WASM (HashMap, zero IPC per keystroke)
   - Use `leptos-hotkeys` scope management:
     - `normal` scope: j, k, h, l, i, :, q, Ctrl+W, Ctrl+T, etc.
     - `insert` scope: empty (events propagate to content child webview)
     - `command` scope: Escape only
   - Multi-key sequences (g then t for new tab) via leptos-hotkeys combo support
   - UI-only actions (focus URL bar, open palette) handled entirely in Leptos
   - Backend actions (navigate, split, close) invoke Tauri commands

4. **Implement Tauri IPC bridge:**
   - `chrome/src/tauri_bridge.rs`: typed wrappers around `window.__TAURI__.core.invoke()`
   - Rust backend: `src/commands.rs` with `#[tauri::command]` handlers
   - State sync: Rust emits events → Leptos listens → updates signals
   - Action dispatch: Leptos invokes → Rust executes → emits state change

5. **Port each egui component to Leptos:**

   | egui Component | Leptos/Thaw Component | Complexity |
   |-----------------|---------------------|------------|
   | Status Bar | Custom header + Stylance | Low |
   | Sidebar | Custom nav + Thaw Scrollbar | Low |
   | Tab List | Thaw Tabs + custom items | Low |
   | URL Bar | Thaw AutoComplete | Medium |
   | Command Palette | Custom (Popover + Input + fuzzy) | Medium |
   | Find Bar | Thaw Input in fixed position | Low |
   | History Panel | Thaw Drawer | Low |
   | Bookmark Panel | Thaw Drawer | Low |
   | Help Panel | Thaw Modal | Low |
   | Tab Search | Thaw Modal + Input | Low |
   | Resize Handles | Custom drag interaction | High |
   | Context Menu | Thaw ContextMenu | Low |
   | Tooltips | Thaw Tooltip | Low |

6. **Port internal pages to standalone HTML:**
   - Extract `wry_pages.rs` HTML strings into `src-tauri/pages/*.html`
   - New tab page, settings, reader mode, file browser, error pages
   - Served via the `aileron://` custom protocol (same as current)
   - Content child webviews navigate to these pages directly

7. **Content area layout:**
   - Chrome webview is transparent in the content area
   - Content child webviews positioned via `Window::add_child()` with `LogicalPosition(0, chrome_height)`
   - BSP pane rects mapped to child webview positions/sizes
   - Resize handles in Leptos chrome dispatch `resize_pane` Tauri commands

**Exit Criteria:**
- All chrome rendered in Leptos WASM
- egui removed from the rendering pipeline
- Keybindings work identically (Normal/Insert/Command modes)
- Shared types compile into both native and WASM targets
- Tauri IPC bridge functional (invoke + event listen)
- Internal pages served via custom protocol
- Stylance CSS theming works

### Phase 4: Integration & Polish (2-3 weeks)

**Goal:** End-to-end integration, performance tuning, and accessibility.

**Tasks:**

1. **Performance:**
   - Profile the IPC bridge (Rust ↔ JS) under load
   - Debounce state updates (e.g., URL bar updates at 60fps throttled)
   - Use Tauri events (not polling) for state synchronization
   - Optimize DOM updates (batch mutations, use `requestAnimationFrame`)

2. **Accessibility:**
   - ARIA roles and labels on all interactive elements
   - Screen reader announcements for mode changes, navigation
   - Keyboard navigation (Tab, Shift+Tab) within the chrome
   - High contrast theme
   - Focus indicators

3. **Remove dead code:**
   - Delete `offscreen_webview.rs` (if offscreen mode is retired)
   - Delete egui dependencies from Cargo.toml
   - Delete `src/gfx/renderer.rs` (wgpu renderer, replaced by Tauri)
   - Delete texture capture/upload pipeline
   - Delete coordinate transform utilities

4. **Update build system:**
   - Frontend build step (copy chrome/ into Tauri's web assets)
   - Remove build.rs git hash injection (use Tauri's built-in versioning)
   - Update CI/CD for new build process

5. **Update tests:**
   - Port unit tests for keybinding system (keep in Rust)
   - Add integration tests for IPC commands
   - Add E2E tests for chrome UI (tauri-plugin-webdriver-automation or Playwright)

**Exit Criteria:**
- IPC latency < 1ms for state updates
- Accessibility audit passes
- Zero dead egui code remaining
- CI builds successfully
- All tests pass

### Phase 5: Feature Parity & Release (2-3 weeks)

**Goal:** Restore all features, ship v0.22.

**Tasks:**

1. **Feature parity checklist:**
   - [ ] All keybindings work (Normal, Insert, Command modes)
   - [ ] BSP tree operations (split, close, navigate, resize)
   - [ ] Tab management (add, close, switch, reorder)
   - [ ] Workspace persistence (save/restore)
   - [ ] History, bookmarks, download management
   - [ ] Ad blocking, content filtering
   - [ ] Content scripts, extensions API
   - [ ] Terminal integration (if feature="terminal")
   - [ ] Private mode
   - [ ] Link hints (vimium-style)
   - [ ] Reader mode
   - [ ] Find in page
   - [ ] Zoom
   - [ ] Print

2. **Documentation:**
   - Update architecture.md
   - Update developer guide for Tauri-based development
   - Update extension API docs

3. **Release:**
   - v0.22.0 with Tauri-based GUI

### Timeline Summary

| Phase | Duration | Effort | Risk |
|-------|----------|--------|------|
| Phase 1: Architecture Decomposition | 2-3 weeks | Low | Low |
| Phase 2: Tauri Migration | 3-4 weeks | High | Medium |
| Phase 3: Chrome Web Migration | 4-6 weeks | Very High | High |
| Phase 4: Integration & Polish | 2-3 weeks | Medium | Medium |
| Phase 5: Feature Parity & Release | 2-3 weeks | Medium | Low |
| **Total** | **13-19 weeks** | | |

### Risk Mitigation

| Risk | Mitigation |
|------|-----------|
| Tauri v2 child window embedding breaks | Phase 2 validates early; fallback to offscreen-only |
| leptos-hotkeys needs patches for current Leptos version | Fork + patch; scope API is ~500 LOC |
| Thaw missing command palette | Build custom with Popover + Input + fuzzy list |
| Focus management still platform-specific | Use Tauri's `Webview::set_focus()` API |
| Long WASM compile times in dev | trunk --watch with incremental builds |
| Tauri `unstable` feature for child webviews | Pin tauri version; track stabilization |

---

## 5. Leptos as Chrome Frontend: Deep-Dive Technical Assessment

After technical research into the integration surface, the recommendation changes from vanilla JS to **Leptos WASM** for the following reasons:

1. **All-Rust types end-to-end.** Keybinding definitions, pane state, navigation actions, and config types can be shared between backend and frontend without FFI or serde serialization.
2. **Fine-grained reactivity** maps directly to browser chrome semantics (mode changes, URL updates, pane focus) better than vanilla JS's imperative DOM manipulation.
3. **leptos-hotkeys scope management** is a near-perfect fit for vim-style modal keybindings.
4. **Thaw component library** covers the vast majority of chrome components needed.
5. **Stylance scoped CSS** eliminates class name conflicts without a build-time class hash step.
6. **Trunk + Tauri is officially documented** by the Tauri project itself.

### 5.2 Critical Finding: Keyboard Events Must Be Handled in WASM

Tauri v2 does **not** expose pre-webview keyboard interception. There is no Rust-side equivalent to winit's `WindowEvent::KeyboardInput`. The `WebviewEvent` enum covers navigation, download, and document title changes only. You cannot `preventDefault()` on key events from the Rust side.

This means the keybinding system must work as follows:

```
Keyboard event path:
  OS → Tauri WebView (WebKitGTK/WebView2/WKWebView)
       → Leptos WASM (document-level keydown listener)
            → leptos-hotkeys scope lookup (Normal/Insert/Command)
                 → Action found:
                      → UI-only action: handled in Leptos (focus URL bar, open palette)
                      → Backend action: invoke Tauri command (navigate, split, close)
                 → No action in current scope:
                      → Let event propagate to content child webview
```

**This is not a problem.** It is actually architecturally cleaner than the current approach:

- **Current:** winit event → egui consumed check (fragile, line 226-258) → `app_handler.rs` 950-line match → mode machine → keybinding lookup → action dispatch
- **Proposed:** document keydown → leptos-hotkeys scope check → invoke Tauri command → Rust action dispatch

The keybinding lookup runs in WASM (near-native speed, zero IPC per keystroke for lookups that don't produce actions). Only action dispatch requires IPC, and actions are infrequent (user presses `j` to scroll = one IPC invoke, not per-frame).

### 5.3 Shared Keybinding Types

The keybinding system can share types between Rust backend and Leptos WASM:

```rust
// aileron-shared/src/keybindings.rs (compiled into both native and WASM)
#[derive(Clone, Serialize, Deserialize)]
pub enum Action {
    ScrollUp, ScrollDown, ScrollLeft, ScrollRight,
    SplitHorizontal, SplitVertical,
    NavigateBack, NavigateForward,
    EnterInsertMode, EnterCommandMode,
    ClosePane, NewTab,
    ToggleLinkHints, ToggleNormalMode,
    // ... 50+ actions
}

// This same enum is used by:
// 1. Rust backend: action execution (navigate, split, close)
// 2. Leptos WASM: keybinding → action mapping via leptos-hotkeys
// 3. Config parsing: user keybinding overrides
```

The `KeybindingRegistry` (currently 600 LOC in `src/input/keybindings.rs`) can be compiled into WASM. The lookup is a HashMap get -- zero-copy, near-native. No serde round-trip per keystroke.

### 5.4 Tauri + Leptos Architecture

```
Tauri Window (wry webview, full window)
├── Chrome Webview (Leptos WASM, transparent background)
│   ├── Status bar (Thaw components + Stylance CSS)
│   ├── Sidebar / Tab list (Thaw VirtualList + Scrollbar)
│   ├── URL bar (Thaw AutoComplete)
│   ├── Command palette (custom, fuzzy search)
│   ├── Find bar (Thaw Input)
│   ├── History / Bookmark panels (Thaw Modal/Drawer)
│   ├── Help dialog (Thaw Modal)
│   └── Mode indicator (signal-driven CSS class)
│
├── Content Child WebViews (per BSP pane, via Window::add_child())
│   ├── Position: LogicalPosition(0, chrome_height)
│   ├── Size: LogicalSize(width, height - chrome_height)
│   └── Coexists with chrome webview in same native window
│
└── Rust Backend (Tauri managed state)
    ├── BSP tree, pane management
    ├── Navigation, bookmark, history
    ├── Ad blocking, content scripts
    ├── Config, workspace persistence
    └── Tauri commands (IPC API for Leptos)
```

**Key architectural detail:** `Window::add_child()` supports multiple child webviews in the same window. The chrome webview (Leptos) is transparent where the content area should be. Content child webviews are positioned behind/alongside it. Both use logical coordinates relative to the parent window.

### 5.5 Leptos Component Mapping

| Current egui Component | Leptos/Thaw Equivalent | Status |
|------------------------|---------------------|--------|
| Status Bar (TopBottomPanel) | Custom `<header>` + Stylance | Build |
| Sidebar (SidePanel) | Thaw `Sidebar` or custom nav | Available |
| Tab List (selectable labels) | Thaw `Tabs` + custom items | Available |
| URL Bar (TextEdit) | Thaw `AutoComplete` + `Input` | Available |
| Command Palette (TextEdit+popup) | Custom (Thaw `Popover` + `Input` + fuzzy) | Build |
| Find Bar (Area) | Thaw `Input` + buttons in fixed position | Build |
| History Panel (overlay) | Thaw `Drawer` or `Modal` | Available |
| Bookmark Panel (overlay) | Thaw `Drawer` or `Modal` | Available |
| Help Panel (overlay) | Thaw `Modal` | Available |
| Tab Search (overlay) | Thaw `Modal` + `Input` | Available |
| Resize Handles (drag) | Custom (CSS resize or JS drag) | Build |
| Context Menu (right-click) | Thaw `ContextMenu` | Available |
| Tooltips | Thaw `Tooltip` | Available |
| Dropdowns | Thaw `Select`, `ComboBox`, `Popover` | Available |
| Scrollbar | Thaw `Scrollbar` | Available |
| Virtual scroll (large lists) | Thaw `VirtualList` | Available |
| Internal pages (new tab, settings) | Standalone HTML served via custom protocol | Already HTML |

**Summary:** 10 of 16 components available in Thaw. 6 need custom build. None are blocked.

### 5.6 Keybinding System with leptos-hotkeys

```rust
// chrome/src/components/modals.rs (Leptos WASM)
use leptos::*;
use leptos_hotkeys::scopes;

#[component]
fn ChromeApp() -> impl IntoView {
    let (mode, set_mode) = create_signal(Mode::Normal);

    // leptos-hotkeys scope management
    leptos_hotkeys::use_hotkeys_scope(
        "normal",
        vec![
            ("j", move |_| invoke("scroll_down", &())),
            ("k", move |_| invoke("scroll_up", &())),
            ("i", move |_| { set_mode.set(Mode::Insert); }),
            (":", move |_| { set_mode.set(Mode::Command); }),
            ("q", move |_| invoke("close_pane", &())),
            ("g t", move |_| invoke("new_tab", &())),
        ],
    );

    leptos_hotkeys::use_hotkeys_scope(
        "insert",
        vec![],
        // Empty -- no keybindings in Insert mode
        // Events propagate to content child webview
    );

    leptos_hotkeys::use_hotkeys_scope(
        "command",
        vec![
            ("Escape", move |_| { set_mode.set(Mode::Normal); }),
        ],
    );

    // Scope transitions
    Effect::new(move |_| {
        match mode.get() {
            Mode::Normal => leptos_hotkeys::enable_scope("normal"),
            Mode::Insert => {
                leptos_hotkeys::disable_scope("normal");
                invoke("focus_content_webview", &());
            }
            Mode::Command => {
                leptos_hotkeys::disable_scope("normal");
                // Focus command palette input
            }
        }
    });

    view! {
        <header class=mode_indicator_class>/* status bar */</header>
        <nav>/* sidebar + tabs */</nav>
        // ... chrome components
    }
}
```

**Why this works better than current approach:**
- Mode scoping is declarative, not a 950-line match statement
- Keybindings are data, not nested if/else chains
- No "egui consumed" bypass hack (P7 in pain points)
- Mode transitions are reactive signals, not mutable bool flags
- Content focus management is explicit (invoke Tauri command to focus child webview)

### 5.7 Tauri IPC Bridge for Leptos

```rust
// chrome/src/tauri_bridge.rs (Leptos WASM side)
use wasm_bindgen::prelude::*;

#[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"])]
pub async fn invoke(cmd: &str, args: JsValue) -> JsValue;

#[wasm_bindgen(js_namespace = ["window", "__TAURI__", "event"])]
pub fn listen(event: &str, handler: &js_sys::Function);

// Typed Tauri command wrappers
pub async fn navigate(url: &str) -> Result<(), String> {
    let args = serde_json::json!({ "url": url });
    invoke("navigate", serde_wasm_bindgen::to_value(&args)?).await;
    Ok(())
}

pub async fn close_pane(pane_id: &str) -> Result<(), String> {
    let args = serde_json::json!({ "paneId": pane_id });
    invoke("close_pane", serde_wasm_bindgen::to_value(&args)?).await;
    Ok(())
}

// Rust → Leptos state sync
pub fn on_mode_change(callback: impl Fn(Mode) + 'static) {
    listen("mode-changed", &js_sys::Function::new2(move |_, event| {
        let payload = js_sys::Reflect::get(&event, &JsValue::from_str("payload"))
            .unwrap();
        let mode: Mode = serde_wasm_bindgen::from_value(payload).unwrap();
        callback(mode);
    }));
}

pub fn on_panes_changed(callback: impl Fn(Vec<PaneInfo>) + 'static) {
    listen("panes-changed", &js_sys::Function::new2(move |_, event| {
        let payload = js_sys::Reflect::get(&event, &JsValue::from_str("payload"))
            .unwrap();
        let panes: Vec<PaneInfo> = serde_wasm_bindgen::from_value(payload).unwrap();
        callback(panes);
    }));
}
```

**Rust backend side (Tauri commands):**

```rust
// src-tauri/src/commands.rs
#[tauri::command]
async fn navigate(state: State<'_, AppState>, url: String) -> Result<(), String> {
    state.navigate(&url).map_err(|e| e.to_string())
}

#[tauri::command]
async fn close_pane(state: State<'_, AppState>, pane_id: String) -> Result<(), String> {
    let id = Uuid::parse_str(&pane_id).map_err(|e| e.to_string())?;
    state.close_pane(&id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn split_horizontal(state: State<'_, AppState>) -> Result<PaneInfo, String> {
    state.split_horizontal().map(|pane| pane.into()).map_err(|e| e.to_string())
}
```

### 5.8 Build Pipeline

```
aileron/
├── Cargo.toml                  # Workspace root
├── Cargo.lock
├── Trunk.toml                  # Leptos WASM build config
├── chrome/                     # Leptos frontend (WASM target)
│   ├── Cargo.toml              # [dependencies] leptos, thaw, leptos-use, leptos-hotkeys
│   ├── index.html             # WASM entry point
│   └── src/
│       ├── main.rs             # wasm_main() entry
│       ├── app.rs              # ChromeApp component
│       ├── components/
│       │   ├── status_bar.rs
│       │   ├── sidebar.rs
│       │   ├── url_bar.rs
│       │   ├── command_palette.rs
│       │   ├── find_bar.rs
│       │   └── modals.rs       # History, bookmarks, help, tab search
│       ├── tauri_bridge.rs     # invoke/listen wrappers
│       └── styles/
│           ├── status_bar.module.css
│           ├── sidebar.module.css
│           └── ...
├── src/                        # Rust backend (native target)
│   ├── main.rs                 # Tauri entry (tauri::Builder)
│   ├── commands.rs             # Tauri IPC commands
│   └── ...
├── src-tauri/
│   ├── tauri.conf.json         # Tauri config (beforeDevCommand: trunk serve)
│   └── Cargo.toml              # tauri deps
└── aileron-shared/              # Shared types (compiled into both targets)
    ├── Cargo.toml              # [dependencies] serde, uuid, url
    └── src/
        ├── keybindings.rs      # Action enum, KeyCombo, KeybindingRegistry
        ├── pane.rs             # PaneInfo, Tab, Rect
        ├── mode.rs             # Mode enum
        └── config.rs           # Config types
```

**Build commands:**
```bash
# Development (hot reload)
cargo tauri dev
  → trunk serve (port 1420, WASM hot reload via WebSocket)
  → Tauri loads http://localhost:1420

# Production
cargo tauri build
  → trunk build --release (WASM → dist/)
  → Tauri bundles dist/ into native binary
```

### 5.9 CSS Theming with Stylance

```css
/* chrome/src/styles/status_bar.module.css */
.container {
    display: flex;
    align-items: center;
    height: var(--status-bar-height, 32px);
    background: var(--color-surface);
    padding: 0 8px;
    font-family: var(--font-mono);
    font-size: 13px;
}

.mode-badge {
    padding: 2px 6px;
    border-radius: 3px;
    font-weight: bold;
    min-width: 60px;
    text-align: center;
}

.mode-normal { background: var(--color-mode-normal); color: var(--color-bg); }
.mode-insert { background: var(--color-mode-insert); color: var(--color-bg); }
.mode-command { background: var(--color-mode-command); color: var(--color-bg); }
```

```rust
// chrome/src/components/status_bar.rs
use stylance::import_crate_style;

#[import_crate_style]
mod status_bar_css {
    "status_bar.module.css"
}

#[component]
fn StatusBar(mode: ReadSignal<Mode>) -> impl IntoView {
    view! {
        <header class=status_bar_css::container>
            <span class=match mode.get() {
                Mode::Normal => status_bar_css::mode_normal,
                Mode::Insert => status_bar_css::mode_insert,
                Mode::Command => status_bar_css::mode_command,
            }>
                {format!("{:?}", mode.get())}
            </span>
            // ... pane count, url, etc.
        </header>
    }
}
```

Stylance generates Rust constants from CSS class names at compile time. No typos, no runtime class name resolution, scoped to the component.

### 5.10 Performance Considerations

| Metric | Estimate | Notes |
|--------|----------|-------|
| WASM binary size (chrome) | 300-600 KB gzipped | Leptos + thaw + leptos-use |
| WASM cold parse time | 50-100ms | One-time at startup |
| Keystroke-to-action latency | <1ms | WASM HashMap lookup, no IPC |
| Action dispatch IPC | ~0.1-0.5ms | Tauri invoke (async, non-blocking) |
| State sync Rust→Leptos | ~0.1ms per event | Tauri emit → Leptos signal update |
| DOM update (Leptos fine-grained) | <1ms | Direct DOM node mutation, no VDOM diff |
| Incremental WASM compile | 10-30s | trunk --watch, acceptable for dev |

**Key optimization:** Keybinding lookup happens entirely in WASM. Only the ~50 defined actions require IPC invocation. A user holding `j` to scroll produces one IPC call per keypress (scroll_down), not per-frame. The IPC is async and non-blocking -- it does not stall the UI.

### 5.11 Risks and Mitigations

| Risk | Severity | Mitigation |
|------|----------|-----------|
| leptos-hotkeys targets Leptos 0.6, may need patches for 0.7/0.8 | Medium | Fork and patch; scope management API is small (~500 LOC) |
| Thaw missing command palette component | Low | Build custom with Thaw Popover + Input + fuzzy list |
| Tauri child webview focus management still platform-specific | Medium | Phase 2 validates early; use `Webview::set_focus()` API |
| No pre-webview keyboard interception from Rust | Low | WASM handles all key events; Rust only receives action invokes |
| Long WASM compile times | Low | trunk --watch with incremental builds; cargo-leptos as alternative |
| Leptix requires nightly Rust | Low | Use Thaw instead (stable Rust) |
| Tauri `unstable` feature needed for child webviews | Low | Pin tauri version; track stabilization |

### 5.12 Verdict

**Leptos WASM as chrome frontend is technically sound and architecturally preferable** to vanilla JS when the goal is maximizing Rust ecosystem usage. The key findings:

1. **Keyboard events work via leptos-hotkeys scopes** -- no Rust-side interception needed
2. **Child webviews for content are supported** via `Window::add_child()`
3. **Shared types between backend and frontend** eliminate serde friction
4. **Component library (Thaw)** covers 10/16 required components
5. **Build pipeline (trunk + cargo tauri) is officially documented**
6. **WASM performance is adequate** for a chrome UI with ~16 components

The only significant work is building 6 custom components (command palette, resize handles, and 4 layout containers). These are straightforward Leptos components, not fundamental blockers.

---

## 6. Appendix: Specific Ecosystem Components Evaluated

### From Tauri List -- Relevant

| Component | Relevance | Notes |
|-----------|-----------|-------|
| tauri-plugin-context-menu | High | Native right-click on chrome elements |
| tauri-plugin-clipboard | High | Clipboard access for copy/paste |
| Tauri Specta | Medium | Not needed for Leptos (shared Rust types) |
| tauri-plugin-tracing | Medium | Structured logging bridge |
| tauri-plugin-aptabase | Low | Analytics, future consideration |
| tauri-plugin-theme | Medium | Dynamic theme switching |
| tauri-plugin-webdriver-automation | Medium | E2E testing |
| tauri-update-cloudflare | Low | Auto-update infrastructure |

### From Tauri List -- Not Relevant

| Component | Reason |
|-----------|--------|
| All mobile plugins (Android/iOS) | Aileron is desktop-only |
| Bluetooth, NFC, serial port | Not a browser feature |
| VPN, networking, SSH | Out of scope |
| IAP, store integrations | Not applicable |
| Audio/video processing | Not a media app |
| Blockchain/wallet | Not applicable |

### From Leptos List -- Used in Migration

| Component | Phase | Role |
|-----------|-------|------|
| leptos | 3 | Core reactive framework |
| leptos-use | 3 | use_event_listener, use_window, reactive primitives |
| leptos-hotkeys | 3 | Vim-mode scope management for keybindings |
| thaw | 3 | Component library (Input, AutoComplete, Tabs, Modal, etc.) |
| stylance | 3 | Scoped CSS modules |
| tailwind-fuse | 4 | Utility class conflict resolution (if Tailwind adopted) |
| tauri-leptos-ssr | 2 | Reference template for Tauri + Leptos integration |
| trunk | 2 | WASM build tool |

### From Leptos List -- Future Consideration

| Component | Reason |
|-----------|--------|
| leptix | Headless Radix port; useful if we want unstyled primitives instead of Thaw |
| Rust shadcn/ui | Leptos shadcn port; alternative to Thaw for copy-paste components |
| leptos-chartistry | Not needed for browser chrome |
| leptos-pdf | Future: PDF viewer in-browser |
| leptos-i18n / leptos-fluent | Future: localization |
| leptos-darkmode | Future: theme switching |

### From Leptos List -- Not Relevant

| Component | Reason |
|-----------|--------|
| SSR templates (Axum, Actix, Spin) | Aileron is a desktop app, not a web server |
| Blog templates | Not applicable |
| Leaflet/MapLibre | Not a mapping app |
| Chart libraries | Not a data viz app |
| OIDC/auth | Not applicable |
| Meilisearch | Not applicable |
