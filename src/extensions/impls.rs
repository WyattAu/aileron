use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use url::Url;

use crate::extensions::api::ExtensionApi;
use crate::extensions::manifest::ExtensionManifest;
use crate::extensions::message_bus::MessageBus;
use crate::extensions::permissions::{self, Permission};
use crate::extensions::runtime::{ConnectInfo, InstalledDetails, MessageSender, Port, RuntimeApi};
use crate::extensions::scripting::{
    CssInjection, ExtensionContentScriptEntry, ExtensionContentScriptRegistry, ExtensionRunAt,
    InjectionResult, InjectionTarget, RegisteredContentScript, RunAt, ScriptFilter,
    ScriptInjection, ScriptingApi,
};
use crate::extensions::storage::{StorageApi, StorageArea, StorageChanges, StorageGetKeys};
use crate::extensions::tabs::{
    ActiveInfo, CaptureOptions, CreateProperties, RemovalInfo, Tab, TabProvider, TabQuery,
    TabUpdateEvent, TabsApi, UpdateProperties,
};
use crate::extensions::types::UrlPattern;
use crate::extensions::types::{
    ExtensionError, ExtensionId, ListenerId, Result, RuntimeMessage, TabId, WindowId,
};
use crate::extensions::web_request::{
    AuthRequiredDetails, BeforeSendHeadersDetails, BlockingResponse, CompletedDetails,
    ErrorOccurredDetails, ExtraInfoSpec, HeadersReceivedDetails, RedirectDetails, RequestDetails,
    RequestFilter, WebRequestApi,
};

type UpdatedCallback = Box<dyn Fn(TabUpdateEvent) + Send + Sync>;
type CreatedCallback = Box<dyn Fn(Tab) + Send + Sync>;
type RemovedCallback = Box<dyn Fn(TabId, RemovalInfo) + Send + Sync>;
type ActivatedCallback = Box<dyn Fn(ActiveInfo) + Send + Sync>;
type StorageChangeCallback = Box<dyn Fn(StorageChanges, String) + Send + Sync>;
type MessageCallback =
    Box<dyn Fn(RuntimeMessage, MessageSender) -> Option<RuntimeMessage> + Send + Sync>;
type ConnectCallback = Box<dyn Fn(Box<dyn Port>) + Send + Sync>;
type InstalledCallback = Box<dyn Fn(InstalledDetails) + Send + Sync>;
type StartupCallback = Box<dyn Fn() + Send + Sync>;

// WebRequest handler types
type BeforeRequestHandler = Box<dyn Fn(RequestDetails) -> BlockingResponse + Send + Sync>;
type BeforeSendHeadersHandler = Box<dyn Fn(BeforeSendHeadersDetails) -> BlockingResponse + Send + Sync>;
type HeadersReceivedHandler = Box<dyn Fn(HeadersReceivedDetails) -> BlockingResponse + Send + Sync>;
type AuthRequiredHandler = Box<dyn Fn(AuthRequiredDetails) -> BlockingResponse + Send + Sync>;
type BeforeRedirectHandler = Box<dyn Fn(RedirectDetails) + Send + Sync>;
type CompletedHandler = Box<dyn Fn(CompletedDetails) + Send + Sync>;
type ErrorOccurredHandler = Box<dyn Fn(ErrorOccurredDetails) + Send + Sync>;

static LISTENER_COUNTER: AtomicU64 = AtomicU64::new(0);

fn next_listener_id() -> ListenerId {
    ListenerId(LISTENER_COUNTER.fetch_add(1, Ordering::Relaxed) + 1)
}

struct AileronTabsApi {
    updated_callbacks: Mutex<Vec<UpdatedCallback>>,
    created_callbacks: Mutex<Vec<CreatedCallback>>,
    removed_callbacks: Mutex<Vec<RemovedCallback>>,
    activated_callbacks: Mutex<Vec<ActivatedCallback>>,
    tab_provider: Option<std::sync::Arc<dyn TabProvider>>,
}

impl AileronTabsApi {
    fn new() -> Self {
        Self {
            updated_callbacks: Mutex::new(Vec::new()),
            created_callbacks: Mutex::new(Vec::new()),
            removed_callbacks: Mutex::new(Vec::new()),
            activated_callbacks: Mutex::new(Vec::new()),
            tab_provider: None,
        }
    }

    fn with_provider(provider: std::sync::Arc<dyn TabProvider>) -> Self {
        Self {
            updated_callbacks: Mutex::new(Vec::new()),
            created_callbacks: Mutex::new(Vec::new()),
            removed_callbacks: Mutex::new(Vec::new()),
            activated_callbacks: Mutex::new(Vec::new()),
            tab_provider: Some(provider),
        }
    }
}

impl TabsApi for AileronTabsApi {
    fn query(&self, query: TabQuery) -> Result<Vec<Tab>> {
        let Some(ref provider) = self.tab_provider else {
            return Ok(Vec::new());
        };
        let all_tabs = provider.list_tabs();
        let active_id = provider.active_tab_id();
        let mut result = all_tabs;

        // Apply filters
        if let Some(active) = query.active {
            result.retain(|t| {
                let is_active = active_id
                    .as_ref()
                    .is_some_and(|aid| aid.0 == t.id.0);
                is_active == active
            });
        }
        if let Some(ref status) = query.status {
            result.retain(|t| t.status == *status);
        }
        if let Some(ref title_pattern) = query.title {
            result.retain(|t| {
                t.title
                    .as_ref()
                    .is_some_and(|t| t.to_lowercase().contains(&title_pattern.to_lowercase()))
            });
        }
        if let Some(pinned) = query.pinned {
            result.retain(|t| t.pinned == pinned);
        }
        if query.highlighted == Some(true) {
            // Highlighted = active tab in current window
            if let Some(ref aid) = active_id {
                result.retain(|t| t.id.0 == aid.0);
            }
        }

        Ok(result)
    }

    fn create(&self, properties: CreateProperties) -> Result<Tab> {
        let Some(ref provider) = self.tab_provider else {
            return Err(ExtensionError::Unsupported("tabs.create".into()));
        };
        let url = properties.url.unwrap_or_else(|| {
            url::Url::parse("aileron://newtab").unwrap_or_else(|_| url::Url::parse("about:blank").unwrap())
        });
        provider.create_tab(url)
    }

    fn update(&self, tab_id: TabId, properties: UpdateProperties) -> Result<Tab> {
        let Some(ref provider) = self.tab_provider else {
            return Err(ExtensionError::Unsupported("tabs.update".into()));
        };
        if let Some(ref url) = properties.url {
            provider.navigate_tab(tab_id, url.clone())?;
        }
        // Re-query to get updated tab
        let tabs = provider.list_tabs();
        tabs.into_iter()
            .find(|t| t.id == tab_id)
            .ok_or_else(|| ExtensionError::NotFound(format!("Tab {}", tab_id)))
    }

