# Aileron Production Roadmap: v0.20.0 to v1.0.0 and Beyond

## Current State (2026-05-17)

| Metric | Value |
|--------|-------|
| Version | 0.20.0 |
| Lib tests | 1037 |
| Integration tests | 252 (13 suites) |
| Doc tests | 4 |
| Total tests | 1294 (1037 lib, 4 bin, 253 integration, 4 doc) |
| Clippy | Zero warnings (all-targets, -D warnings) |
| Formatting | Zero issues (cargo fmt) |
| Unsafe blocks | 21 (12 FFI: WebKitGTK, Cairo, X11, spellcheck; 7 std::env set_var/remove_var in pre-thread-spawn contexts; 2 in wry_engine fallback stubs) |
| `#[must_use]` | 49 attributes across 21 files |
| LOC | ~51,413 Rust across 135 source files |
| Release profile | LTO thin, strip, panic=abort, codegen-units=1 |
| Binary size | ~21 MB stripped (x86_64 Linux) |
| Clean build time | 7m 14s (dev, 6-core x86_64) |
| Incremental build | ~2.5s |
| Vulnerability scan | Zero critical (13 allowed transitive GTK3 warnings) |
| Benchmarks | 27 criterion benchmarks (all passing) |
| CI | Linux (full test), macOS (compile), Windows (compile), cross-compile (aarch64, x86_64) |
| Pre-commit hook | 5 gates: fmt, check, clippy, lib tests, doc tests |
| Pre-push hook | 2 gates: `cargo test --tests` (all integration), doc generation (`cargo doc --no-deps --all-features`) |
| Feature flags | mcp, arp, sync, passwords, lua, terminal (all with dependency-level gating) |

### Core Systems Status

| System | Status | Tests |
|--------|--------|-------|
| Tiling WM (BSP tree) | Complete | 29 unit, 6 integration |
| Modal input (Normal/Insert/Command/Find) | Complete | -- |
| Command palette (Nucleo fuzzy search) | Complete | -- |
| Web rendering (wry/WebKitGTK) | Complete | -- |
| Lua scripting (sandboxed) | Complete | 42 tests |
| MCP bridge (JSON-RPC over stdio) | Complete | 32 tools |
| Ad blocking (EasyList + Aho-Corasick) | Complete | 45 tests |
| Password manager (Bitwarden + keyring) | Complete | -- |
| Extension system (partial MV3) | Complete | 6 API traits, 82 tests |
| Sync protocol (delta + E2EE Age) | Partial | Transports: Local/SSH only |
| i18n (9 locales, runtime switch) | Complete | -- |
| Terminal emulator (alacritty_terminal) | Complete | -- |
| Servo engine | Skeleton | 7 no-op methods, blocked on upstream |
| macOS support | Compile-only | No integration tests |
| Windows support | Compile-only | No integration tests |

### v0.20.0 Completed Items

- Feature gate `terminal` module (~15 call sites, removes portable-pty + alacritty_terminal from `--no-default-features` build)
- Feature gate `lua` module (~10 call sites, removes vendored Lua 5.4 C compilation from `--no-default-features` build)
- Texture/frame buffer pooling (capture buffers stored in `HashMap<Uuid, Vec<u8>>`, reused across frames, only reallocated on dimension change)
- Tab-unload LRU fix (uses `find_lru_pane()` by focus timestamp instead of arbitrary `iter().find()`)
- Dependency-level feature gates (image, base64 behind mcp; tokio-tungstenite behind arp; fastcdc, blake3, age, notify behind sync; keyring behind passwords)
- Hot-path allocation audit (22 findings: 3 HIGH, 9 MEDIUM, 10 LOW documented in CHANGELOG v0.19.0)
- `chrono/serde` unused feature removed
- Collapsible `if` clippy fixes (7 patterns in event_handler.rs and main.rs)

### Remaining Hot-Path Items (from v0.19.0 audit)

| Priority | Finding | Location | Estimate |
|----------|---------|----------|----------|
| HIGH | Tab display cache String clones (3-4 per tab per frame) | `ui/panels.rs` | 3h |
| HIGH | `panes()` returns cloned Vec instead of iterator | `wm/tree.rs` | 2h |
| MEDIUM | HashMap\<String,String\> key alloc (pane_id.to_string() per tab per frame) | `db/tab_names.rs` + 6 call sites | 4h |
| MEDIUM | ARP JSON serialization per frame | `arp/server.rs` | 2h |
| MEDIUM | CachedThemeColors clone per frame | `app/render.rs` | 2h |

