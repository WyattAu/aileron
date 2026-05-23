//! Concrete implementation of [`ContextMenusApi`] for Aileron.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use parking_lot::RwLock;

use crate::extensions::context_menus::{
    ContextMenusApi, ContextType, MenuClickInfo, MenuCreateParams, MenuItem, MenuItemType,
};
use crate::extensions::types::{ListenerId, Result};

static MENU_ITEM_COUNTER: AtomicU64 = AtomicU64::new(0);

type ClickCallback = Arc<dyn Fn(MenuClickInfo) + Send + Sync>;

pub struct AileronContextMenusApi {
    extension_id: String,
    items: RwLock<HashMap<String, MenuItem>>,
    callbacks: RwLock<Vec<(ListenerId, ClickCallback)>>,
}

impl AileronContextMenusApi {
    pub fn new(extension_id: &str) -> Self {
        Self {
            extension_id: extension_id.to_string(),
            items: RwLock::new(HashMap::new()),
            callbacks: RwLock::new(Vec::new()),
        }
    }

    fn next_id() -> String {
        format!(
            "ctx-menu-{}",
            MENU_ITEM_COUNTER.fetch_add(1, Ordering::Relaxed)
        )
    }

    /// Get all menu items for display in the UI.
    pub fn get_items(&self) -> Vec<MenuItem> {
        self.items.read().values().cloned().collect()
    }

    /// Fire click callbacks for a menu item.
    pub fn fire_clicked(&self, info: MenuClickInfo) {
        let callbacks = self.callbacks.read();
        for (_, cb) in callbacks.iter() {
            cb(info.clone());
        }
    }
}

impl ContextMenusApi for AileronContextMenusApi {
    fn create(&self, params: MenuCreateParams) -> Result<String> {
        let id = params.id.clone().unwrap_or_else(Self::next_id);
        let contexts = params.contexts.unwrap_or_else(|| vec![ContextType::Page]);
        let item_type = params.item_type.unwrap_or(MenuItemType::Normal);
        let enabled = params.enabled.unwrap_or(true);

        let item = MenuItem {
            id: id.clone(),
            extension_id: self.extension_id.clone(),
            title: params.title,
            contexts,
            checked: params.checked,
            enabled,
            parent_id: params.parent_id,
            item_type,
            document_url_patterns: params.document_url_patterns,
        };

        self.items.write().insert(id.clone(), item);
        Ok(id)
    }

    fn update(&self, id: &str, params: MenuCreateParams) -> Result<bool> {
        let mut items = self.items.write();
        let Some(existing) = items.get_mut(id) else {
            return Ok(false);
        };

        if let Some(title) = params.title {
            existing.title = Some(title);
        }
        if let Some(contexts) = params.contexts {
            existing.contexts = contexts;
        }
        if let Some(checked) = params.checked {
            existing.checked = Some(checked);
        }
        if let Some(enabled) = params.enabled {
            existing.enabled = enabled;
        }
        if let Some(item_type) = params.item_type {
            existing.item_type = item_type;
        }
        if let Some(parent_id) = params.parent_id {
            existing.parent_id = Some(parent_id);
        }

        Ok(true)
    }

    fn remove(&self, id: &str) -> Result<bool> {
        Ok(self.items.write().remove(id).is_some())
    }

    fn remove_all(&self) -> Result<bool> {
        let mut items = self.items.write();
        let count = items.len();
        items.clear();
        Ok(count > 0)
    }

    fn on_clicked(&self, callback: Arc<dyn Fn(MenuClickInfo) + Send + Sync>) {
        let mut callbacks = self.callbacks.write();
        let id = ListenerId(super::super::impls::next_listener_id_raw());
        callbacks.push((id, callback));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_menu_item_with_id() {
        let api = AileronContextMenusApi::new("ext@test.com");
        let id = api
            .create(MenuCreateParams {
                id: Some("my-item".into()),
                title: Some("Click Me".into()),
                contexts: Some(vec![ContextType::All]),
                checked: None,
                enabled: None,
                parent_id: None,
                document_url_patterns: None,
                item_type: None,
                visible: None,
                selector: None,
            })
            .unwrap();
        assert_eq!(id, "my-item");

        let items = api.get_items();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title.as_deref(), Some("Click Me"));
    }

