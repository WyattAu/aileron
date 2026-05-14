# Aileron Path Forward: Gap Analysis and Roadmap

## Based on Open-Source Browser Comparison (2026-05-12)

### Browsers Analyzed: Zen, Floorp, Thorium, LibreWolf, qutebrowser, Nyxt, Luakit

---

## Part 1: What Aileron Does Best

### Unique Strengths (No Other Browser Has These)

| Feature | Implementation | Competitive Moat |
|---------|---------------|-------------------|
| **BSP tiling** | Unlimited-depth binary space partition tree | Zen has 2-pane split only; no other browser tiles |
| **Embedded terminal** | alacritty_terminal + portable_pty + egui rendering | No other browser has a native terminal pane |
| **MCP server** | JSON-RPC stdio with 10 browser automation tools | No other browser has MCP/AI integration |
| **Offscreen texture compositing** | WebKitGTK -> RGBA -> wgpu texture -> egui overlay | All others use native OS widgets |
| **Pure action dispatch** | Action -> ActionEffect (no side effects in parsing) | qutebrowser has side effects in command handlers |
| **Lua + WebExtensions hybrid** | Lua scripting + MV3 API traits | Nyxt has Lisp only; Luakit has Lua only |
| **Built-in ad blocker** | EasyList parser with Aho-Corasick, $redirect/$csp/$badfilter | No other non-Firefox browser has this |
| **Privacy toolkit** | HTTPS upgrade + Disconnect list + DNT/GPC + per-site settings | LibreWolf has config-only; Aileron has runtime UI |
| **Nix flake** | Hermetic build environment | No other browser provides this |
| **Workspace persistence** | Save/load BSP tree with URLs to SQLite | Zen has session store but no BSP state |

### Strong Advantages

| Feature | Status |
|---------|--------|
| Rust type safety (memory-safe, no GC) | vs Python (qutebrowser), Lisp (Nyxt), C (Luakit) |
| Small binary (21 MB) | vs Thorium (120 MB), Zen (90 MB) |
| Test coverage (1,295 tests) | competitive for codebase size |
| Pre-commit quality gates (6 gates) | no other analyzed browser has this |

---

## Part 2: Critical Gaps (What Aileron Is Missing)

### Tier 1: Must-Fix Before v1.0.0

#### 1. WebExtensions JS Runtime

**Problem:** Background scripts are loaded from manifest but never executed. Content scripts are injected via Lua -> JS but there is no JS runtime for background service workers.

**Impact:** Extensions that rely on background logic (ad blockers, password managers, notification handlers) cannot function.

**How others solve this:** Zen/Floorp use SpiderMonkey (Gecko's JS engine). Thorium uses V8 (Blink's JS engine). Both are deeply integrated with their rendering engine.

**Aileron's constraint:** wry wraps WebKitGTK, which uses JavaScriptCore. There is no standalone JS runtime bundled. Options:
- Bundle a lightweight JS runtime (boa, quick_js) for background scripts
- Use WebKitGTK's JavaScriptCore via FFI for background script evaluation
- Accept this as a v1.1+ feature

**Recommendation:** Use WebKitGTK's JavaScriptCore via FFI. `webkit2gtk` already depends on `javascriptcore-rs` (present in Cargo.lock). Evaluate `jsc` crate for standalone JSC evaluation.

#### 2. Distribution Infrastructure

**Problem:** No signed builds, no auto-update, no Flatpak stable, no Homebrew cask, no Windows/macOS daily-driver support.

**Impact:** Users must build from source. This is the single largest barrier to adoption.

**How others solve this:** Zen and Floorp use Mozilla's build infrastructure. Thorium uses Chromium's. qutebrowser uses PyPI + system packages. All have CI/CD for signed releases.

**Recommendation:**
1. Set up GitHub Actions for Linux release builds (AppImage, Flatpak, tarball)
2. Add Flatpak stable build (complete the experimental manifest)
3. Add code signing (Cosign for Linux, notarization for macOS)
4. Add auto-update check (compare installed vs. latest version via GitHub API)

#### 3. Tab-within-Pane (Multiple Tabs Per Pane)

**Problem:** Each BSP leaf node is a single pane with one URL. No concept of "tabs within a pane" -- every URL gets its own pane in the BSP tree.

**Impact:** Users cannot have multiple tabs in a single pane area. This is a standard browser feature that every other browser provides.

**How others solve this:** All browsers (including Zen, qutebrowser, Nyxt, Luakit) use a flat tab list. The tab bar is a 1D container.

**Recommendation:** Add an optional tab list within each pane. The BSP tree manages pane geometry; each pane can optionally have a tab bar showing 1-N tabs. This is orthogonal to tiling -- the pane is still a BSP leaf, but it can switch between multiple webviews.

#### 4. Container/Isolated Tabs

**Problem:** All panes share the same cookie jar, storage, and session. No isolation between contexts (e.g., personal vs. work).

