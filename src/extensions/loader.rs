use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::extensions::impls::AileronExtensionApi;
use crate::extensions::manifest::ExtensionManifest;
use crate::extensions::message_bus::MessageBus;
use crate::extensions::runtime::{InstallReason, InstalledDetails};
use crate::extensions::scripting::{
    ExtensionContentScriptEntry, ExtensionContentScriptRegistry, ExtensionRunAt,
};
use crate::extensions::types::{BackgroundScript, ExtensionError, ExtensionId};

pub struct ExtensionManager {
    extensions: HashMap<ExtensionId, AileronExtensionApi>,
    extensions_dir: PathBuf,
    storage_dir: PathBuf,
    content_script_registry: ExtensionContentScriptRegistry,
    message_bus: std::sync::Arc<MessageBus>,
}

impl ExtensionManager {
    pub fn new(extensions_dir: PathBuf) -> Self {
        let storage_dir = extensions_dir.join("storage");
        Self {
            extensions: HashMap::new(),
            extensions_dir,
            storage_dir,
            content_script_registry: ExtensionContentScriptRegistry::new(),
            message_bus: std::sync::Arc::new(MessageBus::new()),
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

    /// Fire `on_startup` lifecycle callbacks for all loaded extensions.
    /// Call this after `load_all()` to signal browser startup.
    pub fn fire_all_startup(&self) {
        for (id, api) in &self.extensions {
            api.fire_startup();
            tracing::debug!(
                target: "extensions",
                "Fired on_startup for extension '{}'",
                id.0
            );
        }
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

        // Load background script if specified in manifest
        let background_script = manifest.background.as_ref().and_then(|bg| {
            // Prefer service_worker (MV3) over scripts array (MV2 fallback)
            let script_name = bg
                .service_worker
                .clone()
                .or_else(|| bg.scripts.as_ref().and_then(|s| s.first().cloned()));

            let filename = script_name?;
            let path = ext_dir.join(&filename);

            if !path.exists() {
                tracing::warn!(
                    target: "extensions",
                    "Background script file not found: {:?}",
                    path
                );
                return None;
            }

            let source = std::fs::read_to_string(&path).ok()?;
            Some(BackgroundScript { source, filename })
        });

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

        let mut api = AileronExtensionApi::with_registry_and_storage(
            id.clone(),
            manifest,
            self.content_script_registry.clone(),
            Some(self.storage_dir.clone()),
            None, // tab_provider: wired via set_tab_provider() after construction
            Some(self.message_bus.clone()),
        );

        // Store background script if one was loaded from the manifest
        if let Some(script) = background_script {
            tracing::info!(
                target: "extensions",
                "Loaded background script '{}' for extension '{}'",
                script.filename,
                id.0
            );
            api.set_background_script(script);
        }

        self.extensions.insert(id.clone(), api);

        // Fire on_installed lifecycle event
        // Note: in the current implementation, extensions don't register on_installed
        // handlers until their background script executes (future work). This fires
        // any handlers that were registered programmatically during loading.
        let installed_details = InstalledDetails {
            reason: InstallReason::Install,
            previous_version: None,
            id: id.clone(),
        };
        if let Some(loaded_api) = self.extensions.get(&id) {
            loaded_api.fire_installed(installed_details);
        }

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

    /// Get a reference to the shared message bus for external message routing.
    pub fn message_bus(&self) -> &std::sync::Arc<MessageBus> {
        &self.message_bus
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extensions::api::ExtensionApi;
    use std::sync::Arc;

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

    #[test]
    fn test_load_extension_with_service_worker() {
        let dir = tempfile::tempdir().unwrap();
        let ext_dir = dir.path().join("sw-ext");
        std::fs::create_dir_all(&ext_dir).unwrap();
        std::fs::write(
            ext_dir.join("manifest.json"),
            r#"{
                "manifest_version": 3,
                "name": "Service Worker Extension",
                "version": "1.0.0",
                "background": {
                    "service_worker": "background.js"
                }
            }"#,
        )
        .unwrap();
        std::fs::write(
            ext_dir.join("background.js"),
            "// sw code\nconsole.log('sw');",
        )
        .unwrap();

        let mut manager = ExtensionManager::new(dir.path().to_path_buf());
        let loaded = manager.load_all();
        assert_eq!(loaded.len(), 1);

        let api = manager.get(&loaded[0]).unwrap();
        let bg = api.background_script();
        assert!(bg.is_some(), "Background script should be loaded");
        let bg = bg.unwrap();
        assert_eq!(bg.filename, "background.js");
        assert!(bg.source.contains("sw code"));
    }

    #[test]
    fn test_load_extension_with_mv2_background_scripts() {
        let dir = tempfile::tempdir().unwrap();
        let ext_dir = dir.path().join("mv2-ext");
        std::fs::create_dir_all(&ext_dir).unwrap();
        std::fs::write(
            ext_dir.join("manifest.json"),
            r#"{
                "manifest_version": 3,
                "name": "MV2 Background Extension",
                "version": "1.0.0",
                "background": {
                    "scripts": ["bg1.js", "bg2.js"]
                }
            }"#,
        )
        .unwrap();
        std::fs::write(ext_dir.join("bg1.js"), "// first script").unwrap();
        std::fs::write(ext_dir.join("bg2.js"), "// second script").unwrap();

        let mut manager = ExtensionManager::new(dir.path().to_path_buf());
        let loaded = manager.load_all();
        assert_eq!(loaded.len(), 1);

        let api = manager.get(&loaded[0]).unwrap();
        let bg = api.background_script();
        assert!(bg.is_some(), "Should load first script from scripts array");
        let bg = bg.unwrap();
        assert_eq!(
            bg.filename, "bg1.js",
            "Should prefer first entry in scripts array"
        );
    }

    #[test]
    fn test_load_extension_service_worker_takes_precedence_over_scripts() {
        let dir = tempfile::tempdir().unwrap();
        let ext_dir = dir.path().join("both-bg-ext");
        std::fs::create_dir_all(&ext_dir).unwrap();
        std::fs::write(
            ext_dir.join("manifest.json"),
            r#"{
                "manifest_version": 3,
                "name": "Both Background Extension",
                "version": "1.0.0",
                "background": {
                    "service_worker": "sw.js",
                    "scripts": ["legacy.js"]
                }
            }"#,
        )
        .unwrap();
        std::fs::write(ext_dir.join("sw.js"), "// service worker").unwrap();
        std::fs::write(ext_dir.join("legacy.js"), "// legacy").unwrap();

        let mut manager = ExtensionManager::new(dir.path().to_path_buf());
        let loaded = manager.load_all();
        assert_eq!(loaded.len(), 1);

        let api = manager.get(&loaded[0]).unwrap();
        let bg = api.background_script().unwrap();
        assert_eq!(
            bg.filename, "sw.js",
            "service_worker should take precedence over scripts"
        );
    }

    #[test]
    fn test_load_extension_missing_background_script_file() {
        let dir = tempfile::tempdir().unwrap();
        let ext_dir = dir.path().join("missing-bg-ext");
        std::fs::create_dir_all(&ext_dir).unwrap();
        std::fs::write(
            ext_dir.join("manifest.json"),
            r#"{
                "manifest_version": 3,
                "name": "Missing BG Extension",
                "version": "1.0.0",
                "background": {
                    "service_worker": "nonexistent.js"
                }
            }"#,
        )
        .unwrap();
        // Do NOT create the background.js file

        let mut manager = ExtensionManager::new(dir.path().to_path_buf());
        let loaded = manager.load_all();
        // Extension should still load (background script is optional)
        assert_eq!(loaded.len(), 1);

        let api = manager.get(&loaded[0]).unwrap();
        assert!(
            api.background_script().is_none(),
            "Missing background script file should result in None"
        );
    }

