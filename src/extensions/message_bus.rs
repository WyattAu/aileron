//! Extension message bus for inter-context communication.
//!
//! Routes messages between extension backgrounds, content scripts, and tabs.
//! Implements the browser.runtime.sendMessage / onMessage pattern.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::extensions::types::{ExtensionId, RuntimeMessage};

type MessageHandler = Box<dyn Fn(RuntimeMessage) -> Option<RuntimeMessage> + Send + Sync>;

/// A message that was routed through the bus.
#[derive(Debug, Clone)]
pub struct RoutedMessage {
    pub source_id: Option<ExtensionId>,
    pub target_id: Option<ExtensionId>,
    pub message: RuntimeMessage,
}

type HandlerList = Vec<MessageHandler>;

/// Shared message bus for extension-to-extension communication.
///
/// Extensions register message handlers via `on_message()`. When an extension
/// calls `send_message(target, msg)`, the bus looks up the target's handler
/// and invokes it, returning any response.
pub struct MessageBus {
    handlers: Mutex<HashMap<ExtensionId, HandlerList>>,
}

impl MessageBus {
    pub fn new() -> Self {
        Self {
            handlers: Mutex::new(HashMap::new()),
        }
    }

    /// Register a message handler for an extension.
    pub fn register_handler(&self, extension_id: ExtensionId, handler: MessageHandler) {
        self.handlers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entry(extension_id)
            .or_default()
            .push(handler);
    }

    /// Send a message from one extension to another (or broadcast).
    /// Returns the response from the target's handler, if any.
    ///
    /// If `target_id` is None, the message is broadcast to all extensions
    /// except the source (fire-and-forget).
    pub fn send_message(
        &self,
        source_id: Option<&ExtensionId>,
        target_id: Option<&ExtensionId>,
        message: RuntimeMessage,
    ) -> Option<RuntimeMessage> {
        let handlers = self.handlers.lock().unwrap_or_else(|e| e.into_inner());

        if let Some(target) = target_id {
            // Direct message to specific extension
            if let Some(target_handlers) = handlers.get(target) {
                for handler in target_handlers.iter() {
                    if let Some(response) = handler(message.clone()) {
                        return Some(response);
                    }
                }
            }
            None
        } else {
            // Broadcast to all except source
            let mut last_response = None;
            for (ext_id, ext_handlers) in handlers.iter() {
                if source_id.is_some_and(|src| src == ext_id) {
                    continue;
                }
                for handler in ext_handlers.iter() {
                    if let Some(response) = handler(message.clone()) {
                        last_response = Some(response);
                    }
                }
            }
            last_response
        }
    }

    /// Check if an extension has any registered message handlers.
    pub fn has_handlers(&self, extension_id: &ExtensionId) -> bool {
        let handlers = self.handlers.lock().unwrap_or_else(|e| e.into_inner());
        handlers.get(extension_id).is_some_and(|h| !h.is_empty())
    }

    /// Remove all handlers for an extension (used on unload).
    pub fn remove_handlers(&self, extension_id: &ExtensionId) {
        self.handlers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(extension_id);
    }

    /// Get the number of registered handlers (for testing).
    #[cfg(test)]
    pub fn handler_count(&self, extension_id: &ExtensionId) -> usize {
        let handlers = self.handlers.lock().unwrap_or_else(|e| e.into_inner());
        handlers.get(extension_id).map(|h| h.len()).unwrap_or(0)
    }
}

impl Default for MessageBus {
    fn default() -> Self {
        Self::new()
    }
}

type PortHandler = Box<dyn Fn(RuntimeMessage) + Send + Sync>;
type DisconnectHandler = Box<dyn Fn() + Send + Sync>;

/// A simple in-process Port implementation for long-lived connections.
pub struct LocalPort {
    port_name: String,
    message_handlers: Mutex<Vec<PortHandler>>,
    disconnect_handlers: Mutex<Vec<DisconnectHandler>>,
    disconnected: Mutex<bool>,
}

impl LocalPort {
    pub fn new(name: &str) -> Self {
        Self {
            port_name: name.to_string(),
            message_handlers: Mutex::new(Vec::new()),
            disconnect_handlers: Mutex::new(Vec::new()),
            disconnected: Mutex::new(false),
        }
    }
}

impl crate::extensions::runtime::Port for LocalPort {
    fn name(&self) -> &str {
        &self.port_name
    }

    fn sender(&self) -> &crate::extensions::runtime::MessageSender {
        // Local ports don't have a meaningful sender
        static EMPTY_SENDER: crate::extensions::runtime::MessageSender =
            crate::extensions::runtime::MessageSender {
                tab_id: None,
                frame_id: None,
                url: None,
                extension_id: None,
            };
        &EMPTY_SENDER
    }

