# TASK-K29: Servo Embedder API Evaluation

**Date:** 2026-04-18
**Author:** Aileron Architecture Team
**Status:** DRAFT
**Related:** ADR-001, ADR-002, ADR-005, BP-GFX-COMPOSITOR-001, BP-APP-CORE-001

---

## 1. Servo Embedder API Status

### 1.1 Release and Crate Availability

Servo published its first crates.io release on 2026-04-13: **servo v0.1.0** (LTS).

| Attribute | Value |
|-----------|-------|
| Crate | `servo` on crates.io |
| Version | 0.1.0 (LTS), preceded by v0.0.1-v0.0.6 (monthly releases since Oct 2025) |
| License | MPL-2.0 |
| Repository | github.com/servo/servo (55,854 commits, 36.4k stars) |
| Documentation | doc.servo.org, book.servo.org |
| Funding | ~7,000 USD/month (Igalia, Futurewei, NLnet, Sovereign Tech Agency) |

The LTS release cycle is every 6 months with 9-month support window (security fixes only). This is directly relevant to Aileron's needs -- we can pin to an LTS branch and upgrade on a predictable schedule, which aligns with ADR-001's strategy of pinning to a specific commit.

### 1.2 API Surface

The `servo` crate exposes two primary types:

**`Servo`** -- the top-level engine handle (cloneable, in-process):
- `ServoBuilder` for configuration (preferences, resource readers, protocol handlers)
- `WebView::new()` to create individual web views
- Delegates for event handling (`ServoDelegate`, `WebViewDelegate`)
- `NetworkManager` for cache management
- `SiteDataManager` for cookies/localStorage/sessionStorage

**`WebView`** -- per-pane rendering context:
- `load_url()`, `reload()`, `navigate_back()`, `navigate_forward()`
- `evaluate_js()` with typed results (`JSValue` enum)
- `set_size()`, `set_position()`, `set_visible()`, `focus()`
- `capture_screenshot()` returning `Image` (RGBA buffer)
- `UserContentManager` for injecting user scripts and stylesheets

**Key delegate traits:**

| Trait | Purpose | Aileron Relevance |
|-------|---------|-------------------|
| `ServoDelegate` | Top-level engine events (media session, prompts, resource interception) | Medium -- media session, auth prompts |
| `WebViewDelegate` | Per-webview events (load status, title changes, context menus, dialogs, console messages, permissions, navigation requests) | **High** -- all core browser functions |
| `RenderingContext` | GPU rendering abstraction (window, offscreen, software) | **Critical** -- compositing pipeline |
| `EventLoopWaker` | Wake embedder event loop from Servo threads | **Critical** -- thread integration |
| `ClipboardDelegate` | System clipboard access | Medium |
| `RefreshDriver` | Frame timing / vsync integration | **High** -- compositor sync |

### 1.3 Rendering Contexts

Servo provides three `RenderingContext` implementations:

| Context | Type | Description |
|---------|------|-------------|
| `WindowRenderingContext` | GPU (surfman + OpenGL) | Renders to a `raw-window-handle` window surface |
| `OffscreenRenderingContext` | GPU (surfman + OpenGL) | Renders to an offscreen surface (not shared with external wgpu) |
| `SoftwareRenderingContext` | CPU (software OpenGL) | Slow fallback; uses Mesa llvmpipe or similar |

