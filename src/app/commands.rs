use tracing::{info, warn};
use open::that as open_that;
use crate::app::WryAction;
use crate::downloads::DownloadProgress;
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
        self.handle_raw_command(cmd);
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

        // Bitwarden/credential commands (bw-*, keyring-test, credentials-save)
        if self.cmd_bitwarden(query).is_some() { return; }

        if let Some(path) = query.strip_prefix("pdf ") {
            let path = path.trim();
            if path.is_empty() {
                self.status_message = "Usage: :pdf <path-or-url>".into();
                return;
            }
            open_that(path)
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
                self.status_message = "Settings".into();
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

        // Clear/privacy commands
        if self.cmd_clear_privacy(query).is_some() { return; }

        if query == "inspect" {
            self.pending_wry_actions.push_back(WryAction::ToggleDevTools);
            return;
        }

        // Extensions/language commands
        if self.cmd_extensions_language(query).is_some() { return; }

        // ARP (Aileron Remote Protocol) commands
        if self.cmd_arp(query).is_some() { return; }

        // Tools commands (grep, git, terminal, print)
        if self.cmd_tools(query).is_some() { return; }

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

        // Downloads commands
        if self.cmd_downloads(query).is_some() { return; }

        // History command
        if self.cmd_history(query).is_some() { return; }

        // Tab search command
        if query == "tabs" {
            self.tab_search_open = !self.tab_search_open;
            self.tab_search_query.clear();
            self.tab_search_selected = 0;
            return;
        }

        // Tab restore command
        if let Some(rest) = query.strip_prefix("tab-restore ") {
            // :tab-restore N — restore the Nth most recent closed tab
            if let Ok(n) = rest.trim().parse::<usize>() {
                if n == 0 {
                    // :tab-restore 0 = most recent
                    if let Some((url, _title)) = self.closed_tab_stack.pop() {
                        if let Ok(parsed) = url::Url::parse(&url) {
                            self.pending_wry_actions.push_back(WryAction::Navigate(parsed));
                            self.status_message = format!("Restored: {}", url);
                        }
                    } else {
                        self.status_message = "No closed tabs to restore".into();
                    }
                } else if let Some((url, _title)) = self.closed_tab_stack.get(n.saturating_sub(1)) {
                    let url_clone = url.clone();
                    if let Ok(parsed) = url::Url::parse(&url_clone) {
                        self.pending_wry_actions.push_back(WryAction::Navigate(parsed));
                        self.status_message = format!("Restored: {}", url_clone);
                    }
                } else {
                    self.status_message = format!("No closed tab at index {}", n);
                }
                return;
            }
        }

        // Help panel
        if query == "help" || query == "?" {
            self.help_panel_open = true;
            return;
        }

        if query == "tab-restore" {
            // :tab-restore (no arg) = restore most recent
            if let Some((url, _title)) = self.closed_tab_stack.pop() {
                if let Ok(parsed) = url::Url::parse(&url) {
                    self.pending_wry_actions.push_back(WryAction::Navigate(parsed));
                    self.status_message = format!("Restored: {}", url);
                }
            } else {
                self.status_message = "No closed tabs to restore".into();
            }
            return;
        }

        // Tab unload: close least-recently-focused background pane
        if query == "tab-unload" {
            if let Some(lru_id) = self.find_lru_pane() {
                // Save to closed tab stack for possible restore
                let panes = self.wm.panes();
                if let Some((_, _)) = panes.iter().find(|(id, _)| *id == lru_id) {
                    // We can't easily get URL/title from BspTree here,
                    // but main.rs captures it before removing
                    self.pending_tab_close = Some(lru_id);
                    self.status_message = "Unloading least-recently-used pane".into();
                }
            } else {
                self.status_message = "Only one pane open, nothing to unload".into();
            }
            return;
        }

        // Bookmarks command
        if self.cmd_bookmarks(query).is_some() { return; }

        // Reader mode command
        if query == "reader" {
            let reader_css = r#"
                (function() {
                    if (document.getElementById('__aileron_reader')) {
                        document.getElementById('__aileron_reader').remove();
                        return;
                    }
                    var style = document.createElement('style');
                    style.id = '__aileron_reader';
                    style.textContent = `
                        body { background: #1a1a2e !important; color: #e0e0e0 !important; }
                        body * { background: transparent !important; color: #e0e0e0 !important;
                                 font-family: Georgia, 'Times New Roman', serif !important;
                                 line-height: 1.7 !important; }
                        article, main, .content, .post, .entry { max-width: 680px !important;
                            margin: 40px auto !important; padding: 0 20px !important; }
                        nav, header, footer, aside, .sidebar, .ad, .advertisement,
                        .social-share, .comments, .related, .newsletter, .popup,
                        [class*="ad-"], [class*="sidebar"], [id*="sidebar"] {
                            display: none !important; }
                        img { max-width: 100% !important; height: auto !important; margin: 1em 0 !important; }
                        a { color: #7eb8f7 !important; }
                        pre, code { background: #2a2a3e !important; color: #c0c0c0 !important;
                                    padding: 2px 6px !important; border-radius: 3px !important; }
                        blockquote { border-left: 3px solid #4db4ff !important; padding-left: 1em !important;
                                    color: #b0b0b0 !important; }
                    `;
                    document.head.appendChild(style);
                    window.ipc.postMessage(JSON.stringify({t:'reader-toggled', enabled: true}));
                })()
            "#;
            self.pending_wry_actions.push_back(WryAction::RunJs(reader_css.into()));
            self.status_message = "Reader mode toggled".into();
            return;
        }

        // Crash recovery: reload the pane that last crashed
        if query == "crash-reload" {
            if let Some(url) = self.crashed_pane_url.take() {
                self.webview_crash_detected = false;
                if let Ok(parsed) = url::Url::parse(&url) {
                    self.pending_wry_actions.push_back(WryAction::Navigate(parsed));
                    self.status_message = format!("Reloaded crashed pane: {}", url);
                }
            } else {
                self.status_message = "No crash to recover from".into();
            }
            return;
        }

        if query == "import-firefox" {
            if let Some(db) = self.db.as_ref() {
                self.status_message = super::cmd::import::import_firefox(db);
            } else {
                self.status_message = "No database connection".into();
            }
            return;
        }

        if query == "import-chrome" {
            if let Some(db) = self.db.as_ref() {
                self.status_message = super::cmd::import::import_chrome(db);
            } else {
                self.status_message = "No database connection".into();
            }
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
            let shell_cmd = crate::platform::platform().shell_command(cmd);
            let shell = &shell_cmd[0];
            let args = &shell_cmd[1..];
            match std::process::Command::new(shell).args(args).output() {
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
                self.status_message = super::cmd::settings::apply_set_setting(&mut self.config, key, value);
            }
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

        // Cookie/site-settings commands
        if self.cmd_site_settings(query).is_some() { return; }

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
            if let Some(ref conn) = self.db
                && let Err(e) = crate::db::quickmarks::set_quickmark(conn, letter, rest)
            {
                tracing::warn!("Failed to persist quickmark {}: {}", letter, e);
            }
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
            self.status_message = super::cmd::sync::execute_sync_push(
                &self.config.sync_target,
                self.config.sync_encrypted,
            );
            return;
        }
        if query == "sync --pull" {
            self.status_message = super::cmd::sync::execute_sync_pull(
                &self.config.sync_target,
                self.config.sync_encrypted,
            );
            return;
        }
        if query == "sync --both" {
            self.status_message = super::cmd::sync::execute_sync_push(
                &self.config.sync_target,
                self.config.sync_encrypted,
            );
            let pull_msg = super::cmd::sync::execute_sync_pull(
                &self.config.sync_target,
                self.config.sync_encrypted,
            );
            self.status_message = format!("{} | {}", self.status_message, pull_msg);
            return;
        }
        if query == "sync --status" {
            self.status_message = super::cmd::sync::execute_sync_status(
                &self.config.sync_target,
                self.config.sync_encrypted,
                self.sync_watcher.is_running(),
            );
            return;
        }
        if query == "sync-watch" {
            if let Err(e) = super::cmd::sync::execute_sync_watch(&self.config.sync_target) {
                self.status_message = e;
            } else {
                let config_dir = crate::config::Config::config_dir();
                match self.sync_watcher.start(&config_dir) {
                    Ok(()) => self.status_message = "Sync watcher started".into(),
                    Err(e) => self.status_message = format!("Failed to start watcher: {}", e),
                }
            }
            return;
        }
        if query == "sync-stop" {
            self.sync_watcher.stop();
            self.status_message = "Sync watcher stopped".into();
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
        if super::cmd::util::looks_like_url(query) {
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
                "q", "quit", "open", "help", "?", "ssh", "set", "vs", "sp", "files", "browse",
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
                "arp-start", "arp-stop", "arp-status", "arp-token",
                "history", "history-clear",
                "tabs", "tab-restore", "tab-unload",
                "bookmarks", "bookmark",
                "reader",
                "crash-reload",
                "sync", "sync --pull", "sync --both", "sync --status",
                "sync-watch", "sync-stop", "sync-target",
            ];
            let cmd = query;
            let suggestion = known_commands
                .iter()
                .filter(|c| c.contains(cmd) || cmd.contains(*c))
                .min_by_key(|c| super::cmd::util::levenshtein_distance(cmd, c));
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

    /// Call a registered Lua custom command by name.
    pub fn call_lua_command(&self, name: &str) -> anyhow::Result<String> {
        if let Some(ref engine) = self.lua_engine {
            engine.call_command(name, &[])
        } else {
            anyhow::bail!("Lua engine not initialized")
        }
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

    // ── Extracted command handlers ──────────────────────────────────────
    // Each returns Some(()) if handled (caller should return),
    // or None if the query doesn't match this handler group.

    /// Handle Bitwarden password manager commands.
    fn cmd_bitwarden(&mut self, query: &str) -> Option<()> {
        if let Some(rest) = query.strip_prefix("bw-unlock ") {
            let password = rest.trim();
            if password.is_empty() {
                self.status_message = "Usage: bw-unlock <password>".into();
                return Some(());
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
            return Some(());
        }

        if let Some(search_query) = query.strip_prefix("bw-search ") {
            let search_query = search_query.trim();
            if search_query.is_empty() {
                self.status_message = "Usage: bw-search <query>".into();
                return Some(());
            }
            if !self.bitwarden.is_unlocked() {
                self.status_message = "Vault is locked. Use bw-unlock <password> first.".into();
                return Some(());
            }
            match self.bitwarden.search(search_query) {
                Ok(items) => {
                    if items.is_empty() {
                        self.status_message = format!("No vault items matching '{}'", search_query);
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
                        self.palette.add_items(credential_items);
                        self.status_message = format!(
                            "Found {} vault items for '{}'. Open palette to select.",
                            items.len(),
                            search_query
                        );
                        self.palette.open();
                        self.command_palette_input.clear();
                        self.palette.update_query("");
                    }
                }
                Err(e) => {
                    self.status_message = format!("Vault search failed: {}", e);
                    warn!("Bitwarden search failed: {}", e);
                }
            }
            return Some(());
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
            return Some(());
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
            return Some(());
        }

        if query == "bw-detect" {
            self.pending_wry_actions.push_back(WryAction::RunJs(
                BitwardenClient::detect_login_forms_js().into(),
            ));
            self.status_message = "Detecting login forms...".into();
            return Some(());
        }

        if query == "keyring-test" {
            if crate::passwords::keyring::is_available() {
                self.status_message = "System keyring: available".into();
            } else {
                self.status_message = "System keyring: not available".into();
            }
            return Some(());
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
            return Some(());
        }

        None
    }

    /// Handle download commands: downloads, downloads-clear, downloads-open, downloads-dir.
    fn cmd_downloads(&mut self, query: &str) -> Option<()> {
        if query == "downloads" {
            let progress = self.download_manager.progress_all();
            if progress.is_empty() {
                if let Some(db) = self.db.as_ref() {
                    match crate::db::downloads::recent_downloads(db, 10) {
                        Ok(entries) => {
                            if entries.is_empty() {
                                self.status_message = "No downloads".into();
                            } else {
                                let items: Vec<String> = entries.iter().map(|e| {
                                    format!("{} [{}]", e.filename, e.status)
                                }).collect();
                                self.status_message = format!("Downloads: {}", items.join(", "));
                            }
                        }
                        Err(e) => self.status_message = format!("Error: {}", e),
                    }
                }
            } else {
                let items: Vec<String> = progress.iter().map(|p| {
                    let size_str = if p.total_bytes > 0 {
                        format!("{}/{}", DownloadProgress::format_bytes(p.received_bytes), DownloadProgress::format_bytes(p.total_bytes))
                    } else {
                        DownloadProgress::format_bytes(p.received_bytes)
                    };
                    format!("{} [{} {}]", p.filename, p.state, size_str)
                }).collect();
                let active = self.download_manager.active_count();
                self.status_message = format!("Downloads ({} active): {}", active, items.join(" | "));
            }
            return Some(());
        }
        if query == "downloads-clear" {
            if let Some(db) = self.db.as_ref() {
                match crate::db::downloads::clear_downloads(db) {
                    Ok(count) => self.status_message = format!("Cleared {} downloads", count),
                    Err(e) => self.status_message = format!("Error: {}", e),
                }
            }
            return Some(());
        }
        if let Some(id_str) = query.strip_prefix("downloads-open ") {
            let id_str = id_str.trim();
            if id_str.is_empty() {
                if let Some(db) = self.db.as_ref() {
                    match crate::db::downloads::get_latest_download_id(db) {
                        Ok(id) => {
                            match crate::db::downloads::get_download_dest_path(db, id) {
                                Ok(dest) => {
                                    let _ = open_that(&dest);
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
                            let _ = open_that(&dest);
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
            return Some(());
        }
        if query == "downloads-dir" {
            if let Some(downloads_dir) = directories::UserDirs::new()
                .and_then(|d| d.download_dir().map(|p| p.to_path_buf()))
            {
                let _ = open_that(&downloads_dir);
                self.status_message = format!("Opened: {}", downloads_dir.display());
            } else {
                self.status_message = "Could not determine downloads directory".into();
            }
            return Some(());
        }
        None
    }

    /// Handle history commands: history, history-clear.
    fn cmd_history(&mut self, query: &str) -> Option<()> {
        match query {
            "history" => {
                // Toggle history panel open/closed
                if self.history_panel_open {
                    self.history_panel_open = false;
                    self.history_entries.clear();
                } else if let Some(db) = self.db.as_ref() {
                    match crate::db::history::recent_entries(db, 100) {
                        Ok(entries) => {
                            self.history_entries = entries;
                            self.history_selected = 0;
                            self.history_panel_open = true;
                        }
                        Err(e) => {
                            self.status_message = format!("History error: {}", e);
                        }
                    }
                }
                Some(())
            }
            "history-clear" => {
                if let Some(db) = self.db.as_ref() {
                    match crate::db::history::clear_history(db) {
                        Ok(count) => {
                            self.status_message = format!("Cleared {} history entries", count);
                            self.history_panel_open = false;
                            self.history_entries.clear();
                        }
                        Err(e) => {
                            self.status_message = format!("Failed to clear history: {}", e);
                        }
                    }
                }
                Some(())
            }
            _ => None,
        }
    }

    /// Handle bookmark commands: bookmarks (panel), bookmark <url> (add), bookmark-clear.
    fn cmd_bookmarks(&mut self, query: &str) -> Option<()> {
        match query {
            "bookmarks" => {
                // Toggle bookmarks panel
                if self.bookmarks_panel_open {
                    self.bookmarks_panel_open = false;
                    self.bookmarks_entries.clear();
                } else if let Some(db) = self.db.as_ref() {
                    match crate::db::bookmarks::all_bookmarks(db) {
                        Ok(entries) => {
                            self.bookmarks_entries = entries;
                            self.bookmarks_selected = 0;
                            self.bookmarks_panel_open = true;
                        }
                        Err(e) => {
                            self.status_message = format!("Bookmarks error: {}", e);
                        }
                    }
                }
                Some(())
            }
            "bookmark-clear" => {
                if let Some(db) = self.db.as_ref() {
                    match crate::db::bookmarks::clear_bookmarks(db) {
                        Ok(count) => {
                            self.status_message = format!("Cleared {} bookmarks", count);
                            self.bookmarks_panel_open = false;
                            self.bookmarks_entries.clear();
                        }
                        Err(e) => {
                            self.status_message = format!("Failed to clear bookmarks: {}", e);
                        }
                    }
                }
                Some(())
            }
            _ => {
                // :bookmark <url> — bookmark current page or specified URL
                if let Some(url_str) = query.strip_prefix("bookmark ") {
                    let url_to_save = if url_str.trim().is_empty() {
                        // Bookmark current active tab
                        None // will be filled below
                    } else {
                        Some(url_str.trim().to_string())
                    };

                    if let Some(db) = self.db.as_ref() {
                        let (url, title) = if let Some(u) = url_to_save {
                            (u, String::new())
                        } else {
                            // Need active tab URL — but we don't have wry_panes here
                            // Use a pending action approach instead
                            self.status_message = "Usage: :bookmark <url>".into();
                            return Some(());
                        };

                        match crate::db::bookmarks::add_bookmark(db, &url, &title) {
                            Ok(id) => {
                                self.status_message = format!("Bookmarked: {} (id={})", url, id);
                            }
                            Err(e) => {
                                self.status_message = format!("Bookmark failed: {}", e);
                            }
                        }
                    }
                    return Some(());
                }
                None
            }
        }
    }

    /// Handle clear/privacy commands: clear <history|bookmarks|workspaces|cookies|all>,
    /// privacy, https-toggle, cookies-clear.
    fn cmd_clear_privacy(&mut self, query: &str) -> Option<()> {
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
            return Some(());
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
            return Some(());
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
            return Some(());
        }

        if query == "cookies-clear" {
            self.pending_wry_actions.push_back(WryAction::RunJs(
                "document.cookie.split(';').forEach(function(c) { document.cookie = c.trim().split('=')[0] + '=;expires=Thu, 01 Jan 1970 00:00:00 GMT;path=/'; }); 'Cookies cleared'".into(),
            ));
            self.status_message = "Cookies cleared for current pane".into();
            return Some(());
        }

        None
    }

    /// Handle site settings commands: site-settings, site-settings set/list/delete/clear,
    /// cookies, cookies-block, cookies-allow.
    fn cmd_site_settings(&mut self, query: &str) -> Option<()> {
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
            return Some(());
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
            return Some(());
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
            return Some(());
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
            return Some(());
        }

        if let Some(domain) = query.strip_prefix("site-settings clear ") {
            let domain = domain.trim();
            if domain.is_empty() {
                self.status_message = "Usage: :site-settings clear <domain>".into();
                return Some(());
            }
            if let Some(db) = self.db.as_ref() {
                match crate::db::site_settings::delete_site_settings_for_domain(db, domain) {
                    Ok(count) => self.status_message = format!("Cleared {} setting(s) for {}", count, domain),
                    Err(e) => self.status_message = format!("Failed: {}", e),
                }
            }
            return Some(());
        }

        // Cookie management
        if query == "cookies" {
            self.pending_wry_actions.push_back(WryAction::RunJs(
                "document.cookie || '(no cookies for this site)'".into(),
            ));
            self.status_message = "Showing cookies...".into();
            return Some(());
        }
        if let Some(domain) = query.strip_prefix("cookies-block ") {
            let domain = domain.trim();
            if domain.is_empty() {
                self.status_message = "Usage: :cookies-block <domain>".into();
                return Some(());
            }
            if let Some(db) = self.db.as_ref() {
                match crate::db::site_settings::set_site_field(db, domain, "exact", "cookies", Some("off")) {
                    Ok(()) => self.status_message = format!("Cookies blocked for {}", domain),
                    Err(e) => self.status_message = format!("Failed: {}", e),
                }
            }
            return Some(());
        }
        if let Some(domain) = query.strip_prefix("cookies-allow ") {
            let domain = domain.trim();
            if domain.is_empty() {
                self.status_message = "Usage: :cookies-allow <domain>".into();
                return Some(());
            }
            if let Some(db) = self.db.as_ref() {
                match crate::db::site_settings::set_site_field(db, domain, "exact", "cookies", Some("on")) {
                    Ok(()) => self.status_message = format!("Cookies allowed for {}", domain),
                    Err(e) => self.status_message = format!("Failed: {}", e),
                }
            }
            return Some(());
        }

        None
    }

    /// Handle git/terminal/tools commands: grep, git-status, git-log, git-diff,
    /// terminal-clear, terminal-search, print.
    fn cmd_tools(&mut self, query: &str) -> Option<()> {
        if let Some(pattern) = query.strip_prefix("grep ") {
            let pattern = pattern.trim();
            if pattern.is_empty() {
                self.status_message = "Usage: :grep <pattern> [path]".into();
                return Some(());
            }
            let (pattern, search_path) = if pattern.contains(' ') {
                let mut parts = pattern.splitn(2, ' ');
                (parts.next().unwrap_or(""), parts.next().unwrap_or("."))
            } else {
                (pattern, ".")
            };

            let output = if std::path::PathBuf::from("/usr/bin/rg").exists()
                || std::path::PathBuf::from("/usr/local/bin/rg").exists()
            {
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
            return Some(());
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
            return Some(());
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
            return Some(());
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
            return Some(());
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
            return Some(());
        }

        if let Some(pattern) = query.strip_prefix("terminal-search ") {
            let pattern = pattern.trim();
            if pattern.is_empty() {
                self.status_message = "Usage: :terminal-search <pattern>".into();
                return Some(());
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
            return Some(());
        }

        if query == "print" {
            self.pending_wry_actions.push_back(WryAction::Print);
            self.status_message = "Printing...".into();
            return Some(());
        }

        None
    }

    /// Handle extension and language commands.
    fn cmd_extensions_language(&mut self, query: &str) -> Option<()> {
        if query == "extensions" {
            let mgr = self.extension_manager.lock().ok()?;
            let ids = mgr.list();
            if ids.is_empty() {
                self.status_message = "No extensions loaded. Use :extension-load to scan, or :extension-install <path> to install.".into();
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
                self.status_message = format!(
                    "Extensions ({}): {}",
                    ids.len(),
                    lines.join(" | ")
                );
            }
            return Some(());
        }

        if query == "extension-load" {
            let loaded = self
                .extension_manager
                .lock()
                .map(|mut m| m.load_all())
                .unwrap_or_default();
            self.status_message = format!("Loaded {} extension(s)", loaded.len());
            return Some(());
        }

        if query == "extension-open" {
            let dir = self
                .extension_manager
                .lock()
                .map(|m| m.extensions_dir().to_path_buf())
                .ok();
            if let Some(dir) = dir {
            if dir.exists() {
                let dir_str = dir.display().to_string();
                let _ = crate::platform::platform().shell_command(&format!(
                    "xdg-open \"{}\" 2>/dev/null || open \"{}\" 2>/dev/null || explorer.exe \"{}\"",
                    dir_str, dir_str, dir_str,
                ));
                self.status_message = format!("Opened {}", dir.display());
                } else {
                    self.status_message = "Extensions directory does not exist yet".into();
                }
            }
            return Some(());
        }

        if let Some(id_str) = query.strip_prefix("extension-disable ") {
            let id_str = id_str.trim();
            if id_str.is_empty() {
                self.status_message = "Usage: extension-disable <id>".into();
                return Some(());
            }
            let ext_id = ExtensionId(id_str.to_string());
            match self
                .extension_manager
                .lock()
                .ok()
                .and_then(|mut m| m.unload(&ext_id))
            {
                Some(name) => {
                    self.status_message = format!("Disabled extension '{}' ({})", name, id_str);
                }
                None => {
                    self.status_message = format!("Extension '{}' not found", id_str);
                }
            }
            return Some(());
        }

        if let Some(path_str) = query.strip_prefix("extension-install ") {
            let path_str = path_str.trim();
            if path_str.is_empty() {
                self.status_message = "Usage: extension-install <path-to-extension-dir>".into();
                return Some(());
            }
            let path = std::path::PathBuf::from(path_str);
            let manifest_path = if path.is_dir() {
                path.join("manifest.json")
            } else if path.ends_with("manifest.json") {
                path
            } else {
                self.status_message = "Path must be a directory containing manifest.json".into();
                return Some(());
            };

            if !manifest_path.exists() {
                self.status_message = format!("No manifest.json found at {}", manifest_path.display());
                return Some(());
            }

            match self
                .extension_manager
                .lock()
                .ok()
                .and_then(|mut m| m.load_extension_from_path(&manifest_path).ok())
            {
                Some(id) => {
                    self.status_message = format!("Installed extension '{}' from {}", id.0, path_str);
                }
                None => {
                    self.status_message = format!("Failed to load extension from {}", path_str);
                }
            }
            return Some(());
        }

        if let Some(id_str) = query.strip_prefix("extension-info ") {
            let id_str = id_str.trim();
            if id_str.is_empty() {
                self.status_message = "Usage: extension-info <id>".into();
                return Some(());
            }
            let ext_id = ExtensionId(id_str.to_string());
            match self.extension_manager.lock().ok().and_then(|m| {
                m.get(&ext_id).map(|api| {
                    (
                        api.manifest().name.clone(),
                        api.manifest().version.clone(),
                        api.extension_id().0.clone(),
                        api.manifest().permissions.clone(),
                    )
                })
            }) {
                Some((name, version, id, permissions)) => {
                    let perms = if permissions.is_empty() {
                        String::new()
                    } else {
                        format!(" | perms: {}", permissions.join(", "))
                    };
                    self.status_message = format!(
                        "{} v{} ({}){}",
                        name, version, id, perms,
                    );
                }
                None => {
                    self.status_message = format!("Extension '{}' not found", id_str);
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
                        format!("{}*", name)
                    } else {
                        name.to_string()
                    }
                })
                .collect();
            self.status_message = format!("Languages: {}", items.join(", "));
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
            return Some(());
        }

        None
    }

    /// Handle ARP (Aileron Remote Protocol) commands.
    fn cmd_arp(&mut self, query: &str) -> Option<()> {
        if query == "arp-start" {
            if let Some(ref server) = self.arp_server
                && server.is_running()
            {
                self.status_message = format!(
                    "ARP server already running on ws://{}:{}",
                    server.host(),
                    server.port(),
                );
                return Some(());
            }
            let config = crate::arp::ArpConfig {
                port: self.config.arp_port.unwrap_or(19743),
                token: self.config.arp_token.clone(),
                ..Default::default()
            };
            match crate::arp::ArpServer::new(config) {
                Ok((server, receiver)) => {
                    match server.start() {
                        Ok(()) => {
                            self.status_message = format!(
                                "ARP server started on ws://127.0.0.1:{}",
                                server.port(),
                            );
                            self.arp_server = Some(server);
                            self.arp_cmd_receiver = Some(std::sync::Mutex::new(receiver));
                        }
                        Err(e) => {
                            self.status_message = format!("ARP server start failed: {}", e);
                        }
                    }
                }
                Err(e) => {
                    self.status_message = format!("ARP server creation failed: {}", e);
                }
            }
            return Some(());
        }

        if query == "arp-stop" {
            if let Some(ref server) = self.arp_server {
                server.stop();
                self.status_message = "ARP server stopped".into();
            } else {
                self.status_message = "ARP server is not running".into();
            }
            return Some(());
        }

        if query == "arp-status" {
            match self.arp_server {
                Some(ref server) => {
                    let state = if server.is_running() { "running" } else { "stopped" };
                    self.status_message = format!(
                        "ARP server: {} on ws://127.0.0.1:{}",
                        state,
                        server.port(),
                    );
                }
                None => {
                    self.status_message = "ARP server: not created (use :arp-start)".into();
                }
            }
            return Some(());
        }

        if query == "arp-token" {
            let token = uuid::Uuid::new_v4().to_string().replace('-', "");
            self.status_message = format!("Generated ARP token: {}", token);
            self.config.arp_token = Some(token);
            return Some(());
        }

        None
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
        assert!(state.should_quit);
    }

    #[test]
    fn test_q_alias() {
        let mut state = make_state();
        state.handle_raw_command("q");
        assert!(state.should_quit);
    }

    #[test]
    fn test_set_command() {
        let mut state = make_state();
        state.handle_raw_command("set adblock off");
        assert!(!state.config.adblock_enabled);
        assert!(state.status_message.contains("adblock"));
    }

    #[test]
    fn test_config_save() {
        let mut state = make_state();
        state.handle_raw_command("config-save");
        assert!(!state.pending_wry_actions.is_empty());
        matches!(state.pending_wry_actions.front(), Some(WryAction::SaveConfig));
    }

    #[test]
    fn test_inspect_command() {
        let mut state = make_state();
        state.handle_raw_command("inspect");
        assert!(!state.pending_wry_actions.is_empty());
        matches!(state.pending_wry_actions.front(), Some(WryAction::ToggleDevTools));
    }

    #[test]
    fn test_back_forward_reload() {
        let mut state = make_state();
        state.handle_raw_command("back");
        assert!(state.pending_wry_actions.iter().any(|a| matches!(a, WryAction::Back)));
        state.pending_wry_actions.clear();

        state.handle_raw_command("fw");
        assert!(state.pending_wry_actions.iter().any(|a| matches!(a, WryAction::Forward)));
        state.pending_wry_actions.clear();

        state.handle_raw_command("reload");
        assert!(state.pending_wry_actions.iter().any(|a| matches!(a, WryAction::Reload)));
    }

    #[test]
    fn test_unknown_command_suggests() {
        let mut state = make_state();
        state.handle_raw_command("quitt");
        assert!(state.status_message.contains("did you mean"));
    }

    #[test]
    fn test_url_navigation() {
        let mut state = make_state();
        state.handle_raw_command("https://example.com");
        assert!(state.pending_wry_actions.iter().any(|a| matches!(a, WryAction::Navigate(_))));
        assert!(state.status_message.contains("Navigating"));
    }

    #[test]
    fn test_bare_domain_navigation() {
        let mut state = make_state();
        state.handle_raw_command("example.com");
        assert!(state.pending_wry_actions.iter().any(|a| matches!(a, WryAction::Navigate(_))));
    }

    #[test]
    fn test_open_command() {
        let mut state = make_state();
        state.handle_raw_command("open https://example.com");
        assert!(state.pending_wry_actions.iter().any(|a| matches!(a, WryAction::Navigate(_))));
    }

    #[test]
    fn test_open_command_invalid() {
        let mut state = make_state();
        state.handle_raw_command("open :::invalid");
        assert!(state.status_message.contains("Invalid URL"));
    }

    #[test]
    fn test_shell_command() {
        let mut state = make_state();
        state.handle_raw_command("!echo hello");
        assert!(state.status_message.contains("hello") || state.status_message.contains("echo"));
    }

    #[test]
    fn test_print_command() {
        let mut state = make_state();
        state.handle_raw_command("print");
        assert!(state.pending_wry_actions.iter().any(|a| matches!(a, WryAction::Print)));
        assert!(state.status_message.contains("Printing"));
    }

    #[test]
    fn test_theme_command() {
        let mut state = make_state();
        state.handle_raw_command("theme");
        assert!(state.status_message.contains("Theme:"));
    }

    #[test]
    fn test_mute_unmute() {
        let mut state = make_state();
        state.handle_raw_command("mute");
        assert!(!state.muted_pane_ids.is_empty());
        state.handle_raw_command("unmute");
        assert!(state.muted_pane_ids.is_empty());
    }

    #[test]
    fn test_chain_commands() {
        let mut state = make_state();
        state.handle_raw_command("print && print");
        // Should have queued two print actions
        let print_count = state.pending_wry_actions.iter().filter(|a| matches!(a, WryAction::Print)).count();
        assert_eq!(print_count, 2);
    }

    #[test]
    fn test_privacy_command() {
        let mut state = make_state();
        state.handle_raw_command("privacy");
        assert!(state.status_message.contains("HTTPS upgrade"));
        assert!(state.status_message.contains("Tracking protection"));
        assert!(state.status_message.contains("Adblock"));
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
        assert!(state.status_message.contains("disabled"));
    }

    #[test]
    fn test_extensions_list() {
        let mut state = make_state();
        state.handle_raw_command("extensions");
        assert!(state.status_message.contains("No extensions") || state.status_message.contains("Extensions:"));
    }

    #[test]
    fn test_cookies_clear() {
        let mut state = make_state();
        state.handle_raw_command("cookies-clear");
        assert!(state.status_message.contains("Cookies cleared"));
    }

    #[test]
    fn test_keyring_test() {
        let mut state = make_state();
        state.handle_raw_command("keyring-test");
        assert!(state.status_message.contains("keyring"));
    }

    #[test]
    fn test_memory_command() {
        let mut state = make_state();
        state.handle_raw_command("memory");
        assert!(state.status_message.contains("RSS"));
    }

    #[test]
    fn test_history_toggle() {
        let mut state = make_state();
        // History panel opens (entries may be empty without DB)
        state.handle_raw_command("history");
        assert!(state.history_panel_open);
        // Toggle off
        state.handle_raw_command("history");
        assert!(!state.history_panel_open);
    }

    #[test]
    fn test_history_clear() {
        let mut state = make_state();
        state.handle_raw_command("history-clear");
        // Should not panic, message indicates action taken
        assert!(!state.status_message.is_empty());
    }

    #[test]
    fn test_tabs_toggle() {
        let mut state = make_state();
        state.handle_raw_command("tabs");
        assert!(state.tab_search_open);
        state.handle_raw_command("tabs");
        assert!(!state.tab_search_open);
    }
}
