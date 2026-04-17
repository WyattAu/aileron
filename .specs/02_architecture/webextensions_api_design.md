# WebExtensions API Design

> **Status:** Design document — no implementation
> **Created:** 2026-04-18
> **Phase:** K.1 (Architecture D Preparation)

## Overview

This document defines a Rust trait-based API surface compatible with a subset of the
[WebExtensions API](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions).
The design enables Aileron to support browser extensions written against the standard
WebExtensions API, while keeping the implementation decoupled through trait objects.

### Goals

1. **Compatibility** — extensions targeting `browser.*` / `chrome.*` APIs work with minimal changes
2. **Safety** — Rust trait boundaries enforce API contracts at compile time
3. **Performance** — trait object dispatch is acceptable; hot paths (ad blocking) use concrete types
4. **Incremental adoption** — implement traits one at a time; missing APIs return `UnsupportedError`

### Non-goals

- Full WebExtensions API coverage (target MVP subset only)
- Extension signing/distribution infrastructure
- Manifest V2 support (Manifest V3 only)

---

## MVP Subset

| API            | Coverage | Priority |
|----------------|----------|----------|
| `browser.tabs` | query, create, update, remove, sendMessage, onUpdated | Critical |
| `browser.storage` | local/sync areas, get/set/remove/clear, onChanged | Critical |
| `browser.runtime` | sendMessage, connect, getManifest, getURL, onMessage, onInstalled | Critical |
| `browser.webRequest` | onBeforeRequest, onHeadersReceived, onCompleted (blocking) | Critical |
| `browser.scripting` | executeScript, insertCSS, registerContentScripts | Critical |
| `browser.windows` | create, update, remove, getCurrent | Deferred |
| `browser.bookmarks` | create, get, remove, search, onCreated | Deferred |
| `browser.history` | search, addUrl, deleteUrl, onVisited | Deferred |
| `browser.notifications` | create, clear, onClicked | Deferred |
| `browser.commands` | onCommand | Deferred |
| `browser.alarms` | create, clear, onAlarm | Deferred |

---

## Core Traits

### ExtensionApi (top-level)

```rust
/// Extension API surface — the single entry point for an extension.
/// Each extension gets its own `ExtensionApi` instance with its
/// manifest's permissions enforced.
pub trait ExtensionApi: Send + Sync {
    /// Returns the extension's unique identifier.
    fn id(&self) -> &ExtensionId;

    /// Returns the parsed manifest for this extension.
    fn manifest(&self) -> &ExtensionManifest;

    /// Tabs API — access and manipulate browser tabs.
    fn tabs(&self) -> &dyn TabsApi;

    /// Storage API — persistent key-value storage.
    fn storage(&self) -> &dyn StorageApi;

    /// Runtime API — extension lifecycle and messaging.
    fn runtime(&self) -> &dyn RuntimeApi;

    /// WebRequest API — intercept and modify network requests.
    fn web_request(&self) -> &dyn WebRequestApi;

    /// Scripting API — content script injection and management.
    fn scripting(&self) -> &dyn ScriptingApi;
}
```

---

### TabsApi

Maps to [`browser.tabs`](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/API/tabs).

