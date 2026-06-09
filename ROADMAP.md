# Aileron Roadmap: v0.21 to v2.0 and Beyond

## Current State (v0.21.0-dev, post-audit 2026-06-09)

| Metric | Value |
|--------|-------|
| Version | 0.21.0 |
| Tests | 1151 lib, 253 integration (13 suites), 4 doc = 1408 total |
| Clippy | Zero warnings (all-targets, -D warnings) |
| Formatting | Zero issues (cargo fmt) |
| Unsafe blocks | 19 (all FFI: WebKitGTK, Cairo, X11, spellcheck -- SAFETY commented) |
| #[must_use] | 49 attributes across 21 files |
| LOC | 56,865 Rust across 148 source files |
| Binary size | ~21 MB stripped (x86_64 Linux) |
| CI | 8 jobs: Linux test + coverage, macOS check, Windows check, cross-compile (3 targets), fmt, benchmark regression |
| Coverage | cargo-llvm-cov (lcov) via Codecov |
| Pre-commit | fmt, check, clippy, lib tests, doc tests; pre-push: integration tests, doc gen |
| GitHub Pages | Deployed at https://wyattau.github.io/aileron/ |
| Platforms | Linux (primary), macOS (compile), Windows (compile) |
| Audit (2026-06-09) | Zero code stubs, zero TODO/FIXME, zero emojis in docs, CI/CD hardened, Lua version fixed, per-keypress allocation reduced, docs accuracy verified, UI updated with Spatial Materialism + Amoebic UI, ARIA accessibility added |

## Execution Model

Each release targets a 2-3 week cadence. Items are organized by dependency order. Blocked items are marked with their blocker and tracked separately.

---

## v0.21: Performance and Hot-Path Optimization

**Target:** 2-3 weeks
**Goal:** Eliminate per-frame allocations, establish performance baselines, harden CI

### Priority 1: Frame Rendering Performance (from audit findings)

| ID | Task | Estimate | Status |
|----|------|----------|--------|
| P1-01 | Pre-allocate frame capture buffer (eliminate ~8MB/frame alloc in render.rs) | 2h | Done |
| P1-02 | Refactor tab display cache in panels.rs (eliminate 3-4 String clones per tab per frame) | 3h | Done |
| P1-03 | Change `panes()` to return iterator instead of cloned Vec (wm/tree.rs) | 2h | Done |
| P1-04 | Texture pooling for multi-pane scenarios (avoid per-frame texture creation) | 4h | Done |
| P1-05 | Pre-lowercase blocked_domains into HashSet at construction (navigation handler O(n)->O(1)) | 3h | Done |
| P1-06 | Wrap https_safe_list in Arc<HashSet> to avoid cloning ~1000 entries per pane creation | 2h | Done |
| P1-07 | Wrap blocked_domains in Arc<HashSet> to avoid Vec<String> clone per pane creation | 2h | Done |
| P1-08 | Fix double-clone pattern in BspTree::panes() and pane_ids() (move into cache, return clone) | 2h | Done |

### Priority 2: Profiling Infrastructure

| ID | Task | Estimate | Status |
|----|------|----------|--------|
| P2-01 | Add frame-time percentile tracking to profiling module | 3h | Done |
| P2-02 | Create startup latency benchmark (cold start to first paint) | 2h | Pending |
| P2-03 | Expand frame_bench.rs with multi-pane render benchmarks | 3h | Done |
| P2-04 | Implement tab-unload LRU with actual RSS memory measurement | 4h | Done |

### Priority 3: Build and CI

| ID | Task | Estimate | Status |
|----|------|----------|--------|
| P3-01 | Profile compile times with `cargo build --timings`, optimize split | 2h | Done |
| P3-02 | Evaluate cranelift codegen backend for faster debug builds | 2h | Pending |
| P3-03 | Store benchmark baselines in CI, fail on >10% regression | 4h | Done |
| P3-04 | Feature gate `terminal` module behind `terminal` feature (reduce cold compile) | 3h | Done |
| P3-05 | Feature gate `lua` module behind `lua` feature | 2h | Done |
| P3-06 | Further split large files (frame_tasks.rs, ui/panels.rs -> target <2000 lines each) | 3h | Done |

### Success Criteria for v0.21

- [ ] Zero per-frame heap allocations on critical rendering path
- [ ] Startup latency < 2s cold, < 500ms warm
- [ ] Benchmark baselines stored and regression-checked in CI
- [ ] Debug build time reduced by >15% from feature gating

