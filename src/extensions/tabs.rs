use url::Url;

use crate::extensions::types::{Result, RuntimeMessage, TabId, UrlPattern, WindowId};

/// Represents a browser tab.
#[derive(Debug, Clone)]
pub struct Tab {
    pub id: TabId,
    pub window_id: WindowId,
    pub active: bool,
    pub pinned: bool,
    pub url: Url,
    pub title: Option<String>,
    pub fav_icon_url: Option<Url>,
    pub status: TabStatus,
    pub incognito: bool,
    pub audible: bool,
    pub muted: bool,
    pub width: u32,
    pub height: u32,
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
    pub active: Option<bool>,
    pub window_id: Option<WindowId>,
    pub url: Option<Vec<UrlPattern>>,
    pub title: Option<String>,
    pub status: Option<TabStatus>,
    pub pinned: Option<bool>,
    pub audible: Option<bool>,
    pub muted: Option<bool>,
    pub incognito: Option<bool>,
    pub current_window: Option<bool>,
    pub highlighted: Option<bool>,
}

/// Properties for creating a new tab.
#[derive(Debug, Clone)]
pub struct CreateProperties {
    pub url: Option<Url>,
    pub active: Option<bool>,
    pub window_id: Option<WindowId>,
    pub index: Option<u32>,
    pub pinned: Option<bool>,
    pub incognito: Option<bool>,
    pub opener_tab_id: Option<TabId>,
}

/// Properties for updating an existing tab.
#[derive(Debug, Clone)]
pub struct UpdateProperties {
    pub url: Option<Url>,
    pub active: Option<bool>,
    pub muted: Option<bool>,
    pub pinned: Option<bool>,
    pub index: Option<u32>,
}

/// Options for tab capture.
#[derive(Debug, Clone)]
pub struct CaptureOptions {
    pub format: CaptureFormat,
    pub quality: Option<u8>,
}

#[derive(Debug, Clone)]
pub enum CaptureFormat {
    Png,
    Jpeg,
    Webp,
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

/// Access and manipulate browser tabs.
pub trait TabsApi: Send + Sync {
    fn query(&self, query: TabQuery) -> Result<Vec<Tab>>;

    fn create(&self, properties: CreateProperties) -> Result<Tab>;

    fn update(&self, tab_id: TabId, properties: UpdateProperties) -> Result<Tab>;

    fn remove(&self, tab_id: TabId) -> Result<()>;

    fn duplicate(&self, tab_id: TabId) -> Result<Tab>;

    fn send_message(
        &self,
        tab_id: TabId,
        message: RuntimeMessage,
    ) -> Result<Option<RuntimeMessage>>;

    fn capture_visible_tab(
        &self,
        window_id: Option<WindowId>,
        options: CaptureOptions,
    ) -> Result<Vec<u8>>;

    fn on_updated(&self, callback: Box<dyn Fn(TabUpdateEvent) + Send + Sync>);

    fn on_created(&self, callback: Box<dyn Fn(Tab) + Send + Sync>);

    fn on_removed(&self, callback: Box<dyn Fn(TabId, RemovalInfo) + Send + Sync>);

    fn on_activated(&self, callback: Box<dyn Fn(ActiveInfo) + Send + Sync>);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tab_query_default() {
        let q = TabQuery::default();
        assert!(q.active.is_none());
        assert!(q.window_id.is_none());
        assert!(q.url.is_none());
        assert!(q.title.is_none());
        assert!(q.status.is_none());
    }

    #[test]
    fn test_tab_query_with_filters() {
        let q = TabQuery {
            active: Some(true),
            status: Some(TabStatus::Complete),
            ..Default::default()
        };
        assert_eq!(q.active, Some(true));
        assert_eq!(q.status, Some(TabStatus::Complete));
    }

    #[test]
    fn test_tab_status_equality() {
        assert_eq!(TabStatus::Loading, TabStatus::Loading);
        assert_eq!(TabStatus::Complete, TabStatus::Complete);
        assert_ne!(TabStatus::Loading, TabStatus::Complete);
    }

    #[test]
    fn test_url_pattern() {
        let p = UrlPattern("*://*.example.com/*".into());
        assert_eq!(p.0, "*://*.example.com/*");
    }

    #[test]
    fn test_create_properties_default_url() {
        let props = CreateProperties {
            url: None,
            active: None,
            window_id: None,
            index: None,
            pinned: None,
            incognito: None,
            opener_tab_id: None,
        };
        assert!(props.url.is_none());
    }

    #[test]
    fn test_update_properties() {
        let props = UpdateProperties {
            url: Some(Url::parse("https://example.com").unwrap()),
            active: Some(true),
            muted: None,
            pinned: None,
            index: None,
        };
        assert!(props.url.is_some());
        assert!(props.muted.is_none());
    }
}
