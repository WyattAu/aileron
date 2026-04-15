# Acceptance Criteria

**Phase:** 0 (Requirements Engineering)
**Project:** Aileron
**Date:** 2026-04-11
**Analyst:** Requirements Engineer

---

## AC-GFX: Graphics & Rendering Pipeline

| ID | Criterion | Verification Method | Priority |
|----|-----------|-------------------|----------|
| AC-GFX-001 | A winit window opens within 2 seconds of launch | Manual test, stopwatch | Critical |
| AC-GFX-002 | Servo renders google.com into a wgpu texture visible in the window | Visual inspection | Critical |
| AC-GFX-003 | egui UI elements (status bar, borders) render over the web content | Visual inspection | Critical |
| AC-GFX-004 | Compositing maintains >=60fps with a single web pane | Frame counter overlay | Critical |
| AC-GFX-005 | Compositing maintains >=30fps with 4 tiled panes | Frame counter overlay | High |
| AC-GFX-006 | Window resize correctly propagates to Servo and egui without artifacts | Manual resize test | High |

## AC-WM: Window Management

| ID | Criterion | Verification Method | Priority |
|----|-----------|-------------------|----------|
| AC-WM-001 | User can split the window horizontally via keybinding | Keybinding test | Critical |
| AC-WM-002 | User can split the window vertically via keybinding | Keybinding test | Critical |
| AC-WM-003 | User can close a pane and the remaining pane fills the space | Keybinding test | Critical |
| AC-WM-004 | User can navigate between 4+ panes using h/j/k/l | Keybinding test | Critical |
| AC-WM-005 | Each pane loads and renders an independent URL | Multi-pane URL test | Critical |

## AC-MODE: Modal Input

| ID | Criterion | Verification Method | Priority |
|----|-----------|-------------------|----------|
| AC-MODE-001 | Pressing `i` in Normal mode enters Insert mode (status bar shows INSERT) | Mode indicator test | Critical |
| AC-MODE-002 | Pressing `Esc` in Insert mode returns to Normal mode | Mode indicator test | Critical |
| AC-MODE-003 | In Normal mode, `j`/`k` scrolls the active pane | Scroll test | Critical |
| AC-MODE-004 | In Insert mode, typing `hello` enters "hello" into a focused text input | Input test | Critical |
| AC-MODE-005 | Mode transitions do not drop or duplicate keystrokes | Rapid mode switching test | High |

## AC-SERVO: Servo Integration

| ID | Criterion | Verification Method | Priority |
|----|-----------|-------------------|----------|
| AC-SERVO-001 | Navigating to https://example.com loads and displays the page | Navigation test | Critical |
| AC-SERVO-002 | Page title changes are reflected in the status bar | Title change test | High |
| AC-SERVO-003 | Click events on links trigger navigation | Click test | Critical |
| AC-SERVO-004 | Ctrl+E opens the current URL in the system default browser | External browser test | High |
| AC-SERVO-005 | JavaScript execution within a pane returns correct results | JS eval test | High |

## AC-CP: Command Palette

| ID | Criterion | Verification Method | Priority |
|----|-----------|-------------------|----------|
| AC-CP-001 | Command palette opens within 100ms of trigger keypress | Latency measurement | Critical |
| AC-CP-002 | Fuzzy search filters 10,000 history entries within 50ms | Performance benchmark | High |
| AC-CP-003 | Selecting a history item navigates to that URL | Navigation test | Critical |
| AC-CP-004 | Command palette renders as a centered overlay over web content | Visual inspection | High |

## AC-AD: Ad-Blocking

| ID | Criterion | Verification Method | Priority |
|----|-----------|-------------------|----------|
| AC-AD-001 | Requests to domains in the blocklist are not executed | Network log inspection | Critical |
| AC-AD-002 | Loading a page with known ad domains shows no ad content | Visual inspection | High |
| AC-AD-003 | Blocking does not add >10ms latency to non-blocked requests | Latency benchmark | High |

## AC-MCP: MCP Server

| ID | Criterion | Verification Method | Priority |
|----|-----------|-------------------|----------|
| AC-MCP-001 | MCP server starts on a background thread without blocking the UI | Thread inspection | Critical |
| AC-MCP-002 | `read_active_pane` returns the page content as Markdown text | MCP client test | Critical |
| AC-MCP-003 | `search_web` returns search results from DuckDuckGo | MCP client test | High |
| AC-MCP-004 | MCP server responds within 500ms for tool calls | Latency measurement | High |

## AC-LUA: Lua Scripting

| ID | Criterion | Verification Method | Priority |
|----|-----------|-------------------|----------|
| AC-LUA-001 | init.lua is loaded and executed on startup without errors | Startup log check | Critical |
| AC-LUA-002 | Custom keybindings defined in init.lua override defaults | Keybinding test | Critical |
| AC-LUA-003 | Theme changes in init.lua are applied to the UI | Visual inspection | High |
| AC-LUA-004 | Lua errors are caught and logged without crashing the browser | Error injection test | High |

## AC-SEC: Security

| ID | Criterion | Verification Method | Priority |
|----|-----------|-------------------|----------|
| AC-SEC-001 | Password values never appear in tracing/log output | Log inspection | Critical |
| AC-SEC-002 | Injected DOM credentials are cleared from Rust memory after use | Memory inspection (Valgrind) | Critical |
| AC-SEC-003 | MCP server rejects unauthenticated tool requests | MCP auth test | High |
| AC-SEC-004 | Panes with different session configs have isolated cookies | Cookie isolation test | High |

## AC-DB: Data Persistence

| ID | Criterion | Verification Method | Priority |
|----|-----------|-------------------|----------|
| AC-DB-001 | Browsing history is persisted to SQLite after page load | Database query | Critical |
| AC-DB-002 | Bookmarks are persisted and survive application restart | Restart test | Critical |
| AC-DB-003 | Config files are placed in XDG-compliant paths | Path inspection | High |
| AC-DB-004 | Workspaces can be saved to and loaded from JSON files | Save/load test | Medium |
