# Aileron Production Roadmap: v0.20.0 to v1.0.0 and Beyond

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
| Release profile | LTO thin, strip, panic=abort, codegen-units=1 |
| Binary size | ~21 MB stripped (x86_64 Linux) |
| LOC | ~51,303 Rust across 135 source files |
| CI | Linux (full), macOS (compile), Windows (compile), cross-compile matrix |
| Pre-commit | 6-gate enforcement (fmt, clippy, lib, doc, 13-suite integration, doc gen) |
| Vulnerability scan | Zero critical (13 allowed warnings from transitive GTK3 deps) |

### Core Systems Status

| System | Status | Notes |
|--------|--------|-------|
| Tiling WM (BSP tree) | Complete | 29 unit tests, 6 integration tests |
| Modal input system | Complete | Normal/Insert/Command/Find modes |
| Command palette | Complete | Nucleo fuzzy search, extensible via Lua |
| Web rendering (wry/WebKitGTK) | Complete | Offscreen + popup window paths |
| Lua scripting | Complete | Sandboxed, 42 tests |
| MCP bridge | Complete | JSON-RPC over stdio, 32 tools |
| Ad blocking | Complete | EasyList parser, Aho-Corasick, 45 tests |
| Password manager | Complete | Bitwarden CLI + system keyring |
| Extension system | Complete (partial MV3) | 6 API traits, 82 tests |
| Sync protocol | Partial | Delta detection, E2EE (Age); transport: Local/SSH only |
| i18n | Complete | 9 locales, runtime switching |
| Terminal emulator | Complete | alacritty_terminal + portable_pty |
| Servo engine | Skeleton | 7 no-op methods; blocked on upstream API |
| macOS support | Compile-only | No integration tests |
| Windows support | Compile-only | No integration tests |

### Known Gaps (Audit 2026-05-12)

1. **Lua navigation API:** `aileron.navigate()` implemented (v0.18.1). Supports init.lua startup navigation and hook callbacks.
2. **Servo non-functional:** Engine selection lists `servo` but the implementation is a skeleton. Users must understand this is experimental.
3. **WebDAV sync not implemented:** README previously claimed "ready for implementation." Actual code: Local/SSH transport only.
4. **Background JS evaluation not implemented:** Extension background scripts are loaded but not executed in a JS runtime.
5. **`#[must_use]` coverage:** 48 attributes across 28 files. All public functions returning Result/Option now annotated (8 missing found and fixed in v0.18.1).
6. **Silent error swallowing:** ~11 silent swallows converted to tracing::warn in v0.18.0. ~15 benign channel-send failures remain (shutdown race, not critical).
7. **Integration test gaps:** All 13 suites now have integration coverage. Remaining gap: website visit test (requires display server).

---

## Phase 1: Hardening and Correctness (v0.19.0)

**Goal:** Eliminate unsafe code ambiguity, expand test coverage, harden error handling.

**Status:** Most items complete. 2 items remain.

### 1.1 Unsafe Block Hygiene

| Task | Status |
|------|--------|
| Consolidate duplicate X11 error handler | Done |
| Add `// SAFETY:` comments to all FFI unsafe blocks | Done |
| Refactor proxy set_var to avoid post-spawn env mutation | Done |
| Consolidate spellcheck FFI between offscreen_webview and wry_engine | Done |
| Audit remaining ~15 WebKitGTK/Cairo FFI for SAFETY comment completeness | Done (all 19 have SAFETY comments) |

### 1.2 Test Coverage Expansion