```rust
/// Access and manipulate browser tabs.
pub trait TabsApi: Send + Sync {
    /// Query tabs matching the given filter. Returns all matching tabs.
    /// Equivalent to `browser.tabs.query(queryInfo)`.
    fn query(&self, query: TabQuery) -> Result<Vec<Tab>, ExtensionError>;

    /// Create a new tab. Returns the created tab.
    /// Equivalent to `browser.tabs.create(createProperties)`.
    fn create(&self, properties: CreateProperties) -> Result<Tab, ExtensionError>;

    /// Update tab properties (url, active, muted, etc.).
    /// Equivalent to `browser.tabs.update(tabId, updateProperties)`.
    fn update(&self, tab_id: TabId, properties: UpdateProperties) -> Result<Tab, ExtensionError>;

    /// Close a tab.
    /// Equivalent to `browser.tabs.remove(tabId)`.
    fn remove(&self, tab_id: TabId) -> Result<(), ExtensionError>;

    /// Duplicate a tab, returning the new tab.
    fn duplicate(&self, tab_id: TabId) -> Result<Tab, ExtensionError>;

    /// Send a message to content scripts running in a specific tab.
    /// Equivalent to `browser.tabs.sendMessage(tabId, message)`.
    fn send_message(
        &self,
        tab_id: TabId,
        message: RuntimeMessage,
    ) -> Result<Option<RuntimeMessage>, ExtensionError>;

    /// Capture the visible area of a tab as an image.
    fn capture_visible_tab(
        &self,
        window_id: Option<WindowId>,
        options: CaptureOptions,
    ) -> Result<Vec<u8>, ExtensionError>;

    /// Register a listener for tab update events (url change, title change, status).
    /// Equivalent to `browser.tabs.onUpdated.addListener(callback)`.
    fn on_updated(&self, callback: Box<dyn Fn(TabUpdateEvent) + Send + Sync>);

    /// Register a listener for tab creation.
    fn on_created(&self, callback: Box<dyn Fn(Tab) + Send + Sync>);

    /// Register a listener for tab removal.
    fn on_removed(&self, callback: Box<dyn Fn(TabId, RemovalInfo) + Send + Sync>);

    /// Register a listener for tab activation (focus).
    fn on_activated(&self, callback: Box<dyn Fn(ActiveInfo) + Send + Sync>);
}
```

---

### StorageApi

Maps to [`browser.storage`](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/API/storage).

```rust
/// Key-value storage for extensions.
pub trait StorageApi: Send + Sync {
    /// The local storage area (persists on disk, per-extension).
    fn local(&self) -> &dyn StorageArea;

    /// The sync storage area (synced across devices, quota-limited).
    fn sync(&self) -> &dyn StorageArea;

    /// The managed storage area (set by enterprise policy, read-only).
    fn managed(&self) -> &dyn StorageArea;
}

/// A storage area with get/set/remove/clear operations.
pub trait StorageArea: Send + Sync {
    /// Retrieve one or more items from storage.
    /// `keys` can be a single key, a list of keys, or None for all items.
    /// Returns a map of key → value.
    fn get(&self, keys: StorageGetKeys) -> Result<StorageChanges, ExtensionError>;

    /// Store one or more items. Values must be JSON-serializable.
    fn set(&self, items: StorageChanges) -> Result<(), ExtensionError>;

    /// Remove one or more items.
    fn remove(&self, keys: Vec<String>) -> Result<(), ExtensionError>;

    /// Remove all items from this storage area.
    fn clear(&self) -> Result<(), ExtensionError>;

    /// Get the approximate bytes in use for this storage area.
    fn get_bytes_in_use(&self, keys: Option<Vec<String>>) -> Result<u64, ExtensionError>;

    /// Register a listener for storage changes.
    fn on_changed(
        &self,
        callback: Box<dyn Fn(StorageChanges, String) + Send + Sync>,
    );
}
```

---

### RuntimeApi

Maps to [`browser.runtime`](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/API/runtime).

```rust
/// Extension runtime — lifecycle, messaging, and manifest access.
pub trait RuntimeApi: Send + Sync {
    /// Send a one-time message to event listeners in the extension
    /// (background script or other contexts).
    fn send_message(
        &self,
        extension_id: Option<ExtensionId>,
        message: RuntimeMessage,
    ) -> Result<Option<RuntimeMessage>, ExtensionError>;

    /// Establish a long-lived connection for bidirectional messaging.
    fn connect(&self, connect_info: ConnectInfo) -> Result<Port, ExtensionError>;

    /// Get the parsed extension manifest.
    fn get_manifest(&self) -> Result<ExtensionManifest, ExtensionError>;

    /// Resolve a path relative to the extension's install directory.
    /// Equivalent to `browser.runtime.getURL(path)`.
    fn get_url(&self, path: &str) -> Result<Url, ExtensionError>;

    /// Get the extension's own ID.
    fn get_id(&self) -> &ExtensionId;

    /// Register a listener for incoming messages.
    fn on_message(
        &self,
        callback: Box<dyn Fn(RuntimeMessage, MessageSender) -> Option<RuntimeMessage> + Send + Sync>,
    );

    /// Register a listener for incoming port connections.
    fn on_connect(&self, callback: Box<dyn Fn(Port) + Send + Sync>);

    /// Register a listener for extension installation/update.
    fn on_installed(&self, callback: Box<dyn Fn(InstalledDetails) + Send + Sync>);

    /// Register a listener for browser startup (extension already installed).
    fn on_startup(&self, callback: Box<dyn Fn() + Send + Sync>);

    /// Reload the extension (developer use).
    fn reload(&self) -> Result<(), ExtensionError>;

    /// Open the extension's options page (if declared in manifest).
    fn open_options_page(&self) -> Result<(), ExtensionError>;
}
```