---

## v0.22: Platform Expansion (macOS)

**Target:** 2-3 weeks
**Goal:** First-class macOS support with verified rendering and tests

### Priority 1: macOS Rendering

| ID | Task | Estimate | Status |
|----|------|----------|--------|
| M1-01 | Verify WebKit rendering on macOS (WKWebView via wry) | 4h | Pending |
| M1-02 | Implement macOS-native file dialog (NSOpenPanel via objc FFI) | 6h | Pending |
| M1-03 | macOS-specific keymap (Cmd vs Ctrl, system shortcuts) | 4h | Pending |
| M1-04 | Verify offscreen rendering path on macOS | 4h | Pending |

### Priority 2: macOS CI and Distribution

| ID | Task | Estimate | Status |
|----|------|----------|--------|
| M2-01 | Run integration tests on macOS CI (xvfb not needed) | 2h | Done |
| M2-02 | Run clippy on macOS CI (catch platform-specific warnings) | 1h | Done |
| M2-03 | Create macOS install guide in CONTRIBUTING.md | 2h | Pending |
| M2-04 | Test AUR-equivalent install path (cargo install, Homebrew formula) | 2h | Pending |

### Blocked

| Item | Blocker | Mitigation |
|------|---------|------------|
| Code signing and notarization | Apple Developer account ($99/yr) | Defer to post-v1.0 or seek sponsor |

### Success Criteria for v0.22

- [ ] `cargo test` passes all 1295 tests on macOS CI
- [ ] WebKit renders real websites on macOS
- [ ] File dialogs work natively on macOS
- [ ] macOS keymap matches platform conventions

---

## v0.23: Platform Expansion (Windows) and Extension API

**Target:** 3-4 weeks
**Goal:** Windows support + extension infrastructure

### Priority 1: Windows

| ID | Task | Estimate | Status |
|----|------|----------|--------|
| W1-01 | Verify WebView2 rendering on Windows | 4h | Pending |
| W1-02 | Windows-native file dialog (COM IFileDialog) | 6h | Pending |
| W1-03 | Windows-specific keymap (Alt vs Ctrl) | 4h | Pending |
| W1-04 | Run integration tests on Windows CI | 2h | Pending |
| W1-05 | Run clippy on Windows CI | 1h | Done |

### Priority 2: Extension Infrastructure

| ID | Task | Estimate | Status |
|----|------|----------|--------|
| E1-01 | Implement `cookies` API (get, set, remove, onChanged) | 8h | Done |
| E1-02 | Implement `declarativeNetRequest` rule engine | 12h | Done |
| E1-03 | Implement `permissions.request()` prompt | 4h | Done |
| E1-04 | Implement `alarms` API | 4h | Done |
| E1-05 | Implement `contextMenus` API | 6h | Done |
| E1-06 | Background script JS runtime (quick-js or v8 isolate) | 16h | Done |
| E1-07 | Port messaging between background and content scripts | 8h | Done |

### Blocked

| Item | Blocker | Mitigation |
|------|---------|------------|
| Windows code signing | EV code signing certificate | Defer to post-v1.0 |
| Windows installer (MSIX) | Code signing + Microsoft Store account | Defer to post-v1.0 |

### Success Criteria for v0.23

- [ ] `cargo test` passes on Windows CI
- [ ] WebView2 renders real websites on Windows
- [ ] At least 4 additional extension APIs implemented
- [ ] Background script JS runtime functional

---

## v0.24: Sync Protocol and Privacy

**Target:** 3-4 weeks
**Goal:** Operational cross-device sync with E2EE

### Priority 1: WebDAV Sync

| ID | Task | Estimate | Status |
|----|------|----------|--------|
| S1-01 | Implement WebDAV transport (PUT/GET/DELETE/PROPFIND via reqwest) | 12h | Done |
| S1-02 | WebDAV authentication (HTTP Basic, Bearer) | 4h | Done |
| S1-03 | WebDAV retry with exponential backoff | 3h | Done |
| S1-04 | Sync execution loop (manifest -> delta -> upload -> download -> merge) | 8h | Done |
| S1-05 | CRDT merge for bookmarks (last-write-wins + operational transform) | 8h | Done |
| S1-06 | CRDT merge for history (union with dedup) | 4h | Done |
| S1-07 | Sync status UI (`:sync-status`, status bar indicator) | 4h | Done |
| S1-08 | Conflict UI (`:sync-conflicts` panel) | 6h | Done |