| Task | Status |
|------|--------|
| `tests/db_integration.rs` | Done (9 tests) |
| `tests/adblock_integration.rs` | Done (14 tests) |
| `tests/extension_integration.rs` | Done (24 tests) |
| `tests/mcp_integration.rs` | Done (14 tests) |
| `tests/sync_integration.rs` | Done (12 tests) |
| `tests/input_routing_integration.rs` | Done (34 tests) |
| `tests/lua_integration.rs` | Done (51 tests) |
| `tests/frame_tasks_integration.rs` | Done (20 tests) |
| Website visit test (real WebKitGTK rendering) | Pending (requires display server) |
| `tests/downloads_integration.rs` | Done (14 tests) |
| `tests/terminal_integration.rs` | Done (21 tests) |

### 1.3 Code Quality Hardening

| Task | Status |
|------|--------|
| Add `#[must_use]` to all public Result/Option returns | Done (48 across 28 files; 8 missing found and added in v0.18.1) |
| Replace remaining `unwrap()` in non-test code | Done (all remaining are compile-time constants or guarded) |
| Audit all `expect()` messages for actionability | Done |
| Add `cargo doc` generation to CI | Done |
| Audit and convert `let _ =` silent error swallows to `tracing::warn` | Partial (11 done in v0.18.0; ~15 remain) |
| Database migration error hardening | Done (converted to `expect()`) |

---

## Phase 2: Performance (v0.20.0)

**Goal:** Validate and harden frame budget, memory footprint, and startup latency. Establish regression detection.

### 2.1 Frame Budget Validation

| Metric | Target | Status |
|--------|--------|--------|
| 1 pane @ 60 fps | >= 60 fps | Requires runtime measurement (display server) |
| 4 panes @ 30 fps | >= 30 fps | Requires runtime measurement |
| 16 panes @ 15 fps | >= 15 fps | Requires runtime measurement |
| Frame time jitter (1 sigma) | < 2 ms | Profiler exists; needs runtime validation |
| Cold start to first paint | < 2 s | Requires runtime measurement |
| 95th percentile latency | < 33 ms | InputLatencyTracker exists; needs runtime validation |

### 2.2 Memory Optimization

- **Heap profiling:** NOT implemented. Only global RSS via /proc/self/status. Needs allocator integration (jemalloc/mimalloc). Per-pane attribution requires thread-local context or explicit accounting.
- **Tab-unload LRU:** FIXED. Automatic eviction now uses `find_lru_pane()` (proper LRU by focus timestamp) instead of `iter().find()`.
- **Hot-path allocation audit:** DONE. 22 findings documented. 3 HIGH: tab cache String clones/8MB frame buffer/panes Vec clone. 9 MEDIUM: ARP JSON, dispatch Vec, WryAction clone, theme clone, HashMap key alloc, HashSet per keypress. See CHANGELOG v0.19.0 for full list.
- **Texture pooling:** Not started. Frame capture buffer (render.rs:169) allocates 8MB per dirty pane per frame. Should pre-allocate and reuse.

### 2.3 Build Time & Binary Size

- **Compile time profiling:** DONE. Clean dev build: 7m 14s (6-core x86_64). Heaviest: wry/webkit2gtk, egui/wgpu, mlua (vendored Lua), alacritty_terminal.
- **Cranelift backend:** Not started.
- **Feature gate non-critical modules:** DONE. Existing features (mcp, arp, sync, passwords) now gate Cargo.toml dependencies. `--no-default-features` compiles clean. Future: `terminal` (15+ sites) and `lua` (10+ sites) identified for gating.
- **Dependency audit:** DONE. Removed unused `chrono/serde` feature. All other 15 dependency features verified as used.

### 2.4 Regression Detection

- **Automated benchmark in CI:** Benchmark verification step exists in CI. Baseline comparison not yet implemented (requires stored baseline infrastructure).
- **Memory regression:** Not started.
- **Startup latency tracking:** Not started.

---

## Phase 3: Platform Expansion (v0.21.0)

**Goal:** Full test execution on macOS and Windows. Daily-driver readiness on all platforms.

### 3.1 macOS

