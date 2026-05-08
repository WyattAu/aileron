use crate::app::WryAction;
use crate::passwords::BitwardenClient;
use crate::ui::search::SearchCategory;
use crate::ui::search::SearchItem;
use tracing::{info, warn};

use super::super::AppState;

pub fn cmd_bitwarden(state: &mut AppState, query: &str) -> Option<()> {
    if let Some(rest) = query.strip_prefix("bw-unlock ") {
        let password = rest.trim();
        if password.is_empty() {
            state.status_message = "Usage: bw-unlock <password>".into();
            return Some(());
        }
        match state.bitwarden.unlock(password) {
            Ok(_) => {
                state.status_message = "Vault unlocked".into();
                info!("Bitwarden vault unlocked");
            }
            Err(e) => {
                state.status_message = format!("Unlock failed: {e}");
                warn!("Bitwarden unlock failed: {}", e);
            }
        }
        return Some(());
    }

    if let Some(search_query) = query.strip_prefix("bw-search ") {
        let search_query = search_query.trim();
        if search_query.is_empty() {
            state.status_message = "Usage: bw-search <query>".into();
            return Some(());
        }
        if !state.bitwarden.is_unlocked() {
            state.status_message = "Vault is locked. Use bw-unlock <password> first.".into();
            return Some(());
        }
        match state.bitwarden.search(search_query) {
            Ok(items) => {
                if items.is_empty() {
                    state.status_message = format!("No vault items matching '{search_query}'");
                } else {
                    let credential_items: Vec<SearchItem> = items
                        .iter()
                        .map(|item| SearchItem {
                            id: format!("credential:{}", item.id),
                            label: item.name.clone(),
                            description: item.url.clone().unwrap_or_else(|| item.id.clone()),
                            category: SearchCategory::Credential,
                        })
                        .collect();
                    state.palette.add_items(credential_items);
                    state.status_message = format!(
                        "Found {} vault items for '{}'. Open palette to select.",
                        items.len(),
                        search_query
                    );
                    state.palette.open();
                    state.command_palette_input.clear();
                    state.palette.update_query("");
                }
            }
            Err(e) => {
                state.status_message = format!("Vault search failed: {e}");
                warn!("Bitwarden search failed: {}", e);
            }
        }
        return Some(());
    }

    if query == "bw-lock" {
        state.bitwarden.lock();
        state.status_message = "Vault locked".into();
        state.palette.set_items(
            state
                .palette
                .results()
                .iter()
                .filter(|i| i.category != SearchCategory::Credential)
                .cloned()
                .collect(),
        );
        return Some(());
    }

    if query == "bw-autofill" {
        let active_id = state.wm.active_pane_id();
        if let Some(engine) = state.engines.get(&active_id)
            && let Some(url) = engine.current_url()
        {
            let url_str = url.to_string();
            if !state.bitwarden.is_unlocked() {
                state.status_message = "Vault locked. Use :bw-unlock <password>".into();
            } else {
                match state.bitwarden.search_for_url(&url_str) {
                    Ok(items) if items.len() == 1 => {
                        match state.bitwarden.get_credential(&items[0].id) {
                            Ok(cred) => {
                                let js = state.bitwarden.autofill_js(&cred);
                                state.pending_wry_actions.push_back(WryAction::RunJs(js));
                                state.status_message = format!("Auto-filled: {}", items[0].name);
                            }
                            Err(e) => state.status_message = format!("!{e}"),
                        }
                    }
                    Ok(items) if items.is_empty() => {
                        state.status_message = "No credentials found for this site".into();
                    }
                    Ok(items) => {
                        state.status_message = format!(
                            "Multiple matches ({}). Use :bw-search <query> to pick.",
                            items.len()
                        );
                    }
                    Err(e) => state.status_message = format!("!{e}"),
                }
            }
        }
        return Some(());
    }

    if query == "bw-detect" {
        state.pending_wry_actions.push_back(WryAction::RunJs(
            BitwardenClient::detect_login_forms_js().into(),
        ));
        state.status_message = "Detecting login forms...".into();
        return Some(());
    }

    if query == "keyring-test" {
        if crate::passwords::keyring::is_available() {
            state.status_message = "System keyring: available".into();
        } else {
            state.status_message = "System keyring: not available".into();
        }
        return Some(());
    }

    if query == "credentials-save" {
        state.pending_wry_actions.push_back(WryAction::RunJs(
            r#"
                (function() {
                    var data = window.__aileron_credential_save;
                    window.__aileron_credential_save = null;
                    if (data && data.username && data.password) {
                        JSON.stringify({type: 'credential_save', username: data.username, password: data.password, url: data.url});
                    } else {
                        JSON.stringify({type: 'credential_save', status: 'none'});
                    }
                })();
                "#.into(),
        ));
        state.status_message = "Checking for credentials to save...".into();
        return Some(());
    }

    None
}
