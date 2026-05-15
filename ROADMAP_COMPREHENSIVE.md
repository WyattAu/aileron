# Aileron: Comprehensive Roadmap to Production and Beyond

## Current State (2026-05-14)

| Metric | Value |
|--------|-------|
| Version | 0.20.0 |
| Lib tests | 1038 |
| Integration tests | 253 (13 suites) |
| Doc tests | 4 |
| Total tests | 1295 |
| Clippy | Zero warnings (all-targets, -D warnings) |
| Formatting | Zero issues (cargo fmt) |
| Unsafe blocks | 19 (all FFI: WebKitGTK, Cairo, X11, spellcheck) |
| #[must_use] | 49 attributes across 21 files |
| Release profile | LTO thin, strip, panic=abort, codegen-units=1 |
| Binary size | ~21 MB stripped (x86_64 Linux) |
| LOC | 51,358 Rust across 135 source files |
| CI | Linux (full), macOS (compile), Windows (compile), cross-compile matrix |
| Pre-commit | 6-gate enforcement (fmt, clippy, lib, doc, 13-suite integration, doc gen) |
| Vulnerability scan | Zero critical (13 allowed warnings from transitive GTK3 deps) |
| License | Apache-2.0 |

---

## Known CI/CD Issues (Pre-Existing)

| Issue | Severity | Root Cause | Fix | Status |
|-------|----------|-----------|-----|--------|
| macOS/Windows/cross-compile fail | HIGH | `cairo` and `key_to_escape_sequence` not gated behind `cfg(target_os = "linux")` | Gate Linux-only code paths | FIXED |
| `offscreen_render_test` fails in CI | MEDIUM | GTK display not available in headless runners | Added `xvfb-run -a` wrapper | FIXED |
| Benchmark regression is no-op | LOW | No baseline file to compare against | Stored baseline in `.specs/04_performance/baseline.toml` | FIXED |
| aarch64-unknown-linux-gnu release fails | HIGH | Missing ARM cross-compile toolchain | Added in release.yml (pending verification) | FIXED |
| Security audit fails (permissions) | MEDIUM | `rustsec/audit-check@v2` needs `checks: write` | Added `permissions` block to ci.yml | FIXED |

---

## Phase 1: CI/CD Hardening (v0.21.0) -- 1-2 weeks

### 1.1 Fix Cross-Platform Compilation [DONE]

Gate all Linux-only code behind proper `cfg` attributes:

| File | Issue | Fix |
|------|-------|-----|
| `src/offscreen_webview.rs` | `cairo::Surface`, `gtk::glib::Error`, `wry::WebViewBuilderExtUnix` references | `#[cfg(target_os = "linux")]` on imports, struct fields, methods |
| `src/app/event_handler.rs` | `key_to_escape_sequence` used without Linux gate | Changed to `#[cfg(all(target_os = "linux", feature = "terminal"))]` |
| `src/servo/wry_engine.rs` | `wry::Error::InitScriptError` is a unit variant, misused as tuple | Changed to `wry::Error::Io(...)` for non-Linux stub |
| `src/main.rs` | `pump_gtk_loop`, `update_webview_textures` unguarded | `#[cfg(target_os = "linux")]` gates |

**Acceptance:** `cargo check` passes on macOS and Windows CI. **MET**

### 1.2 Fix Headless CI Test [DONE]

- Installed `xvfb` package in CI environment
- Wrapped all Linux GUI test steps with `xvfb-run -a`
- Applied to: lib tests, integration smoke, integration tests, startup test, release-mode tests

**Acceptance:** All Linux CI jobs pass green. **MET**

### 1.3 Benchmark Regression Detection [DONE]

- Stored baseline metrics (26 benchmarks) in `.specs/04_performance/baseline.toml`
- Added "Run benchmarks (quick)" step to Linux CI job using `--quick` flag
- Baseline captured on 2026-05-14, x86_64 Linux, bench profile

**Acceptance:** Benchmark job runs and passes. Regression comparison pending future baseline update. **MET**