---

### WebRequestApi

Maps to [`browser.webRequest`](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/API/webRequest).
This is the primary hook for ad/tracking blocking.

```rust
/// Intercept and modify network requests in-flight.
/// This is the foundation for ad blockers, privacy extensions,
/// and request modification extensions.
pub trait WebRequestApi: Send + Sync {
    /// Fired before a request is sent. Can block or redirect.
    /// Equivalent to `browser.webRequest.onBeforeRequest`.
    fn on_before_request(
        &self,
        filter: RequestFilter,
        extra_info_spec: Vec<ExtraInfoSpec>,
        handler: Box<dyn Fn(RequestDetails) -> BlockingResponse + Send + Sync>,
    ) -> ListenerId;

    /// Fired before request headers are sent. Can modify request headers.
    /// Equivalent to `browser.webRequest.onBeforeSendHeaders`.
    fn on_before_send_headers(
        &self,
        filter: RequestFilter,
        extra_info_spec: Vec<ExtraInfoSpec>,
        handler: Box<dyn Fn(BeforeSendHeadersDetails) -> BlockingResponse + Send + Sync>,
    ) -> ListenerId;

    /// Fired when response headers are received. Can modify response headers.
    /// Equivalent to `browser.webRequest.onHeadersReceived`.
    fn on_headers_received(
        &self,
        filter: RequestFilter,
        extra_info_spec: Vec<ExtraInfoSpec>,
        handler: Box<dyn Fn(HeadersReceivedDetails) -> BlockingResponse + Send + Sync>,
    ) -> ListenerId;

    /// Fired when authentication is required. Can provide credentials.
    fn on_auth_required(
        &self,
        filter: RequestFilter,
        handler: Box<dyn Fn(AuthRequiredDetails) -> BlockingResponse + Send + Sync>,
    ) -> ListenerId;

    /// Fired when a request is about to redirect. Can cancel or redirect.
    fn on_before_redirect(
        &self,
        filter: RequestFilter,
        callback: Box<dyn Fn(RedirectDetails) + Send + Sync>,
    ) -> ListenerId;

    /// Fired when a request completes successfully.
    fn on_completed(
        &self,
        filter: RequestFilter,
        callback: Box<dyn Fn(CompletedDetails) + Send + Sync>,
    ) -> ListenerId;

    /// Fired when a request encounters an error.
    fn on_error_occurred(
        &self,
        filter: RequestFilter,
        callback: Box<dyn Fn(ErrorOccurredDetails) + Send + Sync>,
    ) -> ListenerId;

    /// Remove a previously registered listener.
    fn remove_listener(&self, listener_id: ListenerId) -> Result<(), ExtensionError>;
}
```

---

### ScriptingApi

Maps to [`browser.scripting`](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/API/scripting).

```rust
/// Content script injection and management.
pub trait ScriptingApi: Send + Sync {
    /// Inject a JavaScript function or file into a page/frame.
    fn execute_script(
        &self,
        target: InjectionTarget,
        injection: ScriptInjection,
    ) -> Result<Vec<InjectionResult>, ExtensionError>;

    /// Inject CSS into a page/frame.
    fn insert_css(
        &self,
        target: InjectionTarget,
        injection: CssInjection,
    ) -> Result<(), ExtensionError>;

    /// Remove previously injected CSS.
    fn remove_css(
        &self,
        target: InjectionTarget,
        injection: CssInjection,
    ) -> Result<(), ExtensionError>;

    /// Register content scripts declared in manifest or dynamically.
    fn register_content_scripts(
        &self,
        scripts: Vec<RegisteredContentScript>,
    ) -> Result<(), ExtensionError>;

    /// Get all registered content scripts for this extension.
    fn get_registered_content_scripts(
        &self,
        filter: Option<ScriptFilter>,
    ) -> Result<Vec<RegisteredContentScript>, ExtensionError>;

    /// Unregister previously registered content scripts.
    fn unregister_content_scripts(
        &self,
        filter: Option<ScriptFilter>,
    ) -> Result<(), ExtensionError>;
}
```

