use crate::extensions::scripting::{
    CssInjection, ExtensionContentScriptEntry, ExtensionContentScriptRegistry, ExtensionRunAt,
    InjectionResult, InjectionTarget, RegisteredContentScript, RunAt, ScriptFilter,
    ScriptInjection, ScriptingApi,
};
use crate::extensions::types::{ExtensionError, Result, UrlPattern};

pub(super) struct AileronScriptingApi {
    pub(super) registry: ExtensionContentScriptRegistry,
    pending_injections: crate::extensions::scripting::PendingInjections,
}

impl AileronScriptingApi {
    pub(super) fn new(registry: ExtensionContentScriptRegistry) -> Self {
        Self {
            registry,
            pending_injections: crate::extensions::scripting::PendingInjections::new(),
        }
    }

    /// Get a handle to the pending injections queue.
    /// Used by frame_tasks to drain and execute injections.
    #[allow(dead_code)]
    pub fn pending_injections(&self) -> &crate::extensions::scripting::PendingInjections {
        &self.pending_injections
    }
}

impl ScriptingApi for AileronScriptingApi {
    fn execute_script(
        &self,
        target: InjectionTarget,
        injection: ScriptInjection,
    ) -> Result<Vec<InjectionResult>> {
        let code = match injection {
            ScriptInjection::Function { func, args } => {
                let args_json = serde_json::to_string(&args).unwrap_or_else(|_| "[]".into());
                format!("({func})({args_json})")
            }
            ScriptInjection::File { .. } => {
                // File-based injection: in Manifest V3, files are resolved at load time.
                // For programmatic API calls, we treat the file path as inline code.
                return Err(ExtensionError::Unsupported(
                    "scripting.executeScript with file injection not yet supported".into(),
                ));
            }
        };

        tracing::info!(
            target: "extensions",
            "scripting.executeScript(tab={}, {} bytes)",
            target.tab_id,
            code.len()
        );

        self.pending_injections.push_js(target.tab_id, code);

        // NOTE: This is an asynchronous injection. The script is queued via
        // `pending_injections` and will execute when the target frame loads or
        // on next navigation. `result: None` correctly indicates no synchronous
        // return value; callers that need the JS evaluation result should use
        // the messaging API instead.
        Ok(vec![InjectionResult {
            frame_id: target
                .frame_ids
                .as_ref()
                .and_then(|f| f.first().copied())
                .unwrap_or(crate::extensions::types::FrameId(0)),
            result: None,
            error: None,
        }])
    }

    fn insert_css(&self, target: InjectionTarget, injection: CssInjection) -> Result<()> {
        let css = match injection {
            CssInjection::Css { css } => css,
            CssInjection::File { .. } => {
                return Err(ExtensionError::Unsupported(
                    "scripting.insertCSS with file injection not yet supported".into(),
                ));
            }
        };

        let key = format!("aileron-ext-{}", uuid::Uuid::new_v4());

        tracing::info!(
            target: "extensions",
            "scripting.insertCSS(tab={}, {} bytes, key={})",
            target.tab_id,
            css.len(),
            key
        );

        self.pending_injections.push_css(target.tab_id, css, key);
        Ok(())
    }

    fn remove_css(&self, target: InjectionTarget, injection: CssInjection) -> Result<()> {
        let css_to_remove = match injection {
            CssInjection::Css { css } => css,
            CssInjection::File { .. } => {
                return Err(ExtensionError::Unsupported(
                    "scripting.removeCSS with file injection not yet supported".into(),
                ));
            }
        };

        let removal_js = format!(
            "document.querySelectorAll('style[data-aileron-css]').forEach(function(s) {{ \
                if (s.textContent.includes({css_to_remove:?})) s.remove(); \
            }});"
        );

        tracing::info!(
            target: "extensions",
            "scripting.removeCSS(tab={})",
            target.tab_id
        );

        self.pending_injections.push_js(target.tab_id, removal_js);
        Ok(())
    }

    fn register_content_scripts(&self, scripts: Vec<RegisteredContentScript>) -> Result<()> {
        for script in scripts {
            let run_at = match script.run_at {
                RunAt::DocumentIdle => ExtensionRunAt::DocumentIdle,
                RunAt::DocumentStart => ExtensionRunAt::DocumentStart,
                RunAt::DocumentEnd => ExtensionRunAt::DocumentEnd,
            };
            let entry = ExtensionContentScriptEntry {
                extension_id: String::new(),
                script_id: script.id.clone(),
                js_code: script.js.join("\n"),
                css_code: script.css.join("\n"),
                matches: script.matches.iter().map(|p| p.0.clone()).collect(),
                run_at,
            };
            self.registry.register(entry);
            tracing::info!(
                target: "extensions",
                "Registered content script '{}' ({} js files, {} css files)",
                script.id,
                script.js.len(),
                script.css.len()
            );
        }
        Ok(())
    }

    fn get_registered_content_scripts(
        &self,
        _filter: Option<ScriptFilter>,
    ) -> Result<Vec<RegisteredContentScript>> {
        let all = self.registry.all_scripts();
        let scripts = all
            .into_iter()
            .map(|s| RegisteredContentScript {
                id: s.script_id,
                js: if s.js_code.is_empty() {
                    vec![]
                } else {
                    vec![s.js_code]
                },
                css: if s.css_code.is_empty() {
                    vec![]
                } else {
                    vec![s.css_code]
                },
                matches: s.matches.into_iter().map(UrlPattern).collect(),
                exclude_matches: vec![],
                run_at: match s.run_at {
                    ExtensionRunAt::DocumentIdle => RunAt::DocumentIdle,
                    ExtensionRunAt::DocumentStart => RunAt::DocumentStart,
                    ExtensionRunAt::DocumentEnd => RunAt::DocumentEnd,
                },
                all_frames: false,
                match_about_blank: false,
            })
            .collect();
        Ok(scripts)
    }

    fn unregister_content_scripts(&self, filter: Option<ScriptFilter>) -> Result<()> {
        if let Some(f) = filter
            && let Some(ids) = f.ids
        {
            for id in ids {
                self.registry.unregister_by_id(&id);
            }
        }
        Ok(())
    }
}
