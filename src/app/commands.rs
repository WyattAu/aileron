use crate::app::WryAction;

use super::AppState;

impl AppState {
    pub(crate) fn navigate_with_redirects(&mut self, mut url: url::Url) {
        self.session.session_dirty = true;
        if let Some(ref engine) = self.lua_engine {
            url = engine.apply_url_redirects(&url);
        }
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
        self.handle_raw_command(cmd);
    }

    pub(crate) fn handle_raw_command(&mut self, query: &str) {
        if query.contains(" && ") {
            for part in query.split(" && ") {
                self.handle_raw_command(part.trim());
            }
            return;
        }

        match query {
            "q" | "quit" => {
                self.session.should_quit = true;
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
                let path =
                    crate::git::repo_root(std::env::current_dir().unwrap_or_default().as_path())
                        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
                let encoded =
                    crate::servo::wry_engine::percent_encode_path(&path.to_string_lossy());
                if let Ok(url) = url::Url::parse(&format!("aileron://files?path={encoded}")) {
                    self.navigate_with_redirects(url);
                    self.ui.status_message = format!("File browser: {}", path.display());
                }
                return;
            }
            _ => {}
        }

        if let Some(name) = query.strip_prefix("layout-save ") {
            let name = name.trim();
            if name.is_empty() {
                self.ui.status_message = "Usage: :layout-save <name>".into();
                return;
            }
            self.pending_wry_actions
                .push_back(WryAction::SaveWorkspace {
                    name: name.to_string(),
                    pane_urls: std::collections::HashMap::new(),
                });
            self.ui.status_message = format!("Saving layout: {name}...");
            return;
        }

        if let Some(name) = query.strip_prefix("layout-load ") {
            let name = name.trim();
            if name.is_empty() {
                self.ui.status_message = "Usage: :layout-load <name>".into();
                return;
            }
            self.pending_workspace_restore = Some(name.to_string());
            self.ui.status_message = format!("Loading layout: {name}...");
            return;
        }

        #[cfg(feature = "passwords")]
        if super::cmd::bitwarden::cmd_bitwarden(self, query).is_some() {
            return;
        }

        if let Some(path) = query.strip_prefix("pdf ") {
            let path = path.trim();
            if path.is_empty() {
                self.ui.status_message = "Usage: :pdf <path-or-url>".into();
                return;
            }
            let url = if path.starts_with("http://") || path.starts_with("https://") {
                url::Url::parse(path).ok()
            } else {
                let abs = std::path::Path::new(path);
                let abs = if abs.is_absolute() {
                    abs.to_path_buf()
                } else {
                    std::env::current_dir().unwrap_or_default().join(abs)
                };
                url::Url::from_file_path(abs).ok()
            };
            match url {
                Some(u) => {
                    self.navigate_with_redirects(u);
                    self.ui.status_message = format!("Loading PDF: {path}");
                }
                None => {
                    self.ui.status_message = format!("!Invalid path or URL: {path}");
                }
            }
            return;
        }

        if let Some(host) = query.strip_prefix("ssh ") {
            let host = host.trim();
            if host.is_empty() {
                self.ui.status_message = "Usage: ssh <host>".into();
                return;
            }
            self.pending_terminal_command = Some(format!("ssh {host}\n"));
            self.execute_action(&crate::input::Action::OpenTerminal);
            return;
        }

        if super::cmd::workspaces::handle_workspace_commands(self, query).is_some() {
            return;
        }

        if query == "only" {
            self.execute_action(&crate::input::Action::CloseOtherPanes);
            return;
        }

        if query == "reader" {
            self.execute_action(&crate::input::Action::ToggleReaderMode);
            return;
        }

        if query == "minimal" {
            self.execute_action(&crate::input::Action::ToggleMinimalMode);
            return;
        }

        if query == "settings" {
            if let Ok(url) = url::Url::parse("aileron://settings") {
                self.navigate_with_redirects(url);
                self.ui.status_message = "Settings".into();
            }
            return;
        }

        if query == "site-settings" {
            self.panels.site_settings_panel_open = !self.panels.site_settings_panel_open;
            if self.panels.site_settings_panel_open {
                self.panels.site_settings_zoom = None;
                self.panels.site_settings_js = None;
                self.panels.site_settings_cookies = None;
                self.panels.site_settings_adblock = None;
                self.panels.site_settings_url_pattern = String::new();
                self.ui.status_message = "Site settings panel opened".into();
            }
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
            self.pending_wry_actions
                .push_back(WryAction::ClearNetworkLog);
            return;
        }
        if query == "console" || query == "consolelog" {
            self.pending_wry_actions.push_back(WryAction::GetConsoleLog);
            return;
        }
        if query == "console-clear" {
            self.pending_wry_actions
                .push_back(WryAction::ClearConsoleLog);
            return;
        }

        if super::cmd::privacy::cmd_clear_privacy(self, query).is_some() {
            return;
        }

        if query == "inspect" {
            self.pending_wry_actions
                .push_back(WryAction::ToggleDevTools);
            return;
        }

        if super::cmd::extensions_cmd::cmd_extensions_language(self, query).is_some() {
            return;
        }

        #[cfg(feature = "arp")]
        if super::cmd::arp_cmd::cmd_arp(self, query).is_some() {
            return;
        }

        if super::cmd::tools::cmd_tools(self, query).is_some() {
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
            self.ui.status_message = format!(
                "RSS: {} | WebViews: {}x~50MB | Terminals: {}x~3MB | Est pane: {}",
                rss,
                web_count,
                term_count,
                crate::profiling::memory::format_human_bytes(estimated)
            );
            return;
        }

        if query == "stats" {
            let lat = &self.input_latency;
            self.ui.status_message = if lat.sample_count() == 0 {
                "No latency samples yet. Press some keys.".into()
            } else {
                format!(
                    "Input latency — avg: {:.1}ms | max: {:.1}ms | p99: {:.1}ms ({} samples)",
                    lat.avg_latency_ms(),
                    lat.max_latency_ms(),
                    lat.p99_latency_ms(),
                    lat.sample_count(),
                )
            };
            return;
        }

        if query == "adaptive-quality" || query == "adaptive_quality" {
            self.config.adaptive_quality = !self.config.adaptive_quality;
            self.ui.status_message = format!(
                "Adaptive quality: {}",
                if self.config.adaptive_quality {
                    "on"
                } else {
                    "off"
                }
            );
            return;
        }

        if super::cmd::downloads::cmd_downloads(self, query).is_some() {
            return;
        }

        if super::cmd::history::cmd_history(self, query).is_some() {
            return;
        }

        if query == "tabs" {
            self.panels.tab_search_open = !self.panels.tab_search_open;
            self.panels.tab_search_query.clear();
            self.panels.tab_search_selected = 0;
            return;
        }

        if super::cmd::tabs::handle_tab_commands(self, query).is_some() {
            return;
        }

        if query == "help" || query == "?" {
            self.panels.help_panel_open = true;
            return;
        }

        if super::cmd::bookmarks::handle_quickmark_commands(self, query).is_some() {
            return;
        }

        if super::cmd::bookmarks::cmd_bookmarks(self, query).is_some() {
            return;
        }

        if query == "crash-reload" {
            if let Some(url) = self.crash.crashed_pane_url.take() {
                self.crash.webview_crash_detected = false;
                if let Ok(parsed) = url::Url::parse(&url) {
                    self.pending_wry_actions
                        .push_back(WryAction::Navigate(parsed));
                    self.ui.status_message = format!("Reloaded crashed pane: {url}");
                }
            } else {
                self.ui.status_message = "No crash to recover from".into();
            }
            return;
        }

        if let Some(args) = query.strip_prefix("replace ") {
            let parts: Vec<&str> = args.splitn(3, ' ').collect();
            if parts.len() >= 2 {
                let old_text = parts[0];
                let new_text = parts[1];
                let case_sensitive = parts.len() >= 3 && parts[2] == "case";
                let flags = if case_sensitive { "g" } else { "gi" };
                let safe_old = old_text
                    .replace('\\', "\\\\")
                    .replace('"', "\\\"")
                    .replace('\'', "\\'")
                    .replace(')', "\\)")
                    .replace('}', "\\}")
                    .replace(']', "\\]");
                let safe_new = new_text
                    .replace('\\', "\\\\")
                    .replace('"', "\\\"")
                    .replace('\'', "\\'")
                    .replace('$', "\\$");
                let js = format!(
                    r#"(function() {{
                        let count = 0;
                        function walk(node) {{
                            if (node.nodeType === 3) {{
                                let re = new RegExp("{safe_old}", "{flags}");
                                let m = node.textContent.match(re);
                                if (m) count += m.length;
                                node.textContent = node.textContent.replace(re, "{safe_new}");
                            }} else {{
                                for (let c of node.childNodes) walk(c);
                            }}
                        }}
                        walk(document.body);
                        return count;
                    }})()"#
                );
                self.pending_wry_actions.push_back(WryAction::RunJs(js));
                let mode_str = if case_sensitive {
                    "case-sensitive"
                } else {
                    "case-insensitive"
                };
                self.ui.status_message =
                    format!("Replacing '{old_text}' with '{new_text}' ({mode_str})");
            } else {
                self.ui.status_message = "Usage: :replace <old> <new> [case]".into();
            }
            return;
        }

        if query == "import-firefox" {
            if let Some(db) = self.db.as_ref() {
                self.ui.status_message = super::cmd::import::import_firefox(db);
            } else {
                self.ui.status_message = "No database connection".into();
            }
            return;
        }

        if query == "import-chrome" {
            if let Some(db) = self.db.as_ref() {
                self.ui.status_message = super::cmd::import::import_chrome(db);
            } else {
                self.ui.status_message = "No database connection".into();
            }
            return;
        }

        if let Some(proxy_url) = query.strip_prefix("proxy ") {
            let proxy_url = proxy_url.trim();
            if proxy_url.is_empty() || proxy_url == "none" {
                self.config.proxy = None;
                self.ui.status_message = "Proxy disabled (restart required)".into();
            } else {
                self.config.proxy = Some(proxy_url.to_string());
                self.ui.status_message = format!("Proxy: {proxy_url} (restart required)");
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

        if query == "engine" {
            self.ui.status_message = format!("Engine: {}", self.config.engine_selection);
            return;
        }
        if query == "engine auto" || query == "engine servo" || query == "engine webkit" {
            let val = query.strip_prefix("engine ").unwrap();
            match val.parse::<crate::servo::EngineSelection>() {
                Ok(selection) => {
                    self.config.engine_selection = selection.to_string();
                    self.ui.status_message = format!("Engine: {selection}");
                }
                Err(e) => {
                    self.ui.status_message = e;
                }
            }
            return;
        }

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
                            .map(|(k, v)| format!("{k}={v}"))
                            .collect();
                        if all.is_empty() {
                            self.ui.status_message = "No compat overrides".into();
                        } else {
                            let display = all.join(", ");
                            let msg = if display.len() > 80 {
                                format!("{}...", &display[..77])
                            } else {
                                display
                            };
                            self.ui.status_message = format!("Compat overrides: {msg}");
                        }
                    }
                    "add" => {
                        if let (Some(domain), Some(engine)) = (parts.next(), parts.next()) {
                            let engine = engine.trim();
                            if engine != "webkit" && engine != "servo" {
                                self.ui.status_message =
                                    "Usage: compat-override add <domain> webkit|servo".into();
                            } else {
                                self.config
                                    .compat_overrides
                                    .insert(domain.to_string(), engine.to_string());
                                self.ui.status_message =
                                    format!("Compat override: {domain} -> {engine}");
                            }
                        } else {
                            self.ui.status_message =
                                "Usage: compat-override add <domain> webkit|servo".into();
                        }
                    }
                    "remove" => {
                        if let Some(domain) = parts.next() {
                            if self.config.compat_overrides.remove(domain).is_some() {
                                self.ui.status_message = format!("Removed override for {domain}");
                            } else {
                                self.ui.status_message = format!("No override for {domain}");
                            }
                        } else {
                            self.ui.status_message =
                                "Usage: compat-override remove <domain>".into();
                        }
                    }
                    _ => {
                        self.ui.status_message = "Usage: compat-override list|add|remove".into();
                    }
                }
            }
            return;
        }

        if let Some(engine_name) = query.strip_prefix("engine ") {
            let engine_name = engine_name.trim();
            if engine_name.is_empty() {
                let current = &self.config.search_engine;
                let name = self
                    .config
                    .search_engines
                    .iter()
                    .find(|(_, url)| *url == current)
                    .map(|(name, _)| name.as_str())
                    .unwrap_or("default");
                self.ui.status_message = format!("Search engine: {name} ({current})");
            } else if engine_name == "default" {
                self.config.search_engine = "https://duckduckgo.com/?q={query}".into();
                self.ui.status_message = "Search engine: default (DuckDuckGo)".into();
            } else if let Some(url) = self.config.search_engines.get(engine_name) {
                self.config.search_engine = url.clone();
                self.ui.status_message = format!("Search engine: {engine_name} ({url})");
            } else {
                let available: Vec<&str> = std::iter::once("default")
                    .chain(self.config.search_engines.keys().map(|s| s.as_str()))
                    .collect();
                self.ui.status_message = format!(
                    "Unknown engine: {}. Available: {}",
                    engine_name,
                    available.join(", ")
                );
            }
            return;
        }

        if super::cmd::site_settings::cmd_site_settings(self, query).is_some() {
            return;
        }

        if let Some(url_str) = query.strip_prefix("open ") {
            let url_str = url_str.trim();
            if url_str.is_empty() {
                self.ui.status_message = "Usage: open <url>".into();
                return;
            }
            let url = if url_str.contains("://") {
                url::Url::parse(url_str)
            } else {
                url::Url::parse(&format!("https://{url_str}"))
            };
            match url {
                Ok(u) => {
                    self.navigate_with_redirects(u);
                    self.ui.status_message = format!("Opening: {url_str}");
                }
                Err(e) => {
                    self.ui.status_message = format!("Invalid URL: {e}");
                }
            }
            return;
        }

        if query == "private" || query.starts_with("private ") {
            let active_id = self.wm.active_pane_id();
            self.tabs.private_pane_ids.insert(active_id);
            let target = query.strip_prefix("private ").unwrap_or("").trim();
            if !target.is_empty() {
                let url_str = if target.contains("://") {
                    target.to_string()
                } else {
                    format!("https://{target}")
                };
                if let Ok(url) = url::Url::parse(&url_str) {
                    self.navigate_with_redirects(url);
                    self.ui.status_message = format!("Private: {target}");
                } else {
                    self.ui.status_message = "Invalid URL".into();
                }
            } else {
                self.ui.status_message = "Private mode on (no history saved)".into();
            }
            return;
        }

        if query == "yt" {
            let active_id = self.wm.active_pane_id();
            if let Some(engine) = self.engines.get(&active_id)
                && let Some(url) = engine.current_url()
            {
                let title = url.host_str().unwrap_or("untitled").to_string();
                let copied = crate::platform::platform().clipboard_copy(&title);
                if copied {
                    self.ui.status_message = format!("Copied title: {title}");
                } else {
                    self.ui.status_message = "Clipboard: no clipboard tool available".into();
                }
                return;
            }
            self.ui.status_message = "No page to yank title from".into();
            return;
        }

        if let Some(cmd) = query.strip_prefix("!") {
            let cmd = cmd.trim();
            if cmd.is_empty() {
                self.ui.status_message = "Usage: !<command>".into();
                return;
            }
            let shell_cmd = crate::platform::platform().shell_command(cmd);
            let shell = &shell_cmd[0];
            let args = &shell_cmd[1..];
            match std::process::Command::new(shell).args(args).output() {
                Ok(output) => {
                    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    let line = stdout.lines().next().unwrap_or("");
                    if line.len() > 80 {
                        self.ui.status_message = format!("{}...", &line[..77]);
                    } else if line.is_empty() {
                        self.ui.status_message = format!("(exit {})", output.status);
                    } else {
                        self.ui.status_message = line.to_string();
                    }
                }
                Err(e) => {
                    self.ui.status_message = format!("!{cmd}: {e}");
                }
            }
            return;
        }

        if let Some(rest) = query.strip_prefix("set ") {
            let rest = rest.trim();
            let mut parts = rest.splitn(2, ' ');
            if let Some(key) = parts.next() {
                let value = parts.next().unwrap_or("");
                self.ui.status_message =
                    super::cmd::settings::apply_set_setting(&mut self.config, key, value);
                self.cache.config_json_dirty = true;
            }
            return;
        }

        if query == "popups" {
            self.config.popup_blocker_enabled = !self.config.popup_blocker_enabled;
            self.ui.status_message = format!(
                "Popup blocker: {}",
                if self.config.popup_blocker_enabled {
                    "on"
                } else {
                    "off"
                }
            );
            return;
        }
        if let Some(val) = query.strip_prefix("popups ") {
            let val = val.trim();
            self.config.popup_blocker_enabled =
                !val.contains("off") && !val.contains("false") && !val.contains("0");
            self.ui.status_message = format!(
                "Popup blocker: {}",
                if self.config.popup_blocker_enabled {
                    "on"
                } else {
                    "off"
                }
            );
            return;
        }

        if query == "mute" {
            let active_id = self.wm.active_pane_id();
            self.tabs.muted_pane_ids.insert(active_id);
            self.pending_wry_actions.push_back(WryAction::RunJs(
                "document.querySelectorAll('video, audio').forEach(function(el) { el.muted = true; el.pause(); });"
                    .into(),
            ));
            self.ui.status_message = "Muted".into();
            return;
        }
        if query == "unmute" {
            let active_id = self.wm.active_pane_id();
            self.tabs.muted_pane_ids.remove(&active_id);
            self.pending_wry_actions.push_back(WryAction::RunJs(
                "document.querySelectorAll('video, audio').forEach(function(el) { el.muted = false; });"
                    .into(),
            ));
            self.ui.status_message = "Unmuted".into();
            return;
        }

        #[cfg(feature = "passwords")]
        if query == "autofill" {
            let active_id = self.wm.active_pane_id();
            if let Some(engine) = self.engines.get(&active_id)
                && let Some(url) = engine.current_url()
            {
                let url_str = url.to_string();
                let domain = url.domain().unwrap_or("unknown");
                if !self.bitwarden.is_unlocked() {
                    self.ui.status_message =
                        format!("No credentials found for {domain} (vault locked)");
                } else {
                    match self.bitwarden.search_for_url(&url_str) {
                        Ok(items) if !items.is_empty() => {
                            match self.bitwarden.get_credential(&items[0].id) {
                                Ok(cred) => {
                                    let js = self.bitwarden.autofill_by_id_js(
                                        &self.autofill.username_id,
                                        &self.autofill.password_id,
                                        &cred,
                                    );
                                    self.pending_wry_actions.push_back(WryAction::RunJs(js));
                                    self.ui.status_message =
                                        format!("Auto-filled credentials for {domain}");
                                    self.autofill.available = false;
                                    self.autofill.js = None;
                                }
                                Err(e) => self.ui.status_message = format!("Auto-fill failed: {e}"),
                            }
                        }
                        Ok(_) => {
                            self.ui.status_message = format!("No credentials found for {domain}")
                        }
                        Err(e) => self.ui.status_message = format!("Auto-fill failed: {e}"),
                    }
                }
            } else {
                self.ui.status_message = "No login form detected".into();
            }
            return;
        }

        if query == "theme" {
            self.ui.status_message = format!("Theme: {}", self.config.theme);
            return;
        }
        if query == "theme list" {
            let themes = self.config.available_themes();
            self.ui.status_message = format!("Themes: {}", themes.join(", "));
            return;
        }
        if let Some(name) = query.strip_prefix("theme ") {
            let name = name.trim();
            if name.is_empty() {
                self.ui.status_message = format!("Theme: {}", self.config.theme);
                return;
            }
            if self.config.themes.contains_key(name) {
                self.config.theme = name.to_string();
                self.ui.status_message = format!("Theme: {name}");
            } else {
                let available = self.config.available_themes();
                self.ui.status_message = format!(
                    "Unknown theme '{}'. Available: {}",
                    name,
                    available.join(", ")
                );
            }
            return;
        }

        if query.starts_with('m') && query.len() >= 2 && query.as_bytes()[1].is_ascii_alphabetic() {
            let letter = query.as_bytes()[1] as char;
            let rest = query[2..].trim();
            if rest.is_empty() {
                self.ui.status_message = format!(
                    "Quickmark {}: {}",
                    letter,
                    self.session
                        .quickmarks
                        .get(&letter.to_string())
                        .map(|s| s.as_str())
                        .unwrap_or("(not set)")
                );
                return;
            }
            let key = letter.to_string();
            self.session
                .quickmarks
                .insert(key.clone(), rest.to_string());
            if let Some(ref conn) = self.db
                && let Err(e) = crate::db::quickmarks::set_quickmark(conn, &key, rest)
            {
                tracing::warn!("Failed to persist quickmark {}: {}", key, e);
            }
            self.ui.status_message = format!("Quickmark {letter} set");
            return;
        }

        if query.starts_with('g') && query.len() == 2 && query.as_bytes()[1].is_ascii_alphabetic() {
            let letter = query.as_bytes()[1] as char;
            let key = letter.to_string();
            match self.session.quickmarks.get(&key) {
                Some(url_str) => {
                    if let Ok(url) = url::Url::parse(url_str) {
                        self.navigate_with_redirects(url);
                        self.ui.status_message = format!("Quickmark {letter}");
                    }
                }
                None => {
                    self.ui.status_message = format!("Quickmark {letter} not set");
                }
            }
            return;
        }

        if query.starts_with("g ") && query.len() > 2 {
            let target = query[2..].trim();
            if !target.is_empty() {
                let url_str = if target.contains("://") {
                    target.to_string()
                } else {
                    format!("https://{target}")
                };
                if let Ok(url) = url::Url::parse(&url_str) {
                    self.pending_new_tab_url = Some(url);
                    self.execute_action(&crate::input::Action::SplitHorizontal);
                    return;
                } else {
                    self.ui.status_message = "Invalid URL".into();
                }
            }
            return;
        }

        #[cfg(feature = "sync")]
        if query == "sync" {
            self.ui.status_message = super::cmd::sync::execute_sync_push(
                &self.config.sync_target,
                self.config.sync_encrypted,
            );
            return;
        }
        #[cfg(feature = "sync")]
        if query == "sync --pull" {
            self.ui.status_message = super::cmd::sync::execute_sync_pull(
                &self.config.sync_target,
                self.config.sync_encrypted,
            );
            return;
        }
        #[cfg(feature = "sync")]
        if query == "sync --both" {
            self.ui.status_message = super::cmd::sync::execute_sync_push(
                &self.config.sync_target,
                self.config.sync_encrypted,
            );
            let pull_msg = super::cmd::sync::execute_sync_pull(
                &self.config.sync_target,
                self.config.sync_encrypted,
            );
            self.ui.status_message = format!("{} | {}", self.ui.status_message, pull_msg);
            return;
        }
        #[cfg(feature = "sync")]
        if query == "sync --status" {
            self.ui.status_message = super::cmd::sync::execute_sync_status(
                &self.config.sync_target,
                self.config.sync_encrypted,
                self.sync_watcher.is_running(),
            );
            return;
        }
        #[cfg(feature = "sync")]
        if query == "sync-watch" {
            if let Err(e) = super::cmd::sync::execute_sync_watch(&self.config.sync_target) {
                self.ui.status_message = e;
            } else {
                let config_dir = crate::config::Config::config_dir();
                match self.sync_watcher.start(&config_dir) {
                    Ok(()) => self.ui.status_message = "Sync watcher started".into(),
                    Err(e) => self.ui.status_message = format!("Failed to start watcher: {e}"),
                }
            }
            return;
        }
        #[cfg(feature = "sync")]
        if query == "sync-stop" {
            self.sync_watcher.stop();
            self.ui.status_message = "Sync watcher stopped".into();
            return;
        }
        if let Some(target) = query.strip_prefix("sync-target ") {
            let target = target.trim();
            if target.is_empty() {
                self.ui.status_message = "Usage: :sync-target <target>".into();
                return;
            }
            self.config.sync_target = target.to_string();
            self.ui.status_message = format!("Sync target: {target}");
            return;
        }

        if super::cmd::util::looks_like_url(query) {
            let url = if let Ok(u) = url::Url::parse(query) {
                u
            } else if let Ok(u) = url::Url::parse(&format!("https://{query}")) {
                u
            } else {
                self.ui.status_message = format!("Invalid URL: {query}");
                return;
            };

            self.navigate_with_redirects(url);
            self.ui.status_message = format!("Navigating to {query}");
        } else {
            let known_commands = [
                "q",
                "quit",
                "open",
                "help",
                "?",
                "ssh",
                "set",
                "vs",
                "sp",
                "files",
                "browse",
                "g",
                "bw-unlock",
                "bw-search",
                "bw-lock",
                "bw-autofill",
                "bw-detect",
                "keyring-test",
                "credentials-save",
                "adblock-toggle",
                "adblock-count",
                "adblock-update",
                "privacy",
                "https-toggle",
                "downloads",
                "downloads-open",
                "downloads-dir",
                "downloads-clear",
                "import-firefox",
                "import-chrome",
                "site-settings",
                "cookies",
                "cookies-clear",
                "cookies-block",
                "cookies-allow",
                "popups",
                "mute",
                "unmute",
                "theme",
                "theme-list",
                "autofill",
                "print",
                "pdf",
                "pin",
                "scripts",
                "network",
                "network-clear",
                "console",
                "console-clear",
                "inspect",
                "proxy",
                "config-save",
                "clear",
                "layout-save",
                "layout-load",
                "ws-save",
                "ws-load",
                "ws-list",
                "ws-delete",
                "ws-panel",
                "workspaces",
                "ws-next",
                "ws-prev",
                "reader",
                "minimal",
                "only",
                "detach",
                "replace",
                "memory",
                "stats",
                "perf",
                "perf-on",
                "perf-off",
                "adaptive-quality",
                "adaptive_quality",
                "language",
                "language-list",
                "engine",
                "compat-override",
                "extensions",
                "extension-load",
                "extension-info",
                "extension-list",
                "extension-enable",
                "arp-start",
                "arp-stop",
                "arp-status",
                "arp-token",
                "history",
                "history-clear",
                "tabs",
                "tab-restore",
                "tab-unload",
                "tab-move",
                "tab-rename",
                "bookmarks",
                "bookmark",
                "quickmark-add",
                "quickmark-del",
                "quickmark-list",
                "crash-reload",
                "private",
                "yt",
                "bind",
                "unbind",
                "sync",
                "sync --pull",
                "sync --both",
                "sync --status",
                "sync-watch",
                "sync-stop",
                "sync-target",
            ];
            let cmd = query;
            let suggestion = known_commands
                .iter()
                .filter(|c| c.contains(cmd) || cmd.contains(*c))
                .min_by_key(|c| super::cmd::util::levenshtein_distance(cmd, c));
            if let Some(sug) = suggestion {
                self.ui.status_message = format!("Unknown command: {cmd} (did you mean :{sug}?)");
            } else if let Some(url) = self.config.search_url(query) {
                self.navigate_with_redirects(url);
                self.ui.status_message = format!("Searching: {query}");
            } else {
                self.ui.status_message = format!("Search failed for: {query}");
            }
        }
    }

    pub fn call_lua_command(&self, name: &str) -> anyhow::Result<String> {
        if let Some(ref engine) = self.lua_engine {
            engine.call_command(name, &[])
        } else {
            anyhow::bail!("Lua engine not initialized")
        }
    }

    pub fn save_workspace_with_urls(
        &self,
        name: &str,
        pane_urls: &std::collections::HashMap<uuid::Uuid, String>,
    ) -> anyhow::Result<()> {
        super::cmd::workspaces::save_workspace_with_urls(self, name, pane_urls)
    }

    pub fn list_workspaces(&self) -> Vec<crate::db::workspaces::Workspace> {
        super::cmd::workspaces::list_workspaces(self)
    }

    pub fn record_visit(&self, url: &url::Url, title: &str) {
        super::cmd::history::record_visit(self, url, title)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn make_state() -> AppState {
        let viewport = crate::wm::Rect::new(0.0, 0.0, 800.0, 600.0);
        AppState::new(viewport, Config::default()).unwrap()
    }

    #[test]
    fn test_quit_command() {
        let mut state = make_state();
        state.handle_raw_command("quit");
        assert!(state.session.should_quit);
    }

    #[test]
    fn test_q_alias() {
        let mut state = make_state();
        state.handle_raw_command("q");
        assert!(state.session.should_quit);
    }

    #[test]
    fn test_set_command() {
        let mut state = make_state();
        state.handle_raw_command("set adblock off");
        assert!(!state.config.adblock_enabled);
        assert!(state.ui.status_message.contains("adblock"));
    }

    #[test]
    fn test_config_save() {
        let mut state = make_state();
        state.handle_raw_command("config-save");
        assert!(!state.pending_wry_actions.is_empty());
        matches!(
            state.pending_wry_actions.front(),
            Some(WryAction::SaveConfig)
        );
    }

    #[test]
    fn test_inspect_command() {
        let mut state = make_state();
        state.handle_raw_command("inspect");
        assert!(!state.pending_wry_actions.is_empty());
        matches!(
            state.pending_wry_actions.front(),
            Some(WryAction::ToggleDevTools)
        );
    }

    #[test]
    fn test_back_forward_reload() {
        let mut state = make_state();
        state.handle_raw_command("back");
        assert!(
            state
                .pending_wry_actions
                .iter()
                .any(|a| matches!(a, WryAction::Back))
        );
        state.pending_wry_actions.clear();

        state.handle_raw_command("fw");
        assert!(
            state
                .pending_wry_actions
                .iter()
                .any(|a| matches!(a, WryAction::Forward))
        );
        state.pending_wry_actions.clear();

        state.handle_raw_command("reload");
        assert!(
            state
                .pending_wry_actions
                .iter()
                .any(|a| matches!(a, WryAction::Reload))
        );
    }

    #[test]
    fn test_unknown_command_suggests() {
        let mut state = make_state();
        state.handle_raw_command("quitt");
        assert!(state.ui.status_message.contains("did you mean"));
    }

    #[test]
    fn test_url_navigation() {
        let mut state = make_state();
        state.handle_raw_command("https://example.com");
        assert!(
            state
                .pending_wry_actions
                .iter()
                .any(|a| matches!(a, WryAction::Navigate(_)))
        );
        assert!(state.ui.status_message.contains("Navigating"));
    }

    #[test]
    fn test_bare_domain_navigation() {
        let mut state = make_state();
        state.handle_raw_command("example.com");
        assert!(
            state
                .pending_wry_actions
                .iter()
                .any(|a| matches!(a, WryAction::Navigate(_)))
        );
    }

    #[test]
    fn test_open_command() {
        let mut state = make_state();
        state.handle_raw_command("open https://example.com");
        assert!(
            state
                .pending_wry_actions
                .iter()
                .any(|a| matches!(a, WryAction::Navigate(_)))
        );
    }

    #[test]
    fn test_open_command_invalid() {
        let mut state = make_state();
        state.handle_raw_command("open :::invalid");
        assert!(state.ui.status_message.contains("Invalid URL"));
    }

    #[test]
    fn test_shell_command() {
        let mut state = make_state();
        state.handle_raw_command("!echo hello");
        assert!(
            state.ui.status_message.contains("hello") || state.ui.status_message.contains("echo")
        );
    }

    #[test]
    fn test_print_command() {
        let mut state = make_state();
        state.handle_raw_command("print");
        assert!(
            state
                .pending_wry_actions
                .iter()
                .any(|a| matches!(a, WryAction::Print))
        );
        assert!(state.ui.status_message.contains("Printing"));
    }

    #[test]
    fn test_theme_command() {
        let mut state = make_state();
        state.handle_raw_command("theme");
        assert!(state.ui.status_message.contains("Theme:"));
    }

    #[test]
    fn test_mute_unmute() {
        let mut state = make_state();
        state.handle_raw_command("mute");
        assert!(!state.tabs.muted_pane_ids.is_empty());
        state.handle_raw_command("unmute");
        assert!(state.tabs.muted_pane_ids.is_empty());
    }

    #[test]
    fn test_chain_commands() {
        let mut state = make_state();
        state.handle_raw_command("print && print");
        let print_count = state
            .pending_wry_actions
            .iter()
            .filter(|a| matches!(a, WryAction::Print))
            .count();
        assert_eq!(print_count, 2);
    }

    #[test]
    fn test_privacy_command() {
        let mut state = make_state();
        state.handle_raw_command("privacy");
        assert!(state.ui.status_message.contains("HTTPS upgrade"));
        assert!(state.ui.status_message.contains("Tracking protection"));
        assert!(state.ui.status_message.contains("Adblock"));
    }

    #[test]
    fn test_adaptive_quality_toggle() {
        let mut state = make_state();
        let original = state.config.adaptive_quality;
        state.handle_raw_command("adaptive-quality");
        assert_ne!(state.config.adaptive_quality, original);
    }

    #[test]
    fn test_proxy_none() {
        let mut state = make_state();
        state.handle_raw_command("proxy none");
        assert!(state.config.proxy.is_none());
        assert!(state.ui.status_message.contains("disabled"));
    }

    #[test]
    fn test_extensions_list() {
        let mut state = make_state();
        state.handle_raw_command("extensions");
        assert!(
            state.ui.status_message.contains("No extensions")
                || state.ui.status_message.contains("Extensions:")
        );
    }

    #[test]
    fn test_cookies_clear() {
        let mut state = make_state();
        state.handle_raw_command("cookies-clear");
        assert!(state.ui.status_message.contains("Cookies cleared"));
    }

    #[test]
    fn test_keyring_test() {
        let mut state = make_state();
        state.handle_raw_command("keyring-test");
        assert!(state.ui.status_message.contains("keyring"));
    }

    #[test]
    fn test_memory_command() {
        let mut state = make_state();
        state.handle_raw_command("memory");
        assert!(state.ui.status_message.contains("RSS"));
    }

    #[test]
    fn test_history_toggle() {
        let mut state = make_state();
        state.handle_raw_command("history");
        assert!(state.panels.history_panel_open);
        state.handle_raw_command("history");
        assert!(!state.panels.history_panel_open);
    }

    #[test]
    fn test_history_clear() {
        let mut state = make_state();
        state.handle_raw_command("history-clear");
        assert!(!state.ui.status_message.is_empty());
    }

    #[test]
    fn test_tabs_toggle() {
        let mut state = make_state();
        state.handle_raw_command("tabs");
        assert!(state.panels.tab_search_open);
        state.handle_raw_command("tabs");
        assert!(!state.panels.tab_search_open);
    }

    #[test]
    fn test_autofill_command_vault_locked() {
        let mut state = make_state();
        state.handle_raw_command("autofill");
        assert!(
            state.ui.status_message.contains("vault locked")
                || state.ui.status_message.contains("credentials")
                || state.ui.status_message.contains("login form")
        );
    }

    #[test]
    fn test_pdf_command_url_navigation() {
        let mut state = make_state();
        state.handle_raw_command("pdf https://example.com/document.pdf");
        assert!(
            state
                .pending_wry_actions
                .iter()
                .any(|a| matches!(a, WryAction::Navigate(_))),
            "pdf command should queue a Navigate action"
        );
        assert!(state.ui.status_message.contains("Loading PDF"));
    }

    #[test]
    fn test_pdf_command_empty_path() {
        let mut state = make_state();
        state.handle_raw_command("pdf ");
        assert!(state.ui.status_message.contains("Usage"));
    }

    #[test]
    fn test_pdf_command_usage() {
        let mut state = make_state();
        state.handle_raw_command("pdf  ");
        assert!(state.ui.status_message.contains("Usage"));
    }

    #[test]
    fn test_pdf_command_nonexistent_file() {
        let mut state = make_state();
        state.handle_raw_command("pdf /nonexistent/path/file.pdf");
        assert!(
            state
                .pending_wry_actions
                .iter()
                .any(|a| matches!(a, WryAction::Navigate(_))),
            "pdf command should queue Navigate even for nonexistent files"
        );
    }

    #[test]
    fn test_extension_list_command() {
        let mut state = make_state();
        state.handle_raw_command("extension-list");
        assert!(
            state.ui.status_message.contains("No extensions")
                || state.ui.status_message.contains("Extensions (")
        );
    }

    #[test]
    fn test_extension_enable_already_enabled() {
        let mut state = make_state();
        state.extension_manager.write().register_builtin_adblock();
        state.handle_raw_command("extension-enable aileron-adblock@builtin");
        assert!(state.ui.status_message.contains("already enabled"));
    }

    #[test]
    fn test_extension_enable_not_found() {
        let mut state = make_state();
        state.handle_raw_command("extension-enable nonexistent@example.com");
        assert!(state.ui.status_message.contains("Failed to enable"));
    }

    #[test]
    fn test_extension_enable_usage() {
        let mut state = make_state();
        state.handle_raw_command("extension-enable ");
        assert!(state.ui.status_message.contains("Usage"));
    }
}
