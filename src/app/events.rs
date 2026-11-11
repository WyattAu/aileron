use tracing::info;

use crate::input::{EventDestination, Key, KeyEvent, Mode};
use crate::ui::palette::PaletteAction;

use super::AppState;
use super::WryAction;
use super::dispatch::ActionEffect;

impl AppState {
    pub fn process_key_event(&mut self, event: KeyEvent) {
        // Escape closes find bar first (highest priority)
        if self.core.ui.find_bar_open && event.key == Key::Escape {
            self.core.ui.find_bar_open = false;
            self.core.ui.find_query.clear();
            self.core.pending_wry_actions.push_back(WryAction::RunJs(
                "window.getSelection().removeAllRanges()".into(),
            ));
            return;
        }

        // History panel: j/k and arrow navigation
        if self.headed().panels.history_panel_open {
            match &event.key {
                Key::Character('j') | Key::Down => {
                    if !self.headed().panels.history_entries.is_empty() {
                        self.headed_mut().panels.history_selected =
                            (self.headed_mut().panels.history_selected + 1)
                                .min(self.headed().panels.history_entries.len() - 1);
                    }
                    return;
                }
                Key::Character('k') | Key::Up => {
                    self.headed_mut().panels.history_selected =
                        self.headed_mut().panels.history_selected.saturating_sub(1);
                    return;
                }
                Key::Enter => {
                    if let Some(entry) = self
                        .headed()
                        .panels
                        .history_entries
                        .get(self.headed().panels.history_selected)
                        && let Ok(url) = url::Url::parse(&entry.url)
                    {
                        self.core
                            .pending_wry_actions
                            .push_back(WryAction::Navigate(url));
                    }
                    self.headed_mut().panels.history_panel_open = false;
                    self.headed_mut().panels.history_entries.clear();
                    return;
                }
                // Escape handled in main.rs
                _ => {}
            }
        }

        // Tab search panel: j/k navigation (only when TextEdit not focused)
        if self.headed().panels.tab_search_open {
            match &event.key {
                Key::Down => {
                    self.headed_mut().panels.tab_search_selected =
                        self.headed().panels.tab_search_selected.saturating_sub(1);
                    return;
                }
                Key::Up => {
                    self.headed_mut().panels.tab_search_selected =
                        self.headed().panels.tab_search_selected.saturating_sub(1);
                    return;
                }
                Key::Enter => {
                    let ids: Vec<_> = self.core.wm.panes_ref().iter().map(|(id, _)| *id).collect();
                    if let Some(id) = ids.get(self.headed().panels.tab_search_selected) {
                        self.core.wm.set_active_pane(*id);
                    }
                    self.headed_mut().panels.tab_search_open = false;
                    return;
                }
                // Escape handled in main.rs
                _ => {}
            }
        }

        // Bookmarks panel: j/k navigation
        if self.headed().panels.bookmarks_panel_open {
            match &event.key {
                Key::Character('j') | Key::Down => {
                    if !self.headed().panels.bookmarks_entries.is_empty() {
                        self.headed_mut().panels.bookmarks_selected =
                            (self.headed_mut().panels.bookmarks_selected + 1)
                                .min(self.headed().panels.bookmarks_entries.len() - 1);
                    }
                    return;
                }
                Key::Character('k') | Key::Up => {
                    self.headed_mut().panels.bookmarks_selected =
                        self.headed().panels.bookmarks_selected.saturating_sub(1);
                    return;
                }
                Key::Enter => {
                    if let Some(bm) = self
                        .headed()
                        .panels
                        .bookmarks_entries
                        .get(self.headed().panels.bookmarks_selected)
                        && let Ok(url) = url::Url::parse(&bm.url)
                    {
                        self.core
                            .pending_wry_actions
                            .push_back(WryAction::Navigate(url));
                    }
                    self.headed_mut().panels.bookmarks_panel_open = false;
                    self.headed_mut().panels.bookmarks_entries.clear();
                    return;
                }
                Key::Character('d') => {
                    // d to delete selected bookmark
                    if let Some(bm) = self
                        .headed()
                        .panels
                        .bookmarks_entries
                        .get(self.headed().panels.bookmarks_selected)
                    {
                        if let Some(db) = self.core.db.as_ref()
                            && let Err(e) = crate::db::bookmarks::remove_bookmark_by_id(db, bm.id)
                        {
                            tracing::warn!("Failed to remove bookmark by id: {}", e);
                        }
                        let removed_id = bm.id;
                        self.headed_mut()
                            .panels
                            .bookmarks_entries
                            .retain(|b| b.id != removed_id);
                        if self.headed().panels.bookmarks_selected
                            >= self.headed().panels.bookmarks_entries.len()
                        {
                            self.headed_mut().panels.bookmarks_selected = self
                                .headed()
                                .panels
                                .bookmarks_entries
                                .len()
                                .saturating_sub(1);
                        }
                    }
                    return;
                }
                _ => {}
            }
        }

        // Sync conflicts panel: j/k navigation (feature-gated)
        #[cfg(feature = "sync")]
        if self.headed_mut().panels.sync_conflicts_panel_open {
            match &event.key {
                Key::Character('j') | Key::Down => {
                    if !self.headed_mut().panels.sync_conflict_entries.is_empty() {
                        self.headed_mut().panels.sync_conflict_selected =
                            (self.headed_mut().panels.sync_conflict_selected + 1)
                                .min(self.headed_mut().panels.sync_conflict_entries.len() - 1);
                    }
                    return;
                }
                Key::Character('k') | Key::Up => {
                    self.headed_mut().panels.sync_conflict_selected = self
                        .headed_mut()
                        .panels
                        .sync_conflict_selected
                        .saturating_sub(1);
                    return;
                }
                Key::Enter => {
                    // Keep local for selected conflict
                    if !self.headed_mut().panels.sync_conflict_entries.is_empty() {
                        let idx = self.headed_mut().panels.sync_conflict_selected;
                        self.headed_mut().panels.sync_conflict_entries.remove(idx);
                        self.core.ui.status_message = "Kept local version".into();
                        if self.headed_mut().panels.sync_conflict_selected
                            >= self.headed_mut().panels.sync_conflict_entries.len()
                        {
                            self.headed_mut().panels.sync_conflict_selected = self
                                .headed_mut()
                                .panels
                                .sync_conflict_entries
                                .len()
                                .saturating_sub(1);
                        }
                    }
                    return;
                }
                Key::Character('d') => {
                    // Keep remote for selected conflict
                    if !self.headed_mut().panels.sync_conflict_entries.is_empty() {
                        let idx = self.headed_mut().panels.sync_conflict_selected;
                        if let Some(conflict) =
                            self.headed_mut().panels.sync_conflict_entries.get(idx)
                        {
                            self.core.ui.status_message =
                                format!("Keep remote: {} (run :sync --pull)", conflict.path);
                        }
                        self.headed_mut().panels.sync_conflict_entries.remove(idx);
                        if self.headed_mut().panels.sync_conflict_selected
                            >= self.headed_mut().panels.sync_conflict_entries.len()
                        {
                            self.headed_mut().panels.sync_conflict_selected = self
                                .headed_mut()
                                .panels
                                .sync_conflict_entries
                                .len()
                                .saturating_sub(1);
                        }
                    }
                    return;
                }
                _ => {}
            }
        }

        // If palette is open, route input to it
        if self.headed().palette.open {
            let key_str: Option<String> = match &event.key {
                Key::Up => Some("Up".into()),
                Key::Down => Some("Down".into()),
                Key::Enter => Some("Enter".into()),
                Key::Escape => Some("Escape".into()),
                Key::Backspace => Some("Backspace".into()),
                Key::Character(c) => Some(c.to_string()),
                _ => None,
            };

            if let Some(key_str) = key_str {
                let action = self.headed_mut().palette.handle_input(&key_str);
                match action {
                    PaletteAction::ItemSelected(item) => {
                        self.headed_mut().palette.close();
                        self.core.ui.command_palette_input.clear();
                        self.execute_palette_selection(&item);
                    }
                    PaletteAction::Closed => {
                        self.headed_mut().palette.close();
                        self.core.ui.command_palette_input.clear();
                    }
                    PaletteAction::QuerySubmit(query) => {
                        self.headed_mut().palette.close();
                        self.core.ui.command_palette_input.clear();
                        self.handle_raw_command(&query);
                    }
                    PaletteAction::Consumed => {
                        self.core.ui.command_palette_input =
                            self.headed_mut().palette.query.clone();
                    }
                }
            }
            return;
        }

        // Handle pending mark actions (m or ' prefix)
        if let Some(action) = self.core.session.pending_mark_action.take()
            && let Key::Character(c) = &event.key
            && c.is_ascii_lowercase()
        {
            let active_id = self.core.wm.active_pane_id();
            match action {
                's' => {
                    // Store the pending mark letter; JS will send the actual
                    // scroll fraction via IPC, which is handled in frame_tasks.rs.
                    self.core.session.pending_mark_set = Some(*c);
                    self.core
                        .pending_wry_actions
                        .push_back(WryAction::CaptureScrollFraction);
                    self.core.ui.status_message = format!("Mark {c} set");
                }
                'g' => {
                    if let Some(frac) = self
                        .core
                        .session
                        .marks
                        .get(&active_id)
                        .and_then(|m| m.get(c))
                        .copied()
                    {
                        // Set a pending scroll target; the main loop will apply it.
                        self.core.session.pending_mark_jump = Some(frac);
                        self.core.ui.status_message = format!("Mark {c} jumped");
                    } else {
                        self.core.ui.status_message = format!("Mark {c} not set");
                    }
                }
                _ => {}
            }
            return;
        }

        // Check keybindings first
        let action = self
            .headed()
            .keybindings
            .lookup(self.headed().mode, event.modifiers, event.key.clone())
            .cloned();
        if let Some(action) = action {
            self.execute_action(&action);
            return;
        }

        // Mark prefix keys in Normal mode
        if self.headed().mode == Mode::Normal {
            if let Key::Character('m') = &event.key {
                self.core.session.pending_mark_action = Some('s');
                self.core.ui.status_message = "Set mark (press a-z)".into();
                return;
            } else if let Key::Character('\'') = &event.key {
                self.core.session.pending_mark_action = Some('g');
                self.core.ui.status_message = "Go to mark (press a-z)".into();
                return;
            }
        }

        // Check mode transitions
        if let Some(new_mode) = crate::input::mode::transition(self.headed().mode, &event) {
            self.headed_mut().mode = new_mode;
            self.update_status();
            #[cfg(feature = "lua")]
            if let Some(ref engine) = self.lua_engine {
                engine.call_hooks("mode_change", &[self.headed().mode.as_str()]);
            }
            return;
        }

        // Route to destination
        let dest = crate::input::router::route_event(self.headed().mode, &event);
        match dest {
            EventDestination::KeybindingHandler => {}
            EventDestination::Servo => {
                if let Key::Character(c) = &event.key {
                    tracing::debug!("Would send '{}' to Servo", c);
                }
            }
            EventDestination::CommandPalette => {
                if let Key::Character(c) = &event.key {
                    self.core.ui.command_palette_input.push(*c);
                } else if event.key == Key::Backspace {
                    self.core.ui.command_palette_input.pop();
                } else if event.key == Key::Enter {
                    let input = self.core.ui.command_palette_input.clone();
                    self.execute_command(&input);
                    self.headed_mut().palette.close();
                    self.core.ui.command_palette_input.clear();
                }
            }
            EventDestination::Egui => {}
            EventDestination::Discard => {}
        }
    }