**Impact:** Users cannot separate browsing contexts for privacy or security.

**How others solve it:** Firefox Multi-Account Containers. Chrome Profiles. LibreWolf inherits Firefox containers.

**Recommendation:** Implement per-pane cookie/storage isolation. Each pane can optionally be assigned a "context" that has its own cookie jar, localStorage, and sessionStorage. This can be built on top of the existing per-site settings infrastructure.

### Tier 2: Important for Competitive Parity

#### 5. Keyboard Macro Recording

**Problem:** No way to record and replay key sequences.

**Impact:** Power users who perform repetitive multi-step workflows (e.g., "open GitHub, click Issues, filter by label") must type the same sequence every time.

**How others solve this:** qutebrowser has `:macro-record` / `:macro-run`. Vim has `q{register}@q`.

**Recommendation:** Record sequences of Action values (not raw key events) for deterministic replay. Store as named macros in config.

#### 6. Vertical Tabs Enhancement

**Problem:** Tab sidebar exists but is a simple list. No tree structure for grouping.

**Impact:** Users with many tabs (20+) cannot organize them hierarchically.

**How others solve this:** Zen has workspaces. Floorp has vertical tabs. Firefox has Tree Style Tab extension.

**Recommendation:** Add tab grouping (folders within the sidebar) with collapse/expand. This is orthogonal to the BSP tree -- groups organize the tab list, not the layout.

#### 7. Fingerprint Protection

**Problem:** No randomization of canvas, WebGL, font, or audio fingerprinting vectors.

**Impact:** Users can be tracked across sites even with ad blocking and DNT headers.

**How others solve this:** Tor Browser randomizes all vectors. LibreWolf enables resistFingerprinting. Brave has built-in fingerprint blocking.

**Recommendation:** Inject JS that overrides `canvas.toDataURL()`, `WebGLRenderingContext.getParameter()`, and audio context APIs with deterministic or randomized values. Can be implemented as a content script.

#### 8. Form Autofill

**Problem:** Password manager integration exists (Bitwarden CLI) but no general-purpose form autofill (addresses, credit cards, etc.).

**Impact:** Users must manually fill non-password forms.

**How others solve this:** Firefox/Chrome have built-in form autofill with address and payment methods.

**Recommendation:** Extend the existing Bitwarden integration to support non-password items (identities, cards, notes). The `browser_fill_form` MCP tool already demonstrates the pattern.

#### 9. Reader Mode Enhancement

**Problem:** Reader mode exists (strips CSS, extracts text) but lacks: estimated reading time, text-to-speech, font size controls, save to file.

**Impact:** Reader mode is less useful than Firefox/Safari implementations.

**Recommendation:** Add reading time estimation (word count / 200 WPM), font size toggle, save-to-markdown, and integrate with bookmark system.

#### 10. Download Manager Enhancement

**Problem:** Download manager has progress tracking but lacks: pause/resume, concurrent download limits, download history persistence across restarts.

**Impact:** Downloads are lost on restart; large downloads cannot be paused.

**Recommendation:** Persist download state to SQLite, add pause/resume via HTTP Range headers, add concurrent limit.

### Tier 3: Nice-to-Have

| Feature | Priority | Effort | Notes |
|---------|----------|--------|-------|
| Picture-in-Picture | Low | Medium | Requires WebKitGTK PiP API |
| PDF viewer (built-in) | Low | Medium | Currently uses system viewer |
| WebRTC support | Low | High | Requires WebKitGTK WebRTC |
| Reading list | Low | Low | Simple SQLite table |
| Drag-and-drop tab reorder | Low | Medium | Needs egui DnD support |
| Session manager UI | Low | Medium | Visual workspace list with previews |
| Crash reporter | Low | Medium | Structured crash dump with backtrace |
| `aileron --profile <dir>` | Low | Low | Multi-profile support |

---

## Part 3: Revised Roadmap

### Phase 1: Hardening (v0.19.0) -- IN PROGRESS

- [x] SAFETY comments on all FFI blocks (5 remaining, now 0)
- [x] Flaky test elimination
- [x] Input integration tests (34 tests)
- [x] Lua integration tests (51 tests)
- [x] Frame_tasks integration tests (20 tests)
- [x] Key conversion unit tests (14 tests, previously zero coverage)
- [ ] Audit remaining ~15 WebKitGTK/Cairo FFI for SAFETY comment completeness

### Phase 2: Core Gaps (v0.20.0)

- [ ] Tab-within-pane (multiple tabs per BSP leaf)
- [ ] JS runtime for background scripts (JSC via FFI)
- [ ] Container/isolated tabs (per-pane cookie/storage)
- [ ] Keyboard macro recording
- [ ] Vertical tab groups (tree structure)

### Phase 3: Privacy and Polish (v0.21.0)

- [ ] Fingerprint protection (canvas/WebGL/audio override)
- [ ] Form autofill extension (Bitwarden identities/cards)
- [ ] Reader mode enhancement (reading time, font controls)
- [ ] Download manager persistence and pause/resume
- [ ] Picture-in-Picture

