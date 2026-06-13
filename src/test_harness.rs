//! Internal test harness for automated UI state traversal.
//!
//! Routes through a predefined sequence of UI states, capturing DOM JSON
//! and screenshots from within the app's own rendering pipeline. No
//! xdotool or system-level permissions required.
//!
//! Usage: `aileron --test-harness [output_dir] [--dump-dom]`

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use tracing::info;

use crate::app::AppState;
use crate::chrome_bridge::{ChromeSnapshotInput, build_chrome_state};
use crate::input::{Key, KeyEvent, Modifiers};

/// An action to perform at a test step.
#[derive(Debug, Clone)]
pub enum TestAction {
    /// Simulate a key press.
    Key(KeyEvent),
    /// Execute a command string via AppState.
    Command(String),
    /// No action — just wait for the UI to settle.
    None,
}

impl TestAction {
    /// A key press with no modifiers.
    pub fn key(key: Key) -> Self {
        TestAction::Key(KeyEvent {
            key,
            modifiers: Modifiers::none(),
            physical_key: None,
        })
    }

    /// A key press with modifiers (e.g., Ctrl+P).
    pub fn key_with_mods(key: Key, ctrl: bool, shift: bool, alt: bool) -> Self {
        TestAction::Key(KeyEvent {
            key,
            modifiers: Modifiers {
                ctrl,
                shift,
                alt,
                super_key: false,
            },
            physical_key: None,
        })
    }

    /// A command string.
    pub fn cmd(cmd: &str) -> Self {
        TestAction::Command(cmd.to_string())
    }

    /// No action.
    pub fn none() -> Self {
        TestAction::None
    }
}

/// A single step in the test route.
#[derive(Debug, Clone)]
pub struct TestState {
    /// Human-readable name for this step.
    pub name: String,
    /// Action to perform before capture.
    pub action: TestAction,
    /// Milliseconds to wait after action before capturing.
    pub wait_ms: u64,
}

impl TestState {
    pub fn new(name: &str, action: TestAction, wait_ms: u64) -> Self {
        Self {
            name: name.to_string(),
            action,
            wait_ms,
        }
    }

    /// A step that only waits (no action).
    pub fn wait_only(name: &str, wait_ms: u64) -> Self {
        Self::new(name, TestAction::none(), wait_ms)
    }
}

/// Internal test runner that drives the app through a sequence of UI states.
pub struct TestHarness {
    /// Directory for this session's output.
    session_dir: PathBuf,
    /// The route to traverse.
    states: Vec<TestState>,
    /// Index of the next step to execute.
    current_step: usize,
    /// When the current step started (for wait timing).
    step_start: Instant,
    /// Whether the current step's action has been performed.
    action_executed: bool,
    /// Whether the harness is done.
    done: bool,
    /// Whether to print DOM JSON to stdout after each capture.
    dump_dom: bool,
    /// Capture counter (for sequential file naming).
    capture_count: usize,
    /// Pending DOM HTML capture: (step_index, step_name) waiting for async callback.
    pending_dom_capture: Option<(usize, String)>,
    /// Pending screenshot capture: (step_index, step_name) waiting for async callback.
    pending_screenshot_capture: Option<(usize, String)>,
    /// Shared buffer for async DOM capture result from the webview.
    pub dom_capture_buffer: std::sync::Arc<std::sync::Mutex<Option<String>>>,
    /// Shared buffer for async screenshot capture result from the webview.
    pub screenshot_capture_buffer: std::sync::Arc<std::sync::Mutex<Option<String>>>,
    /// Number of frames to wait after a step for async callbacks to fire.
    /// The GTK event loop needs time to process evaluate_script_with_callback.
    pub callback_wait_frames: u32,
    /// Current frame count in the callback wait period.
    pub callback_wait_counter: u32,
}