    pub(crate) fn execute_action(&mut self, action: &crate::input::Action) {
        self.core.session.session_dirty = true;
        use ActionEffect;

        let effects = super::dispatch::dispatch_action(action);

        for effect in &effects {
            match effect {
                ActionEffect::Wry(wry_action) => {
                    self.core.pending_wry_actions.push_back(wry_action.clone());
                }
                ActionEffect::Status(msg) => {
                    self.core.ui.status_message = msg.clone();
                }
                ActionEffect::SetMode(mode) => {
                    self.headed_mut().mode = *mode;
                    self.update_status();
                    #[cfg(feature = "lua")]
                    if let Some(ref engine) = self.lua_engine {
                        engine.call_hooks("mode_change", &[self.headed().mode.as_str()]);
                    }
                }
                ActionEffect::Quit => {
                    info!("Quit requested");
                    self.core.session.should_quit = true;
                }
                ActionEffect::OpenPalette => {
                    // Refresh items before opening so recent history/bookmarks are current
                    self.refresh_palette_items();
                    self.headed_mut().palette.open();
                    self.core.ui.command_palette_input.clear();
                    self.core.ui.status_message = "Command palette".into();
                }
                ActionEffect::RequestSplit(direction) => {
                    let active = self.core.wm.active_pane_id();
                    let new_url = self
                        .headed_mut()
                        .pending_new_tab_url
                        .take()
                        .unwrap_or_else(|| url::Url::parse("aileron://new").unwrap());
                    let is_private = self.headed().tabs.private_pane_ids.contains(&active);
                    match self.core.wm.split(active, *direction, 0.5) {
                        Ok(new_id) => {
                            self.core.engines.create_pane(new_id, new_url, None);
                            // Propagate private mode to new pane
                            if is_private {
                                self.headed_mut().tabs.private_pane_ids.insert(new_id);
                            }
                            self.core.ui.status_message = "Split vertical".into();
                        }
                        Err(e) => self.core.ui.status_message = format!("Split failed: {e}"),
                    }
                }
                ActionEffect::OpenTerminal => {
                    #[cfg(feature = "terminal")]
                    {
                        let active = self.core.wm.active_pane_id();
                        let term_url = url::Url::parse("aileron://terminal")
                            .expect("'aileron://terminal' is a valid URL");
                        match self
                            .core
                            .wm
                            .split(active, crate::wm::SplitDirection::Vertical, 0.5)
                        {
                            Ok(new_id) => {
                                self.core
                                    .engines
                                    .create_pane(new_id, term_url.clone(), None);
                                self.terminal_pane_ids.insert(new_id);
                                self.core.ui.status_message = "Terminal opened".into();
                            }
                            Err(e) => self.core.ui.status_message = format!("Terminal failed: {e}"),
                        }
                    }
                    #[cfg(not(feature = "terminal"))]
                    {
                        self.core.ui.status_message = "Terminal feature not enabled".into();
                    }
                }
                ActionEffect::RequestClosePane => {
                    let active = self.core.wm.active_pane_id();
                    if self.headed().tabs.pinned_pane_ids.contains(&active) {
                        self.core.ui.status_message =
                            "Cannot close pinned pane (use :pin to unpin)".into();
                        return;
                    }
                    if let Ok(()) = self.core.wm.close(active) {
                        self.core.engines.remove_pane(&active);
                        self.core.ui.status_message = "Pane closed".into();
                    }
                }
                ActionEffect::RequestNavigatePane(direction) => {
                    let current = self.core.wm.active_pane_id();
                    if let Some(id) = self.core.wm.navigate(*direction) {
                        self.headed_mut().tabs.last_active_pane_id = Some(current);
                        self.core.wm.set_active_pane(id);
                        self.update_status();
                        self.headed_mut().autofill.available = false;
                        self.headed_mut().autofill.js = None;
                        self.headed_mut().autofill.username_id.clear();
                        self.headed_mut().autofill.password_id.clear();
                        self.headed_mut().autofill.status_msg.clear();
                    }
                }
                ActionEffect::RequestExternalBrowser => {
                    let active_id = self.core.wm.active_pane_id();
                    if let Some(engine) = self.core.engines.get(&active_id)
                        && let Some(url) = engine.current_url()
                    {
                        match crate::servo::open_in_system_browser(url) {
                            Ok(()) => {
                                self.core.ui.status_message = "Opened in system browser".into();
                            }
                            Err(e) => {
                                self.core.ui.status_message = format!("Failed: {e}");
                            }
                        }
                    }
                }
                ActionEffect::OpenFindBar => {
                    self.core.ui.find_bar_open = true;
                    self.core.ui.find_query.clear();
                    self.core.ui.status_message = "Find: ".into();
                }
                ActionEffect::CloseFindBar => {
                    self.core.ui.find_bar_open = false;
                    self.core.ui.find_query.clear();
                    // Clear highlights in the page
                    self.core.pending_wry_actions.push_back(WryAction::RunJs(
                        "window.getSelection().removeAllRanges()".into(),
                    ));
                }
                ActionEffect::FindInPage { query, forward } => {
                    if !query.is_empty() {
                        let direction = if *forward { "true" } else { "false" };
                        let escaped = query.replace('\\', "\\\\").replace('\'', "\\'");
                        self.core.pending_wry_actions.push_back(WryAction::RunJs(format!(
                            "window.find('{escaped}', false, true, {direction}, false, false, false)"
                        )));
                    }
                }
                ActionEffect::ToggleLinkHints => {
                    self.core.ui.hint_mode = !self.core.ui.hint_mode;
                    self.core.ui.hint_new_tab = false;
                    if self.core.ui.hint_mode {
                        self.core.ui.status_message =
                            "Link hints: type letters, Escape to cancel".into();
                    } else {
                        self.core.ui.status_message.clear();
                    }
                    // Wry(RunJs) effect is also dispatched to inject/remove the CSS
                }
                ActionEffect::FollowLinkNewTab => {
                    self.core.ui.hint_mode = !self.core.ui.hint_mode;
                    self.core.ui.hint_new_tab = self.core.ui.hint_mode;
                    if self.core.ui.hint_mode {
                        self.core.ui.status_message =
                            "Link hints (new tab): type letters, Escape to cancel".into();
                    } else {
                        self.core.ui.status_message.clear();
                    }
                }
                ActionEffect::SaveWorkspace => {
                    // Queue a save action for main.rs to handle.
                    // main.rs has access to WryPaneManager for live URLs.
                    let name =
                        format!("workspace-{}", chrono::Local::now().format("%Y%m%d-%H%M%S"));
                    self.core
                        .pending_wry_actions
                        .push_back(WryAction::SaveWorkspace {
                            name: name.clone(),
                            pane_urls: std::collections::HashMap::new(),
                        });
                    self.core.ui.status_message = format!("Saving workspace: {name}...");
                }
                ActionEffect::CopyUrl => {
                    let active_id = self.core.wm.active_pane_id();
                    if let Some(engine) = self.core.engines.get(&active_id)
                        && let Some(url) = engine.current_url()
                    {
                        let url_str = url.to_string();
                        let copied = crate::platform::platform().clipboard_copy(&url_str);
                        if copied {
                            let display = if url_str.len() > 60 {
                                format!("{}...", &url_str[..57])
                            } else {
                                url_str
                            };
                            self.core.ui.status_message = format!("Copied: {display}");
                        } else {
                            self.core.ui.status_message =
                                "Clipboard: no clipboard tool available".into();
                        }
                    }
                }
                ActionEffect::ResizePane(direction) => {
                    let active = self.core.wm.active_pane_id();
                    let amount = match direction {
                        crate::wm::Direction::Left | crate::wm::Direction::Up => -0.05,
                        crate::wm::Direction::Right | crate::wm::Direction::Down => 0.05,
                    };
                    match self.core.wm.resize_pane(active, amount) {
                        Ok(()) => self.core.ui.status_message = "Pane resized".into(),
                        Err(e) => self.core.ui.status_message = format!("Resize failed: {e}"),
                    }
                }
                ActionEffect::NewWindow => {
                    self.headed_mut().pending_new_window = true;
                    self.core.ui.status_message = "Opening new window...".into();
                }
                ActionEffect::EnterReaderMode => {
                    let active_id = self.core.wm.active_pane_id();
                    if self.headed().tabs.reader_mode_panes.contains(&active_id) {
                        self.headed_mut().tabs.reader_mode_panes.remove(&active_id);
                        self.core
                            .pending_wry_actions
                            .push_back(WryAction::ExitReaderMode);
                        self.core.ui.status_message = "Reader mode off".into();
                    } else {
                        self.headed_mut().tabs.reader_mode_panes.insert(active_id);
                        self.core
                            .pending_wry_actions
                            .push_back(WryAction::EnterReaderMode);
                        self.core.ui.status_message = "Reader mode on".into();
                    }
                }
                ActionEffect::ExitReaderMode => {}
                ActionEffect::EnterMinimalMode => {
                    let active_id = self.core.wm.active_pane_id();
                    if self.headed().tabs.minimal_mode_panes.contains(&active_id) {
                        self.headed_mut().tabs.minimal_mode_panes.remove(&active_id);
                        self.core
                            .pending_wry_actions
                            .push_back(WryAction::ExitMinimalMode);
                        self.core.ui.status_message = "Minimal mode off".into();
                    } else {
                        self.headed_mut().tabs.minimal_mode_panes.insert(active_id);
                        self.core
                            .pending_wry_actions
                            .push_back(WryAction::EnterMinimalMode);
                        self.core.ui.status_message = "Minimal mode on".into();
                    }
                }
                ActionEffect::ExitMinimalMode => {}
                ActionEffect::GetNetworkLog => {
                    self.core
                        .pending_wry_actions
                        .push_back(WryAction::GetNetworkLog);
                }
                ActionEffect::ClearNetworkLog => {}
                ActionEffect::GetConsoleLog => {
                    self.core
                        .pending_wry_actions
                        .push_back(WryAction::GetConsoleLog);
                }
                ActionEffect::ClearConsoleLog => {}
                ActionEffect::DetachPane => {
                    let active_id = self.core.wm.active_pane_id();
                    let url = self
                        .core
                        .engines
                        .get(&active_id)
                        .and_then(|e| e.current_url().cloned());
                    if let Some(url) = url {
                        match self.core.wm.close(active_id) {
                            Ok(()) => {
                                self.core.engines.remove_pane(&active_id);
                                #[cfg(feature = "terminal")]
                                self.terminal_pane_ids.remove(&active_id);
                                self.headed_mut().pending_new_window = true;
                                self.headed_mut().pending_detach_url = Some(url);
                                self.core.ui.status_message = "Detaching pane to popup...".into();
                            }
                            Err(_) => {
                                self.core.ui.status_message = "Cannot detach the only pane".into();
                            }
                        }
                    } else {
                        self.core.ui.status_message = "No URL to detach".into();
                    }
                }
                ActionEffect::CloseOtherPanes => {
                    let active_id = self.core.wm.active_pane_id();
                    let other_ids: Vec<uuid::Uuid> = self
                        .core
                        .wm
                        .panes_ref()
                        .iter()
                        .filter_map(|(id, _)| if *id != active_id { Some(*id) } else { None })
                        .collect();
                    for id in &other_ids {
                        self.core.engines.remove_pane(id);
                        #[cfg(feature = "terminal")]
                        self.terminal_pane_ids.remove(id);
                    }
                    if let Err(e) = self.core.wm.retain_only(active_id) {
                        self.core.ui.status_message = format!("Failed: {e}");
                    } else {
                        self.core.ui.status_message =
                            format!("Closed {} other pane(s)", other_ids.len());
                    }
                }
                ActionEffect::Print => {
                    self.core.pending_wry_actions.push_back(WryAction::Print);
                    self.core.ui.status_message = "Printing...".into();
                }
                ActionEffect::RequestNewTab => {
                    let active_id = self.core.wm.active_pane_id();
                    let new_url = self
                        .headed_mut()
                        .pending_new_tab_url
                        .take()
                        .unwrap_or_else(|| url::Url::parse("aileron://new").unwrap());
                    // Compute everything pane-derived inside the tree borrow,
                    // then mutate headed state after it ends.
                    let pane_info = self
                        .core
                        .wm
                        .root_mut()
                        .and_then(|root| crate::wm::BspTree::find_pane_mut(root, active_id))
                        .map(|pane| {
                            let new_tab_id = pane.tabs.add(new_url.clone());
                            let active_url = pane.tabs.active().url.clone();
                            (
                                new_tab_id,
                                active_url,
                                pane.tabs.active_index() + 1,
                                pane.tabs.len(),
                            )
                        });
                    if let Some((new_tab_id, active_url, idx, total)) = pane_info {
                        self.core.engines.create_pane(new_tab_id, active_url, None);
                        // Navigate the existing webview to the new tab's URL.
                        self.core
                            .pending_wry_actions
                            .push_back(WryAction::Navigate(new_url));
                        self.headed_mut().tabs.tab_display_dirty = true;
                        self.core.ui.status_message = format!("Tab {}/{}", idx, total);
                    }
                }
                ActionEffect::RequestCloseTab => {
                    let active_id = self.core.wm.active_pane_id();
                    if let Some(pane) = self
                        .core
                        .wm
                        .root_mut()
                        .and_then(|root| crate::wm::BspTree::find_pane_mut(root, active_id))
                    {
                        if pane.tabs.is_single() {
                            // Last tab: close the entire pane
                            if let Ok(()) = self.core.wm.close(active_id) {
                                self.core.engines.remove_pane(&active_id);
                                #[cfg(feature = "terminal")]
                                self.terminal_pane_ids.remove(&active_id);
                                self.core.ui.status_message = "Pane closed".into();
                            }
                        } else {
                            let closed = pane.tabs.close_active();
                            if let Some(closed_tab) = closed {
                                let tab_status = format!(
                                    "Tab {}/{}",
                                    pane.tabs.active_index() + 1,
                                    pane.tabs.len()
                                );
                                self.core.engines.remove_pane(&closed_tab.id);
                                self.core.ui.status_message = tab_status;
                            }
                        }
                    }
                }
                ActionEffect::RequestNextTab => {
                    let active_id = self.core.wm.active_pane_id();
                    let nav = self
                        .core
                        .wm
                        .root_mut()
                        .and_then(|root| crate::wm::BspTree::find_pane_mut(root, active_id))
                        .map(|pane| {
                            pane.tabs.next();
                            (
                                pane.tabs.active().url.clone(),
                                pane.tabs.active_index() + 1,
                                pane.tabs.len(),
                            )
                        });
                    if let Some((url, idx, total)) = nav {
                        self.core
                            .pending_wry_actions
                            .push_back(WryAction::Navigate(url));
                        self.core.ui.status_message = format!("Tab {}/{}", idx, total);
                    }
                }
                ActionEffect::RequestPrevTab => {
                    let active_id = self.core.wm.active_pane_id();
                    let nav = self
                        .core
                        .wm
                        .root_mut()
                        .and_then(|root| crate::wm::BspTree::find_pane_mut(root, active_id))
                        .map(|pane| {
                            pane.tabs.prev();
                            (
                                pane.tabs.active().url.clone(),
                                pane.tabs.active_index() + 1,
                                pane.tabs.len(),
                            )
                        });
                    if let Some((url, idx, total)) = nav {
                        self.core
                            .pending_wry_actions
                            .push_back(WryAction::Navigate(url));
                        self.core.ui.status_message = format!("Tab {}/{}", idx, total);
                    }
                }
                ActionEffect::PinPane => {
                    let active_id = self.core.wm.active_pane_id();
                    if self.headed().tabs.pinned_pane_ids.contains(&active_id) {
                        self.headed_mut().tabs.pinned_pane_ids.remove(&active_id);
                        self.core.ui.status_message =
                            crate::i18n::tr(crate::i18n::TrKey("status_unpinned")).into();
                    } else {
                        self.headed_mut().tabs.pinned_pane_ids.insert(active_id);
                        self.core.ui.status_message =
                            crate::i18n::tr(crate::i18n::TrKey("status_pinned")).into();
                    }
                }
            }
        }
    }

