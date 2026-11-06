# GATE-1 S4 — physical crate split: move manifest

**Status:** READY-TO-EXECUTE (execution-grade; each stage lands green)
**Preconditions (all met):** S1 agent-profile compiles (`e53b10b`); S2
compositor/windowing deps absent from the agent profile (`3ebbbd1`);
S3 chrome out of default builds (`6bbd91b`).

## Target layout

```
aileron/               workspace root
├── core/              aileron-core      (agent-visible everything)
├── engine/            aileron-engine    (wry/webkit; agent needs it)
├── headed/            aileron-headed    (gfx, chrome bridge, windowing)
├── src/               aileron (bin)     (headed binary: thin main)
└── bin/               aileron-agent (bin) (thin: core+engine only)
```

## Move manifest (per target crate)

### aileron-core (no webkit, no wgpu, no winit)
From current `src/`: `color.rs`, `config.rs` (done: color-free), `db/`,
`wm/`, `net/`, `sync/`, `extensions/`, `lua/`, `i18n/`, `mcp/` (bridge,
server, tools, transport, tcp_bridge, aci), `frame_tasks/`, `scripts.rs`,
`passwords/`, `arp/`, `input/{mode,router,macro_recorder,keybindings}`,
`ui/{palette,search}`, `llm/`, `terminal/{mod,grid}` (grid types; PTY
behind `terminal` feature), `debug_capturer.rs`, `git.rs`, `profiling/`,
`update_check.rs`, `event_handlers.rs` (verified: key→VT100 sequences,
terminal-feature only, zero winit refs — core), `app/` (AppState minus
headed fields — the S1 cfg gates become real field moves).
Feature set carried over: `mcp`, `terminal`, `llm`, `lua`, `passwords`,
`sync`, `arp` (as today).

### aileron-engine (wry + webkit2gtk; depends on core)
`servo/` (wry_engine, wry_pages), `offscreen_webview.rs`, `net/adblock`
if it touches webview types (verify), `workspace_restore.rs` (panes_to_create
consumer stays core — only the webview factory moves).

### aileron-headed (depends on core + engine)
`gfx/`, `chrome_bridge.rs`, `chrome_native.rs`, `popup.rs`,
`app/{popup,event_handler}`, `app_handler.rs`, `input/{keymap,
key_conversion}`, `platform/` (winit/x11 parts), `terminal/pty.rs`,
`test_harness.rs`, `wry_actions.rs` (headed-only half; audit first).

### bins
- `aileron` (headed): `main.rs` + `bootstrap.rs` — thin: parse, headed
  init, `aileron_headed::run()`.
- `aileron-agent` (new): `agent.rs` loop + `bin/mcp_server.rs` path —
  depends on **core + engine only**; Cargo.toml lists no headed deps.

## Ordering (each step compiles green before the next)

1. **engine out**: move `servo/` + `offscreen_webview.rs` into
   `aileron-engine`; `aileron` depends on it. Gate: both profiles build,
   1209 tests green.
2. **headed out**: move gfx/chrome_bridge/popup/app_handler/keymap/pty +
   winit-using slices. Gate: same.
3. **agent bin**: `aileron-agent` bin (core+engine only), required-features
   dropped from `aileron` (headed is now its own crate). Gate: agent bin
   builds and serves 49 tools live.
4. **app/ decomposition** (AppState split): `CoreState` fields move to
   core; `HeadedState` (window, compositor handles, popup, terminal
   manager) lives in headed. Gate: `cargo tree -p aileron-agent` shows
   zero wgpu/winit/webkit-in-headed.

## Failure modes seen in S1–S3 (carry the lessons)

- Feature gates must move WITH their modules (the `terminal` feature
  gates PTY, not grid).
- cfg-split signatures over cfg-tangled call sites (the
  `restore_workspace`/`llm_*` patterns from S1).
- `required-features` on bins, not cfg in bins.
- test_harness moves with headed; integration tests that need webviews
  belong to engine.

## Verification checklist (per stage)

- `cargo build` (headed) rc=0
- `cargo build --no-default-features --features mcp` rc=0
- `cargo tree -p <agent pkg> -e normal | grep -ciE "wgpu|winit|leptos"` → 0
- full pre-push gate (fmt, clippy -D warnings, 1209 lib tests, docs,
  integration)
- live: agent daemon serves 49 tools; headed service starts with the
  real UI (manual, Conflicts= respected)