---

## v0.21.0: Performance Optimization

**Goal:** Eliminate remaining per-frame heap allocations, establish performance baselines, harden CI regression detection.

**Duration:** 2-3 weeks

### Tasks

| ID | Task | Estimate | Status |
|----|------|----------|--------|
| P-01 | Refactor tab display cache in panels.rs (pre-extract data before mutable borrow) | 3h | Pending |
| P-02 | Change `panes()` to return iterator, not cloned Vec | 2h | Pending |
| P-03 | HashMap\<String,String\> -> HashMap\<Uuid,String\> for tab_names | 4h | Pending |
| P-04 | Cache ARP JSON serialization output | 2h | Pending |
| P-05 | Cache CachedThemeColors via Arc or lazy init | 2h | Pending |
| P-06 | Add frame-time percentile tracking (p50, p95, p99) | 3h | Pending |
| P-07 | Create startup latency benchmark (cold start to first paint) | 2h | Pending |
| P-08 | Expand frame_bench.rs with multi-pane render benchmarks | 3h | Pending |
| P-09 | Store benchmark baselines in CI, fail on >10% regression | 4h | Pending |
| P-10 | Profile compile times with `cargo build --timings` | 2h | Pending |
| P-11 | Evaluate cranelift codegen backend for debug builds | 2h | Pending |
| P-12 | Split main.rs (currently ~2618 lines, target <2000) | 3h | Pending |

### Success Criteria

- Zero per-frame heap allocations on critical rendering path
- Startup latency < 2s cold, < 500ms warm
- Benchmark baselines stored and regression-checked in CI
- Debug build time reduced by >15% from feature gating

---

## v0.22.0: macOS Platform

**Goal:** First-class macOS support with verified rendering, tests, and platform conventions.

**Duration:** 2-3 weeks

### Tasks

| ID | Task | Estimate | Status |
|----|------|----------|--------|
| M-01 | Verify WebKit rendering on macOS (WKWebView via wry) | 4h | Pending |
| M-02 | Verify offscreen rendering path on macOS | 4h | Pending |
| M-03 | Implement macOS-native file dialog (NSOpenPanel via objc FFI) | 6h | Pending |
| M-04 | macOS-specific keymap (Cmd vs Ctrl, system shortcuts) | 4h | Pending |
| M-05 | Run full integration test suite on macOS CI | 2h | Pending |
| M-06 | Run clippy on macOS CI | 1h | Pending |
| M-07 | Create macOS install guide in CONTRIBUTING.md | 2h | Pending |

### Blocked

| Item | Blocker | Mitigation |
|------|---------|------------|
| Code signing and notarization | Apple Developer account ($99/yr) | Defer to post-v1.0 |

### Success Criteria

- `cargo test` passes all 1293+ tests on macOS CI
- WebKit renders real websites on macOS
- File dialogs work natively on macOS
- macOS keymap matches platform conventions

---

## v0.23.0: Windows Platform + Extension API

**Goal:** Windows support + critical extension infrastructure.

**Duration:** 3-4 weeks

### Windows Tasks

| ID | Task | Estimate | Status |
|----|------|----------|--------|
| W-01 | Verify WebView2 rendering on Windows | 4h | Pending |
| W-02 | Windows-native file dialog (COM IFileDialog) | 6h | Pending |
| W-03 | Windows-specific keymap (Alt vs Ctrl) | 4h | Pending |
| W-04 | Run full integration test suite on Windows CI | 2h | Pending |
| W-05 | Run clippy on Windows CI | 1h | Pending |

### Extension API Tasks

| ID | Task | Estimate | Status |
|----|------|----------|--------|
| E-01 | Implement `cookies` API (get, set, remove, onChanged) | 8h | Pending |
| E-02 | Implement `declarativeNetRequest` rule engine | 12h | Pending |
| E-03 | Implement `permissions.request()` prompt | 4h | Pending |
| E-04 | Implement `alarms` API | 4h | Pending |
| E-05 | Implement `contextMenus` API | 6h | Pending |
| E-06 | Background script JS runtime (quick-js or v8 isolate) | 16h | Pending |
| E-07 | Port messaging between background and content scripts | 8h | Pending |

