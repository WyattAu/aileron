# ADR-003: Offscreen Webview Rendering via wgpu Textures

## Status
Proposed

## Date
2026-04-16

## Context

Aileron embeds web views (wry/WebKitGTK) into a winit window using either:
- `build_as_child` — X11 only, requires `Xlib` window handle
- `build_gtk` with a standalone `gtk::Window` — creates a separate Wayland surface

Both approaches have critical flaws:
1. **`build_as_child`** fails on Wayland (winit provides `WaylandWindowHandle`, wry needs `XlibWindowHandle`)
2. **Standalone `gtk::Window`** causes `Gdk Error 71 (Protocol error)` — two top-level Wayland surfaces from different toolkits conflict
3. **Current workaround** (XWayland) forces `WAYLAND_DISPLAY` removal + `GDK_BACKEND=x11` + `XSetErrorHandler` for `GLXBadWindow` — fragile, three band-aids on a fundamental toolkit mismatch
4. **XWayland** loses native Wayland features (per-monitor DPI, idle inhibit, etc.)

Every crash fixed in v0.2.0 was a symptom of the winit+GTK boundary. This is not sustainable.

## Decision

**Render web views offscreen via `gtk::OffscreenWindow`, capture frames as pixel data, upload to wgpu textures, and display in egui.**

Architecture:
```
winit Window (native Wayland/X11, no hacks)
└── wgpu Surface → egui (full control of visual space)
    ├── Chrome panels (tabs, palette, status bar, URL bar) — native egui widgets
    ├── Pane 1: egui::Image(webview_texture_1)  ← OffscreenWindow + wry build_gtk
    ├── Pane 2: egui::Image(webview_texture_2)  ← OffscreenWindow + wry build_gtk
    └── Pane 3: egui::Image(webview_texture_3)  ← OffscreenWindow + wry build_gtk
```

Key insight: `wry::WebViewBuilder::build_gtk(&offscreen)` works because `gtk::OffscreenWindow` implements `gtk::Container`. The webview renders to the offscreen buffer instead of to screen. We read pixels from `offscreen.pixbuf()` and upload to wgpu.

## Technical Details

### Frame Capture
```rust
let offscreen = gtk::OffscreenWindow::new();
offscreen.set_default_size(width, height);
let webview = WebViewBuilder::new().with_url("...").build_gtk(&offscreen)?;
offscreen.show_all();

// Each frame (after pumping GTK event loop):
let pixbuf = offscreen.pixbuf()?;  // gdk::Pixbuf with BGRA pixel data
let pixels = pixbuf.pixels();      // &[u8] — raw pixel data
```

### wgpu Texture Upload
```rust
let texture = device.create_texture(&wgpu::TextureDescriptor {
    format: wgpu::TextureFormat::Bgra8UnormSrgb,  // Matches GTK's pixel format
    size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
    ..Default::default()
});

// Each frame:
queue.write_texture(&wgpu::ImageCopyTexture { texture: &texture, .. }, &pixels, rowstride, size);
```

### Input Forwarding (via JavaScript)
```javascript
// Mouse click at (x, y) relative to webview
document.elementFromPoint(x, y)?.dispatchEvent(new MouseEvent('click', { clientX: x, clientY: y }));

// Keyboard
document.dispatchEvent(new KeyboardEvent('keydown', { key: 'a', code: 'KeyA' }));

// Scroll
window.scrollBy(deltaX, deltaY);
```

### What Stays the Same
- `wry::WebView` — all management APIs (navigation, scripts, IPC, custom protocols) work identically
- `gtk::init()` — still needed for WebKitGTK rendering
- GTK event loop pump — still needed for webview rendering (not windowing)
- BSP tiling logic — unchanged
- egui UI code — unchanged (just add Image widgets for webview textures)

### What Gets Removed
- `build_as_child` X11 path
- Standalone `gtk::Window` fallback
- `WAYLAND_DISPLAY` removal hack
- `GDK_BACKEND=x11` override
- `XSetErrorHandler` for `GLXBadWindow`
- `x11-dl` dependency
- wry `x11` feature flag

### What Gets Added
- `OffscreenWebView` struct (offscreen window + wry webview + wgpu texture)
- Frame capture pipeline (GTK → pixbuf → wgpu texture → egui Image)
- Input forwarding (egui events → JavaScript dispatchEvent)

## Consequences

### Positive
- **Single toolkit** (winit) for windowing — no GTK/winit conflict
- **Native Wayland** — no XWayland, full DPI, proper input
- **Webview engine pluggable** — swap WebKitGTK for Servo (ADR-002, Oct 2026) without touching windowing
- **Process isolation** (future) — webviews can run in subprocesses
- **Pixel-perfect tiling** — egui controls exact positioning, no window positioning hacks
- **Eliminates an entire class of bugs** — no X11 protocol errors, no GLXBadWindow, no GTK window management

### Negative
- **Frame capture overhead** — pixel readback + wgpu upload per pane per frame (~2MB @ 800×600)
- **Input forwarding complexity** — mouse/keyboard/scroll/IME must be manually forwarded via JS
- **No native webview input** — text selection, right-click context menus, drag-and-drop require reimplementation
- **Software rendering** — `get_pixbuf()` may not capture hardware-accelerated WebKitGTK content; may need to disable WebKit hardware acceleration initially

### Performance Mitigations
1. **Dirty flag** — only capture frames when web content changes (load-changed signal)
2. **Frame rate limiting** — capture at 30fps, not 60fps
3. **Texture sharing** (future) — use EGL/GL interop to share textures directly, avoiding CPU readback
4. **Resize throttling** — only resize offscreen window on BSP layout change, not every frame

## Migration Plan

1. **Prototype**: Create `OffscreenWebView` struct, verify frame capture + wgpu upload
2. **Integrate**: Replace `WryPane` visible windows with `OffscreenWebView` in main render loop
3. **Input**: Implement mouse/keyboard/scroll forwarding from egui to webviews
4. **Clean up**: Remove all X11/GTK hacks, simplify Cargo.toml
5. **Optimize**: Dirty flags, frame rate limiting, texture caching
6. **Test**: Full integration on KDE Plasma Wayland

## Related ADRs
- ADR-001: Servo embedder API risk
- ADR-002: Servo revisit criteria (October 2026)
