# GATE-1 Execution Plan — the Compile-Out (crate decomposition)

**Status:** PLAN (approved direction, execution is multi-session)
**Goal:** the agent daemon builds without linking wgpu, the Leptos chrome,
the terminal stack, or the input subsystem — the binary and dependency
surface the split decision (`agent-native-split.md`) promised.

## Why it hasn't been a flag flip

`aileron --agent` today is a runtime profile over the full binary: the lib
compiles every module unconditionally, and `AppState` is a god-object that
instantiates headed subsystems. Excluding wgpu/chrome from the LINK requires
module-level feature gating plus state decomposition. Coupling map
(2026-11-01 grep): 11 lib files reference headed modules (`gfx`, `terminal`,
`chrome_bridge`, `ui`), including `app/mod.rs` (AppState itself),
`frame_tasks/mod.rs`, `config.rs`, `workspace_restore.rs`.

## Target crate layout

| Crate | Contents | Depends on |
|---|---|---|
| `aileron-core` | wm (BSP), config, db, downloads, net (adblock), extensions, mcp (bridge/server/tools/transport/aci), scripts, frame_tasks (event routing), agent runtime | core-only |
| `aileron-engine` | servo (wry), offscreen_webview, workspace_restore | core, webkit2gtk |
| `aileron-headed` | gfx (wgpu compositor), terminal, input, chrome_bridge, ui, popup, app_handler, main (headed) | engine |
| `aileron-agent` (bin) | thin: `aileron_core::agent::run()` | core (+engine for webviews) |

Dependency direction is one-way: core never sees headed. The agent runtime
moves into `aileron-core` (it already only touches core types — AppState
minus headed fields is the work).

## Staging (each stage lands green)

1. **S1 — extract `aileron-core`:** move wm, config, db, downloads, net,
   extensions, mcp, scripts, frame_tasks, workspace_restore, agent into the
   new crate; AppState splits into `CoreState` (agent-visible) + `HeadedState`
   (window, compositor, chrome, terminal, input). The headed binary holds both;
   the agent runtime holds only CoreState. *Gate: both binaries build, all
   1209 tests pass.*
2. **S2 — drop the headed dep tree from the agent build:** `aileron-agent`
   Cargo.toml lists core+engine only. wgpu, leptos, alacritty_terminal leave
   the agent dependency graph. *Gate: `cargo tree -p aileron-agent` contains
   no wgpu/leptos/alacritty; binary size measured.*
3. **S3 — module audit:** `cargo deny`/`cargo tree` diffs tracked in the
   ledger; the split-decision gate table (idle RSS, binary size, dep count)
   re-measured and recorded.

## Risks

- AppState field extraction touches every subsystem's state access — the
  largest mechanical risk; staged by moving fields in groups with compile
  gates per group.
- frame_tasks references headed capture paths (process_wry_events) — the
  event-routing split keeps offscreen routing in core, headed capture in
  engine.
- The estate's shared-checkout rule: this refactor must run in a worktree
  (`opencode worktree_create`), not the shared tree.
