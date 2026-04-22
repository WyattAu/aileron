use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::extensions::impls::AileronExtensionApi;
use crate::extensions::manifest::ExtensionManifest;
use crate::extensions::scripting::{
    ExtensionContentScriptEntry, ExtensionContentScriptRegistry, ExtensionRunAt,
};
use crate::extensions::types::{ExtensionError, ExtensionId};

pub struct ExtensionManager {
    extensions: HashMap<ExtensionId, AileronExtensionApi>,
    extensions_dir: PathBuf,
    storage_dir: PathBuf,
    content_script_registry: ExtensionContentScriptRegistry,
}

impl ExtensionManager {
    pub fn new(extensions_dir: PathBuf) -> Self {
        let storage_dir = extensions_dir.join("storage");
        Self {
            extensions: HashMap::new(),
            extensions_dir,
            storage_dir,
            content_script_registry: ExtensionContentScriptRegistry::new(),
        }
    }

    pub fn content_script_registry(&self) -> &ExtensionContentScriptRegistry {
        &self.content_script_registry
    }

    pub fn load_all(&mut self) -> Vec<ExtensionId> {
        let mut loaded = Vec::new();

        if !self.extensions_dir.exists() {
            if let Err(e) = std::fs::create_dir_all(&self.extensions_dir) {
                tracing::warn!(
                    target: "extensions",
                    "Cannot create extensions dir {:?}: {}",
                    self.extensions_dir,
                    e
                );
                return loaded;
            }
            return loaded;
        }

        let entries = match std::fs::read_dir(&self.extensions_dir) {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!(
                    target: "extensions",
                    "Cannot read extensions dir {:?}: {}",
                    self.extensions_dir,
                    e
                );
                return loaded;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let manifest_path = path.join("manifest.json");
            if !manifest_path.exists() {
                continue;
            }

            match self.load_extension(&manifest_path) {
                Ok(id) => {
                    tracing::info!(target: "extensions", "Loaded extension: {}", id);
                    loaded.push(id.clone());
                }
                Err(e) => {
                    tracing::warn!(
                        target: "extensions",
                        "Failed to load extension from {:?}: {}",
                        path,
                        e
                    );
                }
            }
        }

        loaded
    }

    fn load_extension(&mut self, manifest_path: &Path) -> Result<ExtensionId, ExtensionError> {
        let content = std::fs::read_to_string(manifest_path)
            .map_err(|e| ExtensionError::LoadFailed(format!("Cannot read manifest: {}", e)))?;

        let manifest: ExtensionManifest = serde_json::from_str(&content)
            .map_err(|e| ExtensionError::LoadFailed(format!("Invalid manifest JSON: {}", e)))?;

        let id = ExtensionId(
            manifest_path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string(),
        );

        let ext_dir = manifest_path.parent().unwrap_or(manifest_path);

        if let Some(content_scripts) = &manifest.content_scripts {
            for (i, cs) in content_scripts.iter().enumerate() {
                let js_code = cs
                    .js
                    .as_ref()
                    .map(|files| {
                        files
                            .iter()
                            .filter_map(|f| {
                                let file_path = ext_dir.join(f);
                                std::fs::read_to_string(&file_path).ok()
                            })
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .unwrap_or_default();

                let css_code = cs
                    .css
                    .as_ref()
                    .map(|files| {
                        files
                            .iter()
                            .filter_map(|f| {
                                let file_path = ext_dir.join(f);
                                std::fs::read_to_string(&file_path).ok()
                            })
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .unwrap_or_default();

                let run_at = match cs.run_at.as_deref() {
                    Some("document_start") => ExtensionRunAt::DocumentStart,
                    Some("document_end") => ExtensionRunAt::DocumentEnd,
                    _ => ExtensionRunAt::DocumentIdle,
                };

                let entry = ExtensionContentScriptEntry {
                    extension_id: id.0.clone(),
                    script_id: format!("{}-{}", id.0, i),
                    js_code,
                    css_code,
                    matches: cs.matches.clone(),
                    run_at,
                };
                self.content_script_registry.register(entry);
                tracing::info!(
                    target: "extensions",
                    "Registered {} content script(s) for extension '{}'",
                    content_scripts.len(),
                    id.0
                );
            }
        }

        let api = AileronExtensionApi::with_registry_and_storage(
            id.clone(),
            manifest,
            self.content_script_registry.clone(),
            Some(self.storage_dir.clone()),
            None, // tab_provider: wired via set_tab_provider() after construction
        );
        self.extensions.insert(id.clone(), api);
        Ok(id)
    }

    pub fn get(&self, id: &ExtensionId) -> Option<&AileronExtensionApi> {
        self.extensions.get(id)
    }

    pub fn list(&self) -> Vec<&ExtensionId> {
        self.extensions.keys().collect()
    }

    pub fn extensions_dir(&self) -> &Path {
        &self.extensions_dir
    }

    pub fn storage_dir(&self) -> &Path {
        &self.storage_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extension_manager_creation() {
        let dir = tempfile::tempdir().unwrap();
        let manager = ExtensionManager::new(dir.path().to_path_buf());
        assert!(manager.list().is_empty());
    }

    #[test]
    fn test_load_nonexistent_directory() {
        let dir = tempfile::tempdir().unwrap();
        let extensions_dir = dir.path().join("nonexistent");
        let mut manager = ExtensionManager::new(extensions_dir.clone());
        let loaded = manager.load_all();
        assert!(loaded.is_empty());
        assert!(extensions_dir.exists(), "Should create the directory");
    }

    #[test]
    fn test_load_empty_directory() {
        let dir = tempfile::tempdir().unwrap();
        let mut manager = ExtensionManager::new(dir.path().to_path_buf());
        let loaded = manager.load_all();
        assert!(loaded.is_empty());
    }

    #[test]
    fn test_load_valid_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let ext_dir = dir.path().join("my-extension");
        std::fs::create_dir_all(&ext_dir).unwrap();
        std::fs::write(
            ext_dir.join("manifest.json"),
            r#"{
                "manifest_version": 3,
                "name": "Test Extension",
                "version": "1.0.0"
            }"#,
        )
        .unwrap();

        let mut manager = ExtensionManager::new(dir.path().to_path_buf());
        let loaded = manager.load_all();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].as_ref(), "my-extension");

        let api = manager.get(&loaded[0]).unwrap();
        assert_eq!(api.manifest().name, "Test Extension");
        assert_eq!(api.manifest().version, "1.0.0");
    }

    #[test]
    fn test_load_invalid_json() {
        let dir = tempfile::tempdir().unwrap();
        let ext_dir = dir.path().join("bad-extension");
        std::fs::create_dir_all(&ext_dir).unwrap();
        std::fs::write(ext_dir.join("manifest.json"), "not json").unwrap();

        let mut manager = ExtensionManager::new(dir.path().to_path_buf());
        let loaded = manager.load_all();
        assert!(loaded.is_empty(), "Should skip invalid manifest gracefully");
    }

    #[test]
    fn test_load_missing_required_fields() {
        let dir = tempfile::tempdir().unwrap();
        let ext_dir = dir.path().join("incomplete");
        std::fs::create_dir_all(&ext_dir).unwrap();
        std::fs::write(ext_dir.join("manifest.json"), r#"{"name": "No Version"}"#).unwrap();

        let mut manager = ExtensionManager::new(dir.path().to_path_buf());
        let loaded = manager.load_all();
        assert!(loaded.is_empty());
    }

    #[test]
    fn test_load_multiple_extensions() {
        let dir = tempfile::tempdir().unwrap();

        for name in &["ext-a", "ext-b", "ext-c"] {
            let ext_dir = dir.path().join(name);
            std::fs::create_dir_all(&ext_dir).unwrap();
            std::fs::write(
                ext_dir.join("manifest.json"),
                format!(
                    r#"{{
                    "manifest_version": 3,
                    "name": "Extension {}",
                    "version": "1.0.0"
                }}"#,
                    name
                ),
            )
            .unwrap();
        }

        let mut manager = ExtensionManager::new(dir.path().to_path_buf());
        let loaded = manager.load_all();
        assert_eq!(loaded.len(), 3);
    }

