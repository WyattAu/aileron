use tracing::{info, warn};

use crate::app::WryAction;
use crate::db::bookmarks;
use crate::extensions::ExtensionId;
use crate::passwords::BitwardenClient;
use crate::ui::search::SearchCategory;
use crate::ui::search::SearchItem;

use super::AppState;

impl AppState {
    /// Queue a navigation to a URL, applying any Lua URL redirect rules.
    pub(crate) fn navigate_with_redirects(&mut self, mut url: url::Url) {
        self.session_dirty = true;
        // Apply URL redirect rules from Lua engine
        if let Some(ref engine) = self.lua_engine {
            url = engine.apply_url_redirects(&url);
        }
        // Update placeholder engine
        let active_id = self.wm.active_pane_id();
        if let Some(engine) = self.engines.get_mut(&active_id) {
            engine.navigate(&url);
        }
        if let Some(ref engine) = self.lua_engine {
            engine.call_hooks("navigate", &[url.as_str()]);
        }
        self.pending_wry_actions.push_back(WryAction::Navigate(url));
    }

    pub(crate) fn execute_command(&mut self, cmd: &str) {
        let cmd = cmd.trim();

        // Command chaining: split on " && " and execute each
        if cmd.contains(" && ") {
            for part in cmd.split(" && ") {
                self.execute_command(part.trim());
            }
            return;
        }

        match cmd {
            "q" | "quit" => self.should_quit = true,
            "vs" => self.execute_action(&crate::input::Action::SplitVertical),
            "sp" => self.execute_action(&crate::input::Action::SplitHorizontal),
            "files" | "browse" => {
                let path = crate::git::repo_root(std::env::current_dir().unwrap_or_default().as_path())
                    .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
                let encoded = crate::servo::wry_engine::percent_encode_path(&path.to_string_lossy());
                if let Ok(url) = url::Url::parse(&format!("aileron://files?path={}", encoded)) {
                    self.navigate_with_redirects(url);
                    self.status_message = format!("File browser: {}", path.display());
                }
            }
            "swap" | "tab-swap" => self.swap_panes(),
            "only" => {
                self.execute_action(&crate::input::Action::CloseOtherPanes);
            }
            "reader" => {
                self.execute_action(&crate::input::Action::ToggleReaderMode);
            }
            "minimal" => {
                self.execute_action(&crate::input::Action::ToggleMinimalMode);
            }
            "settings" => {
                if let Ok(url) = url::Url::parse("aileron://settings") {
                    self.navigate_with_redirects(url);
                    self.status_message = "Settings".into();
                }
            }
            "privacy" => {
                let https = self.config.https_upgrade_enabled;
                let tracking = self.config.tracking_protection_enabled;
                let adblock = self.config.adblock_enabled;
                self.status_message = format!(
                    "HTTPS upgrade: {} | Tracking protection: {} | Adblock: {}",
                    if https { "ON" } else { "OFF" },
                    if tracking { "ON" } else { "OFF" },
                    if adblock { "ON" } else { "OFF" },
                );
            }
            "language-list" => {
                let locales = crate::i18n::available_locales();
                let current = crate::i18n::detect_locale();
                let items: Vec<String> = locales
                    .iter()
                    .map(|(locale, name)| {
                        if *locale == current {
                            format!("{}*", name)
                        } else {
                            name.to_string()
                        }
                    })
                    .collect();
                self.status_message = format!("Languages: {}", items.join(", "));
            }
            "memory" => {
                let rss = crate::profiling::memory::process_rss_human();
                let term_count = self.terminal_pane_ids.len();
                let total_panes = self.wm.panes().len();
                let web_count = total_panes - term_count;
                let estimated = crate::profiling::memory::estimate_pane_memory(web_count, term_count);
                self.status_message = format!(
                    "RSS: {} | WebViews: {}x~50MB | Terminals: {}x~3MB | Est pane: {}",
                    rss, web_count, term_count,
                    crate::profiling::memory::format_human_bytes(estimated)
                );
            }
            "adaptive-quality" | "adaptive_quality" => {
                self.config.adaptive_quality = !self.config.adaptive_quality;
                self.status_message = format!(
                    "Adaptive quality: {}",
                    if self.config.adaptive_quality { "on" } else { "off" }
                );
            }
            "engine" => {
                self.status_message = format!("Engine: {}", self.config.engine_selection);
            }
            "https-toggle" => {
                let active_id = self.wm.active_pane_id();
                if let Some(engine) = self.engines.get(&active_id)
                    && let Some(url) = engine.current_url()
                    && let Some(host) = url.host_str()
                {
                    let host_lower = host.to_lowercase();
                    let safe_list = crate::net::privacy::load_https_safe_list();
                    if crate::net::privacy::is_https_safe(&host_lower, &safe_list) {
                        self.status_message = format!(
                            "HTTPS upgrade: {} is in the safe list",
                            host_lower
                        );
                    } else {
                        self.status_message = format!(
                            "HTTPS upgrade: {} is not in the safe list ({} domains)",
                            host_lower,
                            safe_list.len()
                        );
                    }
                } else {
                    self.status_message = "No active page URL".into();
                }
            }
            "extensions" => {
                let ids = self.extension_manager.list();
                if ids.is_empty() {
                    self.status_message = "No extensions loaded".into();
                } else {
                    let names: Vec<String> = ids
                        .iter()
                        .map(|id| {
                            self.extension_manager
                                .get(id)
                                .map(|api| api.manifest().name.clone())
                                .unwrap_or_else(|| id.to_string())
                        })
                        .collect();
                    self.status_message = format!("Extensions: {}", names.join(", "));
                }
            }
            "extension-load" => {
                let loaded = self.extension_manager.load_all();
                self.status_message = format!("Loaded {} extension(s)", loaded.len());
            }
            "" => {}
            _ => {
                if let Some(code) = cmd.strip_prefix("language ") {
                    let code = code.trim();
                    if code.is_empty() {
                        let current = crate::i18n::detect_locale();
                        let locales = crate::i18n::available_locales();
                        let name = locales
                            .iter()
                            .find(|(l, _)| *l == current)
                            .map(|(_, n)| *n)
                            .unwrap_or("?");
                        self.status_message =
                            format!("Language: {} ({})", name, current.code());
                    } else if let Some(locale) = crate::i18n::Locale::from_code(code) {
                        crate::i18n::set_locale(locale);
                        self.config.language = Some(code.to_string());
                        let locales = crate::i18n::available_locales();
                        let name = locales
                            .iter()
                            .find(|(l, _)| *l == locale)
                            .map(|(_, n)| *n)
                            .unwrap_or("?");
                        self.status_message = format!("Language: {}", name);
                    } else {
                        let available: Vec<&str> = crate::i18n::available_locales()
                            .iter()
                            .map(|(l, _)| l.code())
                            .collect();
                        self.status_message = format!(
                            "Unknown language: {}. Available: {}",
                            code,
                            available.join(", ")
                        );
                    }
                    return;
                }

                if let Some(id_str) = cmd.strip_prefix("extension-info ") {
                    let id_str = id_str.trim();
                    if id_str.is_empty() {
                        self.status_message = "Usage: extension-info <id>".into();
                        return;
                    }
                    let ext_id = ExtensionId(id_str.to_string());
                    match self.extension_manager.get(&ext_id) {
                        Some(api) => {
                            let m = api.manifest();
                            let perms = if m.permissions.is_empty() {
                                String::new()
                            } else {
                                format!(" | perms: {}", m.permissions.join(", "))
                            };
                            self.status_message = format!(
                                "{} v{} ({}){}",
                                m.name,
                                m.version,
                                api.extension_id(),
                                perms,
                            );
                        }
                        None => {
                            self.status_message =
                                format!("Extension '{}' not found", id_str);
                        }
                    }
                    return;
                }

                // Shell command: !<cmd>
                if let Some(cmd) = cmd.strip_prefix("!") {
                    let cmd = cmd.trim();
                    if cmd.is_empty() {
                        self.status_message = "Usage: !<command>".into();
                        return;
                    }
                    match std::process::Command::new("sh").args(["-c", cmd]).output() {
                        Ok(output) => {
                            let stdout =
                                String::from_utf8_lossy(&output.stdout).trim().to_string();
                            let line = stdout.lines().next().unwrap_or("");
                            if line.len() > 80 {
                                self.status_message = format!("{}...", &line[..77]);
                            } else if line.is_empty() {
                                self.status_message = format!("(exit {})", output.status);
                            } else {
                                self.status_message = line.to_string();
                            }
                        }
                        Err(e) => {
                            self.status_message = format!("!{}: {}", cmd, e);
                        }
                    }
                    return;
                }

                // Runtime config: set <key> <value>
                if let Some(rest) = cmd.strip_prefix("set ") {
                    let rest = rest.trim();
                    let mut parts = rest.splitn(2, ' ');
                    if let Some(key) = parts.next() {
                        let value = parts.next().unwrap_or("");
                        match key {
                            "search_engine" if !value.is_empty() => {
                                self.config.search_engine = value.to_string();
                                self.status_message = format!("search_engine = {}", value);
                            }
                            "homepage" if !value.is_empty() => {
                                self.config.homepage = value.to_string();
                                self.status_message = format!("homepage = {}", value);
                            }
                            "adblock" => {
                                self.config.adblock_enabled = !value.contains("off")
                                    && !value.contains("false")
                                    && !value.contains("0");
                                self.status_message = format!(
                                    "adblock = {}",
                                    self.config.adblock_enabled
                                );
                            }
                            "https_upgrade" | "https-upgrade" => {
                                self.config.https_upgrade_enabled = !value.contains("off")
                                    && !value.contains("false")
                                    && !value.contains("0");
                                self.status_message = format!(
                                    "https_upgrade = {}",
                                    self.config.https_upgrade_enabled
                                );
                            }
                    "tracking_protection" | "tracking-protection" => {
                        self.config.tracking_protection_enabled = !value.contains("off")
                            && !value.contains("false")
                            && !value.contains("0");
                        self.status_message = format!(
                            "tracking_protection = {}",
                            self.config.tracking_protection_enabled
                        );
                    }
                    "popup_blocker" | "popup-blocker" | "popups" => {
                        self.config.popup_blocker_enabled = !value.contains("off")
                            && !value.contains("false")
                            && !value.contains("0");
                        self.status_message = format!(
                            "popup_blocker = {}",
                            self.config.popup_blocker_enabled
                        );
                    }
                    _ => {
                        self.status_message = format!(
                            "Unknown setting: {} (try: search_engine, homepage, adblock, https_upgrade, tracking_protection, popup_blocker)",
                            key
                        );
                    }
                }
            }
            return;
        }

        // Explicit navigate: open <url>
        if let Some(url_str) = cmd.strip_prefix("open ") {
                    let url_str = url_str.trim();
                    if url_str.is_empty() {
                        self.status_message = "Usage: open <url>".into();
                        return;
                    }
                    let url = if url_str.contains("://") {
                        url::Url::parse(url_str)
                    } else {
                        url::Url::parse(&format!("https://{}", url_str))
                    };
                    match url {
                        Ok(u) => {
                            self.navigate_with_redirects(u);
                            self.status_message = format!("Opening: {}", url_str);
                        }
                        Err(e) => {
                            self.status_message = format!("Invalid URL: {}", e);
                        }
                    }
                    return;
                }

                // Check for ssh <host> command
                if let Some(host) = cmd.strip_prefix("ssh ") {
                    let host = host.trim();
                    if host.is_empty() {
                        self.status_message = "Usage: ssh <host>".into();
                        return;
                    }
                    self.pending_terminal_command = Some(format!("ssh {}\n", host));
                    self.execute_action(&crate::input::Action::OpenTerminal);
                    return;
                }

                // Try to navigate if it looks like a URL
                if Self::looks_like_url(cmd)
                    && let Ok(url) = url::Url::parse(cmd) {
                        self.navigate_with_redirects(url);
                        self.status_message = format!("Navigating to {}", cmd);
                        return;
                    }
                // Treat as search query
                if let Some(url) = self.config.search_url(cmd) {
                    self.navigate_with_redirects(url);
                    self.status_message = format!("Searching: {}", cmd);
                } else {
                    self.status_message = format!("Unknown command: {}", cmd);
                }
            }
        }
    }