    pub fn update_status(&mut self) {
        let mode_str = self.headed().mode.as_str().to_string();
        self.core.ui.status_message = format!("-- {mode_str} --");
        self.core.ui.accessibility_text = format!("Mode: {mode_str}");
    }

    /// Update the accessibility live-region text with a status summary.
    /// Call this when important state changes occur (navigation, error, etc.).
    pub fn update_a11y(&mut self, msg: &str) {
        self.core.ui.status_message = msg.to_string();
        self.core.ui.accessibility_text = msg.to_string();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::input::Mode;
    use crate::wm::Rect;

    fn make_state() -> AppState {
        let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
        AppState::new(viewport, Config::default()).unwrap()
    }

    #[test]
    fn test_update_status_normal() {
        let mut state = make_state();
        state.headed_mut().mode = Mode::Normal;
        state.update_status();
        assert_eq!(state.core.ui.status_message, "-- NORMAL --");
        assert_eq!(state.core.ui.accessibility_text, "Mode: NORMAL");
    }

    #[test]
    fn test_update_status_insert() {
        let mut state = make_state();
        state.headed_mut().mode = Mode::Insert;
        state.update_status();
        assert_eq!(state.core.ui.status_message, "-- INSERT --");
        assert_eq!(state.core.ui.accessibility_text, "Mode: INSERT");
    }

    #[test]
    fn test_update_status_command() {
        let mut state = make_state();
        state.headed_mut().mode = Mode::Command;
        state.update_status();
        assert_eq!(state.core.ui.status_message, "-- COMMAND --");
        assert_eq!(state.core.ui.accessibility_text, "Mode: COMMAND");
    }

    #[test]
    fn test_update_a11y() {
        let mut state = make_state();
        state.update_a11y("Navigation complete");
        assert_eq!(state.core.ui.status_message, "Navigation complete");
        assert_eq!(state.core.ui.accessibility_text, "Navigation complete");
    }

    #[test]
    fn test_update_a11y_overrides_status() {
        let mut state = make_state();
        state.headed_mut().mode = Mode::Normal;
        state.update_status();
        assert_eq!(state.core.ui.status_message, "-- NORMAL --");
        state.update_a11y("Error: page crashed");
        assert_eq!(state.core.ui.status_message, "Error: page crashed");
        assert_eq!(state.core.ui.accessibility_text, "Error: page crashed");
    }

    #[test]
    fn test_update_status_after_a11y() {
        let mut state = make_state();
        state.update_a11y("some message");
        state.headed_mut().mode = Mode::Insert;
        state.update_status();
        assert_eq!(state.core.ui.status_message, "-- INSERT --");
        assert_eq!(state.core.ui.accessibility_text, "Mode: INSERT");
    }
}