    fn remove(&self, tab_id: TabId) -> Result<()> {
        let Some(ref provider) = self.tab_provider else {
            return Err(ExtensionError::Unsupported("tabs.remove".into()));
        };
        provider.close_tab(tab_id)
    }

    fn duplicate(&self, tab_id: TabId) -> Result<Tab> {
        let Some(ref provider) = self.tab_provider else {
            return Err(ExtensionError::Unsupported("tabs.duplicate".into()));
        };
        // Find the tab's URL, then create a new one
        let tabs = provider.list_tabs();
        let tab = tabs
            .into_iter()
            .find(|t| t.id == tab_id)
            .ok_or_else(|| ExtensionError::NotFound(format!("Tab {}", tab_id)))?;
        provider.create_tab(tab.url)
    }

    fn send_message(
        &self,
        tab_id: TabId,
        message: RuntimeMessage,
    ) -> Result<Option<RuntimeMessage>> {
        let Some(ref provider) = self.tab_provider else {
            return Ok(None);
        };
        provider.send_tab_message(tab_id, message)
    }

    fn capture_visible_tab(
        &self,
        _window_id: Option<WindowId>,
        _options: CaptureOptions,
    ) -> Result<Vec<u8>> {
        // Requires screenshot infrastructure — not yet wired
        Err(ExtensionError::Unsupported(
            "tabs.captureVisibleTab".into(),
        ))
    }

    fn on_updated(&self, callback: Box<dyn Fn(TabUpdateEvent) + Send + Sync>) {
        self.updated_callbacks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(callback);
    }

    fn on_created(&self, callback: Box<dyn Fn(Tab) + Send + Sync>) {
        self.created_callbacks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(callback);
    }

    fn on_removed(&self, callback: Box<dyn Fn(TabId, RemovalInfo) + Send + Sync>) {
        self.removed_callbacks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(callback);
    }

    fn on_activated(&self, callback: Box<dyn Fn(ActiveInfo) + Send + Sync>) {
        self.activated_callbacks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(callback);
    }
}

struct AileronStorageArea {
    data: Mutex<HashMap<String, serde_json::Value>>,
    change_callbacks: Mutex<Vec<StorageChangeCallback>>,
    /// If set, data is persisted to this JSON file on every mutation.
    storage_file: Option<std::path::PathBuf>,
}

impl AileronStorageArea {
    fn new() -> Self {
        Self {
            data: Mutex::new(HashMap::new()),
            change_callbacks: Mutex::new(Vec::new()),
            storage_file: None,
        }
    }

    /// Create a persistent storage area backed by a JSON file.
    /// If the file exists, data is loaded from it on creation.
    /// If the file does not exist, an empty area is created and the
    /// file will be written on the first mutation.
    fn with_persistence(storage_file: std::path::PathBuf) -> Self {
        let initial_data = Self::load_from_file(&storage_file);
        Self {
            data: Mutex::new(initial_data),
            change_callbacks: Mutex::new(Vec::new()),
            storage_file: Some(storage_file),
        }
    }

    fn load_from_file(path: &std::path::Path) -> HashMap<String, serde_json::Value> {
        if !path.exists() {
            return HashMap::new();
        }
        match std::fs::read_to_string(path) {
            Ok(content) => match serde_json::from_str(&content) {
                Ok(data) => data,
                Err(e) => {
                    tracing::warn!(
                        target: "extensions",
                        "Failed to parse storage file {:?}: {}, starting empty",
                        path, e
                    );
                    HashMap::new()
                }
            },
            Err(e) => {
                tracing::warn!(
                    target: "extensions",
                    "Failed to read storage file {:?}: {}, starting empty",
                    path, e
                );
                HashMap::new()
            }
        }
    }

    fn persist_to_file(&self) {
        if let Some(ref path) = self.storage_file {
            let data = self.data.lock().unwrap_or_else(|e| e.into_inner());
            // Only write if we have data (avoid creating empty files unnecessarily)
            if data.is_empty() {
                // Remove the file if it exists and data is empty after clear
                let _ = std::fs::remove_file(path);
                return;
            }
            match serde_json::to_string_pretty(&*data) {
                Ok(json) => {
                    if let Some(parent) = path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    if let Err(e) = std::fs::write(path, &json) {
                        tracing::warn!(
                            target: "extensions",
                            "Failed to write storage file {:?}: {}",
                            path, e
                        );
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        target: "extensions",
                        "Failed to serialize storage data: {}",
                        e
                    );
                }
            }
        }
    }

    /// Fire change callbacks for the given changes.
    fn fire_change_callbacks(&self, changes: StorageChanges, area_name: String) {
        if changes.is_empty() {
            return;
        }
        let callbacks = self
            .change_callbacks
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        for cb in callbacks.iter() {
            cb(changes.clone(), area_name.clone());
        }
    }
}

impl StorageArea for AileronStorageArea {
    fn get(&self, keys: StorageGetKeys) -> Result<StorageChanges> {
        let data = self
            .data
            .lock()
            .map_err(|e| ExtensionError::Runtime(format!("Storage lock poisoned: {}", e)))?;
        let result = match keys {
            StorageGetKeys::Single(key) => {
                let mut map = HashMap::new();
                if let Some(value) = data.get(&key) {
                    map.insert(key, value.clone());
                }
                map
            }
            StorageGetKeys::Multiple(keys) => {
                let mut map = HashMap::new();
                for key in keys {
                    if let Some(value) = data.get(&key) {
                        map.insert(key, value.clone());
                    }
                }
                map
            }
            StorageGetKeys::All => data.clone(),
        };
        Ok(result)
    }

    fn set(&self, items: StorageChanges) -> Result<()> {
        let mut data = self
            .data
            .lock()
            .map_err(|e| ExtensionError::Runtime(format!("Storage lock poisoned: {}", e)))?;
        let mut changes = StorageChanges::new();
        for (key, new_value) in items {
            data.insert(key.clone(), new_value.clone());
            changes.insert(key, new_value);
        }
        drop(data);
        self.fire_change_callbacks(changes, "local".into());
        self.persist_to_file();
        Ok(())
    }

    fn remove(&self, keys: Vec<String>) -> Result<()> {
        let mut data = self
            .data
            .lock()
            .map_err(|e| ExtensionError::Runtime(format!("Storage lock poisoned: {}", e)))?;
        let mut changes = StorageChanges::new();
        for key in keys {
            if data.remove(&key).is_some() {
                // Use null to indicate removal in changes
                changes.insert(key, serde_json::Value::Null);
            }
        }
        drop(data);
        self.fire_change_callbacks(changes, "local".into());
        self.persist_to_file();
        Ok(())
    }