This is a significant finding: Servo renders via **WebRender** (Mozilla's GPU renderer, v0.68) through a **surfman** OpenGL context, not directly to wgpu textures. The `OffscreenRenderingContext` creates its own OpenGL context and surface, separate from any external wgpu device. This means direct wgpu texture sharing (as envisioned in ADR-005 and BP-GFX-COMPOSITOR-001) is **not natively supported** in the current API.

### 1.4 Stability Assessment

| Factor | Rating | Notes |
|--------|--------|-------|
| API naming stability | MEDIUM | APIs renamed frequently (e.g., `constellation_sender` removed, `DebugOpts` moved to `DiagnosticsLogging`, `clear_cookies` moved to `SiteDataManager`) |
| Breaking change frequency | HIGH | Monthly releases with breaking changes expected. LTS branch reduces this. |
| Documentation quality | MEDIUM | doc.servo.org has API docs. Book is "work in progress". Embedding guide exists but is sparse. |
| Community activity | HIGH | ~100+ PRs/month, active Zulip chat, funded maintainer work (jdm), Outreachy interns |
| Real-world embedders | LOW | tauri-runtime-verso (Tauri), servo-gtk (GTK4 widget). Neither is production-grade. |

### 1.5 Known Embedding Examples

| Project | Status | Platform | Notes |
|---------|--------|----------|-------|
| servoshell | Active (demo browser) | Cross-platform | Uses winit + egui -- similar stack to Aileron |
| tauri-runtime-verso | Experimental | Desktop | Custom Tauri runtime using Servo |
| servo-gtk | Experimental | Linux (GTK4) | GTK4 web browser widget |

servoshell is the most relevant reference implementation: it already uses winit and egui, the same combination Aileron uses.

---

## 2. Rendering Pipeline

### 2.1 How Servo Renders

```
Web Content
  -> HTML/CSS parsing (html5ever, cssparser)
  -> Style resolution (Stylo -- shared with Firefox)
  -> Layout (Servo layout engine, multi-threaded)
  -> Paint (Servo paint, display list construction)
  -> WebRender (GPU renderer, v0.68)
  -> surfman (OpenGL surface management)
  -> Screen / Offscreen buffer
```

Servo uses WebRender for GPU-accelerated rendering. WebRender was originally developed for Firefox and uses OpenGL (via surfman) for rendering. It does **not** currently use wgpu for rendering output.

### 2.2 wgpu Texture Sharing Assessment

| Approach | Feasibility | Performance Impact | Complexity |
|----------|-------------|--------------------|------------|
| Direct wgpu texture sharing | **NOT AVAILABLE** | N/A | Blocked -- Servo uses OpenGL, not wgpu |
| OpenGL interop (GL texture -> wgpu texture) | POSSIBLE | ~0.5-1ms per frame (texture import) | HIGH |
| CPU readback from Servo GL surface | POSSIBLE | ~3-5ms per frame at 1080p | MEDIUM |
| Servo renders to its own window region | POSSIBLE | ~0ms (no copy) | LOW but limits compositing |
| DMA-BUF sharing (Linux) | POSSIBLE (future) | ~0.1ms per frame | VERY HIGH |

**Key finding:** ADR-005's assumption that Servo can "render directly to wgpu textures" is **incorrect** for the current API. Servo renders through OpenGL via surfman/WebRender. To composite Servo output with egui (which uses wgpu), we need one of:

1. **GL-to-wgpu interop**: Import the Servo OpenGL texture into wgpu using `wgpu::Instance::create_surface` or by sharing the underlying EGL/GL context. This is platform-specific and fragile.
2. **CPU readback**: Read the Servo GL framebuffer to CPU memory, upload to wgpu texture. This reintroduces the bottleneck ADR-005 sought to eliminate (~3-5ms vs. ~5-8ms for WebKitGTK readback -- marginal improvement).
3. **Separate Servo window layer**: Let Servo render to its own window region underneath the egui overlay. This avoids texture sharing entirely but limits compositor flexibility (e.g., no arbitrary BSP positioning of Servo panes within the egui-rendered frame).

### 2.3 Offscreen Rendering

Servo's `OffscreenRenderingContext` creates an OpenGL offscreen surface. The `capture_screenshot()` method provides CPU readback as `Image` (RGBA pixels). This is the simplest integration path but comes with the CPU readback cost.

### 2.4 Recommended Rendering Strategy

For the Q3 2026 proof-of-concept phase, use `OffscreenRenderingContext` + `capture_screenshot()` for CPU readback. This validates the embedding API, input handling, and navigation without investing in GPU interop. If the PoC is successful, invest in OpenGL-to-wgpu interop for Q4 2026.

---

## 3. Feature Completeness

### 3.1 CSS Support

Servo uses **Stylo** (shared with Firefox) for CSS parsing and style resolution. This gives it a strong CSS foundation.

| CSS Feature Area | Servo Support | WebKitGTK | Notes |
|------------------|---------------|-----------|-------|
| CSS Selectors (Level 3) | **Excellent** | Excellent | Stylo is production-proven in Firefox |
| CSS Selectors (Level 4) | Good | Good | `:has()`, `:is()`, `:where()`, nesting |
| Flexbox | Good | Excellent | Minor edge-case bugs |
| Grid | Good | Excellent | Improving rapidly |
| CSS Custom Properties | Good | Excellent | `@property` landed Feb 2026 |
| CSS Transforms/Animations | Good | Excellent | `preserve-3d` partial as of v0.0.6 |
| CSS Containment | Partial | Good | `container queries` in progress |
| Vendor prefixes | Partial | Excellent | `-moz-transform` supported (v0.0.4) |

**Assessment:** CSS support is estimated at 85-90% for developer-oriented content (docs, dashboards, code review). Complex layouts using advanced Grid features or niche CSS properties may break.

### 3.2 JavaScript Engine

Servo uses **SpiderMonkey** (Mozilla's JS engine, same as Firefox), not JavaScriptCore (WebKit). SpiderMonkey is highly conformant with modern ECMAScript.

| JS Feature | Support | Notes |
|------------|---------|-------|
| ES2024+ | Excellent | SpiderMonkey tracks latest spec |
| Modules (import/export) | Good | `import.meta.resolve()` landed Feb 2026 |
| Async/await, generators | Excellent | Full support |
| WebAssembly | Good | |
| TypedArrays, SharedArrayBuffer | Good | Shared memory uses Arc<Vec<u8>> in single-process |

### 3.3 Web API Support

| API | Servo Status | Aileron Impact |
|-----|-------------|----------------|
| DOM (Core, HTML, SVG) | Good | Essential for page rendering |
| Fetch API | Good | Aileron network interception works via `WebResourceLoad` |
| WebSocket | Good | Relative URL resolution fixed Feb 2026 |
| localStorage / sessionStorage | Good | Managed via `SiteDataManager` |
| IndexedDB | Good (improving) | Transaction conformance improved Feb 2026 |
| Cookies | Good | `SiteDataManager` provides full CRUD |
| IntersectionObserver | Good | |
| ResizeObserver | Partial | |
| Clipboard API | Good | `ClipboardDelegate` trait |
| Notification API | Good | `Notification` struct exposed to embedder |
| Permissions API | Good | `PermissionRequest` in `WebViewDelegate` |
| Geolocation | Partial (gated by pref) | |
| Media Session | Good | `MediaMetadata`, `MediaPositionState` exposed |
| Web Workers | Partial | |
| Service Workers | **Not supported** | PWA offline functionality unavailable |

### 3.4 Media Support

| Feature | Status | Notes |
|---------|--------|-------|
| HTML5 Video/Audio | Good | Player controls added Feb 2026 |
| Media Source Extensions | **Not supported** | No adaptive streaming (HLS/DASH) |
| WebCodecs | **Not supported** | |
| GStreamer backend | Optional (feature flag) | Linux media playback |

### 3.5 WebGL / WebGPU

| Feature | Status | Notes |
|---------|--------|-------|
| WebGL 1 | Good | `servo-webgl` crate |
| WebGL 2 | Partial | Memory tracking in about:memory |
| WebGPU | Partial | `servo-webgpu` crate exists but immature |

### 3.6 Accessibility

Servo started accessibility work in February 2026, gated by a preference (`accessibility_enabled`). Uses AccessKit for the accessibility tree, with egui already migrated to the new AccessKit API. This is early-stage but promising for Aileron's long-term accessibility goals.

---

## 4. Integration Complexity

### 4.1 Thread Model

Servo uses a multi-threaded architecture internally:

| Thread | Responsibility | Communicates via |
|--------|---------------|-------------------|
| Main thread (embedder) | Event loop, UI | `ServoDelegate`, `WebViewDelegate` callbacks |
| Constellation | Navigation, lifecycle management | IPC channels (crossbeam/ipc-channel) |
| Script threads | JavaScript execution, DOM | IPC channels |
| Layout threads | Style, layout | IPC channels |
| Painter | WebRender compositing | Shared memory, GL commands |

The `Servo` type and `WebView` are `!Send` (main-thread only). Servo communicates back to the embedder via delegate trait methods called on the main thread. The `EventLoopWaker` trait allows Servo to wake the embedder's event loop when it needs attention.

**Impact on Aileron:** Servo's threading model aligns well with Aileron's architecture (BP-APP-CORE-001). The main thread runs the winit event loop; Servo runs its internal threads; communication happens via delegate callbacks and the event loop waker.

### 4.2 Event Loop Integration

```rust
// Conceptual integration
impl EventLoopWaker for AileronEventLoopWaker {
    fn wake(&self) {
        // Wake the winit event loop (e.g., via winit::EventLoopProxy)
        self.proxy.wakeup().ok();
    }
}

// In the winit event loop handler:
Event::MainEventsCleared => {
    // Process Servo messages (delegate callbacks are called here)
    servo.run_ahead();
}
```

Servo provides `Servo::run_ahead()` which processes pending messages. This should be called in the winit event loop during `MainEventsCleared` or similar idle period.

### 4.3 Input Handling

Servo's `InputEvent` enum maps well to winit events:

| Servo InputEvent | winit Source |
|-----------------|-------------|
| `KeyboardEvent` | `WindowEvent::KeyboardInput`, `WindowEvent::Ime` |
| `MouseMoveEvent` | `WindowEvent::CursorMoved` |
| `MouseButtonEvent` | `WindowEvent::MouseInput` |
| `WheelEvent` | `WindowEvent::MouseWheel` |
| `TouchEvent` | `WindowEvent::Touch` |
| `CompositionEvent` | `WindowEvent::Ime` |

The `WebView::on_input_event()` method returns `InputEventResult` indicating whether the event was consumed, enabling proper input routing between egui and Servo (as described in BP-INPUT-ROUTER-001).

### 4.4 Navigation and History

| Capability | API | Status |
|-----------|-----|--------|
| Load URL | `WebView::load_url()` | Available |
| Back/Forward | `WebView::navigate_back()`, `navigate_forward()` | Available |
| Reload | `WebView::reload()` | Available |
| Stop loading | Available via internal messages | Available |
| History API (pushState) | Automatic via Script thread | Available |
| Navigation interception | `WebViewDelegate::allow_navigation()` | Available |

### 4.5 Cookie/Storage Management

| Capability | API | Status |
|-----------|-----|--------|
| Read cookies | `SiteDataManager` (in development) | Partial |
| Clear cookies | `SiteDataManager::clear_cookies()` | Available |
| localStorage | Managed internally | Available |
| sessionStorage | Managed internally | Available |
| Site data listing | `SiteDataManager::get_site_data()` | Available |
| Cache management | `NetworkManager::clear_cache()` | Available |

### 4.6 Print Support

**Not currently supported** in the Servo embedding API. No `WebView::print()` method exists. This is a gap compared to WebKitGTK which has native print support. Aileron's `:print` command would need to fall back to WebKitGTK for printing Servo-rendered panes.

### 4.7 DevTools Support

Servo has built-in DevTools server support (Chrome DevTools Protocol), exposed via `Preferences::devtools_server_listen_address`. Console messages are forwarded via `WebViewDelegate::show_console_message`. This provides a path for Aileron's F12 DevTools integration.

---

## 5. Compatibility Assessment

### 5.1 Site Compatibility Matrix

| Site Category | Expected Servo Support | Notes |
|---------------|----------------------|-------|
| Static documentation (MDN, Rust docs) | **Excellent** | CSS + basic JS, no complex interactivity |
| Developer dashboards (GitHub, GitLab) | **Good** | Modern SPAs but increasingly standard APIs |
| Code review tools (Phabricator, Gerrit) | **Good** | Server-rendered with JS enhancements |
| Terminal/web SSH (ttyd, web-console) | **Good** | WebSocket + basic DOM |
| Rich text editors (CodeMirror, Monaco) | **Moderate** | `execCommand` in progress; contentEditable limited |
| Complex SPAs (Gmail, Jira) | **Poor** | Heavy use of non-standard APIs, complex layouts |
| Video streaming (YouTube, Twitch) | **Poor** | No MSE; basic video playback only |
| Web apps (Google Docs, Figma) | **Poor** | WebGL/WebGPU gaps, complex interaction patterns |
| Internal aileron:// pages | **Full** | We control the content; design for Servo |

### 5.2 Known Incompatibilities

1. **No Service Workers** -- PWA features, offline caching, and push notifications will not work in Servo panes.
2. **No Media Source Extensions** -- Adaptive bitrate streaming (HLS/DASH) unavailable.
3. **No print support** -- Must fall back to WebKitGTK.
4. **contentEditable / execCommand** -- Still in progress; rich text editing in web apps will be limited.
5. **No WebExtensions** -- Servo has no extension API. Aileron's content scripts must be reimplemented via `UserContentManager`.
6. **CSS gaps** -- `container queries`, `subgrid`, and some advanced Grid features incomplete.
7. **OpenGL rendering** -- Cannot directly render to wgpu textures (see Section 2).

### 5.3 Performance Characteristics

| Metric | Servo (estimated) | WebKitGTK (current) | Notes |
|--------|-------------------|---------------------|-------|
| Page load (simple doc) | Fast | Fast | Servo's parallel layout is advantageous |
| Page load (complex SPA) | Moderate | Fast | JS execution is competitive, layout may be slower |
| Memory per pane | 30-80 MB | 40-100 MB | Servo has no multi-process overhead in single-process mode |
| Rendering latency | ~3-5ms (readback) | ~5-8ms (readback) | Marginal improvement with CPU readback |
| GPU rendering latency | ~0.5-1ms (interop) | N/A (no GPU sharing) | Potential with GL-to-wgpu interop |
| Binary size impact | +15-20 MB | 0 MB (system library) | Servo statically linked |
| Build time | 15-30 min (from source) | 0 min (system dep) | Significant developer friction |

---

## 6. Risk Assessment

### 6.1 Risk Matrix

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Embedding API breaking changes | **High** | Medium | Pin to LTS branch; use `WebView` trait abstraction (ADR-001) |
| GPU interop complexity exceeds estimates | **High** | High | Start with CPU readback; defer GPU interop to Q4 2026 |
| Servo rendering regressions on key sites | Medium | Medium | Fallback to WebKitGTK per ADR-002; heuristic engine selection |
| Servo build time slows development iteration | Medium | Low | Use crates.io release; cache builds; consider prebuilt binary |
| Servo memory usage higher than expected | Low | Medium | Profile with about:memory; set memory limits |
| Servo project loses funding or stalls | Low | **Critical** | Aileron's `PaneRenderer` trait abstraction (ADR-001, ADR-002) allows fallback to WebKitGTK |
| SpiderMonkey security vulnerabilities | Low | Medium | LTS releases include security patches |
| Accessibility support insufficient | Medium | Medium | Servo is actively working on AccessKit integration; gate on pref |

### 6.2 Fallback Plan

Per ADR-002, the fallback plan is well-defined:

1. Every pane can fall back to WebKitGTK via `Ctrl+E` (manual switch).
2. The `PaneRenderer` trait (`src/servo/engine.rs`) already abstracts engine selection.
3. Per-URL engine heuristics can auto-route unsupported content to WebKitGTK.
4. If Servo integration is abandoned entirely, Aileron continues to work with WebKitGTK only.

**The fallback plan is strong.** The primary risk is wasted engineering effort, not a dead-end architecture.

### 6.3 Timeline Risks

| Milestone | Target | Risk Level | Concern |
|-----------|--------|------------|---------|
| PoC (single pane, no navigation) | Q3 2026 | LOW | API is mature enough for basic embedding |
| Basic navigation + forms + CSS | Q4 2026 | MEDIUM | May need to work around API changes |
| Hybrid mode (per-pane engine selection) | Q1 2027 | MEDIUM-HIGH | GPU interop and engine switching UX are complex |
| Servo as default for dev content | Q2 2027 | HIGH | Depends on CSS/JS compatibility reaching target threshold |

---

## 7. Recommendation

### 7.1 Go/No-Go Decision

**CONDITIONAL GO** for Q3 2026 integration.

The Servo embedding API has reached a sufficient maturity level for a proof-of-concept. The v0.1.0 LTS release, combined with active development (100+ PRs/month) and the existing servoshell reference implementation (which uses winit + egui), provides a credible foundation.

**Conditions for proceeding:**
1. Accept that direct wgpu texture sharing is not currently available; plan for CPU readback in the PoC phase.
2. Pin to the LTS branch (v0.1.0) and upgrade on the LTS schedule (every 6 months).
3. Scope the PoC to static developer content (docs, README rendering, simple dashboards) where CSS/JS compatibility is strongest.
4. Maintain WebKitGTK as the primary engine; Servo is additive, not a replacement.

### 7.2 Prerequisites

| Prerequisite | Status | Action Required |
|-------------|--------|-----------------|
| PaneRenderer trait abstraction | **DONE** | `src/servo/engine.rs` -- already implemented |
| ServoPane skeleton | **DONE** | `src/servo/servo_engine.rs` -- stubs exist |
| GPU compositor design | **DONE** | BP-GFX-COMPOSITOR-001 -- may need revision for GL rendering |
| App core thread model | **DONE** | BP-APP-CORE-001 -- compatible with Servo's threading |
| Servo build environment | **NOT STARTED** | Need to verify build on target systems |
| CPU readback integration path | **NOT STARTED** | Need to implement `capture_screenshot()` -> wgpu texture upload |
| Input routing from winit to Servo | **NOT STARTED** | Need to implement winit event -> `InputEvent` translation |

### 7.3 Recommended Integration Approach

**Phase 1: Proof-of-Concept (Q3 2026, ~60-80 hours)**

1. Add `servo = "0.1.0"` to Cargo.toml (LTS branch).
2. Implement `ServoPane` with `OffscreenRenderingContext`.
3. Implement `EventLoopWaker` using winit's `EventLoopProxy`.
4. Translate winit events to Servo `InputEvent`s.
5. Use `capture_screenshot()` to get RGBA frames; upload to wgpu texture.
6. Render a single `aileron://` page or static doc URL in a Servo pane.
7. Validate: navigation, form inputs, keyboard focus, basic CSS rendering.

**Phase 2: Basic Browsing (Q4 2026, ~80-120 hours)**

1. Implement `WebViewDelegate` for load status, title changes, console messages.
2. Add cookie and storage management via `SiteDataManager`.
3. Implement per-URL engine routing heuristics (Servo for docs, WebKit for general web).
4. Add `UserContentManager` integration for Aileron's content scripts.
5. Investigate OpenGL-to-wgpu interop for GPU-accelerated compositing.
6. Benchmark: memory usage, page load time, rendering latency vs. WebKitGTK.

**Phase 3: Hybrid Mode (Q1 2027, ~100-160 hours)**

1. Implement seamless engine switching UX (per-pane, per-URL).
2. GPU interop integration (if feasible).
3. Accessibility support (if Servo's AccessKit integration matures).
4. User-facing engine indicator in status bar.
5. Configuration options for default engine per URL pattern.

### 7.4 Milestones and Checkpoints

| Checkpoint | Date | Success Criteria | Go/No-Go |
|-----------|------|-----------------|----------|
| CP-1: Servo builds in Aileron | 2026-05-15 | `cargo build` succeeds with `servo` dependency; binary < 40MB | Go: proceed to CP-2 |
| CP-2: Static page renders | 2026-06-15 | A static HTML page renders in a Servo pane at correct resolution | Go: proceed to CP-3 |
| CP-3: Input works | 2026-07-01 | Keyboard, mouse, and scroll input function correctly in Servo pane | Go: proceed to CP-4 |
| CP-4: Navigation works | 2026-07-15 | Can navigate between URLs, back/forward, reload in Servo pane | Go: proceed to Phase 2 |
| CP-5: Performance acceptable | 2026-08-01 | CPU readback < 8ms at 1080p; memory < 100MB per pane | Go: invest in GPU interop |
| CP-6: Hybrid mode viable | 2026-10-01 | Can switch engines per-pane without user-visible glitches | Go: proceed to Phase 3 |

### 7.5 Key Design Decision: Revising BP-GFX-COMPOSITOR-001

BP-GFX-COMPOSITOR-001 assumes Servo renders to wgpu textures. This evaluation reveals that Servo renders via OpenGL (surfman/WebRender). The compositor design must be revised to account for one of:

1. **CPU readback path**: Servo GL framebuffer -> CPU (capture_screenshot) -> wgpu texture upload -> composite.
2. **GL interop path**: Share the Servo OpenGL context with wgpu via platform-specific APIs (EGL/GLX interop on Linux).
3. **Hybrid path**: Servo renders to its own window region; egui renders as a transparent overlay on top.

Option 1 is the simplest and should be used for the PoC. Option 2 is the long-term target but requires significant platform-specific work. Option 3 is a potential intermediate step that avoids texture copying entirely but limits compositing flexibility.

---

## Appendix A: Servo Crate Dependency Tree (Key Crates)

| Crate | Version | Purpose |
|-------|---------|---------|
| servo | 0.1.0 | Top-level embedding crate |
| servo-embedder-traits | 0.1.0 | Embedding trait definitions |
| webrender | 0.68 | GPU rendering engine |
| webrender_api | 0.68 | WebRender public API |
| surfman | 0.11.0 | OpenGL surface management |
| stylo | 0.15.0 | CSS styling engine (Firefox-derived) |
| servo-script | 0.1.0 | JavaScript/DOM integration |
| servo-layout | 0.1.0 | Layout engine |
| servo-net | 0.1.0 | Network stack |
| servo-media | 0.1.0 | Audio/video playback |
| servo-storage | 0.1.0 | localStorage, sessionStorage, IndexedDB |
| servo-webgl | 0.1.0 | WebGL support |
| servo-webgpu | 0.1.0 | WebGPU support |
| servo-devtools | 0.1.0 | DevTools protocol |
| accesskit | 0.24.0 | Accessibility tree |

## Appendix B: ADR Cross-References

| ADR | Title | Relevance to This Evaluation |
|-----|-------|------------------------------|
| ADR-001 | Servo Embedder API Stability Risk | Confirmed: API is still changing. LTS + trait abstraction is the correct mitigation. |
| ADR-002 | Multi-Engine Architecture | Confirmed: WebViewDelegate + PaneRenderer trait enables dual-engine. |
| ADR-005 | Architecture D Hybrid Servo+WebKitGTK | Needs revision: Servo does not render directly to wgpu textures. |
