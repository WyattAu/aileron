# Aileron Code Coverage Report

**Date:** 2026-06-09
**Tool:** cargo-llvm-cov 0.8.7
**LCOV Output:** `lcov.info` (48,332 lines)
**Tests:** 1,178 passed, 26 ignored, 0 failed

## Overall Coverage

| Metric | Value | Percentage |
|--------|-------|------------|
| **Line Coverage** | 21,540 / 36,300 | **59.3%** |
| **Branch Coverage** | N/A | Not available (LLVM source-based coverage does not report branches in LCOV format) |

## Coverage by Module

| Module | Lines Hit | Lines Found | Coverage | Files | Status |
|--------|-----------|-------------|----------|-------|--------|
| `i18n` | 493 | 496 | **99.4%** | 2 | EXCELLENT |
| `shared` | 6 | 6 | **100.0%** | 1 | EXCELLENT |
| `ui` | 302 | 341 | **88.6%** | 3 | GOOD |
| `db` | 1,552 | 1,732 | **89.6%** | 9 | GOOD |
| `profiling` | 409 | 471 | **86.8%** | 2 | GOOD |
| `extensions` | 4,597 | 5,423 | **84.8%** | 26 | GOOD |
| `net` | 1,503 | 1,814 | **82.9%** | 3 | GOOD |
| `lua` | 736 | 880 | **83.6%** | 2 | GOOD |
| `passwords` | 460 | 573 | **80.3%** | 2 | GOOD |
| `input` | 1,121 | 1,285 | **87.2%** | 6 | GOOD |
| `mcp` | 1,223 | 1,673 | **73.1%** | 4 | MODERATE |
| `wm` | 875 | 1,202 | **72.8%** | 3 | MODERATE |
| `platform` | 283 | 398 | **71.1%** | 6 | MODERATE |
| `sync` | 1,238 | 1,853 | **66.8%** | 7 | MODERATE |
| `arp` | 368 | 585 | **62.9%** | 1 | MODERATE |
| `terminal` | 379 | 634 | **59.8%** | 3 | MODERATE |
| `downloads` | 208 | 405 | **51.4%** | 1 | LOW |
| `servo` | 1,076 | 2,287 | **47.0%** | 8 | LOW |
| `app` | 2,752 | 6,859 | **40.1%** | 26 | LOW |
| `lib` | 1,828 | 5,264 | **34.7%** | 14 | LOW |
| `frame_tasks` | 131 | 1,803 | **7.3%** | 2 | CRITICAL |
| `chrome` | 0 | 316 | **0.0%** | 1 | CRITICAL |

## Critical Low-Coverage Areas (<50%)

### `frame_tasks` (7.3%)
- `ipc.rs`: 0/722 lines (0.0%) -- IPC handling completely untested
- `mod.rs`: 131/1081 (12.1%) -- frame task orchestration mostly untested

### `chrome` (0.0%)
- `lib.rs`: 0/316 (0.0%) -- Leptos UI components require browser runtime, not unit-testable

### `lib` (34.7%)
- `app_handler.rs`: 0/835 (0.0%) -- application lifecycle management
- `bootstrap.rs`: 0/236 (0.0%) -- application bootstrap/startup
- `main.rs`: 88/454 (19.4%) -- main entry point
- `wry_actions.rs`: 125/615 (20.3%) -- WebView action handlers
- `offscreen_webview.rs`: 227/1003 (22.6%) -- offscreen rendering pipeline

### `app` (40.1%)
- `event_handler.rs`: 0/588 (0.0%) -- event loop handler
- `events.rs`: 134/608 (22.0%) -- event processing
- `tabs.rs`: 23/241 (9.5%) -- tab management
- `workspaces.rs`: 38/242 (15.7%) -- workspace management
- `downloads.rs`: 17/179 (9.5%) -- download management

### `servo` (47.0%)
- `wry_engine.rs`: 103/745 (13.8%) -- WebView engine integration
- `wry_pages.rs`: 477/989 (48.2%) -- page generation

## Assessment

**Overall: 59.3% line coverage -- BELOW 95% target.**

The project has strong test coverage (>80%) in core subsystems: `i18n`, `extensions`, `net`, `input`, `db`, `profiling`, `lua`, `passwords`, and `ui`. These are the critical logic modules and are well-tested.

Low coverage areas are concentrated in:
1. **Integration/UI layers** (`chrome`, `lib/bootstrap.rs`, `app/event_handler.rs`) -- these require a running windowing system and cannot be unit-tested easily
2. **IPC and frame orchestration** (`frame_tasks/ipc.rs`) -- tightly coupled to runtime state
3. **WebView rendering** (`servo/wry_engine.rs`, `lib/offscreen_webview.rs`) -- depends on WebKitGTK runtime

These are expected gaps for a desktop application with GUI dependencies. The core business logic, data processing, and algorithm modules are well-covered.
