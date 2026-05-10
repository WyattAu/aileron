# Aileron Roadmap v0.19.0 -- v1.0.0

## Current State (2026-05-10)

- **Version:** 0.18.0 (shipped)
- **Tests:** 998 lib, 26 integration, 13 startup, 4 doc (1041 total)
- **Code:** 49,591 lines Rust across 132 source files
- **Quality:** Zero clippy warnings, zero fmt issues, pre-commit hook enforced
- **Unsafe:** ~50 blocks (all FFI: WebKitGTK, Cairo, X11, env vars, spellcheck)
- **CI:** Linux (full), macOS (compile), Windows (compile), cross-compile matrix
- **Platform:** Linux primary (x86_64), macOS/Windows compile-only

---

## Phase 1: Hardening and Correctness (v0.19.0)

### 1.1 Unsafe Block Reduction

The ~50 unsafe blocks fall into three categories:

| Category | Count | Action |
|----------|-------|--------|
| `std::env::set_var` / `remove_var` | ~30 (24 in tests) | Test blocks are acceptable. Production blocks in `commands.rs` (proxy command) run after thread spawn -- refactor to use thread-local or move before thread creation. |
| WebKitGTK / Cairo FFI | ~15 | Required, justified. Add SAFETY documentation comments where missing. |
| X11 error handler | ~3 | Duplicate definitions in `platform/x11.rs` and `main.rs` -- consolidate to single module. |

**Deliverables:**
- [ ] Consolidate duplicate X11 error handler into `platform/x11.rs`, import in `main.rs`
- [ ] Refactor `commands.rs` proxy `set_var` to avoid post-spawn env mutation
- [ ] Add `// SAFETY:` comments to all FFI unsafe blocks missing them
- [ ] Consolidate duplicate spellcheck FFI between `offscreen_webview.rs` and `wry_engine.rs` into shared helper

### 1.2 Test Coverage Expansion

Current gaps in integration test coverage:

| Module | Lib Tests | Integration Tests | Gap |
|--------|-----------|-------------------|-----|
| db/ | 38 | 0 | No integration tests for SQLite operations |
| net/adblock | 48 | 0 | No end-to-end filter + block test |
| extensions/ | 82 | 0 | No extension load + content script injection test |
| mcp/ | 24 | 0 | No end-to-end MCP tool call test |
| sync/ | 12 | 0 | No end-to-end sync manifest roundtrip test |
| downloads/ | 8 | 0 | No download manager integration test |
| terminal/ | 12 | 0 | No PTY lifecycle integration test |

**Deliverables:**
- [ ] `tests/db_integration.rs` -- workspace save/load, bookmark CRUD, history dedup
- [ ] `tests/adblock_integration.rs` -- load EasyList, verify block/allow on known domains
- [ ] `tests/extension_integration.rs` -- load manifest, fire content script, verify JS injection
- [ ] `tests/mcp_integration.rs` -- JSON-RPC initialize, tools/list, tools/call roundtrip
- [ ] `tests/sync_integration.rs` -- manifest computation, delta detection, age encrypt/decrypt

### 1.3 Code Quality Hardening

**Deliverables:**
- [ ] Add `#[must_use]` to all fallible function returns (Result, Option)
- [ ] Replace remaining `unwrap()` in non-test code with `?` or explicit error handling
- [ ] Audit all `expect()` messages for actionability (no "this should never happen")
- [ ] Add `cargo doc` generation to CI and fix all `#[warn(missing_docs)]` items

---

## Phase 2: Performance (v0.20.0)

### 2.1 Frame Budget Compliance

Per `.specs/04_performance/performance_requirements.md`:

| Metric | Target | Current Status | Action |
|--------|--------|----------------|--------|
| 1 pane @ 60 fps | >= 60 fps | Likely met | Validate with frame counter |
| 4 panes @ 30 fps | >= 30 fps | Unknown | Benchmark with 4-pane grid |
| 16 panes @ 15 fps | >= 15 fps | Unknown | Benchmark; may need texture pooling |
| Frame time jitter (1 sigma) | < 2 ms | Unknown | Add statistical frame profiler |
| Cold start to first paint | < 2 s | Unknown | Measure and optimize |

**Deliverables:**
- [ ] Add automated frame-time measurement to `profiling/` module
- [ ] Create `benches/frame_bench.rs` -- multi-pane render benchmarks
- [ ] Startup latency benchmark (cold start to first paint)
- [ ] Texture pooling for multi-pane scenarios (avoid per-frame allocation)

### 2.2 Memory Optimization

**Deliverables:**
- [ ] Add heap profiling to `profiling/memory.rs` (track per-pane allocation)
- [ ] Implement tab-unload LRU with actual memory measurement (not just heuristic)
- [ ] Audit and reduce Clone/Arc overhead on hot paths (frame_tasks, wry_actions)

### 2.3 Build Time Reduction

