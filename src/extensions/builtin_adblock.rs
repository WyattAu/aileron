use crate::extensions::manifest::ExtensionManifest;
use crate::extensions::types::ExtensionId;

pub fn builtin_adblock_id() -> ExtensionId {
    ExtensionId(String::from("aileron-adblock@builtin"))
}

const BUILTIN_ADBLOCK_MANIFEST_JSON: &str = r#"{
    "manifest_version": 3,
    "name": "Aileron AdBlock",
    "version": "1.0.0",
    "description": "Built-in adblock filter engine",
    "permissions": ["webRequest", "webRequestBlocking"],
    "host_permissions": ["<all_urls>"]
}"#;

pub fn builtin_adblock_manifest() -> ExtensionManifest {
    ExtensionManifest::from_json(BUILTIN_ADBLOCK_MANIFEST_JSON).expect("built-in adblock manifest is valid")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_adblock_id() {
        assert_eq!(builtin_adblock_id().as_ref(), "aileron-adblock@builtin");
    }

    #[test]
    fn test_builtin_adblock_manifest() {
        let manifest = builtin_adblock_manifest();
        assert_eq!(manifest.name, "Aileron AdBlock");
        assert_eq!(manifest.version, "1.0.0");
        assert_eq!(
            manifest.description.as_deref(),
            Some("Built-in adblock filter engine")
        );
        assert!(manifest.permissions.contains(&"webRequest".to_string()));
        assert!(manifest.permissions.contains(&"webRequestBlocking".to_string()));
        assert!(manifest.host_permissions.contains(&"<all_urls>".to_string()));
    }
}