    fn post_message(&self, message: RuntimeMessage) -> crate::extensions::types::Result<()> {
        if *self.disconnected.lock().unwrap_or_else(|e| e.into_inner()) {
            return Err(crate::extensions::ExtensionError::Runtime(
                "Port is disconnected".into(),
            ));
        }
        let handlers = self
            .message_handlers
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        for handler in handlers.iter() {
            handler(message.clone());
        }
        Ok(())
    }

    fn disconnect(&self) {
        let mut disconnected = self.disconnected.lock().unwrap_or_else(|e| e.into_inner());
        if *disconnected {
            return;
        }
        *disconnected = true;
        let handlers = self
            .disconnect_handlers
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        for handler in handlers.iter() {
            handler();
        }
    }

    fn on_message(&self, callback: Box<dyn Fn(RuntimeMessage) + Send + Sync>) {
        self.message_handlers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(callback);
    }

    fn on_disconnect(&self, callback: Box<dyn Fn() + Send + Sync>) {
        self.disconnect_handlers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(callback);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extensions::runtime::Port;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn test_register_and_send_direct() {
        let bus = Arc::new(MessageBus::new());
        let bus_clone = bus.clone();

        let target = ExtensionId("target".into());
        bus.register_handler(
            target.clone(),
            Box::new(move |msg| {
                if msg.as_str() == Some("hello") {
                    Some(serde_json::json!("world"))
                } else {
                    None
                }
            }),
        );

        let response = bus_clone.send_message(
            Some(&ExtensionId("source".into())),
            Some(&target),
            serde_json::json!("hello"),
        );
        assert_eq!(response, Some(serde_json::json!("world")));
    }

    #[test]
    fn test_send_to_nonexistent_returns_none() {
        let bus = MessageBus::new();
        let response = bus.send_message(
            Some(&ExtensionId("source".into())),
            Some(&ExtensionId("nobody".into())),
            serde_json::json!("hello"),
        );
        assert!(response.is_none());
    }

    #[test]
    fn test_broadcast_skips_source() {
        let bus = Arc::new(MessageBus::new());
        let bus_clone = bus.clone();

        let call_count = Arc::new(AtomicUsize::new(0));

        let count_a = call_count.clone();
        bus.register_handler(
            ExtensionId("a".into()),
            Box::new(move |_| {
                count_a.fetch_add(1, Ordering::Relaxed);
                None
            }),
        );

        let count_b = call_count.clone();
        bus.register_handler(
            ExtensionId("b".into()),
            Box::new(move |_| {
                count_b.fetch_add(1, Ordering::Relaxed);
                None
            }),
        );

        // Broadcast from "a" — should only reach "b"
        bus_clone.send_message(
            Some(&ExtensionId("a".into())),
            None,
            serde_json::json!("ping"),
        );
        assert_eq!(call_count.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_has_handlers() {
        let bus = MessageBus::new();
        let ext = ExtensionId("ext".into());
        assert!(!bus.has_handlers(&ext));
        bus.register_handler(ext.clone(), Box::new(|_| None));
        assert!(bus.has_handlers(&ext));
    }

    #[test]
    fn test_remove_handlers() {
        let bus = MessageBus::new();
        let ext = ExtensionId("ext".into());
        bus.register_handler(ext.clone(), Box::new(|_| None));
        assert!(bus.has_handlers(&ext));
        bus.remove_handlers(&ext);
        assert!(!bus.has_handlers(&ext));
    }

    #[test]
    fn test_local_port_name() {
        let port = LocalPort::new("test-port");
        assert_eq!(port.name(), "test-port");
    }

    #[test]
    fn test_local_port_post_message() {
        let port = LocalPort::new("port");
        let received = Arc::new(AtomicUsize::new(0));
        let r = received.clone();
        port.on_message(Box::new(move |_| {
            r.fetch_add(1, Ordering::Relaxed);
        }));
        port.post_message(serde_json::json!("hello")).unwrap();
        assert_eq!(received.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_local_port_disconnect() {
        let port = LocalPort::new("port");
        let disconnected = Arc::new(AtomicUsize::new(0));
        let d = disconnected.clone();
        port.on_disconnect(Box::new(move || {
            d.fetch_add(1, Ordering::Relaxed);
        }));
        port.disconnect();
        assert_eq!(disconnected.load(Ordering::Relaxed), 1);
        // Second disconnect should be no-op
        port.disconnect();
        assert_eq!(disconnected.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_local_port_post_after_disconnect_fails() {
        let port = LocalPort::new("port");
        port.disconnect();
        let result = port.post_message(serde_json::json!("hello"));
        assert!(result.is_err());
    }
}