---

## Type Definitions

### Identifiers

```rust
/// Unique extension identifier (e.g., "adblock@example.com").
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct ExtensionId(pub String);

/// Unique tab identifier. Opaque to extensions; assigned by the browser.
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub struct TabId(pub u64);

/// Unique window identifier.
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub struct WindowId(pub u64);

/// Listener registration handle for removal.
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub struct ListenerId(pub u64);
```

### Tab Types

```rust
/// Represents a browser tab.
#[derive(Debug, Clone)]
pub struct Tab {
    /// Tab identifier.
    pub id: TabId,
    /// The window containing this tab.
    pub window_id: WindowId,
    /// Whether this is the active tab in its window.
    pub active: bool,
    /// Whether the tab is pinned.
    pub pinned: bool,
    /// The tab's current URL (may be about:blank while loading).
    pub url: Url,
    /// The page title, if available.
    pub title: Option<String>,
    /// The favIconUrl, if available.
    pub fav_icon_url: Option<Url>,
    /// Tab loading status.
    pub status: TabStatus,
    /// Whether the tab is in an incognito/private window.
    pub incognito: bool,
    /// The tab's current audible state.
    pub audible: bool,
    /// Whether the tab is muted.
    pub muted: bool,
    /// The tab's width in pixels.
    pub width: u32,
    /// The tab's height in pixels.
    pub height: u32,
    /// Index of this tab within its window.
    pub index: u32,
}

/// Tab loading status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabStatus {
    Loading,
    Complete,
}

/// Filter for querying tabs.
#[derive(Debug, Clone, Default)]
pub struct TabQuery {
    /// Match tabs with this active state.
    pub active: Option<bool>,
    /// Match tabs in this window.
    pub window_id: Option<WindowId>,
    /// Match tabs with this URL pattern.
    pub url: Option<Vec<UrlPattern>>,
    /// Match tabs with this title pattern.
    pub title: Option<String>,
    /// Match tabs with this status.
    pub status: Option<TabStatus>,
    /// Match pinned tabs.
    pub pinned: Option<bool>,
    /// Match audible tabs.
    pub audible: Option<bool>,
    /// Match muted tabs.
    pub muted: Option<bool>,
    /// Match incognito tabs.
    pub incognito: Option<bool>,
    /// Match tabs in the current window.
    pub current_window: Option<bool>,
    /// Match the currently active tab.
    pub highlighted: Option<bool>,
}

/// Properties for creating a new tab.
#[derive(Debug, Clone)]
pub struct CreateProperties {
    /// The URL to navigate to. Defaults to new tab page.
    pub url: Option<Url>,
    /// Whether the tab should become active.
    pub active: Option<bool>,
    /// The window to create the tab in.
    pub window_id: Option<WindowId>,
    /// The position to insert the tab at.
    pub index: Option<u32>,
    /// Whether to pin the tab.
    pub pinned: Option<bool>,
    /// Whether to open in incognito window.
    pub incognito: Option<bool>,
    /// Opener tab ID (for grouping).
    pub opener_tab_id: Option<TabId>,
}

/// Properties for updating an existing tab.
#[derive(Debug, Clone)]
pub struct UpdateProperties {
    /// Navigate to a new URL.
    pub url: Option<Url>,
    /// Activate or deactivate the tab.
    pub active: Option<bool>,
    /// Mute or unmute the tab.
    pub muted: Option<bool>,
    /// Pin or unpin the tab.
    pub pinned: Option<bool>,
    /// Move to this index.
    pub index: Option<u32>,
}

/// Event fired when a tab is updated.
#[derive(Debug, Clone)]
pub struct TabUpdateEvent {
    pub tab_id: TabId,
    pub change_info: TabChangeInfo,
    pub tab: Tab,
}

/// Describes what changed in a tab update event.
#[derive(Debug, Clone)]
pub struct TabChangeInfo {
    pub url: Option<Url>,
    pub status: Option<TabStatus>,
    pub title: Option<String>,
    pub fav_icon_url: Option<Url>,
    pub audible: Option<bool>,
    pub muted: Option<bool>,
    pub pinned: Option<bool>,
}

/// Information about why a tab was removed.
#[derive(Debug, Clone)]
pub struct RemovalInfo {
    pub window_id: WindowId,
    pub is_window_closing: bool,
}

/// Information about which tab is now active.
#[derive(Debug, Clone)]
pub struct ActiveInfo {
    pub tab_id: TabId,
    pub window_id: WindowId,
}

/// Options for tab capture.
#[derive(Debug, Clone)]
pub struct CaptureOptions {
    pub format: CaptureFormat,
    pub quality: Option<u8>, // 0-100
}

#[derive(Debug, Clone)]
pub enum CaptureFormat {
    Png,
    Jpeg,
    Webp,
}
```

