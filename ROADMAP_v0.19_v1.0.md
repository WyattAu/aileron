# Aileron Path Forward: v0.19.0 to v1.0.0

## Current State (2026-05-12)

| Metric | Value |
|--------|-------|
| Version | 0.18.0 (shipped) |
| Lib tests | 1038 |
| Integration tests | 217 |
| Doc tests | 4 |
| Total tests | 1259 |
| Clippy | Zero warnings (all-targets, -D warnings) |
| Formatting | Zero issues (cargo fmt) |
| Vulnerabilities | Zero critical (13 unmaintained warnings from transitive GTK3 deps via wry) |
| Unsafe blocks | 19 (all FFI: WebKitGTK, Cairo, X11, spellcheck) |
| Release profile | LTO thin, strip, panic=abort, codegen-units=1 |
| Binary size | ~21 MB stripped (x86_64 Linux) |
| LOC | ~50,800 Rust across 135 source files |
| Pre-commit hook | 6-gate enforcement (fmt, clippy, lib, doc, integration, doc gen) |

---

## Quality Audit Results

### What Passed

- **Clippy:** Zero warnings across all targets with `-D warnings`
- **Formatting:** Zero deviations from rustfmt
- **Tests:** 1259 tests all passing (1038 lib + 217 integration + 4 doc)
- **Pre-commit hook:** All 6 quality gates pass deterministically
- **Documentation:** Zero emojis, technically accurate, API signatures match source
- **Determinism:** BTreeMap for sync manifest serialization, cached pane lists
- **Error handling:** All silent swallows converted to `tracing::warn`
- **`#[must_use]`:** 40 attributes across 20 files
- **`unwrap()` audit:** Zero risky unwrap() in production code paths
- **Concurrency:** 18 PASS, 12 WARN, 0 FAIL, 0 deadlocks

### What Was Fixed

- **Flaky i18n test race:** `test_detect_locale_japanese` failed intermittently due to concurrent `std::env::set_var("LANG", ...)` across parallel test threads. Fixed by extracting a pure `parse_lang_env()` function and routing all tests through the thread-safe `LOCALE_OVERRIDE` (RwLock) instead of the process-wide `LANG` env var.

### Known Stubs (Acceptable, Documented)

| Location | Description | Status |
|----------|-------------|--------|
| `servo/servo_engine.rs` | Servo engine skeleton (7 no-op methods) | Blocked on Servo Embedder API stabilization |
| `platform/windows.rs` | Windows file dialog stub | Compile-only, functional |
| `platform/macos.rs` | macOS file dialog stub | Compile-only, functional |
| `extensions/impls/scripting.rs` | `file` injection returns Unsupported | Documented in extension-api.md |
| `mcp/tools.rs` | `create_default_stub_tools` | Test helper, not production |

### Audit Warnings (Informational, Non-Blocking)

| Category | Count | Notes |
|----------|-------|-------|
| `#[allow(dead_code)]` | 8 | Protocol fields and WebExtension type stubs (intentionally forward-declared) |
| Unmaintained crates (cargo audit) | 13 | All transitive from GTK3 via wry; not actionable without wry migration |
| `unsafe` blocks | ~50 | All FFI; ~24 in test code, ~15 WebKitGTK/Cairo, ~3 X11, ~5 env vars |
| `unwrap()` in tests | Common | Acceptable; production code has zero risky unwrap() |

---

## Phase 1: Hardening and Correctness (v0.19.0)

### 1.1 Remaining Unsafe Block Reduction

| Category | Count | Action |
|----------|-------|--------|
| Test-only `set_var` | Eliminated | All test env var mutation removed |
| Production `set_var` (proxy command) | 0 | Previously fixed (moved before thread spawn) |
| WebKitGTK / Cairo FFI | ~15 | Required, justified. Add missing SAFETY comments. |
| X11 error handler | ~3 | Previously consolidated into `platform/x11.rs`. |
| Spellcheck FFI | ~3 | Previously deduplicated into shared helper. |

**Deliverables:**
- [x] Consolidate duplicate X11 error handler
- [x] Refactor proxy `set_var` to avoid post-spawn env mutation
- [x] Add SAFETY comments to all FFI unsafe blocks
- [x] Deduplicate spellcheck FFI
- [x] Eliminate test env var races (pure `parse_lang_env`)
- [ ] Audit remaining ~15 WebKitGTK/Cairo FFI for SAFETY comment completeness

### 1.2 Test Coverage Expansion

Current coverage by module:

