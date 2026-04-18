use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use url::Url;

use crate::extensions::api::ExtensionApi;
use crate::extensions::manifest::ExtensionManifest;
use crate::extensions::runtime::{ConnectInfo, InstalledDetails, MessageSender, Port, RuntimeApi};
use crate::extensions::scripting::{
    CssInjection, ExtensionContentScriptEntry, ExtensionContentScriptRegistry, ExtensionRunAt,
    InjectionResult, InjectionTarget, RegisteredContentScript, RunAt, ScriptFilter,
    ScriptInjection, ScriptingApi,
};
use crate::extensions::storage::{StorageApi, StorageArea, StorageChanges, StorageGetKeys};
use crate::extensions::tabs::{
    ActiveInfo, CaptureOptions, CreateProperties, RemovalInfo, Tab, TabQuery, TabUpdateEvent,
    TabsApi, UpdateProperties,
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

static LISTENER_COUNTER: AtomicU64 = AtomicU64::new(0);

fn next_listener_id() -> ListenerId {
    ListenerId(LISTENER_COUNTER.fetch_add(1, Ordering::Relaxed) + 1)
}

struct AileronTabsApi {
    updated_callbacks: Mutex<Vec<UpdatedCallback>>,
    created_callbacks: Mutex<Vec<CreatedCallback>>,
    removed_callbacks: Mutex<Vec<RemovedCallback>>,
    activated_callbacks: Mutex<Vec<ActivatedCallback>>,
}

impl AileronTabsApi {
    fn new() -> Self {
        Self {
            updated_callbacks: Mutex::new(Vec::new()),
            created_callbacks: Mutex::new(Vec::new()),
            removed_callbacks: Mutex::new(Vec::new()),
            activated_callbacks: Mutex::new(Vec::new()),
        }
    }
}

impl TabsApi for AileronTabsApi {
    fn query(&self, _query: TabQuery) -> Result<Vec<Tab>> {
        Ok(Vec::new())
    }

    fn create(&self, properties: CreateProperties) -> Result<Tab> {
        tracing::warn!(
            target: "extensions",
            "tabs.create not yet implemented (url: {:?})",
            properties.url
        );
        Err(ExtensionError::Unsupported("tabs.create".into()))
    }

    fn update(&self, tab_id: TabId, _properties: UpdateProperties) -> Result<Tab> {
        tracing::warn!(
            target: "extensions",
            "tabs.update({}) not yet implemented",
            tab_id
        );
        Err(ExtensionError::NotFound(format!("Tab {}", tab_id)))
    }

    fn remove(&self, tab_id: TabId) -> Result<()> {
        tracing::warn!(
            target: "extensions",
            "tabs.remove({}) not yet implemented",
            tab_id
        );
        Err(ExtensionError::NotFound(format!("Tab {}", tab_id)))
    }

    fn duplicate(&self, tab_id: TabId) -> Result<Tab> {
        tracing::warn!(
            target: "extensions",
            "tabs.duplicate({}) not yet implemented",
            tab_id
        );
        Err(ExtensionError::Unsupported("tabs.duplicate".into()))
    }

    fn send_message(
        &self,
        tab_id: TabId,
        _message: RuntimeMessage,
    ) -> Result<Option<RuntimeMessage>> {
        tracing::warn!(
            target: "extensions",
            "tabs.sendMessage({}, ...) not yet implemented",
            tab_id
        );
        Ok(None)
    }

    fn capture_visible_tab(
        &self,
        _window_id: Option<WindowId>,
        _options: CaptureOptions,
    ) -> Result<Vec<u8>> {
        tracing::warn!(
            target: "extensions",
            "tabs.captureVisibleTab not yet implemented"
        );
        Err(ExtensionError::Unsupported("tabs.captureVisibleTab".into()))
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
}

impl AileronStorageArea {
    fn new() -> Self {
        Self {
            data: Mutex::new(HashMap::new()),
            change_callbacks: Mutex::new(Vec::new()),
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
        for (key, value) in items {
            data.insert(key, value);
        }
        Ok(())
    }

    fn remove(&self, keys: Vec<String>) -> Result<()> {
        let mut data = self
            .data
            .lock()
            .map_err(|e| ExtensionError::Runtime(format!("Storage lock poisoned: {}", e)))?;
        for key in keys {
            data.remove(&key);
        }
        Ok(())
    }

    fn clear(&self) -> Result<()> {
        self.data
            .lock()
            .map_err(|e| ExtensionError::Runtime(format!("Storage lock poisoned: {}", e)))?
            .clear();
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
    fn new() -> Self {
        Self {
            local: AileronStorageArea::new(),
            sync: AileronStorageArea::new(),
            managed: AileronStorageArea::new(),
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
    message_callbacks: Mutex<Vec<MessageCallback>>,
    connect_callbacks: Mutex<Vec<ConnectCallback>>,
    installed_callbacks: Mutex<Vec<InstalledCallback>>,
    startup_callbacks: Mutex<Vec<StartupCallback>>,
}

impl AileronRuntimeApi {
    fn new(extension_id: ExtensionId, manifest: ExtensionManifest) -> Self {
        Self {
            extension_id,
            manifest,
            message_callbacks: Mutex::new(Vec::new()),
            connect_callbacks: Mutex::new(Vec::new()),
            installed_callbacks: Mutex::new(Vec::new()),
            startup_callbacks: Mutex::new(Vec::new()),
        }
    }
}

impl RuntimeApi for AileronRuntimeApi {
    fn send_message(
        &self,
        _extension_id: Option<ExtensionId>,
        _message: RuntimeMessage,
    ) -> Result<Option<RuntimeMessage>> {
        tracing::warn!(
            target: "extensions",
            "runtime.sendMessage not yet implemented"
        );
        Ok(None)
    }

    fn connect(&self, _connect_info: ConnectInfo) -> Result<Box<dyn Port>> {
        tracing::warn!(
            target: "extensions",
            "runtime.connect not yet implemented"
        );
        Err(ExtensionError::Unsupported("runtime.connect".into()))
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
    listeners: Mutex<Vec<ListenerId>>,
}

impl AileronWebRequestApi {
    fn new() -> Self {
        Self {
            listeners: Mutex::new(Vec::new()),
        }
    }
}

impl WebRequestApi for AileronWebRequestApi {
    fn on_before_request(
        &self,
        _filter: RequestFilter,
        _extra_info_spec: Vec<ExtraInfoSpec>,
        _handler: Box<dyn Fn(RequestDetails) -> BlockingResponse + Send + Sync>,
    ) -> ListenerId {
        let id = next_listener_id();
        tracing::warn!(
            target: "extensions",
            "webRequest.onBeforeRequest registered (listener {:?})",
            id
        );
        self.listeners
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(id);
        id
    }

    fn on_before_send_headers(
        &self,
        _filter: RequestFilter,
        _extra_info_spec: Vec<ExtraInfoSpec>,
        _handler: Box<dyn Fn(BeforeSendHeadersDetails) -> BlockingResponse + Send + Sync>,
    ) -> ListenerId {
        let id = next_listener_id();
        tracing::warn!(
            target: "extensions",
            "webRequest.onBeforeSendHeaders registered (listener {:?})",
            id
        );
        self.listeners
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(id);
        id
    }

    fn on_headers_received(
        &self,
        _filter: RequestFilter,
        _extra_info_spec: Vec<ExtraInfoSpec>,
        _handler: Box<dyn Fn(HeadersReceivedDetails) -> BlockingResponse + Send + Sync>,
    ) -> ListenerId {
        let id = next_listener_id();
        tracing::warn!(
            target: "extensions",
            "webRequest.onHeadersReceived registered (listener {:?})",
            id
        );
        self.listeners
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(id);
        id
    }

    fn on_auth_required(
        &self,
        _filter: RequestFilter,
        _handler: Box<dyn Fn(AuthRequiredDetails) -> BlockingResponse + Send + Sync>,
    ) -> ListenerId {
        let id = next_listener_id();
        tracing::warn!(
            target: "extensions",
            "webRequest.onAuthRequired registered (listener {:?})",
            id
        );
        self.listeners
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(id);
        id
    }

    fn on_before_redirect(
        &self,
        _filter: RequestFilter,
        _callback: Box<dyn Fn(RedirectDetails) + Send + Sync>,
    ) -> ListenerId {
        let id = next_listener_id();
        self.listeners
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(id);
        id
    }

    fn on_completed(
        &self,
        _filter: RequestFilter,
        _callback: Box<dyn Fn(CompletedDetails) + Send + Sync>,
    ) -> ListenerId {
        let id = next_listener_id();
        self.listeners
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(id);
        id
    }

    fn on_error_occurred(
        &self,
        _filter: RequestFilter,
        _callback: Box<dyn Fn(ErrorOccurredDetails) + Send + Sync>,
    ) -> ListenerId {
        let id = next_listener_id();
        self.listeners
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(id);
        id
    }

    fn remove_listener(&self, listener_id: ListenerId) -> Result<()> {
        let mut listeners = self
            .listeners
            .lock()
            .map_err(|e| ExtensionError::Runtime(format!("WebRequest lock poisoned: {}", e)))?;
        let before = listeners.len();
        listeners.retain(|&id| id != listener_id);
        if listeners.len() < before {
            Ok(())
        } else {
            Err(ExtensionError::NotFound(format!(
                "Listener {}",
                listener_id.0
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
        Self {
            tabs_api: AileronTabsApi::new(),
            storage_api: AileronStorageApi::new(),
            runtime_api: AileronRuntimeApi::new(extension_id.clone(), manifest.clone()),
            web_request_api: AileronWebRequestApi::new(),
            scripting_api: AileronScriptingApi::new(registry),
            extension_id,
            manifest,
        }
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
}