### Storage Types

```rust
/// Keys to retrieve from storage.
#[derive(Debug, Clone)]
pub enum StorageGetKeys {
    /// A single key.
    Single(String),
    /// A list of keys.
    Multiple(Vec<String>),
    /// Get all items.
    All,
}

/// A map of key-value pairs for storage operations.
pub type StorageChanges = HashMap<String, serde_json::Value>;

/// Storage change event data.
#[derive(Debug, Clone)]
pub struct StorageChange {
    pub old_value: Option<serde_json::Value>,
    pub new_value: Option<serde_json::Value>,
}
```

### Runtime Types

```rust
/// A message passed between extension contexts.
/// Must be JSON-serializable (structured clone algorithm).
pub type RuntimeMessage = serde_json::Value;

/// Information about the sender of a message.
#[derive(Debug, Clone)]
pub struct MessageSender {
    pub tab_id: Option<TabId>,
    pub frame_id: Option<FrameId>,
    pub url: Option<Url>,
    /// The extension ID of the sender, if from an extension.
    pub extension_id: Option<ExtensionId>,
}

/// Frame identifier within a tab. 0 is the top-level frame.
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub struct FrameId(pub u32);

/// Information for establishing a port connection.
#[derive(Debug, Clone)]
pub struct ConnectInfo {
    pub extension_id: Option<ExtensionId>,
    pub name: Option<String>,
    /// Include TLS channel ID (for authentication).
    pub include_tls_channel_id: Option<bool>,
}

/// A long-lived communication port between extension contexts.
pub trait Port: Send + Sync {
    /// The port's name.
    fn name(&self) -> &str;

    /// The other end's extension ID, if applicable.
    fn sender(&self) -> &MessageSender;

    /// Post a message to the other end.
    fn post_message(&self, message: RuntimeMessage) -> Result<(), ExtensionError>;

    /// Disconnect the port.
    fn disconnect(&self);

    /// Register a listener for incoming messages.
    fn on_message(&self, callback: Box<dyn Fn(RuntimeMessage) + Send + Sync>);

    /// Register a listener for port disconnection.
    fn on_disconnect(&self, callback: Box<dyn Fn() + Send + Sync>);
}

/// Details about extension installation/update.
#[derive(Debug, Clone)]
pub struct InstalledDetails {
    pub reason: InstallReason,
    pub previous_version: Option<String>,
    pub id: ExtensionId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallReason {
    Install,
    Update,
    BrowserUpdate,
    SharedModuleUpdate,
}
```

### WebRequest Types

