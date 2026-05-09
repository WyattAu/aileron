use super::super::AppState;

pub fn cmd_site_settings(state: &mut AppState, query: &str) -> Option<()> {
    if query == "site-settings" {
        let active_id = state.wm.active_pane_id();
        if let Some(engine) = state.engines.get(&active_id)
            && let Some(url) = engine.current_url()
        {
            if let Some(db) = state.db.as_ref() {
                match crate::db::site_settings::get_site_settings_for_url(db, url.as_str()) {
                    Ok(settings) => {
                        if settings.is_empty() {
                            state.ui.status_message = "No per-site settings for current URL".into();
                        } else {
                            let items: Vec<String> = settings
                                .iter()
                                .map(|s| {
                                    let mut parts =
                                        vec![format!("{}[{}]", s.pattern, s.pattern_type)];
                                    if let Some(z) = s.zoom_level {
                                        parts.push(format!("zoom={z}"));
                                    }
                                    if let Some(b) = s.adblock_enabled {
                                        parts.push(format!(
                                            "adblock={}",
                                            if b { "on" } else { "off" }
                                        ));
                                    }
                                    if let Some(b) = s.javascript_enabled {
                                        parts.push(format!("js={}", if b { "on" } else { "off" }));
                                    }
                                    if let Some(b) = s.cookies_enabled {
                                        parts.push(format!(
                                            "cookies={}",
                                            if b { "on" } else { "off" }
                                        ));
                                    }
                                    if let Some(b) = s.autoplay_enabled {
                                        parts.push(format!(
                                            "autoplay={}",
                                            if b { "on" } else { "off" }
                                        ));
                                    }
                                    parts.join(" ")
                                })
                                .collect();
                            state.ui.status_message =
                                format!("Site settings: {}", items.join(" | "));
                        }
                    }
                    Err(e) => state.ui.status_message = format!("Error: {e}"),
                }
            }
        } else {
            state.ui.status_message = "No active URL".into();
        }
        return Some(());
    }

    if let Some(rest) = query.strip_prefix("site-settings set ") {
        let rest = rest.trim();
        let mut parts = rest.splitn(2, ' ');
        if let (Some(key), Some(value)) = (parts.next(), parts.next()) {
            let value = value.trim();
            let active_id = state.wm.active_pane_id();
            let host = state
                .engines
                .get(&active_id)
                .and_then(|e| e.current_url())
                .and_then(|u| u.host_str())
                .map(|h| h.to_lowercase())
                .unwrap_or_default();

            if host.is_empty() {
                state.ui.status_message = "No active URL for site settings".into();
            } else if let Some(db) = state.db.as_ref() {
                match crate::db::site_settings::set_site_field(db, &host, "exact", key, Some(value))
                {
                    Ok(()) => state.ui.status_message = format!("Set {key}={value} for {host}"),
                    Err(e) => state.ui.status_message = format!("Failed: {e}"),
                }
            }
        } else {
            state.ui.status_message =
                "Usage: :site-settings set <key> <value> (zoom, adblock, js, cookies, autoplay)"
                    .into();
        }
        return Some(());
    }

    if query == "site-settings list" {
        if let Some(db) = state.db.as_ref() {
            match crate::db::site_settings::list_site_settings(db) {
                Ok(settings) => {
                    if settings.is_empty() {
                        state.ui.status_message = "No site settings".into();
                    } else {
                        let items: Vec<String> = settings
                            .iter()
                            .take(10)
                            .map(|s| format!("[{}] {} (id:{})", s.pattern_type, s.pattern, s.id))
                            .collect();
                        let suffix = if settings.len() > 10 {
                            format!(" (+{} more)", settings.len() - 10)
                        } else {
                            String::new()
                        };
                        state.ui.status_message = format!("{}{}", items.join(" | "), suffix);
                    }
                }
                Err(e) => state.ui.status_message = format!("Error: {e}"),
            }
        }
        return Some(());
    }

    if let Some(id_str) = query.strip_prefix("site-settings delete ") {
        let id_str = id_str.trim();
        if let Ok(id) = id_str.parse::<i64>() {
            if let Some(db) = state.db.as_ref() {
                match crate::db::site_settings::delete_site_setting(db, id) {
                    Ok(true) => state.ui.status_message = format!("Deleted site setting {id}"),
                    Ok(false) => state.ui.status_message = format!("No site setting with id {id}"),
                    Err(e) => state.ui.status_message = format!("Failed: {e}"),
                }
            }
        } else {
            state.ui.status_message = "Usage: :site-settings delete <id>".into();
        }
        return Some(());
    }

    if let Some(domain) = query.strip_prefix("site-settings clear ") {
        let domain = domain.trim();
        if domain.is_empty() {
            state.ui.status_message = "Usage: :site-settings clear <domain>".into();
            return Some(());
        }
        if let Some(db) = state.db.as_ref() {
            match crate::db::site_settings::delete_site_settings_for_domain(db, domain) {
                Ok(count) => {
                    state.ui.status_message = format!("Cleared {count} setting(s) for {domain}")
                }
                Err(e) => state.ui.status_message = format!("Failed: {e}"),
            }
        }
        return Some(());
    }

    if query == "cookies" {
        state
            .pending_wry_actions
            .push_back(crate::app::WryAction::RunJs(
                "document.cookie || '(no cookies for this site)'".into(),
            ));
        state.ui.status_message = "Showing cookies...".into();
        return Some(());
    }
    if let Some(domain) = query.strip_prefix("cookies-block ") {
        let domain = domain.trim();
        if domain.is_empty() {
            state.ui.status_message = "Usage: :cookies-block <domain>".into();
            return Some(());
        }
        if let Some(db) = state.db.as_ref() {
            match crate::db::site_settings::set_site_field(
                db,
                domain,
                "exact",
                "cookies",
                Some("off"),
            ) {
                Ok(()) => state.ui.status_message = format!("Cookies blocked for {domain}"),
                Err(e) => state.ui.status_message = format!("Failed: {e}"),
            }
        }
        return Some(());
    }
    if let Some(domain) = query.strip_prefix("cookies-allow ") {
        let domain = domain.trim();
        if domain.is_empty() {
            state.ui.status_message = "Usage: :cookies-allow <domain>".into();
            return Some(());
        }
        if let Some(db) = state.db.as_ref() {
            match crate::db::site_settings::set_site_field(
                db,
                domain,
                "exact",
                "cookies",
                Some("on"),
            ) {
                Ok(()) => state.ui.status_message = format!("Cookies allowed for {domain}"),
                Err(e) => state.ui.status_message = format!("Failed: {e}"),
            }
        }
        return Some(());
    }

    None
}
