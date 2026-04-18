use tracing::info;
use tracing::warn;

use crate::db::bookmarks;
use crate::ui::search::SearchCategory;
use crate::ui::search::SearchItem;

use super::AppState;

impl AppState {
    /// Refresh the command palette with latest history items from the DB
    /// and open pane URLs from the engine manager.
    pub fn refresh_palette_items(&mut self) {
        self.palette.clear_items();
        if let Some(ref conn) = self.db {
            if let Ok(entries) = crate::db::history::recent_entries(conn, 50) {
                for entry in entries {
                    self.palette.add_item(SearchItem {
                        id: format!("history:{}", entry.id),
                        label: entry.title.clone(),
                        description: entry.url.clone(),
                        category: SearchCategory::History,
                    });
                }
            }
            if let Ok(bm_list) = bookmarks::all_bookmarks(conn) {
                for bm in bm_list {
                    self.palette.add_item(SearchItem {
                        id: format!("bookmark:{}", bm.id),
                        label: bm.title.clone(),
                        description: bm.url.clone(),
                        category: SearchCategory::Bookmark,
                    });
                }
            }
        }

        // Add open pane URLs as switchable tabs
        let panes = self.wm.panes();
        for (pane_id, _rect) in &panes {
            let url_str = self
                .engines
                .get(pane_id)
                .and_then(|e| e.current_url().cloned())
                .map(|u| u.to_string())
                .unwrap_or_else(|| "aileron://new".into());
            let is_active = *pane_id == self.wm.active_pane_id();
            let label = if is_active {
                format!("● {}", url_str)
            } else {
                url_str.clone()
            };
            self.palette.add_item(SearchItem {
                id: format!("tab:{}", pane_id),
                label,
                description: url_str,
                category: SearchCategory::OpenTab,
            });
        }

        // Re-add custom Lua commands
        if let Some(ref engine) = self.lua_engine {
            for cmd in engine.custom_commands() {
                self.palette.add_item(SearchItem {
                    id: format!("custom:{}", cmd.name),
                    label: cmd.name.clone(),
                    description: cmd.description,
                    category: SearchCategory::Custom,
                });
            }
        }
    }

    pub fn execute_palette_selection(&mut self, item: &crate::ui::SearchItem) {
        match item.category {
            SearchCategory::History => {
                // Navigate to the selected history URL
                if let Ok(url) = url::Url::parse(&item.description) {
                    let active_id = self.wm.active_pane_id();
                    if let Some(engine) = self.engines.get_mut(&active_id) {
                        engine.navigate(&url);
                        self.status_message = format!("Navigating to {}", item.label);
                    }
                }
            }
            SearchCategory::Command => {
                self.execute_command(&item.id);
            }
            SearchCategory::Bookmark => {
                if let Ok(url) = url::Url::parse(&item.description) {
                    let active_id = self.wm.active_pane_id();
                    if let Some(engine) = self.engines.get_mut(&active_id) {
                        engine.navigate(&url);
                        self.status_message = format!("Opening bookmark: {}", item.label);
                    }
                }
            }
            SearchCategory::Credential => {
                // Extract item ID from "credential:<id>"
                if let Some(item_id) = item.id.strip_prefix("credential:") {
                    if !self.bitwarden.is_unlocked() {
                        self.status_message =
                            "Vault is locked. Use bw-unlock <password> first.".into();
                        return;
                    }
                    match self.bitwarden.get_credential(item_id) {
                        Ok(credential) => {
                            let js = self.bitwarden.autofill_js(&credential);
                            info!("Auto-filling credential for: {}", credential.name);
                            self.pending_wry_actions
                                .push_back(crate::app::WryAction::Autofill { js });
                            self.status_message = format!("Auto-filled: {}", credential.name);
                        }
                        Err(e) => {
                            self.status_message = format!("Failed to get credential: {}", e);
                            warn!("Bitwarden get_credential failed: {}", e);
                        }
                    }
                }
            }
            SearchCategory::Custom => {
                // Extract command name from "custom:<name>"
                if let Some(name) = item.id.strip_prefix("custom:") {
                    match self.call_lua_command(name) {
                        Ok(result) => {
                            info!("Lua command '{}' executed: {}", name, result);
                            self.status_message = format!("✓ {}", name);
                        }
                        Err(e) => {
                            self.status_message = format!("Command '{}' failed: {}", name, e);
                            warn!("Lua command '{}' failed: {}", name, e);
                        }
                    }
                }
            }
            SearchCategory::OpenTab => {
                if let Some(pane_id_str) = item.id.strip_prefix("tab:")
                    && let Ok(pane_id) = uuid::Uuid::parse_str(pane_id_str)
                        && self.wm.get_rect(pane_id).is_some() {
                            let old_active = self.wm.active_pane_id();
                            if old_active != pane_id {
                                self.last_active_pane_id = Some(old_active);
                            }
                            self.wm.set_active_pane(pane_id);
                            let url = self
                                .engines
                                .get(&pane_id)
                                .and_then(|e| e.current_url().cloned())
                                .map(|u| u.to_string())
                                .unwrap_or_default();
                            let display = if url.len() > 50 {
                                format!("{}...", &url[..47])
                            } else {
                                url
                            };
                            self.status_message = format!("Switched to: {}", display);
                        }
            }
            _ => {
                self.status_message = format!("Selected: {}", item.label);
            }
        }
    }
}
