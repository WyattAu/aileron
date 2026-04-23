# NVIDIA + Wayland Compatibility Audit

**Target:** CachyOS, Wayland, NVIDIA RTX 2060, proprietary drivers
**Date:** 2026-04-23
**Scope:** Pre-deployment audit of Aileron v0.12.0

---

## Issue Summary

| Severity | Count |
|----------|-------|
| Critical | 2 |
| High     | 5 |
| Medium   | 6 |
| Low      | 4 |
| **Total**| **17** |

---

## CRITICAL Issues

### C1. Vulkan-only backend with no fallback — GPU init may fail entirely

- **File:** `src/gfx/renderer.rs:20-21`
- **Code:**
  ```rust
  let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
      backends: wgpu::Backends::VULKAN,
      ..Default::default()
  });
  ```
- **Issue:** Only Vulkan is requested. On NVIDIA + Wayland, the Vulkan WSI layer must support `VK_KHR_wayland_surface`. If the NVIDIA driver or Vulkan ICD is misconfigured, partially installed, or the Wayland compositor doesn't advertise the required Vulkan extension, `request_adapter` returns `None` and the app fails immediately with no fallback path. CachyOS typically ships `nvidia` + `nvidia-utils` with Vulkan, but the ICD JSON must be present at `/usr/share/vulkan/icd.d/nvidia_icd.json`.
- **Severity:** Critical
- **Fix:** Add `wgpu::Backends::VULKAN | wgpu::Backends::GL` as a fallback. Or at minimum, detect adapter failure early and provide actionable diagnostics (the existing error message at line 39-45 is good but only fires after failure, not as a proactive check).

### C2. `WEBKIT_DISABLE_COMPOSITING_MODE=1` forces software rendering — severe performance degradation

- **File:** `src/main.rs:1979-1988`
- **Code:**
  ```rust
  std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
  ```
- **Issue:** This env var disables WebKitGTK's threaded compositor, forcing all web content through the cairo software rendering path. Every frame of every webview is rendered by the CPU, then captured via `snapshot()`, BGRA-to-RGBA converted, and uploaded as a GPU texture. On an RTX 2060 this is catastrophic for performance — complex pages (YouTube, heavy SPAs) will be sluggish and CPU-bound. The original reason (pixbuf capture not seeing GL content) has been superseded by the `snapshot()` API which *does* capture GL-composited content correctly (see `capture_frame_snapshot` at `src/offscreen_webview.rs:393`). The env var is now counterproductive.
- **Severity:** Critical
- **Fix:** Remove `WEBKIT_DISABLE_COMPOSITING_MODE=1`. The `snapshot()` API in WebKitGTK 2.38+ (which is required via `Cargo.toml:115` features `v2_38`) correctly captures GL-composited content. The software compositing mode was needed only for the old `pixbuf()` capture path which is now the fallback for `aileron://` pages only. Verify that `capture_frame_snapshot()` still works with the threaded compositor enabled.

---

## HIGH Issues

### H1. `wry` feature `x11` enabled unconditionally — no Wayland-native webview support

- **File:** `Cargo.toml:88-93`
- **Code:**
  ```toml
  wry = { version = "0.55.0", default-features = false, features = [
     "os-webview", "protocol", "devtools", "x11",
  ] }
  ```
- **Issue:** The `x11` feature is hardcoded. There is no `wayland` feature flag in the wry dependency. This means wry will always use X11/XWayland for webview embedding. On NVIDIA + Wayland, this forces all webview child windows through XWayland, which adds latency, limits compositor integration (no Wayland-native subsurfaces), and can cause visual glitches with multi-GPU or mixed-DPI setups.
- **Severity:** High
- **Fix:** This is partly a wry limitation — wry's Linux backend (webkitgtk) does not have a pure Wayland embedding mode. The `GDK_BACKEND=x11` workaround at `src/main.rs:1958-1967` is already in place. The offscreen Architecture B path (`src/offscreen_webview.rs`) avoids this entirely. Consider making `render_mode = "offscreen"` the default on Wayland systems.

### H2. `GDK_BACKEND=x11` forced on Wayland — GTK/XWayland conflict potential

- **File:** `src/main.rs:1958-1967`
- **Code:**
  ```rust
  if std::env::var("WAYLAND_DISPLAY").is_ok() {
      unsafe { std::env::set_var("GDK_BACKEND", "x11"); }
  }
  ```