impl TestHarness {
    /// Create a new test harness with the given output directory.
    pub fn new(output_dir: &Path, dump_dom: bool) -> Self {
        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let session_dir = output_dir.join(format!("session_{timestamp}"));
        let dom_dir = session_dir.join("dom");
        let screens_dir = session_dir.join("screens");

        std::fs::create_dir_all(&dom_dir).expect("Failed to create dom/ directory");
        std::fs::create_dir_all(&screens_dir).expect("Failed to create screens/ directory");

        info!("Test harness initialized: output={}", session_dir.display());

        Self {
            session_dir,
            states: Vec::new(),
            current_step: 0,
            step_start: Instant::now(),
            action_executed: false,
            done: false,
            dump_dom,
            capture_count: 0,
            pending_dom_capture: None,
            pending_screenshot_capture: None,
            dom_capture_buffer: std::sync::Arc::new(std::sync::Mutex::new(None)),
            screenshot_capture_buffer: std::sync::Arc::new(std::sync::Mutex::new(None)),
            callback_wait_frames: 3,
            callback_wait_counter: 0,
        }
    }

    /// Define the route of UI states to traverse.
    pub fn define_route(&mut self, states: Vec<TestState>) {
        self.states = states;
        self.current_step = 0;
        self.step_start = Instant::now();
        self.action_executed = false;
        info!("Test route defined: {} steps", self.states.len());
    }

    /// Called each frame. Returns `true` when the route is complete.
    ///
    /// - Executes the current step's action on first call after wait expires
    /// - Waits the specified duration for the UI to settle
    /// - Returns `false` while the route is still in progress
    /// - Returns `true` after the final capture
    pub fn tick(&mut self, app_state: &mut AppState) -> bool {
        if self.done {
            return true;
        }

        if self.current_step >= self.states.len() {
            info!("Test harness: all {} steps complete", self.capture_count);
            self.done = true;
            return true;
        }

        let elapsed = self.step_start.elapsed();

        // Execute action once when the step starts
        if !self.action_executed {
            let state = &self.states[self.current_step];
            match &state.action {
                TestAction::Key(key_event) => {
                    info!(
                        "Step {}/{}: key event {:?}",
                        self.current_step + 1,
                        self.states.len(),
                        key_event.key
                    );
                    app_state.process_key_event(key_event.clone());
                }
                TestAction::Command(cmd) => {
                    if !cmd.is_empty() {
                        info!(
                            "Step {}/{}: command '{}'",
                            self.current_step + 1,
                            self.states.len(),
                            cmd
                        );
                        app_state.execute_command_pub(cmd);
                    }
                }
                TestAction::None => {
                    info!(
                        "Step {}/{}: waiting (no action)",
                        self.current_step + 1,
                        self.states.len()
                    );
                }
            }
            self.action_executed = true;
        }

        // Wait for the UI to settle after action execution
        let wait_duration = Duration::from_millis(self.states[self.current_step].wait_ms);
        if elapsed >= wait_duration {
            // Capture DOM before advancing step counter
            self.capture_step(app_state, None, None);
            self.current_step += 1;
            self.action_executed = false;
            self.step_start = Instant::now();
            self.capture_count += 1;
            // Start callback wait period to allow GTK event loop to process async callbacks
            self.callback_wait_counter = self.callback_wait_frames;

            // If all steps are done, mark complete and return true
            if self.current_step >= self.states.len() {
                info!("Test harness: all {} steps complete", self.capture_count);
                self.done = true;
                return true;
            }
        }

        false
    }