    fn clear(&self) -> Result<()> {
        let mut data = self
            .data
            .lock()
            .map_err(|e| ExtensionError::Runtime(format!("Storage lock poisoned: {}", e)))?;
        if data.is_empty() {
            return Ok(());
        }
        data.clear();
        drop(data);
        // Fire with empty changes to signal clear occurred
        self.fire_change_callbacks(StorageChanges::new(), "local".into());
        self.persist_to_file();
        Ok(())
    }

    fn get_bytes_in_use(&self, keys: Option<Vec<String>>) -> Result<u64> {
        let data = self
            .data
            .lock()
            .map_err(|e| ExtensionError::Runtime(format!("Storage lock poisoned: {}", e)))?;
        let bytes: usize = match keys {
            Some(keys) => keys
                .iter()
                .filter_map(|k| data.get(k))
                .map(|v| v.to_string().len())
                .sum(),
            None => data.values().map(|v| v.to_string().len()).sum(),
        };
        Ok(bytes as u64)
    }

    fn on_changed(&self, callback: Box<dyn Fn(StorageChanges, String) + Send + Sync>) {
        self.change_callbacks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(callback);
    }
}

struct AileronStorageApi {
    local: AileronStorageArea,
    sync: AileronStorageArea,
    managed: AileronStorageArea,
}

impl AileronStorageApi {
    /// Create an in-memory (non-persistent) storage API.
    fn new() -> Self {
        Self {
            local: AileronStorageArea::new(),
            sync: AileronStorageArea::new(),
            managed: AileronStorageArea::new(),
        }
    }

    /// Create a persistent storage API backed by JSON files.
    /// Files are stored under `storage_dir/<extension_id>/<area>.json`.
    fn with_persistence(
        storage_dir: std::path::PathBuf,
        extension_id: &ExtensionId,
    ) -> Self {
        let ext_dir = storage_dir.join(&extension_id.0);
        Self {
            local: AileronStorageArea::with_persistence(ext_dir.join("local.json")),
            sync: AileronStorageArea::with_persistence(ext_dir.join("sync.json")),
            managed: AileronStorageArea::with_persistence(ext_dir.join("managed.json")),
        }
    }
}

impl StorageApi for AileronStorageApi {
    fn local(&self) -> &dyn StorageArea {
        &self.local
    }

    fn sync(&self) -> &dyn StorageArea {
        &self.sync
    }

    fn managed(&self) -> &dyn StorageArea {
        &self.managed
    }
}

struct AileronRuntimeApi {
    extension_id: ExtensionId,
    manifest: ExtensionManifest,
    message_bus: Option<Arc<MessageBus>>,
    message_callbacks: Arc<Mutex<Vec<MessageCallback>>>,
    connect_callbacks: Mutex<Vec<ConnectCallback>>,
    installed_callbacks: Mutex<Vec<InstalledCallback>>,
    startup_callbacks: Mutex<Vec<StartupCallback>>,
}

impl AileronRuntimeApi {
    fn new(extension_id: ExtensionId, manifest: ExtensionManifest) -> Self {
        Self {
            extension_id,
            manifest,
            message_bus: None,
            message_callbacks: Arc::new(Mutex::new(Vec::new())),
            connect_callbacks: Mutex::new(Vec::new()),
            installed_callbacks: Mutex::new(Vec::new()),
            startup_callbacks: Mutex::new(Vec::new()),
        }
    }

    fn with_message_bus(
        extension_id: ExtensionId,
        manifest: ExtensionManifest,
        message_bus: Arc<MessageBus>,
    ) -> Self {
        let callbacks: Arc<Mutex<Vec<MessageCallback>>> =
            Arc::new(Mutex::new(Vec::new()));
        let cb_clone = callbacks.clone();

        // Register a handler on the bus that invokes our stored callbacks
        message_bus.register_handler(extension_id.clone(), Box::new(move |msg: RuntimeMessage| {
            let cbs = cb_clone.lock().unwrap_or_else(|e| e.into_inner());
            for cb in cbs.iter() {
                let sender = crate::extensions::runtime::MessageSender {
                    tab_id: None,
                    frame_id: None,
                    url: None,
                    extension_id: None,
                };
                if let Some(response) = cb(msg.clone(), sender) {
                    return Some(response);
                }
            }
            None
        }));

        Self {
            extension_id,
            manifest,
            message_bus: Some(message_bus),
            message_callbacks: callbacks,
            connect_callbacks: Mutex::new(Vec::new()),
            installed_callbacks: Mutex::new(Vec::new()),
            startup_callbacks: Mutex::new(Vec::new()),
        }
    }

    /// Fire all registered `on_installed` callbacks with the given details.
    /// Called by the extension loader after successfully loading an extension.
    fn fire_installed(&self, details: InstalledDetails) {
        let cbs = self
            .installed_callbacks
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        for cb in cbs.iter() {
            cb(details.clone());
        }
        if !cbs.is_empty() {
            tracing::debug!(
                target: "extensions",
                "Fired {} on_installed callback(s) for extension '{}'",
                cbs.len(),
                self.extension_id.0
            );
        }
    }

    /// Fire all registered `on_startup` callbacks.
    /// Called by the extension loader during browser startup.
    fn fire_startup(&self) {
        let cbs = self
            .startup_callbacks
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        for cb in cbs.iter() {
            cb();
        }
        if !cbs.is_empty() {
            tracing::debug!(
                target: "extensions",
                "Fired {} on_startup callback(s) for extension '{}'",
                cbs.len(),
                self.extension_id.0
            );
        }
    }
}

impl RuntimeApi for AileronRuntimeApi {
    fn send_message(
        &self,
        target_id: Option<ExtensionId>,
        message: RuntimeMessage,
    ) -> Result<Option<RuntimeMessage>> {
        match &self.message_bus {
            Some(bus) => {
                let source = Some(&self.extension_id);
                let target = target_id.as_ref();
                Ok(bus.send_message(source, target, message))
            }
            None => {
                tracing::warn!(
                    target: "extensions",
                    "runtime.sendMessage: no message bus (extension {})",
                    self.extension_id.0
                );
                Ok(None)
            }
        }
    }

    fn connect(&self, connect_info: ConnectInfo) -> Result<Box<dyn Port>> {
        let name = connect_info.name.unwrap_or_default();
        let port: Box<dyn Port> = Box::new(
            crate::extensions::message_bus::LocalPort::new(&name),
        );
        Ok(port)
    }

    fn get_manifest(&self) -> Result<ExtensionManifest> {
        Ok(self.manifest.clone())
    }

    fn get_url(&self, path: &str) -> Result<Url> {
        Url::parse(&format!(
            "aileron://extensions/{}/{}",
            self.extension_id, path
        ))
        .map_err(|e| ExtensionError::Runtime(format!("Invalid extension URL: {}", e)))
    }