| Module | Lib Tests | Integration Tests | Status |
|--------|-----------|-------------------|--------|
| `wm/` (BSP tree, pane, rect) | 35 | 7 (smoke) | Good |
| `net/adblock` | 48 | 14 (adblock_integration) | Good |
| `extensions/` | 82 | 24 (extension_integration) | Good |
| `mcp/` | 24 | 14 (mcp_integration) | Good |
| `sync/` | 12 | 12 (sync_integration) | Good |
| `db/` | 38 | 9 (db_integration) | Good |
| `input/` (keymap, keybindings, mode, router) | 40 | 0 | Needs integration |
| `lua/` (api, sandbox) | 25 | 0 | Needs integration |
| `i18n/` | 46 | 0 | Good (pure tests) |
| `terminal/` | 15 | 0 | Needs integration (PTY) |
| `downloads/` | 8 | 0 | Needs integration |
| `platform/` | 30 | 0 | Platform-specific |
| `profiling/` | 15 | 0 | Good |
| `scripts/` | 8 | 0 | Needs integration |
| `passwords/` | 12 | 0 | Needs integration (keyring) |
| `frame_tasks/` | 18 | 0 | Needs integration |
| `servo/` | 25 | 0 | Blocked on Servo |

**Priority gaps:**
- `input/` integration: Key event routing end-to-end
- `lua/` integration: Full init.lua execution with real API
- `terminal/` integration: PTY lifecycle with real shell
- `passwords/` integration: Keyring round-trip (if available in CI)

### 1.3 Performance Baseline Establishment

Benchmark results (criterion --quick, from VERSION.md):

| Benchmark | Time | Category |
|-----------|------|----------|
| BSP tree creation | 137 ns | Window management |
| Vertical pane split | 406 ns | Window management |
| Horizontal pane split | 331 ns | Window management |
| 4-pane grid navigation | 61 ns | Window management |
| Fuzzy search (100 items) | 42-132 us | Command palette |
| Action dispatch (10 actions) | 524 ns | Core dispatch |
| EasyList filter parse | 1.17 us | Ad blocking |
| URL pattern match (regex) | 8.36 us | Per-site settings |
| Content script match (100) | 19 us | Extensions |
| Domain block check | 53 ns | Ad blocking |

**Missing baselines:**
- Cold start to first paint (target: < 2s)
- Frame time at 1/4/16 panes
- Memory per pane (target: < 50 MB)
- Navigation latency (URL input to first content paint)

---

## Phase 2: Performance (v0.20.0)

### 2.1 Frame Budget Compliance

| Metric | Target | Current Status | Action |
|--------|--------|----------------|--------|
| 1 pane @ 60 fps | >= 60 fps | Likely met | Validate with frame counter |
| 4 panes @ 30 fps | >= 30 fps | Unknown | Benchmark with 4-pane grid |
| 16 panes @ 15 fps | >= 15 fps | Unknown | May need texture pooling |
| Frame time jitter (1 sigma) | < 2 ms | Unknown | Add statistical profiler |
| Cold start to first paint | < 2 s | Unknown | Measure and optimize |

### 2.2 Memory Optimization

- Heap profiling per pane (`profiling/memory.rs` already exists)
- LRU tab unload with actual memory measurement
- Clone/Arc overhead audit on hot paths

### 2.3 Build Time

- `cargo build --timings` profiling
- Cranelift codegen backend evaluation for debug builds

---

## Phase 3: Platform Expansion (v0.21.0)

### macOS (compile-only to daily-driver)

| Task | Effort | Blocker |
|------|--------|---------|
| Run tests on macOS CI | Low | None |
| Verify WebKit rendering | Medium | None |
| Native file dialog (NSOpenPanel) | Medium | None |
| macOS keymap (Cmd vs Ctrl) | Medium | None |
| Sign and notarize | High | Apple Developer account |

### Windows (compile-only to daily-driver)

| Task | Effort | Blocker |
|------|--------|---------|
| Run tests on Windows CI | Low | None |
| Verify WebView2 rendering | Medium | None |
| Windows file dialog | Medium | None |
| Windows installer (MSIX/NSIS) | High | Code signing |

---

## Phase 4: Servo Integration (v0.22.0)

External blocker: Servo Embedder API stabilization. When ready:

- Implement real `ServoPane` lifecycle (new, navigate, resize, destroy)
- Texture sharing via `servo/texture_share.rs` (already scaffolded)
- Engine selection runtime toggle (`:engine servo|webkit|auto`)
- Per-domain compat overrides
- Graceful fallback on Servo crash

---

## Phase 5: Extension API Completion (v0.23.0)