    #[test]
    fn test_create_menu_item_auto_id() {
        let api = AileronContextMenusApi::new("ext@test.com");
        let id = api
            .create(MenuCreateParams {
                id: None,
                title: Some("Auto".into()),
                contexts: None,
                checked: None,
                enabled: None,
                parent_id: None,
                document_url_patterns: None,
                item_type: None,
                visible: None,
                selector: None,
            })
            .unwrap();
        assert!(id.starts_with("ctx-menu-"));
    }

    #[test]
    fn test_update_menu_item() {
        let api = AileronContextMenusApi::new("ext@test.com");
        api.create(MenuCreateParams {
            id: Some("upd".into()),
            title: Some("Before".into()),
            contexts: None,
            checked: None,
            enabled: None,
            parent_id: None,
            document_url_patterns: None,
            item_type: None,
            visible: None,
            selector: None,
        })
        .unwrap();

        let found = api
            .update(
                "upd",
                MenuCreateParams {
                    id: None,
                    title: Some("After".into()),
                    contexts: None,
                    checked: None,
                    enabled: Some(false),
                    parent_id: None,
                    document_url_patterns: None,
                    item_type: None,
                    visible: None,
                    selector: None,
                },
            )
            .unwrap();
        assert!(found);

        let items = api.get_items();
        assert_eq!(items[0].title.as_deref(), Some("After"));
        assert!(!items[0].enabled);
    }

    #[test]
    fn test_remove_menu_item() {
        let api = AileronContextMenusApi::new("ext@test.com");
        api.create(MenuCreateParams {
            id: Some("rm".into()),
            title: Some("Remove Me".into()),
            contexts: None,
            checked: None,
            enabled: None,
            parent_id: None,
            document_url_patterns: None,
            item_type: None,
            visible: None,
            selector: None,
        })
        .unwrap();

        assert!(api.remove("rm").unwrap());
        assert!(!api.remove("rm").unwrap());
        assert!(api.get_items().is_empty());
    }

    #[test]
    fn test_remove_all() {
        let api = AileronContextMenusApi::new("ext@test.com");
        api.create(MenuCreateParams {
            id: Some("a".into()),
            title: None,
            contexts: None,
            checked: None,
            enabled: None,
            parent_id: None,
            document_url_patterns: None,
            item_type: None,
            visible: None,
            selector: None,
        })
        .unwrap();
        api.create(MenuCreateParams {
            id: Some("b".into()),
            title: None,
            contexts: None,
            checked: None,
            enabled: None,
            parent_id: None,
            document_url_patterns: None,
            item_type: None,
            visible: None,
            selector: None,
        })
        .unwrap();

        assert!(api.remove_all().unwrap());
        assert!(api.get_items().is_empty());
        assert!(!api.remove_all().unwrap());
    }

    #[test]
    fn test_update_nonexistent() {
        let api = AileronContextMenusApi::new("ext@test.com");
        let found = api
            .update(
                "nope",
                MenuCreateParams {
                    id: None,
                    title: Some("x".into()),
                    contexts: None,
                    checked: None,
                    enabled: None,
                    parent_id: None,
                    document_url_patterns: None,
                    item_type: None,
                    visible: None,
                    selector: None,
                },
            )
            .unwrap();
        assert!(!found);
    }

    #[test]
    fn test_on_clicked_callback() {
        let api = AileronContextMenusApi::new("ext@test.com");
        let clicked = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let clicked_clone = clicked.clone();
        api.on_clicked(Arc::new(move |_info| {
            clicked_clone.store(true, Ordering::Relaxed);
        }));

        api.fire_clicked(MenuClickInfo {
            menu_item_id: "test".into(),
            parent_menu_item_id: None,
            context: ContextType::Page,
            checked: None,
            page_url: Some("https://example.com".into()),
            link_url: None,
            src_url: None,
            selection_text: None,
        });

        assert!(clicked.load(Ordering::Relaxed));
    }

    #[test]
    fn test_create_separator() {
        let api = AileronContextMenusApi::new("ext@test.com");
        let _id = api
            .create(MenuCreateParams {
                id: Some("sep".into()),
                title: None,
                contexts: Some(vec![ContextType::All]),
                checked: None,
                enabled: None,
                parent_id: None,
                document_url_patterns: None,
                item_type: Some(MenuItemType::Separator),
                visible: None,
                selector: None,
            })
            .unwrap();
        let items = api.get_items();
        assert_eq!(items[0].item_type, MenuItemType::Separator);
    }
}