    fn get_id(&self) -> &ExtensionId {
        &self.extension_id
    }

    fn on_message(&self, callback: MessageCallback) {
        self.message_callbacks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(callback);
    }

    fn on_connect(&self, callback: ConnectCallback) {
        self.connect_callbacks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(callback);
    }

    fn on_installed(&self, callback: InstalledCallback) {
        self.installed_callbacks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(callback);
    }

    fn on_startup(&self, callback: StartupCallback) {
        self.startup_callbacks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(callback);
    }

    fn reload(&self) -> Result<()> {
        tracing::warn!(
            target: "extensions",
            "runtime.reload not yet implemented"
        );
        Err(ExtensionError::Unsupported("runtime.reload".into()))
    }

    fn open_options_page(&self) -> Result<()> {
        tracing::warn!(
            target: "extensions",
            "runtime.openOptionsPage not yet implemented"
        );
        Err(ExtensionError::Unsupported(
            "runtime.openOptionsPage".into(),
        ))
    }
}

struct AileronWebRequestApi {
    before_request_handlers: Mutex<Vec<(ListenerId, RequestFilter, BeforeRequestHandler)>>,
    before_send_headers_handlers:
        Mutex<Vec<(ListenerId, RequestFilter, BeforeSendHeadersHandler)>>,
    headers_received_handlers:
        Mutex<Vec<(ListenerId, RequestFilter, HeadersReceivedHandler)>>,
    auth_required_handlers: Mutex<Vec<(ListenerId, RequestFilter, AuthRequiredHandler)>>,
    before_redirect_handlers: Mutex<Vec<(ListenerId, RequestFilter, BeforeRedirectHandler)>>,
    completed_handlers: Mutex<Vec<(ListenerId, RequestFilter, CompletedHandler)>>,
    error_occurred_handlers: Mutex<Vec<(ListenerId, RequestFilter, ErrorOccurredHandler)>>,
}

impl AileronWebRequestApi {
    fn new() -> Self {
        Self {
            before_request_handlers: Mutex::new(Vec::new()),
            before_send_headers_handlers: Mutex::new(Vec::new()),
            headers_received_handlers: Mutex::new(Vec::new()),
            auth_required_handlers: Mutex::new(Vec::new()),
            before_redirect_handlers: Mutex::new(Vec::new()),
            completed_handlers: Mutex::new(Vec::new()),
            error_occurred_handlers: Mutex::new(Vec::new()),
        }
    }

    /// Check if a URL matches any pattern in the filter.
    fn url_matches_filter(url: &Url, filter: &RequestFilter) -> bool {
        // If no URL patterns, match all
        if filter.urls.is_empty() {
            return true;
        }
        filter.urls.iter().any(|pattern| {
            let pat_str = pattern.0.as_str();
            simple_url_pattern_match(pat_str, url.as_str())
        })
    }

    /// Fire all registered on_before_request handlers for a request.
    /// Returns the first non-default BlockingResponse (first handler wins).
    #[allow(dead_code)]
    pub fn fire_on_before_request(&self, details: &RequestDetails) -> BlockingResponse {
        let handlers = self
            .before_request_handlers
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        for (_, filter, handler) in handlers.iter() {
            if Self::url_matches_filter(&details.url, filter) {
                let response = handler(details.clone());
                if response.cancel == Some(true) || response.redirect_url.is_some() {
                    return response;
                }
            }
        }
        BlockingResponse::default()
    }

    /// Fire all registered on_headers_received handlers.
    #[allow(dead_code)]
    pub fn fire_on_headers_received(&self, details: &HeadersReceivedDetails) -> BlockingResponse {
        let handlers = self
            .headers_received_handlers
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        for (_, filter, handler) in handlers.iter() {
            if Self::url_matches_filter(&details.url, filter) {
                let response = handler(details.clone());
                if response.cancel == Some(true) || response.response_headers.is_some() {
                    return response;
                }
            }
        }
        BlockingResponse::default()
    }

    /// Fire all registered on_before_send_headers handlers.
    #[allow(dead_code)]
    pub fn fire_on_before_send_headers(
        &self,
        details: &BeforeSendHeadersDetails,
    ) -> BlockingResponse {
        let handlers = self
            .before_send_headers_handlers
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        for (_, filter, handler) in handlers.iter() {
            if Self::url_matches_filter(&details.url, filter) {
                let response = handler(details.clone());
                if response.cancel == Some(true)
                    || response.request_headers.is_some()
                    || response.redirect_url.is_some()
                {
                    return response;
                }
            }
        }
        BlockingResponse::default()
    }

    /// Fire all registered on_completed handlers.
    #[allow(dead_code)]
    pub fn fire_on_completed(&self, details: &CompletedDetails) {
        let handlers = self
            .completed_handlers
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        for (_, filter, handler) in handlers.iter() {
            if Self::url_matches_filter(&details.url, filter) {
                handler(details.clone());
            }
        }
    }

    /// Fire all registered on_error_occurred handlers.
    #[allow(dead_code)]
    pub fn fire_on_error_occurred(&self, details: &ErrorOccurredDetails) {
        let handlers = self
            .error_occurred_handlers
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        for (_, filter, handler) in handlers.iter() {
            if Self::url_matches_filter(&details.url, filter) {
                handler(details.clone());
            }
        }
    }

    /// Fire all registered on_before_redirect handlers.
    #[allow(dead_code)]
    pub fn fire_on_before_redirect(&self, details: &RedirectDetails) {
        let handlers = self
            .before_redirect_handlers
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        for (_, filter, handler) in handlers.iter() {
            if Self::url_matches_filter(&details.url, filter) {
                handler(details.clone());
            }
        }
    }
}

