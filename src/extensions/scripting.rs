use std::sync::Arc;
use std::sync::RwLock;

use crate::extensions::types::{FrameId, Result, TabId, UrlPattern};

/// A pending script or CSS injection queued by the Scripting API.
#[derive(Debug, Clone)]
pub struct PendingInjection {
    /// Target tab ID.
    pub tab_id: TabId,
    /// Target frame IDs (None = all frames).
    pub frame_ids: Option<Vec<FrameId>>,
    /// The JavaScript code to execute.
    pub js_code: Option<String>,
    /// The CSS code to insert.
    pub css_code: Option<String>,
    /// Unique injection key for removal (used by remove_css).
    pub key: Option<String>,
}

/// Shared queue for pending script/CSS injections.
/// Pushed by ScriptingApi, drained by frame_tasks during navigation.
#[derive(Debug, Clone, Default)]
pub struct PendingInjections {
    injections: Arc<RwLock<Vec<PendingInjection>>>,
}

impl PendingInjections {
    pub fn new() -> Self {
        Self {
            injections: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Queue a JS injection.
    pub fn push_js(&self, tab_id: TabId, js_code: String) {
        self.injections
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .push(PendingInjection {
                tab_id,
                frame_ids: None,
                js_code: Some(js_code),
                css_code: None,
                key: None,
            });
    }

    /// Queue a CSS injection with a key for later removal.
    pub fn push_css(&self, tab_id: TabId, css_code: String, key: String) {
        self.injections
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .push(PendingInjection {
                tab_id,
                frame_ids: None,
                js_code: None,
                css_code: Some(css_code),
                key: Some(key),
            });
    }

    /// Drain all pending injections.
    pub fn drain(&self) -> Vec<PendingInjection> {
        let mut injections = self.injections.write().unwrap_or_else(|e| e.into_inner());
        std::mem::take(&mut *injections)
    }
}

/// An extension content script registered at load time.
#[derive(Debug, Clone)]
pub struct ExtensionContentScriptEntry {
    pub extension_id: String,
    pub script_id: String,
    pub js_code: String,
    pub css_code: String,
    pub matches: Vec<String>,
    pub run_at: ExtensionRunAt,
}

/// Timing for extension content script injection.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ExtensionRunAt {
    DocumentStart,
    DocumentEnd,
    #[default]
    DocumentIdle,
}

/// Shared registry for extension content scripts.
/// Passed to ContentScriptManager, AileronScriptingApi, and ExtensionManager.
#[derive(Debug, Clone, Default)]
pub struct ExtensionContentScriptRegistry {
    scripts: Arc<RwLock<Vec<ExtensionContentScriptEntry>>>,
}

impl ExtensionContentScriptRegistry {
    pub fn new() -> Self {
        Self {
            scripts: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn register(&self, entry: ExtensionContentScriptEntry) {
        let mut scripts = self.scripts.write().unwrap_or_else(|e| e.into_inner());
        scripts
            .retain(|s| !(s.extension_id == entry.extension_id && s.script_id == entry.script_id));
        scripts.push(entry);
    }

    pub fn unregister_by_extension(&self, extension_id: &str) {
        let mut scripts = self.scripts.write().unwrap_or_else(|e| e.into_inner());
        scripts.retain(|s| s.extension_id != extension_id);
    }

    pub fn unregister_by_id(&self, script_id: &str) {
        let mut scripts = self.scripts.write().unwrap_or_else(|e| e.into_inner());
        scripts.retain(|s| s.script_id != script_id);
    }

    pub fn scripts_for_url(
        &self,
        url: &str,
        run_at: ExtensionRunAt,
    ) -> Vec<ExtensionContentScriptEntry> {
        let scripts = self.scripts.read().unwrap_or_else(|e| e.into_inner());
        scripts
            .iter()
            .filter(|s| s.run_at == run_at && s.matches.iter().any(|p| url_matches_pattern(url, p)))
            .cloned()
            .collect()
    }

    pub fn all_scripts(&self) -> Vec<ExtensionContentScriptEntry> {
        self.scripts
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

fn url_matches_pattern(url: &str, pattern: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.is_empty() {
        return url == pattern;
    }
    let mut pos = 0;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if let Some(found) = url[pos..].find(part) {
            pos += found + part.len();
        } else {
            return false;
        }
        if i == parts.len() - 1 && !pattern.ends_with('*') && pos != url.len() {
            return false;
        }
    }
    true
}

/// Target for script/CSS injection.
#[derive(Debug, Clone)]
pub struct InjectionTarget {
    pub tab_id: TabId,
    pub frame_ids: Option<Vec<FrameId>>,
    pub all_frames: bool,
}

/// Script injection parameters.
#[derive(Debug, Clone)]
pub enum ScriptInjection {
    Function {
        func: String,
        args: Vec<serde_json::Value>,
    },
    File {
        file: String,
    },
}

/// CSS injection parameters.
#[derive(Debug, Clone)]
pub enum CssInjection {
    Css { css: String },
    File { file: String },
}

/// Where to inject CSS: "author" (page) or "user" (user stylesheet).
#[derive(Debug, Clone, Default)]
pub enum CssOrigin {
    #[default]
    Author,
    User,
}

/// Result of a script injection.
#[derive(Debug, Clone)]
pub struct InjectionResult {
    pub frame_id: FrameId,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
}

/// A dynamically registered content script.
#[derive(Debug, Clone)]
pub struct RegisteredContentScript {
    pub id: String,
    pub js: Vec<String>,
    pub css: Vec<String>,
    pub matches: Vec<UrlPattern>,
    pub exclude_matches: Vec<UrlPattern>,
    pub run_at: RunAt,
    pub all_frames: bool,
    pub match_about_blank: bool,
}

/// When to inject relative to page load.
#[derive(Debug, Clone, Default)]
pub enum RunAt {
    DocumentIdle,
    #[default]
    DocumentStart,
    DocumentEnd,
}

/// Filter for querying registered content scripts.
#[derive(Debug, Clone)]
pub struct ScriptFilter {
    pub ids: Option<Vec<String>>,
}

/// Content script injection and management.
pub trait ScriptingApi: Send + Sync {
    fn execute_script(
        &self,
        target: InjectionTarget,
        injection: ScriptInjection,
    ) -> Result<Vec<InjectionResult>>;

    fn insert_css(&self, target: InjectionTarget, injection: CssInjection) -> Result<()>;

    fn remove_css(&self, target: InjectionTarget, injection: CssInjection) -> Result<()>;

    fn register_content_scripts(&self, scripts: Vec<RegisteredContentScript>) -> Result<()>;

    fn get_registered_content_scripts(
        &self,
        filter: Option<ScriptFilter>,
    ) -> Result<Vec<RegisteredContentScript>>;

    fn unregister_content_scripts(&self, filter: Option<ScriptFilter>) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_injection_target() {
        let target = InjectionTarget {
            tab_id: TabId(1),
            frame_ids: Some(vec![FrameId(0)]),
            all_frames: false,
        };
        assert_eq!(target.tab_id, TabId(1));
        assert!(!target.all_frames);
    }

    #[test]
    fn test_script_injection_function() {
        let inj = ScriptInjection::Function {
            func: "function() { return 42; }".into(),
            args: vec![serde_json::json!(1), serde_json::json!("hello")],
        };
        match inj {
            ScriptInjection::Function { func, args } => {
                assert!(func.contains("return 42"));
                assert_eq!(args.len(), 2);
            }
            _ => panic!("Expected Function"),
        }
    }

    #[test]
    fn test_script_injection_file() {
        let inj = ScriptInjection::File {
            file: "content.js".into(),
        };
        match inj {
            ScriptInjection::File { file } => assert_eq!(file, "content.js"),
            _ => panic!("Expected File"),
        }
    }

    #[test]
    fn test_css_injection() {
        let inj = CssInjection::Css {
            css: "body { background: red; }".into(),
        };
        match inj {
            CssInjection::Css { css } => assert!(css.contains("background")),
            _ => panic!("Expected Css"),
        }
    }

    #[test]
    fn test_css_origin_default() {
        let origin = CssOrigin::default();
        match origin {
            CssOrigin::Author => {}
            CssOrigin::User => panic!("Expected Author"),
        }
    }

    #[test]
    fn test_injection_result() {
        let result = InjectionResult {
            frame_id: FrameId(0),
            result: Some(serde_json::json!(42)),
            error: None,
        };
        assert!(result.error.is_none());
        assert!(result.result.is_some());
    }

    #[test]
    fn test_registered_content_script() {
        let script = RegisteredContentScript {
            id: "my-script".into(),
            js: vec!["script.js".into()],
            css: vec![],
            matches: vec![UrlPattern("*://*/*".into())],
            exclude_matches: vec![],
            run_at: RunAt::DocumentStart,
            all_frames: false,
            match_about_blank: false,
        };
        assert_eq!(script.id, "my-script");
        assert_eq!(script.js.len(), 1);
    }

    #[test]
    fn test_script_filter() {
        let filter = ScriptFilter {
            ids: Some(vec!["a".into(), "b".into()]),
        };
        assert_eq!(filter.ids.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn test_run_at_default() {
        let run_at = RunAt::default();
        match run_at {
            RunAt::DocumentStart => {}
            _ => panic!("Expected DocumentStart"),
        }
    }

    #[test]
    fn test_registry_register_and_retrieve() {
        let registry = ExtensionContentScriptRegistry::new();
        registry.register(ExtensionContentScriptEntry {
            extension_id: "ext1".into(),
            script_id: "ext1-0".into(),
            js_code: "console.log(1)".into(),
            css_code: String::new(),
            matches: vec!["https://*.example.com/*".into()],
            run_at: ExtensionRunAt::DocumentIdle,
        });
        let all = registry.all_scripts();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].script_id, "ext1-0");
    }

    #[test]
    fn test_registry_unregister_by_extension() {
        let registry = ExtensionContentScriptRegistry::new();
        registry.register(ExtensionContentScriptEntry {
            extension_id: "ext-a".into(),
            script_id: "ext-a-0".into(),
            js_code: String::new(),
            css_code: String::new(),
            matches: vec!["*://*/*".into()],
            run_at: ExtensionRunAt::DocumentIdle,
        });
        registry.register(ExtensionContentScriptEntry {
            extension_id: "ext-b".into(),
            script_id: "ext-b-0".into(),
            js_code: String::new(),
            css_code: String::new(),
            matches: vec!["*://*/*".into()],
            run_at: ExtensionRunAt::DocumentIdle,
        });
        registry.unregister_by_extension("ext-a");
        let all = registry.all_scripts();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].extension_id, "ext-b");
    }

    #[test]
    fn test_registry_unregister_by_id() {
        let registry = ExtensionContentScriptRegistry::new();
        registry.register(ExtensionContentScriptEntry {
            extension_id: "ext1".into(),
            script_id: "script-1".into(),
            js_code: String::new(),
            css_code: String::new(),
            matches: vec!["*://*/*".into()],
            run_at: ExtensionRunAt::DocumentIdle,
        });
        registry.register(ExtensionContentScriptEntry {
            extension_id: "ext1".into(),
            script_id: "script-2".into(),
            js_code: String::new(),
            css_code: String::new(),
            matches: vec!["*://*/*".into()],
            run_at: ExtensionRunAt::DocumentIdle,
        });
        registry.unregister_by_id("script-1");
        let all = registry.all_scripts();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].script_id, "script-2");
    }