    /// Handle a raw query submitted from the command palette (no matching results).
    /// Checks if it's a URL, a known command, or shows an error.
    pub(crate) fn handle_raw_command(&mut self, query: &str) {
        // Command chaining: split on " && " and execute each
        if query.contains(" && ") {
            for part in query.split(" && ") {
                self.handle_raw_command(part.trim());
            }
            return;
        }

        // Check for known commands first
        match query {
            "q" | "quit" => {
                self.should_quit = true;
                return;
            }
            "vs" => {
                self.execute_action(&crate::input::Action::SplitVertical);
                return;
            }
            "sp" => {
                self.execute_action(&crate::input::Action::SplitHorizontal);
                return;
            }
            "files" | "browse" => {
                let path = crate::git::repo_root(std::env::current_dir().unwrap_or_default().as_path())
                    .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
                let encoded = crate::servo::wry_engine::percent_encode_path(&path.to_string_lossy());
                if let Ok(url) = url::Url::parse(&format!("aileron://files?path={}", encoded)) {
                    self.navigate_with_redirects(url);
                    self.status_message = format!("File browser: {}", path.display());
                }
                return;
            }
            _ => {}
        }

        // Layout save/load aliases for workspace save/load
        if let Some(name) = query.strip_prefix("layout-save ") {
            let name = name.trim();
            if name.is_empty() {
                self.status_message = "Usage: :layout-save <name>".into();
                return;
            }
            self.pending_wry_actions
                .push_back(WryAction::SaveWorkspace {
                    name: name.to_string(),
                    pane_urls: std::collections::HashMap::new(),
                });
            self.status_message = format!("Saving layout: {}...", name);
            return;
        }

        if let Some(name) = query.strip_prefix("layout-load ") {
            let name = name.trim();
            if name.is_empty() {
                self.status_message = "Usage: :layout-load <name>".into();
                return;
            }
            self.pending_workspace_restore = Some(name.to_string());
            self.status_message = format!("Loading layout: {}...", name);
            return;
        }

        // Check for bw- commands (password manager)
        if let Some(rest) = query.strip_prefix("bw-unlock ") {
            let password = rest.trim();
            if password.is_empty() {
                self.status_message = "Usage: bw-unlock <password>".into();
                return;
            }
            match self.bitwarden.unlock(password) {
                Ok(_) => {
                    self.status_message = "Vault unlocked".into();
                    info!("Bitwarden vault unlocked");
                }
                Err(e) => {
                    self.status_message = format!("Unlock failed: {}", e);
                    warn!("Bitwarden unlock failed: {}", e);
                }
            }
            return;
        }

        if let Some(rest) = query.strip_prefix("bw-search ") {
            let search_query = rest.trim();
            if search_query.is_empty() {
                self.status_message = "Usage: bw-search <query>".into();
                return;
            }
            if !self.bitwarden.is_unlocked() {
                self.status_message = "Vault is locked. Use bw-unlock <password> first.".into();
                return;
            }
            match self.bitwarden.search(search_query) {
                Ok(items) => {
                    if items.is_empty() {
                        self.status_message = format!("No vault items matching '{}'", search_query);
                    } else {
                        // Add search results to palette as Credential items
                        let credential_items: Vec<SearchItem> = items
                            .iter()
                            .map(|item| SearchItem {
                                id: format!("credential:{}", item.id),
                                label: item.name.clone(),
                                description: item.url.clone().unwrap_or_else(|| item.id.clone()),
                                category: SearchCategory::Credential,
                            })
                            .collect();
                        self.palette.add_items(credential_items);
                        self.status_message = format!(
                            "Found {} vault items for '{}'. Open palette to select.",
                            items.len(),
                            search_query
                        );
                        // Auto-open palette to show results
                        self.palette.open();
                        self.command_palette_open = true;
                        self.command_palette_input.clear();
                        // Re-search within palette to show the credential items
                        self.palette.update_query("");
                    }
                }
                Err(e) => {
                    self.status_message = format!("Vault search failed: {}", e);
                    warn!("Bitwarden search failed: {}", e);
                }
            }
            return;
        }

        if query == "bw-lock" {
            self.bitwarden.lock();
            self.status_message = "Vault locked".into();
            self.palette.set_items(
                self.palette
                    .results()
                    .iter()
                    .filter(|i| i.category != SearchCategory::Credential)
                    .cloned()
                    .collect(),
            );
            return;
        }

        if query == "bw-autofill" {
            let active_id = self.wm.active_pane_id();
            if let Some(engine) = self.engines.get(&active_id)
                && let Some(url) = engine.current_url()
            {
                let url_str = url.to_string();
                if !self.bitwarden.is_unlocked() {
                    self.status_message = "Vault locked. Use :bw-unlock <password>".into();
                } else {
                    match self.bitwarden.search_for_url(&url_str) {
                        Ok(items) if items.len() == 1 => {
                            match self.bitwarden.get_credential(&items[0].id) {
                                Ok(cred) => {
                                    let js = self.bitwarden.autofill_js(&cred);
                                    self.pending_wry_actions.push_back(WryAction::RunJs(js));
                                    self.status_message = format!("Auto-filled: {}", items[0].name);
                                }
                                Err(e) => self.status_message = format!("!{}", e),
                            }
                        }
                        Ok(items) if items.is_empty() => {
                            self.status_message = "No credentials found for this site".into();
                        }
                        Ok(items) => {
                            self.status_message = format!(
                                "Multiple matches ({}). Use :bw-search <query> to pick.",
                                items.len()
                            );
                        }
                        Err(e) => self.status_message = format!("!{}", e),
                    }
                }
            }
            return;
        }

        if query == "bw-detect" {
            self.pending_wry_actions.push_back(WryAction::RunJs(
                BitwardenClient::detect_login_forms_js().into(),
            ));
            self.status_message = "Detecting login forms...".into();
            return;
        }

        if query == "keyring-test" {
            if crate::passwords::keyring::is_available() {
                self.status_message = "System keyring: available".into();
            } else {
                self.status_message = "System keyring: not available".into();
            }
            return;
        }

        if let Some(path) = query.strip_prefix("pdf ") {
            let path = path.trim();
            if path.is_empty() {
                self.status_message = "Usage: :pdf <path-or-url>".into();
                return;
            }
            std::process::Command::new("xdg-open")
                .arg(path)
                .spawn()
                .map_err(|e| self.status_message = format!("!{}", e))
                .ok();
            self.status_message = format!("Opening PDF: {}", path);
            return;
        }

        // SSH convenience command: open a terminal and auto-type ssh <host>
        if let Some(host) = query.strip_prefix("ssh ") {
            let host = host.trim();
            if host.is_empty() {
                self.status_message = "Usage: ssh <host>".into();
                return;
            }
            self.pending_terminal_command = Some(format!("ssh {}\n", host));
            self.execute_action(&crate::input::Action::OpenTerminal);
            return;
        }

        // Workspace commands: ws-save <name>, ws-list, ws-load <name>
        if let Some(name) = query.strip_prefix("ws-save ") {
            let name = name.trim();
            if name.is_empty() {
                self.status_message = "Usage: ws-save <name>".into();
                return;
            }
            // Queue save for main.rs (which has WryPaneManager access)
            self.pending_wry_actions
                .push_back(WryAction::SaveWorkspace {
                    name: name.to_string(),
                    pane_urls: std::collections::HashMap::new(),
                });
            self.status_message = format!("Saving workspace: {}...", name);
            return;
        }

        if query == "ws-list" {
            let workspaces = self.list_workspaces();
            if workspaces.is_empty() {
                self.status_message = "No saved workspaces.".into();
            } else {
                let names: Vec<&str> = workspaces
                    .iter()
                    .filter(|w| w.name != "_autosave")
                    .map(|w| w.name.as_str())
                    .collect();
                self.status_message = format!("Workspaces: {}", names.join(", "));
            }
            return;
        }

        if let Some(name) = query.strip_prefix("ws-load ") {
            let name = name.trim();
            if name.is_empty() {
                self.status_message = "Usage: ws-load <name>".into();
                return;
            }
            // Workspace restore requires main.rs to rebuild wry panes.
            // Store the requested workspace name for main.rs to pick up.
            self.pending_workspace_restore = Some(name.to_string());
            self.status_message = format!("Restoring workspace: {}...", name);
            return;
        }

        // Swap active pane with previously active pane
        if query == "swap" || query == "tab-swap" {
            self.swap_panes();
            return;
        }

        if query == "pin" {
            self.execute_action(&crate::input::Action::PinPane);
            return;
        }

        if query == "scripts" || query == "content-scripts" {
            self.pending_wry_actions
                .push_back(WryAction::ListContentScripts);
            return;
        }

        if query == "network" || query == "netlog" {
            self.pending_wry_actions.push_back(WryAction::GetNetworkLog);
            return;
        }
        if query == "network-clear" || query == "netlog-clear" {
            self.pending_wry_actions.push_back(WryAction::ClearNetworkLog);
            return;
        }
        if query == "console" || query == "consolelog" {
            self.pending_wry_actions.push_back(WryAction::GetConsoleLog);
            return;
        }
        if query == "console-clear" {
            self.pending_wry_actions.push_back(WryAction::ClearConsoleLog);
            return;
        }

        if query == "downloads" {
            if let Some(db) = self.db.as_ref() {
                match crate::db::downloads::recent_downloads(db, 10) {
                    Ok(entries) => {
                        if entries.is_empty() {
                            self.status_message = "No downloads".into();
                        } else {
                            let items: Vec<String> = entries.iter().map(|e| {
                                format!("{} [{}%]", e.filename, e.progress_percent)
                            }).collect();
                            self.status_message = format!("Downloads: {}", items.join(", "));
                        }
                    }
                    Err(e) => self.status_message = format!("Error: {}", e),
                }
            }
            return;
        }
        if query == "downloads-clear" {
            if let Some(db) = self.db.as_ref() {
                match crate::db::downloads::clear_downloads(db) {
                    Ok(count) => self.status_message = format!("Cleared {} downloads", count),
                    Err(e) => self.status_message = format!("Error: {}", e),
                }
            }
            return;
        }
        if let Some(id_str) = query.strip_prefix("downloads-open ") {
            let id_str = id_str.trim();
            if id_str.is_empty() {
                if let Some(db) = self.db.as_ref() {
                    match crate::db::downloads::get_latest_download_id(db) {
                        Ok(id) => {
                            match crate::db::downloads::get_download_dest_path(db, id) {
                                Ok(dest) => {
                                    let _ = std::process::Command::new("xdg-open")
                                        .arg(&dest)
                                        .stdout(std::process::Stdio::null())
                                        .stderr(std::process::Stdio::null())
                                        .spawn();
                                    self.status_message = format!("Opened: {}", dest);
                                }
                                Err(e) => self.status_message = format!("Error: {}", e),
                            }
                        }
                        Err(e) => self.status_message = format!("No downloads: {}", e),
                    }
                }
            } else if let Ok(id) = id_str.parse::<i64>() {
                if let Some(db) = self.db.as_ref() {
                    match crate::db::downloads::get_download_dest_path(db, id) {
                        Ok(dest) => {
                            let _ = std::process::Command::new("xdg-open")
                                .arg(&dest)
                                .stdout(std::process::Stdio::null())
                                .stderr(std::process::Stdio::null())
                                .spawn();
                            self.status_message = format!("Opened: {}", dest);
                        }
                        Err(e) => self.status_message = format!("Error: {}", e),
                    }
                } else {
                    self.status_message = "No database".into();
                }
            } else {
                self.status_message = "Usage: downloads-open [id]".into();
            }
            return;
        }
        if query == "downloads-dir" {
            if let Some(downloads_dir) = directories::UserDirs::new()
                .and_then(|d| d.download_dir().map(|p| p.to_path_buf()))
            {
                let _ = std::process::Command::new("xdg-open")
                    .arg(&downloads_dir)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn();
                self.status_message = format!("Opened: {}", downloads_dir.display());
            } else {
                self.status_message = "Could not determine downloads directory".into();
            }
            return;
        }

        if query == "cookies-clear" {
            self.pending_wry_actions.push_back(WryAction::RunJs(
                "document.cookie.split(';').forEach(function(c) { document.cookie = c.trim().split('=')[0] + '=;expires=Thu, 01 Jan 1970 00:00:00 GMT;path=/'; }); 'Cookies cleared'".into(),
            ));
            self.status_message = "Cookies cleared for current pane".into();
            return;
        }

        if query == "inspect" {
            self.pending_wry_actions.push_back(WryAction::ToggleDevTools);
            return;
        }

        if query == "extensions" {
            let ids = self.extension_manager.list();
            if ids.is_empty() {
                self.status_message = "No extensions loaded".into();
            } else {
                let names: Vec<String> = ids
                    .iter()
                    .map(|id| {
                        self.extension_manager
                            .get(id)
                            .map(|api| api.manifest().name.clone())
                            .unwrap_or_else(|| id.to_string())
                    })
                    .collect();
                self.status_message = format!("Extensions: {}", names.join(", "));
            }
            return;
        }

        if query == "extension-load" {
            let loaded = self.extension_manager.load_all();
            self.status_message = format!("Loaded {} extension(s)", loaded.len());
            return;
        }

        if let Some(id_str) = query.strip_prefix("extension-info ") {
            let id_str = id_str.trim();
            if id_str.is_empty() {
                self.status_message = "Usage: extension-info <id>".into();
                return;
            }
            let ext_id = ExtensionId(id_str.to_string());
            match self.extension_manager.get(&ext_id) {
                Some(api) => {
                    let m = api.manifest();
                    let perms = if m.permissions.is_empty() {
                        String::new()
                    } else {
                        format!(" | perms: {}", m.permissions.join(", "))
                    };
                    self.status_message = format!(
                        "{} v{} ({}){}",
                        m.name, m.version, api.extension_id(), perms,
                    );
                }
                None => {
                    self.status_message = format!("Extension '{}' not found", id_str);
                }
            }
            return;
        }

        if query == "language-list" {
            let locales = crate::i18n::available_locales();
            let current = crate::i18n::detect_locale();
            let items: Vec<String> = locales
                .iter()
                .map(|(locale, name)| {
                    if *locale == current {
                        format!("{}*", name)
                    } else {
                        name.to_string()
                    }
                })
                .collect();
            self.status_message = format!("Languages: {}", items.join(", "));
            return;
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
                self.status_message =
                    format!("Language: {} ({})", name, current.code());
            } else if let Some(locale) = crate::i18n::Locale::from_code(code) {
                crate::i18n::set_locale(locale);
                self.config.language = Some(code.to_string());
                let locales = crate::i18n::available_locales();
                let name = locales
                    .iter()
                    .find(|(l, _)| *l == locale)
                    .map(|(_, n)| *n)
                    .unwrap_or("?");
                self.status_message = format!("Language: {}", name);
            } else {
                let available: Vec<&str> = crate::i18n::available_locales()
                    .iter()
                    .map(|(l, _)| l.code())
                    .collect();
                self.status_message = format!(
                    "Unknown language: {}. Available: {}",
                    code,
                    available.join(", ")
                );
            }
            return;
        }