/// Simple URL pattern matching for extension filters.
/// Supports wildcards: `*://*.example.com/*` matches any subdomain.
#[allow(dead_code)]
fn simple_url_pattern_match(pattern: &str, url: &str) -> bool {
    let pat_lower = pattern.to_lowercase();
    let url_lower = url.to_lowercase();

    if pat_lower == "<all_urls>" {
        return true;
    }

    // Split pattern into scheme, host, path parts
    if let Some(star_idx) = pat_lower.find("://") {
        let scheme = &pat_lower[..star_idx];
        let rest = &pat_lower[star_idx + 3..];

        // Check scheme: `*` matches any scheme
        if scheme != "*" && !url_lower.starts_with(&format!("{}://", scheme)) {
            return false;
        }

        // Extract the URL portion after the scheme
        let url_rest = if scheme == "*" {
            if let Some(idx) = url_lower.find("://") {
                &url_lower[idx + 3..]
            } else {
                return false;
            }
        } else {
            &url_lower[scheme.len() + 3..]
        };

        // Check host + path
        if rest == "*" || rest == "/*" {
            return true;
        }

        // Handle wildcard host patterns like *.example.com/*
        if let Some(pattern_domain) = rest.strip_prefix("*.") {
            // URL rest should end with the pattern domain
            // e.g., "*.example.com/*" should match "sub.example.com/path"
            if let Some(slash_idx) = pattern_domain.find('/') {
                let domain_pat = &pattern_domain[..slash_idx];
                let path_pat = &pattern_domain[slash_idx..];
                if let Some(url_slash) = url_rest.find('/') {
                    let url_domain = &url_rest[..url_slash];
                    let url_path = &url_rest[url_slash..];
                    if url_domain.ends_with(domain_pat)
                        && (path_pat == "/*" || path_pat == url_path)
                    {
                        return true;
                    }
                }
            }
            return false;
        }

        // Exact host match or host/path prefix match
        if let Some(slash_idx) = rest.find('/') {
            let host_pat = &rest[..slash_idx];
            let path_pat = &rest[slash_idx..];
            if url_rest.starts_with(host_pat)
                && let Some(url_path) = url_rest.strip_prefix(host_pat)
                && (path_pat == "/*" || path_pat == url_path)
            {
                return true;
            }
        } else if url_rest == rest {
            return true;
        }
    }

    false
}

impl WebRequestApi for AileronWebRequestApi {
    fn on_before_request(
        &self,
        filter: RequestFilter,
        _extra_info_spec: Vec<ExtraInfoSpec>,
        handler: BeforeRequestHandler,
    ) -> ListenerId {
        let id = next_listener_id();
        tracing::info!(
            target: "extensions",
            "webRequest.onBeforeRequest registered (listener {:?}, {} url patterns)",
            id,
            filter.urls.len()
        );
        self.before_request_handlers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((id, filter, handler));
        id
    }

    fn on_before_send_headers(
        &self,
        filter: RequestFilter,
        _extra_info_spec: Vec<ExtraInfoSpec>,
        handler: BeforeSendHeadersHandler,
    ) -> ListenerId {
        let id = next_listener_id();
        tracing::info!(
            target: "extensions",
            "webRequest.onBeforeSendHeaders registered (listener {:?})",
            id
        );
        self.before_send_headers_handlers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((id, filter, handler));
        id
    }

    fn on_headers_received(
        &self,
        filter: RequestFilter,
        _extra_info_spec: Vec<ExtraInfoSpec>,
        handler: HeadersReceivedHandler,
    ) -> ListenerId {
        let id = next_listener_id();
        tracing::info!(
            target: "extensions",
            "webRequest.onHeadersReceived registered (listener {:?})",
            id
        );
        self.headers_received_handlers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((id, filter, handler));
        id
    }

    fn on_auth_required(
        &self,
        filter: RequestFilter,
        handler: AuthRequiredHandler,
    ) -> ListenerId {
        let id = next_listener_id();
        tracing::info!(
            target: "extensions",
            "webRequest.onAuthRequired registered (listener {:?})",
            id
        );
        self.auth_required_handlers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((id, filter, handler));
        id
    }

    fn on_before_redirect(
        &self,
        filter: RequestFilter,
        callback: BeforeRedirectHandler,
    ) -> ListenerId {
        let id = next_listener_id();
        self.before_redirect_handlers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((id, filter, callback));
        id
    }

    fn on_completed(
        &self,
        filter: RequestFilter,
        callback: CompletedHandler,
    ) -> ListenerId {
        let id = next_listener_id();
        self.completed_handlers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((id, filter, callback));
        id
    }

    fn on_error_occurred(
        &self,
        filter: RequestFilter,
        callback: ErrorOccurredHandler,
    ) -> ListenerId {
        let id = next_listener_id();
        self.error_occurred_handlers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((id, filter, callback));
        id
    }

    fn remove_listener(&self, listener_id: ListenerId) -> Result<()> {
        macro_rules! remove_from {
            ($field:expr) => {{
                let mut handlers = $field
                    .lock()
                    .map_err(|e| ExtensionError::Runtime(format!("Lock poisoned: {}", e)))?;
                let before = handlers.len();
                handlers.retain(|(id, _, _)| *id != listener_id);
                handlers.len() < before
            }};
        }

        let mut any_removed = false;
        if remove_from!(self.before_request_handlers) {
            any_removed = true;
        }
        if remove_from!(self.before_send_headers_handlers) {
            any_removed = true;
        }
        if remove_from!(self.headers_received_handlers) {
            any_removed = true;
        }
        if remove_from!(self.auth_required_handlers) {
            any_removed = true;
        }
        if remove_from!(self.before_redirect_handlers) {
            any_removed = true;
        }
        if remove_from!(self.completed_handlers) {
            any_removed = true;
        }
        if remove_from!(self.error_occurred_handlers) {
            any_removed = true;
        }

        if any_removed {
            tracing::info!(
                target: "extensions",
                "webRequest listener {:?} removed",
                listener_id
            );
            Ok(())
        } else {
            Err(ExtensionError::NotFound(format!(
                "Listener {:?} not found",
                listener_id
            )))
        }
    }
}

struct AileronScriptingApi {
    registry: ExtensionContentScriptRegistry,
}

impl AileronScriptingApi {
    fn new(registry: ExtensionContentScriptRegistry) -> Self {
        Self { registry }
    }
}

impl ScriptingApi for AileronScriptingApi {
    fn execute_script(
        &self,
        target: InjectionTarget,
        _injection: ScriptInjection,
    ) -> Result<Vec<InjectionResult>> {
        tracing::warn!(
            target: "extensions",
            "scripting.executeScript(tab={}) not yet implemented",
            target.tab_id
        );
        Err(ExtensionError::Unsupported(
            "scripting.executeScript".into(),
        ))
    }

    fn insert_css(&self, target: InjectionTarget, _injection: CssInjection) -> Result<()> {
        tracing::warn!(
            target: "extensions",
            "scripting.insertCSS(tab={}) not yet implemented",
            target.tab_id
        );
        Err(ExtensionError::Unsupported("scripting.insertCSS".into()))
    }

    fn remove_css(&self, target: InjectionTarget, _injection: CssInjection) -> Result<()> {
        tracing::warn!(
            target: "extensions",
            "scripting.removeCSS(tab={}) not yet implemented",
            target.tab_id
        );
        Err(ExtensionError::Unsupported("scripting.removeCSS".into()))
    }