    #[test]
    fn test_skip_files_without_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let ext_dir = dir.path().join("no-manifest");
        std::fs::create_dir_all(&ext_dir).unwrap();
        std::fs::write(ext_dir.join("readme.txt"), "no manifest here").unwrap();

        let mut manager = ExtensionManager::new(dir.path().to_path_buf());
        let loaded = manager.load_all();
        assert!(loaded.is_empty());
    }

    #[test]
    fn test_load_extension_registers_content_scripts() {
        let dir = tempfile::tempdir().unwrap();
        let ext_dir = dir.path().join("content-ext");
        std::fs::create_dir_all(&ext_dir).unwrap();
        std::fs::write(
            ext_dir.join("manifest.json"),
            r#"{
                "manifest_version": 3,
                "name": "Content Script Extension",
                "version": "1.0.0",
                "content_scripts": [{
                    "matches": ["https://*.example.com/*"],
                    "js": ["content.js"],
                    "css": ["style.css"],
                    "run_at": "document_start"
                }]
            }"#,
        )
        .unwrap();
        std::fs::write(ext_dir.join("content.js"), "console.log('injected');").unwrap();
        std::fs::write(ext_dir.join("style.css"), "body { border: 1px solid red; }").unwrap();

        let mut manager = ExtensionManager::new(dir.path().to_path_buf());
        let loaded = manager.load_all();
        assert_eq!(loaded.len(), 1);

        let registry = manager.content_script_registry();
        let all = registry.all_scripts();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].extension_id, "content-ext");
        assert!(all[0].script_id.starts_with("content-ext-"));
        assert!(all[0].js_code.contains("injected"));
        assert!(all[0].css_code.contains("border"));
        assert_eq!(all[0].matches, vec!["https://*.example.com/*"]);
    }

    #[test]
    fn test_load_extension_content_script_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let ext_dir = dir.path().join("missing-file-ext");
        std::fs::create_dir_all(&ext_dir).unwrap();
        std::fs::write(
            ext_dir.join("manifest.json"),
            r#"{
                "manifest_version": 3,
                "name": "Missing File Extension",
                "version": "1.0.0",
                "content_scripts": [{
                    "matches": ["*://*/*"],
                    "js": ["nonexistent.js"]
                }]
            }"#,
        )
        .unwrap();

        let mut manager = ExtensionManager::new(dir.path().to_path_buf());
        let loaded = manager.load_all();
        assert_eq!(loaded.len(), 1);

        let all = manager.content_script_registry().all_scripts();
        assert_eq!(all.len(), 1);
        assert!(all[0].js_code.is_empty());
    }
}
