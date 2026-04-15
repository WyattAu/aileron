# Requirements

**Phase:** 0 (Requirements Engineering)
**Project:** Aileron
**Date:** 2026-04-11
**Analyst:** Requirements Engineer

---

## REQ-GFX: Graphics & Rendering Pipeline

| ID | EARS Pattern | Requirement |
|----|-------------|-------------|
| REQ-GFX-001 | Ubiquitous | The system shall create an OS window via winit supporting Wayland and X11 backends |
| REQ-GFX-002 | Ubiquitous | The system shall initialize a wgpu surface on the created window |
| REQ-GFX-003 | Ubiquitous | The system shall configure Servo to render web content into off-screen wgpu textures |
| REQ-GFX-004 | Ubiquitous | The system shall register Servo's rendered textures with egui's texture manager |
| REQ-GFX-005 | Ubiquitous | The system shall composite egui UI elements over Servo-rendered web content at >=60fps |
| REQ-GFX-006 | Event-Driven | When the window is resized, the system shall propagate the new dimensions to both Servo and egui |
| REQ-GFX-007 | Unwanted Behaviour | If wgpu surface creation fails, the system shall log a descriptive error and exit gracefully |

## REQ-WM: Window Management & Tiling

| ID | EARS Pattern | Requirement |
|----|-------------|-------------|
| REQ-WM-001 | Ubiquitous | The system shall implement a Binary Space Partitioning (BSP) tree for pane layout |
| REQ-WM-002 | Ubiquitous | The system shall support horizontal and vertical pane splitting |
| REQ-WM-003 | Ubiquitous | The system shall support closing panes and rebalancing the BSP tree |
| REQ-WM-004 | Ubiquitous | The system shall support navigating between panes via keyboard shortcuts (h/j/k/l or arrow keys) |
| REQ-WM-005 | Event-Driven | When the window is resized, the system shall resize pane proportions according to the BSP tree layout |
| REQ-WM-006 | Ubiquitous | Each BSP leaf node shall contain an independent Servo webview instance |

## REQ-MODE: Modal Input System

| ID | EARS Pattern | Requirement |
|----|-------------|-------------|
| REQ-MODE-001 | Ubiquitous | The system shall implement a modal state machine with modes: Normal, Insert, Command |
| REQ-MODE-002 | State-Driven | While in Normal Mode, the system shall intercept keystrokes for navigation commands |
| REQ-MODE-003 | State-Driven | While in Insert Mode, the system shall pass keystrokes directly to the focused Servo pane |
| REQ-MODE-004 | State-Driven | While in Command Mode, the system shall route input to the command palette |
| REQ-MODE-005 | Ubiquitous | The system shall provide a configurable keybinding to switch between modes (e.g., `i` for Insert, `Esc` for Normal) |
| REQ-MODE-006 | Ubiquitous | The system shall display the current mode in a status bar |

## REQ-SERVO: Servo Engine Integration

| ID | EARS Pattern | Requirement |
|----|-------------|-------------|
| REQ-SERVO-001 | Ubiquitous | The system shall implement Servo's Embedder traits for engine initialization |
| REQ-SERVO-002 | Ubiquitous | The system shall run SpiderMonkey JS engine and Servo rendering on background threads |
| REQ-SERVO-003 | Ubiquitous | The system shall translate winit keyboard/mouse events to Servo's input event structs |
| REQ-SERVO-004 | Ubiquitous | The system shall listen to Servo navigation events (LoadStarted, LoadComplete, TitleChanged) via MPSC channels |
| REQ-SERVO-005 | Ubiquitous | The system shall support executing arbitrary JavaScript within a pane's context |
| REQ-SERVO-006 | Event-Driven | When Ctrl+E is pressed, the system shall open the current URL in the OS default browser (via `open` crate) |

## REQ-CP: Command Palette

| ID | EARS Pattern | Requirement |
|----|-------------|-------------|
| REQ-CP-001 | Ubiquitous | The system shall provide a fuzzy-finding command palette triggered by a configurable shortcut |
| REQ-CP-002 | Ubiquitous | The system shall search browsing history, bookmarks, and commands using the nucleo fuzzy matcher |
| REQ-CP-003 | Ubiquitous | The system shall render the command palette as an egui overlay over the active web content |
| REQ-CP-004 | Ubiquitous | The system shall support executing selected commands from the palette |

## REQ-AD: Native Ad-Blocking

