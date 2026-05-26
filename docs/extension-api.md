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

## chrome.alarms

Requires `alarms` permission. Provides timed callbacks for background tasks.

### Methods

| Method | Signature | Returns |
|---|---|---|
| `create` | `create(AlarmCreateParams)` | — |
| `get` | `get(name?) → AlarmInfo?` | Named alarm (default `""`) |
| `getAll` | `getAll() → AlarmInfo[]` | All active alarms |
| `clear` | `clear(name?) → boolean` | Whether alarm existed |
| `clearAll` | `clearAll() → boolean` | Whether any alarms were cleared |

### AlarmCreateParams

| Field | Type | Description |
|---|---|---|
| `name` | `string?` | Alarm name; defaults to `""` |
| `when` | `double?` | Fire time in ms since epoch (mutually exclusive with `delayInMinutes`) |
| `delayInMinutes` | `double?` | Delay from now in minutes (mutually exclusive with `when`) |
| `periodInMinutes` | `double?` | Repeat period in minutes after first fire |

Either `when` or `delayInMinutes` must be provided. Creating an alarm with an existing name replaces it.

### AlarmInfo

`name`, `scheduledTime` (ms since epoch), `periodInMinutes?`

### Events

| Event | Callback Signature |
|---|---|
| `onAlarm` | `(AlarmInfo)` |

One-shot alarms are removed after firing. Periodic alarms are rescheduled by `periodInMinutes * 60_000` ms.

## chrome.cookies

Requires `cookies` permission (plus host permissions for the target domains). Read, write, and observe browser cookies.

### Methods

| Method | Signature | Returns |
|---|---|---|
| `get` | `get(CookieGetParams) → Cookie?` | Matching cookie |
| `getAll` | `getAll(CookieGetAllParams) → Cookie[]` | Filtered list |
| `set` | `set(CookieSetParams) → Cookie?` | Created/updated cookie |
| `remove` | `remove(CookieRemoveParams) → Cookie?` | Removed cookie |

### CookieGetParams

`url` (required), `name` (required), `storeId?`

### CookieGetAllParams

`url?`, `name?`, `domain?`, `path?`, `secure?`, `session?`, `storeId?` — all filters optional; only matching cookies returned.

### CookieSetParams

`url` (required), `name?`, `value?`, `domain?`, `path?` (default `"/"`), `secure?`, `httpOnly?`, `sameSite?`, `expirationDate?`, `storeId?`

Setting a cookie with the same name/domain/path overwrites the existing one. Domain defaults to the URL host (with leading dot) if omitted. `session` is automatically `true` when `expirationDate` is absent.

### CookieRemoveParams

`url` (required), `name` (required), `storeId?`

### Cookie object

`name`, `value`, `domain`, `hostOnly`, `path`, `secure`, `httpOnly`, `sameSite`, `session`, `expirationDate?`, `storeId?`

### SameSiteStatus

`no_restriction`, `lax`, `strict`

### Events

| Event | Callback Signature |
|---|---|
| `onChanged` | `(CookieChangeInfo)` |

### CookieChangeInfo

`removed` (boolean), `cookie` (Cookie), `cause` (CookieChangeCause)

### CookieChangeCause

| Cause | Description |
|---|---|
| `explicit` | Changed by a `cookies.set()` call |
| `overwritten` | Overwritten by a new cookie with same key |
| `expired` | Automatically removed due to expiry |
| `evicted` | Evicted because the cookie jar was full |
| `webRequest` | Modified by a network request (not tracked) |

## chrome.contextMenus

Requires `contextMenus` permission. Create and manage items in the browser context menu.

### Methods

| Method | Signature | Returns |
|---|---|---|
| `create` | `create(MenuCreateParams) → string` | Item ID (auto-generated if not provided) |
| `update` | `update(id, MenuCreateParams) → boolean` | Whether item existed |
| `remove` | `remove(id) → boolean` | Whether item existed |
| `removeAll` | `removeAll() → boolean` | Whether any items were removed |

### MenuCreateParams

| Field | Type | Description |
|---|---|---|
| `id` | `string?` | Unique ID; auto-generated (`ctx-menu-<n>`) if omitted |
| `title` | `string?` | Display text |
| `contexts` | `ContextType[]?` | Defaults to `["page"]` |
| `type` | `MenuItemType?` | Defaults to `"normal"` |
| `checked` | `boolean?` | Initial checked state (checkbox/radio) |
| `enabled` | `boolean?` | Defaults to `true` |
| `parentId` | `string?` | Parent item ID for sub-menus |
| `documentUrlPatterns` | `string[]?` | URL patterns for showing the item |
| `visible` | `boolean?` | Visibility |
| `selector` | `string?` | CSS selector for target element matching |

