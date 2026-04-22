# T02: Aileron Remote Protocol (ARP) Specification

**Date:** 2026-04-22
**Author:** Aileron Architecture Team
**Status:** DRAFT
**Version:** 1.0.0-draft
**Related:** mobile_architecture.md (T01), BP-APP-CORE-001

---

## 1. Protocol Overview

ARP is a JSON-RPC 2.0 protocol over WebSocket, designed for efficient communication between Aileron desktop and mobile clients.

### 1.1 Design Principles

- **Simple**: JSON-RPC 2.0 is well-understood, easy to implement in any language
- **Efficient**: Text-based terminal output, compressed screenshots, no polling
- **Secure**: TLS 1.3 mandatory, token authentication
- **Idempotent**: Most operations are safe to retry
- **Ordered**: WebSocket guarantees message ordering within a connection

### 1.2 Transport

```
ws://[host]:19743/arp    (development only)
wss://[host]:19743/arp   (production, mandatory)
```

Default port: `19743` (chosen to avoid conflicts; memorable as "19" + "743" from Aileron's initials)

### 1.3 Message Format

All messages follow JSON-RPC 2.0 specification (https://www.jsonrpc.org/specification):

**Request:**
```json
{
  "jsonrpc": "2.0",
  "method": "tabs.list",
  "params": {},
  "id": 1
}
```

**Response (success):**
```json
{
  "jsonrpc": "2.0",
  "result": [{ "id": 1, "url": "https://...", "title": "..." }],
  "id": 1
}
```

**Response (error):**
```json
{
  "jsonrpc": "2.0",
  "error": {
    "code": -32602,
    "message": "Invalid params: id must be a number"
  },
  "id": 1
}
```

**Notification (server push, no id):**
```json
{
  "jsonrpc": "2.0",
  "method": "tab.updated",
  "params": { "id": 1, "url": "https://new.com", "title": "New" }
}
```

---

## 2. Connection Lifecycle

### 2.1 Handshake

```
Client                              Server
  |                                    |
  |--- WebSocket Upgrade ------------->|
  |    Sec-WebSocket-Protocol: arp     |
  |    Authorization: Bearer <token>   |
  |                                    |
  |<-- 101 Switching Protocols --------|
  |                                    |
  |<-- { method: "server.hello",       |
  |      params: {                     |
  |        version: "1.0.0",           |
  |        session_id: "abc123",       |
  |        server_info: { ... }        |
  |      }                             |
  |    }                               |
  |                                    |
  |--- { method: "client.info",        |
  |      params: {                     |
  |        client_name: "Aileron Android",|
  |        client_version: "1.0.0",    |
  |        platform: "android"         |
  |      }                             |
  |    }                               |
```

### 2.2 Authentication

**Token Format:** 64-character hex string (256-bit random)

**Generation:**
```rust
use rand::Rng;
let token: String = rand::thread_rng()
    .gen::<[u8; 32]>()
    .iter()
    .map(|b| format!("{:02x}", b))
    .collect();
```

**Storage:**
- Desktop: `config_dir()/arp_token` (file permissions 0600)
- Mobile: Platform secure storage (Android Keystore / iOS Keychain)

**Authorization Header:**
```
Authorization: Bearer a1b2c3d4e5f6...64chars
```

### 2.3 Reconnection

Mobile clients should auto-reconnect with exponential backoff:
- 1s, 2s, 4s, 8s, 16s, 32s, max 60s
- Re-authenticate on each reconnection
- Server sends `server.hello` on reconnect with current state

### 2.4 Disconnection

Either side may close the WebSocket connection at any time. The server should:
- Clean up session state
- Cancel any pending subscriptions
- Release rate limit counters

---

## 3. Error Codes

| Code | Name | Description |
|------|------|-------------|
| -32700 | Parse error | Invalid JSON was received |
| -32600 | Invalid Request | The JSON sent is not a valid Request object |
| -32601 | Method not found | The method does not exist / is not available |
| -32602 | Invalid params | Invalid method parameter(s) |
| -32603 | Internal error | Internal JSON-RPC error |
| -32001 | Auth failed | Invalid or missing authentication token |
| -32002 | Rate limited | Too many requests (per-minute limit exceeded) |
| -32003 | Not found | Requested resource (tab, download, etc.) not found |
| -32004 | Permission denied | Operation not permitted for this session |
| -32005 | Server busy | Server is under heavy load, retry later |
| -32006 | Session expired | Session has been invalidated |

---

## 4. Methods Reference

### 4.1 Server Methods (Client → Server)

#### 4.1.1 System

**`system.info`**
Get server information.

Request:
```json
{ "method": "system.info", "params": {}, "id": 1 }
```

Response:
```json
{
  "result": {
    "aileron_version": "0.14.0",
    "arp_version": "1.0.0",
    "os": "Linux",
    "hostname": "desktop",
    "uptime_seconds": 86400,
    "active_tab_count": 5,
    "active_terminal_count": 2,
    "active_download_count": 1,
    "connected_clients": 2
  },
  "id": 1
}
```

**`system.subscribe`**
Subscribe to server push events.

Request:
```json
{ "method": "system.subscribe", "params": { "events": ["tab.*", "download.*"] }, "id": 2 }
```

Response:
```json
{ "result": { "subscribed": ["tab.*", "download.*"] }, "id": 2 }
```

After subscription, server pushes matching events as notifications (no `id` field).

**`system.ping`**
Keep-alive / latency measurement.

Request:
```json
{ "method": "system.ping", "params": { "timestamp": 1713830400000 }, "id": 3 }
```

Response:
```json
{ "result": { "timestamp": 1713830400000, "server_time": 1713830400042 }, "id": 3 }
```

#### 4.1.2 Tabs

**`tabs.list`**
List all open tabs.

Request:
```json
{ "method": "tabs.list", "params": {}, "id": 10 }
```

Response:
```json
{
  "result": [
    {
      "id": 1,
      "url": "https://github.com",
      "title": "GitHub",
      "loading": false,
      "active": true
    },
    {
      "id": 2,
      "url": "https://docs.rs",
      "title": "docs.rs",
      "loading": true,
      "active": false
    }
  ],
  "id": 10
}
```

**`tabs.get`**
Get details for a specific tab.

Request:
```json
{ "method": "tabs.get", "params": { "id": 1 }, "id": 11 }
```

Response:
```json
{
  "result": {
    "id": 1,
    "url": "https://github.com",
    "title": "GitHub",
    "loading": false,
    "active": true,
    "can_go_back": true,
    "can_go_forward": false
  },
  "id": 11
}
```

**`tabs.activate`**
Switch to a specific tab.

Request:
```json
{ "method": "tabs.activate", "params": { "id": 2 }, "id": 12 }
```

Response:
```json
{ "result": { "activated": 2 }, "id": 12 }
```

**`tabs.create`**
Open a new tab with a URL.

Request:
```json
{ "method": "tabs.create", "params": { "url": "https://example.com" }, "id": 13 }
```

Response:
```json
{ "result": { "id": 3 }, "id": 13 }
```

**`tabs.close`**
Close a tab.

Request:
```json
{ "method": "tabs.close", "params": { "id": 2 }, "id": 14 }
```

Response:
```json
{ "result": { "closed": 2 }, "id": 14 }
```

**`tabs.navigate`**
Navigate a tab to a URL.

Request:
```json
{ "method": "tabs.navigate", "params": { "id": 1, "url": "https://rust-lang.org" }, "id": 15 }
```

Response:
```json
{ "result": { "navigated": true }, "id": 15 }
```

**`tabs.screenshot`**
Capture a screenshot of a tab as JPEG.

Request:
```json
{ "method": "tabs.screenshot", "params": { "id": 1, "width": 800, "quality": 70 }, "id": 16 }
```

Response:
```json
{
  "result": {
    "data": "/9j/4AAQSkZJRgABAQEA...",  // base64-encoded JPEG
    "width": 800,
    "height": 600,
    "content_type": "image/jpeg",
    "size_bytes": 38400
  },
  "id": 16
}
```

**`tabs.scroll`**
Scroll a tab's content.

Request:
```json
{ "method": "tabs.scroll", "params": { "id": 1, "direction": "down", "amount": 300 }, "id": 17 }
```

Response:
```json
{ "result": { "scrolled": true }, "id": 17 }
```

**`tabs.goBack` / `tabs.goForward`**
Navigate history.

Request:
```json
{ "method": "tabs.goBack", "params": { "id": 1 }, "id": 18 }
```

#### 4.1.3 Terminal

**`terminal.list`**
List all terminal panes.

Request:
```json
{ "method": "terminal.list", "params": {}, "id": 20 }
```

Response:
```json
{
  "result": [
    { "id": "pane-3", "title": "bash", "rows": 24, "cols": 80 }
  ],
  "id": 20
}
```

**`terminal.input`**
Send text/keystrokes to a terminal.

Request:
```json
{ "method": "terminal.input", "params": { "id": "pane-3", "text": "ls -la\n" }, "id": 21 }
```

Response:
```json
{ "result": { "sent": true }, "id": 21 }
```

**`terminal.sendKey`**
Send a special key (control sequence).

Request:
```json
{ "method": "terminal.sendKey", "params": { "id": "pane-3", "key": "ctrl_c" }, "id": 22 }
```

Supported keys: `ctrl_c`, `ctrl_d`, `ctrl_z`, `ctrl_l`, `ctrl_a`, `ctrl_e`, `ctrl_u`, `ctrl_k`, `ctrl_w`, `tab`, `enter`, `escape`, `up`, `down`, `left`, `right`, `home`, `end`, `page_up`, `page_down`.

Response:
```json
{ "result": { "sent": true }, "id": 22 }
```

**`terminal.resize`**
Resize a terminal pane.

Request:
```json
{ "method": "terminal.resize", "params": { "id": "pane-3", "cols": 80, "rows": 24 }, "id": 23 }
```

Response:
```json
{ "result": { "resized": true }, "id": 23 }
```

**`terminal.snapshot`**
Get current terminal content as text.

Request:
```json
{ "method": "terminal.snapshot", "params": { "id": "pane-3" }, "id": 24 }
```

Response:
```json
{
  "result": {
    "content": "$ ls -la\ntotal 16\ndrwxr-xr-x  5 user ...\n$ ",
    "cursor_row": 3,
    "cursor_col": 2
  },
  "id": 24
}
```

#### 4.1.4 Downloads

**`downloads.list`**
List recent downloads.

Request:
```json
{ "method": "downloads.list", "params": { "limit": 20 }, "id": 30 }
```

Response:
```json
{
  "result": [
    {
      "id": 1,
      "url": "https://example.com/file.pdf",
      "filename": "file.pdf",
      "state": "downloading",
      "received_bytes": 1048576,
      "total_bytes": 5242880,
      "speed_bytes_per_sec": 2097152,
      "percent": 20
    },
    {
      "id": 2,
      "url": "https://example.com/image.png",
      "filename": "image.png",
      "state": "completed",
      "received_bytes": 340000,
      "total_bytes": 340000,
      "percent": 100
    }
  ],
  "id": 30
}
```

**`downloads.cancel`**
Cancel an active download.

Request:
```json
{ "method": "downloads.cancel", "params": { "id": 1 }, "id": 31 }
```

**`downloads.pause` / `downloads.resume`**
Control download state.

Request:
```json
{ "method": "downloads.pause", "params": { "id": 1 }, "id": 32 }
```

#### 4.1.5 Clipboard

**`clipboard.get`**
Get desktop clipboard content.

Request:
```json
{ "method": "clipboard.get", "params": {}, "id": 40 }
```

Response:
```json
{ "result": { "text": "https://github.com/servo/servo" }, "id": 40 }
```

**`clipboard.set`**
Set desktop clipboard content.

Request:
```json
{ "method": "clipboard.set", "params": { "text": "Hello from mobile" }, "id": 41 }
```

Response:
```json
{ "result": { "set": true }, "id": 41 }
```

#### 4.1.6 Quickmarks & Bookmarks

**`quickmarks.list`**
List all quickmarks.

Request:
```json
{ "method": "quickmarks.list", "params": {}, "id": 50 }
```

Response:
```json
{
  "result": [
    { "keyword": "gh", "url": "https://github.com" },
    { "keyword": "docs", "url": "https://docs.rs" }
  ],
  "id": 50
}
```

**`quickmarks.open`**
Open a quickmark URL.

Request:
```json
{ "method": "quickmarks.open", "params": { "keyword": "gh" }, "id": 51 }
```

---

## 5. Server-Push Events

Events are sent as JSON-RPC notifications (no `id` field) after client subscribes via `system.subscribe`.

### 5.1 Event Pattern Matching

`system.subscribe` params use glob patterns:
- `"tab.*"` — all tab events
- `"download.*"` — all download events
- `"terminal.output"` — terminal output only
- `"*"` — all events

### 5.2 Event Definitions

**`tab.created`**
```json
{
  "method": "tab.created",
  "params": { "id": 3, "url": "https://example.com", "title": "Example" }
}
```

**`tab.updated`**
```json
{
  "method": "tab.updated",
  "params": { "id": 1, "url": "https://new.com", "title": "New Title" }
}
```

**`tab.closed`**
```json
{
  "method": "tab.closed",
  "params": { "id": 2 }
}
```

**`tab.activated`**
```json
{
  "method": "tab.activated",
  "params": { "id": 1 }
}
```

**`tab.loading`**
```json
{
  "method": "tab.loading",
  "params": { "id": 1, "loading": true }
}
```

**`download.started`**
```json
{
  "method": "download.started",
  "params": { "id": 5, "url": "https://...", "filename": "file.pdf" }
}
```

**`download.progress`**
```json
{
  "method": "download.progress",
  "params": {
    "id": 5,
    "received_bytes": 2097152,
    "total_bytes": 5242880,
    "speed_bytes_per_sec": 1048576,
    "percent": 40
  }
}
```

Debounced: maximum 1 update per download per 500ms.

**`download.completed`**
```json
{
  "method": "download.completed",
  "params": { "id": 5, "dest_path": "/home/user/Downloads/file.pdf" }
}
```

**`download.failed`**
```json
{
  "method": "download.failed",
  "params": { "id": 5, "error": "Connection timed out" }
}
```

**`terminal.output`**
```json
{
  "method": "terminal.output",
  "params": { "id": "pane-3", "data": "$ ls\ntotal 16\n" }
}
```

Batched: terminal output aggregated in 50ms windows.

**`notification`**
```json
{
  "method": "notification",
  "params": { "title": "Download Complete", "body": "file.pdf (3.4 MB)" }
}
```

---

## 6. Rate Limiting

| Limit | Value | Notes |
|-------|-------|-------|
| Requests per minute | 100 | Per session |
| Screenshots per minute | 30 | Expensive operation |
| Terminal input per second | 50 | Prevent flooding |
| Max concurrent sessions | 5 | Per server |
| Max message size | 1 MB | Screenshots excluded (streamed) |
| Max screenshot size | 5 MB | JPEG base64 |

Rate limit exceeded returns error code `-32002` with `Retry-After` header equivalent in error data:
```json
{
  "error": {
    "code": -32002,
    "message": "Rate limited",
    "data": { "retry_after_ms": 5000 }
  }
}
```

---

## 7. Configuration

### 7.1 Server Configuration (desktop)

```toml
[remote]
enabled = false
port = 19743
host = "0.0.0.0"
token = ""  # Auto-generated if empty

[remote.rate_limit]
requests_per_minute = 100
screenshots_per_minute = 30
terminal_inputs_per_second = 50
max_sessions = 5

[remote.screenshots]
default_quality = 70
default_max_width = 800
max_quality = 90
max_width = 1920

[remote.tls]
cert_path = ""  # Auto-generated if empty
key_path = ""   # Auto-generated if empty
```

### 7.2 Client Configuration (mobile)

```json
{
  "servers": [
    {
      "name": "Home Desktop",
      "host": "192.168.1.100",
      "port": 19743,
      "token": "a1b2c3...64chars",
      "fingerprint": "sha256:abc123...",
      "auto_connect": true
    }
  ],
  "preferences": {
    "screenshot_quality": 70,
    "screenshot_width": 800,
    "terminal_font_size": 14,
    "dark_mode": true
  }
}
```

---

## 8. Wire Protocol Details

### 8.1 Message Framing

WebSocket text frames contain JSON-RPC messages, one per frame. Binary frames are not used.

### 8.2 Screenshot Streaming

For large screenshots, the server sends a header message followed by chunked data:

```
Frame 1 (text): { "method": "tabs.screenshot.chunk", "params": { "id": 1, "total_chunks": 3, "chunk_index": 0, "data": "base64..." } }
Frame 2 (text): { "method": "tabs.screenshot.chunk", "params": { "id": 1, "total_chunks": 3, "chunk_index": 1, "data": "base64..." } }
Frame 3 (text): { "method": "tabs.screenshot.chunk", "params": { "id": 1, "total_chunks": 3, "chunk_index": 2, "data": "base64..." } }
```

Alternative: For small screenshots (<100KB), send as a single response message with inline base64 data (simpler, covers 95% of cases).

### 8.3 Compression

Messages larger than 1KB MAY be gzip-compressed. The first byte indicates compression:
- `0x00`: Uncompressed JSON
- `0x01`: Gzip-compressed JSON

For WebSocket text frames, compression is optional and negotiated via WebSocket per-message deflate extension.

---

## 9. Versioning

### 9.1 Protocol Version

The protocol version is exchanged during handshake (`server.hello`). Breaking changes increment the major version; additions increment the minor version.

### 9.2 Version Compatibility

| Client | Server 1.0 | Server 1.1 | Server 2.0 |
|--------|-----------|-----------|-----------|
| 1.0 | Full | Full (ignores new methods) | Incompatible |
| 1.1 | Full | Full | Incompatible |
| 2.0 | Incompatible | Incompatible | Full |

### 9.3 Feature Detection

Clients can detect available methods:

Request:
```json
{ "method": "system.capabilities", "params": {}, "id": 99 }
```

Response:
```json
{
  "result": {
    "version": "1.0.0",
    "methods": ["tabs.list", "tabs.get", "tabs.activate", "..."],
    "events": ["tab.created", "tab.updated", "..."]
  },
  "id": 99
}
```

---

## 10. Testing Strategy

### 10.1 Unit Tests

- Message serialization/deserialization
- Rate limiting logic
- Token validation
- Event pattern matching (glob)

### 10.2 Integration Tests

- WebSocket handshake with valid/invalid token
- All RPC methods with mock AppState
- Event subscription and delivery
- Reconnection after disconnect
- Multiple concurrent sessions

### 10.3 Conformance Tests

- JSON-RPC 2.0 spec compliance
- Error code correctness
- Rate limit enforcement
- Session limit enforcement

### 10.4 Mobile Client Tests

- Android instrumented tests with mock WebSocket server
- iOS XCTest with mock WebSocket server
- Screenshot decode performance
- Terminal input latency