```rust
/// Filter for which requests to observe.
#[derive(Debug, Clone)]
pub struct RequestFilter {
    /// URL patterns to match (supports wildcards).
    pub urls: Vec<UrlPattern>,
    /// Resource types to match (main_frame, sub_frame, stylesheet, script, image, etc.).
    pub types: Option<Vec<ResourceType>>,
    /// Restrict to specific tab.
    pub tab_id: Option<TabId>,
    /// Restrict to specific window.
    pub window_id: Option<WindowId>,
}

/// URL pattern matching (e.g., "*://*.example.com/*").
#[derive(Debug, Clone)]
pub struct UrlPattern(pub String);

/// Resource types for request filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceType {
    MainFrame,
    SubFrame,
    Stylesheet,
    Script,
    Image,
    Font,
    Object,
    XmlHttpRequest,
    Ping,
    Media,
    Websocket,
    Other,
}

/// What extra information to include in request details.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtraInfoSpec {
    RequestHeaders,
    ResponseHeaders,
    Blocking,
}

/// Response from a blocking webRequest handler.
#[derive(Debug, Clone, Default)]
pub struct BlockingResponse {
    /// Cancel the request entirely.
    pub cancel: Option<bool>,
    /// Redirect to a different URL.
    pub redirect_url: Option<Url>,
    /// Modify request headers (for onBeforeSendHeaders).
    pub request_headers: Option<Vec<HttpHeader>>,
    /// Modify response headers (for onHeadersReceived).
    pub response_headers: Option<Vec<HttpHeader>>,
    /// Authentication credentials (for onAuthRequired).
    pub auth_credentials: Option<AuthCredentials>,
}

/// An HTTP header with name and value.
#[derive(Debug, Clone)]
pub struct HttpHeader {
    pub name: String,
    pub value: Option<String>, // None = remove header
}

/// Authentication credentials for onAuthRequired.
#[derive(Debug, Clone)]
pub struct AuthCredentials {
    pub username: String,
    pub password: String,
}

/// Details provided to onBeforeRequest handler.
#[derive(Debug, Clone)]
pub struct RequestDetails {
    pub request_id: RequestId,
    pub url: Url,
    pub method: String,
    pub frame_id: FrameId,
    pub parent_frame_id: FrameId,
    pub tab_id: Option<TabId>,
    pub type_: ResourceType,
    pub origin_url: Option<Url>,
    pub timestamp: f64,
    /// Only present if ExtraInfoSpec::RequestHeaders requested.
    pub request_headers: Option<Vec<HttpHeader>>,
}

/// Details provided to onBeforeSendHeaders handler.
#[derive(Debug, Clone)]
pub struct BeforeSendHeadersDetails {
    pub request_id: RequestId,
    pub url: Url,
    pub method: String,
    pub frame_id: FrameId,
    pub tab_id: Option<TabId>,
    pub type_: ResourceType,
    pub request_headers: Vec<HttpHeader>,
}

/// Details provided to onHeadersReceived handler.
#[derive(Debug, Clone)]
pub struct HeadersReceivedDetails {
    pub request_id: RequestId,
    pub url: Url,
    pub status_line: String,
    pub status_code: u16,
    pub frame_id: FrameId,
    pub tab_id: Option<TabId>,
    pub type_: ResourceType,
    pub response_headers: Vec<HttpHeader>,
}

/// Details provided to onBeforeRedirect handler.
#[derive(Debug, Clone)]
pub struct RedirectDetails {
    pub request_id: RequestId,
    pub url: Url,
    pub from_url: Url,
    pub frame_id: FrameId,
    pub tab_id: Option<TabId>,
    pub type_: ResourceType,
    pub status_code: u32,
    pub redirect_url: Url,
}

/// Details provided to onCompleted handler.
#[derive(Debug, Clone)]
pub struct CompletedDetails {
    pub request_id: RequestId,
    pub url: Url,
    pub frame_id: FrameId,
    pub tab_id: Option<TabId>,
    pub type_: ResourceType,
    pub from_cache: bool,
    pub status_code: u16,
    pub ip: Option<std::net::IpAddr>,
    pub timestamp: f64,
}

/// Details provided to onErrorOccurred handler.
#[derive(Debug, Clone)]
pub struct ErrorOccurredDetails {
    pub request_id: RequestId,
    pub url: Url,
    pub frame_id: FrameId,
    pub tab_id: Option<TabId>,
    pub type_: ResourceType,
    pub error: String,
    pub timestamp: f64,
}

/// Details provided to onAuthRequired handler.
#[derive(Debug, Clone)]
pub struct AuthRequiredDetails {
    pub request_id: RequestId,
    pub url: Url,
    pub frame_id: FrameId,
    pub tab_id: Option<TabId>,
    pub type_: ResourceType,
    pub realm: Option<String>,
    pub challenger: AuthChallenger,
    pub is_proxy: bool,
}

#[derive(Debug, Clone)]
pub struct AuthChallenger {
    pub host: String,
    pub port: u16,
}

/// Unique request identifier.
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub struct RequestId(pub u64);
```

### Scripting Types