### Blocked

| Item | Blocker | Mitigation |
|------|---------|------------|
| Windows code signing | EV code signing certificate | Defer to post-v1.0 |
| Windows installer (MSIX) | Code signing + Microsoft Store account | Defer to post-v1.0 |

### Success Criteria

- `cargo test` passes all 1293+ tests on Windows CI
- WebView2 renders real websites on Windows
- At least 4 additional extension APIs implemented
- Background script JS runtime functional

---

## v0.24.0: Sync Protocol

**Goal:** Fully operational cross-device sync with E2EE.

**Duration:** 2-3 weeks

### Tasks

| ID | Task | Estimate | Status |
|----|------|----------|--------|
| S-01 | Implement WebDAV transport (PUT/GET/DELETE/PROPFIND via reqwest) | 12h | Pending |
| S-02 | WebDAV authentication (HTTP Basic, Bearer token) | 4h | Pending |
| S-03 | WebDAV retry with exponential backoff | 3h | Pending |
| S-04 | Sync execution loop (manifest -> delta -> encrypt -> upload) | 8h | Pending |
| S-05 | Pull path (download -> compare -> decrypt -> apply) | 8h | Pending |
| S-06 | CRDT merge for bookmarks (last-write-wins + union with dedup) | 8h | Pending |
| S-07 | CRDT merge for history (union with dedup) | 4h | Pending |
| S-08 | Sync status UI (`:sync-status`, status bar indicator) | 4h | Pending |
| S-09 | Conflict UI (`:sync-conflicts` panel) | 6h | Pending |

### Success Criteria

- WebDAV sync round-trips bookmarks and history between two instances
- E2EE sync with passphrase (age encryption, already implemented)
- Conflict resolution UI functional
- Sync status visible in status bar

---

## v0.25.0: Polish and Distribution

**Goal:** Daily-driver UX completeness + distribution infrastructure.

**Duration:** 2-3 weeks

### UX Tasks

| ID | Task | Estimate | Status |
|----|------|----------|--------|
| U-01 | Vertical tabs (sidebar layout already supports) | 3h | Pending |
| U-02 | Tab groups (color-coded) | 6h | Pending |
| U-03 | Split pane tabs (multiple tabs per BSP leaf) | 12h | Pending |
| U-04 | Drag-and-drop tab reorder | 6h | Pending |
| U-05 | Tab search (fuzzy search across open tabs) | 3h | Pending |
| U-06 | Keyboard macro recording (`:macro-record`, `:macro-play`) | 8h | Pending |
| U-07 | Session manager (visual session list with preview) | 8h | Pending |
| U-08 | Workspace templates (predefined pane layouts) | 4h | Pending |

### Distribution Tasks

| ID | Task | Estimate | Status |
|----|------|----------|--------|
| D-01 | Linux: AppImage build (via cargo-bundle) | 4h | Pending |
| D-02 | Linux: Flatpak build (manifest exists, needs verification) | 6h | Pending |
| D-03 | Linux: AUR stable package (non-git) | 2h | Pending |
| D-04 | Auto-update check (GitHub API version comparison on startup) | 4h | Pending |
| D-05 | CLI flags: `--debug`, `--profile <dir>`, `--dump-config` | 3h | Pending |
| D-06 | Crash reporter (structured dump with stack trace) | 6h | Pending |
| D-07 | Performance overlay (real-time FPS, memory, frame time) | 3h | Pending |

### Documentation Tasks

| ID | Document | Action |
|----|----------|--------|
| DOC-01 | docs/architecture.md | Create: high-level architecture overview |
| DOC-02 | CONTRIBUTING.md | Add macOS/Windows contributor guidance |
| DOC-03 | docs/extension-api.md | Document missing APIs as "planned" |
| DOC-04 | docs/lua-scripting.md | Verify aileron.navigate() examples accurate |
| DOC-05 | docs/config-reference.md | Update with any new config options |
| DOC-06 | docs/keybindings-reference.md | Update with any new bindings |

### Success Criteria

- Multiple tabs per pane functional
- At least 2 Linux distribution formats (AppImage + Flatpak or AUR stable)
- Auto-update notification functional
- All documentation accurate, no misleading claims

---