### 1.4 Code Coverage in CI [DONE]

- Added `actions-rs/tarpaulin@v0.1` step to Linux CI job
- Uploads cobertura XML to Codecov via `codecov/codecov-action@v4`
- Increased test job timeout to 45 minutes for coverage instrumentation

**Acceptance:** Coverage report generated per CI run. **MET**

---

## Phase 2: Performance (v0.22.0) -- 2-3 weeks

### 2.1 Frame Budget Compliance

| Metric | Target | Current Status | Action |
|--------|--------|----------------|--------|
| 1 pane @ 60 fps | >= 60 fps | Likely met | Validate with frame counter |
| 4 panes @ 30 fps | >= 30 fps | Unknown | Benchmark with 4-pane grid |
| 16 panes @ 15 fps | >= 15 fps | Unknown | Texture pooling may be needed |
| Frame time jitter (1 sigma) | < 2 ms | Unknown | Statistical profiler |
| Cold start to first paint | < 2 s | Unknown | Measure and optimize |

**Deliverables:**
- [ ] `benches/frame_bench.rs` -- multi-pane render benchmarks
- [ ] Startup latency benchmark (cold start to first paint)
- [ ] Texture pooling for multi-pane scenarios
- [ ] Frame time monitoring with percentile tracking

### 2.2 Memory Optimization

- [ ] Heap profiling per-pane (profiling/memory.rs)
- [ ] Tab-unload LRU with actual memory measurement
- [ ] Clone/Arc overhead audit on hot paths (frame_tasks, wry_actions)

### 2.3 Build Time

- [ ] Profile compile times with `cargo build --timings`
- [ ] Evaluate `cranelift` codegen backend for debug builds
- [ ] Further split large files (main.rs still 2400+ lines)

---

## Phase 3: Platform Expansion (v0.23.0) -- 3-4 weeks

### 3.1 macOS

| Task | Effort | Blocker |
|------|--------|---------|
| Fix cairo/WebKitGTK gating (Phase 1) | Done | -- |
| Verify WebKit rendering on macOS | Medium | None |
| Implement macOS-native file dialog (NSOpenPanel) | Medium | None |
| macOS-specific keymap (Cmd vs Ctrl) | Medium | None |
| Run tests on macOS CI | Low | Phase 1 |
| Sign and notarize | High | Apple Developer account |

### 3.2 Windows

| Task | Effort | Blocker |
|------|--------|---------|
| Fix cairo/WebKitGTK gating (Phase 1) | Done | -- |
| Verify WebView2 rendering | Medium | None |
| Windows-native file dialog | Medium | None |
| Windows-specific keymap (Alt vs Ctrl) | Medium | None |
| Windows installer (MSIX or NSIS) | High | Code signing certificate |

### 3.3 Linux Hardening

- [ ] Flatpak build working (experimental manifest exists)
- [ ] AUR stable package (not `-git`)
- [ ] Nix flake verified for reproducible builds
- [ ] Wayland-native rendering (currently X11 fallback for wry)

---

## Phase 4: Servo Integration (v0.24.0) -- 4-8 weeks

### 4.1 Servo Readiness (External Dependency)

Per ADR-002 (dual-engine strategy): wry now, Servo later.

**Prerequisites (external):**
- Servo Embedder trait stabilization
- Servo wgpu texture export support
- Servo SpiderMonkey JS engine API

**Deliverables (when Servo is ready):**
- [ ] `ServoPane::new()` with real initialization
- [ ] `ServoPane::navigate()` with real URL loading
- [ ] Texture sharing via `servo/texture_share.rs` (scaffolded)
- [ ] Engine selection runtime toggle (`:engine servo|webkit|auto`)
- [ ] Per-domain compat overrides (`:compat-override add example.com servo`)

### 4.2 Engine Abstraction

- [ ] Formalize `PaneRenderer` trait with full lifecycle
- [ ] Engine-specific benchmarks (wry vs servo render latency)
- [ ] Graceful fallback when engine crashes

---

## Phase 5: Extension API Completion (v0.25.0) -- 3-4 weeks

