# Aileron WebExtensions API Reference

## Overview

Aileron supports a subset of the Chrome WebExtensions API (Manifest V3 primary, MV2 backward-compatible). Extensions are loaded from subdirectories of the platform config directory containing a `manifest.json`:

| Platform | Extensions Directory |
|----------|---------------------|
| Linux | `~/.config/aileron/extensions/<name>/` |
| macOS | `~/Library/Application Support/Aileron/extensions/<name>/` |
| Windows | `%APPDATA%\Aileron\extensions\<name>\` |

The extension ID is derived from the directory name. Storage is persisted at `extensions/storage/<extension_id>/<area>.json`.

## Manifest Format

Required fields: `manifest_version` (3), `name`, `version`.

```jsonc
{
  "manifest_version": 3,
  "name": "My Extension",
  "version": "1.0.0",
  // Optional fields:
  "description": "What this extension does",
  "permissions": ["tabs", "storage"],
  "optional_permissions": [],
  "host_permissions": ["*://*.example.com/*"],
  "background": {
    "service_worker": "background.js",   // MV3 (preferred)
    "scripts": ["background.js"],         // MV2 fallback
    "persistent": false
  },
  "content_scripts": [{
    "matches": ["https://*.example.com/*"],
    "js": ["content.js"],
    "css": ["styles.css"],
    "run_at": "document_start",           // "document_start" | "document_end" | "document_idle"
    "all_frames": false,
    "match_about_blank": false
  }],
  "action": {
    "default_title": "Click me",
    "default_icon": "icon.png",           // string or { "16": "...", "48": "..." }
    "default_popup": "popup.html"
  },
  "options_page": "options.html",
  "options_ui": { "page": "options.html", "open_in_tab": true },
  "web_accessible_resources": ["img/*", "web_accessible_resources/*"],
  "commands": {
    "my-command": {
      "description": "Does something",
      "suggested_key": {
        "default": "Ctrl+Shift+K",
        "mac": "Command+Shift+K",
        "linux": "Ctrl+Shift+K",
        "windows": "Ctrl+Shift+K",
        "chromeos": "Ctrl+Shift+K"
      }
    }
  },
  "declarative_net_request": {
    "rule_resources": [
      { "id": "default", "enabled": true, "path": "rulesets/default.json" }
    ]
  },
  "icons": { "16": "icon16.png", "32": "icon32.png", "128": "icon128.png" }
}
```

**Background script loading:** `service_worker` takes precedence over `scripts`. Only the first entry in `scripts[]` is loaded. Unknown manifest fields are silently ignored.

## Supported Permissions

| Permission String | Enum Variant | API Namespace |
|---|---|---|
| `activeTab` | `ActiveTab` | tabs |
| `tabs` | `Tabs` | tabs |
| `tabHide` | `TabHide` | tabs |
| `topSites` | `TopSites` | — |
| `bookmarks` | `Bookmarks` | bookmarks |
| `history` | `History` | history |
| `downloads` | `Downloads` | downloads |
| `downloads.open` | `DownloadsOpen` | downloads |
| `downloads.ui` | `DownloadsUI` | downloads |
| `storage` | `Storage` | storage |
| `unlimitedStorage` | `UnlimitedStorage` | storage |
| `scripting` | `Scripting` | scripting |
| `clipboardWrite` | `ClipboardWrite` | clipboard |
| `clipboardRead` | `ClipboardRead` | clipboard |
| `notifications` | `Notifications` | notifications |
| `alarms` | `Alarms` | alarms |
| `webRequest` | `WebRequest` | webRequest |
| `webRequestBlocking` | `WebRequestBlocking` | webRequest |
| `webRequestFilterResponse` | `WebRequestFilterResponse` | webRequest |
| `declarativeNetRequest` | `DeclarativeNetRequest` | — |
| `proxy` | `Proxy` | — |
| `dns` | `Dns` | — |
| `identity` | `Identity` | — |
| `privacy` | `Privacy` | — |
| `browsingData` | `BrowsingData` | — |
| `contextMenus` | `ContextMenus` | contextMenus |
| `devtools` | `Devtools` | — |
| `override` | `Override` | — |
| `management` | `Management` | — |
| `theme` | `Theme` | — |

Unrecognized permission strings are parsed as `Custom(String)` and stored but not enforced.

## chrome.tabs

Requires `tabs` permission (except `sendMessage` which requires `activeTab`).

### Methods

| Method | Signature | Returns |
|---|---|---|
| `query` | `query(TabQuery) → Tab[]` | Filtered list of tabs |
| `create` | `create(CreateProperties) → Tab` | New tab |
| `update` | `update(tabId, UpdateProperties) → Tab` | Updated tab |
| `remove` | `remove(tabId)` | — |
| `duplicate` | `duplicate(tabId) → Tab` | Cloned tab |
| `sendMessage` | `sendMessage(tabId, message)` | Optional response |
| `captureVisibleTab` | `captureVisibleTab(windowId?, CaptureOptions)` | **Not implemented** |

### TabQuery filters

`active`, `windowId`, `url` (UrlPattern[]), `title`, `status`, `pinned`, `audible`, `muted`, `incognito`, `currentWindow`, `highlighted`

### Tab object

`id`, `windowId`, `active`, `pinned`, `url`, `title`, `favIconUrl`, `status` (`"loading"` | `"complete"`), `incognito`, `audible`, `muted`, `width`, `height`, `index`

### Events

| Event | Callback Signature |
|---|---|
| `onUpdated` | `(TabUpdateEvent)` — `{ tabId, changeInfo, tab }` |
| `onCreated` | `(Tab)` |
| `onRemoved` | `(TabId, RemovalInfo)` — `{ windowId, isWindowClosing }` |
| `onActivated` | `(ActiveInfo)` — `{ tabId, windowId }` |

## chrome.storage

Requires `storage` permission. Provides three storage areas, all with the same API.

### StorageArea interface

| Method | Signature |
|---|---|
| `get` | `get(keys) → { [key]: value }` — keys: string \| string[] \| null (all) |
| `set` | `set({ key: value, ... })` |
| `remove` | `remove(string[])` |
| `clear` | `clear()` |
| `getBytesInUse` | `getBytesInUse(keys?) → number` |

### Storage areas

- `chrome.storage.local` — persisted to `storage/<id>/local.json`
- `chrome.storage.sync` — persisted to `storage/<id>/sync.json`
- `chrome.storage.managed` — persisted to `storage/<id>/managed.json`

### Events

| Event | Callback |
|---|---|
| `onChanged` | `(changes, areaName)` — `changes: { key: { oldValue, newValue } }` |

Values are JSON-serialized. Setting a key fires `onChanged` with `{ oldValue: undefined, newValue }`. Removing fires with `{ oldValue, newValue: null }`.

## chrome.webRequest

Requires `webRequest` permission. Full blocking support implemented.

### Blocking events (return `BlockingResponse`)

| Event | Handler Receives |
|---|---|
| `onBeforeRequest` | `RequestDetails` |
| `onBeforeSendHeaders` | `BeforeSendHeadersDetails` |
| `onHeadersReceived` | `HeadersReceivedDetails` |
| `onAuthRequired` | `AuthRequiredDetails` |

### Non-blocking events (fire-and-forget)

| Event | Handler Receives |
|---|---|
| `onBeforeRedirect` | `RedirectDetails` |
| `onCompleted` | `CompletedDetails` |
| `onErrorOccurred` | `ErrorOccurredDetails` |

### RequestFilter

```jsonc
{
  "urls": ["*://*.example.com/*"],  // UrlPattern[]; empty = match all
  "types": ["main_frame", "script"], // optional ResourceType[]
  "tabId": 1,                        // optional
  "windowId": 1                      // optional
}
```

### ExtraInfoSpec flags

`requestHeaders`, `responseHeaders`, `blocking`

### BlockingResponse

| Field | Type | Description |
|---|---|---|
| `cancel` | `boolean?` | Cancel the request |
| `redirectUrl` | `string?` | Redirect to this URL |
| `requestHeaders` | `HttpHeader[]?` | Modify request headers |
| `responseHeaders` | `HttpHeader[]?` | Modify response headers |
| `authCredentials` | `{ username, password }?` | Supply auth (onAuthRequired) |

Headers with `value: null` are removed. First handler returning a non-default response wins.

### Resource types

`main_frame`, `sub_frame`, `stylesheet`, `script`, `image`, `font`, `object`, `xmlhttprequest`, `ping`, `media`, `websocket`, `other`

### URL pattern matching

| Pattern | Matches |
|---|---|
| `<all_urls>` | Any URL |
| `*://*.example.com/*` | Any scheme, any subdomain |
| `https://example.com/*` | Exact scheme and host, any path |
| `*://example.com/*` | Any scheme, exact host |