## v0.26.0: Servo Integration (Conditional)

**Goal:** Functional dual-engine rendering. Servo as experimental option, wry as stable fallback.

**Duration:** 4-8 weeks (depends entirely on external Servo readiness)

**Status:** BLOCKED on upstream Servo Embedder trait stabilization.

### Prerequisites (External)

- Servo `Embedder` trait stabilization
- Servo wgpu texture export support
- Servo SpiderMonkey JS engine API

### Tasks

| ID | Task | Estimate | Status |
|----|------|----------|--------|
| SV-01 | Implement `ServoPane::new()` with real Servo initialization | 8h | Blocked |
| SV-02 | Implement `ServoPane::navigate()` with real URL loading | 4h | Blocked |
| SV-03 | Texture sharing via `servo/texture_share.rs` (scaffolded) | 6h | Blocked |
| SV-04 | Engine selection runtime toggle (`:engine servo|webkit|auto`) | 4h | Blocked |
| SV-05 | Per-domain compat overrides | 4h | Blocked |
| SV-06 | Formalize `PaneRenderer` trait (create, navigate, resize, destroy, execute_js, screenshot) | 8h | Pending |
| SV-07 | Engine-specific benchmarks (wry vs servo render latency) | 4h | Blocked |
| SV-08 | Graceful Servo crash fallback to wry | 8h | Blocked |
| SV-09 | Dual-engine regression test suite | 8h | Blocked |

### Mitigation

Servo readiness is the single largest external risk. If Servo does not stabilize within the v1.0.0 timeline, Servo integration moves to a post-v1.0.0 horizon. The PaneRenderer trait formalization (SV-06) can proceed independently.

---

## v1.0.0 Release Criteria

### Release Blockers (Must-Have)

All must be met before tagging v1.0.0.

| ID | Requirement | Status |
|----|-------------|--------|
| R-01 | All tests pass on Linux, macOS, Windows CI | Pending (macOS/Windows CI not yet running tests) |
| R-02 | >= 95% branch coverage on critical paths (wm, input, extensions, adblock) | Pending |
| R-03 | Zero clippy warnings on all 3 platforms (`--all-targets -D warnings`) | Pending (macOS/Windows unverified) |
| R-04 | All 21 unsafe blocks have SAFETY comments | Done |
| R-05 | Zero risky `unwrap()` in production code paths (compile-time constants only) | Done |
| R-06 | `#[must_use]` on all public Result/Option returns (49/49) | Done |
| R-07 | Pre-commit hook passes deterministically (5-gate) | Done |
| R-08 | Pre-push hook passes deterministically (2-gate) | Done |
| R-09 | All performance targets validated: startup <2s cold, input latency p95 <33ms | Pending (requires runtime measurement) |
| R-10 | All documentation accurate, no misleading claims (Servo marked experimental) | Partial |
| R-11 | At least 8 of 11 missing MV3 APIs implemented | Pending (0/11) |
| R-12 | WebDAV sync operational with E2EE | Pending |
| R-13 | ~15 benign shutdown channel sends documented and accepted | Done |

### Release Should-Have

| ID | Requirement | Status |
|----|-------------|--------|
| S-01 | Servo engine functional (experimental flag) | Blocked |
| S-02 | Flatpak published on Flathub | Pending |
| S-03 | AUR stable package (non-git) | Pending |
| S-04 | macOS notarized build | Blocked (Apple Developer account) |
| S-05 | Windows installer (MSIX) | Blocked (code signing) |
| S-06 | Crash reporter with telemetry opt-in | Pending |
| S-07 | Keyboard macro recording | Pending |
| S-08 | Extension marketplace specification | Pending |
| R-09 | Reproducible builds (Nix flake verified) | Pending |

### Release Nice-to-Have

- Extension marketplace
- CRDT conflict resolution for sync
- Remote debugging via Chrome DevTools Protocol
- Picture-in-picture mode
- External security audit

### Release Process

1. Create `release/v1.0.0` branch from `main`
2. Feature freeze; only bug fixes allowed
3. Run full test matrix on all 3 platforms
4. Generate SBOM, verify dependency checksums
5. Tag `v1.0.0-rc.1`, build release artifacts
6. Internal testing period (1 week)
7. Address critical issues found in RC
8. Tag `v1.0.0`, trigger release workflow
9. Publish to AUR, update documentation site
10. Announcement via GitHub Discussion

