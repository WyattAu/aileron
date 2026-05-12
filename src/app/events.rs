use tracing::info;

use crate::input::{EventDestination, Key, KeyEvent, Mode};
use crate::ui::palette::PaletteAction;

use super::AppState;
use super::WryAction;
use super::dispatch::ActionEffect;

impl AppState {
    pub fn process_key_event(&mut self, event: KeyEvent) {
        // History panel: j/k and arrow navigation
        if self.panels.history_panel_open {
            match &event.key {
                Key::Character('j') | Key::Down => {
                    if !self.panels.history_entries.is_empty() {
                        self.panels.history_selected = (self.panels.history_selected + 1)
                            .min(self.panels.history_entries.len() - 1);
                    }
                    return;
                }
                Key::Character('k') | Key::Up => {
                    self.panels.history_selected = self.panels.history_selected.saturating_sub(1);
                    return;
                }
                Key::Enter => {
                    if let Some(entry) = self
                        .panels
                        .history_entries
                        .get(self.panels.history_selected)
                        && let Ok(url) = url::Url::parse(&entry.url)
                    {
                        self.pending_wry_actions.push_back(WryAction::Navigate(url));
                    }
                    self.panels.history_panel_open = false;
                    self.panels.history_entries.clear();
                    return;
                }
                // Escape handled in main.rs
                _ => {}
            }
        }

        // Tab search panel: j/k navigation (only when TextEdit not focused)
        if self.panels.tab_search_open {
            match &event.key {
                Key::Down => {
                    self.panels.tab_search_selected =
                        self.panels.tab_search_selected.saturating_sub(1);
                    return;
                }
                Key::Up => {
                    self.panels.tab_search_selected =
                        self.panels.tab_search_selected.saturating_sub(1);
                    return;
                }
                Key::Enter => {
                    let panes = self.wm.panes();
                    let ids: Vec<_> = panes.iter().map(|(id, _)| *id).collect();
                    if let Some(id) = ids.get(self.panels.tab_search_selected) {
                        self.wm.set_active_pane(*id);
                    }
                    self.panels.tab_search_open = false;
                    return;
                }
                // Escape handled in main.rs
                _ => {}
            }
        }

        // Bookmarks panel: j/k navigation
        if self.panels.bookmarks_panel_open {
            match &event.key {
                Key::Character('j') | Key::Down => {
                    if !self.panels.bookmarks_entries.is_empty() {
                        self.panels.bookmarks_selected = (self.panels.bookmarks_selected + 1)
                            .min(self.panels.bookmarks_entries.len() - 1);
                    }
                    return;
                }
                Key::Character('k') | Key::Up => {
                    self.panels.bookmarks_selected =
                        self.panels.bookmarks_selected.saturating_sub(1);
                    return;
                }
                Key::Enter => {
                    if let Some(bm) = self
                        .panels
                        .bookmarks_entries
                        .get(self.panels.bookmarks_selected)
                        && let Ok(url) = url::Url::parse(&bm.url)
                    {
                        self.pending_wry_actions.push_back(WryAction::Navigate(url));
                    }
                    self.panels.bookmarks_panel_open = false;
                    self.panels.bookmarks_entries.clear();
                    return;
                }
                Key::Character('d') => {
                    // d to delete selected bookmark
                    if let Some(bm) = self
                        .panels
                        .bookmarks_entries
                        .get(self.panels.bookmarks_selected)
                    {
                        if let Some(db) = self.db.as_ref()
                            && let Err(e) = crate::db::bookmarks::remove_bookmark_by_id(db, bm.id)
                        {
                            tracing::warn!("Failed to remove bookmark by id: {}", e);
                        }
                        let removed_id = bm.id;
                        self.panels.bookmarks_entries.retain(|b| b.id != removed_id);
                        if self.panels.bookmarks_selected >= self.panels.bookmarks_entries.len() {
                            self.panels.bookmarks_selected =
                                self.panels.bookmarks_entries.len().saturating_sub(1);
                        }
                    }
                    return;
                }
                _ => {}
            }
        }

        // If palette is open, route input to it
        if self.palette.open {
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
                let action = self.palette.handle_input(&key_str);
                match action {
                    PaletteAction::ItemSelected(item) => {
                        self.palette.close();
                        self.ui.command_palette_input.clear();
                        self.execute_palette_selection(&item);
                    }
                    PaletteAction::Closed => {
                        self.palette.close();
                        self.ui.command_palette_input.clear();
                    }
                    PaletteAction::QuerySubmit(query) => {
                        self.palette.close();
                        self.ui.command_palette_input.clear();
                        self.handle_raw_command(&query);
                    }
                    PaletteAction::Consumed => {
                        self.ui.command_palette_input = self.palette.query.clone();
                    }
                }
            }
            return;
        }

