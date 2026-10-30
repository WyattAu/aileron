# Decision Record: Aileron Split — Human Browser + Agent-Native MCP Daemon

**Date:** 2026-10-30 · **Status:** ADOPTED (direction) — phases pending
**Author:** @WyattAu with the MCP reliability campaign
**Depends on:** `gui-architecture.md` (chrome decision), YP-MCP-NAV-EPOCH-001 (read-your-writes), YP-MCP-CONTRACTS-001 (tool contracts)

## The decision

Split aileron's product surface into two runtime profiles of one codebase:

| | **aileron** (human) | **aileron-agent** (agent-native) |
|---|---|---|
| Purpose | full desktop browser | the leanest, most feature-complete browser MCP server |
| Chrome/GUI | yes (chrome decision per `gui-architecture.md`) | **none** — no chrome webview, no compositing, no terminal, no input router |
| Rendering | wgpu compositor to screen | offscreen webviews; pixels only on explicit `screenshot` |
| MCP | available (co-exists with human use) | the primary (only) interface; multi-session |
| Deploy | desktop app | headless daemon (systemd/OCI), Xvfb or headless WebKit |
| Dependency weight | full (wgpu, leptos chrome, egui…) | compiled-out: no wgpu, no chrome — fewer segfault surfaces, smaller binary |

One binary, profile-selected at launch (cargo feature + runtime config), mirroring
how Chromium ships headed and headless. The agent profile **deletes the exact
subsystems the 2026-10 campaign identified as the cost and failure center**:
frame capture, compositing, chrome webview, input routing.

## Why this wins: the grounded competitive picture (2026-10)

**Playwright MCP** (`@playwright/mcp`, Microsoft — the benchmark; 6.1M weekly npm downloads):
24 core tools + opt-in capability groups (`vision`, `pdf`, `devtools`, `storage`, `network`, `testing`).
Strengths: accessibility snapshot with element refs (its killer feature — agents click by
ref, no vision model), console/network visibility, deterministic waits, huge ecosystem.
Structural weaknesses: **ephemeral** (fresh browser context per session — no persistence,
no workspaces, no cross-restart state), no adblock (agents read ad/consent-wall noise),
wait heuristics with no correctness story (no read-your-writes guarantee), Node runtime,
per-session startup cost, crash = lost session (no watchdog/self-healing story).

**Brave**: first-party MCP is the **search API** (2–8 tools: `brave_web_search`,
`brave_local_search`) — not a browser. Browser control exists only as a community
Chrome-DevTools-Protocol fork ("Brave DevTools MCP") driving a running Brave over CDP.
No first-party browser MCP surface to beat; CDP inherits Chrome's headless
fingerprinting and ephemeral-session model.

**DuckDuckGo ("ddgs")**: no official browser MCP. The ecosystem servers are
**metasearch + scrape** layers (`search_text/news/images/videos/books`, `fetch_content`),
fighting DDG's fingerprint-blocking with TLS-impersonation fallbacks. **No browser
state, no JS evaluation, no sessions, no interactivity** — they cannot click, wait,
or read a page that requires script.

**Emerging (to watch):** WebMCP — a proposed web standard (Google; Mozilla neutral;
Meta shipping) letting *pages* expose tools to browser-based agents. A browser that
natively speaks MCP to agents is one protocol hop from speaking WebMCP to pages;
Chromium will gate it behind Gemini. This is a later moat, noted here.

## Aileron's standing advantages (shipped, proven 2026-10-24..30)

1. **Read-your-writes with proofs** — navigation epochs, settle-hold with stale-event
   flushing (`7fb19f5`), Lean-verified invariants. Playwright offers `wait_for_load_state`
   heuristics; we offer a specified, traced, tested guarantee.
2. **Adblock-clean extraction** — agents read the page, not the consent wall. Unique.
3. **Persistent, resumable state** — workspaces, sessions, bookmarks/history survive
   restarts; Playwright contexts evaporate.
