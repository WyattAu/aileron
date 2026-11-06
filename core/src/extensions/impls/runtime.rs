use std::sync::{Arc, Mutex};

use url::Url;

use crate::extensions::manifest::ExtensionManifest;
use crate::extensions::message_bus::MessageBus;
use crate::extensions::runtime::{ConnectInfo, InstalledDetails, Port, RuntimeApi};
use crate::extensions::types::{ExtensionError, ExtensionId, Result, RuntimeMessage};

use super::{ConnectCallback, InstalledCallback, MessageCallback, StartupCallback};

pub(super) struct AileronRuntimeApi {
    extension_id: ExtensionId,
    manifest: ExtensionManifest,
    message_bus: Option<Arc<MessageBus>>,
    message_callbacks: Arc<Mutex<Vec<MessageCallback>>>,
    connect_callbacks: Mutex<Vec<ConnectCallback>>,
    installed_callbacks: Mutex<Vec<InstalledCallback>>,
    startup_callbacks: Mutex<Vec<StartupCallback>>,
}

impl AileronRuntimeApi {
    pub(super) fn new(extension_id: ExtensionId, manifest: ExtensionManifest) -> Self {
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

    pub(super) fn with_message_bus(
        extension_id: ExtensionId,
        manifest: ExtensionManifest,
        message_bus: Arc<MessageBus>,
    ) -> Self {
        let callbacks: Arc<Mutex<Vec<MessageCallback>>> = Arc::new(Mutex::new(Vec::new()));
        let cb_clone = callbacks.clone();

        // Register a handler on the bus that invokes our stored callbacks
        message_bus.register_handler(
            extension_id.clone(),
            Box::new(move |msg: RuntimeMessage| {
                let callbacks: Vec<_> = cb_clone
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .iter()
                    .cloned()
                    .collect();
                for cb in callbacks {
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
            }),
        );

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
    pub(super) fn fire_installed(&self, details: InstalledDetails) {
        let callbacks: Vec<_> = self
            .installed_callbacks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .cloned()
            .collect();
        for cb in callbacks.iter() {
            cb(details.clone());
        }
        if !callbacks.is_empty() {
            tracing::debug!(
                target: "extensions",
                "Fired {} on_installed callback(s) for extension '{}'",
                callbacks.len(),
                self.extension_id.0
            );
        }
    }

    /// Fire all registered `on_startup` callbacks.
    /// Called by the extension loader during browser startup.
    pub(super) fn fire_startup(&self) {
        let callbacks: Vec<_> = self
            .startup_callbacks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .cloned()
            .collect();
        for cb in callbacks.iter() {
            cb();
        }
        if !callbacks.is_empty() {
            tracing::debug!(
                target: "extensions",
                "Fired {} on_startup callback(s) for extension '{}'",
                callbacks.len(),
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
        let port: Box<dyn Port> = Box::new(crate::extensions::message_bus::LocalPort::new(&name));
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
        .map_err(|e| ExtensionError::Runtime(format!("Invalid extension URL: {e}")))
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