### Phase 4: Distribution (v0.22.0)

- [ ] Stable Flatpak build
- [ ] GitHub Actions release pipeline (AppImage, tarball)
- [ ] Code signing (Cosign)
- [ ] Auto-update check
- [ ] macOS test execution in CI
- [ ] Windows test execution in CI

### Phase 5: Extension Completion (v0.23.0)

- [ ] cookies API
- [ ] alarms API
- [ ] contextMenus API
- [ ] notifications API
- [ ] permissions.request()
- [ ] webNavigation API
- [ ] declarativeNetRequest
- [ ] i18n API

### Phase 6: Sync Protocol (v0.24.0)

- [ ] WebDAV transport implementation
- [ ] Sync execution loop
- [ ] CRDT merge for bookmarks (last-write-wins)
- [ ] Sync status UI (`:sync-status`)
- [ ] Per-site settings sync

### Phase 7: Servo Integration (v0.25.0)

- [ ] Servo Embedder API evaluation
- [ ] Real ServoPane implementation
- [ ] Texture sharing
- [ ] Engine selection runtime toggle
- [ ] Graceful fallback on Servo crash

### Phase 8: v1.0.0 Release (v0.26.0)

- [ ] All v1.0.0 must-have criteria met
- [ ] AUR stable package (not -git)
- [ ] Flatpak on Flathub
- [ ] Complete documentation
- [ ] Performance targets met

---

## Part 4: Architectural Decisions

### ADR-012: Tab-within-Pane Design

**Status:** Proposed

**Context:** BSP tree manages pane geometry. Users need multiple tabs per pane.

**Decision:** Each `Pane` struct gains an optional `TabList` (Vec<Tab>). The pane renders the active tab's webview; inactive tabs are suspended (evicted from memory, URL preserved). Tab bar appears within the pane's allocated rectangle.

**Consequences:**
- Memory efficiency: only active tab uses GPU texture
- Complexity: BSP tree still manages layout; TabList manages per-pane navigation
- Compatibility: does not change the tiling model

### ADR-013: JS Runtime for Extensions

**Status:** Proposed

**Context:** Background scripts need a JS runtime. No standalone JS engine is bundled.

**Decision:** Use JavaScriptCore via `javascriptcore-rs` (already in dependency tree via webkit2gtk). Evaluate a `jsc` crate wrapper for standalone JSC context creation outside of webview.

**Alternatives:**
1. `boa` (pure Rust JS engine) -- smaller but incomplete ES spec
2. `quick-js` (C library, FFI) -- small, fast, ES2020 compatible
3. JavaScriptCore FFI -- full spec, already linked, larger

**Recommendation:** Evaluate `quick-js` for background script evaluation (small footprint, ES2020 compatible, no GTK dependency for non-UI scripts).

### ADR-014: Distribution Strategy

**Status:** Proposed

**Context:** Need signed, auto-updating releases.

**Decision:**
1. GitHub Actions builds release artifacts (Linux x86_64 AppImage + Flatpak, macOS aarch64 + x86_64 DMG, Windows x86_64 MSI)
2. Cosign for container signing
3. GitHub Releases page for distribution
4. Auto-update via GitHub API version check on startup
5. Defer Homebrew, AUR stable, Flathub to community contributors

---

## Part 5: Risk Assessment Post-Comparison

| Risk | Pre-Comparison | Post-Comparison | Mitigation |
|------|---------------|-----------------|------------|
| No one wants a tiling browser | Medium | **Low** -- Zen split-view proves demand | Market split-view users, emphasize terminal + tiling combo |
| Extension ecosystem too small | High | **High** -- MV3 partial without JS runtime | Priority: JS runtime (ADR-013) |
| Distribution gap | High | **High** -- all competitors have auto-update | Priority: release pipeline (Phase 4) |
| Performance regression from features | Medium | **Low** -- offscreen compositing is inherently heavier | Adaptive quality, texture pooling, lazy tab suspension |
| wry dependency lock-in | Medium | **Low** -- Luakit proves WebKitGTK viable long-term | PaneRenderer trait abstraction (already exists) |
| Firefox/Chromium compatibility | N/A | **Low** -- Aileron targets different niche | Position as "developer terminal browser", not Firefox replacement |

---

## Conclusion

Aileron occupies a unique niche: **the only Rust-native, tiling, keyboard-driven browser with an embedded terminal and MCP integration.** No other browser combines all four of these features.

The primary gaps are:
1. **JS runtime for extensions** (enables real WebExtensions)
2. **Tab-within-pane** (standard browser UX expectation)
3. **Distribution infrastructure** (adoption barrier)
4. **Container tabs** (privacy expectation)

The revised roadmap prioritizes these gaps in Phases 2-4 before pursuing Servo integration or extension API completion. This sequence ensures Aileron is competitive on core UX before investing in advanced features.
