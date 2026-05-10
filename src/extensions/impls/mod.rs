use std::sync::Arc;

use crate::extensions::api::ExtensionApi;
use crate::extensions::manifest::ExtensionManifest;
use crate::extensions::message_bus::MessageBus;
use crate::extensions::permissions::{self, Permission};
use crate::extensions::runtime::{InstalledDetails, MessageSender, Port, RuntimeApi};
use crate::extensions::scripting::ExtensionContentScriptRegistry;
use crate::extensions::scripting::ScriptingApi;
use crate::extensions::storage::{StorageApi, StorageChanges};
use crate::extensions::tabs::{RemovalInfo, Tab, TabProvider, TabUpdateEvent, TabsApi};
use crate::extensions::types::{
    ExtensionError, ExtensionId, ListenerId, Result, RuntimeMessage, TabId,
};
use crate::extensions::web_request::WebRequestApi;

mod runtime;
mod scripting;
mod storage;
mod tabs;
mod web_request;

#[cfg(test)]
mod tests;

use runtime::AileronRuntimeApi;
use scripting::AileronScriptingApi;
use storage::AileronStorageApi;
use tabs::AileronTabsApi;
use web_request::AileronWebRequestApi;

type UpdatedCallback = Box<dyn Fn(TabUpdateEvent) + Send + Sync>;
type CreatedCallback = Box<dyn Fn(Tab) + Send + Sync>;
type RemovedCallback = Box<dyn Fn(TabId, RemovalInfo) + Send + Sync>;
type ActivatedCallback = Box<dyn Fn(crate::extensions::tabs::ActiveInfo) + Send + Sync>;
type StorageChangeCallback = Arc<dyn Fn(StorageChanges, String) + Send + Sync>;
type MessageCallback =
    Arc<dyn Fn(RuntimeMessage, MessageSender) -> Option<RuntimeMessage> + Send + Sync>;
type ConnectCallback = Box<dyn Fn(Box<dyn Port>) + Send + Sync>;
type InstalledCallback = Arc<dyn Fn(InstalledDetails) + Send + Sync>;
type StartupCallback = Arc<dyn Fn() + Send + Sync>;

static LISTENER_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn next_listener_id() -> ListenerId {
    ListenerId(LISTENER_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1)
}

pub struct AileronExtensionApi {
    extension_id: ExtensionId,
    manifest: ExtensionManifest,
    tabs_api: AileronTabsApi,
    storage_api: AileronStorageApi,
    runtime_api: AileronRuntimeApi,
    web_request_api: Arc<AileronWebRequestApi>,
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
        let granted_permissions = permissions::parse_permissions(&manifest.permissions);

        let mut storage_api = match storage_dir {
            Some(dir) => AileronStorageApi::with_persistence(dir, &extension_id),
            None => AileronStorageApi::new(),
        };
        storage_api.set_permissions(granted_permissions.clone());

        let tabs_api = match tab_provider {
            Some(provider) => AileronTabsApi::with_provider(provider),
            None => AileronTabsApi::new(),
        };
        let runtime_api = match message_bus {
            Some(bus) => {
                AileronRuntimeApi::with_message_bus(extension_id.clone(), manifest.clone(), bus)
            }
            None => AileronRuntimeApi::new(extension_id.clone(), manifest.clone()),
        };
        let mut wr_api = AileronWebRequestApi::new();
        wr_api.set_permissions(granted_permissions.clone());
        let scripting_api = AileronScriptingApi::new(registry);
        let granted_host_permissions = manifest.host_permissions.clone();
        Self {
            tabs_api,
            storage_api,
            runtime_api,
            web_request_api: Arc::new(wr_api),
            scripting_api,
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
    #[must_use = "ignoring this value may lead to unexpected behavior"]
    pub fn check_api_permission(&self, api: &str, method: &str) -> Result<()> {
        if permissions::check_permission(&self.granted_permissions, api, method) {
            Ok(())
        } else {
            let required = permissions::required_permissions(api, method);
            let names: Vec<String> = required.iter().map(|p| format!("{p:?}")).collect();
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
        if self
            .granted_host_permissions
            .iter()
            .any(|p| p == "<all_urls>")
        {
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
    #[must_use]
    pub fn background_script(&self) -> Option<&crate::extensions::types::BackgroundScript> {
        self.background_script.as_ref()
    }

    /// Set the background script (called during extension loading).
    pub fn set_background_script(&mut self, script: crate::extensions::types::BackgroundScript) {
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
        &*self.web_request_api
    }

    fn scripting(&self) -> &dyn ScriptingApi {
        &self.scripting_api
    }
}

impl AileronExtensionApi {
    /// Get a shared reference to the web request interceptor.
    /// Used by the request lifecycle to dispatch events to extension handlers.
    pub(crate) fn web_request_interceptor(&self) -> Arc<AileronWebRequestApi> {
        self.web_request_api.clone()
    }
}