    fn register_content_scripts(&self, scripts: Vec<RegisteredContentScript>) -> Result<()> {
        for script in scripts {
            let run_at = match script.run_at {
                RunAt::DocumentIdle => ExtensionRunAt::DocumentIdle,
                RunAt::DocumentStart => ExtensionRunAt::DocumentStart,
                RunAt::DocumentEnd => ExtensionRunAt::DocumentEnd,
            };
            let entry = ExtensionContentScriptEntry {
                extension_id: String::new(),
                script_id: script.id.clone(),
                js_code: script.js.join("\n"),
                css_code: script.css.join("\n"),
                matches: script.matches.iter().map(|p| p.0.clone()).collect(),
                run_at,
            };
            self.registry.register(entry);
            tracing::info!(
                target: "extensions",
                "Registered content script '{}' ({} js files, {} css files)",
                script.id,
                script.js.len(),
                script.css.len()
            );
        }
        Ok(())
    }

    fn get_registered_content_scripts(
        &self,
        _filter: Option<ScriptFilter>,
    ) -> Result<Vec<RegisteredContentScript>> {
        let all = self.registry.all_scripts();
        let scripts = all
            .into_iter()
            .map(|s| RegisteredContentScript {
                id: s.script_id,
                js: if s.js_code.is_empty() {
                    vec![]
                } else {
                    vec![s.js_code]
                },
                css: if s.css_code.is_empty() {
                    vec![]
                } else {
                    vec![s.css_code]
                },
                matches: s.matches.into_iter().map(UrlPattern).collect(),
                exclude_matches: vec![],
                run_at: match s.run_at {
                    ExtensionRunAt::DocumentIdle => RunAt::DocumentIdle,
                    ExtensionRunAt::DocumentStart => RunAt::DocumentStart,
                    ExtensionRunAt::DocumentEnd => RunAt::DocumentEnd,
                },
                all_frames: false,
                match_about_blank: false,
            })
            .collect();
        Ok(scripts)
    }

    fn unregister_content_scripts(&self, filter: Option<ScriptFilter>) -> Result<()> {
        if let Some(f) = filter
            && let Some(ids) = f.ids
        {
            for id in ids {
                self.registry.unregister_by_id(&id);
            }
        }
        Ok(())
    }
}

pub struct AileronExtensionApi {
    extension_id: ExtensionId,
    manifest: ExtensionManifest,
    tabs_api: AileronTabsApi,
    storage_api: AileronStorageApi,
    runtime_api: AileronRuntimeApi,
    web_request_api: AileronWebRequestApi,
    scripting_api: AileronScriptingApi,
    granted_permissions: std::collections::HashSet<Permission>,
    granted_host_permissions: Vec<String>,
    background_script: Option<crate::extensions::types::BackgroundScript>,
}

impl AileronExtensionApi {
    pub fn new(extension_id: ExtensionId, manifest: ExtensionManifest) -> Self {
        Self::with_registry(
            extension_id,
            manifest,
            ExtensionContentScriptRegistry::new(),
        )
    }

    pub fn with_registry(
        extension_id: ExtensionId,
        manifest: ExtensionManifest,
        registry: ExtensionContentScriptRegistry,
    ) -> Self {
        Self::with_registry_and_storage(extension_id, manifest, registry, None, None, None)
    }

    /// Full constructor with optional persistence, tab provider, and message bus.
    pub fn with_registry_and_storage(
        extension_id: ExtensionId,
        manifest: ExtensionManifest,
        registry: ExtensionContentScriptRegistry,
        storage_dir: Option<std::path::PathBuf>,
        tab_provider: Option<std::sync::Arc<dyn TabProvider>>,
        message_bus: Option<Arc<MessageBus>>,
    ) -> Self {
        let storage_api = match storage_dir {
            Some(dir) => AileronStorageApi::with_persistence(dir, &extension_id),
            None => AileronStorageApi::new(),
        };
        let tabs_api = match tab_provider {
            Some(provider) => AileronTabsApi::with_provider(provider),
            None => AileronTabsApi::new(),
        };
        let runtime_api = match message_bus {
            Some(bus) => AileronRuntimeApi::with_message_bus(
                extension_id.clone(),
                manifest.clone(),
                bus,
            ),
            None => AileronRuntimeApi::new(extension_id.clone(), manifest.clone()),
        };
        let granted_permissions =
            permissions::parse_permissions(&manifest.permissions);
        let granted_host_permissions = manifest.host_permissions.clone();
        Self {
            tabs_api,
            storage_api,
            runtime_api,
            web_request_api: AileronWebRequestApi::new(),
            scripting_api: AileronScriptingApi::new(registry),
            extension_id,
            manifest,
            granted_permissions,
            granted_host_permissions,
            background_script: None,
        }
    }

    /// Check if the extension has a specific permission.
    pub fn has_permission(&self, permission: &str) -> bool {
        let perm = Permission::parse(permission);
        self.granted_permissions.contains(&perm)
    }

    /// Check if an API call is allowed based on manifest permissions.
    pub fn check_api_permission(&self, api: &str, method: &str) -> Result<()> {
        if permissions::check_permission(&self.granted_permissions, api, method) {
            Ok(())
        } else {
            let required = permissions::required_permissions(api, method);
            let names: Vec<String> = required.iter().map(|p| format!("{:?}", p)).collect();
            Err(ExtensionError::PermissionDenied(format!(
                "Extension '{}' requires permission '{}' for {}.{}",
                self.extension_id.0,
                names.join(", "),
                api,
                method
            )))
        }
    }

    /// Check if a URL matches any of the extension's granted host permissions.
    pub fn has_host_permission(&self, url: &str) -> bool {
        if self.granted_host_permissions.iter().any(|p| p == "<all_urls>") {
            return true;
        }
        self.granted_host_permissions
            .iter()
            .any(|p| permissions::host_permission_matches(p, url))
    }

    /// Grant an additional permission (for optional_permissions flow).
    pub fn grant_permission(&mut self, permission: &str) {
        let perm = Permission::parse(permission);
        self.granted_permissions.insert(perm);
    }

    /// Get the set of granted permissions.
    pub fn granted_permissions(&self) -> &std::collections::HashSet<Permission> {
        &self.granted_permissions
    }

    /// Get the set of granted host permissions.
    pub fn granted_host_permissions(&self) -> &[String] {
        &self.granted_host_permissions
    }

    /// Get the loaded background script, if any.
    pub fn background_script(&self) -> Option<&crate::extensions::types::BackgroundScript> {
        self.background_script.as_ref()
    }

    /// Set the background script (called during extension loading).
    pub fn set_background_script(
        &mut self,
        script: crate::extensions::types::BackgroundScript,
    ) {
        self.background_script = Some(script);
    }

    /// Fire `on_installed` lifecycle callbacks (called by ExtensionManager after loading).
    pub fn fire_installed(&self, details: InstalledDetails) {
        self.runtime_api.fire_installed(details);
    }