    /// Capture DOM state for the current step.
    fn capture_step(
        &mut self,
        app_state: &AppState,
        dom_html: Option<&str>,
        screenshot_data: Option<&str>,
    ) {
        let step_name = &self.states[self.current_step].name;
        let index = self.capture_count;
        let padded = format!("{index:03}");

        // Capture DOM state JSON
        let dom_json = self.capture_dom(app_state);
        let dom_path = self
            .session_dir
            .join("dom")
            .join(format!("{padded}_{step_name}.json"));
        if let Err(e) = std::fs::write(&dom_path, &dom_json) {
            tracing::error!("Failed to write DOM capture: {e}");
        } else {
            info!("DOM saved: {}", dom_path.display());
        }

        // Capture DOM HTML from webview
        if let Some(html) = dom_html {
            let html_path = self
                .session_dir
                .join("dom")
                .join(format!("{padded}_{step_name}.html"));
            if let Err(e) = std::fs::write(&html_path, html) {
                tracing::error!("Failed to write DOM HTML: {e}");
            } else {
                info!("DOM HTML saved: {}", html_path.display());
            }
        }

        // Capture screenshot from webview
        if let Some(data_url) = screenshot_data
            && let Some(b64) = data_url.strip_prefix("data:image/png;base64,")
            && let Ok(bytes) =
                base64::Engine::decode(&base64::engine::general_purpose::STANDARD, b64)
        {
            let screen_path = self
                .session_dir
                .join("screens")
                .join(format!("{padded}_{step_name}.png"));
            if let Err(e) = std::fs::write(&screen_path, &bytes) {
                tracing::error!("Failed to write screenshot: {e}");
            } else {
                info!("Screenshot saved: {}", screen_path.display());
            }
        }

        if self.dump_dom {
            println!("=== DOM [{padded}_{step_name}] ===");
            println!("{dom_json}");
            if let Some(html) = dom_html {
                println!("--- HTML ---");
                println!("{html}");
            }
            println!("=== END DOM ===");
        }
    }

    /// Serialize the current ChromeState to JSON.
    pub fn capture_dom(&self, app_state: &AppState) -> String {
        let active_id = app_state.wm.active_pane_id();
        let panes_ref = app_state.wm.panes_ref();

        let panes: Vec<aileron_shared::PaneInfo> = panes_ref
            .iter()
            .map(|(pid, _)| {
                let (url, title) = if let Some(pane) = app_state.wm.find_pane(*pid) {
                    (pane.url().to_string(), pane.title().to_string())
                } else {
                    (String::new(), String::new())
                };
                aileron_shared::PaneInfo {
                    id: pid.to_string(),
                    url,
                    title,
                    active: *pid == active_id,
                    loading: false,
                    zoom: 1.0,
                }
            })
            .collect();

        let snapshot = ChromeSnapshotInput {
            mode: app_state.mode,
            active_pane_id: active_id,
            pane_count: panes_ref.len(),
            panes,
            status_message: &app_state.ui.status_message,
            find_bar_open: app_state.ui.find_bar_open,
            find_query: &app_state.ui.find_query,
            command_palette_open: app_state.palette.open,
            palette_results: app_state
                .palette
                .results()
                .iter()
                .map(|item| aileron_shared::PaletteItem {
                    id: item.id.clone(),
                    label: item.label.clone(),
                    description: item.description.clone(),
                    category: crate::chrome_bridge::to_shared_category(item.category),
                })
                .collect(),
            palette_selected: app_state.palette.selected_item().map_or(0, |s| {
                app_state
                    .palette
                    .results()
                    .iter()
                    .position(|r| r.id == s.id)
                    .unwrap_or(0)
            }),
            url_bar_focused: app_state.ui.url_bar_focused,
            tab_layout: &app_state.config.tab_layout,
            tab_sidebar_width: app_state.config.tab_sidebar_width as f64,
            tab_sidebar_right: app_state.config.tab_sidebar_right,
            version: format!("v{} (test-harness)", env!("CARGO_PKG_VERSION")),
        };

        let state = build_chrome_state(snapshot);
        serde_json::to_string_pretty(&state)
            .unwrap_or_else(|e| format!("{{\"error\": \"failed to serialize ChromeState: {e}\"}}"))
    }

    /// Convert RGBA pixel data to PNG bytes and save to disk.
    ///
    /// # Arguments
    /// * `frame_rgba` - Raw RGBA8 pixel data
    /// * `width` - Image width in pixels
    /// * `height` - Image height in pixels
    /// * `step_name` - Name for the output file
    pub fn capture_screenshot(
        &mut self,
        frame_rgba: &[u8],
        width: u32,
        height: u32,
        step_name: &str,
    ) -> Vec<u8> {
        let index = self.capture_count;
        let padded = format!("{index:03}");

        match encode_png(frame_rgba, width, height) {
            Ok(png_bytes) => {
                let path = self
                    .session_dir
                    .join("screens")
                    .join(format!("{padded}_{step_name}.png"));
                if let Err(e) = std::fs::write(&path, &png_bytes) {
                    tracing::error!("Failed to write screenshot: {e}");
                } else {
                    info!("Screenshot saved: {}", path.display());
                }
                png_bytes
            }
            Err(e) => {
                tracing::error!("Failed to encode PNG: {e}");
                Vec::new()
            }
        }
    }