    #[test]
    fn test_registry_scripts_for_url_filters_by_run_at() {
        let registry = ExtensionContentScriptRegistry::new();
        registry.register(ExtensionContentScriptEntry {
            extension_id: "ext1".into(),
            script_id: "start".into(),
            js_code: "console.log('start')".into(),
            css_code: String::new(),
            matches: vec!["https://*.example.com/*".into()],
            run_at: ExtensionRunAt::DocumentStart,
        });
        registry.register(ExtensionContentScriptEntry {
            extension_id: "ext1".into(),
            script_id: "idle".into(),
            js_code: "console.log('idle')".into(),
            css_code: String::new(),
            matches: vec!["https://*.example.com/*".into()],
            run_at: ExtensionRunAt::DocumentIdle,
        });

        let start = registry.scripts_for_url(
            "https://www.example.com/page",
            ExtensionRunAt::DocumentStart,
        );
        assert_eq!(start.len(), 1);
        assert_eq!(start[0].script_id, "start");

        let idle =
            registry.scripts_for_url("https://www.example.com/page", ExtensionRunAt::DocumentIdle);
        assert_eq!(idle.len(), 1);
        assert_eq!(idle[0].script_id, "idle");
    }

    #[test]
    fn test_registry_url_matches_pattern() {
        let registry = ExtensionContentScriptRegistry::new();
        registry.register(ExtensionContentScriptEntry {
            extension_id: "ext1".into(),
            script_id: "s1".into(),
            js_code: String::new(),
            css_code: String::new(),
            matches: vec!["https://*.github.com/*".into()],
            run_at: ExtensionRunAt::DocumentIdle,
        });

        assert_eq!(
            registry
                .scripts_for_url("https://api.github.com/repo", ExtensionRunAt::DocumentIdle)
                .len(),
            1
        );
        assert!(registry
            .scripts_for_url("https://google.com", ExtensionRunAt::DocumentIdle)
            .is_empty());
    }

    #[test]
    fn test_registry_deduplicates_on_re_register() {
        let registry = ExtensionContentScriptRegistry::new();
        registry.register(ExtensionContentScriptEntry {
            extension_id: "ext1".into(),
            script_id: "ext1-0".into(),
            js_code: "old".into(),
            css_code: String::new(),
            matches: vec!["*://*/*".into()],
            run_at: ExtensionRunAt::DocumentIdle,
        });
        registry.register(ExtensionContentScriptEntry {
            extension_id: "ext1".into(),
            script_id: "ext1-0".into(),
            js_code: "new".into(),
            css_code: String::new(),
            matches: vec!["*://*/*".into()],
            run_at: ExtensionRunAt::DocumentIdle,
        });
        let all = registry.all_scripts();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].js_code, "new");
    }
}