| Task | Effort | Status |
|------|--------|--------|
| Run tests on macOS CI | Medium | Not started |
| Verify WebKit rendering on macOS | Medium | Not started |
| Implement macOS-native file dialog (NSOpenPanel) | Medium | Not started |
| macOS-specific keymap (Cmd vs Ctrl) | Medium | Not started |
| Sign and notarize for distribution | High | Not started |
| macOS install guide | Low | Not started |

### 3.2 Windows

| Task | Effort | Status |
|------|--------|--------|
| Run tests on Windows CI | Medium | Not started |
| Verify WebView2 rendering | Medium | Not started |
| Windows-native file dialog | Medium | Not started |
| Windows-specific keymap (Alt vs Ctrl) | Medium | Not started |
| Windows installer (MSIX or NSIS) | High | Not started |
| Windows install guide | Low | Not started |

### 3.3 Cross-Platform CI Matrix

- **3-OS test execution:** Linux (full), macOS (test), Windows (test)
- **Cross-compile checks:** aarch64-apple-darwin, aarch64-unknown-linux-gnu
- **Integration test matrix:** Platform-specific tests for file dialogs, notifications, keyring

---

## Phase 4: Servo Integration (v0.22.0)

**Goal:** Functional dual-engine rendering. Servo as experimental option, wry as stable fallback.

**Prerequisites (external):**
- Servo's `Embedder` trait stabilization
- Servo's wgpu texture export support
- Servo's SpiderMonkey JS engine API

### 4.1 Servo Engine Implementation

| Task | Status |
|------|--------|
| Implement `ServoPane::new()` with real Servo initialization | Blocked |
| Implement `ServoPane::navigate()` with real URL loading | Blocked |
| Texture sharing via `servo/texture_share.rs` | Scaffolded |
| Engine selection runtime toggle | Scaffolded |
| Per-domain compat overrides | Scaffolded |
| Graceful Servo crash fallback to wry | Not started |

### 4.2 Engine Abstraction Hardening

| Task | Status |
|------|--------|
| Formalize `PaneRenderer` trait (create, navigate, resize, destroy, execute_js, screenshot) | Not started |
| Engine-specific benchmarks (wry vs servo render latency) | Not started |
| Dual-engine regression test suite | Not started |

---

## Phase 5: Extension API Completion (v0.23.0)

**Goal:** Implement key missing MV3 APIs. Establish extension distribution model.

### 5.1 Missing MV3 APIs

| API | Priority | Effort | Status |
|-----|----------|--------|--------|
| `declarativeNetRequest` | High | High | Manifest parsing done; rule engine not started |
| `permissions.request()` | High | Medium | Not started |
| `cookies` | Medium | Medium | Not started |
| `alarms` | Medium | Low | Not started |
| `menus/contextMenus` | Medium | Medium | Not started |
| `notifications` | Medium | Low | Not started |
| `webNavigation` | Medium | Medium | Not started |
| `i18n` | Low | Medium | Not started |
| `devtools` | Low | High | Not started |
| `sidePanel` | Low | High | Not started |
| `theme` | Low | Medium | Not started |

### 5.2 Background Script Execution

- **JS runtime:** Evaluate background scripts in a WebKitGTK offscreen webview or v8 binding
- **Event dispatch:** Fire browser events (onInstalled, onStartup, etc.) into runtime
- **Port messaging:** Implement long-lived messaging between background and content scripts

### 5.3 Extension Distribution

- **Extension manifest signing:** Verify integrity before loading
- **Extension marketplace specification:** Define distribution protocol
- **Automatic updates:** Check for manifest version updates

---

## Phase 6: Sync Protocol Implementation (v0.24.0)

**Goal:** Fully functional cross-device sync with end-to-end encryption.

### 6.1 Core Sync

| Component | Status |
|-----------|--------|
| Manifest computation (content-addressed) | Done (sync::core) |
| Delta detection | Done (sync::core) |
| Age encryption (E2EE) | Done (sync::crypto) |
| Filesystem watcher (real-time) | Done (sync::watcher) |
| WebDAV transport | Not started |
| Sync execution loop | Not started |
| CRDT conflict resolution | Not started |