### v1.0.0 Success Criteria

- All R-01 through R-13 met
- At least 5 of 8 should-have requirements met
- Zero P0/P1 bugs open
- Documentation reviewed by at least one external contributor
- Release artifacts available for Linux, macOS, Windows

---

## Timeline Estimate

| Phase | Version | Duration | Dependencies |
|-------|---------|----------|-------------|
| Performance optimization | v0.21.0 | 2-3 weeks | None |
| macOS platform | v0.22.0 | 2-3 weeks | v0.21.0 (CI hardening) |
| Windows + extensions | v0.23.0 | 3-4 weeks | v0.21.0 |
| Sync protocol | v0.24.0 | 2-3 weeks | v0.21.0 |
| Polish + distribution | v0.25.0 | 2-3 weeks | v0.21.0-v0.24.0 |
| Servo (conditional) | v0.26.0 | 4-8 weeks | External (Servo API) |
| v1.0.0 RC | v1.0.0-rc.1 | 1 week | All above phases |
| v1.0.0 | v1.0.0 | 1 week | RC stabilization |

**Critical path to v1.0.0 (excluding Servo): 12-17 weeks**

**With Servo: 16-25 weeks** (Servo may run in parallel if API becomes available).

Parallel work possible: v0.22.0 (macOS) and v0.23.0 (Windows+extensions) are independent of each other. v0.24.0 (sync) is independent of platform work. This can compress the wall-clock timeline to approximately 10-14 weeks with concurrent tracks.

*Estimates assume single full-time developer.*

---

## Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Servo Embedder API never stabilizes | High | High | wry/WebKitGTK remains primary; PaneRenderer trait abstraction; Servo deferred to post-v1.0 |
| WebKitGTK API breakage (wry version bumps) | Medium | High | Pin wry version; CI test matrix against multiple WebKitGTK versions |
| wry `!Send + !Sync` constraint | Certain | Medium | Arc\<RwLock\<\>\> + mpsc bridge pattern is functional and tested; no alternative exists |
| macOS/Windows platform bugs undiscovered at scale | Medium | Medium | Extend CI to run full test suites, not just compile-check; recruit platform-specific testers |
| Extension API scope creep (MV3 spec changes) | Medium | Low | Follow MV3 spec strictly; skip deprecated MV2 features; deprecation period for breaks |
| Performance regression from feature additions | Medium | Medium | Automated benchmark regression in CI (v0.21.0); >10% regression fails CI |
| WebDAV sync complexity underestimated | Medium | Medium | Start with Local/SSH transport (already done); iterate on WebDAV incrementally |
| GTK3 deprecation (wry dependency) | Medium | High | Monitor wry GTK4 migration; no action needed until upstream moves; pin if necessary |
| Single-developer bus factor | High | High | Comprehensive documentation; clear ADRs; modular architecture; contribution guide |
| Code signing costs ($99/yr Apple, EV cert for Windows) | Certain | Low | Defer to post-v1.0; seek sponsorship if needed |

---

## Known Gaps (Audit 2026-05-12)

1. **Servo non-functional:** Engine selection lists `servo` but implementation is 7 no-op methods. Must be documented as experimental.
2. **WebDAV sync not implemented:** README previously claimed "ready for implementation." Actual code: Local/SSH transport only. Documentation fixed.
3. **Background JS evaluation not implemented:** Extension background scripts loaded but not executed in a JS runtime.
4. **Website visit integration test deferred:** Requires display server. Accepted as post-v1.0.
5. **Silent error swallows:** ~15 benign shutdown channel sends remain. Converting to tracing::warn would spam logs during normal shutdown. Accepted and documented.
6. **Heap profiling:** Only global RSS via `/proc/self/status`. Per-pane attribution requires allocator integration (jemalloc/mimalloc) or thread-local accounting.

---

## Beyond v1.0.0: Future Horizons

### Horizon 1: Multi-Device Ecosystem (v1.1.0-v1.2.0)