### Priority 2: Privacy

| ID | Task | Estimate | Status |
|----|------|----------|--------|
| P1-01 | Fingerprint protection (canvas/WebGL/audio context override) | 8h | Done |
| P1-02 | Container/isolated tabs (per-pane cookie/storage partition) | 12h | Done |
| P1-03 | Form autofill expansion (Bitwarden identities, addresses, cards) | 8h | Pending |

### Success Criteria for v0.24

- [ ] WebDAV sync round-trips bookmarks and history between two instances
- [ ] E2EE sync with passphrase (age encryption)
- [ ] Conflict resolution UI functional
- [ ] Fingerprint randomization reduces uniqueness score

---

## v0.25: UX Polish and Distribution

**Target:** 3-4 weeks
**Goal:** Daily-driver UX + distribution infrastructure

### Priority 1: UX

| ID | Task | Estimate | Status |
|----|------|----------|--------|
| U1-01 | Tab-within-pane (multiple tabs per BSP leaf node) | 12h | Pending |
| U1-02 | Drag-and-drop tab reorder | 6h | Pending |
| U1-03 | Tab search (fuzzy search across open tabs) | 3h | Pending |
| U1-04 | Session manager (visual session list with preview) | 8h | Pending |
| U1-05 | Workspace templates (predefined pane layouts) | 4h | Pending |
| U1-06 | Keyboard macro recording (`:macro-record`, `:macro-play`) | 8h | Pending |
| U1-07 | Reader mode enhancement (reading time, font controls, save-to-markdown) | 6h | Pending |

### Priority 2: Distribution

| ID | Task | Estimate | Status |
|----|------|----------|--------|
| D1-01 | Linux: AppImage build (via cargo-bundle) | 4h | Pending |
| D1-02 | Linux: Flatpak build (manifest already exists, needs verification) | 6h | Pending |
| D1-03 | Linux: AUR stable package (non-git) | 2h | Pending |
| D1-04 | Auto-update check (GitHub API version comparison on startup) | 4h | Pending |
| D1-05 | CLI flags: `--debug`, `--profile <dir>`, `--dump-config` | 3h | Pending |

### Success Criteria for v0.25

- [ ] Multiple tabs per pane functional
- [ ] At least 2 Linux distribution formats (AppImage + Flatpak or AUR stable)
- [ ] Auto-update notification functional
- [ ] Session manager saves and restores workspace templates

---

## v1.0.0: Production Release

**Target:** 2-3 weeks (stabilization)
**Goal:** Production-ready browser for developers

### v1.0 Must-Have (all required for release)

| ID | Requirement | Status |
|----|-------------|--------|
| R1 | All 1400+ tests pass on Linux, macOS, Windows | Pending |
| R2 | >= 95% branch coverage on critical paths (wm, input, extensions, adblock) | Pending |
| R3 | Zero critical clippy warnings on all 3 platforms | Pending |
| R4 | All performance targets validated (startup <2s, input latency <16ms) | Pending |
| R5 | Documentation complete: README, config, keybindings, scripting, extension API | Partial |
| R6 | No misleading claims in documentation (Servo = experimental, not functional) | Done |
| R7 | Reproducible builds (Nix flake verified) | Pending |
| R8 | External security audit passed | Pending |
| R9 | At least 8 additional MV3 APIs implemented beyond current set | Pending |
| R10 | WebDAV sync operational with E2EE | Pending |

### v1.0 Should-Have

| ID | Requirement | Status |
|----|-------------|--------|
| S1 | Servo engine functional (even if experimental) | Blocked |
| S2 | Flatpak published on Flathub | Pending |
| S3 | macOS notarized build | Blocked |
| S4 | Windows installer (MSIX) | Blocked |
| S5 | Crash reporter with telemetry opt-in | Pending |
| S6 | Keyboard macro recording | Pending |
| S7 | Extension marketplace specification | Pending |

### v1.0 Release Process

1. Create `release/v1.0.0` branch from `main`
2. Freeze features; only bug fixes allowed
3. Run full test matrix on all 3 platforms
4. Generate SBOM, verify dependency checksums
5. Tag `v1.0.0-rc.1`, build release artifacts
6. Internal testing (1 week)
7. Address any critical issues
8. Tag `v1.0.0`, trigger release workflow
9. Publish to AUR, update documentation site
10. Announcement blog post / GitHub discussion