## chrome.scripting

Requires `scripting` permission.

### Methods

| Method | Status | Notes |
|---|---|---|
| `executeScript(target, { func, args })` | Supported | Function injection only |
| `executeScript(target, { file })` | **Unsupported** | Returns error |
| `insertCSS(target, { css })` | Supported | Inline CSS only |
| `insertCSS(target, { file })` | **Unsupported** | Returns error |
| `removeCSS(target, { css })` | Supported | Removes injected styles |
| `removeCSS(target, { file })` | **Unsupported** | Returns error |
| `registerContentScripts(scripts)` | Supported | Dynamic registration |
| `getRegisteredContentScripts(filter?)` | Supported | Query by IDs |
| `unregisterContentScripts(filter?)` | Supported | Unregister by IDs |

### InjectionTarget

```jsonc
{
  "tabId": 1,
  "frameIds": [0],    // optional; omit for all frames
  "allFrames": false
}
```

### RegisteredContentScript

```jsonc
{
  "id": "my-script",
  "js": ["code..."],
  "css": ["code..."],
  "matches": ["*://*/*"],
  "exclude_matches": [],
  "runAt": "document_start",   // "document_start" | "document_end" | "document_idle"
  "allFrames": false,
  "matchAboutBlank": false
}
```

Injections are queued and drained by the frame task system during navigation. `executeScript` returns a placeholder `InjectionResult` — actual return values require JS runtime evaluation.

