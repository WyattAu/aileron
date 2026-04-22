# T01: Mobile Architecture Design

**Date:** 2026-04-22
**Author:** Aileron Architecture Team
**Status:** DRAFT
**Related:** BP-APP-CORE-001, ADR-001, master_plan_v3.toml (Phase T)

---

## 1. Problem Statement

Aileron's primary target is desktop (Linux, Windows, macOS) with keyboard-driven tiling. Users want access to their Aileron workspace from mobile devices — viewing tabs, using the terminal, and managing downloads while away from their desk.

### Constraints

- Mobile devices have limited screen real estate (phone: ~6", tablet: ~10")
- Touch input is fundamentally different from keyboard-driven workflows
- iOS restricts background execution, terminal access, and dynamic code loading
- Android allows more flexibility but has battery/constraint concerns
- Aileron's rendering pipeline (egui + WebKitGTK/wry) cannot run directly on mobile
- Servo may eventually ship mobile binaries (Android aarch64 already exists)

### Goals

1. **Read-only browsing** of desktop Aileron tabs from mobile
2. **Terminal access** with virtual keyboard support
3. **Tab management** (switch, close, open new URLs)
4. **Download management** (view progress, open completed files)
5. **Clipboard sync** between desktop and mobile
6. **Notifications** for download completion, tab events

### Non-Goals