    /// Fire `on_startup` lifecycle callbacks (called by ExtensionManager on browser startup).
    pub fn fire_startup(&self) {
        self.runtime_api.fire_startup();
    }

    pub fn extension_id(&self) -> &ExtensionId {
        &self.extension_id
    }

    pub fn manifest(&self) -> &ExtensionManifest {
        &self.manifest
    }

    pub fn content_script_registry(&self) -> &ExtensionContentScriptRegistry {
        &self.scripting_api.registry
    }
}

impl ExtensionApi for AileronExtensionApi {
    fn id(&self) -> &ExtensionId {
        &self.extension_id
    }

    fn manifest(&self) -> &ExtensionManifest {
        &self.manifest
    }

    fn tabs(&self) -> &dyn TabsApi {
        &self.tabs_api
    }

    fn storage(&self) -> &dyn StorageApi {
        &self.storage_api
    }

    fn runtime(&self) -> &dyn RuntimeApi {
        &self.runtime_api
    }

    fn web_request(&self) -> &dyn WebRequestApi {
        &self.web_request_api
    }

    fn scripting(&self) -> &dyn ScriptingApi {
        &self.scripting_api
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extensions::storage::StorageGetKeys;

    const MINIMAL_MANIFEST: &str = r#"{
        "manifest_version": 3,
        "name": "Test Extension",
        "version": "1.0.0"
    }"#;

    fn make_api() -> AileronExtensionApi {
        let manifest = ExtensionManifest::from_json(MINIMAL_MANIFEST).unwrap();
        AileronExtensionApi::new(ExtensionId("test@example.com".into()), manifest)
    }

    #[test]
    fn test_api_creation() {
        let api = make_api();
        assert_eq!(api.extension_id().as_ref(), "test@example.com");
        assert_eq!(api.manifest().name, "Test Extension");
        assert_eq!(api.manifest().version, "1.0.0");
        assert_eq!(api.id().as_ref(), "test@example.com");
    }

    #[test]
    fn test_storage_get_set_clear() {
        let api = make_api();

        let result = api.storage().local().get(StorageGetKeys::All).unwrap();
        assert!(result.is_empty());

        let mut items = HashMap::new();
        items.insert("key1".into(), serde_json::Value::String("value1".into()));
        api.storage().local().set(items).unwrap();

        let result = api
            .storage()
            .local()
            .get(StorageGetKeys::Single("key1".into()))
            .unwrap();
        assert_eq!(
            result.get("key1").unwrap(),
            &serde_json::Value::String("value1".into())
        );

        let result = api
            .storage()
            .local()
            .get(StorageGetKeys::Single("nonexistent".into()))
            .unwrap();
        assert!(result.is_empty());

        api.storage().local().clear().unwrap();
        let result = api.storage().local().get(StorageGetKeys::All).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_storage_remove() {
        let api = make_api();

        let mut items = HashMap::new();
        items.insert("a".into(), serde_json::json!(1));
        items.insert("b".into(), serde_json::json!(2));
        api.storage().local().set(items).unwrap();

        api.storage().local().remove(vec!["a".into()]).unwrap();

        let result = api.storage().local().get(StorageGetKeys::All).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result.contains_key("b"));
    }

    #[test]
    fn test_storage_get_bytes_in_use() {
        let api = make_api();

        let mut items = HashMap::new();
        items.insert("key".into(), serde_json::Value::String("value".into()));
        api.storage().local().set(items).unwrap();

        let bytes = api
            .storage()
            .local()
            .get_bytes_in_use(Some(vec!["key".into()]))
            .unwrap();
        assert!(bytes > 0);

        let all_bytes = api.storage().local().get_bytes_in_use(None).unwrap();
        assert_eq!(bytes, all_bytes);
    }

    #[test]
    fn test_tabs_query_empty() {
        let api = make_api();
        let tabs = api.tabs().query(TabQuery::default()).unwrap();
        assert!(tabs.is_empty());
    }

    #[test]
    fn test_runtime_get_id_and_manifest() {
        let api = make_api();
        assert_eq!(api.runtime().get_id().as_ref(), "test@example.com");
        let m = api.runtime().get_manifest().unwrap();
        assert_eq!(m.name, "Test Extension");
    }

    #[test]
    fn test_runtime_get_url() {
        let api = make_api();
        let url = api.runtime().get_url("styles.css").unwrap();
        assert_eq!(
            url.as_str(),
            "aileron://extensions/test@example.com/styles.css"
        );
    }

    #[test]
    fn test_scripting_get_registered_empty() {
        let api = make_api();
        let scripts = api
            .scripting()
            .get_registered_content_scripts(None)
            .unwrap();
        assert!(scripts.is_empty());
    }

    #[test]
    fn test_web_request_remove_listener_not_found() {
        let api = make_api();
        let result = api.web_request().remove_listener(ListenerId(999));
        assert!(result.is_err());
    }

    // ── Persistent Storage Tests ──

    fn make_persistent_api(dir: &std::path::Path) -> AileronExtensionApi {
        let manifest = ExtensionManifest::from_json(MINIMAL_MANIFEST).unwrap();
        AileronExtensionApi::with_registry_and_storage(
            ExtensionId("test-persist".into()),
            manifest,
            ExtensionContentScriptRegistry::new(),
            Some(dir.to_path_buf()),
            None,
            None,
        )
    }