```rust
/// Target for script/CSS injection.
#[derive(Debug, Clone)]
pub struct InjectionTarget {
    /// The tab to inject into.
    pub tab_id: TabId,
    /// Specific frame IDs to inject into (None = all frames).
    pub frame_ids: Option<Vec<FrameId>>,
    /// Inject into all frames in the tab.
    pub all_frames: bool,
}

/// Script injection parameters.
#[derive(Debug, Clone)]
pub enum ScriptInjection {
    /// Inject a JavaScript function. The function receives `args` and
    /// runs in the page's JS context.
    Function {
        /// The function body as a string.
        func: String,
        /// Arguments to pass to the function.
        args: Vec<serde_json::Value>,
    },
    /// Inject a JavaScript file from the extension's directory.
    File {
        /// Path relative to the extension's root.
        file: String,
    },
}

/// CSS injection parameters.
#[derive(Debug, Clone)]
pub enum CssInjection {
    /// A CSS string to inject.
    Css { css: String },
    /// A CSS file from the extension's directory.
    File { file: String },
    /// Where to inject: "author" (page) or "user" (user stylesheet).
    origin: CssOrigin,
}

#[derive(Debug, Clone, Default)]
pub enum CssOrigin {
    #[default]
    Author,
    User,
}

/// Result of a script injection.
#[derive(Debug, Clone)]
pub struct InjectionResult {
    pub frame_id: FrameId,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
}

/// A dynamically registered content script.
#[derive(Debug, Clone)]
pub struct RegisteredContentScript {
    pub id: String,
    /// JavaScript files to inject.
    pub js: Vec<String>,
    /// CSS files to inject.
    pub css: Vec<String>,
    /// URL patterns for when to inject.
    pub matches: Vec<UrlPattern>,
    /// URL patterns for when NOT to inject.
    pub exclude_matches: Vec<UrlPattern>,
    /// When to inject relative to page load.
    pub run_at: RunAt,
    /// Inject into all frames (default: top frame only).
    pub all_frames: bool,
    /// Inject into about:, chrome:, etc. URLs.
    pub match_about_blank: bool,
}

#[derive(Debug, Clone, Default)]
pub enum RunAt {
    /// Before any CSS or DOM is constructed.
    DocumentIdle,
    /// After CSS is constructed but before other scripts run.
    #[default]
    DocumentStart,
    /// After the DOM is constructed and scripts have run.
    DocumentEnd,
}

/// Filter for querying registered content scripts.
#[derive(Debug, Clone)]
pub struct ScriptFilter {
    pub ids: Option<Vec<String>>,
}
```

### Manifest Types

```rust
/// Parsed extension manifest (manifest.json → Manifest V3).
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ExtensionManifest {
    pub manifest_version: u32,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub permissions: Vec<String>,
    pub optional_permissions: Option<Vec<String>>,
    pub host_permissions: Option<Vec<String>>,
    pub background: Option<Background>,
    pub content_scripts: Option<Vec<ContentScript>>,
    pub action: Option<Action>,
    pub options_page: Option<String>,
    pub options_ui: Option<OptionsUi>,
    pub web_accessible_resources: Option<Vec<String>>,
    pub commands: Option<HashMap<String, Command>>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Background {
    pub service_worker: Option<String>,
    pub scripts: Option<Vec<String>>,
    pub persistent: Option<bool>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ContentScript {
    pub matches: Vec<String>,
    pub js: Option<Vec<String>>,
    pub css: Option<Vec<String>>,
    pub run_at: Option<String>,
    pub all_frames: Option<bool>,
    pub match_about_blank: Option<bool>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Action {
    pub default_title: Option<String>,
    pub default_icon: Option<IconValue>,
    pub default_popup: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct OptionsUi {
    pub page: String,
    pub open_in_tab: Option<bool>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Command {
    pub description: Option<String>,
    pub suggested_key: Option<SuggestedKey>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct SuggestedKey {
    pub default: Option<String>,
    pub mac: Option<String>,
    pub windows: Option<String>,
    pub chromeos: Option<String>,
    pub linux: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(untagged)]
pub enum IconValue {
    Single(String),
    Sized(HashMap<String, String>),
}
```

### Error Types

