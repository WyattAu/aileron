use tracing::info;

use crate::input::{EventDestination, Key, KeyEvent, Mode};
use crate::ui::palette::PaletteAction;

use super::dispatch::ActionEffect;
use super::AppState;
use super::WryAction;

impl AppState {
    pub fn process_key_event(&mut self, event: KeyEvent) {
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
                        self.command_palette_open = false;
                        self.command_palette_input.clear();
                        self.execute_palette_selection(&item);
                    }
                    PaletteAction::Closed => {
                        self.command_palette_open = false;
                        self.command_palette_input.clear();
                    }
                    PaletteAction::QuerySubmit(query) => {
                        self.command_palette_open = false;
                        self.command_palette_input.clear();
                        self.handle_raw_command(&query);
                    }
                    PaletteAction::Consumed => {
                        self.command_palette_input = self.palette.query.clone();
                    }
                }
            }
            return;
        }

        // Handle pending mark actions (m or ' prefix)
        if let Some(action) = self.pending_mark_action.take()
            && let Key::Character(c) = &event.key
            && c.is_ascii_lowercase()
        {
            let active_id = self.wm.active_pane_id();
            match action {
                's' => {
                    self.marks.entry(active_id).or_default().insert(*c, 0.5);
                    self.status_message = format!("Mark {} set", c);
                }
                'g' => {
                    if self
                        .marks
                        .get(&active_id)
                        .and_then(|m| m.get(c))
                        .is_some()
                    {
                        self.status_message = format!("Mark {} jumped", c);
                    } else {
                        self.status_message = format!("Mark {} not set", c);
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
                self.pending_mark_action = Some('s');
                self.status_message = "Set mark (press a-z)".into();
                return;
            } else if let Key::Character('\'') = &event.key {
                self.pending_mark_action = Some('g');
                self.status_message = "Go to mark (press a-z)".into();
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
                    self.command_palette_input.push(*c);
                } else if event.key == Key::Backspace {
                    self.command_palette_input.pop();
                } else if event.key == Key::Enter {
                    let input = self.command_palette_input.clone();
                    self.execute_command(&input);
                    self.command_palette_open = false;
                    self.command_palette_input.clear();
                }
            }
            EventDestination::Egui => {}
            EventDestination::Discard => {}
        }
    }

    pub(crate) fn execute_action(&mut self, action: &crate::input::Action) {
        self.session_dirty = true;
        use ActionEffect;

        let effects = super::dispatch::dispatch_action(action);

        for effect in &effects {
            match effect {
                ActionEffect::Wry(wry_action) => {
                    self.pending_wry_actions.push_back(wry_action.clone());
                }
                ActionEffect::Status(msg) => {
                    self.status_message = msg.clone();
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
                    self.should_quit = true;
                }
                ActionEffect::OpenPalette => {
                    // Refresh items before opening so recent history/bookmarks are current
                    self.refresh_palette_items();
                    self.palette.open();
                    self.command_palette_open = true;
                    self.command_palette_input.clear();
                    self.status_message = "Command palette".into();
                }
                ActionEffect::RequestSplit(direction) => {
                    let active = self.wm.active_pane_id();
                    let new_url = url::Url::parse("aileron://new").unwrap();
                    match self.wm.split(active, *direction, 0.5) {
                        Ok(new_id) => {
                            self.engines.create_pane(new_id, new_url);
                            self.status_message = "Split vertical".into();
                        }
                        Err(e) => self.status_message = format!("Split failed: {}", e),
                    }
                }
                ActionEffect::OpenTerminal => {
                    let active = self.wm.active_pane_id();
                    let term_url = url::Url::parse("aileron://terminal").unwrap();
                    match self.wm.split(active, crate::wm::SplitDirection::Vertical, 0.5) {
                        Ok(new_id) => {
                            self.engines.create_pane(new_id, term_url.clone());
                            self.terminal_pane_ids.insert(new_id);
                            self.status_message = "Terminal opened".into();
                        }
                        Err(e) => self.status_message = format!("Terminal failed: {}", e),
                    }
                }
                ActionEffect::RequestClosePane => {
                    let active = self.wm.active_pane_id();
                    if self.pinned_pane_ids.contains(&active) {
                        self.status_message = "Cannot close pinned pane (use :pin to unpin)".into();
                        return;
                    }
                    if let Ok(()) = self.wm.close(active) {
                        self.engines.remove_pane(&active);
                        self.status_message = "Pane closed".into();
                    }
                }
                ActionEffect::RequestNavigatePane(direction) => {
                    let current = self.wm.active_pane_id();
                    if let Some(id) = self.wm.navigate(*direction) {
                        self.last_active_pane_id = Some(current);
                        self.wm.set_active_pane(id);
                        self.update_status();
                    }
                }
                ActionEffect::RequestExternalBrowser => {
                    let active_id = self.wm.active_pane_id();
                    if let Some(engine) = self.engines.get(&active_id)
                        && let Some(url) = engine.current_url() {
                            match crate::servo::open_in_system_browser(url) {
                                Ok(()) => {
                                    self.status_message = "Opened in system browser".into();
                                }
                                Err(e) => {
                                    self.status_message = format!("Failed: {}", e);
                                }
                            }
                        }
                }
                ActionEffect::OpenFindBar => {
                    self.find_bar_open = true;
                    self.find_query.clear();
                    self.status_message = "Find: ".into();
                }
                ActionEffect::CloseFindBar => {
                    self.find_bar_open = false;
                    self.find_query.clear();
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
                            "window.find('{}', false, true, {}, false, false, false)",
                            escaped, direction
                        )));
                    }
                }
                ActionEffect::ToggleLinkHints => {
                    self.hint_mode = !self.hint_mode;
                    if self.hint_mode {
                        self.status_message = "Link hints: type number, Escape to cancel".into();
                    } else {
                        self.status_message.clear();
                    }
                    // Wry(RunJs) effect is also dispatched to inject/remove the CSS
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
                    self.status_message = format!("Saving workspace: {}...", name);
                }
                ActionEffect::CopyUrl => {
                    let active_id = self.wm.active_pane_id();
                    if let Some(engine) = self.engines.get(&active_id)
                        && let Some(url) = engine.current_url()
                    {
                        let url_str = url.to_string();
                        let copied = std::process::Command::new("wl-copy")
                            .arg(&url_str)
                            .stdout(std::process::Stdio::null())
                            .stderr(std::process::Stdio::null())
                            .status()
                            .ok()
                            .map(|s| s.success())
                            .unwrap_or(false)
                            || std::process::Command::new("xclip")
                                .args(["-selection", "clipboard"])
                                .arg(&url_str)
                                .stdout(std::process::Stdio::null())
                                .stderr(std::process::Stdio::null())
                                .status()
                                .ok()
                                .map(|s| s.success())
                                .unwrap_or(false);
                        if copied {
                            let display = if url_str.len() > 60 {
                                format!("{}...", &url_str[..57])
                            } else {
                                url_str
                            };
                            self.status_message = format!("Copied: {}", display);
                        } else {
                            self.status_message =
                                "Clipboard: install wl-clipboard or xclip".into();
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
                        Ok(()) => self.status_message = "Pane resized".into(),
                        Err(e) => self.status_message = format!("Resize failed: {}", e),
                    }
                }
                ActionEffect::NewWindow => {
                    self.pending_new_window = true;
                    self.status_message = "Opening new window...".into();
                }
                ActionEffect::EnterReaderMode => {
                    let active_id = self.wm.active_pane_id();
                    if self.reader_mode_panes.contains(&active_id) {
                        self.reader_mode_panes.remove(&active_id);
                        self.pending_wry_actions.push_back(WryAction::ExitReaderMode);
                        self.status_message = "Reader mode off".into();
                    } else {
                        self.reader_mode_panes.insert(active_id);
                        self.pending_wry_actions.push_back(WryAction::EnterReaderMode);
                        self.status_message = "Reader mode on".into();
                    }
                }
                ActionEffect::ExitReaderMode => {}
                ActionEffect::EnterMinimalMode => {
                    let active_id = self.wm.active_pane_id();
                    if self.minimal_mode_panes.contains(&active_id) {
                        self.minimal_mode_panes.remove(&active_id);
                        self.pending_wry_actions.push_back(WryAction::ExitMinimalMode);
                        self.status_message = "Minimal mode off".into();
                    } else {
                        self.minimal_mode_panes.insert(active_id);
                        self.pending_wry_actions.push_back(WryAction::EnterMinimalMode);
                        self.status_message = "Minimal mode on".into();
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
                                self.status_message = "Detaching pane to popup...".into();
                            }
                            Err(_) => {
                                self.status_message = "Cannot detach the only pane".into();
                            }
                        }
                    } else {
                        self.status_message = "No URL to detach".into();
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
                        self.status_message = format!("Failed: {}", e);
                    } else {
                        self.status_message = format!("Closed {} other pane(s)", other_ids.len());
                    }
                }
                ActionEffect::Print => {
                    self.pending_wry_actions.push_back(WryAction::Print);
                    self.status_message = "Printing...".into();
                }
                ActionEffect::PinPane => {
                    let active_id = self.wm.active_pane_id();
                    if self.pinned_pane_ids.contains(&active_id) {
                        self.pinned_pane_ids.remove(&active_id);
                        self.status_message = crate::i18n::tr(crate::i18n::TrKey("status_unpinned")).into();
                    } else {
                        self.pinned_pane_ids.insert(active_id);
                        self.status_message = crate::i18n::tr(crate::i18n::TrKey("status_pinned")).into();
                    }
                }
            }
        }
    }

    pub fn update_status(&mut self) {
        self.status_message = format!("-- {} --", self.mode);
    }
}