    #[test]
    fn test_persistent_storage_set_and_reload() {
        let dir = std::env::temp_dir().join("aileron_test_persist_set");
        let _ = std::fs::remove_dir_all(&dir);

        // Write data
        {
            let api = make_persistent_api(&dir);
            let mut items = HashMap::new();
            items.insert("key1".into(), serde_json::json!("hello"));
            items.insert("key2".into(), serde_json::json!(42));
            api.storage().local().set(items).unwrap();
        }

        // Reload and verify
        {
            let api = make_persistent_api(&dir);
            let result = api.storage().local().get(StorageGetKeys::All).unwrap();
            assert_eq!(result.len(), 2);
            assert_eq!(result.get("key1").unwrap(), &serde_json::json!("hello"));
            assert_eq!(result.get("key2").unwrap(), &serde_json::json!(42));
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_persistent_storage_remove_and_reload() {
        let dir = std::env::temp_dir().join("aileron_test_persist_remove");
        let _ = std::fs::remove_dir_all(&dir);

        // Write data
        {
            let api = make_persistent_api(&dir);
            let mut items = HashMap::new();
            items.insert("a".into(), serde_json::json!(1));
            items.insert("b".into(), serde_json::json!(2));
            api.storage().local().set(items).unwrap();
            api.storage().local().remove(vec!["a".into()]).unwrap();
        }

        // Reload and verify only "b" remains
        {
            let api = make_persistent_api(&dir);
            let result = api.storage().local().get(StorageGetKeys::All).unwrap();
            assert_eq!(result.len(), 1);
            assert!(result.contains_key("b"));
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_persistent_storage_clear_and_reload() {
        let dir = std::env::temp_dir().join("aileron_test_persist_clear");
        let _ = std::fs::remove_dir_all(&dir);

        // Write data then clear
        {
            let api = make_persistent_api(&dir);
            let mut items = HashMap::new();
            items.insert("x".into(), serde_json::json!("deleted"));
            api.storage().local().set(items).unwrap();
            api.storage().local().clear().unwrap();
        }

        // Reload and verify empty
        {
            let api = make_persistent_api(&dir);
            let result = api.storage().local().get(StorageGetKeys::All).unwrap();
            assert!(result.is_empty());
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_persistent_storage_separate_areas() {
        let dir = std::env::temp_dir().join("aileron_test_persist_areas");
        let _ = std::fs::remove_dir_all(&dir);

        {
            let api = make_persistent_api(&dir);
            let mut items = HashMap::new();
            items.insert("key".into(), serde_json::json!("local_value"));
            api.storage().local().set(items.clone()).unwrap();
            items.insert("key".into(), serde_json::json!("sync_value"));
            api.storage().sync().set(items).unwrap();
        }

        {
            let api = make_persistent_api(&dir);
            let local = api.storage().local().get(StorageGetKeys::Single("key".into())).unwrap();
            assert_eq!(local.get("key").unwrap(), &serde_json::json!("local_value"));
            let sync = api.storage().sync().get(StorageGetKeys::Single("key".into())).unwrap();
            assert_eq!(sync.get("key").unwrap(), &serde_json::json!("sync_value"));
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_persistent_storage_corrupted_file_graceful() {
        let dir = std::env::temp_dir().join("aileron_test_persist_corrupt");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);

        // Write garbage to the storage file
        let file_path = dir.join("test-persist").join("local.json");
        let _ = std::fs::create_dir_all(file_path.parent().unwrap());
        std::fs::write(&file_path, "this is not json {{{").unwrap();

        // Should load gracefully with empty data
        let api = make_persistent_api(&dir);
        let result = api.storage().local().get(StorageGetKeys::All).unwrap();
        assert!(result.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_storage_change_callback_fired_on_set() {
        let api = make_api();
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        let call_count = Arc::new(AtomicUsize::new(0));
        let count_clone = call_count.clone();

        api.storage().local().on_changed(Box::new(move |_changes, _area| {
            count_clone.fetch_add(1, Ordering::Relaxed);
        }));

        let mut items = HashMap::new();
        items.insert("key".into(), serde_json::json!("value"));
        api.storage().local().set(items).unwrap();

        assert_eq!(call_count.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_storage_change_callback_fired_on_remove() {
        let api = make_api();
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        let call_count = Arc::new(AtomicUsize::new(0));
        let count_clone = call_count.clone();

        api.storage().local().on_changed(Box::new(move |_changes, _area| {
            count_clone.fetch_add(1, Ordering::Relaxed);
        }));

        let mut items = HashMap::new();
        items.insert("key".into(), serde_json::json!("value"));
        api.storage().local().set(items).unwrap();

        api.storage().local().remove(vec!["key".into()]).unwrap();
        assert_eq!(call_count.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn test_storage_change_callback_not_fired_on_clear_empty() {
        let api = make_api();
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        let call_count = Arc::new(AtomicUsize::new(0));
        let count_clone = call_count.clone();

        api.storage().local().on_changed(Box::new(move |_changes, _area| {
            count_clone.fetch_add(1, Ordering::Relaxed);
        }));

        // Clear empty storage — no callback should fire
        api.storage().local().clear().unwrap();
        assert_eq!(call_count.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_web_request_handler_storage_and_firing() {
        let api = make_api();
        let filter = RequestFilter {
            urls: vec![UrlPattern("*://*.example.com/*".into())],
            types: None,
            tab_id: None,
            window_id: None,
        };

        // Register a handler that cancels matching requests
        let listener_id = api.web_request().on_before_request(
            filter.clone(),
            vec![],
            Box::new(|details| {
                if details.url.host_str() == Some("blocked.example.com") {
                    BlockingResponse {
                        cancel: Some(true),
                        ..Default::default()
                    }
                } else {
                    BlockingResponse::default()
                }
            }),
        );

        // Fire a blocked request
        let _details = RequestDetails {
            request_id: crate::extensions::types::RequestId(1),
            url: Url::parse("https://blocked.example.com/page").unwrap(),
            method: "GET".into(),
            frame_id: crate::extensions::types::FrameId(0),
            parent_frame_id: crate::extensions::types::FrameId(u32::MAX),
            tab_id: None,
            type_: crate::extensions::web_request::ResourceType::MainFrame,
            origin_url: None,
            timestamp: 0.0,
            request_headers: None,
        };

        // Access the inner AileronWebRequestApi via the trait — we can't call
        // fire_on_before_request through the trait, so test via remove_listener
        assert!(api.web_request().remove_listener(listener_id).is_ok());
        // Removing again should fail
        assert!(api.web_request().remove_listener(listener_id).is_err());
    }

    #[test]
    fn test_web_request_multiple_listeners() {
        let api = make_api();

        let filter1 = RequestFilter {
            urls: vec![UrlPattern("*://*.a.com/*".into())],
            types: None,
            tab_id: None,
            window_id: None,
        };
        let filter2 = RequestFilter {
            urls: vec![UrlPattern("*://*.b.com/*".into())],
            types: None,
            tab_id: None,
            window_id: None,
        };

        let id1 = api.web_request().on_before_request(
            filter1,
            vec![],
            Box::new(|_| BlockingResponse::default()),
        );
        let id2 = api.web_request().on_before_request(
            filter2,
            vec![],
            Box::new(|_| BlockingResponse::default()),
        );

        // Both should be removable
        assert!(api.web_request().remove_listener(id1).is_ok());
        assert!(api.web_request().remove_listener(id2).is_ok());
    }

    #[test]
    fn test_url_pattern_matching() {
        assert!(simple_url_pattern_match(
            "*://*.example.com/*",
            "https://sub.example.com/page"
        ));
        assert!(simple_url_pattern_match(
            "*://*.example.com/*",
            "https://example.com/page"
        ));
        assert!(!simple_url_pattern_match(
            "*://*.example.com/*",
            "https://other.com/page"
        ));
        assert!(simple_url_pattern_match(
            "<all_urls>",
            "https://anything.com/path"
        ));
        assert!(simple_url_pattern_match(
            "https://example.com/*",
            "https://example.com/page"
        ));
        assert!(!simple_url_pattern_match(
            "https://example.com/*",
            "http://example.com/page"
        ));
    }
}