- Full Aileron editing experience on mobile (keyboard-driven workflows don't translate)
- Running the Aileron rendering engine on mobile (future consideration via Servo)
- Offline mode (mobile is always a client of the desktop)
- Independent browsing without desktop connection

---

## 2. Architecture Decision: Aileron Remote Protocol (ARP)

### 2.1 Decision

**Option A: Thin WebSocket client** connecting to desktop Aileron via the Aileron Remote Protocol (ARP).

### 2.2 Alternatives Considered

| Option | Description | Pros | Cons | Verdict |
|--------|-------------|------|------|---------|
| **A: ARP WebSocket** | Mobile connects to desktop via JSON-RPC over WebSocket | Minimal mobile code, reuses desktop logic, no duplication | Requires desktop running, network dependency | **SELECTED** |
| B: Native mobile app | Full Aileron port to mobile with different UX | Works offline, native feel | Massive duplication, touch UX fundamentally different | Rejected |
| C: Android-only Termux | Run Aileron inside Termux on Android | No new code | Poor UX, no iOS, limited audience | Rejected |
| D: VNC/RDP viewer | Use existing remote desktop protocols | Standard, no custom protocol | No tab awareness, high bandwidth, poor mobile UX | Rejected |
| E: SSH tunnel + port forward | Forward Aileron's web UI over SSH | Secure, established | Complex setup, no native mobile feel | Rejected |

### 2.3 Rationale

Option A (ARP) provides the best developer experience:
- Minimal mobile codebase (WebSocket client + UI)
- All complex logic runs on desktop where it already works
- Natural separation of concerns
- Enables both Android and iOS from the same protocol
- Bandwidth-efficient (JSON-RPC, compressed screenshots)
- Aileron's terminal content can be streamed efficiently as text

---

## 3. Aileron Remote Protocol (ARP) Overview

### 3.1 Transport Layer

```
Mobile Client  <--WebSocket/TLS-->  ARP Server (Desktop Aileron)
     |                                    |
     |  JSON-RPC 2.0 requests             |
     |  <-------------------------------- |
     |                                    |
     |  JSON-RPC 2.0 responses            |
     |  --------------------------------> |
     |                                    |
     |  Server-push notifications         |
     |  --------------------------------> |
```

**Protocol:** JSON-RPC 2.0 over WebSocket (RFC 7118 conceptually, custom implementation)
**Security:** TLS 1.3 with self-signed certificate + token authentication
**Compression:** Per-message gzip for screenshots, raw for small messages
**Binary data:** Screenshots transmitted as base64-encoded JPEG (quality 70%, max 800px wide)

### 3.2 Authentication

```
1. Desktop generates auth token on first ARP server start
2. Token displayed as QR code in Aileron settings
3. Mobile scans QR code or enters token manually
4. WebSocket upgrade includes Authorization: Bearer <token>
5. Token is persisted on both sides for auto-reconnect
6. Token can be rotated from desktop settings
```

### 3.3 Connection Lifecycle

```
Mobile                           Desktop
  |                                  |
  |--- WebSocket Connect ----------->|
  |--- authorize(token) ------------>|
  |<-- { result: "ok", session_id }--|
  |                                  |
  |--- subscribe(events) ----------->|
  |<-- tab:created event ------------|
  |<-- tab:updated event ------------|
  |                                  |
  |--- tabs.list() ----------------->|
  |<-- [{ id, url, title }, ...] ----|
  |                                  |
  |--- tab.screenshot(id) --------->|
  |<-- { base64_jpeg: "..." } ------|
  |                                  |
  |--- terminal.input(id, "ls\n") ->|
  |<-- terminal.output stream ------|
  |                                  |
  |--- WebSocket Close ------------->|
  |                                  |
```

---

## 4. API Specification (Summary)

### 4.1 Tab Operations

| Method | Description | Returns |
|--------|-------------|---------|
| `tabs.list()` | List all tabs | `[{ id, url, title, active }]` |
| `tabs.get(id)` | Get tab details | `{ id, url, title, favicon, loading }` |
| `tabs.activate(id)` | Switch to tab | `{ success: true }` |
| `tabs.create(url)` | Open new tab | `{ id }` |
| `tabs.close(id)` | Close tab | `{ success: true }` |
| `tabs.navigate(id, url)` | Navigate tab | `{ success: true }` |
| `tabs.screenshot(id)` | Capture tab (JPEG) | `{ base64_jpeg, width, height }` |
| `tabs.scroll(id, direction, amount)` | Scroll tab | `{ success: true }` |

### 4.2 Terminal Operations

| Method | Description | Returns |
|--------|-------------|---------|
| `terminal.list()` | List terminal panes | `[{ id, title }]` |
| `terminal.input(id, text)` | Send text to terminal | `{ success: true }` |
| `terminal.resize(id, cols, rows)` | Resize terminal | `{ success: true }` |
| `terminal.subscribe(id)` | Stream terminal output | Push: `terminal.output` events |

### 4.3 Download Operations

| Method | Description | Returns |
|--------|-------------|---------|
| `downloads.list()` | List recent downloads | `[{ id, url, filename, state, progress }]` |
| `downloads.cancel(id)` | Cancel download | `{ success: true }` |
| `downloads.pause(id)` | Pause download | `{ success: true }` |
| `downloads.resume(id)` | Resume download | `{ success: true }` |

### 4.4 Clipboard Operations

| Method | Description | Returns |
|--------|-------------|---------|
| `clipboard.get()` | Get desktop clipboard | `{ text }` |
| `clipboard.set(text)` | Set desktop clipboard | `{ success: true }` |

### 4.5 System Operations

| Method | Description | Returns |
|--------|-------------|---------|
| `system.info()` | Desktop system info | `{ os, version, active_tabs, uptime }` |
| `system.subscribe()` | Subscribe to events | Push: `tab:*`, `download:*`, `terminal:*` events |

### 4.6 Server-Push Events

| Event | Triggered When | Payload |
|-------|---------------|---------|
| `tab.created` | New tab opened | `{ id, url, title }` |
| `tab.updated` | Tab title/URL changed | `{ id, url, title }` |
| `tab.closed` | Tab closed | `{ id }` |
| `tab.activated` | Tab switched | `{ id }` |
| `download.started` | Download begins | `{ id, url, filename }` |
| `download.progress` | Download progress update | `{ id, progress, speed, eta }` |
| `download.completed` | Download finished | `{ id, dest_path }` |
| `download.failed` | Download error | `{ id, error }` |
| `terminal.output` | Terminal data available | `{ id, data }` |
| `notification` | Desktop notification | `{ title, body }` |

---

## 5. ARP Server Implementation (Desktop Side)

### 5.1 Architecture

```
AppState
   |
   +-- ARP Server (tokio task)
        |
        +-- WebSocket Listener (configurable port, default 19743)
        |
        +-- Session Manager
        |     +-- Auth token validation
        |     +-- Session registry (max 5 concurrent clients)
        |     +-- Rate limiting (100 req/min per session)
        |
        +-- RPC Dispatcher
        |     +-- tabs.* -> AppState.wm + engines
        |     +-- terminal.* -> AppState.terminal_manager
        |     +-- downloads.* -> AppState.download_manager
        |     +-- clipboard.* -> platform().clipboard_copy/get
        |     +-- system.* -> AppState metadata
        |
        +-- Event Bus
              +-- Subscribes to AppState events
              +-- Pushes to all connected sessions
              +-- Debounces rapid tab updates (100ms)
```

### 5.2 Integration Points

The ARP server integrates with existing AppState methods:
- `wm.list_panes()` → `tabs.list()`
- `wm.active_pane_id()` → `tabs.get()` active state
- `engines.get(id).current_url()` → `tabs.get()` URL
- `engines.get(id).eval_js("document.title")` → `tabs.get()` title
- `offscreen_webview.capture_screenshot()` → `tabs.screenshot()`
- `terminal_manager.input(id, text)` → `terminal.input()`
- `download_manager.progress_all()` → `downloads.list()`

### 5.3 Configuration

```toml
[remote]
enabled = false
port = 19743
auto_discover = false  # mDNS/bonjour
max_clients = 5
screenshot_quality = 70  # JPEG quality %
screenshot_max_width = 800
rate_limit_per_minute = 100
```

### 5.4 Dependencies

```toml
# ARP server
tokio-tungstenite = "0.26"  # WebSocket server
rustls = "0.23"             # TLS
jsonwebtoken = "9"          # JWT tokens (optional, simple token sufficient)
qrcode = "0.14"             # QR code generation for pairing
base64 = "0.22"             # Screenshot encoding
```

---

## 6. Mobile Client Architecture

### 6.1 Technology Choices

| Platform | Framework | Rationale |
|----------|-----------|-----------|
| Android | Kotlin + Jetpack Compose | Native UI, best performance, F-Droid compatible |
| iOS | Swift + SwiftUI | Native UI, App Store compatible |
| Cross-platform (future) | Rust + egui-mobile | If demand justifies single codebase |

### 6.2 Android Client Structure

```
app/
  src/main/java/com/aileron/mobile/
    MainActivity.kt          -- Entry point, permissions
    AileronService.kt        -- Background WebSocket connection
    ui/
      TabCarouselScreen.kt   -- Horizontal swipe tab list
      TabViewScreen.kt       -- Tab content (WebView or image)
      TerminalScreen.kt      -- Terminal with virtual keyboard
      DownloadsScreen.kt     -- Download list
      SettingsScreen.kt      -- Server address, auth token
      QRScannerScreen.kt     -- Camera QR code scanning
    network/
      ArpClient.kt           -- WebSocket + JSON-RPC client
      ArpMessage.kt          -- Message types
      ScreenshotDecoder.kt   -- Base64 JPEG → Bitmap
    viewmodel/
      TabsViewModel.kt       -- Tab state management
      TerminalViewModel.kt   -- Terminal state management
```

### 6.3 iOS Client Structure

```
AileronMobile/
  App/
    AileronApp.swift         -- Entry point
  Views/
    TabCarouselView.swift    -- SwiftUI horizontal scroll
    TabDetailView.swift      -- WKWebView or UIImageView
    TerminalView.swift       -- Terminal with custom input
    DownloadsView.swift
    SettingsView.swift
    QRScannerView.swift
  Services/
    ARPClient.swift          -- URLSessionWebSocketTask
    ARPMessage.swift
  ViewModels/
    TabsViewModel.swift
    TerminalViewModel.swift
```

### 6.4 Mobile UI Design

#### Tab Carousel (Main Screen)
```
+----------------------------------+
|  [<] [Tab 1] [Tab 2] [Tab 3] [>] |  ← Horizontal scroll
+----------------------------------+
|                                  |
|     Tab Content Preview          |  ← Screenshot or WebView
|     (pinch to zoom)              |
|                                  |
+----------------------------------+
|  [Terminal] [Downloads] [More]   |  ← Bottom navigation
+----------------------------------+
```

#### Terminal View
```
+----------------------------------+
|  [Back]  Terminal: pane-3        |
+----------------------------------+
|  $ ls -la                        |
|  drwxr-xr-x  5 user staff  160   |
|  -rw-r--r--  1 user staff  42    |
|  $ _                             |
+----------------------------------+
|  [Tab] [Ctrl] [Esc] [↑] [↓]     |  ← Virtual key bar
+----------------------------------+
```

#### Downloads View
```
+----------------------------------+
|  [Back]  Downloads              |
+----------------------------------+
|  file.pdf     45%  2.1 MB/s     |  ← Progress bar
|  image.png   Done  3.4 MB       |
|  data.csv    Paused              |
+----------------------------------+
|  [Cancel All] [Open Folder]      |
+----------------------------------+
```

---

## 7. Performance Considerations

### 7.1 Screenshot Strategy

- **On-demand only**: Screenshots captured when mobile requests, not streamed continuously
- **Debounced capture**: If multiple requests for same tab within 1s, reuse cached screenshot
- **JPEG quality 70%**: Good balance of size vs quality for mobile viewing
- **Max width 800px**: Sufficient for phone screens, reduces bandwidth
- **Typical size**: ~30-60KB per screenshot (800x600 at quality 70%)
- **Future optimization**: Dirty-region tracking to only re-capture changed areas

### 7.2 Terminal Streaming

- **Text-only**: Terminal output is UTF-8 text, extremely bandwidth-efficient
- **Batched**: Terminal output batched in 50ms chunks to reduce message overhead
- **Compression**: Large terminal outputs (>1KB) gzip-compressed

### 7.3 Bandwidth Estimates

| Operation | Size | Frequency |
|-----------|------|-----------|
| Tab list | ~500B | On demand |
| Tab screenshot | ~40KB | On demand |
| Terminal keystroke | ~50B | Per keypress |
| Terminal output | ~100B | Batched 50ms |
| Download progress | ~100B | Per update |

**Idle bandwidth**: ~0 (no polling, push-only events)
**Active browsing**: ~100KB/s (screenshot + interactions)
**Terminal session**: ~5KB/s (text only)

### 7.4 Latency

| Operation | Target Latency | Notes |
|-----------|---------------|-------|
| Tab list | <100ms | Local network |
| Tab screenshot | <500ms | Capture + encode + transmit |
| Terminal input | <50ms | Direct WebSocket |
| Terminal output | <100ms | Batched |

---

## 8. Security Model

### 8.1 Threat Model

| Threat | Mitigation |
|--------|------------|
| Unauthorized access | Token auth + TLS |
| Token interception | TLS 1.3, token over encrypted channel |
| Screenshot content exposure | TLS only, no server-side storage |
| Terminal command injection | Terminal input is raw text, no eval on server |
| Denial of service | Rate limiting, max clients |

### 8.2 Certificate Management

- Self-signed TLS certificate generated on first ARP server start
- Certificate stored in Aileron config directory
- Mobile client verifies certificate fingerprint on first connection
- Option to use Let's Encrypt if desktop has public domain (future)

### 8.3 Token Security

- 256-bit random token generated with `rand::thread_rng()`
- Stored in desktop config (encrypted at rest via OS keyring)
- Transmitted once during pairing (QR code or manual entry)
- Can be rotated from desktop settings
- Never transmitted over unencrypted channel

---

## 9. Future Considerations

### 9.1 Servo Mobile Engine

When Servo adds mature mobile support (Android aarch64 already exists):
- Replace screenshot-based tab viewing with native Servo rendering
- Enable interactive web content on mobile (scroll, click, form input)
- Reduce bandwidth (no screenshots needed)
- Estimated timeline: Servo v0.2+ (~Oct 2026)

### 9.2 Offline Caching

- Cache recent tab screenshots for offline viewing
- Cache tab list metadata
- No interactive features offline

### 9.3 Multi-Desktop Support

- Connect to multiple Aileron desktops
- Unified tab list across desktops
- Desktop discovery via mDNS/bonjour

### 9.4 Biometric Authentication

- Use fingerprint/Face ID to unlock mobile app
- Store ARP token in secure enclave/keychain

---

## 10. Implementation Priority

| Priority | Task | Effort |
|----------|------|--------|
| P0 | ARP server in desktop Aileron | 3-5 days |
| P0 | Core JSON-RPC methods (tabs, terminal) | 2-3 days |
| P1 | Android client (Kotlin + Compose) | 5-7 days |
| P1 | QR code pairing flow | 1 day |
| P2 | iOS client (Swift + SwiftUI) | 5-7 days |
| P2 | Download management on mobile | 2 days |
| P3 | mDNS discovery | 1 day |
| P3 | Biometric auth | 1 day |
