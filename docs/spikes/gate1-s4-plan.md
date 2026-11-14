
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

## Stage 2c design (2026-11-10): PaneOps split — execute mechanically

**Measurement correction (gate table):** agent profile (`--no-default-features
--features mcp`) has **zero** dep-tree edges for wgpu/leptos/alacritty_terminal/
portable-pty/winit — the compile-out target holds. **webkit2gtk + wry remain by
design**: `offscreen_webview.rs` (agent's renderer) imports `wry::` directly,
so webkit is not gateable behind headed. The physical win is crate separation
(parallel compilation, one-way deps), not webkit exclusion.

**Key discovery:** `workspace_restore`'s deps are now all core types
(PaneStateManager, BspTree, db — core since 1b-A) → moves to core cleanly,
and it is called from a `process_mcp_commands` arm.

**Trait sketch** (`core/src/agent/pane_ops.rs`): derive the exact method set
by grepping every engine-param call site inside process_mcp_commands first.
Expected shape: pane_ids, pane_url, pane_title, pane_exists, navigate,
back/forward, reload, execute_js(oneshot result), click, get_text, screenshot,
is_busy, reset_all. Engine side implements `CombinedPaneOps<'a>(&mut
WryPaneManager, &mut OffscreenWebViewManager)` in the main crate.

**Execution order:**
1. `workspace_restore` → core (verify imports first; servo re-export covers)
2. Grep the fn's engine call sites → finalize PaneOps method set → trait in core
3. `process_mcp_commands` signature becomes
   `(&McpBridge, Uuid, &mut CoreState, &mut dyn PaneOps)` — **&mut AppState
   drops to &mut CoreState** (body already touches zero headed fields)
4. Move fn to `core/src/agent::process_mcp_commands`; main keeps a shim
5. Callers (agent.rs, headed frame loop) build CombinedPaneOps and pass
   `&mut app_state.core`
6. Stage 2d (deferred): agent event loop → `core::agent::run()` once the
   offscreen-event path also crosses (OffscreenWebViewManager is engine —
   needs its own trait or stays as the engine handle the agent holds)

## Stage 2c executed (2026-11-10): dispatch + llm moved to core

Step 1: workspace_restore → core (terminal hooks inverted to callers via
RestoreResult::removed_pane_ids). Step 2: **PaneOps trait** (core) +
CombinedPaneOps (src/pane_ops.rs) — process_mcp_commands takes
(&McpBridge, Uuid, &mut CoreState, &mut dyn PaneOps); the Screenshot
arm collapsed 63 → 1 call. Step 3: process_mcp_commands,
wait_mcp_nav_settled, audit_record, wait_for_js_result, the llm_* helpers,
and src/llm (OllamaClient) moved into core/src/agent.rs — every crate::
path resolved identically inside core (db, wm, mcp, downloads,
workspace_restore). Core gains `terminal` + `llm` features, forwarded
from the main crate's. Gates at each step: 578-582 core + 610-621
aileron lib tests, clippy -D warnings, integration green on push
(one integration test needed the core.terminal_pane_ids path).

## Stage 2c measurement + stage 3 refinement (2026-11-10)

Gate table: agent profile dep tree has **0** edges for
wgpu/leptos/alacritty_terminal/portable-pty/winit; webkit2gtk+wry edges
present **by design** (the agent renders offscreen through wry; core
now carries 31 dep lines including tokio/futures-util for downloads).

**Stage 3 (engine crate) cut refinement — critical:** servo/ mixes
wgpu files (texture_share, servo_engine — Architecture D future path)
with webkit files (wry_engine, wry_pages, fallback, engine_selection).
Moving ALL of servo into aileron-engine would drag wgpu into the agent
dep graph and regress the S2 compile-out gate. The engine cut must be
**wry-only**: wry_engine, wry_pages, fallback, engine_selection,
protocol, PaneRenderer + offscreen_webview. The wgpu pair
(servo_engine, texture_share) stays in the headed crate until
Architecture D actually lands. pump_gtk_loop moves to engine (GTK
mechanics; frame_tasks + pane_ops re-export via the shim pattern).

## Stage 2d executed (2026-11-10/11): PaneOps impl + offscreen chain → CoreState

- **2d-1** `daeddad`: CombinedPaneOps + dispatch_active_js_with_callback
  moved to aileron-engine (image/base64 deps followed). Main re-exports
  `aileron_engine::pane_ops`.
- **2d-2a** `cc58017`: scripts.rs (ContentScriptManager), ui/search.rs,
  UiState → core. UiState is the shared feedback store — the offscreen
  IPC chain writes ui.status_message from agent-reachable arms (600+
  call sites swept to core.ui). Core gains nucleo, aileron-shared, an
  optional mlua under its `lua` feature (scripts registers Lua palette
  commands); main keeps its own mlua for src/lua.
- **2d-2b** `a2de49e`: AutofillState → CoreState.autofill; passwords/
  (bitwarden+keyring, self-contained) → core behind a `passwords`
  feature (optional keyring+age, forwarded from main).
- **2d-2c** `351afb7`: process_offscreen_events(+inner),
  handle_ipc_message_offscreen/generic, process_events,
  inject_extension_shim_and_script, sync_tab_url_title all take
  &mut CoreState. CoreState gains injected_content_script_ids,
  private_pane_ids, bitwarden. AppState methods inlined at call sites
  (all trivial core writers).

**Latent panic found + fixed in 2c**: the LoadComplete arm wrote
headed_mut().tabs.tab_display_dirty — agent-reachable with headed:
None (the GATE-3 soak validated the OLD binary, pre-1b, so never hit
this path). The headed wry caller now computes had_load_complete
before process_events and sets the flag after; the agent path never
touches headed state.

**Stage 3 step 1** `c2d6387`: aileron-engine crate created (servo/ +
offscreen_webview + GTK pump + spellcheck). The servo tree is
wgpu-FREE in code (all wgpu mentions are Architecture D comments;
servo_engine is a stub on wry types) — full move, zero wgpu leakage.
Agent profile re-measured: 0 wgpu/leptos/alacritty/winit/portable-pty
edges post-move. platform::spellcheck (webkit-touching) → engine; core
stays GUI-free.

## Stage 2d-3 scope (next session): thin aileron-agent bin

Everything the agent calls now takes CoreState except two moves:

1. **Offscreen chain → engine**: process_offscreen_events(+inner)
   (frame_tasks/mod.rs:326-397), handle_ipc_message_offscreen +
   handle_ipc_message_generic + inject_extension_shim_and_script
   (frame_tasks/ipc.rs — generic is lines 48-431), EventPane +
   EventPaneManager traits + their two impls (event_proc.rs:75-108 —
   WryPaneManager + OffscreenWebViewManager impls, both engine types).
   Generic body crate refs: config/db/passwords only (all core ✓).
   handle_ipc_message (headed wry wrapper, takes AppState) STAYS in
   main and calls aileron_engine::ipc::handle_ipc_message_generic
   (already passes &mut app_state.core). Shims for process_offscreen_
   events keep agent.rs/app_handler call sites unchanged.
2. **Bin crate** `agent/`: Cargo.toml (aileron-core, aileron-engine,
   gtk for the glib loop), src/main.rs = src/agent.rs moved. Fix
   imports to aileron_core/aileron_engine (AppState → CoreState::new,
   frame_tasks shims → direct paths, event_handlers::is_nvidia_gpu →
   move to engine, servo::init_gtk → aileron_engine::servo::init_gtk,
   WryPaneManager → aileron_engine::servo). src/Cargo.toml drops the
   aileron-mcp bin. Workspace members += "agent".
3. Gate-table closure: binary size + idle RSS for aileron-agent vs the
   headed aileron.

## Stage 2d-3 refinement (2026-11-12, from a first extraction attempt)

The offscreen-chain pull is a **single atomic unit of ~700 lines across
3 files** — do it in ONE scripted pass from git HEAD, not incrementally
(incremental attempts tangle trait/impl nesting and drop helper fns):

Cut from `src/frame_tasks/mod.rs`: sync_tab_url_title,
process_offscreen_events_inner, both cfg variants of
process_offscreen_events (drop the `AppState` cfg(not(mcp)) variant —
the agent build always has mcp; McpBridge is unconditional in core).
Cut from `src/frame_tasks/ipc.rs`: ExecuteJs trait,
handle_ipc_message_generic (lines 48-431), handle_ipc_message_offscreen,
inject_extension_shim_and_script, EXTENSION_RUNTIME_SHIM_JS const.
Cut from `src/frame_tasks/event_proc.rs`: EventPane trait,
EventPaneManager trait, all three impls (WryPane, OffscreenWebView,
WryPaneManager, OffscreenWebViewManager), process_events, and
pane_id_from_event (helper).

Missing pieces discovered mid-attempt (add to engine in the same pass):
serde_json + urlencoding deps; MessageBus + ExtensionId + RunAt +
WryEvent imports; sync_tab_url_title goes pub in offscreen_events.rs
and is referenced as `super::offscreen_events::sync_tab_url_title`;
`pane_id_from_event` moves with process_events. Core-module paths
(`crate::db|extensions|net|platform|debug_capturer|scripts|passwords`
...) rewrite to `aileron_core::`; `crate::servo`/`crate::offscreen_
webview`/`crate::pump` stay engine-internal. Main keeps shims:
`pub use aileron_engine::{ipc::*, offscreen_events::process_offscreen_
events};` in frame_tasks. The headed wrapper `handle_ipc_message`
(takes AppState) STAYS in main, calling
`handle_ipc_message_generic(&mut app_state.core, ...)`.

Then 2d-3 step 2: `agent/` bin crate (core+engine deps), agent.rs moved
with import remap (AppState→CoreState::new, frame_tasks shims→direct
aileron_core/aileron_engine paths, event_handlers::is_nvidia_gpu→engine,
servo::init_gtk→aileron_engine::servo::init_gtk, gtk dep for the glib
loop). Workspace members += "agent"; src/Cargo.toml drops the
aileron-mcp bin.

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

## Stage 2d-3 COMPLETE + validation (2026-11-13)

2d-3a landed (`2e799b3`): offscreen chain in engine/src/{ipc,
offscreen_events}.rs. 2d-3b landed (`85eee29`): agent/ crate — thin
`aileron-agent` binary over core+engine+gtk, holding CoreState directly.
2d-3b prep (`704a159`): get_cached_https_safe_list → CoreState method.

**LIVE VALIDATION PASS (2026-11-12, real display, NVIDIA DMA-BUF
workaround active):** initialize (49 tools) → navigate example.com
0.04s → browser_get_text full text incl. Arabic i18n → navigate
rust-lang.org → browser_get_text nav content → browser_snapshot 32 a11y
nodes, epoch advancing → execute_js innerText round-trip → wait_for →
list_tabs/read_active_pane. The decomposed stack works end to end.

**SOAK3 RUNNING** against the decomposed binary (log:
/tmp/opencode/soak-decomposed.log, script /tmp/opencode/soak-
decomposed.py): 2h, navigate+3s settle+browser_get_text+browser_
snapshot cycles with content/role assertions. Cycle 50: 5.59s, 0
errors, 2015-char text extracted. Next session: read the COMPLETE
summary line and record in ERA2. Note: test harnesses must use
id-matched JSON-RPC reads (notifications arrive interleaved) and the
tool name is browser_get_text (not get_text).

The daemon currently serving 9543 IS the decomposed binary (xvfb
display 93). Fleet service cutover (ExecStart → aileron-agent binary
path) is a chezmoi-side change deferred until SOAK3 passes. The old
`aileron --agent` dispatch in bootstrap.rs remains working via shims.

## Stage 3 step 2 scope (headed crate) — for a fresh session

Inventory (2026-11-14): app/ 9,577 + frame_tasks 818 + input 2,399 +
terminal 1,002 + ui 343 + gfx 273 + lua 1,376 + sync 2,758 + arp 890 +
single files 5,333 (chrome_bridge, app_handler, event_handlers,
bootstrap, main, wry_actions, test_harness, chrome_native) ≈ 23,770
lines headed-side. Core-side already extracted (state, mcp, agent,
scripts, search, passwords, downloads, llm, workspace_restore).

This is the atomic-unit class: ONE scripted pass from git HEAD, never
incremental. The app/ split is the hard part — app/mod.rs holds the
AppState wrapper (CoreState + Option<HeadedState>); the wrapper should
move to aileron-headed with HeadedState, while core keeps CoreState.
Call sites in frame_tasks/event_proc take &mut CoreState ✓ (2d-2c) —
the headed halves of frame_tasks (process_wry_events,
process_pending_wry_actions, push_tabs_to_arp, auto_save_workspace,
handle_ipc_message wrapper) move with the headed crate.

Move order:
1. aileron-headed crate skeleton (deps: core, engine, aileron-shared,
   gtk stack, wgpu, egui, mlua, leptos/chrome deps stay on aileron)
2. Atomic move: gfx, terminal, input, ui/palette+popup, chrome_bridge,
   chrome_native, app_handler, event_handlers, bootstrap, main(bin),
   wry_actions, test_harness, lua, sync, arp, app/ headed halves
   (HeadedState, dispatch, commands, cmd/, events, event_handler,
   instance, omnibox, palette, panes, popup), frame_tasks headed fns
3. app/mod.rs retains: AppState wrapper → moves to headed too; core
   call sites already take CoreState ✓
4. aileron (src/) keeps: lib.rs shims for bin compat (main.rs moves),
   bin/ relay, frame_tasks headed fns OR shims
5. Bin cutover: aileron bin → aileron-headed crate; fleet unit for
   ailon.service updated in same pass

Alternatively DEFER entirely: the functional compile-out is done
(agent = core+engine+agent crates; binary 10.8 MB measured). The
headed split is workspace symmetry + parallel compilation, not
capability. Do it when the compilation-time pain is measured.

## Walker validation on decomposed runtime (2026-11-14, phone AX)

GATE-4 methodology re-run against the NEW aileron-agent binary:
phone real-Chrome AX (CDP getFullAXTree, R5CT50JA09T) vs
browser_snapshot on the decomposed runtime, same page (example.com).
Phone: 9 named nodes (1 link + 8 StaticText i18n). Walker: 1 node
(link "Learn more") — exact match on the interactive element;
StaticText filtered by walker design. **Walker survived the
decomposition intact.** Confirmed with real device ground truth.