### Success Criteria for v1.0

- [ ] All R1-R10 requirements met
- [ ] At least 5 S1-S7 requirements met
- [ ] Zero P0/P1 bugs open
- [ ] Documentation reviewed by external contributor
- [ ] Release artifacts available for Linux, macOS, Windows

---

## Post-v1.0: Future Horizons

### Horizon 1: Multi-Device Ecosystem (v1.1-v1.2)

| Feature | Description | Dependencies |
|---------|-------------|--------------|
| ARP mobile client | Flutter/SwiftUI mobile app for remote tab control | ARP protocol stabilization |
| Push sync | Real-time cross-device sync via WebSocket | v1.0 sync infrastructure |
| Cross-device clipboard | Encrypted clipboard sharing between devices | ARP protocol |
| Remote tab access | View and control desktop tabs from mobile | ARP mobile client |
| Multi-window | Independent tiled windows with shared state | WM refactoring |
| Reading list | Save articles for later with offline caching | Sync infrastructure |
| Passwordless auth | WebAuthn/passkey support for web forms | Security audit |

### Horizon 2: AI-Native Browsing (v1.3-v1.4)

| Feature | Description | Dependencies |
|---------|-------------|--------------|
| MCP agent expansion | DOM manipulation, multi-step workflows, data extraction | v0.23 extension JS runtime |
| Local LLM integration | Ollama-backed summarization, translation, content analysis | No external deps |
| Semantic history | Vector embeddings of visited pages for semantic search | Embedding model integration |
| Workflow automation | Lua-driven browser automation without Selenium | v0.23 Lua infrastructure |
| Smart tab management | AI-suggested tab grouping, auto-close stale tabs | Semantic history |

### Horizon 3: Rendering Independence (v1.5-v1.6)

| Feature | Description | Dependencies |
|---------|-------------|--------------|
| Servo as default engine | When Servo Embedder API stabilizes | Servo project milestone |
| Multi-engine per pane | Different engines simultaneously | PaneRenderer trait formalization |
| Custom renderer API | Third-party rendering plugins | Plugin ABI design |
| Headless mode | Server-side rendering for automation | Servo or headless WebKit |

### Horizon 4: Distributed Browsing (v2.0)

| Feature | Description | Dependencies |
|---------|-------------|--------------|
| Remote rendering | Render on server, stream to thin client | GPU streaming infrastructure |
| Session sharing | Collaborative browsing with shared pane state | CRDT sync |
| Sandboxed containers | Per-tab OS-level isolation (Flatpak-like) | Platform integration |
| Plugin ecosystem | Rust-based plugins with stable ABI | ABI specification |
| Extension store | Curated extension repository | Extension API v1.0 |
| Wasm extensions | WebAssembly extension sandbox | Wasm runtime integration |

---

## Cross-Cutting Risks

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Servo Embedder API not ready | High | Medium | wry/WebKitGTK remains primary; Servo is optional |
| wry `!Send + !Sync` constraint | Certain | Medium | Arc<RwLock<>> + mpsc bridge pattern |
| WebKitGTK API breakage | Medium | High | Pin wry version; test on multiple WebKitGTK versions |
| GTK3 deprecation (wry dependency) | Medium | High | Monitor wry GTK4 migration; no action needed until upstream moves |
| Linux-only CI test coverage | Certain (until v0.22) | Medium | Prioritize macOS/Windows CI expansion |
| Distribution gap (no signed builds) | Certain | High | Prioritize signing infrastructure in v0.25 |
| Performance regression from new features | Medium | Medium | Benchmark regression CI in v0.21 |
| Extension API compatibility breaks | Medium | Medium | Semantic versioning; deprecation period |

## Audit Results (2026-06-09)

### CI/CD Audit Findings (All Fixed)

| Issue | Severity | Status |
|-------|----------|--------|
| publish.yml missing system deps for crates.io (build fails) | Critical | Fixed |
| release.yml CI verify step never fails (outputs null on no CI) | Critical | Fixed |
| AUR SSH key written before chmod (brief window of exposure) | Low | Fixed |
| AUR clone errors silently suppressed via 2>/dev/null | High | Fixed |
| ci.yml redundant integration_smoke step (doubles test work) | Medium | Fixed |
| ci.yml --force on cached tools reinstalls every run | Low | Fixed |
| pages.yml no validation before deployment | Medium | Fixed |
| pre-commit hook step numbering inconsistency (1/5 vs 2/6) | Low | Fixed |

