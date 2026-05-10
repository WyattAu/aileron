use crate::extensions::ExtensionId;
use crate::extensions::builtin_adblock_id;

use super::super::AppState;

#[must_use = "ignoring this value may lead to unexpected behavior"]
pub fn cmd_extensions_language(state: &mut AppState, query: &str) -> Option<()> {
    if query == "extensions" {
        let mgr = state.extension_manager.read();
        let ids = mgr.list();
        if ids.is_empty() {
            state.ui.status_message = "No extensions loaded. Use :extension-load to scan, or :extension-install <path> to install.".into();
        } else {
            let lines: Vec<String> = ids
                .iter()
                .map(|id| {
                    mgr.get(id)
                        .map(|api| {
                            format!(
                                "{} v{} [{}]",
                                api.manifest().name,
                                api.manifest().version,
                                api.extension_id()
                            )
                        })
                        .unwrap_or_else(|| id.to_string())
                })
                .collect();
            state.ui.status_message = format!("Extensions ({}): {}", ids.len(), lines.join(" | "));
        }
        return Some(());
    }

    if query == "extension-list" {
        let mgr = state.extension_manager.read();
        let ids = mgr.list();
        if ids.is_empty() {
            state.ui.status_message = "No extensions loaded.".into();
        } else {
            let lines: Vec<String> = ids
                .iter()
                .map(|id| {
                    mgr.get(id)
                        .map(|api| {
                            format!(
                                "{} v{} [{}] enabled",
                                api.manifest().name,
                                api.manifest().version,
                                id.0,
                            )
                        })
                        .unwrap_or_else(|| format!("{} [unknown]", id.0))
                })
                .collect();
            state.ui.status_message = format!("Extensions ({}): {}", ids.len(), lines.join(" | "));
        }
        return Some(());
    }

    if query == "extension-load" {
        let loaded = state.extension_manager.write().load_all();
        state.ui.status_message = format!("Loaded {} extension(s)", loaded.len());
        return Some(());
    }

    if query == "extension-open" {
        let dir = state
            .extension_manager
            .read()
            .extensions_dir()
            .to_path_buf();
        if dir.exists() {
            let dir_str = dir.display().to_string();
            let _ = crate::platform::platform().shell_command(&format!(
                "xdg-open \"{dir_str}\" 2>/dev/null || open \"{dir_str}\" 2>/dev/null || explorer.exe \"{dir_str}\"",
            ));
            state.ui.status_message = format!("Opened {}", dir.display());
        } else {
            state.ui.status_message = "Extensions directory does not exist yet".into();
        }
        return Some(());
    }

    if let Some(id_str) = query.strip_prefix("extension-disable ") {
        let id_str = id_str.trim();
        if id_str.is_empty() {
            state.ui.status_message = "Usage: extension-disable <id>".into();
            return Some(());
        }
        let ext_id = ExtensionId(id_str.to_string());
        match state.extension_manager.write().unload(&ext_id) {
            Some(name) => {
                state.ui.status_message = format!("Disabled extension '{name}' ({id_str})");
            }
            None => {
                state.ui.status_message = format!("Extension '{id_str}' not found");
            }
        }
        return Some(());
    }

    if let Some(id_str) = query.strip_prefix("extension-enable ") {
        let id_str = id_str.trim();
        if id_str.is_empty() {
            state.ui.status_message = "Usage: extension-enable <id>".into();
            return Some(());
        }
        let ext_id = ExtensionId(id_str.to_string());
        if state.extension_manager.read().get(&ext_id).is_some() {
            state.ui.status_message = format!("Extension '{id_str}' is already enabled");
            return Some(());
        }
        let enabled = {
            let mut mgr = state.extension_manager.write();
            if ext_id == builtin_adblock_id() {
                mgr.register_builtin_adblock();
            } else {
                mgr.load_all();
            }
            mgr.get(&ext_id).is_some()
        };
        if enabled {
            state.ui.status_message = format!("Enabled extension '{id_str}'");
        } else {
            state.ui.status_message = format!("Failed to enable extension '{id_str}'");
        }
        return Some(());
    }

    if let Some(path_str) = query.strip_prefix("extension-install ") {
        let path_str = path_str.trim();
        if path_str.is_empty() {
            state.ui.status_message = "Usage: extension-install <path-to-extension-dir>".into();
            return Some(());
        }
        let path = std::path::PathBuf::from(path_str);
        let manifest_path = if path.is_dir() {
            path.join("manifest.json")
        } else if path.ends_with("manifest.json") {
            path
        } else {
            state.ui.status_message = "Path must be a directory containing manifest.json".into();
            return Some(());
        };

        if !manifest_path.exists() {
            state.ui.status_message =
                format!("No manifest.json found at {}", manifest_path.display());
            return Some(());
        }

        match state
            .extension_manager
            .write()
            .load_extension_from_path(&manifest_path)
            .ok()
        {
            Some(id) => {
                state.ui.status_message =
                    format!("Installed extension '{}' from {}", id.0, path_str);
            }
            None => {
                state.ui.status_message = format!("Failed to load extension from {path_str}");
            }
        }
        return Some(());
    }

    if let Some(id_str) = query.strip_prefix("extension-info ") {
        let id_str = id_str.trim();
        if id_str.is_empty() {
            state.ui.status_message = "Usage: extension-info <id>".into();
            return Some(());
        }
        let ext_id = ExtensionId(id_str.to_string());
        match state.extension_manager.read().get(&ext_id).map(|api| {
            (
                api.manifest().name.clone(),
                api.manifest().version.clone(),
                api.extension_id().0.clone(),
                api.manifest().permissions.clone(),
            )
        }) {
            Some((name, version, id, permissions)) => {
                let perms = if permissions.is_empty() {
                    String::new()
                } else {
                    format!(" | perms: {}", permissions.join(", "))
                };
                state.ui.status_message = format!("{name} v{version} ({id}){perms}",);
            }
            None => {
                state.ui.status_message = format!("Extension '{id_str}' not found");
            }
        }
        return Some(());
    }

    if query == "language-list" {
        let locales = crate::i18n::available_locales();
        let current = crate::i18n::detect_locale();
        let items: Vec<String> = locales
            .iter()
            .map(|(locale, name)| {
                if *locale == current {
                    format!("{name}*")
                } else {
                    name.to_string()
                }
            })
            .collect();
        state.ui.status_message = format!("Languages: {}", items.join(", "));
        return Some(());
    }

    if let Some(code) = query.strip_prefix("language ") {
        let code = code.trim();
        if code.is_empty() {
            let current = crate::i18n::detect_locale();
            let locales = crate::i18n::available_locales();
            let name = locales
                .iter()
                .find(|(l, _)| *l == current)
                .map(|(_, n)| *n)
                .unwrap_or("?");
            state.ui.status_message = format!("Language: {} ({})", name, current.code());
        } else if let Some(locale) = crate::i18n::Locale::from_code(code) {
            crate::i18n::set_locale(locale);
            state.config.language = Some(code.to_string());
            let locales = crate::i18n::available_locales();
            let name = locales
                .iter()
                .find(|(l, _)| *l == locale)
                .map(|(_, n)| *n)
                .unwrap_or("?");
            state.ui.status_message = format!("Language: {name}");
        } else {
            let available: Vec<&str> = crate::i18n::available_locales()
                .iter()
                .map(|(l, _)| l.code())
                .collect();
            state.ui.status_message = format!(
                "Unknown language: {}. Available: {}",
                code,
                available.join(", ")
            );
        }
        return Some(());
    }

    None
}