        if query == "privacy" {
            let https = self.config.https_upgrade_enabled;
            let tracking = self.config.tracking_protection_enabled;
            let adblock = self.config.adblock_enabled;
            self.status_message = format!(
                "HTTPS upgrade: {} | Tracking protection: {} | Adblock: {}",
                if https { "ON" } else { "OFF" },
                if tracking { "ON" } else { "OFF" },
                if adblock { "ON" } else { "OFF" },
            );
            return;
        }

        if query == "https-toggle" {
            let active_id = self.wm.active_pane_id();
            if let Some(engine) = self.engines.get(&active_id)
                && let Some(url) = engine.current_url()
                && let Some(host) = url.host_str()
            {
                let host_lower = host.to_lowercase();
                let safe_list = crate::net::privacy::load_https_safe_list();
                if crate::net::privacy::is_https_safe(&host_lower, &safe_list) {
                    self.status_message = format!(
                        "HTTPS upgrade: {} is already in the safe list",
                        host_lower
                    );
                } else {
                    self.status_message = format!(
                        "HTTPS upgrade: {} is not in the safe list ({} domains loaded)",
                        host_lower,
                        safe_list.len()
                    );
                }
            } else {
                self.status_message = "No active page URL".into();
            }
            return;
        }

        if query == "config-save" {
            self.pending_wry_actions.push_back(WryAction::SaveConfig);
            return;
        }