- **Issue:** This forces all GTK operations (including wry's webview creation) through XWayland. While necessary for NVIDIA GL context creation, it creates a split display stack: winit uses Wayland directly (via `WAYLAND_DISPLAY` which is NOT unset per comment at line 1961), but GTK/WebKitGTK use XWayland. This can cause:
  - Window positioning drift (Wayland compositor coordinates vs X11 coordinates)
  - Input focus conflicts between Wayland and XWayland windows
  - DPI scaling mismatches between the winit window and GTK child windows
- **Severity:** High
- **Fix:** If Architecture B (offscreen) is used, `GDK_BACKEND=x11` may still be needed for the GL context. But document that this creates a hybrid stack. Ideally, test with `GDK_BACKEND=wayland` on NVIDIA + recent drivers (525+) which have improved GBM/EGL support. If offscreen mode is default on Wayland, the GTK window placement issue is moot since there are no visible GTK windows.

### H3. `alpha_mode` may cause transparency issues on NVIDIA Wayland

- **File:** `src/gfx/renderer.rs:82`
- **Code:**
  ```rust
  alpha_mode: surface_capabilities.alpha_modes[0],
  ```
- **Issue:** The first alpha mode from surface capabilities is used blindly. On NVIDIA + Wayland, some compositors (e.g., KWin with NVIDIA) may report `PreMultiplied` or `PostMultiplied` as the first option, which can cause visual artifacts (black background bleeding through, or incorrect alpha compositing). The `resize()` method at line 119 uses `Auto` which may resolve differently than the initial configure, causing a mismatch.
- **Severity:** High
- **Fix:** Explicitly prefer `Opaque` alpha mode when transparency is not needed (which it isn't — the window has a solid dark background). Use:
  ```rust
  alpha_mode: surface_capabilities.alpha_modes.iter()
      .find(|m| **m == wgpu::CompositeAlphaMode::Opaque)
      .copied()
      .unwrap_or(surface_capabilities.alpha_modes[0]),
  ```
  Also align the `resize()` alpha_mode to match.

### H4. GTK fallback window (Wayland mode) is unanchored — compositor controls position

- **File:** `src/servo/wry_engine.rs:159-214`
- **Code:**
  ```rust
  fn create_gtk_pane(...) -> Result<Self, wry::Error> {
      let gtk_window = gtk::Window::new(gtk::WindowType::Toplevel);
      gtk_window.set_decorated(false);
      gtk_window.show();
      ...
  }
  ```
- **Issue:** When `build_as_child` fails (which it will on pure Wayland since it requires X11), the fallback creates a `gtk::Window::Toplevel` with `set_decorated(false)`. On Wayland, the compositor controls window positioning — there is no way to anchor this window to the winit parent. The webview will appear as a separate floating window that the compositor may tile, overlap, or place unpredictably. This is not just cosmetic — it breaks the entire multi-pane tiling layout since each pane becomes an independent window.
- **Severity:** High
- **Fix:** Architecture B (offscreen mode) is the correct solution for Wayland. Ensure `render_mode = "offscreen"` is the default when `is_wayland()` returns true. Document that `render_mode = "native"` on Wayland uses a broken fallback path. Consider logging a warning when native mode is used on Wayland.

### H5. Texture capture pipeline is CPU-intensive — no GPU-direct path

- **File:** `src/main.rs:718-786`, `src/offscreen_webview.rs:362-484`
- **Issue:** Every frame of every dirty offscreen pane goes through:
  1. WebKitGTK renders to GL texture
  2. `snapshot()` copies to a cairo ImageSurface (GPU → CPU readback)
  3. `bgra_to_rgba()` copies and swizzles pixel data
  4. `egui::ColorImage::from_rgba_unmultiplied()` allocates and converts
  5. `egui_wgpu::Renderer::update_texture()` uploads CPU data to GPU texture

  This is 3 full copies of the pixel data (one in snapshot, one in bgra_to_rgba, one in egui upload) plus a GPU→CPU→GPU roundtrip per pane per frame. At 1920x1080, that's ~8MB per pane per frame. With multiple panes, this creates significant CPU and memory bandwidth pressure.
- **Severity:** High
- **Fix:** Short-term: reuse `TextureHandle` (already done at line 767, good). Medium-term: if `WEBKIT_DISABLE_COMPOSITING_MODE` is removed (see C2), snapshot() may return surfaces that can be more efficiently uploaded. Long-term: implement DMA-BUF sharing (`ShareStrategy::DmaBuf` stubs exist at `src/servo/texture_share.rs:10`) to avoid the CPU roundtrip entirely.

---

## MEDIUM Issues

### M1. Clipboard uses `wl-copy` / `xclip` / `xsel` external processes — fragile

- **File:** `src/platform/linux.rs:117-145`
- **Code:**
  ```rust
  std::process::Command::new("wl-copy")
      .arg(text)...
  ```
- **Issue:** Clipboard operations shell out to external processes. On CachyOS, `wl-copy` (from `wl-clipboard`) may or may not be installed. The fallback to `xclip`/`xsel` works on XWayland but not on pure Wayland sessions. Additionally, shelling out per-clipboard-operation is slow and creates race conditions (process may not complete before the next clipboard request). `xclip` is called with the text as an argument rather than via stdin, which will fail for multi-line or special-character content.
- **Severity:** Medium
- **Fix:** Use the `arboard` or `copypasta` crate which uses Wayland's native `wl_data_device_manager` protocol directly. Alternatively, pipe clipboard content via stdin: `xclip -selection clipboard` reads from stdin by default. Also add `wl-paste` for clipboard reading (currently no `clipboard_paste` method exists in the traits).
- **Note:** `xclip` at line 129 passes text as `.arg(text)` — this should be `.stdin(Stdio::piped())` with `write_all()` to handle newlines and special chars.

### M2. No clipboard *read* implementation

- **File:** `src/platform/traits.rs:21-22`
- **Code:**
  ```rust
  fn clipboard_copy(&self, text: &str) -> bool;
  ```
- **Issue:** The platform trait only has `clipboard_copy` — there is no `clipboard_paste` or `clipboard_read`. The app dispatch at `src/app/dispatch.rs:128` uses `navigator.clipboard.readText()` via JS injection, which requires the webview to have focus and clipboard permission. This won't work in offscreen mode where the webview is never focused.
- **Severity:** Medium
- **Fix:** Add `clipboard_paste(&self) -> Option<String>` to `PlatformOps` and implement using `wl-paste` on Wayland.

### M3. `desired_maximum_frame_latency: 2` may cause frame queuing on NVIDIA

- **File:** `src/gfx/renderer.rs:84, 121`
- **Code:**
  ```rust
  desired_maximum_frame_latency: 2,
  ```
- **Issue:** A latency of 2 allows the GPU to queue up to 2 frames ahead. On NVIDIA + Wayland, this can cause noticeable input lag because the swapchain may present frames that are 2 vsync intervals old. Combined with the CPU-bound texture capture pipeline (H5), this compounds the perceived latency.
- **Severity:** Medium
- **Fix:** Set to `1` for lower latency. If frame drops occur, the adaptive quality system (`AdaptiveQuality`) should handle reducing capture rate rather than buffering frames.

### M4. `wgpu::Limits::default()` may be too restrictive or too permissive

- **File:** `src/gfx/renderer.rs:56`
- **Code:**
  ```rust
  required_limits: wgpu::Limits::default(),
  ```
- **Issue:** Default limits use conservative values. The RTX 2060 supports 16GB VRAM and can handle much larger buffer sizes. Conversely, `max_texture_dimension_2d` from the default limits may be lower than what the GPU supports, unnecessarily limiting texture sizes. The actual GPU limit is queried and stored at `src/main.rs:200`, but the device is created with default limits which may not match the RTX 2060's capabilities.
- **Severity:** Medium
- **Fix:** Use `adapter.limits()` (the adapter's actual limits) instead of `wgpu::Limits::default()` for maximum capability utilization. Or at least increase `max_texture_size_2d` and `max_buffer_size` to known RTX 2060 values.

### M5. X11 error handler only swallows error code 170 (GLXBadWindow)

- **File:** `src/main.rs:32-46`
- **Code:**
  ```rust
  if error.error_code == 170 {
      return 0; // Swallow GLXBadWindow
  }
  ```
- **Issue:** On NVIDIA + XWayland, other X11 errors are possible: `GLXBadDrawable` (error 169), `BadWindow` (error 3), and NVIDIA-specific error codes. The handler returns 1 (triggering Xlib's default handler which may abort) for any non-170 error. This is fragile — a different X11 error could crash the app.
- **Severity:** Medium
- **Fix:** Log all X11 errors via tracing but only return 0 (swallow) for known benign errors. For unknown errors, still return 0 but log at `error!` level. Never let Xlib's default handler run — it calls `abort()`.

### M6. `WEBKIT_DISABLE_DMABUF_RENDERER=1` is set unconditionally, not just on NVIDIA

- **File:** `src/main.rs:1970-1977`
- **Code:**
  ```rust
  #[cfg(target_os = "linux")]
  unsafe {
      std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
  }
  ```
- **Issue:** This disables DMA-BUF rendering for all Linux systems, not just NVIDIA. On AMD/Intel GPUs, DMA-BUF is the preferred rendering path and disabling it forces the shared GL texture path which is slower. The comment says "on NVIDIA" but the code doesn't check for NVIDIA.
- **Severity:** Medium
- **Fix:** Only set this env var when an NVIDIA GPU is detected. Check via `VK_ICD_FILENAMES`, `lsmod | grep nvidia`, or parse `/sys/class/drm/card*/device/vendor` for `0x10de`.

---

## LOW Issues

### L1. `is_wayland()` detection can false-positive in SSH sessions

- **File:** `src/platform/linux.rs:24-29`
- **Code:**
  ```rust
  fn is_wayland(&self) -> bool {
      std::env::var("WAYLAND_DISPLAY").is_ok()
          || std::env::var("XDG_SESSION_TYPE").map(|s| s == "wayland").unwrap_or(false)
  }
  ```
- **Issue:** If running over SSH with X11 forwarding, `XDG_SESSION_TYPE` may still say "wayland" from the local session. The `WAYLAND_DISPLAY` check alone is more reliable.
- **Severity:** Low
- **Fix:** Rely solely on `WAYLAND_DISPLAY` being set AND the actual Wayland socket being connectable. For Aileron's purposes this is minor since it won't be used over SSH.

### L2. No `__NV_PRIME_RENDER_OFFLOAD` or `__GLX_VENDOR_LIBRARY_NAME` handling

- **File:** N/A (missing)
- **Issue:** On hybrid GPU laptops (NVIDIA + Intel), the app may need `__NV_PRIME_RENDER_OFFLOAD=1` and `__GLX_VENDOR_LIBRARY_NAME=nvidia` to use the NVIDIA GPU. Aileron doesn't set or detect these.
- **Severity:** Low (not applicable for RTX 2060 desktop)
- **Fix:** Document in README. Optionally detect via `nvidia-smi` and set automatically.

### L3. `file_open_dialog` checks `DISPLAY` but not Wayland

- **File:** `src/platform/linux.rs:58`
- **Code:**
  ```rust
  if std::env::var("DISPLAY").is_err() && std::env::var("WAYLAND_DISPLAY").is_err() {
      return None;
  }
  ```
- **Issue:** This correctly checks both `DISPLAY` and `WAYLAND_DISPLAY`. However, on a system where `GDK_BACKEND=x11` is forced (see H2), `DISPLAY` must also be set (from XWayland). If XWayland is not running, the dialog will fail silently.
- **Severity:** Low
- **Fix:** The check is already correct. Just ensure XWayland is available (it should be on any standard Wayland compositor).

### L4. `notify-send` for notifications may not work on all Wayland compositors

- **File:** `src/platform/linux.rs:105-107`
- **Code:**
  ```rust
  fn show_notification(&self, title: &str, body: &str) {
      let _ = Command::new("notify-send").arg(title).arg(body).spawn();
  }
  ```
- **Issue:** `notify-send` uses D-Bus which works on most compositors but may require `libnotify` and a notification daemon. On minimal CachyOS installs, this may silently fail.
- **Severity:** Low
- **Fix:** Consider using the `zbus` crate for direct D-Bus notification (org.freedesktop.Notifications). Low priority.

---

## Already Mitigated (Existing Workarounds)

The following issues are already handled by existing code:

1. **DMA-BUF crash** — `WEBKIT_DISABLE_DMABUF_RENDERER=1` at `src/main.rs:1976` prevents GBM buffer failures on NVIDIA.
2. **GLXBadWindow crash** — Custom X11 error handler at `src/main.rs:32-46` swallows error code 170.
3. **WebKitGTK SIGTRAP** — GLib log handler at `src/servo/wry_engine.rs:834-888` intercepts fatal WebKitGTK messages.
4. **Offscreen rendering** — Architecture B (`src/offscreen_webview.rs`) avoids the winit+GTK toolkit conflict entirely.
5. **GTK init** — `init_gtk()` at `src/servo/wry_engine.rs:782` is called before any WebView creation.
6. **GPU diagnostics** — `bootstrap::log_environment()` at `src/bootstrap.rs:42` logs Vulkan/GLX/ICD info on startup.
7. **Adaptive quality** — `AdaptiveQuality` at `src/main.rs:120` reduces texture capture rate under load.

---

## Recommended Priority Actions

1. **[C2]** Remove `WEBKIT_DISABLE_COMPOSITING_MODE=1` — test that `snapshot()` works with threaded compositor
2. **[C1]** Add GL backend fallback to `wgpu::Backends`
3. **[H3]** Use `Opaque` alpha mode explicitly
4. **[H4]** Default to offscreen render mode on Wayland
5. **[M6]** Make `WEBKIT_DISABLE_DMABUF_RENDERER=1` conditional on NVIDIA detection
6. **[M1]** Fix `xclip` usage to pipe via stdin
7. **[H5]** Investigate DMA-BUF texture sharing for GPU-direct path