Missing from MV3 spec (priority order):

| API | Priority | Effort |
|-----|----------|--------|
| `cookies` | High | Medium |
| `declarativeNetRequest` | High | High |
| `permissions.request()` | High | Medium |
| `alarms` | Medium | Low |
| `menus/contextMenus` | Medium | Medium |
| `notifications` | Medium | Low |
| `i18n` | Low | Medium |
| `webNavigation` | Medium | Medium |
| `sidePanel` | Low | High |
| `theme` | Low | Medium |
| `devtools` | Low | High |

---

## Phase 6: Sync Protocol (v0.24.0)

| Component | Status |
|-----------|--------|
| Manifest computation (content-addressed) | Implemented |
| Delta detection | Implemented |
| Age encryption (E2EE) | Implemented |
| Filesystem watcher | Implemented |
| WebDAV transport | Spec only |
| CRDT conflict resolution | Spec only |
| Sync execution loop | Not implemented |
| Sync status UI | Not implemented |

---

## Phase 7: Polish and v1.0.0 (v0.25.0)

### UX Polish

- Vertical tabs (already have sidebar foundation)
- Tab groups (color-coded)
- Keyboard macro recording
- Session manager with visual preview
- Drag-and-drop tab reorder

### Developer Experience

- `aileron --debug` structured output
- `aileron --profile <dir>`
- `aileron --dump-config`
- Crash reporter with structured stack trace
- Performance overlay (partially done)

### v1.0.0 Release Criteria

**Must-Have (blockers):**
- All Phase 1 items complete
- macOS and Windows run tests in CI
- Zero unsafe blocks without SAFETY comments
- Zero unwrap() in production code paths
- >= 95% branch coverage on critical paths
- All performance targets met
- Complete user-facing documentation

**Should-Have:**
- Servo engine functional (even if experimental)
- WebDAV sync operational
- 8+ of 11 missing WebExtensions APIs
- Flatpak build working
- AUR package stable (not -git)

---

## Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Servo Embedder API changes | High | High | PaneRenderer trait abstraction; wry as fallback |
| WebKitGTK API breakage | Medium | High | Pin wry version; test on multiple WebKitGTK versions |
| wry `!Send + !Sync` | Ongoing | Medium | Arc<RwLock<>> + mpsc bridge (functional) |
| macOS/Windows platform bugs | Medium | Medium | Extend CI to run tests, not just compile |
| Extension API fragmentation | Medium | Low | Follow MV3 spec strictly |
| Performance regression | Medium | Medium | Automated regression detection in CI |
| GTK3 deprecation (cargo audit) | High | Low | Blocked on wry GTK4 migration; no alternative |

---

## Technical Debt Inventory

| Item | Priority | Effort | Location |
|------|----------|--------|----------|
| `main.rs` size (2400+ lines) | Medium | Medium | Already partially split |
| `offscreen_webview.rs` size (1150+ lines) | Low | Low | Already split |
| Servo stub methods (7 no-ops) | Low | N/A | Blocked externally |
| No code coverage measurement in CI | Medium | Low | Add `cargo-llvm-cov` to CI |
| `lua-scripting.md` says `aileron.theme.set` is placeholder | Low | Low | Update when implemented |
| `extensions.reload` stubbed in Lua API | Low | Low | Document or implement |

---

## Timeline Estimate

| Phase | Version | Duration | Dependencies |
|-------|---------|----------|-------------|
| 1. Hardening | v0.19.0 | 1-2 weeks | None |
| 2. Performance | v0.20.0 | 2-3 weeks | Phase 1 |
| 3. Platform | v0.21.0 | 3-4 weeks | Phase 1 (CI tests) |
| 4. Servo | v0.22.0 | 4-8 weeks | External (Servo API) |
| 5. Extensions | v0.23.0 | 3-4 weeks | Phase 1 |
| 6. Sync | v0.24.0 | 2-3 weeks | Phase 1 |
| 7. Polish + v1.0.0 | v0.25.0 | 2-3 weeks | Phases 1-6 |

**Estimated total:** 17-27 weeks to v1.0.0 (Servo dependency is the primary variable).

---

## Immediate Next Steps (v0.19.0)

1. Add SAFETY comments to remaining WebKitGTK/Cairo FFI blocks
2. Add `input/` integration tests (key event routing end-to-end)
3. Add cold-start-to-first-paint benchmark
4. Add `cargo-llvm-cov` to CI for coverage measurement
5. Evaluate `cranelift` codegen backend for debug build times
6. Add macOS test execution to CI (not just compile check)
