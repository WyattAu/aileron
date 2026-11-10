
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

## Stage 1b Phase A executed (2026-11-09): agent-state types moved to core

Boundary grep against the real AppState (src/app/mod.rs) corrected the
manifest below:

- The struct has **47 fields + 7 embedded sub-state structs** (ui,
  panels, tabs, session, crash, autofill, cache), several cfg-gated
  (lua_engine, terminal_pane_ids, bitwarden, pending_terminal_command,
  sync_watcher, arp_server, arp_cmd_receiver) — none of which the
  original 43-field inventory listed.
- Agent-touched (must stay reachable headed-free): `session`
  (session_dirty written from MCP arms), `cache` (safe-list getter from
  agent.rs), `download_manager` (McpCommand::ListDownloads reads
  progress_all). The manifest missed the sub-states and misfiled
  download_manager risk.
- `input_latency`/`profiler`: headed-only per call sites, but their
  types already live in core — kept in CoreState anyway (zero cost).

Shipped in `core/src/state.rs`: SessionCaps + SessionRegistry (std
only, moved from mcp/bridge.rs — bridge re-exports for API compat,
S39 confinement tests moved with them), EngineType + PaneState +
PaneStateManager (from servo/engine.rs — PaneRenderer stays in the
main crate, it mentions wry::Rect), SessionState + CacheState (+ their
Defaults; private fields made pub — cross-crate access).

servo/mod.rs re-exports the moved types so servo::*/super::engine::*
paths resolve unchanged. aileron re-exports `state` one-for-one.

Decision recorded: DownloadManager moves to core in Phase B and drags
`tokio` (rt-multi-thread) into aileron-core deps. Accepted: the gate's
compile-out target is wgpu/winit/webkit, not tokio; the agent runtime
owns downloads (ListDownloads tool).

Corrected split for Phase B:
- **CoreState** (core crate): wm, db, config, engines, session_registry,
  audit_log, pending_mcp_pane_creations, mcp_nav_epoch, mcp_settled_epoch,
  pending_workspace_restore, current_workspace_name, extension_manager,
  download_manager, update_checker, adblock_blocked_count, input_latency,
  profiler, pending_tab_close, pending_reader_save_path, first_frame/
  resize_pending/frame_count, **session: SessionState, cache: CacheState**
  (agent-touched sub-states).
- **AppState wrapper** keeps cfg-gated capability fields in place
  (lua_engine, terminal_pane_ids, bitwarden, pending_terminal_command,
  sync_watcher, arp_server, arp_cmd_receiver) — their home modules
  (lua/, passwords/, sync/, arp/, terminal/) move to core in a later
  stage; the fields are feature-off in agent builds so compile-out holds.
- **HeadedState** (main crate): window, mode, keybindings, palette,
  macro_recorder, macro_player, last_macro_events, pending_wry_actions,
  pending_terminal_command→(wrapper), pending_new_window, pending_detach_url,
  popup, pane_last_focus (private, LRU unload), plus the headed-only
  sub-states ui/panels/tabs/crash/autofill. All access via
  `headed()` accessor (panic in agent mode = contract, not bug).

## Stage 1b Phase B executed (2026-11-09): the split landed

`AppState` = `{ core: CoreState, headed: Option<HeadedState> }` plus the
cfg-gated capability fields (lua_engine, terminal_pane_ids, bitwarden,
pending_terminal_command, sync_watcher, arp_server, arp_cmd_receiver).
Agent mode constructs `headed: None`; `headed()`/`headed_mut()` panic in
agent mode by contract.

Late discovery folded into the split: **`pending_wry_actions` is
agent-written** (the MCP create-tab arm queues `WryAction::Navigate` in
`process_mcp_commands`, frame_tasks/mod.rs:821) — so `pending_wry_actions`
and the `WryAction` enum both moved to core. `WryAction` is a plain data
enum (url/String/HashMap) — clean move; `app::WryAction` re-exports.

Also moved to core this stage: the `downloads` module (with tokio +
futures-util deps added to aileron-core). `check_offscreen_crashes` has
no callers — crash state stays headed. `handle_pending_import` is
headed-only (event_handler/app_handler callers) — import pend stay
headed.

Call-site migration was compiler-driven (the field tally drove it):
agent profile 991 E0609 → headed profile 283 → bins (agent.rs,
main.rs, app_handler.rs) 175 → test targets 278. Guard-lifetime
restructures: guard temporaries bound in inner scopes
(extensions_cmd, palette), pane-borrow conflicts resolved by computing
pane-derived values inside the tree borrow and mutating headed state
after (events.rs RequestNewTab/Next/Prev, app_handler.rs new-tab).

Gates at `1c69bc0`: both profiles 0 errors 0 warnings, aileron 648 +
core 551 lib tests, clippy -D warnings, fmt, chrome WASM — all green.

## Stage 2a executed (2026-11-10): MCP stack into aileron-core

`src/mcp` (bridge, tcp_bridge, server, tools, transport, aci) moved to
`core/src/mcp`. The module was fully self-contained: zero crate::
references outside the mcp tree, external deps (tracing, tokio,
attohttpc, urlencoding) already core deps — zero new dependencies.
bridge.rs re-export switched to `crate::state` (types moved in 1b-A).
Main crate re-exports `aileron_core::mcp` unconditionally; main's mcp
feature keeps its image/base64 deps (consumed by frame_tasks +
test_harness, the actual users). Gates: core 589 (+38 mcp tests) +
aileron 610 (−38) — exactly consistent; both profiles 0 err/0 warn.

## Stage 2b scope (next): agent runtime + event-routing split

Signature audit of frame_tasks (all `pub fn`):

| fn | params | verdict |
|---|---|---|
| `spawn_mcp_server(&McpBridge, SessionRegistry, &str)` | all core | → core, trivial |
| `poll_git_status(&mut GitStatus, &Option<GitPoller>)` | core git | → core, trivial |
| `process_mcp_commands(&McpBridge, &mut WryPaneManager, Uuid, &mut AppState, &mut OffscreenWebViewManager, …)` | body is core-clean; signature drags engine types | → core via `PaneOps` callback trait |
| `process_offscreen_events(AppState, OffscreenWebViewManager, …)` | engine bridge | stays with engine (main crate → aileron-engine later) |
| `process_pending_wry_actions(Option<&mut AppState>, …)` | headed consumer | stays |
| `process_arp_commands(&mut AppState)` | cfg arp wrapper fields | stays |
| `auto_save_workspace` / `push_tabs_to_arp` | `&WryPaneManager` | stays |

Key fact: `process_mcp_commands`' body touches NO headed state anymore
(stage 1b moved everything it needs into CoreState) — only its
signature drags engine types. Stage 2b = define `trait PaneOps` in
core (create/navigate/query offscreen pane), have the headed/engine
side provide `&mut dyn PaneOps`; then process_mcp_commands +
spawn_mcp_server + the audit helper move to core, and `agent.rs`
follows (it holds an engine handle — the target table's "core
(+engine for webviews)"). Terminal MCP arms: verify their state access
before the move (terminal_pane_ids is a wrapper-cfg field).

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