**Deliverables:**
- [ ] Profile compile times with `cargo build --timings`
- [ ] Evaluate `cranelift` codegen backend for debug builds
- [ ] Split `offscreen_webview.rs` (2500+ lines) and `main.rs` (2400+ lines) into smaller modules

---

## Phase 3: Platform Expansion (v0.21.0)

### 3.1 macOS

Current status: compile-only. Steps to daily-driver:

| Task | Effort | Blocker |
|------|--------|---------|
| Run tests on macOS CI | Low | None |
| Verify WebKit rendering on macOS | Medium | None |
| Implement macOS-native file dialog (NSOpenPanel) | Medium | None |
| macOS-specific keymap (Cmd vs Ctrl) | Medium | None |
| Sign and notarize for distribution | High | Apple Developer account |

### 3.2 Windows

Current status: compile-only. Steps:

| Task | Effort | Blocker |
|------|--------|---------|
| Run tests on Windows CI | Low | None |
| Verify WebView2 rendering | Medium | None |
| Windows-native file dialog | Medium | None |
| Windows-specific keymap (Alt vs Ctrl) | Medium | None |
| Windows installer (MSIX or NSIS) | High | Code signing certificate |

### 3.3 Cross-Platform CI

**Deliverables:**
- [ ] Add macOS test execution to CI (not just compile check)
- [ ] Add Windows test execution to CI (not just compile check)
- [ ] Cross-platform integration test matrix

---

## Phase 4: Servo Integration (v0.22.0)

### 4.1 Servo Readiness

Per `ADR-002` (dual-engine strategy): wry now, Servo later.

Current Servo status: skeleton stub (`servo/servo_engine.rs`) with 7 no-op methods.

**Prerequisites (external):**
- Servo's `Embedder` trait stabilization
- Servo's wgpu texture export support
- Servo's SpiderMonkey JS engine API

**Deliverables (when Servo is ready):**
- [ ] Implement `ServoPane::new()` with real Servo initialization
- [ ] Implement `ServoPane::navigate()` with real URL loading
- [ ] Implement texture sharing via `servo/texture_share.rs` (already scaffolded)
- [ ] Engine selection runtime toggle (`:engine servo|webkit|auto`)
- [ ] Per-domain compat overrides (`:compat-override add example.com servo`)

### 4.2 Engine Abstraction Hardening

**Deliverables:**
- [ ] Formalize `PaneRenderer` trait with full lifecycle (create, navigate, resize, destroy, execute_js, screenshot)
- [ ] Add engine-specific benchmarks (wry vs servo render latency)
- [ ] Graceful fallback when Servo crashes (auto-switch to wry, log error)

---

## Phase 5: Extension API Completion (v0.23.0)

### 5.1 WebExtensions API Surface

Currently implemented: 6 API traits (runtime, tabs, storage, scripting, webRequest, permissions).

Missing from MV3 spec:

| API | Priority | Effort |
|-----|----------|--------|
| `alarms` | Medium | Low |
| `cookies` | Medium | Medium |
| `declarativeNetRequest` | High | High |
| `devtools` | Low | High |
| `i18n` | Low | Medium |
| `menus/contextMenus` | Medium | Medium |
| `notifications` | Medium | Low |
| `permissions.request()` | High | Medium |
| `scripting.registerContentScripts` | Done | -- |
| `sidePanel` | Low | High |
| `theme` | Low | Medium |
| `webNavigation` | Medium | Medium |

### 5.2 Extension Distribution

**Deliverables:**
- [ ] Extension store / marketplace specification
- [ ] Extension signing and verification
- [ ] Sandboxed extension installation (per-extension process isolation not feasible with wry; use JS sandbox)

---

## Phase 6: Sync Protocol Implementation (v0.24.0)

### 6.1 Core Sync

Per `.specs/02_architecture/sync_protocol_design.md`:

| Component | Status | Effort |
|-----------|--------|--------|
| Manifest computation (content-addressed) | Implemented (sync::core) | -- |
| Delta detection | Implemented (sync::core) | -- |
| Age encryption (E2EE) | Implemented (sync::crypto) | -- |
| WebDAV transport | Spec only | Medium |
| Filesystem watcher | Implemented (sync::watcher) | -- |
| CRDT conflict resolution | Spec only | High |
| Actual sync execution | Not implemented | High |

**Deliverables:**
- [ ] Implement WebDAV transport layer (PUT/GET/DELETE/PROPFIND)
- [ ] Implement sync execution loop (manifest -> delta -> upload -> download -> merge)
- [ ] Implement CRDT merge for bookmarks (last-write-wins with operational transform)
- [ ] Implement CRDT merge for history (union with dedup)
- [ ] Add sync status UI (`:sync-status`, sync indicator in status bar)

---

## Phase 7: Polish and Growth (v0.25.0)

### 7.1 UX Polish