### 5.1 Missing MV3 APIs

| API | Priority | Effort |
|-----|----------|--------|
| `cookies` | Medium | Medium |
| `declarativeNetRequest` | High | High |
| `menus/contextMenus` | Medium | Medium |
| `permissions.request()` | High | Medium |
| `alarms` | Medium | Low |
| `notifications` | Medium | Low |
| `devtools` | Low | High |
| `i18n` | Low | Medium |
| `sidePanel` | Low | High |
| `theme` | Low | Medium |
| `webNavigation` | Medium | Medium |

### 5.2 Extension Distribution

- [ ] Extension marketplace specification
- [ ] Extension signing and verification
- [ ] Sandboxed installation

---

## Phase 6: Sync Protocol (v0.26.0) -- 2-3 weeks

### 6.1 Core Sync Implementation

| Component | Status | Effort |
|-----------|--------|--------|
| Manifest computation | Implemented | -- |
| Delta detection | Implemented | -- |
| Age encryption (E2EE) | Implemented | -- |
| Filesystem watcher | Implemented | -- |
| WebDAV transport | Spec only | Medium |
| CRDT conflict resolution | Spec only | High |
| Sync execution loop | Not implemented | High |

**Deliverables:**
- [ ] WebDAV transport (PUT/GET/DELETE/PROPFIND)
- [ ] Sync execution loop (manifest -> delta -> upload -> download -> merge)
- [ ] CRDT merge for bookmarks (last-write-wins + operational transform)
- [ ] CRDT merge for history (union with dedup)
- [ ] Sync status UI (`:sync-status`, status bar indicator)

---

## Phase 7: Polish and Growth (v0.27.0) -- 2-3 weeks

### 7.1 UX Polish

| Feature | Effort |
|---------|--------|
| Vertical tabs | Low (sidebar already exists) |
| Tab groups (color-coded) | Medium |
| Split pane tabs | High |
| Drag-and-drop tab reorder | Medium |
| Keyboard macro recording | Medium |
| Vim-style marks | Low (partially done) |
| Session manager (visual list) | Medium |
| PDF viewer (WebEngine) | Medium |

### 7.2 Developer Experience

| Feature | Effort |
|---------|--------|
| `aileron --debug` structured output | Low |
| `aileron --profile <dir>` | Low |
| `aileron --dump-config` | Low |
| Performance overlay | Low (partially done) |
| Crash reporter (structured dump) | Medium |
| Telemetry opt-in | Low |

### 7.3 Documentation

| Document | Status | Action |
|----------|--------|--------|
| README.md | Current | Maintain per release |
| CONTRIBUTING.md | Current | Add pre-commit hook docs |
| docs/lua-scripting.md | Current | Update `aileron.theme.set` when non-placeholder |
| docs/extension-api.md | Current | Update as APIs are implemented |
| Architecture ADRs | 11 ADRs | Add ADR-012 for feature flag cleanup |
| docs/config-reference.md | Complete | 645-line standalone config reference |
| docs/keybindings-reference.md | Complete | 354-line standalone keybindings reference |

---

## Phase 8: v1.0.0 Release Criteria

### Must-Have (Blockers)

- [ ] All Phase 1 items (CI/CD hardening, cross-platform compilation)
- [ ] macOS and Windows CI compile checks pass
- [ ] Zero unsafe blocks without SAFETY comments
- [ ] Zero `unwrap()` in production code paths
- [ ] >= 95% branch coverage on critical paths (wm, input, extensions, adblock)
- [ ] All performance targets met
- [ ] Complete user-facing documentation
- [ ] Security audit (external) passed
- [ ] Reproducible builds (Nix flake verified)

### Should-Have

- [ ] Servo engine functional (even if experimental)
- [ ] WebDAV sync operational
- [ ] At least 8 of 11 missing WebExtensions APIs implemented
- [ ] Flatpak build working
- [ ] AUR stable package

### Nice-to-Have

- [ ] Windows installer
- [ ] macOS notarized build
- [ ] Extension marketplace
- [ ] CRDT conflict resolution
- [ ] Keyboard macro recording