| Feature | Description | Dependencies |
|---------|-------------|--------------|
| ARP mobile client | Flutter/SwiftUI mobile app consuming ARP WebSocket protocol | ARP protocol stabilization |
| Push sync notifications | Real-time sync triggers via WebSocket or push service | v1.0 sync infrastructure |
| Cross-device clipboard | Encrypted clipboard sharing between devices | ARP protocol |
| Remote tab access | View and control desktop tabs from mobile | ARP mobile client |
| Multi-window | Independent tiled windows with shared state | WM refactoring |
| Reading list | Save articles for later with offline caching | Sync infrastructure |
| WebAuthn/passkey support | Passwordless authentication for web forms | Security audit |

### Horizon 2: AI-Native Browsing (v1.3.0-v1.4.0)

| Feature | Description | Dependencies |
|---------|-------------|--------------|
| MCP agent tools expansion | DOM manipulation, form filling, data extraction, multi-step workflows | Extension JS runtime |
| Local LLM integration | Ollama-backed summarization, translation, content analysis | No external deps |
| Semantic history | Vector embeddings of visited pages for semantic search | Embedding model integration |
| Workflow automation | Lua-driven browser automation without Selenium | v0.20+ Lua infrastructure |
| Smart tab management | AI-suggested tab grouping, auto-close stale tabs | Semantic history |

### Horizon 3: Rendering Independence (v1.5.0-v1.6.0)

| Feature | Description | Dependencies |
|---------|-------------|--------------|
| Servo as default engine | When Servo Embedder API stabilizes | Servo project milestone |
| Multi-engine per pane | Different engines for different panes simultaneously | PaneRenderer trait formalization |
| Custom renderer API | Third-party rendering engine plugins via PaneRenderer trait | Plugin ABI design |
| Headless mode | Full headless rendering for server-side automation | Servo or headless WebKit |
| Wayland-native rendering | Remove X11 fallback entirely | wry Wayland support |

### Horizon 4: Distributed Browsing (v2.0.0)

| Feature | Description | Dependencies |
|---------|-------------|--------------|
| Remote rendering | Render web pages on server, stream to thin client | GPU streaming infrastructure |
| Session sharing | Collaborative browsing with shared pane state | CRDT sync |
| Sandboxed containers | Per-tab OS-level isolation (Flatpak-like) | Platform integration |
| Plugin ecosystem | Rust-based plugins with stable ABI | ABI specification |
| Extension store | Curated extension repository with signed distribution | Extension API v1.0 |
| Wasm extensions | WebAssembly extension sandbox (beyond MV3 JS model) | Wasm runtime integration |
| Embedded/IoT | Lightweight build for resource-constrained devices | Feature gate cleanup |

---

## Performance Budgets (Targets for v1.0.0)

| Metric | Target | Measurement Method |
|--------|--------|-------------------|
| 1 pane @ 60 fps | >= 60 fps (frame time <= 16.67ms) | Frame counter, criterion benchmark |
| 4 panes @ 30 fps | >= 30 fps (frame time <= 33.33ms) | Frame counter, multi-pane benchmark |
| 16 panes @ 15 fps | >= 15 fps (frame time <= 66.67ms) | Frame counter, stress benchmark |
| Frame time jitter (1 sigma) | < 2 ms | Statistical profiler |
| Cold start to first paint | < 2 s | Startup benchmark |
| Warm start to first paint | < 500 ms | Startup benchmark |
| Input latency p95 | < 33 ms | InputLatencyTracker |
| Input latency p99 | < 100 ms | InputLatencyTracker |
| Memory per web pane | < 100 MB RSS | /proc/self/status + heuristic |
| Memory per terminal pane | < 10 MB RSS | /proc/self/status + heuristic |

## CI/CD Gate Summary

| Gate | Hook | Command |
|------|------|---------|
| Format check | Pre-commit | `cargo fmt --all -- --check` |
| Compile check | Pre-commit | `cargo check --all-targets` |
| Clippy lint | Pre-commit | `cargo clippy --all-targets -- -D warnings` |
| Lib tests | Pre-commit | `cargo test --lib` |
| Doc tests | Pre-commit | `cargo test --doc` |
| Integration tests | Pre-push | `cargo test --tests` (all 13 suites) |
| Doc generation | Pre-push | `cargo doc --no-deps --all-features` |
| Security audit | CI | `cargo audit` |
| Benchmarks | CI | `cargo bench` (regression detection) |
| Cross-compile | CI | macOS, Windows, aarch64 checks |
| Coverage | CI | `cargo-llvm-cov` (lcov) via Codecov |