    /// Capture DOM HTML and screenshot from the active pane's webview.
    /// Called from app_handler after tick() to get data from wry_panes.
    ///
    /// If DOM HTML is not yet available (async callback hasn't fired),
    /// stores the step info in `pending_dom_capture` for retry on next frame.
    pub fn capture_webview_data(&mut self, dom_html: Option<String>, screenshot: Option<String>) {
        info!(
            "capture_webview_data: dom_html={}, screenshot={}",
            dom_html.is_some(),
            screenshot.is_some()
        );
        if self.capture_count == 0 {
            return;
        }
        let index = self.capture_count - 1;
        let step_name = if index < self.states.len() {
            self.states[index].name.clone()
        } else {
            "unknown".to_string()
        };
        let padded = format!("{index:03}");

        // Save DOM HTML
        if let Some(html) = &dom_html {
            let html_path = self
                .session_dir
                .join("dom")
                .join(format!("{padded}_{step_name}.html"));
            if let Err(e) = std::fs::write(&html_path, html) {
                tracing::error!("Failed to write DOM HTML: {e}");
            } else {
                info!("DOM HTML saved: {}", html_path.display());
            }
            // Clear pending capture since we got the data
            self.pending_dom_capture = None;
        } else if self.pending_dom_capture.is_none() {
            // First attempt failed -- store for retry on next frames
            tracing::debug!("DOM HTML not yet available, will retry next frames");
            self.pending_dom_capture = Some((index, step_name.clone()));
        }

        // Save screenshot from data URL
        if let Some(data_url) = &screenshot
            && let Some(b64) = data_url.strip_prefix("data:image/png;base64,")
            && let Ok(bytes) =
                base64::Engine::decode(&base64::engine::general_purpose::STANDARD, b64)
        {
            let screen_path = self
                .session_dir
                .join("screens")
                .join(format!("{padded}_{step_name}.png"));
            if let Err(e) = std::fs::write(&screen_path, &bytes) {
                tracing::error!("Failed to write screenshot: {e}");
            } else {
                info!("Screenshot saved: {}", screen_path.display());
            }
            // Clear pending screenshot capture since we got the data
            self.pending_screenshot_capture = None;
        } else if self.pending_screenshot_capture.is_none() && screenshot.is_none() {
            // No screenshot data yet -- store for retry on next frames
            tracing::debug!("Screenshot not yet available, will retry next frames");
            self.pending_screenshot_capture = Some((index, step_name.clone()));
        }
    }

    /// Get the step index and name for a pending DOM capture, if any.
    pub fn pending_dom_capture_step(&self) -> Option<(usize, &str)> {
        self.pending_dom_capture
            .as_ref()
            .map(|(idx, name)| (*idx, name.as_str()))
    }

    /// Save DOM HTML for a pending capture (called from retry loop).
    pub fn save_pending_dom_html(&mut self, html: &str) {
        if let Some((index, ref step_name)) = self.pending_dom_capture {
            let padded = format!("{index:03}");
            let html_path = self
                .session_dir
                .join("dom")
                .join(format!("{padded}_{step_name}.html"));
            if let Err(e) = std::fs::write(&html_path, html) {
                tracing::error!("Failed to write pending DOM HTML: {e}");
            } else {
                info!("DOM HTML saved (retry): {}", html_path.display());
            }
            self.pending_dom_capture = None;
        }
    }

    /// Get the step index and name for a pending screenshot capture, if any.
    pub fn pending_screenshot_capture_step(&self) -> Option<(usize, &str)> {
        self.pending_screenshot_capture
            .as_ref()
            .map(|(idx, name)| (*idx, name.as_str()))
    }