## chrome.runtime

No permission required (intrinsic).

### Methods

| Method | Signature | Status |
|---|---|---|
| `sendMessage` | `sendMessage(extensionId?, message) → response?` | Supported |
| `connect` | `connect({ extensionId?, name? }) → Port` | Supported |
| `getManifest` | `getManifest() → Manifest` | Supported |
| `getURL` | `getURL(path) → "aileron://extensions/<id>/<path>"` | Supported |
| `getId` | `getId() → string` | Supported |
| `reload` | `reload()` | **Not implemented** |
| `openOptionsPage` | `openOptionsPage()` | **Not implemented** |

### Events

| Event | Callback Signature |
|---|---|
| `onMessage` | `(message, sender) → response?` |
| `onConnect` | `(Port)` |
| `onInstalled` | `(InstalledDetails)` — `{ reason, previousVersion?, id }` |
| `onStartup` | `()` |

`InstallReason`: `install`, `update`, `browser_update`, `shared_module_update`.

## Message Bus

Inter-extension communication via `runtime.sendMessage` and `runtime.onMessage`.

### Modes

| Mode | Behavior |
|---|---|
| **Direct** | `sendMessage(targetId, msg)` — routes to specific extension |
| **Broadcast** | `sendMessage(undefined, msg)` — delivers to all extensions except source |

Messages are JSON-serializable (`serde_json::Value`). The first handler that returns a non-null response wins.

### LocalPort

Long-lived connections created via `runtime.connect()`.

| Method | Description |
|---|---|
| `name()` | Port name string |
| `postMessage(msg)` | Send a message; errors after disconnect |
| `disconnect()` | Close the port (idempotent) |
| `onMessage(cb)` | Register message handler |
| `onDisconnect(cb)` | Register disconnect handler |

## Extension Manager

The `ExtensionManager` handles discovery, loading, and lifecycle.

| Operation | Description |
|---|---|
| `load_all()` | Scan extensions dir, load all valid manifests |
| `fire_all_startup()` | Trigger `onStartup` for all loaded extensions |
| `unload(id)` | Remove an extension by ID |
| `get(id)` | Access extension API by ID |
| `list()` | List all loaded extension IDs |
| `count()` | Number of loaded extensions |

## Limitations

| Feature | Status |
|---|---|
| `tabs.captureVisibleTab` | Not implemented (stubbed — returns `Unsupported`) |
| `scripting.executeScript` with `file` | Not implemented (returns `Unsupported`) |
| `scripting.insertCSS` with `file` | Not implemented (returns `Unsupported`) |
| `scripting.removeCSS` with `file` | Not implemented (returns `Unsupported`) |
| `runtime.reload()` | Not implemented (stubbed) |
| `runtime.openOptionsPage()` | Not implemented (stubbed) |
| Background script JS execution | Scripts are loaded but not yet evaluated in a JS runtime |
| `background.scripts` array | Only the first entry is loaded |
| Popup windows | Not yet implemented |

## Example Extension

```
~/.config/aileron/extensions/my-extension/
├── manifest.json
├── background.js
├── content.js
└── icon.png
```

**manifest.json:**

```json
{
  "manifest_version": 3,
  "name": "Hello World",
  "version": "1.0.0",
  "description": "A minimal Aileron extension",
  "permissions": ["tabs", "storage", "scripting"],
  "background": {
    "service_worker": "background.js"
  },
  "content_scripts": [{
    "matches": ["https://*.example.com/*"],
    "js": ["content.js"],
    "run_at": "document_end"
  }],
  "action": {
    "default_title": "Hello World",
    "default_icon": "icon.png"
  }
}
```

**background.js:**

```js
chrome.runtime.onInstalled.addListener((details) => {
  chrome.storage.local.set({ greeting: "Hello from Aileron!" });
  console.log("Extension installed:", details.reason);
});

chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
  if (message.type === "ping") {
    sendResponse({ pong: true });
  }
});

chrome.tabs.query({ active: true }, (tabs) => {
  if (tabs.length > 0) {
    chrome.scripting.executeScript({
      target: { tabId: tabs[0].id },
      func: () => { document.title = "Injected!"; }
    });
  }
});
```

**content.js:**

```js
chrome.runtime.sendMessage({ type: "ping" }, (response) => {
  console.log("Response:", response);
});

chrome.storage.local.get("greeting", (items) => {
  if (items.greeting) {
    console.log(items.greeting);
  }
});
```
