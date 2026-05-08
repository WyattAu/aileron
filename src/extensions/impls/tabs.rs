use std::sync::Mutex;

use crate::extensions::tabs::{
    ActiveInfo, CaptureOptions, CreateProperties, RemovalInfo, Tab, TabProvider, TabQuery,
    TabUpdateEvent, TabsApi, UpdateProperties,
};
use crate::extensions::types::{ExtensionError, Result, RuntimeMessage, TabId, WindowId};

use super::{ActivatedCallback, CreatedCallback, RemovedCallback, UpdatedCallback};

pub(super) struct AileronTabsApi {
    updated_callbacks: Mutex<Vec<UpdatedCallback>>,
    created_callbacks: Mutex<Vec<CreatedCallback>>,
    removed_callbacks: Mutex<Vec<RemovedCallback>>,
    activated_callbacks: Mutex<Vec<ActivatedCallback>>,
    tab_provider: Option<std::sync::Arc<dyn TabProvider>>,
}

impl AileronTabsApi {
    pub(super) fn new() -> Self {
        Self {
            updated_callbacks: Mutex::new(Vec::new()),
            created_callbacks: Mutex::new(Vec::new()),
            removed_callbacks: Mutex::new(Vec::new()),
            activated_callbacks: Mutex::new(Vec::new()),
            tab_provider: None,
        }
    }

    pub(super) fn with_provider(provider: std::sync::Arc<dyn TabProvider>) -> Self {
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
                let is_active = active_id.as_ref().is_some_and(|aid| aid.0 == t.id.0);
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
            url::Url::parse("aileron://newtab")
                .unwrap_or_else(|_| url::Url::parse("about:blank").unwrap())
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
            .ok_or_else(|| ExtensionError::NotFound(format!("Tab {tab_id}")))
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
            .ok_or_else(|| ExtensionError::NotFound(format!("Tab {tab_id}")))?;
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