        // Handle pending mark actions (m or ' prefix)
        if let Some(action) = self.session.pending_mark_action.take()
            && let Key::Character(c) = &event.key
            && c.is_ascii_lowercase()
        {
            let active_id = self.wm.active_pane_id();
            match action {
                's' => {
                    // Store the pending mark letter; JS will send the actual
                    // scroll fraction via IPC, which is handled in frame_tasks.rs.
                    self.session.pending_mark_set = Some(*c);
                    self.pending_wry_actions
                        .push_back(WryAction::CaptureScrollFraction);
                    self.ui.status_message = format!("Mark {c} set");
                }
                'g' => {
                    if let Some(frac) = self
                        .session
                        .marks
                        .get(&active_id)
                        .and_then(|m| m.get(c))
                        .copied()
                    {
                        // Set a pending scroll target; the main loop will apply it.
                        self.session.pending_mark_jump = Some(frac);
                        self.ui.status_message = format!("Mark {c} jumped");
                    } else {
                        self.ui.status_message = format!("Mark {c} not set");
                    }
                }
                _ => {}
            }
            return;
        }

        // Check keybindings first
        let action = self
            .keybindings
            .lookup(self.mode, event.modifiers, event.key.clone())
            .cloned();
        if let Some(action) = action {
            self.execute_action(&action);
            return;
        }

        // Mark prefix keys in Normal mode
        if self.mode == Mode::Normal {
            if let Key::Character('m') = &event.key {
                self.session.pending_mark_action = Some('s');
                self.ui.status_message = "Set mark (press a-z)".into();
                return;
            } else if let Key::Character('\'') = &event.key {
                self.session.pending_mark_action = Some('g');
                self.ui.status_message = "Go to mark (press a-z)".into();
                return;
            }
        }

        // Check mode transitions
        if let Some(new_mode) = crate::input::mode::transition(self.mode, &event) {
            self.mode = new_mode;
            self.update_status();
            if let Some(ref engine) = self.lua_engine {
                engine.call_hooks("mode_change", &[self.mode.as_str()]);
            }
            return;
        }