        if query == "memory" {
            let rss = crate::profiling::memory::process_rss_human();
            let term_count = self.terminal_pane_ids.len();
            let total_panes = self.wm.panes().len();
            let web_count = total_panes - term_count;
            let estimated = crate::profiling::memory::estimate_pane_memory(web_count, term_count);
            self.status_message = format!(
                "RSS: {} | WebViews: {}x~50MB | Terminals: {}x~3MB | Est pane: {}",
                rss, web_count, term_count,
                crate::profiling::memory::format_human_bytes(estimated)
            );
            return;
        }

        if query == "adaptive-quality" || query == "adaptive_quality" {
            self.config.adaptive_quality = !self.config.adaptive_quality;
            self.status_message = format!(
                "Adaptive quality: {}",
                if self.config.adaptive_quality { "on" } else { "off" }
            );
            return;
        }

        if query == "credentials-save" {
            self.pending_wry_actions.push_back(WryAction::RunJs(
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
            self.status_message = "Checking for credentials to save...".into();
            return;
        }

        if query == "import-firefox" {
            self.import_firefox();
            return;
        }

        if query == "import-chrome" {
            self.import_chrome();
            return;
        }

        if let Some(proxy_url) = query.strip_prefix("proxy ") {
            let proxy_url = proxy_url.trim();
            if proxy_url.is_empty() || proxy_url == "none" {
                self.config.proxy = None;
                unsafe { std::env::remove_var("all_proxy") };
                self.status_message = "Proxy disabled".into();
            } else {
                self.config.proxy = Some(proxy_url.to_string());
                unsafe { std::env::set_var("all_proxy", proxy_url) };
                self.status_message = format!("Proxy: {}", proxy_url);
            }
            return;
        }

        if query == "back" || query == "bd" {
            self.pending_wry_actions.push_back(WryAction::Back);
            return;
        }
        if query == "forward" || query == "fw" {
            self.pending_wry_actions.push_back(WryAction::Forward);
            return;
        }
        if query == "reload" {
            self.pending_wry_actions.push_back(WryAction::Reload);
            return;
        }

        // Rendering engine info: :engine, :engine auto, :engine servo, :engine webkit
        if query == "engine" {
            self.status_message = format!("Engine: {}", self.config.engine_selection);
            return;
        }
        if query == "engine auto" || query == "engine servo" || query == "engine webkit" {
            let val = query.strip_prefix("engine ").unwrap();
            match val.parse::<crate::servo::EngineSelection>() {
                Ok(selection) => {
                    self.config.engine_selection = selection.to_string();
                    self.status_message = format!("Engine: {}", selection);
                }
                Err(e) => {
                    self.status_message = e;
                }
            }
            return;
        }

        // Compat override command: :compat-override <add|remove|list> [domain] [engine]
        if let Some(rest) = query.strip_prefix("compat-override ") {
            let rest = rest.trim();
            let mut parts = rest.splitn(3, ' ');
            if let Some(subcmd) = parts.next() {
                match subcmd {
                    "list" => {
                        let all: Vec<String> = self
                            .config
                            .compat_overrides
                            .iter()
                            .map(|(k, v)| format!("{}={}", k, v))
                            .collect();
                        if all.is_empty() {
                            self.status_message = "No compat overrides".into();
                        } else {
                            let display = all.join(", ");
                            let msg = if display.len() > 80 {
                                format!("{}...", &display[..77])
                            } else {
                                display
                            };
                            self.status_message = format!("Compat overrides: {}", msg);
                        }
                    }
                    "add" => {
                        if let (Some(domain), Some(engine)) = (parts.next(), parts.next()) {
                            let engine = engine.trim();
                            if engine != "webkit" && engine != "servo" {
                                self.status_message = "Usage: compat-override add <domain> webkit|servo".into();
                            } else {
                                self.config.compat_overrides.insert(domain.to_string(), engine.to_string());
                                self.status_message = format!("Compat override: {} -> {}", domain, engine);
                            }
                        } else {
                            self.status_message = "Usage: compat-override add <domain> webkit|servo".into();
                        }
                    }
                    "remove" => {
                        if let Some(domain) = parts.next() {
                            if self.config.compat_overrides.remove(domain).is_some() {
                                self.status_message = format!("Removed override for {}", domain);
                            } else {
                                self.status_message = format!("No override for {}", domain);
                            }
                        } else {
                            self.status_message = "Usage: compat-override remove <domain>".into();
                        }
                    }
                    _ => {
                        self.status_message = "Usage: compat-override list|add|remove".into();
                    }
                }
            }
            return;
        }

        // Search engine switching: :engine <name>
        if let Some(engine_name) = query.strip_prefix("engine ") {
            let engine_name = engine_name.trim();
            if engine_name.is_empty() {
                let current = &self.config.search_engine;
                let name = self.config.search_engines.iter()
                    .find(|(_, url)| *url == current)
                    .map(|(name, _)| name.as_str())
                    .unwrap_or("default");
                self.status_message = format!("Search engine: {} ({})", name, current);
            } else if engine_name == "default" {
                self.config.search_engine = "https://duckduckgo.com/?q={query}".into();
                self.status_message = "Search engine: default (DuckDuckGo)".into();
            } else if let Some(url) = self.config.search_engines.get(engine_name) {
                self.config.search_engine = url.clone();
                self.status_message = format!("Search engine: {} ({})", engine_name, url);
            } else {
                let available: Vec<&str> = std::iter::once("default")
                    .chain(self.config.search_engines.keys().map(|s| s.as_str()))
                    .collect();
                self.status_message = format!("Unknown engine: {}. Available: {}", engine_name, available.join(", "));
            }
            return;
        }

        // Clear browsing data: :clear history|bookmarks|workspaces|all
        if let Some(subcmd) = query.strip_prefix("clear ") {
            let subcmd = subcmd.trim();
            match subcmd {
                "history" => {
                    if let Some(db) = self.db.as_ref() {
                        match crate::db::history::clear_history(db) {
                            Ok(count) => self.status_message = format!("Cleared {} history entries", count),
                            Err(e) => self.status_message = format!("Failed: {}", e),
                        }
                    }
                }
                "bookmarks" => {
                    if let Some(db) = self.db.as_ref() {
                        match crate::db::bookmarks::clear_bookmarks(db) {
                            Ok(count) => self.status_message = format!("Cleared {} bookmarks", count),
                            Err(e) => self.status_message = format!("Failed: {}", e),
                        }
                    }
                }
                "workspaces" => {
                    let workspaces = self.list_workspaces();
                    if let Some(db) = self.db.as_ref() {
                        for ws in &workspaces {
                            let _ = crate::db::workspaces::delete_workspace(db, &ws.name);
                        }
                    }
                    self.status_message = format!("Cleared {} workspaces", workspaces.len());
                }
                "cookies" => {
                    self.pending_wry_actions.push_back(WryAction::RunJs(
                        "document.cookie.split(';').forEach(function(c) { document.cookie = c.trim().split('=')[0] + '=;expires=Thu, 01 Jan 1970 00:00:00 GMT;path=/'; }); 'Cookies cleared'".into(),
                    ));
                    self.status_message = "Cookies cleared for current pane".into();
                }
                "all" => {
                    let mut parts = Vec::new();
                    if let Some(db) = self.db.as_ref() {
                        if let Ok(c) = crate::db::history::clear_history(db) {
                            parts.push(format!("{} history", c));
                        }
                        if let Ok(c) = crate::db::bookmarks::clear_bookmarks(db) {
                            parts.push(format!("{} bookmarks", c));
                        }
                        let ws = self.list_workspaces();
                        for w in &ws {
                            let _ = crate::db::workspaces::delete_workspace(db, &w.name);
                        }
                        parts.push(format!("{} workspaces", ws.len()));
                    }
                    self.status_message = format!("Cleared: {}", parts.join(", "));
                }
                _ => {
                    self.status_message = "Usage: :clear history|bookmarks|workspaces|cookies|all".into();
                }
            }
            return;
        }

        // Explicit navigate: open <url>
        if let Some(url_str) = query.strip_prefix("open ") {
            let url_str = url_str.trim();
            if url_str.is_empty() {
                self.status_message = "Usage: open <url>".into();
                return;
            }
            let url = if url_str.contains("://") {
                url::Url::parse(url_str)
            } else {
                url::Url::parse(&format!("https://{}", url_str))
            };
            match url {
                Ok(u) => {
                    self.navigate_with_redirects(u);
                    self.status_message = format!("Opening: {}", url_str);
                }
                Err(e) => {
                    self.status_message = format!("Invalid URL: {}", e);
                }
            }
            return;
        }

        // Shell command: !<cmd>
        if let Some(cmd) = query.strip_prefix("!") {
            let cmd = cmd.trim();
            if cmd.is_empty() {
                self.status_message = "Usage: !<command>".into();
                return;
            }
            match std::process::Command::new("sh").args(["-c", cmd]).output() {
                Ok(output) => {
                    let stdout =
                        String::from_utf8_lossy(&output.stdout).trim().to_string();
                    let line = stdout.lines().next().unwrap_or("");
                    if line.len() > 80 {
                        self.status_message = format!("{}...", &line[..77]);
                    } else if line.is_empty() {
                        self.status_message = format!("(exit {})", output.status);
                    } else {
                        self.status_message = line.to_string();
                    }
                }
                Err(e) => {
                    self.status_message = format!("!{}: {}", cmd, e);
                }
            }
            return;
        }

        // Runtime config: set <key> <value>
        if let Some(rest) = query.strip_prefix("set ") {
            let rest = rest.trim();
            let mut parts = rest.splitn(2, ' ');
            if let Some(key) = parts.next() {
                let value = parts.next().unwrap_or("");
                match key {
                    "search_engine" if !value.is_empty() => {
                        self.config.search_engine = value.to_string();
                        self.status_message = format!("search_engine = {}", value);
                    }
                    "homepage" if !value.is_empty() => {
                        self.config.homepage = value.to_string();
                        self.status_message = format!("homepage = {}", value);
                    }
                    "adblock" => {
                        self.config.adblock_enabled = !value.contains("off")
                            && !value.contains("false")
                            && !value.contains("0");
                        self.status_message =
                            format!("adblock = {}", self.config.adblock_enabled);
                    }
                    "https_upgrade" | "https-upgrade" => {
                        self.config.https_upgrade_enabled = !value.contains("off")
                            && !value.contains("false")
                            && !value.contains("0");
                        self.status_message = format!(
                            "https_upgrade = {}",
                            self.config.https_upgrade_enabled
                        );
                    }
                    "tracking_protection" | "tracking-protection" => {
                        self.config.tracking_protection_enabled = !value.contains("off")
                            && !value.contains("false")
                            && !value.contains("0");
                        self.status_message = format!(
                            "tracking_protection = {}",
                            self.config.tracking_protection_enabled
                        );
                    }
                    "popup_blocker" | "popup-blocker" | "popups" => {
                        self.config.popup_blocker_enabled = !value.contains("off")
                            && !value.contains("false")
                            && !value.contains("0");
                        self.status_message = format!(
                            "popup_blocker = {}",
                            self.config.popup_blocker_enabled
                        );
                    }
                    _ => {
                        self.status_message = format!(
                            "Unknown setting: {} (try: search_engine, homepage, adblock, https_upgrade, tracking_protection, popup_blocker)",
                            key
                        );
                    }
                }
            }
            return;
        }

        // Project-wide search: :grep <pattern> [path]
        if let Some(pattern) = query.strip_prefix("grep ") {
            let pattern = pattern.trim();
            if pattern.is_empty() {
                self.status_message = "Usage: :grep <pattern> [path]".into();
                return;
            }
            let (pattern, search_path) = if pattern.contains(' ') {
                let mut parts = pattern.splitn(2, ' ');
                let p = parts.next().unwrap_or("");
                let path = parts.next().unwrap_or(".");
                (p, path)
            } else {
                (pattern, ".")
            };

            let output = if std::path::PathBuf::from("/usr/bin/rg").exists() {
                std::process::Command::new("rg")
                    .args(["--no-heading", "-n", "-i", pattern, search_path])
                    .output()
            } else {
                std::process::Command::new("grep")
                    .args(["-rn", "-i", pattern, search_path])
                    .output()
            };

            match output {
                Ok(output) if output.status.success() => {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let lines: Vec<&str> = stdout.lines().take(15).collect();
                    let total = stdout.lines().count();
                    if lines.is_empty() {
                        self.status_message = "No matches found".into();
                    } else {
                        let results: Vec<String> = lines.iter().map(|l| {
                            if l.len() > 80 { format!("{}...", &l[..77]) } else { l.to_string() }
                        }).collect();
                        let suffix = if total > 15 { format!(" (+{} more)", total - 15) } else { String::new() };
                        self.status_message = format!("{}{}", results.join(" │ "), suffix);
                    }
                }
                Ok(output) => {
                    self.status_message = format!("grep: {}", String::from_utf8_lossy(&output.stderr));
                }
                Err(e) => {
                    self.status_message = format!("grep failed: {}", e);
                }
            }
            return;
        }

        if query == "git-status" || query == "gs" {
            if let Some(root) = crate::git::repo_root(std::env::current_dir().unwrap_or_default().as_path()) {
                match std::process::Command::new("git")
                    .args(["-C", &root.to_string_lossy(), "status", "--short"])
                    .output()
                {
                    Ok(output) if output.status.success() => {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        let lines: Vec<&str> = stdout.lines().take(10).collect();
                        if lines.is_empty() {
                            self.status_message = "Working tree clean".into();
                        } else {
                            let total = stdout.lines().count();
                            let suffix = if total > 10 { format!(" (+{} more)", total - 10) } else { String::new() };
                            self.status_message = format!("{}{}", lines.join(" │ "), suffix);
                        }
                    }
                    Ok(output) => {
                        self.status_message = format!("git: {}", String::from_utf8_lossy(&output.stderr).trim());
                    }
                    Err(e) => self.status_message = format!("git failed: {}", e),
                }
            } else {
                self.status_message = "Not in a git repository".into();
            }
            return;
        }

        if query == "git-log" || query == "gl" {
            if let Some(root) = crate::git::repo_root(std::env::current_dir().unwrap_or_default().as_path()) {
                match std::process::Command::new("git")
                    .args(["-C", &root.to_string_lossy(), "log", "--oneline", "-10"])
                    .output()
                {
                    Ok(output) if output.status.success() => {
                        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                        self.status_message = if stdout.is_empty() { "No commits".into() } else { format!("Log: {}", stdout.replace('\n', " │ ")) };
                    }
                    Ok(output) => {
                        self.status_message = format!("git: {}", String::from_utf8_lossy(&output.stderr).trim());
                    }
                    Err(e) => self.status_message = format!("git failed: {}", e),
                }
            } else {
                self.status_message = "Not in a git repository".into();
            }
            return;
        }

        if query == "git-diff" || query == "gd" {
            if let Some(root) = crate::git::repo_root(std::env::current_dir().unwrap_or_default().as_path()) {
                match std::process::Command::new("git")
                    .args(["-C", &root.to_string_lossy(), "diff", "--stat"])
                    .output()
                {
                    Ok(output) if output.status.success() => {
                        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                        self.status_message = if stdout.is_empty() { "No changes".into() } else { format!("Diff: {}", stdout.replace('\n', " │ ")) };
                    }
                    Ok(output) => {
                        self.status_message = format!("git: {}", String::from_utf8_lossy(&output.stderr).trim());
                    }
                    Err(e) => self.status_message = format!("git failed: {}", e),
                }
            } else {
                self.status_message = "Not in a git repository".into();
            }
            return;
        }

        if query == "terminal-clear" || query == "cls" {
            let active_id = self.wm.active_pane_id();
            if self.terminal_pane_ids.contains(&active_id) {
                self.pending_wry_actions.push_back(WryAction::RunJs(
                    r#"if (window._terminal && window._terminal.clear) { window._terminal.clear(); }"#.into(),
                ));
                self.status_message = "Terminal cleared".into();
            } else {
                self.status_message = "Not a terminal pane".into();
            }
            return;
        }

        if let Some(pattern) = query.strip_prefix("terminal-search ") {
            let pattern = pattern.trim();
            if pattern.is_empty() {
                self.status_message = "Usage: :terminal-search <pattern>".into();
                return;
            }
            let active_id = self.wm.active_pane_id();
            if self.terminal_pane_ids.contains(&active_id) {
                let escaped = pattern.replace('\\', "\\\\").replace('\'', "\\'");
                self.pending_wry_actions.push_back(WryAction::RunJs(
                    format!(r#"
if (window._terminal && window._terminal.buffer) {{
    var buffer = window._terminal.buffer;
    var lines = buffer.active.bufferBase.getLines();
    var matches = [];
    for (var i = 0; i < lines.length; i++) {{
        if (lines[i].includes('{}')) {{
            matches.push((i, lines[i].trim()));
        }}
    }}
    if (matches.length > 0) {{
        var firstMatch = matches[0];
        window._terminal.scrollToLine(firstMatch[0]);
    }}
    matches.length + ' match(es) in scrollback';
}}
"#, escaped),
                ));
            } else {
                self.status_message = "Not a terminal pane".into();
            }
            return;
        }

        // Print command
        if query == "print" {
            self.pending_wry_actions.push_back(WryAction::Print);
            self.status_message = "Printing...".into();
            return;
        }

        // Popup blocker toggle
        if query == "popups" {
            self.config.popup_blocker_enabled = !self.config.popup_blocker_enabled;
            self.status_message = format!(
                "Popup blocker: {}",
                if self.config.popup_blocker_enabled { "on" } else { "off" }
            );
            return;
        }
        if let Some(val) = query.strip_prefix("popups ") {
            let val = val.trim();
            self.config.popup_blocker_enabled =
                !val.contains("off") && !val.contains("false") && !val.contains("0");
            self.status_message = format!(
                "Popup blocker: {}",
                if self.config.popup_blocker_enabled { "on" } else { "off" }
            );
            return;
        }

        // Cookie management
        if query == "cookies" {
            self.pending_wry_actions.push_back(WryAction::RunJs(
                "document.cookie || '(no cookies for this site)'".into(),
            ));
            self.status_message = "Showing cookies...".into();
            return;
        }
        if let Some(domain) = query.strip_prefix("cookies-block ") {
            let domain = domain.trim();
            if domain.is_empty() {
                self.status_message = "Usage: :cookies-block <domain>".into();
                return;
            }
            if let Some(db) = self.db.as_ref() {
                match crate::db::site_settings::set_site_field(
                    db,
                    domain,
                    "exact",
                    "cookies",
                    Some("off"),
                ) {
                    Ok(()) => self.status_message = format!("Cookies blocked for {}", domain),
                    Err(e) => self.status_message = format!("Failed: {}", e),
                }
            }
            return;
        }
        if let Some(domain) = query.strip_prefix("cookies-allow ") {
            let domain = domain.trim();
            if domain.is_empty() {
                self.status_message = "Usage: :cookies-allow <domain>".into();
                return;
            }
            if let Some(db) = self.db.as_ref() {
                match crate::db::site_settings::set_site_field(
                    db,
                    domain,
                    "exact",
                    "cookies",
                    Some("on"),
                ) {
                    Ok(()) => self.status_message = format!("Cookies allowed for {}", domain),
                    Err(e) => self.status_message = format!("Failed: {}", e),
                }
            }
            return;
        }

        // Mute / unmute
        if query == "mute" {
            let active_id = self.wm.active_pane_id();
            self.muted_pane_ids.insert(active_id);
            self.pending_wry_actions.push_back(WryAction::RunJs(
                "document.querySelectorAll('video, audio').forEach(function(el) { el.muted = true; el.pause(); });"
                    .into(),
            ));
            self.status_message = "Muted".into();
            return;
        }
        if query == "unmute" {
            let active_id = self.wm.active_pane_id();
            self.muted_pane_ids.remove(&active_id);
            self.pending_wry_actions.push_back(WryAction::RunJs(
                "document.querySelectorAll('video, audio').forEach(function(el) { el.muted = false; });"
                    .into(),
            ));
            self.status_message = "Unmuted".into();
            return;
        }

        // Theme commands
        if query == "theme" {
            self.status_message = format!("Theme: {}", self.config.theme);
            return;
        }
        if query == "theme list" {
            let themes = self.config.available_themes();
            self.status_message = format!("Themes: {}", themes.join(", "));
            return;
        }
        if let Some(name) = query.strip_prefix("theme ") {
            let name = name.trim();
            if name.is_empty() {
                self.status_message = format!("Theme: {}", self.config.theme);
                return;
            }
            if self.config.themes.contains_key(name) {
                self.config.theme = name.to_string();
                self.status_message = format!("Theme: {}", name);
            } else {
                let available = self.config.available_themes();
                self.status_message = format!(
                    "Unknown theme '{}'. Available: {}",
                    name,
                    available.join(", ")
                );
            }
            return;
        }

        // Site settings commands
        if query == "site-settings" {
            let active_id = self.wm.active_pane_id();
            if let Some(engine) = self.engines.get(&active_id)
                && let Some(url) = engine.current_url()
            {
                if let Some(db) = self.db.as_ref() {
                    match crate::db::site_settings::get_site_settings_for_url(db, url.as_str()) {
                        Ok(settings) => {
                            if settings.is_empty() {
                                self.status_message = "No per-site settings for current URL".into();
                            } else {
                                let items: Vec<String> = settings
                                    .iter()
                                    .map(|s| {
                                        let mut parts = vec![format!("{}[{}]", s.pattern, s.pattern_type)];
                                        if let Some(z) = s.zoom_level {
                                            parts.push(format!("zoom={}", z));
                                        }
                                        if let Some(b) = s.adblock_enabled {
                                            parts.push(format!("adblock={}", if b { "on" } else { "off" }));
                                        }
                                        if let Some(b) = s.javascript_enabled {
                                            parts.push(format!("js={}", if b { "on" } else { "off" }));
                                        }
                                        if let Some(b) = s.cookies_enabled {
                                            parts.push(format!("cookies={}", if b { "on" } else { "off" }));
                                        }
                                        if let Some(b) = s.autoplay_enabled {
                                            parts.push(format!("autoplay={}", if b { "on" } else { "off" }));
                                        }
                                        parts.join(" ")
                                    })
                                    .collect();
                                self.status_message = format!("Site settings: {}", items.join(" | "));
                            }
                        }
                        Err(e) => self.status_message = format!("Error: {}", e),
                    }
                }
            } else {
                self.status_message = "No active URL".into();
            }
            return;
        }
        if let Some(rest) = query.strip_prefix("site-settings set ") {
            let rest = rest.trim();
            let mut parts = rest.splitn(2, ' ');
            if let (Some(key), Some(value)) = (parts.next(), parts.next()) {
                let value = value.trim();
                let active_id = self.wm.active_pane_id();
                let host = self
                    .engines
                    .get(&active_id)
                    .and_then(|e| e.current_url())
                    .and_then(|u| u.host_str())
                    .map(|h| h.to_lowercase())
                    .unwrap_or_default();

                if host.is_empty() {
                    self.status_message = "No active URL for site settings".into();
                } else if let Some(db) = self.db.as_ref() {
                    match crate::db::site_settings::set_site_field(db, &host, "exact", key, Some(value)) {
                        Ok(()) => self.status_message = format!("Set {}={} for {}", key, value, host),
                        Err(e) => self.status_message = format!("Failed: {}", e),
                    }
                }
            } else {
                self.status_message = "Usage: :site-settings set <key> <value> (zoom, adblock, js, cookies, autoplay)".into();
            }
            return;
        }
        if query == "site-settings list" {
            if let Some(db) = self.db.as_ref() {
                match crate::db::site_settings::list_site_settings(db) {
                    Ok(settings) => {
                        if settings.is_empty() {
                            self.status_message = "No site settings".into();
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
                            self.status_message = format!("{}{}", items.join(" | "), suffix);
                        }
                    }
                    Err(e) => self.status_message = format!("Error: {}", e),
                }
            }
            return;
        }
        if let Some(id_str) = query.strip_prefix("site-settings delete ") {
            let id_str = id_str.trim();
            if let Ok(id) = id_str.parse::<i64>() {
                if let Some(db) = self.db.as_ref() {
                    match crate::db::site_settings::delete_site_setting(db, id) {
                        Ok(true) => self.status_message = format!("Deleted site setting {}", id),
                        Ok(false) => self.status_message = format!("No site setting with id {}", id),
                        Err(e) => self.status_message = format!("Failed: {}", e),
                    }
                }
            } else {
                self.status_message = "Usage: :site-settings delete <id>".into();
            }
            return;
        }
        if let Some(domain) = query.strip_prefix("site-settings clear ") {
            let domain = domain.trim();
            if domain.is_empty() {
                self.status_message = "Usage: :site-settings clear <domain>".into();
                return;
            }
            if let Some(db) = self.db.as_ref() {
                match crate::db::site_settings::delete_site_settings_for_domain(db, domain) {
                    Ok(count) => self.status_message = format!("Cleared {} setting(s) for {}", count, domain),
                    Err(e) => self.status_message = format!("Failed: {}", e),
                }
            }
            return;
        }

        // Quickmark set: m<letter> <url>
        if query.starts_with('m') && query.len() >= 2 && query.as_bytes()[1].is_ascii_alphabetic() {
            let letter = query.as_bytes()[1] as char;
            let rest = query[2..].trim();
            if rest.is_empty() {
                self.status_message = format!("Quickmark {}: {}", letter,
                    self.quickmarks.get(&letter).map(|s| s.as_str()).unwrap_or("(not set)"));
                return;
            }
            self.quickmarks.insert(letter, rest.to_string());
            self.status_message = format!("Quickmark {} set", letter);
            return;
        }

        // Quickmark go: g<letter>
        if query.starts_with('g') && query.len() == 2 && query.as_bytes()[1].is_ascii_alphabetic() {
            let letter = query.as_bytes()[1] as char;
            match self.quickmarks.get(&letter) {
                Some(url_str) => {
                    if let Ok(url) = url::Url::parse(url_str) {
                        self.navigate_with_redirects(url);
                        self.status_message = format!("Quickmark {}", letter);
                    }
                }
                None => {
                    self.status_message = format!("Quickmark {} not set", letter);
                }
            }
            return;
        }

        // Sync commands
        if query == "sync" {
            self.execute_sync_push();
            return;
        }
        if query == "sync --pull" {
            self.execute_sync_pull();
            return;
        }
        if query == "sync --both" {
            self.execute_sync_push();
            self.execute_sync_pull();
            return;
        }
        if query == "sync --status" {
            self.execute_sync_status();
            return;
        }
        if query == "sync-watch" {
            self.execute_sync_watch();
            return;
        }
        if query == "sync-stop" {
            self.execute_sync_stop();
            return;
        }
        if let Some(target) = query.strip_prefix("sync-target ") {
            let target = target.trim();
            if target.is_empty() {
                self.status_message = "Usage: :sync-target <target>".into();
                return;
            }
            self.config.sync_target = target.to_string();
            self.status_message = format!("Sync target: {}", target);
            return;
        }

        // Check if it looks like a URL
        if Self::looks_like_url(query) {
            // Try parsing as-is first, then prepend https://
            let url = if let Ok(u) = url::Url::parse(query) {
                u
            } else if let Ok(u) = url::Url::parse(&format!("https://{}", query)) {
                u
            } else {
                self.status_message = format!("Invalid URL: {}", query);
                return;
            };

            // Update placeholder engine + apply URL redirects
            self.navigate_with_redirects(url);
            self.status_message = format!("Navigating to {}", query);
        } else {
            // Try fuzzy suggestion before falling back to search
            let known_commands = [
                "q", "quit", "open", "ssh", "set", "vs", "sp", "files", "browse",
                "bw-unlock", "bw-search", "bw-lock", "bw-autofill", "bw-detect",
                "keyring-test", "credentials-save",
                "adblock-toggle", "adblock-count", "adblock-update", "privacy", "https-toggle",
                "downloads", "downloads-open", "downloads-dir", "downloads-clear",
                "import-firefox", "import-chrome",
                "site-settings", "cookies", "cookies-clear", "cookies-block", "cookies-allow",
                "popups", "mute", "unmute", "theme", "theme-list",
                "print", "pdf", "pin",
                "scripts", "network", "network-clear", "console", "console-clear",
                "inspect", "proxy", "config-save", "clear",
                "layout-save", "layout-load", "ws-save", "ws-load", "ws-list",
                "reader", "minimal", "only", "detach",
                "memory", "perf", "perf-on", "perf-off",
                "adaptive-quality", "adaptive_quality",
                "language", "language-list",
                "engine", "compat-override",
                "extensions", "extension-load", "extension-info",
                "sync", "sync --pull", "sync --both", "sync --status",
                "sync-watch", "sync-stop", "sync-target",
            ];
            let cmd = query;
            let suggestion = known_commands
                .iter()
                .filter(|c| c.contains(cmd) || cmd.contains(*c))
                .min_by_key(|c| Self::levenshtein_distance(cmd, c));
            if let Some(sug) = suggestion {
                self.status_message = format!("Unknown command: {} (did you mean :{}?)", cmd, sug);
            } else if let Some(url) = self.config.search_url(query) {
                self.navigate_with_redirects(url);
                self.status_message = format!("Searching: {}", query);
            } else {
                self.status_message = format!("Search failed for: {}", query);
            }
        }
    }

    fn levenshtein_distance(a: &str, b: &str) -> usize {
        let a: Vec<char> = a.chars().collect();
        let b: Vec<char> = b.chars().collect();
        let m = a.len();
        let n = b.len();
        if m == 0 { return n; }
        if n == 0 { return m; }
        let mut dp = vec![vec![0; n + 1]; m + 1];
        for (i, row) in dp.iter_mut().enumerate().take(m + 1) { row[0] = i; }
        for (j, val) in dp[0].iter_mut().enumerate().take(n + 1).skip(1) { *val = j; }
        for i in 1..=m {
            for j in 1..=n {
                let cost = if a[i-1] == b[j-1] { 0 } else { 1 };
                dp[i][j] = (dp[i-1][j] + 1).min((dp[i][j-1] + 1).min(dp[i-1][j-1] + cost));
            }
        }
        dp[m][n]
    }

    /// Check if a string looks like a URL.
    /// Matches: http://, https://, aileron://, or bare domains like "example.com"
    pub(crate) fn looks_like_url(s: &str) -> bool {
        // Explicit scheme
        if s.contains("://") {
            return true;
        }
        // Bare domain: contains a dot and no spaces, and doesn't look like a command
        if s.contains('.') && !s.contains(' ') && !s.contains('/') {
            // Exclude things that look like file paths or commands
            let parts: Vec<&str> = s.split('.').collect();
            if parts.len() >= 2 && parts.iter().all(|p| !p.is_empty()) {
                // Check TLD is reasonable (at least 2 chars, all alpha)
                if let Some(tld) = parts.last() {
                    return tld.len() >= 2 && tld.chars().all(|c| c.is_alphabetic());
                }
            }
        }
        false
    }

    /// Call a registered Lua custom command by name.
    pub fn call_lua_command(&self, name: &str) -> anyhow::Result<String> {
        if let Some(ref engine) = self.lua_engine {
            engine.call_command(name, &[])
        } else {
            anyhow::bail!("Lua engine not initialized")
        }
    }

    /// Save the current pane layout as a named workspace.
    /// Uses URLs from the BSP tree's Pane structs (no live URL capture).
    pub fn save_workspace(&self, name: &str) -> anyhow::Result<()> {
        self.save_workspace_with_urls(name, &std::collections::HashMap::new())
    }

    /// Save the current pane layout as a named workspace with live URLs.
    /// The `pane_urls` map overrides BSP tree URLs with current wry pane URLs.
    pub fn save_workspace_with_urls(
        &self,
        name: &str,
        pane_urls: &std::collections::HashMap<uuid::Uuid, String>,
    ) -> anyhow::Result<()> {
        let db = self
            .db
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No database connection"))?;

        let url_resolver =
            |pane_id: uuid::Uuid| -> Option<String> { pane_urls.get(&pane_id).cloned() };

        let data = self.wm.to_workspace_data(url_resolver)?;
        crate::db::workspaces::save_workspace(db, name, &data)?;
        Ok(())
    }

    /// List all saved workspaces.
    pub fn list_workspaces(&self) -> Vec<crate::db::workspaces::Workspace> {
        self.db
            .as_ref()
            .and_then(|conn| crate::db::workspaces::list_workspaces(conn).ok())
            .unwrap_or_default()
    }

    pub fn record_visit(&self, url: &url::Url, title: &str) {
        if let Some(ref conn) = self.db
            && let Err(e) = crate::db::history::record_visit(conn, url, title) {
                warn!("Failed to record visit: {}", e);
            }
    }

    pub fn recent_history(&self, limit: usize) -> Vec<crate::db::history::HistoryEntry> {
        self.db
            .as_ref()
            .and_then(|conn| crate::db::history::recent_entries(conn, limit).ok())
            .unwrap_or_default()
    }

    pub fn search_history(
        &self,
        query: &str,
        limit: usize,
    ) -> Vec<crate::db::history::HistoryEntry> {
        self.db
            .as_ref()
            .and_then(|conn| crate::db::history::search(conn, query, limit).ok())
            .unwrap_or_default()
    }

    fn import_firefox(&mut self) {
        let db = match self.db.as_ref() {
            Some(db) => db,
            None => {
                self.status_message = "No database connection".into();
                return;
            }
        };

        let home = match std::env::var("HOME") {
            Ok(h) => h,
            Err(_) => {
                self.status_message = "Cannot determine HOME directory".into();
                return;
            }
        };

        let firefox_dir = std::path::Path::new(&home).join(".mozilla/firefox");
        if !firefox_dir.exists() {
            self.status_message = "Firefox data not found (~/.mozilla/firefox)".into();
            return;
        }

        let mut bookmarks_imported = 0usize;
        let mut history_imported = 0usize;

        let profiles: Vec<std::path::PathBuf> = std::fs::read_dir(&firefox_dir)
            .ok()
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
                    .filter(|e| {
                        e.file_name()
                            .to_str()
                            .map(|n| n.ends_with(".default") || n.contains(".default-"))
                            .unwrap_or(false)
                    })
                    .map(|e| e.path())
                    .collect()
            })
            .unwrap_or_default();

        for profile_dir in &profiles {
            let bk_dir = profile_dir.join("bookmarkbackups");
            if bk_dir.exists()
                && let Ok(entries) = std::fs::read_dir(&bk_dir)
            {
                let mut backups: Vec<std::path::PathBuf> = entries
                    .filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| {
                        p.extension()
                            .and_then(|e| e.to_str())
                            .map(|e| e == "json" || e == "html")
                            .unwrap_or(false)
                    })
                    .collect();
                backups.sort();
                backups.reverse();
                if let Some(latest) = backups.first() {
                    let ext = latest.extension().and_then(|e| e.to_str()).unwrap_or("");
                    if ext == "json" {
                        bookmarks_imported += Self::import_firefox_bookmarks_json(db, latest);
                    } else {
                        bookmarks_imported += Self::import_firefox_bookmarks_html(db, latest);
                    }
                }
            }

            let places_path = profile_dir.join("places.sqlite");
            if places_path.exists() {
                history_imported += Self::import_firefox_history(db, &places_path);
            }
        }

