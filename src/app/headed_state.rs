//! Headed-only state (S4 stage 1b).
//!
//! Everything the agent runtime never touches: keyboard modes, palette,
//! macros, UI sub-states, profilers, and the pending-action queues the
//! headed render loop consumes. Held by `AppState` behind
//! `Option<HeadedState>` (`None` in agent mode).

use std::collections::{HashMap, HashSet};

use uuid::Uuid;

use super::{AutofillState, CrashRecoveryState, PanelState, TabState};
use crate::input::Mode;
use crate::input::keybindings::KeybindingRegistry;
use crate::input::macro_recorder::{MacroPlayer, MacroRecorder, RecordedEvent};
use crate::profiling::{InputLatencyTracker, Profiler};
use crate::ui::palette::CommandPalette;

/// Headed-only state. Access via `AppState::headed` / `AppState::headed_mut`.
pub struct HeadedState {
    /// Keyboard mode (headed input system).
    pub mode: Mode,
    pub keybindings: KeybindingRegistry,
    /// Command palette state.
    pub palette: CommandPalette,
    /// U1-06: Keyboard macro recorder.
    pub macro_recorder: MacroRecorder,
    /// U1-06: Keyboard macro player.
    pub macro_player: MacroPlayer,
    /// U1-06: Last recorded macro events (pending save).
    pub last_macro_events: Option<Vec<RecordedEvent>>,
    /// When true, the next about_to_wait iteration requests a new popup window.
    pub pending_new_window: bool,
    /// URL to navigate a popup window to after creation (from pane detach).
    pub pending_detach_url: Option<url::Url>,
    /// Pending bookmark import: "firefox" or "chrome".
    pub pending_import: Option<String>,
    /// Pending URL to open in a new tab (set by `:g <url>` command).
    pub pending_new_tab_url: Option<url::Url>,
    /// Set to true when the UI requests a new tab (e.g., "+" button).
    pub pending_new_tab: bool,
    /// Per-pane tracking of already-injected content script IDs.
    /// Keys are pane IDs, values are sets of "extension_id:script_id" strings.
    /// Cleared on each LoadStarted to allow re-injection on new navigations.
    pub injected_content_script_ids: HashMap<Uuid, HashSet<String>>,
    /// Per-pane last-focus timestamp for LRU tab unloading.
    /// Updated each time a pane becomes active.
    pub pane_last_focus: HashMap<Uuid, std::time::Instant>,
    /// Adblock blocked request count (updated by main.rs each frame).
    pub adblock_blocked_count: u64,
    /// Tracks key-to-frame latency for profiling.
    pub input_latency: InputLatencyTracker,
    /// Frame timing profiler — collects per-phase duration samples.
    /// Exposed on HeadedState so `:stats` command can read stats.
    pub profiler: Profiler,
    pub panels: PanelState,
    pub tabs: TabState,
    pub crash: CrashRecoveryState,
    pub autofill: AutofillState,
}

impl HeadedState {
    /// Build headed state. Reads history/bookmarks/tab names from `db`
    /// to seed the palette and tab metadata.
    pub fn new(
        db: Option<&rusqlite::Connection>,
        config: &crate::config::Config,
    ) -> anyhow::Result<Self> {
        let mut keybindings = KeybindingRegistry::default();

        // Apply custom keybinding overrides from config
        if !config.keybindings.is_empty() {
            let applied = keybindings.apply_config_overrides(&config.keybindings);
            if applied > 0 {
                tracing::info!("Applied {} custom keybinding(s)", applied);
            }
        }

        // Build command palette with history + bookmarks from DB
        let mut palette = CommandPalette::new();
        if let Some(conn) = db {
            // History items
            if let Ok(entries) = crate::db::history::recent_entries(conn, 50) {
                for entry in entries {
                    palette.add_item(crate::ui::search::SearchItem {
                        id: format!("history:{}", entry.id),
                        label: entry.title.clone(),
                        description: entry.url.clone(),
                        category: crate::ui::search::SearchCategory::History,
                    });
                }
            }
            // Bookmark items
            if let Ok(bm_list) = crate::db::bookmarks::all_bookmarks(conn) {
                for bm in bm_list {
                    palette.add_item(crate::ui::search::SearchItem {
                        id: format!("bookmark:{}", bm.id),
                        label: bm.title.clone(),
                        description: bm.url.clone(),
                        category: crate::ui::search::SearchCategory::Bookmark,
                    });
                }
            }
        }

        // Load tab names from database
        let tab_names = if let Some(conn) = db {
            crate::db::tab_names::load_tab_names(conn).unwrap_or_default()
        } else {
            HashMap::new()
        };

        Ok(Self {
            mode: Mode::Normal,
            keybindings,
            palette,
            macro_recorder: MacroRecorder::new(),
            macro_player: MacroPlayer::new(),
            last_macro_events: None,
            pending_new_window: false,
            pending_detach_url: None,
            pending_import: None,
            pending_new_tab_url: None,
            pending_new_tab: false,
            injected_content_script_ids: HashMap::new(),
            pane_last_focus: HashMap::new(),
            adblock_blocked_count: 0,
            input_latency: InputLatencyTracker::new(),
            profiler: {
                let mut p = Profiler::new();
                p.enable();
                p
            },
            panels: PanelState::default(),
            tabs: TabState {
                tab_names,
                ..Default::default()
            },
            crash: CrashRecoveryState::default(),
            autofill: AutofillState::default(),
        })
    }
}
