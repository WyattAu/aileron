//! WebExtensions `browser.contextMenus` API.
//!
//! Provides context menu item management for extensions. Extensions can create
//! menu items that appear in the browser's context menu, with various types
//! (normal, checkbox, radio, separator) and context filters (page, link, image, etc.).

use std::sync::Arc;

use crate::extensions::types::Result;

/// Context in which a menu item should appear.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContextType {
    All,
    Page,
    Frame,
    Selection,
    Link,
    Editable,
    Image,
    Video,
    Audio,
    Launcher,
    BrowserAction,
    PageAction,
    Tab,
}

impl ContextType {
    /// Parse from the string used in manifest.json / API calls.
    pub fn parse(s: &str) -> Self {
        match s {
            "all" => Self::All,
            "page" => Self::Page,
            "frame" => Self::Frame,
            "selection" => Self::Selection,
            "link" => Self::Link,
            "editable" => Self::Editable,
            "image" => Self::Image,
            "video" => Self::Video,
            "audio" => Self::Audio,
            "launcher" => Self::Launcher,
            "browser_action" => Self::BrowserAction,
            "page_action" => Self::PageAction,
            "tab" => Self::Tab,
            _ => Self::Page,
        }
    }
}

/// The type of menu item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuItemType {
    Normal,
    Checkbox,
    Radio,
    Separator,
}

impl MenuItemType {
    pub fn parse(s: &str) -> Self {
        match s {
            "checkbox" => Self::Checkbox,
            "radio" => Self::Radio,
            "separator" => Self::Separator,
            _ => Self::Normal,
        }
    }
}

/// Parameters for creating a context menu item.
#[derive(Debug, Clone)]
pub struct MenuCreateParams {
    /// Unique identifier for the item. Auto-generated if not provided.
    pub id: Option<String>,
    /// The text displayed for the item.
    pub title: Option<String>,
    /// List of contexts where the item should appear. Defaults to ["page"].
    pub contexts: Option<Vec<ContextType>>,
    /// Whether the item is initially checked (checkbox/radio).
    pub checked: Option<bool>,
    /// Whether the item is initially enabled. Defaults to true.
    pub enabled: Option<bool>,
    /// Parent menu item ID (for sub-menus).
    pub parent_id: Option<String>,
    /// URL pattern for when to show the item.
    pub document_url_patterns: Option<Vec<String>>,
    /// The type of menu item. Defaults to "normal".
    pub item_type: Option<MenuItemType>,
    /// Only show when this string is selected.
    pub visible: Option<bool>,
    /// Only show when the target element matches this CSS selector.
    pub selector: Option<String>,
}

/// Information about a clicked context menu item.
#[derive(Debug, Clone)]
pub struct MenuClickInfo {
    /// The ID of the menu item clicked.
    pub menu_item_id: String,
    /// The parent ID, if any.
    pub parent_menu_item_id: Option<String>,
    /// The context in which the click occurred.
    pub context: ContextType,
    /// Whether a checkbox/radio item is checked after the click.
    pub checked: Option<bool>,
    /// The URL of the page where the click occurred.
    pub page_url: Option<String>,
    /// If context is "link", the URL of the link.
    pub link_url: Option<String>,
    /// If context is "image", the src of the image.
    pub src_url: Option<String>,
    /// If context is "selection", the selected text.
    pub selection_text: Option<String>,
}

/// A registered context menu item.
#[derive(Debug, Clone)]
pub struct MenuItem {
    pub id: String,
    pub extension_id: String,
    pub title: Option<String>,
    pub contexts: Vec<ContextType>,
    pub checked: Option<bool>,
    pub enabled: bool,
    pub parent_id: Option<String>,
    pub item_type: MenuItemType,
    pub document_url_patterns: Option<Vec<String>>,
}

/// Extension context menus API.
pub trait ContextMenusApi: Send + Sync {
    /// Create a context menu item. Returns the item ID.
    fn create(&self, params: MenuCreateParams) -> Result<String>;

    /// Update a previously created menu item.
    fn update(&self, id: &str, params: MenuCreateParams) -> Result<bool>;

    /// Remove a menu item by ID.
    fn remove(&self, id: &str) -> Result<bool>;

    /// Remove all menu items for this extension.
    fn remove_all(&self) -> Result<bool>;

    /// Register a callback for when a menu item is clicked.
    fn on_clicked(&self, callback: Arc<dyn Fn(MenuClickInfo) + Send + Sync>);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_type_parse() {
        assert_eq!(ContextType::parse("all"), ContextType::All);
        assert_eq!(ContextType::parse("selection"), ContextType::Selection);
        assert_eq!(ContextType::parse("link"), ContextType::Link);
        assert_eq!(ContextType::parse("unknown"), ContextType::Page);
    }

    #[test]
    fn test_menu_item_type_parse() {
        assert_eq!(MenuItemType::parse("normal"), MenuItemType::Normal);
        assert_eq!(MenuItemType::parse("checkbox"), MenuItemType::Checkbox);
        assert_eq!(MenuItemType::parse("separator"), MenuItemType::Separator);
    }

    #[test]
    fn test_menu_create_params_defaults() {
        let params = MenuCreateParams {
            id: Some("test".into()),
            title: Some("Test Item".into()),
            contexts: None,
            checked: None,
            enabled: None,
            parent_id: None,
            document_url_patterns: None,
            item_type: None,
            visible: None,
            selector: None,
        };
        assert_eq!(params.id.as_deref(), Some("test"));
    }
}