    #[test]
    fn test_load_extension_no_background_field() {
        let dir = tempfile::tempdir().unwrap();
        let ext_dir = dir.path().join("no-bg-ext");
        std::fs::create_dir_all(&ext_dir).unwrap();
        std::fs::write(
            ext_dir.join("manifest.json"),
            r#"{
                "manifest_version": 3,
                "name": "No Background Extension",
                "version": "1.0.0"
            }"#,
        )
        .unwrap();

        let mut manager = ExtensionManager::new(dir.path().to_path_buf());
        let loaded = manager.load_all();
        assert_eq!(loaded.len(), 1);

        let api = manager.get(&loaded[0]).unwrap();
        assert!(api.background_script().is_none());
    }

    #[test]
    fn test_load_extension_empty_scripts_array() {
        let dir = tempfile::tempdir().unwrap();
        let ext_dir = dir.path().join("empty-scripts-ext");
        std::fs::create_dir_all(&ext_dir).unwrap();
        std::fs::write(
            ext_dir.join("manifest.json"),
            r#"{
                "manifest_version": 3,
                "name": "Empty Scripts Extension",
                "version": "1.0.0",
                "background": {
                    "scripts": []
                }
            }"#,
        )
        .unwrap();

        let mut manager = ExtensionManager::new(dir.path().to_path_buf());
        let loaded = manager.load_all();
        assert_eq!(loaded.len(), 1);

        let api = manager.get(&loaded[0]).unwrap();
        assert!(
            api.background_script().is_none(),
            "Empty scripts array should yield None"
        );
    }

    #[test]
    fn test_fire_installed_callback() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let dir = tempfile::tempdir().unwrap();
        let ext_dir = dir.path().join("callback-ext");
        std::fs::create_dir_all(&ext_dir).unwrap();
        std::fs::write(
            ext_dir.join("manifest.json"),
            r#"{
                "manifest_version": 3,
                "name": "Callback Extension",
                "version": "1.0.0"
            }"#,
        )
        .unwrap();

        let mut manager = ExtensionManager::new(dir.path().to_path_buf());
        let loaded = manager.load_all();
        assert_eq!(loaded.len(), 1);

        // Register an on_installed callback via the runtime API
        let fire_count = Arc::new(AtomicUsize::new(0));
        let count_clone = fire_count.clone();
        let ext_id = loaded[0].clone();
        manager
            .get(&ext_id)
            .unwrap()
            .runtime()
            .on_installed(Box::new(move |_details| {
                count_clone.fetch_add(1, Ordering::SeqCst);
            }));

        // Fire installed via a reload cycle: load_extension fires it automatically.
        // For this test, verify the callback can be invoked manually.
        let api = manager.get(&ext_id).unwrap();
        api.fire_installed(InstalledDetails {
            reason: InstallReason::Install,
            previous_version: None,
            id: ext_id.clone(),
        });
        assert_eq!(
            fire_count.load(Ordering::SeqCst),
            1,
            "on_installed should have been called"
        );
    }

    #[test]
    fn test_fire_all_startup() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let dir = tempfile::tempdir().unwrap();

        // Create two extensions
        for name in &["startup-a", "startup-b"] {
            let ext_dir = dir.path().join(name);
            std::fs::create_dir_all(&ext_dir).unwrap();
            std::fs::write(
                ext_dir.join("manifest.json"),
                format!(
                    r#"{{
                    "manifest_version": 3,
                    "name": "Startup {}",
                    "version": "1.0.0"
                }}"#,
                    name
                ),
            )
            .unwrap();
        }

        let mut manager = ExtensionManager::new(dir.path().to_path_buf());
        let loaded = manager.load_all();
        assert_eq!(loaded.len(), 2);

        // Register startup callbacks
        let fire_count = Arc::new(AtomicUsize::new(0));
        for id in &loaded {
            let count_clone = fire_count.clone();
            manager
                .get(id)
                .unwrap()
                .runtime()
                .on_startup(Box::new(move || {
                    count_clone.fetch_add(1, Ordering::SeqCst);
                }));
        }

        manager.fire_all_startup();
        assert_eq!(
            fire_count.load(Ordering::SeqCst),
            2,
            "Both extensions' on_startup should have been called"
        );
    }
}