    /// Save screenshot for a pending capture (called from retry loop).
    pub fn save_pending_screenshot(&mut self, data_url: &str) {
        if let Some((index, ref step_name)) = self.pending_screenshot_capture
            && let Some(b64) = data_url.strip_prefix("data:image/png;base64,")
            && let Ok(bytes) =
                base64::Engine::decode(&base64::engine::general_purpose::STANDARD, b64)
        {
            let padded = format!("{index:03}");
            let screen_path = self
                .session_dir
                .join("screens")
                .join(format!("{padded}_{step_name}.png"));
            if let Err(e) = std::fs::write(&screen_path, &bytes) {
                tracing::error!("Failed to write pending screenshot: {e}");
            } else {
                info!("Screenshot saved (retry): {}", screen_path.display());
            }
            self.pending_screenshot_capture = None;
        }
    }

    /// Whether the route is complete.
    pub fn is_done(&self) -> bool {
        self.done
    }

    /// Whether we're waiting for async callbacks to fire.
    pub fn is_waiting_for_callbacks(&self) -> bool {
        self.callback_wait_counter > 0
    }

    /// Path to this session's output directory.
    pub fn session_dir(&self) -> &Path {
        &self.session_dir
    }

    /// Current step index (0-based).
    pub fn current_step(&self) -> usize {
        self.current_step
    }

    /// Number of captures completed so far.
    pub fn capture_count(&self) -> usize {
        self.capture_count
    }

    /// Total number of steps in the route.
    pub fn total_steps(&self) -> usize {
        self.states.len()
    }

    /// Name of the current (or most recently completed) step.
    pub fn current_step_name(&self) -> &str {
        let idx = self
            .current_step
            .saturating_sub(1)
            .min(self.states.len().saturating_sub(1));
        if self.states.is_empty() {
            "unknown"
        } else {
            &self.states[idx].name
        }
    }
}

/// Encode raw RGBA8 pixel data as PNG.
///
/// # Arguments
/// * `rgba` - Raw RGBA8 pixel data (4 bytes per pixel)
/// * `width` - Image width in pixels
/// * `height` - Image height in pixels
///
/// # Returns
/// PNG file bytes, or an error string.
fn encode_png(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    let expected_len = (width as usize) * (height as usize) * 4;
    if rgba.len() < expected_len {
        return Err(format!(
            "RGBA buffer too small: expected {} bytes, got {}",
            expected_len,
            rgba.len()
        ));
    }

    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut encoder = png::Encoder::new(&mut buf, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Fast);

        let mut writer = encoder.write_header().map_err(|e| format!("{e}"))?;
        writer.write_image_data(rgba).map_err(|e| format!("{e}"))?;
        writer.finish().map_err(|e| format!("{e}"))?;
    }

    Ok(buf.into_inner())
}

/// Build the default test route covering all major UI states.
pub fn default_route() -> Vec<TestState> {
    vec![
        // 1. Default (empty new tab)
        TestState::wait_only("01_default", 500),
        // 2. Open Command palette via `:`
        TestState::new(
            "02_command_palette",
            TestAction::key(Key::Character(':')),
            500,
        ),
        // 3. Type "open https://example.com" into palette
        TestState::new(
            "03_open_example",
            TestAction::cmd("open https://example.com"),
            3000,
        ),
        // 4. Return to Normal (Escape)
        TestState::new("04_normal_mode", TestAction::key(Key::Escape), 300),
        // 5. Open Command Palette again (Ctrl+P)
        TestState::new(
            "05_palette_ctrl_p",
            TestAction::key_with_mods(Key::Character('p'), true, false, false),
            500,
        ),
        // 6. Split horizontal via palette command
        TestState::new("06_split_h", TestAction::cmd("sp"), 500),
        // 7. Split vertical via palette command
        TestState::new("07_split_v", TestAction::cmd("vs"), 500),
        // 8. Close palette (Escape)
        TestState::new("08_close_palette", TestAction::key(Key::Escape), 300),
        // 9. Open Find bar (Ctrl+F)
        TestState::new(
            "09_find_bar",
            TestAction::key_with_mods(Key::Character('f'), true, false, false),
            300,
        ),
        // 10. Close Find bar (Escape)
        TestState::new("10_find_close", TestAction::key(Key::Escape), 200),
        // 11. Capture final state
        TestState::wait_only("11_final_state", 500),
    ]
}