        self.status_message = format!(
            "Firefox import: {} bookmarks, {} history entries",
            bookmarks_imported, history_imported
        );
    }

    fn import_firefox_bookmarks_json(db: &rusqlite::Connection, path: &std::path::Path) -> usize {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return 0,
        };
        let json: serde_json::Value = match serde_json::from_str(&content) {
            Ok(j) => j,
            Err(_) => return 0,
        };
        let mut count = 0usize;
        Self::walk_firefox_json_bookmarks(&json, db, &mut count);
        count
    }

    fn walk_firefox_json_bookmarks(node: &serde_json::Value, db: &rusqlite::Connection, count: &mut usize) {
        if let Some(children) = node.get("children").and_then(|c| c.as_array()) {
            for child in children {
                if child.get("type").and_then(|t| t.as_str()) == Some("text/x-moz-place")
                    && let (Some(url), Some(title)) = (
                        child.get("uri").and_then(|u| u.as_str()),
                        child.get("title").and_then(|t| t.as_str()),
                    )
                    && url.starts_with("http")
                    && bookmarks::import_bookmark(db, url, title).unwrap_or(false)
                {
                    *count += 1;
                }
                Self::walk_firefox_json_bookmarks(child, db, count);
            }
        }
    }

    fn import_firefox_bookmarks_html(db: &rusqlite::Connection, path: &std::path::Path) -> usize {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return 0,
        };
        let mut count = 0usize;
        for line in content.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("<DT><A ")
                && let Some(href_start) = rest.find("HREF=\"")
                && let Some(href_end) = rest[href_start + 6..].find('"')
            {
                let after_href = &rest[href_start + 6..];
                let url = &after_href[..href_end];
                let title = after_href[href_end + 1..]
                    .find('>')
                    .and_then(|gt| {
                        let after_gt = &after_href[gt + 1..];
                        after_gt.find("</A>").map(|end| &after_gt[..end])
                    })
                    .unwrap_or(url);
                if url.starts_with("http")
                    && bookmarks::import_bookmark(db, url, title).unwrap_or(false)
                {
                    count += 1;
                }
            }
        }
        count
    }

    fn import_firefox_history(db: &rusqlite::Connection, places_path: &std::path::Path) -> usize {
        let conn = match rusqlite::Connection::open_with_flags(
            places_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        ) {
            Ok(c) => c,
            Err(_) => return 0,
        };
        let mut stmt = match conn.prepare(
            "SELECT p.url, p.title, h.visit_date
             FROM moz_places p
             JOIN moz_historyvisits h ON p.id = h.place_id
             WHERE p.url LIKE 'http%' AND h.visit_type IN (1, 2)
             ORDER BY h.visit_date DESC
             LIMIT 500",
        ) {
            Ok(s) => s,
            Err(_) => return 0,
        };
        let rows = match stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1).unwrap_or_default(),
                row.get::<_, i64>(2).unwrap_or(0),
            ))
        }) {
            Ok(r) => r,
            Err(_) => return 0,
        };
        let mut count = 0usize;
        for row in rows.filter_map(|r| r.ok()) {
            let (url, title, visit_date) = row;
            let visited_at = if visit_date > 0 {
                let epoch_us = visit_date / 1000;
                let secs = epoch_us / 1_000_000;
                chrono::DateTime::from_timestamp(secs, 0)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                    .unwrap_or_default()
            } else {
                String::new()
            };
            if crate::db::history::import_visit(db, &url, &title, &visited_at).unwrap_or(false) {
                count += 1;
            }
        }
        count
    }

    fn import_chrome(&mut self) {
        let db = match self.db.as_ref() {
            Some(db) => db,
            None => {
                self.status_message = "No database connection".into();
                return;
            }
        };

        let home = match std::env::var("HOME") {
            Ok(h) => h,
            Err(_) => {
                self.status_message = "Cannot determine HOME directory".into();
                return;
            }
        };

        let chrome_dir = std::path::Path::new(&home)
            .join(".config/google-chrome/Default");
        if !chrome_dir.exists() {
            self.status_message = "Chrome data not found (~/.config/google-chrome/Default)".into();
            return;
        }

        let bookmarks_path = chrome_dir.join("Bookmarks");
        let history_path = chrome_dir.join("History");

        let mut bookmarks_imported = 0usize;
        let mut history_imported = 0usize;

        if bookmarks_path.exists() {
            bookmarks_imported = Self::import_chrome_bookmarks(db, &bookmarks_path);
        }

        if history_path.exists() {
            history_imported = Self::import_chrome_history(db, &history_path);
        }

        self.status_message = format!(
            "Chrome import: {} bookmarks, {} history entries",
            bookmarks_imported, history_imported
        );
    }

    fn import_chrome_bookmarks(db: &rusqlite::Connection, path: &std::path::Path) -> usize {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return 0,
        };
        let json: serde_json::Value = match serde_json::from_str(&content) {
            Ok(j) => j,
            Err(_) => return 0,
        };
        let mut count = 0usize;
        if let Some(roots) = json.get("roots").and_then(|r| r.as_object()) {
            for (_key, node) in roots {
                Self::walk_chrome_bookmark_node(node, db, &mut count);
            }
        }
        count
    }

    fn walk_chrome_bookmark_node(node: &serde_json::Value, db: &rusqlite::Connection, count: &mut usize) {
        if node.get("type").and_then(|t| t.as_str()) == Some("url")
            && let (Some(url), Some(name)) = (
                node.get("url").and_then(|u| u.as_str()),
                node.get("name").and_then(|n| n.as_str()),
            )
            && url.starts_with("http")
            && bookmarks::import_bookmark(db, url, name).unwrap_or(false)
        {
            *count += 1;
            return;
        }
        if let Some(children) = node.get("children").and_then(|c| c.as_array()) {
            for child in children {
                Self::walk_chrome_bookmark_node(child, db, count);
            }
        }
    }

    fn import_chrome_history(db: &rusqlite::Connection, path: &std::path::Path) -> usize {
        let conn = match rusqlite::Connection::open_with_flags(
            path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        ) {
            Ok(c) => c,
            Err(_) => return 0,
        };
        let mut stmt = match conn.prepare(
            "SELECT u.url, u.title, v.visit_time
             FROM urls u
             JOIN visits v ON u.id = v.url
             ORDER BY v.visit_time DESC
             LIMIT 500",
        ) {
            Ok(s) => s,
            Err(_) => return 0,
        };
        let rows = match stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1).unwrap_or_default(),
                row.get::<_, i64>(2).unwrap_or(0),
            ))
        }) {
            Ok(r) => r,
            Err(_) => return 0,
        };
        let mut count = 0usize;
        for row in rows.filter_map(|r| r.ok()) {
            let (url, title, visit_time) = row;
            let visited_at = if visit_time > 0 {
                let epoch_us = visit_time / 1000;
                let secs = epoch_us / 1_000_000;
                chrono::DateTime::from_timestamp(secs, 0)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                    .unwrap_or_default()
            } else {
                String::new()
            };
            if crate::db::history::import_visit(db, &url, &title, &visited_at).unwrap_or(false) {
                count += 1;
            }
        }
        count
    }

    fn swap_panes(&mut self) {
        if let Some(last_id) = self.last_active_pane_id {
            let active_id = self.wm.active_pane_id();
            if last_id != active_id
                && self
                    .wm
                    .panes()
                    .iter()
                    .any(|(id, _)| *id == last_id)
            {
                let active_url = self
                    .engines
                    .get(&active_id)
                    .and_then(|e| e.current_url().cloned());
                let last_url = self
                    .engines
                    .get(&last_id)
                    .and_then(|e| e.current_url().cloned());
                if let (Some(a_url), Some(l_url)) = (active_url, last_url) {
                    if let Some(engine) = self.engines.get_mut(&active_id) {
                        engine.navigate(&l_url);
                    }
                    if let Some(engine) = self.engines.get_mut(&last_id) {
                        engine.navigate(&a_url);
                    }
                    self.status_message = "Panes swapped".into();
                }
            } else {
                self.status_message = "No previous pane to swap with".into();
            }
        } else {
            self.status_message = "No previous pane".into();
        }
    }

    fn execute_sync_push(&mut self) {
        if self.config.sync_target.is_empty() {
            self.status_message = "No sync target set. Use :sync-target <target>".into();
            return;
        }
        let target = match crate::sync::SyncTarget::parse(&self.config.sync_target) {
            Ok(t) => t,
            Err(e) => {
                self.status_message = format!("Invalid sync target: {}", e);
                return;
            }
        };

        let config_dir = crate::config::Config::config_dir();
        let sm = crate::sync::SyncManager::new(config_dir);
        let staging = sm.state_dir().to_path_buf();

        if let Err(e) = std::fs::create_dir_all(&staging) {
            self.status_message = format!("Failed to create staging dir: {}", e);
            return;
        }

        if self.config.sync_encrypted {
            if let Err(e) = sm.create_db_snapshots() {
                self.status_message = format!("DB snapshot failed: {}", e);
                return;
            }
            self.status_message = "Sync push (encrypted): preparing...".into();
        } else {
            if let Err(e) = sm.create_db_snapshots() {
                self.status_message = format!("DB snapshot failed: {}", e);
                return;
            }
        }

        match crate::sync::transport::push(sm.local_dir(), &staging, &target, self.config.sync_encrypted) {
            Ok(n) => {
                let _ = sm.save_manifest();
                self.status_message = format!(
                    "Synced {} files to {}",
                    n,
                    target.display()
                );
            }
            Err(e) => {
                self.status_message = format!("Sync push failed: {}", e);
            }
        }
    }

    fn execute_sync_pull(&mut self) {
        if self.config.sync_target.is_empty() {
            self.status_message = "No sync target set. Use :sync-target <target>".into();
            return;
        }
        let target = match crate::sync::SyncTarget::parse(&self.config.sync_target) {
            Ok(t) => t,
            Err(e) => {
                self.status_message = format!("Invalid sync target: {}", e);
                return;
            }
        };

        let config_dir = crate::config::Config::config_dir();
        let sm = crate::sync::SyncManager::new(config_dir);
        let staging = sm.state_dir().join("incoming");
        if let Err(e) = std::fs::create_dir_all(&staging) {
            self.status_message = format!("Failed to create staging dir: {}", e);
            return;
        }

        match crate::sync::transport::pull(sm.local_dir(), &staging, &target, self.config.sync_encrypted) {
            Ok(n) => {
                self.status_message = format!(
                    "Pulled {} files from {}",
                    n,
                    target.display()
                );
            }
            Err(e) => {
                self.status_message = format!("Sync pull failed: {}", e);
            }
        }
    }

    fn execute_sync_status(&mut self) {
        if self.config.sync_target.is_empty() {
            self.status_message = "Sync: disabled (no target)".into();
            return;
        }
        let config_dir = crate::config::Config::config_dir();
        let sm = crate::sync::SyncManager::new(config_dir);
        let manifest = sm.compute_manifest().unwrap_or_default();
        let parts = [
            format!("target: {}", self.config.sync_target),
            format!("encrypted: {}", self.config.sync_encrypted),
            format!("watcher: {}", if self.sync_watcher.is_running() { "running" } else { "stopped" }),
            format!("files: {}", manifest.files.len()),
        ];
        self.status_message = format!("Sync: {}", parts.join(" | "));
    }

    fn execute_sync_watch(&mut self) {
        if self.config.sync_target.is_empty() {
            self.status_message = "No sync target set. Use :sync-target <target>".into();
            return;
        }
        let config_dir = crate::config::Config::config_dir();
        match self.sync_watcher.start(&config_dir) {
            Ok(()) => {
                self.status_message = "Sync watcher started".into();
            }
            Err(e) => {
                self.status_message = format!("Failed to start watcher: {}", e);
            }
        }
    }

    fn execute_sync_stop(&mut self) {
        self.sync_watcher.stop();
        self.status_message = "Sync watcher stopped".into();
    }
}
