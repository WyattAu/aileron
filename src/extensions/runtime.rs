use std::sync::Arc;

use url::Url;

use crate::extensions::types::{ExtensionId, FrameId, Result, RuntimeMessage, TabId};

/// Information for establishing a port connection.
#[derive(Debug, Clone)]
pub struct ConnectInfo {
    pub extension_id: Option<ExtensionId>,
    pub name: Option<String>,
    pub include_tls_channel_id: Option<bool>,
}

/// A long-lived communication port between extension contexts.
pub trait Port: Send + Sync {
    fn name(&self) -> &str;

    fn sender(&self) -> &MessageSender;

    fn post_message(&self, message: RuntimeMessage) -> Result<()>;

    fn disconnect(&self);

    fn on_message(&self, callback: Box<dyn Fn(RuntimeMessage) + Send + Sync>);

    fn on_disconnect(&self, callback: Box<dyn Fn() + Send + Sync>);
}

/// Information about the sender of a message.
#[derive(Debug, Clone)]
pub struct MessageSender {
    pub tab_id: Option<TabId>,
    pub frame_id: Option<FrameId>,
    pub url: Option<Url>,
    pub extension_id: Option<ExtensionId>,
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

/// Extension runtime — lifecycle, messaging, and manifest access.
pub trait RuntimeApi: Send + Sync {
    fn send_message(
        &self,
        extension_id: Option<ExtensionId>,
        message: RuntimeMessage,
    ) -> Result<Option<RuntimeMessage>>;

    fn connect(&self, connect_info: ConnectInfo) -> Result<Box<dyn Port>>;

    fn get_manifest(&self) -> Result<crate::extensions::manifest::ExtensionManifest>;

    fn get_url(&self, path: &str) -> Result<Url>;

    fn get_id(&self) -> &ExtensionId;

    fn on_message(
        &self,
        callback: Arc<
            dyn Fn(RuntimeMessage, MessageSender) -> Option<RuntimeMessage> + Send + Sync,
        >,
    );

    fn on_connect(&self, callback: Box<dyn Fn(Box<dyn Port>) + Send + Sync>);

    fn on_installed(&self, callback: Arc<dyn Fn(InstalledDetails) + Send + Sync>);

    fn on_startup(&self, callback: Arc<dyn Fn() + Send + Sync>);

    fn reload(&self) -> Result<()>;

    fn open_options_page(&self) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_sender() {
        let sender = MessageSender {
            tab_id: Some(TabId(1)),
            frame_id: Some(FrameId(0)),
            url: Some(Url::parse("https://example.com").unwrap()),
            extension_id: Some(ExtensionId("ext@test.com".into())),
        };
        assert!(sender.tab_id.is_some());
        assert!(sender.extension_id.is_some());
    }

    #[test]
    fn test_message_sender_all_none() {
        let sender = MessageSender {
            tab_id: None,
            frame_id: None,
            url: None,
            extension_id: None,
        };
        assert!(sender.tab_id.is_none());
    }

    #[test]
    fn test_connect_info() {
        let info = ConnectInfo {
            extension_id: Some(ExtensionId("ext@test.com".into())),
            name: Some("my-port".into()),
            include_tls_channel_id: Some(true),
        };
        assert_eq!(info.name.as_deref(), Some("my-port"));
    }

    #[test]
    fn test_install_reason() {
        assert_eq!(InstallReason::Install, InstallReason::Install);
        assert_ne!(InstallReason::Install, InstallReason::Update);
    }

    #[test]
    fn test_installed_details() {
        let details = InstalledDetails {
            reason: InstallReason::Install,
            previous_version: None,
            id: ExtensionId("ext@test.com".into()),
        };
        assert_eq!(details.reason, InstallReason::Install);
        assert!(details.previous_version.is_none());
    }
}