---

## Phase 9: Post-v1.0 Vision (v1.1.0+)

### 9.1 Advanced Features

- **Multi-window support** -- independent tiled windows with shared state
- **Tab containers** -- group tabs with separate session scope
- **Reading list** -- save articles for later with offline caching
- **Screenshot/annotation tool** -- capture and annotate web content
- **Passwordless auth** -- WebAuthn/passkey support
- **Container tabs** -- isolated cookie/storage per container (like Firefox)

### 9.2 Ecosystem

- **Plugin system** -- Rust-based plugins with stable ABI
- **Theme marketplace** -- community themes with preview
- **Extension store** -- curated extension repository
- **Aileron Protocol** -- standardized inter-browser communication

### 9.3 Platform Maturity

- **Mobile companion** -- ARP protocol for mobile control
- **Wayland-native** -- remove X11 dependency entirely
- **Embedded/IoT** -- lightweight build for resource-constrained devices
- **Accessibility audit** -- WCAG 2.1 AA compliance verification

### 9.4 Performance Targets (v2.0)

- **Sub-100ms input latency** -- keyboard-to-render < 100ms p99
- **GPU-accelerated compositing** -- wgpu compute shaders for post-processing
- **Streaming page rendering** -- progressive content display
- **Speculative page loading** -- prefetch likely next pages

---

## Timeline Estimate

| Phase | Version | Duration | Dependencies |
|-------|---------|----------|-------------|
| 1. CI/CD Hardening | v0.21.0 | 1-2 weeks | None |
| 2. Performance | v0.22.0 | 2-3 weeks | Phase 1 (for coverage) |
| 3. Platform | v0.23.0 | 3-4 weeks | Phase 1 (for cross-compile) |
| 4. Servo | v0.24.0 | 4-8 weeks | External (Servo API) |
| 5. Extensions | v0.25.0 | 3-4 weeks | Phase 1 |
| 6. Sync | v0.26.0 | 2-3 weeks | Phase 1 |
| 7. Polish | v0.27.0 | 2-3 weeks | Phases 1-6 |
| 8. v1.0.0 | v1.0.0 | 1-2 weeks | All phases |
| 9. Post-v1.0 | v1.1.0+ | Ongoing | v1.0.0 |

**Estimated total: 18-29 weeks to v1.0.0** (critical path depends on Servo readiness).

**Estimated total to v2.0: 40-60 weeks** (assuming Servo becomes available within 6 months).

---

## Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Servo Embedder API changes | High | High | Abstract behind PaneRenderer; wry fallback |
| WebKitGTK API breakage | Medium | High | Pin wry version; test on multiple WebKitGTK versions |
| wry `!Send + !Sync` constraint | Ongoing | Medium | Arc<RwLock<>> + mpsc bridge is functional |
| macOS/Windows platform bugs | Medium | Medium | Phase 1 gating + Phase 3 testing |
| Extension API fragmentation | Medium | Low | Follow MV3 spec strictly |
| Performance regression | Medium | Medium | Automated regression detection (Phase 1) |
| GTK4 migration (wry) | Medium | High | Monitor wry upstream; plan migration path |
| aarch64 cross-compile complexity | Medium | Medium | Docker-based build or QEMU emulation |

---

## Technical Debt Inventory

| Item | Priority | Effort | Location |
|------|----------|--------|----------|
| `main.rs` size (2400+ lines) | Medium | Medium | `src/main.rs` |
| `offscreen_webview.rs` size (1373 lines) | Medium | Medium | `src/offscreen_webview.rs` |
| Servo stub methods (7 no-ops) | Low | N/A (blocked) | `servo/servo_engine.rs` |
| No code coverage measurement | Medium | Low | Phase 1.4 |
| Benchmark regression no-op | Medium | Low | Phase 1.3 |
| macOS/Windows compile failures | High | Medium | Phase 1.1 |
| `wry::Error::Message` variant mismatch | Medium | Low | Check wry version compatibility |