        // Route to destination
        let dest = crate::input::router::route_event(self.mode, &event);
        match dest {
            EventDestination::KeybindingHandler => {}
            EventDestination::Servo => {
                if let Key::Character(c) = &event.key {
                    tracing::debug!("Would send '{}' to Servo", c);
                }
            }
            EventDestination::CommandPalette => {
                if let Key::Character(c) = &event.key {
                    self.ui.command_palette_input.push(*c);
                } else if event.key == Key::Backspace {
                    self.ui.command_palette_input.pop();
                } else if event.key == Key::Enter {
                    let input = self.ui.command_palette_input.clone();
                    self.execute_command(&input);
                    self.palette.close();
                    self.ui.command_palette_input.clear();
                }
            }
            EventDestination::Egui => {}
            EventDestination::Discard => {}
        }
    }

    pub(crate) fn execute_action(&mut self, action: &crate::input::Action) {
        self.session.session_dirty = true;
        use ActionEffect;

        let effects = super::dispatch::dispatch_action(action);

        for effect in &effects {
            match effect {
                ActionEffect::Wry(wry_action) => {
                    self.pending_wry_actions.push_back(wry_action.clone());
                }
                ActionEffect::Status(msg) => {
                    self.ui.status_message = msg.clone();
                }
                ActionEffect::SetMode(mode) => {
                    self.mode = *mode;
                    self.update_status();
                    if let Some(ref engine) = self.lua_engine {
                        engine.call_hooks("mode_change", &[self.mode.as_str()]);
                    }
                }
                ActionEffect::Quit => {
                    info!("Quit requested");
                    self.session.should_quit = true;
                }
                ActionEffect::OpenPalette => {
                    // Refresh items before opening so recent history/bookmarks are current
                    self.refresh_palette_items();
                    self.palette.open();
                    self.ui.command_palette_input.clear();
                    self.ui.status_message = "Command palette".into();
                }
                ActionEffect::RequestSplit(direction) => {
                    let active = self.wm.active_pane_id();
                    let new_url = self
                        .pending_new_tab_url
                        .take()
                        .unwrap_or_else(|| url::Url::parse("aileron://new").unwrap());
                    let is_private = self.tabs.private_pane_ids.contains(&active);
                    match self.wm.split(active, *direction, 0.5) {
                        Ok(new_id) => {
                            self.engines.create_pane(new_id, new_url, None);
                            // Propagate private mode to new pane
                            if is_private {
                                self.tabs.private_pane_ids.insert(new_id);
                            }
                            self.ui.status_message = "Split vertical".into();
                        }
                        Err(e) => self.ui.status_message = format!("Split failed: {e}"),
                    }
                }
                ActionEffect::OpenTerminal => {
                    let active = self.wm.active_pane_id();
                    let term_url = url::Url::parse("aileron://terminal").unwrap();
                    match self
                        .wm
                        .split(active, crate::wm::SplitDirection::Vertical, 0.5)
                    {
                        Ok(new_id) => {
                            self.engines.create_pane(new_id, term_url.clone(), None);
                            self.terminal_pane_ids.insert(new_id);
                            self.ui.status_message = "Terminal opened".into();
                        }
                        Err(e) => self.ui.status_message = format!("Terminal failed: {e}"),
                    }
                }
                ActionEffect::RequestClosePane => {
                    let active = self.wm.active_pane_id();
                    if self.tabs.pinned_pane_ids.contains(&active) {
                        self.ui.status_message =
                            "Cannot close pinned pane (use :pin to unpin)".into();
                        return;
                    }
                    if let Ok(()) = self.wm.close(active) {
                        self.engines.remove_pane(&active);
                        self.ui.status_message = "Pane closed".into();
                    }
                }
                ActionEffect::RequestNavigatePane(direction) => {
                    let current = self.wm.active_pane_id();
                    if let Some(id) = self.wm.navigate(*direction) {
                        self.tabs.last_active_pane_id = Some(current);
                        self.wm.set_active_pane(id);
                        self.update_status();
                        self.autofill.available = false;
                        self.autofill.js = None;
                        self.autofill.username_id.clear();
                        self.autofill.password_id.clear();
                        self.autofill.status_msg.clear();
                    }
                }
                ActionEffect::RequestExternalBrowser => {
                    let active_id = self.wm.active_pane_id();
                    if let Some(engine) = self.engines.get(&active_id)
                        && let Some(url) = engine.current_url()
                    {
                        match crate::servo::open_in_system_browser(url) {
                            Ok(()) => {
                                self.ui.status_message = "Opened in system browser".into();
                            }
                            Err(e) => {
                                self.ui.status_message = format!("Failed: {e}");
                            }
                        }
                    }
                }
                ActionEffect::OpenFindBar => {
                    self.ui.find_bar_open = true;
                    self.ui.find_query.clear();
                    self.ui.status_message = "Find: ".into();
                }
                ActionEffect::CloseFindBar => {
                    self.ui.find_bar_open = false;
                    self.ui.find_query.clear();
                    // Clear highlights in the page
                    self.pending_wry_actions.push_back(WryAction::RunJs(
                        "window.getSelection().removeAllRanges()".into(),
                    ));
                }
                ActionEffect::FindInPage { query, forward } => {
                    if !query.is_empty() {
                        let direction = if *forward { "true" } else { "false" };
                        let escaped = query.replace('\\', "\\\\").replace('\'', "\\'");
                        self.pending_wry_actions.push_back(WryAction::RunJs(format!(
                            "window.find('{escaped}', false, true, {direction}, false, false, false)"
                        )));
                    }
                }
                ActionEffect::ToggleLinkHints => {
                    self.ui.hint_mode = !self.ui.hint_mode;
                    self.ui.hint_new_tab = false;
                    if self.ui.hint_mode {
                        self.ui.status_message =
                            "Link hints: type letters, Escape to cancel".into();
                    } else {
                        self.ui.status_message.clear();
                    }
                    // Wry(RunJs) effect is also dispatched to inject/remove the CSS
                }
                ActionEffect::FollowLinkNewTab => {
                    self.ui.hint_mode = !self.ui.hint_mode;
                    self.ui.hint_new_tab = self.ui.hint_mode;
                    if self.ui.hint_mode {
                        self.ui.status_message =
                            "Link hints (new tab): type letters, Escape to cancel".into();
                    } else {
                        self.ui.status_message.clear();
                    }
                }
                ActionEffect::SaveWorkspace => {
                    // Queue a save action for main.rs to handle.
                    // main.rs has access to WryPaneManager for live URLs.
                    let name =
                        format!("workspace-{}", chrono::Local::now().format("%Y%m%d-%H%M%S"));
                    self.pending_wry_actions
                        .push_back(WryAction::SaveWorkspace {
                            name: name.clone(),
                            pane_urls: std::collections::HashMap::new(),
                        });
                    self.ui.status_message = format!("Saving workspace: {name}...");
                }
                ActionEffect::CopyUrl => {
                    let active_id = self.wm.active_pane_id();
                    if let Some(engine) = self.engines.get(&active_id)
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
                            self.ui.status_message = format!("Copied: {display}");
                        } else {
                            self.ui.status_message =
                                "Clipboard: no clipboard tool available".into();
                        }
                    }
                }
                ActionEffect::ResizePane(direction) => {
                    let active = self.wm.active_pane_id();
                    let amount = match direction {
                        crate::wm::Direction::Left | crate::wm::Direction::Up => -0.05,
                        crate::wm::Direction::Right | crate::wm::Direction::Down => 0.05,
                    };
                    match self.wm.resize_pane(active, amount) {
                        Ok(()) => self.ui.status_message = "Pane resized".into(),
                        Err(e) => self.ui.status_message = format!("Resize failed: {e}"),
                    }
                }
                ActionEffect::NewWindow => {
                    self.pending_new_window = true;
                    self.ui.status_message = "Opening new window...".into();
                }
                ActionEffect::EnterReaderMode => {
                    let active_id = self.wm.active_pane_id();
                    if self.tabs.reader_mode_panes.contains(&active_id) {
                        self.tabs.reader_mode_panes.remove(&active_id);
                        self.pending_wry_actions
                            .push_back(WryAction::ExitReaderMode);
                        self.ui.status_message = "Reader mode off".into();
                    } else {
                        self.tabs.reader_mode_panes.insert(active_id);
                        self.pending_wry_actions
                            .push_back(WryAction::EnterReaderMode);
                        self.ui.status_message = "Reader mode on".into();
                    }
                }
                ActionEffect::ExitReaderMode => {}
                ActionEffect::EnterMinimalMode => {
                    let active_id = self.wm.active_pane_id();
                    if self.tabs.minimal_mode_panes.contains(&active_id) {
                        self.tabs.minimal_mode_panes.remove(&active_id);
                        self.pending_wry_actions
                            .push_back(WryAction::ExitMinimalMode);
                        self.ui.status_message = "Minimal mode off".into();
                    } else {
                        self.tabs.minimal_mode_panes.insert(active_id);
                        self.pending_wry_actions
                            .push_back(WryAction::EnterMinimalMode);
                        self.ui.status_message = "Minimal mode on".into();
                    }
                }
                ActionEffect::ExitMinimalMode => {}
                ActionEffect::GetNetworkLog => {
                    self.pending_wry_actions.push_back(WryAction::GetNetworkLog);
                }
                ActionEffect::ClearNetworkLog => {}
                ActionEffect::GetConsoleLog => {
                    self.pending_wry_actions.push_back(WryAction::GetConsoleLog);
                }
                ActionEffect::ClearConsoleLog => {}
                ActionEffect::DetachPane => {
                    let active_id = self.wm.active_pane_id();
                    let url = self
                        .engines
                        .get(&active_id)
                        .and_then(|e| e.current_url().cloned());
                    if let Some(url) = url {
                        match self.wm.close(active_id) {
                            Ok(()) => {
                                self.engines.remove_pane(&active_id);
                                self.terminal_pane_ids.remove(&active_id);
                                self.pending_new_window = true;
                                self.pending_detach_url = Some(url);
                                self.ui.status_message = "Detaching pane to popup...".into();
                            }
                            Err(_) => {
                                self.ui.status_message = "Cannot detach the only pane".into();
                            }
                        }
                    } else {
                        self.ui.status_message = "No URL to detach".into();
                    }
                }
                ActionEffect::CloseOtherPanes => {
                    let active_id = self.wm.active_pane_id();
                    let other_ids: Vec<uuid::Uuid> = self
                        .wm
                        .panes()
                        .iter()
                        .filter_map(|(id, _)| if *id != active_id { Some(*id) } else { None })
                        .collect();
                    for id in &other_ids {
                        self.engines.remove_pane(id);
                        self.terminal_pane_ids.remove(id);
                    }
                    if let Err(e) = self.wm.retain_only(active_id) {
                        self.ui.status_message = format!("Failed: {e}");
                    } else {
                        self.ui.status_message =
                            format!("Closed {} other pane(s)", other_ids.len());
                    }
                }
                ActionEffect::Print => {
                    self.pending_wry_actions.push_back(WryAction::Print);
                    self.ui.status_message = "Printing...".into();
                }
                ActionEffect::RequestNewTab => {
                    let active_id = self.wm.active_pane_id();
                    let new_url = self
                        .pending_new_tab_url
                        .take()
                        .unwrap_or_else(|| url::Url::parse("aileron://new").unwrap());
                    if let Some(pane) = self
                        .wm
                        .root_mut()
                        .and_then(|root| crate::wm::BspTree::find_pane_mut(root, active_id))
                    {
                        let new_tab_id = pane.tabs.add(new_url);
                        self.engines
                            .create_pane(new_tab_id, pane.tabs.active().url.clone(), None);
                        self.ui.status_message =
                            format!("Tab {}/{}", pane.tabs.active_index() + 1, pane.tabs.len());
                    }
                }
                ActionEffect::RequestCloseTab => {
                    let active_id = self.wm.active_pane_id();
                    if let Some(pane) = self
                        .wm
                        .root_mut()
                        .and_then(|root| crate::wm::BspTree::find_pane_mut(root, active_id))
                    {
                        if pane.tabs.is_single() {
                            // Last tab: close the entire pane
                            if let Ok(()) = self.wm.close(active_id) {
                                self.engines.remove_pane(&active_id);
                                self.terminal_pane_ids.remove(&active_id);
                                self.ui.status_message = "Pane closed".into();
                            }
                        } else {
                            let closed = pane.tabs.close_active();
                            if let Some(closed_tab) = closed {
                                self.engines.remove_pane(&closed_tab.id);
                                self.ui.status_message = format!(
                                    "Tab {}/{}",
                                    pane.tabs.active_index() + 1,
                                    pane.tabs.len()
                                );
                            }
                        }
                    }
                }
                ActionEffect::RequestNextTab => {
                    let active_id = self.wm.active_pane_id();
                    if let Some(pane) = self
                        .wm
                        .root_mut()
                        .and_then(|root| crate::wm::BspTree::find_pane_mut(root, active_id))
                    {
                        pane.tabs.next();
                        self.ui.status_message =
                            format!("Tab {}/{}", pane.tabs.active_index() + 1, pane.tabs.len());
                    }
                }
                ActionEffect::RequestPrevTab => {
                    let active_id = self.wm.active_pane_id();
                    if let Some(pane) = self
                        .wm
                        .root_mut()
                        .and_then(|root| crate::wm::BspTree::find_pane_mut(root, active_id))
                    {
                        pane.tabs.prev();
                        self.ui.status_message =
                            format!("Tab {}/{}", pane.tabs.active_index() + 1, pane.tabs.len());
                    }
                }
                ActionEffect::PinPane => {
                    let active_id = self.wm.active_pane_id();
                    if self.tabs.pinned_pane_ids.contains(&active_id) {
                        self.tabs.pinned_pane_ids.remove(&active_id);
                        self.ui.status_message =
                            crate::i18n::tr(crate::i18n::TrKey("status_unpinned")).into();
                    } else {
                        self.tabs.pinned_pane_ids.insert(active_id);
                        self.ui.status_message =
                            crate::i18n::tr(crate::i18n::TrKey("status_pinned")).into();
                    }
                }
            }
        }
    }

    pub fn update_status(&mut self) {
        let mode_str = self.mode.as_str().to_string();
        self.ui.status_message = format!("-- {mode_str} --");
        self.ui.accessibility_text = format!("Mode: {mode_str}");
    }

    /// Update the accessibility live-region text with a status summary.
    /// Call this when important state changes occur (navigation, error, etc.).
    pub fn update_a11y(&mut self, msg: &str) {
        self.ui.status_message = msg.to_string();
        self.ui.accessibility_text = msg.to_string();
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
        state.mode = Mode::Normal;
        state.update_status();
        assert_eq!(state.ui.status_message, "-- NORMAL --");
        assert_eq!(state.ui.accessibility_text, "Mode: NORMAL");
    }

    #[test]
    fn test_update_status_insert() {
        let mut state = make_state();
        state.mode = Mode::Insert;
        state.update_status();
        assert_eq!(state.ui.status_message, "-- INSERT --");
        assert_eq!(state.ui.accessibility_text, "Mode: INSERT");
    }

    #[test]
    fn test_update_status_command() {
        let mut state = make_state();
        state.mode = Mode::Command;
        state.update_status();
        assert_eq!(state.ui.status_message, "-- COMMAND --");
        assert_eq!(state.ui.accessibility_text, "Mode: COMMAND");
    }

    #[test]
    fn test_update_a11y() {
        let mut state = make_state();
        state.update_a11y("Navigation complete");
        assert_eq!(state.ui.status_message, "Navigation complete");
        assert_eq!(state.ui.accessibility_text, "Navigation complete");
    }

    #[test]
    fn test_update_a11y_overrides_status() {
        let mut state = make_state();
        state.mode = Mode::Normal;
        state.update_status();
        assert_eq!(state.ui.status_message, "-- NORMAL --");
        state.update_a11y("Error: page crashed");
        assert_eq!(state.ui.status_message, "Error: page crashed");
        assert_eq!(state.ui.accessibility_text, "Error: page crashed");
    }

    #[test]
    fn test_update_status_after_a11y() {
        let mut state = make_state();
        state.update_a11y("some message");
        state.mode = Mode::Insert;
        state.update_status();
        assert_eq!(state.ui.status_message, "-- INSERT --");
        assert_eq!(state.ui.accessibility_text, "Mode: INSERT");
    }
}