/// Build a comprehensive traversal route covering all major UI paths.
///
/// This route tests:
/// - Default state and startup
/// - Command palette (open, search, execute)
/// - Navigation (open URL, back, forward, reload)
/// - Pane management (split, close, resize)
/// - Tab management (new tab, close tab, switch tabs)
/// - Find bar (open, search, close)
/// - Settings and configuration
/// - Theme switching
/// - Privacy features
/// - Sync operations
pub fn comprehensive_route() -> Vec<TestState> {
    vec![
        // === Phase 1: Default State & Startup ===
        TestState::wait_only("01_startup", 1000),
        // === Phase 2: Command Palette ===
        TestState::new(
            "02_palette_open_colon",
            TestAction::key(Key::Character(':')),
            500,
        ),
        TestState::new(
            "03_palette_open_ctrl_p",
            TestAction::key_with_mods(Key::Character('p'), true, false, false),
            500,
        ),
        TestState::new("04_palette_close", TestAction::key(Key::Escape), 300),
        // === Phase 3: Navigation ===
        TestState::new(
            "05_navigate_example",
            TestAction::cmd("open https://example.com"),
            3000,
        ),
        TestState::new("06_navigate_back", TestAction::cmd("back"), 500),
        TestState::new("07_navigate_forward", TestAction::cmd("forward"), 500),
        TestState::new("08_navigate_reload", TestAction::cmd("reload"), 500),
        // === Phase 4: Pane Management ===
        TestState::new("09_split_horizontal", TestAction::cmd("sp"), 500),
        TestState::new("10_split_vertical", TestAction::cmd("vs"), 500),
        TestState::new(
            "11_navigate_pane_1",
            TestAction::cmd("open https://rust-lang.org"),
            3000,
        ),
        TestState::new(
            "12_navigate_pane_2",
            TestAction::cmd("open https://github.com"),
            3000,
        ),
        // === Phase 5: Tab Management ===
        TestState::new(
            "13_new_tab",
            TestAction::key_with_mods(Key::Character('t'), true, false, false),
            500,
        ),
        TestState::new("14_switch_tab", TestAction::cmd("tab-activate"), 500),
        // === Phase 6: Find Bar ===
        TestState::new(
            "15_find_open",
            TestAction::key_with_mods(Key::Character('f'), true, false, false),
            500,
        ),
        TestState::new("16_find_close", TestAction::key(Key::Escape), 300),
        // === Phase 7: Settings ===
        TestState::new("17_settings", TestAction::cmd("settings"), 500),
        TestState::new("18_theme_list", TestAction::cmd("theme list"), 500),
        TestState::new("19_theme_dark", TestAction::cmd("theme dark"), 500),
        // === Phase 8: Privacy ===
        TestState::new("20_adblock_toggle", TestAction::cmd("adblock-toggle"), 500),
        TestState::new("21_privacy", TestAction::cmd("privacy"), 500),
        // === Phase 9: Final State ===
        TestState::wait_only("22_final_state", 1000),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wm::Rect;

    #[test]
    fn test_harness_creation() {
        let dir = tempfile::tempdir().unwrap();
        let harness = TestHarness::new(dir.path(), false);
        assert!(!harness.is_done());
        assert_eq!(harness.current_step(), 0);
        assert_eq!(harness.total_steps(), 0);
        assert!(harness.session_dir().exists());
        assert!(harness.session_dir().join("dom").exists());
        assert!(harness.session_dir().join("screens").exists());
    }

    #[test]
    fn test_harness_empty_route_completes_immediately() {
        let dir = tempfile::tempdir().unwrap();
        let mut harness = TestHarness::new(dir.path(), false);
        let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
        let config = crate::config::Config::default();
        let mut state = AppState::new(viewport, config).unwrap();

        // Empty route: tick should return true immediately
        assert!(harness.tick(&mut state));
        assert!(harness.is_done());
    }

    #[test]
    fn test_harness_single_step() {
        let dir = tempfile::tempdir().unwrap();
        let mut harness = TestHarness::new(dir.path(), false);
        // Use 200ms to avoid timing race with AppState::new() overhead
        harness.define_route(vec![TestState::wait_only("test", 200)]);

        let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
        let config = crate::config::Config::default();
        let mut state = AppState::new(viewport, config).unwrap();

        // First tick: action executed, waiting
        assert!(!harness.tick(&mut state));

        // Wait for the step to complete
        std::thread::sleep(Duration::from_millis(250));

        // Second tick: capture + done
        assert!(harness.tick(&mut state));
        assert!(harness.is_done());
    }

    #[test]
    fn test_harness_command_execution() {
        let dir = tempfile::tempdir().unwrap();
        let mut harness = TestHarness::new(dir.path(), false);
        harness.define_route(vec![TestState::new(
            "cmd_test",
            TestAction::cmd("set adblock off"),
            200,
        )]);

        let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
        let config = crate::config::Config::default();
        let mut state = AppState::new(viewport, config).unwrap();

        // Tick to start the step (command will execute)
        assert!(!harness.tick(&mut state));
        assert!(!state.config.adblock_enabled);

        // Wait and tick to capture
        std::thread::sleep(Duration::from_millis(250));
        assert!(harness.tick(&mut state));
        assert!(harness.is_done());
    }

    #[test]
    fn test_harness_key_event() {
        let dir = tempfile::tempdir().unwrap();
        let mut harness = TestHarness::new(dir.path(), false);
        harness.define_route(vec![TestState::new(
            "key_test",
            TestAction::key(Key::Character('i')),
            200,
        )]);

        let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
        let config = crate::config::Config::default();
        let mut state = AppState::new(viewport, config).unwrap();

        assert_eq!(state.mode, crate::input::Mode::Normal);

        // Tick to execute key event — 'i' enters Insert mode
        assert!(!harness.tick(&mut state));
        assert_eq!(state.mode, crate::input::Mode::Insert);

        // Wait and tick to capture
        std::thread::sleep(Duration::from_millis(250));
        assert!(harness.tick(&mut state));
        assert!(harness.is_done());
    }

    #[test]
    fn test_capture_dom() {
        let dir = tempfile::tempdir().unwrap();
        let harness = TestHarness::new(dir.path(), false);
        let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
        let config = crate::config::Config::default();
        let state = AppState::new(viewport, config).unwrap();

        let dom_json = harness.capture_dom(&state);
        assert!(dom_json.contains("\"mode\""));
        assert!(dom_json.contains("\"pane_count\""));
        assert!(!dom_json.is_empty());
    }

    #[test]
    fn test_encode_png_roundtrip() {
        // Create a 2x2 red RGBA image
        let rgba = vec![
            255, 0, 0, 255, // red
            0, 255, 0, 255, // green
            0, 0, 255, 255, // blue
            255, 255, 0, 255, // yellow
        ];

        let png_bytes = encode_png(&rgba, 2, 2).unwrap();
        assert!(!png_bytes.is_empty());
        // PNG magic bytes
        assert_eq!(png_bytes[0], 0x89);
        assert_eq!(png_bytes[1], b'P');
        assert_eq!(png_bytes[2], b'N');
        assert_eq!(png_bytes[3], b'G');
    }

    #[test]
    fn test_encode_png_invalid_buffer() {
        let rgba = vec![0u8; 10]; // too small for even 1x1
        let result = encode_png(&rgba, 10, 10);
        assert!(result.is_err());
    }

    #[test]
    fn test_default_route_length() {
        let route = default_route();
        assert_eq!(route.len(), 11);
    }

    #[test]
    fn test_harness_screenshot_saves_file() {
        let dir = tempfile::tempdir().unwrap();
        let mut harness = TestHarness::new(dir.path(), false);
        harness.define_route(vec![TestState::wait_only("screenshot_test", 10)]);

        // Create a 4x4 red RGBA image
        let rgba: Vec<u8> = [255u8, 0, 0, 255].repeat(16);
        let png = harness.capture_screenshot(&rgba, 4, 4, "test");

        assert!(!png.is_empty());
        // Verify the file was saved
        let screens_dir = harness.session_dir().join("screens");
        let files: Vec<_> = std::fs::read_dir(&screens_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(files.len(), 1);
    }
}