### 6.2 WebDAV Transport

- **HTTP client:** Implement PUT/GET/DELETE/PROPFIND with reqwest
- **Authentication:** HTTP Basic, Bearer token, or client certificate
- **Directory management:** Create remote directories on first sync
- **Retry logic:** Exponential backoff for transient failures

### 6.3 Sync Execution

- **Push:** Manifest compare -> delta compute -> encrypt -> upload
- **Pull:** Download -> load manifest -> compare -> download deltas -> decrypt -> apply
- **Merge:** Last-write-wins for bookmarks; union with dedup for history
- **Status UI:** `:sync-status` command; sync indicator in status bar
- **Conflict UI:** Show merge conflicts in `:sync-conflicts` panel

---

## Phase 7: Polish and Growth (v0.25.0)

**Goal:** Daily-driver UX completeness. Developer experience hardening.

### 7.1 UX Features

| Feature | Effort | Notes |
|---------|--------|-------|
| Vertical tabs | Low | Sidebar layout already supports |
| Tab groups | Medium | Color-coded groups |
| Split pane tabs | High | Multiple tabs within a pane |
| Drag-and-drop tab reorder | Medium | Drag between panes |
| Tab search | Low | Fuzzy search across open tabs |
| Keyboard macro recording | Medium | Record/replay key sequences |
| Session manager | Medium | Visual session list with preview |
| Workspace templates | Medium | Predefined pane layouts |
| Picture-in-picture | Medium | Detach video to floating window |

### 7.2 Developer Experience

| Feature | Effort | Notes |
|---------|--------|-------|
| `aileron --debug` | Low | Structured debug output |
| `aileron --profile <dir>` | Low | Custom profile directory |
| `aileron --dump-config` | Low | Print resolved config |
| Performance overlay | Low | Real-time FPS, memory, frame time |
| Crash reporter | Medium | Structured crash dump with stack trace |
| Telemetry opt-in | Low | Anonymous usage statistics |
| Remote debugging | High | Chrome DevTools Protocol bridge |

### 7.3 Documentation Completion

| Document | Action |
|----------|--------|
| README.md | Maintain with each release |
| CONTRIBUTING.md | Add macOS/Windows contributor guidance |
| docs/lua-scripting.md | Navigate examples now functional (aileron.navigate() implemented) |
| docs/extension-api.md | Document missing APIs as "planned" |
| docs/config-reference.md | Create: full config.toml reference |
| docs/keybindings-reference.md | Create: printable keybinding cheat sheet |
| docs/architecture.md | Create: high-level architecture overview |
| Architecture ADRs | Maintain with each major decision |

---

## Phase 8: v1.0.0 Release Criteria

### Release Blockers (Must-Have)

- [x] Zero undocumented unsafe blocks (19/19 have SAFETY comments; verified v0.18.1)
- [x] Zero `unwrap()` outside compile-time constants in production paths
- [x] `#[must_use]` on all public Result/Option returns (48 across 28 files; verified v0.18.1)
- [ ] Silent error swallows: ~15 benign shutdown channel sends documented; converting would spam (accepted)
- [ ] macOS runs full test suite in CI
- [ ] Windows runs full test suite in CI
- [ ] >= 95% branch coverage on critical paths (wm, input, extensions, adblock)
- [ ] All performance targets validated (per Section 2.1)
- [x] Pre-commit hook passes deterministically (6-gate hook validated v0.18.1)
- [ ] All documentation accurate and free of placeholder/stub claims
- [ ] At least 8 of 11 missing MV3 WebExtensions APIs implemented
- [ ] WebDAV sync operational
- [ ] All integration test gaps closed (downloads, terminal done; website visit deferred -- needs display server)

### Release Should-Have