| ID | EARS Pattern | Requirement |
|----|-------------|-------------|
| REQ-AD-001 | Ubiquitous | The system shall intercept HTTP requests at the network resource loader level |
| REQ-AD-002 | Ubiquitous | The system shall parse standard filter list rules (EasyList/StevenBlack) using the adblock crate |
| REQ-AD-003 | Ubiquitous | The system shall block matching ad/tracking domains before the HTTP request is made |
| REQ-AD-004 | Ubiquitous | The system shall load filter lists from a configurable local path on startup |

## REQ-MCP: LLM MCP Server

| ID | EARS Pattern | Requirement |
|----|-------------|-------------|
| REQ-MCP-001 | Ubiquitous | The system shall run an MCP server on a background tokio thread |
| REQ-MCP-002 | Ubiquitous | The system shall expose a `read_active_pane` tool that extracts DOM content as Markdown |
| REQ-MCP-003 | Ubiquitous | The system shall expose a `search_web` tool for web search |
| REQ-MCP-004 | Ubiquitous | The system shall support both stdio and SSE transport modes for the MCP server |
| REQ-MCP-005 | Optional Feature | Where the MCP server is enabled in configuration, the system shall expose MCP tools to connected clients |

## REQ-LUA: Lua Scripting

| ID | EARS Pattern | Requirement |
|----|-------------|-------------|
| REQ-LUA-001 | Ubiquitous | The system shall load and execute `~/.config/aileron/init.lua` on startup |
| REQ-LUA-002 | Ubiquitous | The system shall expose keybinding configuration to Lua (e.g., `aileron.keymap.set`) |
| REQ-LUA-003 | Ubiquitous | The system shall expose theme configuration to Lua (e.g., `aileron.theme.set`) |
| REQ-LUA-004 | Ubiquitous | The system shall allow creating custom commands via Lua |

## REQ-SEC: Security

| ID | EARS Pattern | Requirement |
|----|-------------|-------------|
| REQ-SEC-001 | Unwanted Behaviour | If credentials are handled by the system, the system shall not expose them in log output |
| REQ-SEC-002 | Unwanted Behaviour | When injecting passwords into the DOM, the system shall clear the injected variables from memory after use |
| REQ-SEC-003 | Unwanted Behaviour | If an unauthenticated client connects to the MCP server, the system shall reject tool requests |
| REQ-SEC-004 | State-Driven | While per-pane session isolation is configured, the system shall isolate session state (cookies/cache) per pane |

## REQ-DB: Data Persistence

| ID | EARS Pattern | Requirement |
|----|-------------|-------------|
| REQ-DB-001 | Ubiquitous | The system shall store browsing history in a local SQLite database |
| REQ-DB-002 | Ubiquitous | The system shall store bookmarks in the SQLite database |
| REQ-DB-003 | Ubiquitous | The system shall follow XDG Base Directory specification for all file paths |
| REQ-DB-004 | Ubiquitous | The system shall support workspace serialization to JSON/TOML files |

## REQ-PLAT: Cross-Platform

| ID | EARS Pattern | Requirement |
|----|-------------|-------------|
| REQ-PLAT-001 | Ubiquitous | The system shall compile and run on x86_64 Linux (Wayland and X11) |
| REQ-PLAT-002 | Ubiquitous | The system shall compile and run on aarch64 macOS (Apple Silicon) |
| REQ-PLAT-003 | Ubiquitous | The system shall compile and run on x86_64 Windows |

---

## Requirements Traceability Matrix

| Phase (init_requirements.md) | Requirements |
|------------------------------|-------------|
| Phase 1: Core Infrastructure | REQ-DB-001, REQ-DB-002, REQ-DB-003, REQ-PLAT-001..003 |
| Phase 2: Windowing & Graphics | REQ-GFX-001..007, REQ-PLAT-001..003 |
| Phase 3: Servo Integration | REQ-SERVO-001..006 |
| Phase 4: Tiling & Modality | REQ-WM-001..006, REQ-MODE-001..006, REQ-CP-001..004 |
| Phase 5: Killer Features | REQ-AD-001..004, REQ-MCP-001..005 |
| Phase 6: Configuration & Scripting | REQ-LUA-001..004 |
| Phase 7: Build & CI/CD | REQ-PLAT-001..003 |
| Cross-cutting | REQ-SEC-001..004, REQ-DB-003, REQ-DB-004 |