4. **Self-healing ops** — service + watchdog + cleanup-proof builds; five overnight
   cycles proven.
5. **Session-typed relay, multi-session by design** — several agents, one browser,
   typed panes (Playwright: one client per browser).
6. **Single Rust binary, no Node**, MCP stdio↔TCP, boot in milliseconds.

## Gap matrix — what Playwright has that the agent profile must match

| Capability | Playwright tool | Aileron status | Implementation path (WebKitGTK) |
|---|---|---|---|
| A11y/DOM snapshot with refs | `browser_snapshot` | **missing** — the #1 gap | in-page JS walker (ARIA roles + heuristics) → pruned tree w/ stable refs; `click/type` accept refs |
| Click/type/hover/select by ref | `browser_click` etc. | partial (coordinate-less actions exist?) | ref→element dispatch via the same walker |
| Console messages | `browser_console_messages` | missing | `script-message`/console signals → ring buffer per pane |
| Network request log | `browser_network_requests` | missing (interceptor registry exists) | instrument via user-script fetch/XHR hooks + resource signals |
| Network interception | `browser_route` | partial (registry built, not MCP-exposed) | expose allow/block/redirect tools over the registry |
| Cookies/storage | `browser_cookie_list` (`--caps=storage`) | missing | `webkit_website_data_manager` APIs |
| File upload / dialogs | `browser_file_upload`, dialog tools | missing | `run-file-chooser` + script-dialog signals |
| PDF save | `browser_pdf_save` (`--caps=pdf`) | missing | WebKit print-to-PDF operation |
| Tabs | `browser_tabs` | have (panes — richer: splits, workspaces) | done |
| Waits | `browser_wait_for` | have (epochs + settle + wait_for) | done, with proofs |
| Screenshots | `browser_take_screenshot` | have (on-demand capture) | keep on-demand only in agent profile |
| JS evaluation | `browser_evaluate` | have (`execute_js` + callback) | done |
| History/downloads | (not in Playwright core) | have bookmarks/history; downloads partial | downloads events already wired |

## Metric targets (beat, not match)

| Metric | Playwright MCP baseline (typical) | aileron-agent target |
|---|---|---|
| Cold start → first tool ready | ~1–3s (Node + browser launch) | <300ms (daemon always-on; relay attach <50ms) |
| navigate→readable-text latency | ~0.5–2s + heuristic waits | <500ms p50 with **guaranteed** freshness |
| Idle RSS | ~250–500MB (Chromium context) | <350MB daemon with 4 panes (spike to verify) |
| Crash recovery | session lost | watchdog restart + workspace restore, agent reattaches |
| Tool count | 24 core (+caps) | ≥30 core, zero capability flags |
| Correctness | heuristic waits | specified + Lean-proven read-your-writes |

## Phases

1. **P1 — agent profile spike (next):** `--profile agent` runtime: compile-out
   compositor/chrome/terminal/input (`cfg(feature)`), offscreen webviews only, MCP up,
   runs under Xvfb; measure RSS/startup/latency. Exit: numbers vs the table above.
2. **P2 — close the snapshot gap:** `browser_snapshot` walker + ref-based
   `click/type` — the single feature that decides agent-market credibility.
3. **P3 — observability:** console ring, network log, cookies, file upload, PDF.
4. **P4 — benchmark harness:** scripted identical task lists vs playwright-mcp;
   publish the matrix (latency percentiles, tool coverage, failure recovery).
5. **P5 — human component:** revisit chrome substrate (`gui-architecture.md`) as
   product polish — now decoupled from the agent product's success.

## Risks

- **WebKitGTK headless quirks** (OffscreenWindow under Xvfb) — spike validates first.
- **Snapshot walker fidelity** vs Playwright's CDP-backed a11y tree — iterate on real
  sites; the QA matrix gains a snapshot test vector set.
- **Scope creep** — the human browser keeps its own roadmap; the agent profile only
  shares the core (webview, network, storage, relay, specs).