- [ ] Servo engine functional (even if experimental flag)
- [ ] Flatpak build published on Flathub
- [ ] AUR package stable (not `-git`)
- [ ] macOS notarized build
- [ ] Windows installer (MSIX)
- [ ] Crash reporter with telemetry opt-in

### Release Nice-to-Have

- [ ] Keyboard macro recording
- [ ] Extension marketplace
- [ ] CRDT conflict resolution for sync
- [ ] Remote debugging via CDP
- [ ] Picture-in-picture mode

---

## Timeline Estimate

| Phase | Version | Duration | Dependencies |
|-------|---------|----------|-------------|
| 1. Hardening (remaining) | v0.19.0 | 1-2 weeks | None |
| 2. Performance | v0.20.0 | 2-3 weeks | Phase 1 |
| 3. Platform (macOS/Windows) | v0.21.0 | 3-4 weeks | Phase 1 (CI tests) |
| 4. Servo | v0.22.0 | 4-8 weeks | External (Servo API stabilization) |
| 5. Extensions | v0.23.0 | 3-4 weeks | Phase 1 |
| 6. Sync | v0.24.0 | 2-3 weeks | Phase 1 |
| 7. Polish | v0.25.0 | 2-3 weeks | Phases 1-6 |
| 8. v1.0.0 RC | v1.0.0-rc | 2 weeks | All phases |
| 9. v1.0.0 | v1.0.0 | 1 week | RC stabilization |

**Estimated total: 19-30 weeks to v1.0.0** (depending on Servo readiness and platform testing).

---

## Beyond v1.0.0: Future Horizons

### Horizon 1: Multi-Device Ecosystem (v1.1.0)

- **ARP mobile client:** Flutter or SwiftUI client consuming ARP WebSocket protocol
- **Push sync notifications:** Real-time sync triggers via WebSocket or FCM
- **Cross-device clipboard:** ARP clipboard sharing with encryption
- **Remote tab access:** Access desktop tabs from mobile; send tabs between devices

### Horizon 2: AI-Native Browsing (v1.2.0)

- **MCP agent tools expansion:** DOM manipulation, form filling, data extraction, multi-step workflows
- **Local LLM integration:** Ollama-backed summarization, translation, content analysis
- **Semantic history:** Vector embeddings of visited pages for semantic search
- **Workflow automation:** Lua-driven browser automation without Selenium

### Horizon 3: Rendering Independence (v1.3.0)

- **Servo as default engine:** When Servo's Embedder API stabilizes
- **Multi-engine per pane:** Different engines for different panes simultaneously
- **Custom renderer API:** Third-party rendering engine plugins via PaneRenderer trait
- **Headless mode:** Full headless rendering for server-side automation

### Horizon 4: Distributed Browsing (v2.0.0)

- **Remote rendering:** Render web pages on server, stream to thin client
- **Session sharing:** Collaborative browsing with shared pane state
- **Sandboxed containers:** Per-tab OS-level isolation (container tabs)
- **Plugin ecosystem:** Extension marketplace with signed distribution
- **WebAssembly extensions:** Wasm-based extension sandbox (beyond MV3 JS model)

---

## Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Servo Embedder API changes or indefinite delay | High | High | Keep wry (WebKitGTK) as primary; abstract behind PaneRenderer trait |
| WebKitGTK API breakage (wry version bumps) | Medium | High | Pin wry version; CI test matrix against multiple WebKitGTK versions |
| wry `!Send + !Sync` constraint | Ongoing | Medium | Current Arc<RwLock<>> + mpsc bridge is functional; no alternative |
| macOS/Windows platform bugs undiscovered | Medium | Medium | Extend CI to run tests, not just compile-check |
| Extension API scope creep (MV3 spec grows) | Medium | Low | Follow MV3 spec strictly; skip deprecated MV2 features |
| Performance regression with feature additions | Medium | Medium | Automated regression detection in CI; benchmark baseline |
| WebDAV sync complexity underestimated | Medium | Medium | Start with simple Local transport; iterate on WebDAV |