| Feature | Description | Effort |
|---------|-------------|--------|
| Vertical tabs | Tab bar on left/right side | Low (already have sidebar) |
| Tab groups | Color-coded tab groups | Medium |
| Split pane tabs | Multiple tabs per pane | High |
| Drag-and-drop tab reorder | Drag tabs between panes | Medium |
| Tab search | Fuzzy search across open tabs | Low (already have `:tabs`) |
| Keyboard macro recording | Record/replay key sequences | Medium |
| Vim-style marks | Set/jump marks across panes | Low (partially done) |
| Session manager | Visual session list with preview | Medium |

### 7.2 Developer Experience

| Feature | Description | Effort |
|---------|-------------|--------|
| `aileron --debug` | Structured debug output mode | Low |
| `aileron --profile <dir>` | Custom profile directory | Low |
| `aileron --dump-config` | Print resolved configuration | Low |
| Performance overlay | Real-time FPS, memory, frame time graph | Low (partially done) |
| Crash reporter | Structured crash dump with stack trace | Medium |
| Telemetry opt-in | Anonymous usage statistics | Low |

### 7.3 Documentation

| Document | Status | Action |
|----------|--------|--------|
| README.md | Current | Maintain with each release |
| CONTRIBUTING.md | Current | Add pre-commit hook documentation |
| docs/lua-scripting.md | Current | Add `aileron.theme.set` non-placeholder docs |
| docs/extension-api.md | Current | Update as new APIs are implemented |
| Architecture ADRs | 11 ADRs | Add ADR-012 for feature flag cleanup |
| Inline documentation | Partial | Add `#[warn(missing_docs)]` and fix all warnings |

---

## Phase 8: v1.0.0 Release Criteria

### Must-Have (Blockers)

- [ ] All Phase 1 items (hardening, correctness, test coverage)
- [ ] macOS runs tests in CI
- [ ] Windows runs tests in CI
- [ ] Zero unsafe blocks without SAFETY comments
- [ ] Zero `unwrap()` in production code paths
- [ ] >= 95% branch coverage on critical paths (wm, input, extensions, adblock)
- [ ] All performance targets met (per performance_requirements.md)
- [ ] Complete user-facing documentation (README, keybindings, config reference, scripting guide)

### Should-Have

- [ ] Servo engine functional (even if experimental)
- [ ] WebDAV sync operational
- [ ] At least 8 of 11 missing WebExtensions APIs implemented
- [ ] Flatpak build working
- [ ] AUR package stable (not `-git`)

### Nice-to-Have

- [ ] Windows installer
- [ ] macOS notarized build
- [ ] Extension marketplace
- [ ] CRDT conflict resolution for sync
- [ ] Keyboard macro recording

---

## Timeline Estimate

| Phase | Version | Duration | Dependencies |
|-------|---------|----------|-------------|
| 1. Hardening | v0.19.0 | 2-3 weeks | None |
| 2. Performance | v0.20.0 | 2-3 weeks | Phase 1 |
| 3. Platform | v0.21.0 | 3-4 weeks | Phase 1 (for CI tests) |
| 4. Servo | v0.22.0 | 4-8 weeks | External (Servo API) |
| 5. Extensions | v0.23.0 | 3-4 weeks | Phase 1 |
| 6. Sync | v0.24.0 | 2-3 weeks | Phase 1 |
| 7. Polish | v0.25.0 | 2-3 weeks | Phases 1-6 |
| 8. v1.0.0 | v1.0.0 | 1-2 weeks | All phases |

**Estimated total:** 19-30 weeks to v1.0.0 (depending on Servo readiness and platform testing).

---

## Technical Debt Inventory

| Item | Priority | Effort | Location |
|------|----------|--------|----------|
| Duplicate X11 error handler | Medium | Low | `platform/x11.rs` + `main.rs` |
| Duplicate spellcheck FFI | Medium | Low | `offscreen_webview.rs` + `wry_engine.rs` |
| `set_var` after thread spawn | High | Medium | `app/commands.rs:393-403` |
| `main.rs` size (2400+ lines) | Medium | Medium | `src/main.rs` |
| `offscreen_webview.rs` size (2500+ lines) | Medium | Medium | `src/offscreen_webview.rs` |
| Servo stub methods (7 no-ops) | Low | N/A (blocked) | `servo/servo_engine.rs` |
| Missing integration tests for 7 modules | High | High | `tests/` |
| No code coverage measurement in CI | Medium | Low | `.github/workflows/ci.yml` |
| VERSION.md stale LOC/binary size | Low | Low | `VERSION.md:103-106` |

---

## Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Servo Embedder API changes | High | High | Abstract behind PaneRenderer trait; wry as fallback |
| WebKitGTK API breakage | Medium | High | Pin wry version; test on multiple WebKitGTK versions |
| wry `!Send + !Sync` constraint | Ongoing | Medium | Current Arc<RwLock<>> + mpsc bridge is functional |
| macOS/Windows platform bugs | Medium | Medium | Extend CI to run tests, not just compile |
| Extension API fragmentation | Medium | Low | Follow MV3 spec strictly; skip deprecated MV2 features |
| Performance regression with feature additions | Medium | Medium | Automated regression detection in CI |
