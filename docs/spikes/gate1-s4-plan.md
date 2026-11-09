
## Stage 1a executed (2026-11-06): aileron-core leaf modules

The manifest's ordering was corrected by the dependency audit: **core
extraction precedes engine extraction** — servo/offscreen_webview sit ON
core modules (wm, extensions, db, net::privacy, debug_capturer), so
"engine out first" was impossible without core.

Shipped: `aileron-core` crate with config, color, db, debug_capturer,
extensions, git, i18n, net, platform/paths (dir half), profiling,
update_check, wm. aileron re-exports one-for-one — zero call-site churn.
Core Cargo deps: the 16 crates the moved modules import (extracted
mechanically, then iterated: thiserror, chrono, reqwest(blocking),
attohttpc, tempfile(dev) surfaced by the compiler).

Verified: core 540 tests, aileron lib 663, both profiles green, clippy
clean. paths.rs split at its real seam (ProjectDirs dir helpers = core;
PlatformOps OS helpers = aileron platform).

Lesson for stage 1b: run the crate-boundary grep BEFORE the manifest
asserts cleanliness — the pattern `crate::(platform|mcp|sync|…)` was
missed on the first sweep and the compiler caught platform::paths.

## Stage 1b — CoreState/HeadedState split: field-by-field boundary

The AppState struct (43 fields, app/mod.rs:283) splits into CoreState
(agent runtime, stays in aileron-core) and HeadedState (headed UI, moves
to aileron-headed). AppState becomes `CoreState + Option<HeadedState>`
(None in agent mode — the agent runtime never touches headed state).

### CoreState (24 fields, stays in aileron-core)

| Field | Type | Why core |
|---|---|---|
| `wm` | BspTree | pane tree (agent reads pane layout) |
| `db` | Option<rusqlite::Connection> | database |
| `session_registry` | SessionRegistry | MCP sessions |
| `audit_log` | RefCell<VecDeque<String>> | MCP audit ring |
| `engines` | PaneStateManager | engine instances (wry + offscreen) |
| `config` | Config | user configuration |
| `pending_mcp_pane_creations` | VecDeque<(Uuid, Url)> | MCP pane queue |
| `mcp_nav_epoch` | HashMap<Uuid, u64> | nav epoch tracking |
| `mcp_settled_epoch` | HashMap<Uuid, u64> | settled epoch tracking |
| `pending_workspace_restore` | Option<String> | workspace restore |
| `current_workspace_name` | String | workspace name |
| `terminal_pane_ids` | HashSet<Uuid> | terminal tracking |
| `extension_manager` | Arc<RwLock<ExtensionManager>> | extensions (adblock) |
| `download_manager` | DownloadManager | downloads |
| `arp_server` | Option<ArpServer> | ARP server |
| `sync_watcher` | SyncWatcher | file sync |
| `adblock_blocked_count` | u64 | stats |
| `input_latency` | InputLatencyTracker | profiling |
| `profiler` | Profiler | profiling |
| `update_checker` | UpdateChecker | updates |
| `bitwarden` | BitwardenClient | password manager |
| `pending_tab_close` | Option<Uuid> | tab close queue |
| `pending_reader_save_path` | Option<PathBuf> | reader mode save |
| `first_frame`, `resize_pending`, `frame_count` | bool/u64 | frame pacing (agent offscreen renders too) |

### HeadedState (10 fields, moves to aileron-headed)

| Field | Type | Why headed |
|---|---|---|
| `window` | Option<Arc<Window>> | winit window handle |
| `mode` | Mode | keyboard mode (headed input system) |
| `keybindings` | KeybindingRegistry | headed keybindings |
| `palette` | CommandPalette | command palette UI |
| `macro_recorder` | MacroRecorder | headed macro recording |
| `macro_player` | MacroPlayer | headed macro playback |
| `last_macro_events` | Option<Vec<RecordedEvent>> | macro save |
| `pending_wry_actions` | VecDeque<WryAction> | headed wry pane actions |
| `pending_terminal_command` | Option<String> | headed terminal |
| `pending_new_window` | bool | headed windowing |
| `pending_detach_url` | Option<Url> | headed windowing |
| `popup` | PopupManager | headed popup UI |

### Execution

1. Define `CoreState` in core/src/state.rs (the 24 core fields)
2. Define `HeadedState` in headed/src/headed_state.rs (the 12 headed fields)
3. `AppState` becomes a thin wrapper: `pub struct AppState { pub core: CoreState, pub headed: Option<HeadedState> }`
4. Migrate `app_state.<field>` → `app_state.core.<field>` for core fields;
   `app_state.headed.as_mut().unwrap().<field>` for headed fields
5. frame_tasks/mcp/agent reference `CoreState` only; main.rs/app_handler
   hold both and delegate

This is the multi-session deep refactor — each step compiles green.