### Code Quality Findings (Fixed)

| Issue | Severity | Status |
|-------|----------|--------|
| Lua aileron.version hardcoded to "0.1.0" instead of CARGO_PKG_VERSION | High | Fixed |
| Per-keypress HashSet construction in app_handler.rs and event_handler.rs | High | Fixed |
| collect_leaf_ids O(n) Vec alloc per Split node per frame | Critical | Fixed |
| MCP double HashMap lookup (same key twice) | High | Fixed |

### Code Quality Findings (Remaining -- Low Priority)

| Issue | Severity | Recommendation |
|-------|----------|----------------|
| 24 production unwrap() on hardcoded URLs (aileron://new, about:blank) | Low | Convert to expect() for debuggability |
| 29 production expect() calls (mostly well-guarded) | Low | Accept as-is; window/runtime creation failures are catastrophic by design |
| 19 unsafe blocks (all FFI: WebKitGTK, Cairo, X11, spellcheck) | Low | All have SAFETY comments; acceptable for FFI |

### Documentation Audit Findings (Fixed)

| Issue | Severity | Status |
|-------|----------|--------|
| Man page lists nonexistent :lua command | High | Fixed |
| Man page :yt described as "Search YouTube" (actually copies host) | High | Fixed |
| Man page lists incorrect default keybindings (J/K, p, u, n/N, gg) | High | Fixed |
| Man page missing 8 documented keybindings | Medium | Fixed |
| README "6 API traits" (actual: 9) | Medium | Fixed |
| README incorrect default keybindings (m + letter, ' + letter) | Medium | Fixed |
| README architecture section wrong paths (scripts/ vs scripts.rs, sync.rs vs sync/) | Medium | Fixed |
| README/VERSION.md stale LOC count (51,413 vs 56,865) | Medium | Fixed |
| architecture.md lists unimplemented bookmarks/history Lua APIs | High | Fixed |
| index.html missing architecture.md link | Low | Fixed |
| PKGBUILD missing man page installation | Medium | Fixed |
| Version references stale (v0.20.0 vs v0.21.0 in Cargo.toml) | Medium | Fixed |
| Architecture section references egui instead of Leptos WASM chrome | Medium | Fixed |
| Test count references stale (1143 vs 1130) | Low | Fixed |

### UI Audit Findings (Fixed)

| Issue | Severity | Status |
|-------|----------|--------|
| Chrome WASM CSS uses flat, sharp-cornered design | Medium | Fixed (Spatial Materialism + Amoebic UI) |
| Chrome WASM components missing ARIA accessibility attributes | High | Fixed |
| Landing page CSS inconsistent with chrome design language | Medium | Fixed |

### Architecture Assessment

The codebase demonstrates high quality:
- Zero unimplemented!/todo! macros in production code
- Zero panic!() calls in production code
- All 19 unsafe blocks have SAFETY comments
- Feature flags cleanly gate optional dependencies
- Pure dispatch pattern (Action -> ActionEffect) is well-structured
- 1130 lib + 253 integration + 4 doc = 1387 tests, zero failures
- Chrome WASM UI fully accessible with ARIA roles and labels
- Design language consistent across chrome and landing page

Known architectural debt:
- Servo integration is skeleton only (documented, tracked)
- wry !Send + !Sync requires bridging (documented, ADR-009)
- main.rs is 730 lines (well under 2000 target after event_handlers.rs extraction)

## Timeline Estimate

| Release | Target Date | Weeks from Now |
|---------|------------|----------------|
| v0.21 | 2026-06-12 | 1 |
| v0.22 | 2026-07-03 | 4 |
| v0.23 | 2026-07-31 | 8 |
| v0.24 | 2026-08-28 | 12 |
| v0.25 | 2026-09-25 | 16 |
| v1.0.0-rc.1 | 2026-10-16 | 19 |
| v1.0.0 | 2026-10-30 | 21 |
| v1.1 | 2026-12 | 26 |
| v2.0 | 2027-Q2 | 38+ |

*Estimates assume single full-time developer. Parallel work on independent tracks can compress timeline.*