### ContextType

`all`, `page`, `frame`, `selection`, `link`, `editable`, `image`, `video`, `audio`, `launcher`, `browser_action`, `page_action`, `tab`

### MenuItemType

`normal`, `checkbox`, `radio`, `separator`

### MenuItem object

`id`, `extensionId`, `title?`, `contexts`, `checked?`, `enabled`, `parentId?`, `item_type`, `documentUrlPatterns?`

### Events

| Event | Callback Signature |
|---|---|
| `onClicked` | `(MenuClickInfo)` |

### MenuClickInfo

`menuItemId`, `parentMenuItemId?`, `context` (ContextType), `checked?`, `pageUrl?`, `linkUrl?`, `srcUrl?`, `selectionText?`

## chrome.declarativeNetRequest

Requires `declarativeNetRequest` permission. Declarative network request modification — the MV3 replacement for `webRequest` blocking. Rules are defined in JSON and loaded from static rulesets or added dynamically.

### Methods

| Method | Signature | Returns |
|---|---|---|
| `updateStaticRuleset` | `updateStaticRuleset(rulesetId, enabled)` | — |
| `getEnabledRulesets` | `getEnabledRulesets() → DnrRuleset[]` | Enabled static rulesets |
| `loadStaticRuleset` | `loadStaticRuleset(DnrRuleset)` | — |
| `addDynamicRules` | `addDynamicRules(DnrRule[])` | — |
| `removeDynamicRules` | `removeDynamicRules(ruleIds[])` | — |

Static rulesets are loaded from `declarative_net_request.rule_resources` in the manifest. Dynamic rules are session-scoped. Rules with duplicate IDs are replaced on add.

### Evaluation

Rules are evaluated by priority (descending). The first matching rule's action produces a `DnrVerdict`. `allow` / `allowAllRequests` override matching `block` rules at lower priority.

### DnrRule

`id` (u32), `priority?` (u32, default 1), `action` (DnrAction), `condition` (DnrCondition)

### DnrAction

| Type | Fields | Description |
|---|---|---|
| `block` | — | Block the request |
| `redirect` | `url?`, `extensionPath?`, `transform?` | Redirect the request |
| `modifyHeaders` | `requestHeaders?`, `responseHeaders?` | Modify request/response headers |
| `allow` | — | Override a matching block at lower priority |
| `allowAllRequests` | — | Allow all requests from this extension |

### DnrCondition

| Field | Type | Description |
|---|---|---|
| `urlFilter` | `string?` | URL filter pattern (`*` wildcard, `\|\|` domain anchor, `^` separator) |
| `regexFilter` | `string?` | Regex (alternative to `urlFilter`) |
| `resourceTypes` | `DnrResourceType[]?` | Only match these types |
| `excludedResourceTypes` | `DnrResourceType[]?` | Exclude these types |
| `domains` | `string[]?` | Only match from these initiator domains |
| `excludedDomains` | `string[]?` | Exclude these initiator domains |
| `isUrlFilterCaseSensitive` | `boolean?` | Default `false` |

### DnrResourceType

`main_frame`, `sub_frame`, `stylesheet`, `script`, `image`, `font`, `object`, `xmlhttprequest`, `ping`, `csp_report`, `media`, `websocket`, `webtransport`, `webbundle`, `other`

### URL filter syntax

| Token | Meaning |
|---|---|
| `*` | Matches any sequence of characters |
| `\|\|` | Domain anchor — matches beginning of host |
| `^` | Separator — end of URL or path separator |

### DnrHeaderOperation

`header` (string), `operation` (`append` \| `set` \| `remove`), `value?`

### DnrUrlTransform

`scheme?`, `host?`, `port?`, `path?`, `query?`, `queryTransform?` (DnrQueryTransform), `fragment?`

### DnrQueryTransform

`removeParams?` (string[]), `addOrReplaceParams?` (DnrQueryParameter[])

### DnrQueryParameter

`key` (string), `value?`, `replaceOnly?`

### DnrVerdict (internal result)

| Variant | Description |
|---|---|
| `Block` | Request is blocked |
| `Redirect(url)` | Request redirected to URL or extension path |
| `ModifyHeaders` | Request/response headers modified |
| `Allow` | Override matching block rule |

### DnrRuleset

`id` (string), `enabled` (boolean), `rules` (DnrRule[])

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