---

## v0.18.1 Quality Audit Results (2026-05-12)

### Audit Execution

| Check | Tool | Result |
|-------|------|--------|
| Unit tests (lib) | `cargo test --lib` | 1038 passed, 0 failed |
| Integration tests (13 suites) | `cargo test --test '*'` | 253 passed, 0 failed |
| Doc tests | `cargo test --doc` | 4 passed, 0 failed |
| Clippy | `cargo clippy --all-targets -- -D warnings` | Zero warnings |
| Rustfmt | `cargo fmt --all -- --check` | Zero issues |
| Security audit | `cargo audit` | Zero critical vulnerabilities; 13 allowed transitive warnings (GTK3 unmaintained) |
| Benchmarks | `cargo bench --no-run` | 27 benchmarks compile and pass |
| Pre-commit hook | Bash script (6 gates) | All gates pass (fmt, clippy, lib, doc, integration, docs) |
| Doc generation | `cargo doc --no-deps --all-features` | Compiles without warnings |
| Unsafe audit | Manual + grep | 19 blocks (down from ~50), all FFI with SAFETY comments |
| Emoji audit | Regex scan | Zero emojis in documentation |
| Stub audit | Grep `TODO\|FIXME\|STUB\|placeholder` | 0 code stubs; 1 legitimate STUB_GIF for adblock |
| #[must_use] coverage | Grep | 48 attributes across 28 files; all public Result/Option returns annotated |

### CI Configuration Verification

| Item | Pre-Commit Hook | GitHub Actions CI |
|------|-----------------|-------------------|
| `cargo fmt` | Checked | `fmt` job (ubuntu) |
| `cargo clippy` | `--all-targets -D warnings` | `--all-targets -D warnings` (fixed) |
| Unit tests | `cargo test --lib` | `cargo test --lib --release` |
| Integration tests | All 13 suites | All 13 suites (fixed) |
| Doc tests | `cargo test --doc` | `cargo test --doc` |
| Doc generation | `cargo doc --no-deps --all-features` | `cargo doc --no-deps --all-features` |
| Security audit | Not in hook | `cargo audit` |
| Benchmarks | Not in hook | `cargo bench` (regression detection) |
| Cross-compile | Not in hook | macOS/Windows/aarch64 checks |

---

## Immediate Next Steps (Next 2 Weeks)

Priority-ordered by impact:

1. **HIGH: Frame capture buffer reuse (render.rs:169):** Pre-allocate 8MB buffer on OffscreenWebView, reuse across frames. Eliminates ~8MB allocation per dirty pane per frame during scrolling.
2. **HIGH: Tab display cache borrow refactoring (panels.rs):** Eliminate 3-4 String clones per tab per frame. Requires restructuring egui closure to pre-extract data before mutable borrow section.
3. **HIGH: panes() return iterator (wm/tree.rs):** Replace `panes_cache.borrow().clone()` with borrowed reference or iterator. Eliminates 32*N bytes cloned per frame.
4. **MEDIUM: Feature gate `terminal` module:** Add `#[cfg(feature = "terminal")]` at 15+ call sites. Removes portable-pty + alacritty_terminal from compilation. Estimated: saves ~1-2m compile time.
5. **MEDIUM: Feature gate `lua` module:** Add `#[cfg(feature = "lua")]` at 10+ call sites. Removes vendored Lua 5.4 C compilation. Estimated: saves ~30-60s compile time.
6. **MEDIUM: Benchmark regression CI:** Store baseline criterion results, compare on each PR, fail if >10% regression.
7. **LOW: HashMap<String,String> -> HashMap<Uuid,String> for tab_names:** Avoids pane_id.to_string() allocation per tab per frame. Touches DB layer + 6 call sites.