```rust
/// Extension API error.
#[derive(Debug, Clone)]
pub enum ExtensionError {
    /// The API method is not supported in Aileron.
    Unsupported(String),
    /// The extension does not have the required permission.
    PermissionDenied(String),
    /// A required argument was missing or invalid.
    InvalidArgument(String),
    /// The target tab, window, or frame was not found.
    NotFound(String),
    /// A runtime error occurred.
    Runtime(String),
    /// JSON serialization/deserialization failed.
    Serialization(String),
}
```

---

## Integration with Existing Content Script System

### Current System

Aileron currently supports Lua-based content scripts:
- Scripts defined in `~/.config/aileron/scripts/`
- `@match` regex for URL matching
- `@run-at` lifecycle stages
- JS bridge for DOM manipulation
- Shared storage via Aileron's config/key-value store

### Migration Path

1. **Phase K.1** — Define the trait surface (this document). No changes to existing system.
2. **Phase L** — Implement `ScriptingApi` by wrapping the existing Lua content script
   infrastructure. Lua scripts continue to work; new extensions can use WebExtensions API.
3. **Phase M** — Add WebExtensions manifest parsing and extension loading. Existing Lua
   scripts can optionally declare a minimal manifest for compatibility.
4. **Phase N** — Deprecate raw Lua scripts in favor of manifest-declared content scripts.

### Bridge Architecture

```
Extension (JS)                Aileron (Rust)
─────────────────             ────────────────
browser.tabs.sendMessage()  →  TabsApi::send_message()
browser.webRequest.onB..()  →  WebRequestApi::on_before_request()
browser.scripting.exec..()  →  ScriptingApi::execute_script()
  ↓                              ↓
  via IPC (wry evaluate)         via trait dispatch
  ↓                              ↓
Content Script (JS)         ←  Content Script Bridge (Rust→JS)
```

The existing JS bridge (wry `evaluate_script`) serves as the transport layer.
The `ScriptingApi` wraps it with a typed interface.

---

## Ad Blocking via WebRequestApi

### How uBlock Origin Works

uBlock Origin uses `browser.webRequest.onBeforeRequest` with a `BlockingResponse`
to cancel or redirect ad/tracking requests. The filter list is compiled into a
lookup structure; each network request is checked against it.

### Aileron Integration

Aileron's existing ad blocker (domain-based + ABP filter lists) can be preserved
as the high-performance path, while the `WebRequestApi` provides the extension
compatibility layer:

```
Network Request
       │
       ▼
┌──────────────────┐
│ Built-in Ad Block │  ← Fast path: compiled filter list, no trait dispatch
│ (current system) │
└──────┬───────────┘
       │ (pass-through if not blocked)
       ▼
┌──────────────────┐
│ WebRequestApi    │  ← Extension hooks: uBlock Origin, Privacy Badger, etc.
│ on_before_request│
└──────┬───────────┘
       │
       ▼
   Actual Request
```

This two-tier approach means extensions don't pay for the built-in ad blocker's
overhead, and the built-in ad blocker doesn't pay for extension dispatch.

---

## Mapping to WebExtensions Spec

| Rust Trait              | WebExtensions API                    | Notes                                      |
|------------------------|--------------------------------------|--------------------------------------------|
| `ExtensionApi`         | `chrome` / `browser` global          | Top-level namespace                         |
| `TabsApi`              | `chrome.tabs`                        | All MVP methods                             |
| `StorageApi`           | `chrome.storage`                     | local, sync, managed areas                  |
| `StorageArea`          | `chrome.storage.local` etc.          | get/set/remove/clear                        |
| `RuntimeApi`           | `chrome.runtime`                     | Messaging + lifecycle                       |
| `WebRequestApi`        | `chrome.webRequest`                  | Blocking + non-blocking handlers            |
| `ScriptingApi`         | `chrome.scripting`                   | executeScript, insertCSS, registerContentScripts |
| `Tab`                  | `tabs.Tab`                           | Direct field mapping                        |
| `TabQuery`             | `tabs.query(queryInfo)`              | Optional fields = None                      |
| `RequestFilter`        | `webRequest.RequestFilter`           | urlPatterns + resourceTypes                 |
| `BlockingResponse`     | `webRequest.BlockingResponse`        | cancel / redirect / headers                 |
| `ExtensionManifest`    | `manifest.json`                      | Manifest V3 only                            |
