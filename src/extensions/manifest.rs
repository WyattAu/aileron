use std::collections::HashMap;

/// Parsed extension manifest (manifest.json → Manifest V3).
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ExtensionManifest {
    pub manifest_version: u32,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub optional_permissions: Vec<String>,
    #[serde(default)]
    pub host_permissions: Vec<String>,
    pub background: Option<Background>,
    pub content_scripts: Option<Vec<ContentScript>>,
    pub action: Option<Action>,
    pub options_page: Option<String>,
    pub options_ui: Option<OptionsUi>,
    pub web_accessible_resources: Option<Vec<String>>,
    pub commands: Option<HashMap<String, Command>>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Background {
    pub service_worker: Option<String>,
    pub scripts: Option<Vec<String>>,
    pub persistent: Option<bool>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ContentScript {
    pub matches: Vec<String>,
    pub js: Option<Vec<String>>,
    pub css: Option<Vec<String>>,
    pub run_at: Option<String>,
    #[serde(default)]
    pub all_frames: bool,
    #[serde(default)]
    pub match_about_blank: bool,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Action {
    pub default_title: Option<String>,
    pub default_icon: Option<IconValue>,
    pub default_popup: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct OptionsUi {
    pub page: String,
    pub open_in_tab: Option<bool>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Command {
    pub description: Option<String>,
    pub suggested_key: Option<SuggestedKey>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct SuggestedKey {
    pub default: Option<String>,
    pub mac: Option<String>,
    pub windows: Option<String>,
    pub chromeos: Option<String>,
    pub linux: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(untagged)]
pub enum IconValue {
    Single(String),
    Sized(HashMap<String, String>),
}

impl ExtensionManifest {
    /// Parse a manifest from a JSON string (manifest.json V3 format).
    pub fn from_json(json: &str) -> crate::extensions::Result<Self> {
        serde_json::from_str(json)
            .map_err(|e| crate::extensions::ExtensionError::Serialization(e.to_string()))
    }

    /// Check if the manifest declares a specific permission.
    pub fn has_permission(&self, permission: &str) -> bool {
        self.permissions.iter().any(|p| p == permission)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minimal_manifest() {
        let json = r#"{
            "manifest_version": 3,
            "name": "Test Extension",
            "version": "1.0.0"
        }"#;
        let manifest = ExtensionManifest::from_json(json).unwrap();
        assert_eq!(manifest.manifest_version, 3);
        assert_eq!(manifest.name, "Test Extension");
        assert_eq!(manifest.version, "1.0.0");
        assert!(manifest.description.is_none());
        assert!(manifest.permissions.is_empty());
        assert!(manifest.content_scripts.is_none());
        assert!(manifest.background.is_none());
    }

    #[test]
    fn test_parse_full_manifest() {
        let json = r#"{
            "manifest_version": 3,
            "name": "Ad Blocker",
            "version": "2.1.0",
            "description": "Blocks ads and trackers",
            "permissions": ["tabs", "storage", "webRequest", "webRequestBlocking"],
            "host_permissions": ["*://*.example.com/*"],
            "background": {
                "service_worker": "background.js"
            },
            "content_scripts": [{
                "matches": ["*://*.example.com/*"],
                "js": ["content.js"],
                "css": ["styles.css"],
                "run_at": "document_start",
                "all_frames": true
            }],
            "action": {
                "default_title": "Block ads",
                "default_popup": "popup.html",
                "default_icon": "icon.png"
            },
            "options_page": "options.html",
            "commands": {
                "toggle-blocking": {
                    "description": "Toggle ad blocking",
                    "suggested_key": {
                        "default": "Ctrl+Shift+B",
                        "mac": "Command+Shift+B"
                    }
                }
            }
        }"#;
        let manifest = ExtensionManifest::from_json(json).unwrap();
        assert_eq!(manifest.name, "Ad Blocker");
        assert_eq!(
            manifest.description.as_deref(),
            Some("Blocks ads and trackers")
        );
        assert_eq!(manifest.permissions.len(), 4);
        assert_eq!(manifest.host_permissions.len(), 1);
        assert!(manifest.background.is_some());
        assert!(manifest.content_scripts.is_some());
        assert!(manifest.action.is_some());
        assert!(manifest.commands.is_some());
    }

    #[test]
    fn test_has_permission() {
        let json = r#"{
            "manifest_version": 3,
            "name": "Test",
            "version": "1.0.0",
            "permissions": ["tabs", "storage"]
        }"#;
        let manifest = ExtensionManifest::from_json(json).unwrap();
        assert!(manifest.has_permission("tabs"));
        assert!(manifest.has_permission("storage"));
        assert!(!manifest.has_permission("webRequest"));
    }

    #[test]
    fn test_parse_invalid_json() {
        let result = ExtensionManifest::from_json("not json");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_missing_required_fields() {
        let json = r#"{"name": "Test"}"#;
        let result = ExtensionManifest::from_json(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_content_script_defaults() {
        let json = r#"{
            "manifest_version": 3,
            "name": "Test",
            "version": "1.0.0",
            "content_scripts": [{
                "matches": ["*://*/*"],
                "js": ["script.js"]
            }]
        }"#;
        let manifest = ExtensionManifest::from_json(json).unwrap();
        let cs = &manifest.content_scripts.unwrap()[0];
        assert!(!cs.all_frames);
        assert!(!cs.match_about_blank);
        assert!(cs.run_at.is_none());
        assert!(cs.css.is_none());
    }

    #[test]
    fn test_icon_value_single() {
        let json = r#"{
            "manifest_version": 3,
            "name": "Test",
            "version": "1.0.0",
            "action": {
                "default_icon": "icon.png"
            }
        }"#;
        let manifest = ExtensionManifest::from_json(json).unwrap();
        let action = manifest.action.unwrap();
        match action.default_icon.unwrap() {
            IconValue::Single(s) => assert_eq!(s, "icon.png"),
            IconValue::Sized(_) => panic!("Expected Single variant"),
        }
    }

    #[test]
    fn test_icon_value_sized() {
        let json = r#"{
            "manifest_version": 3,
            "name": "Test",
            "version": "1.0.0",
            "action": {
                "default_icon": {
                    "16": "icon16.png",
                    "48": "icon48.png"
                }
            }
        }"#;
        let manifest = ExtensionManifest::from_json(json).unwrap();
        let action = manifest.action.unwrap();
        match action.default_icon.unwrap() {
            IconValue::Single(_) => panic!("Expected Sized variant"),
            IconValue::Sized(map) => {
                assert_eq!(map.get("16").unwrap(), "icon16.png");
                assert_eq!(map.get("48").unwrap(), "icon48.png");
            }
        }
    }
}
