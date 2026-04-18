use crate::extensions::types::{FrameId, Result, TabId, UrlPattern};

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
}
