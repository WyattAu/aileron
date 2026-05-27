//! ApplicationHandler trait implementation for AileronApp.

use super::*;

impl ApplicationHandler for AileronApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        info!("── resumed(): Creating window ──");
        let window = Arc::new(
            event_loop
                .create_window(
                    WindowAttributes::default()
                        .with_title("Aileron")
                        .with_inner_size(winit::dpi::LogicalSize::new(
                            self.config.window_width,
                            self.config.window_height,
                        )),
                )
                .expect("Failed to create window"),
        );

        info!(
            "Window created: {}x{}",
            window.inner_size().width,
            window.inner_size().height
        );
        self.init_graphics(window);

        // Create initial wry pane for the root BSP leaf
        if let Some(app_state) = &self.app_state {
            let root_pane_id = app_state.wm.active_pane_id();
            let root_url = app_state
                .engines
                .get(&root_pane_id)
                .and_then(|e| e.current_url().cloned())
                .unwrap_or_else(|| url::Url::parse("aileron://welcome").unwrap());
            self.create_wry_pane_for(root_pane_id, &root_url);
        }

        // Auto-restore workspace based on session state.
        // Prefer _autosave (crash recovery) if previous session was unclean.
        if let Some(app_state) = &mut self.app_state {
            let was_unclean = Config::was_previous_session_unclean();

            if (!self.config.restore_session || !was_unclean)
                && let Some(db) = app_state.db.as_ref()
                && let Err(e) = aileron::db::workspaces::delete_workspace(db, "_autosave")
            {
                tracing::warn!("Failed to delete autosave workspace: {}", e);
            }

            if self.config.restore_session {
                let all_workspaces = app_state
                    .db
                    .as_ref()
                    .and_then(|conn| aileron::db::workspaces::list_workspaces(conn).ok())
                    .unwrap_or_default();

                let to_restore = if was_unclean {
                    all_workspaces
                        .iter()
                        .find(|ws| ws.name == "_autosave")
                        .cloned()
                } else {
                    all_workspaces
                        .iter()
                        .find(|ws| ws.name != "_autosave")
                        .cloned()
                };

                if let Some(workspace) = to_restore {
                    info!("Auto-restoring workspace: {}", workspace.name);
                    app_state.pending_workspace_restore = Some(workspace.name.clone());
                    app_state.current_workspace_name = workspace.name;
                    app_state.session.session_dirty = true;
                }
            }
        }

        frame_tasks::load_default_adblock_rules(&mut self.adblocker);

        #[cfg(feature = "mcp")]
        frame_tasks::spawn_mcp_server(&self.mcp_bridge);

        info!(
            "Window + initial pane created in {:?}",
            self.startup_start.elapsed()
        );

        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        // Route popup window events
        if self.popup.contains_key(&window_id) {
            self.handle_popup_event(window_id, &event);
            return;
        }

        // Handle initialization event for a newly created popup window
        if let Some((popup_id, popup_window)) = self.popup.pending_popup_window.take() {
            if window_id == popup_id {
                self.init_popup_window(popup_id, popup_window);
                return;
            }
            self.popup.pending_popup_window = Some((popup_id, popup_window));
        }

        let window = match &self.window {
            Some(w) => Arc::clone(w),
            None => return,
        };
        let winit_state = match &mut self.egui_winit {
            Some(s) => s,
            None => return,
        };

        // Feed event to egui first
        let egui_response = winit_state.on_window_event(&window, &event);

        // Track modifiers
        if let WindowEvent::ModifiersChanged(state) = &event {
            let ms = state.state();
            self.modifiers = Modifiers {
                ctrl: ms.control_key(),
                alt: ms.alt_key(),
                shift: ms.shift_key(),
                super_key: ms.super_key(),
            };
        }

        // Handle resize — convert physical pixels to logical for BSP tree
        if let Some(app_state) = &mut self.app_state
            && let WindowEvent::Resized(physical_size) = &event
            && physical_size.width > 0
            && physical_size.height > 0
        {
            let scale = self
                .window
                .as_ref()
                .map(|w| w.scale_factor())
                .unwrap_or(1.0);
            app_state.wm.resize(Rect::new(
                0.0,
                0.0,
                physical_size.width as f64 / scale,
                physical_size.height as f64 / scale,
            ));
            // Defer pane repositioning to RedrawRequested to avoid calling
            // into GTK/WebKitGTK during the resize event itself, which can
            // deadlock or crash on NVIDIA + XWayland.
            self.resize_pending = true;
        }

        // Handle events
        match &event {
            WindowEvent::CloseRequested => {
                info!("Close requested — quitting");
                event_loop.exit();
            }

            WindowEvent::RedrawRequested => {
                let frame_start = std::time::Instant::now();
                self.frame_count += 1;
                if self.frame_count <= 3 || self.frame_count.is_multiple_of(300) {
                    info!("Render frame #{}", self.frame_count);
                }
                self.render();
                let frame_time = frame_start.elapsed();
                let frame_time_ms = frame_time.as_secs_f64() * 1000.0;
                if frame_time.as_millis() > 17 {
                    tracing::debug!("Frame over budget: {:.1}ms", frame_time_ms);
                }
                self.adaptive_quality.update(frame_time_ms);
            }

            WindowEvent::Resized(physical_size) => {
                if physical_size.width > 0
                    && physical_size.height > 0
                    && let Some(gfx) = &mut self.gfx
                {
                    gfx.resize(physical_size.width, physical_size.height);
                    self.resize_pending = true;
                }
            }

            WindowEvent::KeyboardInput {
                event:
                    winit::event::KeyEvent {
                        physical_key,
                        logical_key,
                        state: winit::event::ElementState::Pressed,
                        repeat,
                        ..
                    },
                ..
            } => {
                if *repeat && let Some(_app_state) = &self.app_state {
                    #[cfg(feature = "terminal")]
                    let active_id = _app_state.wm.active_pane_id();
                    #[cfg(feature = "terminal")]
                    if !self.terminal_manager.is_terminal(&active_id) {
                        return;
                    }
                }

                // On Wayland, a single keystroke can produce both an
                // Ime::Commit and a KeyboardInput event. Skip the
                // KeyboardInput if we already handled the IME commit.
                if self.ime_just_committed {
                    self.ime_just_committed = false;
                    return;
                }

                // Let egui consume the event first, UNLESS the command
                // palette is open or we're in Insert mode (where keys
                // must reach the offscreen webview, not be swallowed by egui).
                let palette_open = self
                    .app_state
                    .as_ref()
                    .map(|s| s.palette.open)
                    .unwrap_or(false);
                let insert_mode = self
                    .app_state
                    .as_ref()
                    .map(|s| s.mode == aileron::input::Mode::Insert)
                    .unwrap_or(false);
                if egui_response.consumed && !palette_open && !insert_mode {
                    return;
                }

                let key = aileron::input::map_key(*physical_key, logical_key);

                // Route key event through app state (mode machine + keybindings)
                let mods = self.modifiers;
                let aileron_event = AileronKeyEvent {
                    key: key.clone(),
                    modifiers: mods,
                    physical_key: None,
                };

                if let Some(app_state) = &mut self.app_state {
                    // ─── Hint mode: intercept letter keys to follow hinted links ───
                    if app_state.ui.hint_mode {
                        match &key {
                            aileron::input::Key::Character(c) if c.is_ascii_lowercase() => {
                                app_state.ui.hint_buffer.push(*c);
                                let hint_buf = app_state.ui.hint_buffer.clone();
                                let new_tab = app_state.ui.hint_new_tab;
                                // Use IPC to get click feedback for auto-exit
                                let js = if new_tab {
                                    format!(
                                        "(function() {{ \
                                            var el = document.querySelector('[data-aileron-hint=\"{hint_buf}\"]'); \
                                            if (el && el.href) {{ window.open(el.href, '_blank'); window.ipc.postMessage(JSON.stringify({{t:'hint-clicked'}})); return; }} \
                                            if (el) {{ el.click(); window.ipc.postMessage(JSON.stringify({{t:'hint-clicked'}})); return; }} \
                                            var all = document.querySelectorAll('[data-aileron-hint]'); \
                                            var matches = []; \
                                            all.forEach(function(e) {{ \
                                                if (e.getAttribute('data-aileron-hint').startsWith('{hint_buf}')) matches.push(e); \
                                            }}); \
                                            if (matches.length === 1 && matches[0].href) {{ window.open(matches[0].href, '_blank'); window.ipc.postMessage(JSON.stringify({{t:'hint-clicked'}})); return; }} \
                                            if (matches.length === 1) {{ matches[0].click(); window.ipc.postMessage(JSON.stringify({{t:'hint-clicked'}})); return; }} \
                                        }})()"
                                    )
                                } else {
                                    format!(
                                        "(function() {{ \
                                            var el = document.querySelector('[data-aileron-hint=\"{hint_buf}\"]'); \
                                            if (el) {{ el.click(); window.ipc.postMessage(JSON.stringify({{t:'hint-clicked'}})); return; }} \
                                            var all = document.querySelectorAll('[data-aileron-hint]'); \
                                            var matches = []; \
                                            all.forEach(function(e) {{ \
                                                if (e.getAttribute('data-aileron-hint').startsWith('{hint_buf}')) matches.push(e); \
                                            }}); \
                                            if (matches.length === 1) {{ matches[0].click(); window.ipc.postMessage(JSON.stringify({{t:'hint-clicked'}})); return; }} \
                                        }})()"
                                    )
                                };
                                let active_id = app_state.wm.active_pane_id();
                                if let Some(wry_pane) = self.wry_panes.get(&active_id) {
                                    wry_pane.execute_js(&js);
                                } else if let Some(pane) = self.offscreen_panes.get_mut(&active_id)
                                {
                                    pane.execute_js(&js);
                                    pane.mark_dirty();
                                }
                                return;
                            }
                            _ => {
                                // Any non-letter key exits hint mode
                                let active_id = app_state.wm.active_pane_id();
                                app_state.ui.hint_mode = false;
                                app_state.ui.hint_new_tab = false;
                                app_state.ui.hint_buffer.clear();
                                app_state.ui.status_message.clear();
                                let clear_js = r#"
                                    (function() {
                                        var style = document.getElementById('__aileron_hints');
                                        if (style) style.remove();
                                        document.querySelectorAll('[data-aileron-hint]').forEach(el => {
                                            el.removeAttribute('data-aileron-hint');
                                        });
                                    })();
                                "#;
                                if let Some(wry_pane) = self.wry_panes.get(&active_id) {
                                    wry_pane.execute_js(clear_js);
                                } else if let Some(pane) = self.offscreen_panes.get_mut(&active_id)
                                {
                                    pane.execute_js(clear_js);
                                    pane.mark_dirty();
                                }
                                return;
                            }
                        }
                    }

                    // Escape closes find bar first, then URL bar, then history panel
                    if app_state.ui.find_bar_open && key == aileron::input::Key::Escape {
                        app_state.ui.find_bar_open = false;
                        app_state.ui.find_query.clear();
                        let active_id = app_state.wm.active_pane_id();
                        if let Some(wry_pane) = self.wry_panes.get(&active_id) {
                            wry_pane.execute_js("window.getSelection().removeAllRanges()");
                        }
                        return;
                    }
                    if app_state.ui.url_bar_focused && key == aileron::input::Key::Escape {
                        app_state.ui.url_bar_focused = false;
                        app_state.ui.url_bar_input.clear();
                        return;
                    }
                    if app_state.panels.history_panel_open && key == aileron::input::Key::Escape {
                        app_state.panels.history_panel_open = false;
                        app_state.panels.history_entries.clear();
                        return;
                    }
                    if app_state.panels.tab_search_open && key == aileron::input::Key::Escape {
                        app_state.panels.tab_search_open = false;
                        return;
                    }
                    if app_state.panels.bookmarks_panel_open && key == aileron::input::Key::Escape {
                        app_state.panels.bookmarks_panel_open = false;
                        app_state.panels.bookmarks_entries.clear();
                        return;
                    }
                    if app_state.panels.help_panel_open && key == aileron::input::Key::Escape {
                        app_state.panels.help_panel_open = false;
                        return;
                    }
                    // Track pane count before processing key.
                    // Only construct HashSets if pane count actually changed.
                    let pane_count_before = app_state.wm.leaf_count();

                    app_state.process_key_event(aileron_event);
                    app_state.input_latency.record_key_press();

                    let pane_count_after = app_state.wm.leaf_count();

                    let (closed_pane_ids, new_pane_ids): (Vec<uuid::Uuid>, Vec<uuid::Uuid>) =
                        if pane_count_before == pane_count_after {
                            (Vec::new(), Vec::new())
                        } else {
                            let pane_ids_before: std::collections::HashSet<uuid::Uuid> =
                                app_state.wm.panes_ref().iter().map(|(id, _)| *id).collect();
                            let pane_ids_after: std::collections::HashSet<uuid::Uuid> =
                                app_state.wm.panes_ref().iter().map(|(id, _)| *id).collect();
                            (
                                pane_ids_before
                                    .difference(&pane_ids_after)
                                    .copied()
                                    .collect(),
                                pane_ids_after
                                    .difference(&pane_ids_before)
                                    .copied()
                                    .collect(),
                            )
                        };

                    let need_reposition = pane_count_before != pane_count_after;
                    let active_pane_id = app_state.wm.active_pane_id();
                    let is_insert_mode = app_state.mode == aileron::input::Mode::Insert;

                    // Now sync wry panes (drop borrow on app_state first)
                    for pid in &new_pane_ids {
                        let new_url = url::Url::parse("aileron://new").unwrap();
                        if *pid == active_pane_id || !self.config.is_offscreen() {
                            self.create_wry_pane_for(*pid, &new_url);
                        } else {
                            self.pending_pane_creates.push_back((*pid, new_url));
                        }
                    }

                    for pid in &closed_pane_ids {
                        // Save closed tab info for :tab-restore
                        let url = self
                            .wry_panes
                            .url_for(pid)
                            .map(|u| u.to_string())
                            .unwrap_or_default();
                        let title = self
                            .wry_panes
                            .get(pid)
                            .map(|p| p.title().to_string())
                            .unwrap_or_default();
                        if !url.is_empty()
                            && let Some(app_state) = &mut self.app_state
                        {
                            app_state.tabs.closed_tab_stack.push_back((url, title));
                            // Limit stack to 50 entries
                            while app_state.tabs.closed_tab_stack.len() > 50 {
                                app_state.tabs.closed_tab_stack.pop_front();
                            }
                        }
                        self.remove_wry_pane_for(pid);
                    }

                    if need_reposition {
                        self.reposition_all_panes();
                    }

                    // Handle Insert mode: focus the wry webview (native mode only)
                    if is_insert_mode
                        && !self.config.is_offscreen()
                        && let Some(wry_pane) = self.wry_panes.get(&active_pane_id)
                    {
                        wry_pane.focus();
                    }

                    // Offscreen mode: forward keyboard to webview via JS or native terminal.
                    // This is the SOLE forwarding point for offscreen panes — all key
                    // forwarding (character, backspace, enter, tab, arrows, etc.) happens
                    // here. The earlier process_key_event() routes to a Servo stub; the
                    // actual delivery is done in this block.
                    if is_insert_mode && self.config.is_offscreen() {
                        #[cfg(feature = "terminal")]
                        let is_terminal = self.terminal_manager.is_terminal(&active_pane_id);

                        #[cfg(feature = "terminal")]
                        if is_terminal {
                            // Native terminal: write directly to PTY
                            if let aileron::input::Key::Character(c) = &key {
                                let mut buf = [0u8; 4];
                                let s = c.encode_utf8(&mut buf);
                                self.terminal_manager.write_input(&active_pane_id, s);
                            } else {
                                // Convert special keys to escape sequences
                                let escape_seq = key_to_escape_sequence(&key, mods);
                                if !escape_seq.is_empty() {
                                    self.terminal_manager
                                        .write_input(&active_pane_id, &escape_seq);
                                }
                            }
                        }

                        #[cfg(feature = "terminal")]
                        let forward_to_webview = !is_terminal;
                        #[cfg(not(feature = "terminal"))]
                        let forward_to_webview = true;

                        if forward_to_webview
                            && let Some(pane) = self.offscreen_panes.get_mut(&active_pane_id)
                        {
                            // Web content: forward via JS
                            if let aileron::input::Key::Character(c) = &key {
                                let mut buf = [0u8; 4];
                                let s = c.encode_utf8(&mut buf);
                                pane.insert_text(s);
                            } else if let aileron::input::Key::Backspace = &key {
                                // Use execCommand('delete') for backspace — it
                                // operates on the focused input/textarea regardless of
                                // React/virtual-DOM event handling. Dispatching a
                                // synthetic KeyboardEvent on document often gets ignored
                                // by SPA frameworks like Google's React-based search.
                                pane.execute_js("document.execCommand('delete', false, null)");
                                pane.mark_dirty();
                            } else if matches!(
                                &key,
                                aileron::input::Key::Enter | aileron::input::Key::Tab
                            ) {
                                let (js_key, js_code) = key_to_js(&key);
                                let mods = aileron::offscreen_webview::modifiers_js(
                                    mods.ctrl,
                                    mods.alt,
                                    mods.shift,
                                    mods.super_key,
                                );
                                pane.forward_key_event("keydown", &js_key, &js_code, &mods);
                            } else {
                                // Arrow keys, F-keys, Home/End, etc.
                                let (js_key, js_code) = key_to_js(&key);
                                let mods = aileron::offscreen_webview::modifiers_js(
                                    mods.ctrl,
                                    mods.alt,
                                    mods.shift,
                                    mods.super_key,
                                );
                                pane.forward_key_event("keydown", &js_key, &js_code, &mods);
                            }
                        }
                    }
                }
            }

            // Forward keyup events to the active offscreen webview (needed for
            // proper key state tracking in web content — e.g., shift-release
            // ending text selection).
            WindowEvent::KeyboardInput {
                event:
                    winit::event::KeyEvent {
                        physical_key,
                        logical_key,
                        state: winit::event::ElementState::Released,
                        ..
                    },
                ..
            } => {
                if let Some(app_state) = &self.app_state
                    && app_state.mode == aileron::input::Mode::Insert
                {
                    let active_id = app_state.wm.active_pane_id();
                    #[cfg(feature = "terminal")]
                    let is_terminal = self.terminal_manager.is_terminal(&active_id);
                    #[cfg(not(feature = "terminal"))]
                    let is_terminal = false;

                    if !is_terminal {
                        let key = aileron::input::map_key(*physical_key, logical_key);
                        if let Some(pane) = self.offscreen_panes.get_mut(&active_id) {
                            let (js_key, js_code) = key_to_js(&key);
                            let mods = aileron::offscreen_webview::modifiers_js(
                                self.modifiers.ctrl,
                                self.modifiers.alt,
                                self.modifiers.shift,
                                self.modifiers.super_key,
                            );
                            pane.forward_key_event("keyup", &js_key, &js_code, &mods);
                        }
                    }
                }
            }

            WindowEvent::DroppedFile(path) => {
                info!("File dropped: {:?}", path);
            }

            WindowEvent::MouseWheel { delta, .. } => {
                if let Some(app_state) = &self.app_state {
                    let active_id = app_state.wm.active_pane_id();
                    let (dx, dy) = match delta {
                        winit::event::MouseScrollDelta::LineDelta(x, y) => {
                            (*x as f64 * 40.0, *y as f64 * 40.0)
                        }
                        winit::event::MouseScrollDelta::PixelDelta(pos) => (pos.x, pos.y),
                    };
                    if dx.abs() > 0.1 || dy.abs() > 0.1 {
                        #[cfg(feature = "terminal")]
                        {
                            if self.terminal_manager.is_terminal(&active_id) {
                                // Native terminal: scroll scrollback buffer
                                // Positive dy = scroll down (toward bottom), negative = scroll up
                                let lines = (dy / 40.0).round() as i32;
                                if lines != 0 {
                                    self.terminal_manager.scroll(&active_id, -lines);
                                }
                                return;
                            }
                        }

                        let is_terminal = {
                            #[cfg(feature = "terminal")]
                            {
                                self.terminal_manager.is_terminal(&active_id)
                            }
                            #[cfg(not(feature = "terminal"))]
                            {
                                false
                            }
                        };

                        if !is_terminal && !self.config.is_offscreen() {
                            if let Some(wry_pane) = self.wry_panes.get(&active_id) {
                                let js = format!(
                                    "window.scrollBy({{top: {}, left: {}, behavior: 'instant'}})",
                                    -dy, -dx
                                );
                                wry_pane.execute_js(&js);
                            }
                        } else if !is_terminal
                            && let Some(pane) = self.offscreen_panes.get_mut(&active_id)
                        {
                            pane.scroll_by(-dx, -dy);
                        }
                    }
                }
            }

            WindowEvent::MouseInput { state, button, .. } => {
                if self.config.is_offscreen()
                    && let Some(app_state) = &self.app_state
                    && app_state.mode == aileron::input::Mode::Insert
                {
                    self.offscreen_mouse_pressed = *state == winit::event::ElementState::Pressed;

                    let active_id = app_state.wm.active_pane_id();

                    #[cfg(feature = "terminal")]
                    if self.terminal_manager.is_terminal(&active_id) {
                        let terminal_info = (|| {
                            let ws = self.egui_winit.as_ref()?;
                            let ctx = ws.egui_ctx();
                            let pos = ctx.pointer_latest_pos()?;
                            let panes = app_state.wm.panes_ref();
                            let (_, rect) = panes.iter().find(|(id, _)| *id == active_id)?;
                            let top_offset = STATUS_BAR_HEIGHT as f32;
                            let sidebar_offset = if app_state.config.tab_layout == "sidebar"
                                && !app_state.config.tab_sidebar_right
                            {
                                app_state.config.tab_sidebar_width
                            } else {
                                0.0
                            };
                            let local_x = pos.x - rect.x as f32 - sidebar_offset;
                            let local_y = pos.y - rect.y as f32 - top_offset;
                            if local_x >= 0.0 && local_y >= 0.0 {
                                Some((local_x, local_y))
                            } else {
                                None
                            }
                        })();

                        if let Some((local_x, local_y)) = terminal_info {
                            use aileron::terminal::grid::CellMetrics;
                            if let Some(ws) = self.egui_winit.as_ref() {
                                let metrics = CellMetrics::from_egui(ws.egui_ctx(), 14.0);
                                if let Some(pane) = self.terminal_manager.get_mut(&active_id) {
                                    let (line, col) = pane.pixel_to_grid(
                                        local_x,
                                        local_y,
                                        metrics.cell_width,
                                        metrics.cell_height,
                                    );
                                    match (state, button) {
                                        (
                                            winit::event::ElementState::Pressed,
                                            winit::event::MouseButton::Left,
                                        ) => {
                                            pane.start_selection(line, col);
                                        }
                                        (
                                            winit::event::ElementState::Released,
                                            winit::event::MouseButton::Left,
                                        ) => {
                                            pane.end_selection();
                                            if let Some(text) = pane.selection_text() {
                                                ws.egui_ctx().copy_text(text);
                                            }
                                        }
                                        (
                                            winit::event::ElementState::Pressed,
                                            winit::event::MouseButton::Right,
                                        ) => {
                                            pane.clear_selection();
                                        }
                                        (
                                            winit::event::ElementState::Pressed,
                                            winit::event::MouseButton::Middle,
                                        ) => {
                                            if let Some(pane_ref) =
                                                self.terminal_manager.get(&active_id)
                                                && let Some(text) = pane_ref.selection_text()
                                            {
                                                self.terminal_manager
                                                    .write_input(&active_id, &text);
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }

                    #[cfg(feature = "terminal")]
                    let is_terminal = self.terminal_manager.is_terminal(&active_id);
                    #[cfg(not(feature = "terminal"))]
                    let is_terminal = false;

                    if !is_terminal {
                        let forward_info = (|| {
                            let ws = self.egui_winit.as_ref()?;
                            let ctx = ws.egui_ctx();
                            let pos = ctx.pointer_latest_pos()?;
                            let panes = app_state.wm.panes_ref();
                            let (_, rect) = panes.iter().find(|(id, _)| *id == active_id)?;
                            let (pw, ph) = self.offscreen_panes.get(&active_id)?.dimensions();
                            let top_offset = STATUS_BAR_HEIGHT as f32;
                            let sidebar_offset = if app_state.config.tab_layout == "sidebar"
                                && !app_state.config.tab_sidebar_right
                            {
                                app_state.config.tab_sidebar_width
                            } else {
                                0.0
                            };
                            let local_x = pos.x - rect.x as f32 - sidebar_offset;
                            let local_y = pos.y - rect.y as f32 - top_offset;
                            if local_x >= 0.0
                                && local_y >= 0.0
                                && local_x < pw as f32
                                && local_y < ph as f32
                            {
                                let event_type = match state {
                                    winit::event::ElementState::Pressed => "mousedown",
                                    winit::event::ElementState::Released => "mouseup",
                                };
                                let btn = match button {
                                    winit::event::MouseButton::Left => "0",
                                    winit::event::MouseButton::Middle => "1",
                                    winit::event::MouseButton::Right => "2",
                                    winit::event::MouseButton::Back => "3",
                                    winit::event::MouseButton::Forward => "4",
                                    _ => "0",
                                };
                                Some((event_type, local_x as f64, local_y as f64, btn))
                            } else {
                                None
                            }
                        })();

                        if let Some((event_type, local_x, local_y, btn)) = forward_info {
                            // Ctrl+Click or Middle-click opens links in new tab
                            if (*button == winit::event::MouseButton::Left
                                && *state == winit::event::ElementState::Pressed
                                && self.modifiers.ctrl)
                                || (*button == winit::event::MouseButton::Middle
                                    && *state == winit::event::ElementState::Pressed)
                            {
                                let js = format!(
                                    r#"(function() {{
                                        var el = document.elementFromPoint({local_x}, {local_y});
                                        while (el && el.tagName !== 'A') {{ el = el.parentElement; }}
                                        if (el && el.href) {{
                                            window.open(el.href, '_blank');
                                        }}
                                    }})();"#
                                );
                                if let Some(pane) = self.offscreen_panes.get_mut(&active_id) {
                                    pane.execute_js(&js);
                                }
                            } else {
                                let mods = aileron::offscreen_webview::modifiers_js(
                                    self.modifiers.ctrl,
                                    self.modifiers.alt,
                                    self.modifiers.shift,
                                    self.modifiers.super_key,
                                );
                                if let Some(pane) = self.offscreen_panes.get_mut(&active_id) {
                                    pane.forward_mouse_event(
                                        event_type, local_x, local_y, btn, &mods,
                                    );
                                    // Dispatch a click event after mouseup so that
                                    // website button handlers (which listen for 'click')
                                    // fire correctly.
                                    if *state == winit::event::ElementState::Released {
                                        pane.forward_mouse_event(
                                            "click", local_x, local_y, btn, &mods,
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }

            WindowEvent::CursorMoved { position, .. } if self.config.is_offscreen() => {
                let scale = window.scale_factor() as f32;
                let logical_pos = egui::pos2(position.x as f32 / scale, position.y as f32 / scale);

                if let Some(app_state) = &self.app_state
                    && app_state.mode == aileron::input::Mode::Insert
                {
                    let active_id = app_state.wm.active_pane_id();

                    #[cfg(feature = "terminal")]
                    if self.terminal_manager.is_terminal(&active_id) {
                        let terminal_info = (|| {
                            let panes = app_state.wm.panes_ref();
                            let (_, rect) = panes.iter().find(|(id, _)| *id == active_id)?;
                            let top_offset = STATUS_BAR_HEIGHT as f32;
                            let sidebar_offset = if app_state.config.tab_layout == "sidebar"
                                && !app_state.config.tab_sidebar_right
                            {
                                app_state.config.tab_sidebar_width
                            } else {
                                0.0
                            };
                            let local_x = logical_pos.x - rect.x as f32 - sidebar_offset;
                            let local_y = logical_pos.y - rect.y as f32 - top_offset;
                            if local_x >= 0.0 && local_y >= 0.0 {
                                Some((local_x, local_y))
                            } else {
                                None
                            }
                        })();

                        if let Some((local_x, local_y)) = terminal_info {
                            use aileron::terminal::grid::CellMetrics;
                            if let Some(ws) = self.egui_winit.as_ref()
                                && let Some(pane) = self.terminal_manager.get_mut(&active_id)
                                && pane.is_selecting()
                            {
                                let metrics = CellMetrics::from_egui(ws.egui_ctx(), 14.0);
                                let (line, col) = pane.pixel_to_grid(
                                    local_x,
                                    local_y,
                                    metrics.cell_width,
                                    metrics.cell_height,
                                );
                                pane.extend_selection(line, col);
                            }
                        }
                    }

                    #[cfg(feature = "terminal")]
                    let is_terminal = self.terminal_manager.is_terminal(&active_id);
                    #[cfg(not(feature = "terminal"))]
                    let is_terminal = false;

                    if !is_terminal {
                        let forward_info = (|| {
                            let panes = app_state.wm.panes_ref();
                            let (_, rect) = panes.iter().find(|(id, _)| *id == active_id)?;
                            let (pw, ph) = self.offscreen_panes.get(&active_id)?.dimensions();
                            let top_offset = STATUS_BAR_HEIGHT as f32;
                            let sidebar_offset = if app_state.config.tab_layout == "sidebar"
                                && !app_state.config.tab_sidebar_right
                            {
                                app_state.config.tab_sidebar_width
                            } else {
                                0.0
                            };
                            let local_x = logical_pos.x - rect.x as f32 - sidebar_offset;
                            let local_y = logical_pos.y - rect.y as f32 - top_offset;
                            if local_x >= 0.0
                                && local_y >= 0.0
                                && local_x < pw as f32
                                && local_y < ph as f32
                            {
                                Some((local_x as f64, local_y as f64))
                            } else {
                                None
                            }
                        })();

                        if let Some((local_x, local_y)) = forward_info
                            && self.offscreen_mouse_pressed
                        {
                            let mods = aileron::offscreen_webview::modifiers_js(
                                self.modifiers.ctrl,
                                self.modifiers.alt,
                                self.modifiers.shift,
                                self.modifiers.super_key,
                            );
                            if let Some(pane) = self.offscreen_panes.get_mut(&active_id) {
                                pane.forward_mouse_event("mousemove", local_x, local_y, "0", &mods);
                            }
                        }
                    }
                }
            }

            WindowEvent::Ime(ime) => {
                if let Some(app_state) = &mut self.app_state {
                    match ime {
                        winit::event::Ime::Commit(text) => {
                            // On Wayland, printable characters arrive as IME events
                            // instead of KeyboardInput. Route them through the same
                            // keybind system in Normal/Command modes so that single-
                            // character keybinds (j, k, i, :, etc.) work.
                            let chars: Vec<char> = text.chars().collect();
                            if chars.len() != 1 {
                                // Multi-char IME results (emoji pickers, etc.)
                                // — let egui handle them in Insert mode.
                                if app_state.mode == aileron::input::Mode::Insert
                                    && self.config.is_offscreen()
                                {
                                    let active_id = app_state.wm.active_pane_id();
                                    let text_owned = text.clone();
                                    #[cfg(feature = "terminal")]
                                    if self.terminal_manager.is_terminal(&active_id) {
                                        self.terminal_manager.write_input(&active_id, &text_owned);
                                    }
                                    #[cfg(feature = "terminal")]
                                    if !self.terminal_manager.is_terminal(&active_id)
                                        && let Some(pane) = self.offscreen_panes.get_mut(&active_id)
                                    {
                                        pane.insert_text(&text_owned);
                                    }
                                    #[cfg(not(feature = "terminal"))]
                                    if let Some(pane) = self.offscreen_panes.get_mut(&active_id) {
                                        pane.insert_text(&text_owned);
                                    }
                                }
                                return;
                            }

                            let c = chars[0];
                            let is_newline = c == '\r' || c == '\n';

                            if app_state.palette.open {
                                // When the palette is open, egui's TextEdit
                                // receives the IME commit and updates palette.query
                                // automatically. We only need to intercept Enter
                                // (submit) and Escape (close) ourselves.
                                if is_newline {
                                    // Enter: submit the query from palette.query
                                    let query = app_state.palette.query.trim().to_string();
                                    app_state.execute_command_pub(&query);
                                    app_state.palette.close();
                                    app_state.ui.command_palette_input.clear();
                                } else if c == '\x1b' {
                                    // Escape: close palette
                                    app_state.palette.close();
                                    app_state.ui.command_palette_input.clear();
                                }
                                // For regular characters, let egui's TextEdit
                                // handle them — no action needed here.
                                self.ime_just_committed = true;
                            } else {
                                match app_state.mode {
                                    aileron::input::Mode::Normal
                                    | aileron::input::Mode::Command => {
                                        let key = if is_newline {
                                            aileron::input::Key::Enter
                                        } else {
                                            aileron::input::Key::Character(c)
                                        };
                                        let aileron_event = aileron::input::KeyEvent {
                                            key: key.clone(),
                                            modifiers: self.modifiers,
                                            physical_key: None,
                                        };
                                        app_state.process_key_event(aileron_event);
                                        self.ime_just_committed = true;
                                    }
                                    aileron::input::Mode::Insert => {
                                        let active_id = app_state.wm.active_pane_id();
                                        let text_owned = text.clone();

                                        if self.config.is_offscreen() {
                                            #[cfg(feature = "terminal")]
                                            if self.terminal_manager.is_terminal(&active_id) {
                                                self.terminal_manager
                                                    .write_input(&active_id, &text_owned);
                                            }
                                            #[cfg(feature = "terminal")]
                                            if !self.terminal_manager.is_terminal(&active_id)
                                                && let Some(pane) =
                                                    self.offscreen_panes.get_mut(&active_id)
                                            {
                                                pane.insert_text(&text_owned);
                                            }
                                            #[cfg(not(feature = "terminal"))]
                                            if let Some(pane) =
                                                self.offscreen_panes.get_mut(&active_id)
                                            {
                                                pane.insert_text(&text_owned);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        winit::event::Ime::Preedit(text, _cursor) => {
                            if text.is_empty() {
                                if app_state.ui.status_message.starts_with("composing: ") {
                                    app_state.ui.status_message.clear();
                                }
                            } else {
                                app_state.ui.status_message = format!("composing: {text}");
                            }
                        }
                        _ => {}
                    }
                }
            }

            _ => {}
        }

        // Check if app wants to quit
        if let Some(app_state) = &self.app_state
            && app_state.session.should_quit
        {
            event_loop.exit();
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        _event: winit::event::DeviceEvent,
    ) {
        // Reserved for future raw input handling (X11 XInput2 events).
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(app_state) = &mut self.app_state {
            app_state.profiler.start_frame();
        }

        // Defer pane repositioning to end-of-frame (single call).
        let mut layout_dirty = self.resize_pending;
        if self.resize_pending {
            // Reset capture timers so the first frame after resize is captured
            // immediately rather than being throttled by the interval timer.
            for (_, ts) in self.offscreen_last_capture.iter_mut() {
                *ts = std::time::Instant::now() - std::time::Duration::from_secs(10);
            }
        }
        self.resize_pending = false;

        if self.first_frame {
            self.first_frame = false;
            info!("Startup completed in {:?}", self.startup_start.elapsed());
        }

        if let Some(app_state) = &mut self.app_state
            && app_state.pending_new_window
        {
            app_state.pending_new_window = false;
            self.popup.pending_new_window = true;
        }

        // Handle pending new tab request from UI ("+" button).
        if let Some(app_state) = &mut self.app_state
            && app_state.pending_new_tab
        {
            app_state.pending_new_tab = false;
            let new_url = url::Url::parse("aileron://new").unwrap();
            let active_id = app_state.wm.active_pane_id();
            if let Some(pane) = app_state
                .wm
                .root_mut()
                .and_then(|root| aileron::wm::BspTree::find_pane_mut(root, active_id))
            {
                let new_tab_id = pane.tabs.add(new_url.clone());
                app_state
                    .engines
                    .create_pane(new_tab_id, pane.tabs.active().url.clone(), None);
                app_state
                    .pending_wry_actions
                    .push_back(aileron::app::WryAction::Navigate(new_url));
                app_state.tabs.tab_display_dirty = true;
                app_state.ui.status_message =
                    format!("Tab {}/{}", pane.tabs.active_index() + 1, pane.tabs.len());
            }
        }

        frame_tasks::poll_git_status(&mut self.git_status, &self.git_poller);
        if let Some(app_state) = &mut self.app_state {
            app_state.adblock_blocked_count = self.adblocker.blocked_count();
            frame_tasks::auto_save_workspace(app_state, &self.wry_panes);
            #[cfg(feature = "arp")]
            frame_tasks::push_tabs_to_arp(app_state, &self.wry_panes);
            #[cfg(feature = "arp")]
            frame_tasks::process_arp_commands(app_state);
        }

        {
            let interval =
                std::time::Duration::from_secs(self.config.adblock_update_interval_hours * 3600);
            if self.last_filter_update.elapsed() >= interval {
                self.last_filter_update = std::time::Instant::now();
                // Run filter list HTTP downloads on a background thread to avoid blocking the UI.
                let reload_flag = self.adblock_reload_pending.clone();
                std::thread::spawn(move || {
                    let updated = aileron::net::filter_list::update_all_filter_lists();
                    if updated > 0 {
                        reload_flag.store(true, std::sync::atomic::Ordering::Release);
                        info!("Periodic filter list update: {} list(s) refreshed", updated);
                    }
                });
            }
            // Check if background thread finished updating filter lists
            if self
                .adblock_reload_pending
                .swap(false, std::sync::atomic::Ordering::Acquire)
            {
                frame_tasks::load_default_adblock_rules(&mut self.adblocker);
                if let Some(app_state) = &mut self.app_state {
                    app_state.ui.status_message = "Filter lists updated".into();
                }
            }
        }

        // Clone interceptor_registry once; share reference with both call sites.
        let interceptor_registry = self
            .app_state
            .as_ref()
            .map(|s| s.extension_manager.read().interceptor_registry.clone());

        {
            let app_state = match &mut self.app_state {
                Some(s) => s,
                None => return,
            };
            let registry = match &interceptor_registry {
                Some(r) => r,
                None => return,
            };
            frame_tasks::process_wry_events(
                app_state,
                &mut self.wry_panes,
                &self.content_scripts,
                #[cfg(feature = "mcp")]
                &mut self.mcp_bridge,
                &self.adblocker,
                registry,
            );
        }

        frame_tasks::process_pending_wry_actions(
            &mut self.app_state,
            &mut self.wry_panes,
            &mut self.offscreen_panes,
            &self.content_scripts,
        );

        if self.config.is_offscreen()
            && let Some(app_state) = &mut self.app_state
            && let Some(registry) = &interceptor_registry
        {
            frame_tasks::process_offscreen_events(
                app_state,
                &mut self.offscreen_panes,
                &self.content_scripts,
                #[cfg(feature = "mcp")]
                &mut self.mcp_bridge,
                &self.adblocker,
                registry,
            );
            // Check for webview crashes (stalled loading panes)
            frame_tasks::check_offscreen_crashes(app_state, &mut self.offscreen_panes);
        }

        let ws_name = self
            .app_state
            .as_mut()
            .and_then(|s| s.pending_workspace_restore.take());

        if let Some(ws_name) = ws_name {
            info!("Restoring workspace: {}", ws_name);
            if let Some(app_state) = self.app_state.as_mut() {
                app_state.current_workspace_name = ws_name.clone();
            }

            let viewport = match &self.window {
                Some(w) => {
                    let size = w.inner_size();
                    let scale = w.scale_factor();
                    Rect::new(
                        0.0,
                        0.0,
                        size.width as f64 / scale,
                        size.height as f64 / scale,
                    )
                }
                None => {
                    if let Some(app_state) = &mut self.app_state {
                        app_state.ui.status_message = "Restore failed: no window".into();
                    }
                    return;
                }
            };

            self.wry_panes.remove_all();
            self.offscreen_panes = OffscreenWebViewManager::new();
            self.webview_textures.clear();
            self.webview_texture_handles.clear();
            self.offscreen_last_capture.clear();
            self.pending_pane_creates.clear();

            let app_state = match &mut self.app_state {
                Some(s) => s,
                None => return,
            };

            let outcome = aileron::workspace_restore::restore_workspace(
                &ws_name,
                viewport,
                app_state.db.as_ref(),
                #[cfg(feature = "terminal")]
                &mut app_state.terminal_pane_ids,
                &mut app_state.engines,
                &mut app_state.wm,
                #[cfg(feature = "terminal")]
                &mut self.terminal_manager,
            );

            match outcome {
                aileron::workspace_restore::RestoreOutcome::Restored(result) => {
                    let active_id = app_state.wm.active_pane_id();
                    for (pid, url) in result.panes_to_create {
                        if pid == active_id {
                            self.create_wry_pane_for(pid, &url);
                        } else if self.config.is_offscreen() {
                            self.pending_pane_creates.push_back((pid, url));
                        } else {
                            self.create_wry_pane_for(pid, &url);
                        }
                    }
                    if let Some(s) = self.app_state.as_mut() {
                        s.ui.status_message = format!(
                            "Workspace restored: {} ({} panes)",
                            ws_name, result.pane_count
                        );
                    }
                }
                aileron::workspace_restore::RestoreOutcome::NotFound => {
                    if let Some(s) = self.app_state.as_mut() {
                        s.ui.status_message = format!("Workspace '{ws_name}' not found");
                    }
                }
                aileron::workspace_restore::RestoreOutcome::NoDatabase => {
                    if let Some(s) = self.app_state.as_mut() {
                        s.ui.status_message = "Restore failed: no database".into();
                    }
                }
                aileron::workspace_restore::RestoreOutcome::TreeError(e) => {
                    if let Some(s) = self.app_state.as_mut() {
                        s.ui.status_message = format!("Restore failed (tree): {e}");
                    }
                }
            }
        }

        #[cfg(feature = "mcp")]
        {
            let active_id = self
                .app_state
                .as_ref()
                .map(|s| s.wm.active_pane_id())
                .unwrap_or_default();
            if let Some(app_state) = self.app_state.as_mut() {
                frame_tasks::process_mcp_commands(
                    &self.mcp_bridge,
                    &mut self.wry_panes,
                    active_id,
                    app_state,
                    &mut self.offscreen_panes,
                );
            }
        }

        if let Some(close_id) = self
            .app_state
            .as_mut()
            .and_then(|s| s.pending_tab_close.take())
        {
            if let Some(app_state) = &mut self.app_state {
                frame_tasks::handle_pending_tab_close(app_state, close_id);
            }
            self.remove_wry_pane_for(&close_id);
            layout_dirty = true;
        }

        #[cfg(feature = "terminal")]
        frame_tasks::poll_terminal_output(&mut self.terminal_manager);

        // Handle pending bookmark import.
        if let Some(app_state) = &mut self.app_state {
            frame_tasks::handle_pending_import(app_state);
        }

        // Handle pending mark jumps (scroll to stored position).
        if let Some(app_state) = &mut self.app_state
            && let Some(frac) = app_state.session.pending_mark_jump.take()
        {
            let active_id = app_state.wm.active_pane_id();
            if let Some(pane) = self.offscreen_panes.get_mut(&active_id) {
                let js =
                    format!("window.scrollTo(0, document.documentElement.scrollHeight * {frac})");
                pane.execute_js(&js);
                pane.mark_dirty();
            }
        }

        // Memory limit enforcement: evict least-recently active background tab
        // when process RSS exceeds the configured limit. Runs once per ~60 frames
        // to avoid per-frame syscall overhead.
        if self.config.memory_limit_mb > 0
            && self.frame_count.is_multiple_of(60)
            && let Some(rss_bytes) = aileron::profiling::memory::process_rss_bytes()
        {
            let limit_bytes = self.config.memory_limit_mb * 1024 * 1024;
            if rss_bytes > limit_bytes {
                // Collect eviction info without holding mutable borrow.
                let evict_info = self.app_state.as_ref().and_then(|app_state| {
                    let evict_id = app_state.find_lru_pane()?;
                    let url = self
                        .wry_panes
                        .get(&evict_id)
                        .map(|p| p.url().to_string())
                        .unwrap_or_default();
                    Some((evict_id, url))
                });

                if let Some((evict_id, url)) = evict_info {
                    info!(
                        "Memory limit exceeded (RSS {} > {} MB): evicting tab {}",
                        aileron::profiling::memory::format_human_bytes(rss_bytes),
                        self.config.memory_limit_mb,
                        evict_id
                    );
                    self.remove_wry_pane_for(&evict_id);
                    if let Some(app_state) = &mut self.app_state {
                        app_state
                            .pending_wry_actions
                            .push_back(aileron::app::WryAction::Navigate(
                                url::Url::parse(&url)
                                    .unwrap_or_else(|_| url::Url::parse("aileron://new").unwrap()),
                            ));
                        app_state.ui.status_message = format!(
                            "Memory limit reached — evicted background tab ({})",
                            aileron::profiling::memory::format_human_bytes(rss_bytes)
                        );
                    }
                    layout_dirty = true;
                }
            }
        }

        // Single reposition_all_panes() call per frame (was up to 3x).
        if layout_dirty {
            self.reposition_all_panes();
        }
        #[cfg(target_os = "linux")]
        frame_tasks::pump_gtk_loop();

        // TASK-K27: create at most one deferred offscreen pane per frame.
        self.drain_pending_pane_creates();

        // Architecture B: capture dirty offscreen frames and update egui textures.
        #[cfg(target_os = "linux")]
        let textures_updated = self.update_webview_textures();
        #[cfg(not(target_os = "linux"))]
        let textures_updated: bool = false;

        // Record frame end for input latency measurement.
        if let Some(app_state) = &mut self.app_state {
            app_state.input_latency.record_frame_end();
            app_state.profiler.end_frame("about_to_wait");
        }

        // Request redraw if:
        // 1. egui explicitly requested a repaint (UI interaction), OR
        // 2. A webview texture was updated (new frame from offscreen webview), OR
        // 3. We have offscreen panes (continuous repaint for async web content)
        if let Some(winit_state) = &self.egui_winit {
            let egui_ctx = winit_state.egui_ctx();
            let needs_repaint = egui_ctx.has_requested_repaint()
                || textures_updated
                || !self.offscreen_panes.is_empty();
            if needs_repaint && let Some(window) = &self.window {
                window.request_redraw();
            }
        }
    }

    fn new_events(&mut self, event_loop: &ActiveEventLoop, _cause: winit::event::StartCause) {
        if self.popup.pending_new_window {
            self.popup.pending_new_window = false;
            let window = Arc::new(
                event_loop
                    .create_window(
                        WindowAttributes::default()
                            .with_title("Aileron")
                            .with_inner_size(winit::dpi::LogicalSize::new(
                                self.config.window_width,
                                self.config.window_height,
                            )),
                    )
                    .expect("Failed to create popup window"),
            );
            let window_id = window.id();
            self.popup.pending_popup_window = Some((window_id, window));
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        info!("Clean shutdown — clearing session-active flag");
        Config::clear_session_active();

        // Drop GPU state while the Wayland display is still alive.
        // NVIDIA's libnvidia-egl-wayland2.so crashes in eglTerminate()
        // if called after the wl_surface has been destroyed. The implicit
        // Drop of AileronApp drops `window` (field 1) before `gfx` (field 3),
        // so the wl_surface is gone before EGL cleanup runs. By dropping
        // gfx and egui_winit here, the EGL teardown happens while the
        // Wayland connection and surface are still valid.
        self.gfx = None;
        self.egui_winit = None;
    }
}
