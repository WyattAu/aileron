use crate::config::Config;
use crate::downloads::DownloadProgress;
use open::that as open_that;

use super::super::AppState;

#[must_use = "ignoring this value may lead to unexpected behavior"]
pub fn cmd_downloads(state: &mut AppState, query: &str) -> Option<()> {
    if query == "downloads" {
        let progress = state.download_manager.progress_all();
        if progress.is_empty() {
            if let Some(db) = state.db.as_ref() {
                match crate::db::downloads::recent_downloads(db, 10) {
                    Ok(entries) => {
                        if entries.is_empty() {
                            state.ui.status_message = "No downloads".into();
                        } else {
                            let items: Vec<String> = entries
                                .iter()
                                .map(|e| format!("{} [{}]", e.filename, e.status))
                                .collect();
                            state.ui.status_message = format!("Downloads: {}", items.join(", "));
                        }
                    }
                    Err(e) => state.ui.status_message = format!("Error: {e}"),
                }
            }
        } else {
            let items: Vec<String> = progress
                .iter()
                .map(|p| {
                    let size_str = if p.total_bytes > 0 {
                        format!(
                            "{}/{}",
                            DownloadProgress::format_bytes(p.received_bytes),
                            DownloadProgress::format_bytes(p.total_bytes)
                        )
                    } else {
                        DownloadProgress::format_bytes(p.received_bytes)
                    };
                    format!("{} [{} {}]", p.filename, p.state, size_str)
                })
                .collect();
            let active = state.download_manager.active_count();
            state.ui.status_message =
                format!("Downloads ({} active): {}", active, items.join(" | "));
        }
        return Some(());
    }
    if query == "downloads-clear" {
        if let Some(db) = state.db.as_ref() {
            match crate::db::downloads::clear_downloads(db) {
                Ok(count) => state.ui.status_message = format!("Cleared {count} downloads"),
                Err(e) => state.ui.status_message = format!("Error: {e}"),
            }
        }
        return Some(());
    }
    if let Some(id_str) = query.strip_prefix("downloads-open ") {
        let id_str = id_str.trim();
        if id_str.is_empty() {
            if let Some(db) = state.db.as_ref() {
                match crate::db::downloads::get_latest_download_id(db) {
                    Ok(id) => match crate::db::downloads::get_download_dest_path(db, id) {
                        Ok(dest) => {
                            let _ = open_that(&dest);
                            state.ui.status_message = format!("Opened: {dest}");
                        }
                        Err(e) => state.ui.status_message = format!("Error: {e}"),
                    },
                    Err(e) => state.ui.status_message = format!("No downloads: {e}"),
                }
            }
        } else if let Ok(id) = id_str.parse::<i64>() {
            if let Some(db) = state.db.as_ref() {
                match crate::db::downloads::get_download_dest_path(db, id) {
                    Ok(dest) => {
                        let _ = open_that(&dest);
                        state.ui.status_message = format!("Opened: {dest}");
                    }
                    Err(e) => state.ui.status_message = format!("Error: {e}"),
                }
            } else {
                state.ui.status_message = "No database".into();
            }
        } else {
            state.ui.status_message = "Usage: downloads-open [id]".into();
        }
        return Some(());
    }
    if query == "downloads-dir" {
        if let Some(downloads_dir) =
            directories::UserDirs::new().and_then(|d| d.download_dir().map(|p| p.to_path_buf()))
        {
            let _ = open_that(&downloads_dir);
            state.ui.status_message = format!("Opened: {}", downloads_dir.display());
        } else {
            state.ui.status_message = "Could not determine downloads directory".into();
        }
        return Some(());
    }

    if let Some(args) = query.strip_prefix("bind ") {
        let args = args.trim();
        if args.is_empty() {
            if state.config.keybindings.is_empty() {
                state.ui.status_message =
                    "No custom keybindings. Usage: :bind normal j ScrollDown".into();
            } else {
                let mut lines: Vec<String> = Vec::new();
                for (key, action) in &state.config.keybindings {
                    lines.push(format!("  {key} → {action}"));
                }
                state.ui.status_message = format!("Custom bindings:\n{}", lines.join("\n"));
            }
        } else {
            let parts: Vec<&str> = args.splitn(3, ' ').collect();
            if parts.len() < 2 {
                state.ui.status_message = "Usage: :bind <mode> <key> [action]".into();
            } else {
                let mode_str = parts[0];
                let key_str = parts[1];
                if crate::input::keybindings::KeybindingRegistry::parse_mode(mode_str).is_none() {
                    state.ui.status_message =
                        format!("Unknown mode: {mode_str}. Use: normal, insert, command");
                } else {
                    let binding_key = format!("{mode_str} {key_str}");
                    if parts.len() >= 3 {
                        let action_str = parts[2];
                        state
                            .config
                            .keybindings
                            .insert(binding_key.clone(), action_str.to_string());
                        state.ui.status_message = format!("Bound: {binding_key} → {action_str}");
                    } else {
                        let current = state
                            .config
                            .keybindings
                            .get(&binding_key)
                            .map(|s| s.as_str())
                            .unwrap_or("(default)");
                        state.ui.status_message = format!("{binding_key} → {current}");
                    }
                    if let Err(e) = Config::save(&state.config) {
                        tracing::warn!("Failed to save config: {}", e);
                    }
                }
            }
        }
        return Some(());
    }

    if let Some(args) = query.strip_prefix("unbind ") {
        let parts: Vec<&str> = args.trim().splitn(2, ' ').collect();
        if parts.len() < 2 {
            state.ui.status_message = "Usage: :unbind <mode> <key>".into();
        } else {
            let binding_key = format!("{} {}", parts[0], parts[1]);
            if state.config.keybindings.remove(&binding_key).is_some() {
                state.ui.status_message = format!("Unbound: {binding_key}");
            } else {
                state.ui.status_message = format!("No custom binding: {binding_key}");
            }
            if let Err(e) = Config::save(&state.config) {
                tracing::warn!("Failed to save config: {}", e);
            }
        }
        return Some(());
    }

    if query == "stats" {
        let tab_count = state.wm.pane_ids().len();
        let term_count = state.terminal_pane_count();
        let ext_count = state.extension_manager.read().count();

        let mut stats = format!("Tabs: {tab_count} ({term_count} terminal)");
        stats.push_str(&format!(" | Extensions: {ext_count}"));

        #[cfg(target_os = "linux")]
        {
            if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
                for line in status.lines() {
                    if let Some(vms) = line.strip_prefix("VmRSS:") {
                        let kb: u64 = vms.trim().parse().unwrap_or(0);
                        stats.push_str(&format!(" | Memory: {} MB", kb / 1024));
                        break;
                    }
                }
            }
        }

        let db_info = state
            .db
            .as_ref()
            .map(|conn| {
                let bm: usize = crate::db::bookmarks::all_bookmarks(conn)
                    .unwrap_or_default()
                    .len();
                let hist: usize = crate::db::history::recent_entries(conn, 0)
                    .unwrap_or_default()
                    .len();
                format!(" | Bookmarks: {bm} | History: {hist}")
            })
            .unwrap_or_default();
        stats.push_str(&db_info);

        state.ui.status_message = stats;
        return Some(());
    }

    None
}
