# Domain Analysis

**Phase:** -1 (Context Discovery)
**Project:** Aileron
**Date:** 2026-04-11
**Analyst:** Domain Analyst

---

## 1. Primary Domain Classification

Desktop application software — specifically a keyboard-driven, tiling web browser/environment for developer power-users. Aileron embeds the Servo web engine and composites rendered web content as wgpu textures inside an egui-based UI shell. It occupies the intersection of web browsers, tiling window managers, and developer tool platforms.

**Domain Taxonomy:**

- **Level 0:** Application Software
- **Level 1:** Desktop Application / Web Browser
- **Level 2:** Tiling Web Environment / Developer Workstation
- **Level 3:** Keyboard-Driven Browser with Embedded Scripting & AI Integration

---

## 2. Domain Decomposition

### 2.1 Rendering & Compositing

Responsible for GPU-accelerated rendering and texture sharing between the Servo web engine and the egui UI framework.

**Key Concerns:**
- wgpu device and surface management
- Servo compositor output as GPU textures (wgpu::Texture)
- Texture sharing bridge between Servo's rendering pipeline and egui's paint callback
- Multi-window texture lifecycle management
- VSync and frame pacing

**Components:**
- GPU texture allocator
- Compositing bridge (Servo → wgpu → egui)
- Frame scheduler
- Render pass orchestration

### 2.2 Window Management

Implements BSP (Binary Space Partition) tree tiling with modal input modes inspired by Neovim.

**Key Concerns:**
- BSP tree operations: split, resize, close, focus navigation
- Modal state machine: Normal, Insert, Command modes
- Event routing from winit to active pane(s)
- Tab management and workspace serialization
- Keyboard mapping and command dispatch

**Components:**
- BSP tree data structure
- Modal state machine (Normal → Insert → Command transitions)
- Event router / keybinding dispatcher
- Tab and workspace manager

### 2.3 Web Engine Integration

Wraps Servo's Embedder API to provide web content rendering within managed panes.

**Key Concerns:**
- Servo embedder lifecycle (create, navigate, destroy)
- SpiderMonkey JavaScript runtime access
- Network request interception for ad-blocking
- DOM inspection and manipulation (for password manager injection)
- Web Inspector / DevTools integration
- Cookie and session management

**Components:**
- Servo embedder wrapper
- Navigation controller
- Network interceptor (ad-blocking pipeline)
- DOM access bridge

### 2.4 Extensibility

Provides programmable extension points via Lua scripting, MCP server protocol, and CLI tool integration.

**Key Concerns:**
- Lua sandboxing and API surface exposure (via mlua)
- MCP server: exposing browser state/actions to LLM agents
- Password manager CLI integration (pass, gopass, etc.)
- Command palette and fuzzy-finding (nucleo crate)
- Plugin discovery and lifecycle

**Components:**
- Lua runtime and scripting API
- MCP server (stdio transport, Anthropic protocol)
- Password manager bridge
- Command palette (nucleo fuzzy finder)
- Plugin loader

### 2.5 Data Persistence

Manages local storage, browsing history, bookmarks, and session state.

**Key Concerns:**
- SQLite database for history, bookmarks, passwords
- Session serialization (JSON/TOML) for workspace restoration
- Configuration file management (XDG-compliant paths)
- Cache management for web content
- Migration between schema versions

**Components:**
- SQLite storage layer
- Session serializer/deserializer
- Configuration manager
- Cache controller

### 2.6 Security

Addresses security concerns arising from web content rendering, credential handling, and AI agent integration.

**Key Concerns:**
- Network-level ad-blocking (Brave adblock crate)
- Credential injection security (password manager DOM manipulation)
- XSS prevention from JavaScript injection
- MCP server access control (exposing browser state to AI agents)
- Sandboxing of Lua scripts
- Secure credential storage

**Components:**
- Ad-blocking filter engine
- Credential injection sandbox
- MCP server access policy
- Lua sandbox boundary
- Secure storage (encrypted credentials)

---

## 3. Applicable Standards

### 3.1 Mandatory Standards

| Standard | Domain | Key Clauses | Applicability |
|----------|--------|-------------|---------------|
| IEEE 1016-2009 | Software Design Descriptions | All clauses | Architecture and design documentation |
| ISO/IEC 12207:2017 | Software Life Cycle Processes | All clauses | Development process governance |
| OWASP Top 10 (2021) | Web Application Security | All categories | Browser security posture |
| NIST SP 800-53 Rev 5 | Security Controls | AC, AU, CM, IA, SC families | MCP server, credential handling |
| ISO/IEC 27001:2022 | Information Security Management | A.5-A.12 controls | Credential storage, data handling |

### 3.2 Domain Standards (Reference)

| Standard | Domain | Purpose | Applicability |
|----------|--------|---------|---------------|
| WebGPU Specification | Graphics | wgpu API compliance | Rendering pipeline |
| Model Context Protocol (MCP) Specification | AI Integration | LLM communication protocol | MCP server implementation |
| XDG Base Directory Specification | Filesystem (Linux) | Config/data/cache paths | Linux deployment |
| HTML5 / CSS3 / ES2024 | Web Standards | Servo rendering compliance | Web engine integration |
| Wayland Protocol | Display Server | Windowing on Linux/Wayland | winit Wayland backend |
| ICCCM / EWMH | X11 | Window management hints | winit X11 backend |

### 3.3 Standard Conflicts

None identified in Phase -1. Conflicts will be tracked in `STANDARD_CONFLICTS.md` as they arise.

---

## 4. Domain-Specific Risks

### CPR-001: Servo Embedder API Stability

**Severity:** High
**Likelihood:** Medium

Servo is under active development. The Embedder API may undergo breaking changes between versions, requiring ongoing maintenance of the integration layer.

**Mitigation:**
- Pin Servo to specific commits/releases
- Abstract behind a trait-based adapter layer
- Monitor Servo changelog for breaking changes
- Contribute upstream to stabilize embedder API

### CPR-002: wgpu Version Compatibility with Servo Rendering Pipeline

**Severity:** High
**Likelihood:** Medium

Servo's internal rendering pipeline may depend on specific wgpu features or versions. Mismatches could cause rendering failures or texture format incompatibilities.

**Mitigation:**
- Align wgpu versions with Servo's dependencies
- Implement fallback rendering paths
- Add integration tests for texture format compatibility

### CPR-003: GPU Texture Sharing Between Servo and egui

**Severity:** Critical
**Likelihood:** High

The compositing bridge that shares GPU textures between Servo's output and egui's paint pipeline is architecturally novel and complex. Texture lifecycle, format conversion, and synchronization issues are likely.

**Mitigation:**
- Prototype the compositing bridge early (Phase 0)
- Use wgpu's native texture sharing mechanisms
- Implement robust error handling and fallback to software rendering
- Benchmark synchronization overhead

### CPR-004: Modal State Machine Race Conditions

**Severity:** Medium
**Likelihood:** Medium

The modal state machine (Normal/Insert/Command) must correctly route events across threads (winit event thread → UI thread → Servo embedder). Race conditions could cause dropped events, incorrect mode transitions, or deadlocks.

**Mitigation:**
- Design the state machine as a single-threaded actor with message passing
- Use crossbeam-channel for thread-safe communication
- Write property-based tests for state transitions
- Consider formal verification with Lean 4 (TBD based on complexity)

### CPR-005: MCP Server Security

**Severity:** High
**Likelihood:** Low (if properly designed)

The MCP server exposes browser state and actions to external LLM agents. Improper access controls could allow unauthorized reading of browsing history, form data, or credential injection.

**Mitigation:**
- Implement granular permission model (per-tool, per-capability)
- Require explicit user approval for sensitive operations
- Audit and log all MCP server interactions
- Follow NIST SP 800-53 AC and AU controls

### CPR-006: JavaScript Injection Security

**Severity:** High
**Likelihood:** Medium

Password manager credential injection requires DOM manipulation via JavaScript. This creates XSS attack surface if the injected scripts or DOM selectors are compromised.

**Mitigation:**
- Sandbox all injected JavaScript
- Use Content Security Policy headers where possible
- Minimize the JavaScript surface for credential injection
- Security audit of all DOM manipulation paths

### CPR-007: Cross-Platform Event Handling

**Severity:** Medium
**Likelihood:** High

Supporting Wayland, X11, macOS, and Windows requires handling platform-specific event semantics (keyboard layout, IME input, window decorations, clipboard, drag-and-drop).

**Mitigation:**
- Rely on winit for platform abstraction where possible
- Implement platform-specific integration tests
- Prioritize Linux (Wayland + X11) for V1.0
- Defer macOS and Windows to V1.1+

---

## 5. Multi-Lingual Requirements

**Priority:** Low for V1.0

| Aspect | Status | Notes |
|--------|--------|-------|
| Primary Language | English | All UI text, documentation, code comments |
| TQA Level | Level 1 | Sufficient for initial research and development |
| i18n Framework | Not required for V1.0 | Defer to post-V1.0 |
| Localization | Not required for V1.0 | English-only initially |
| Cross-Lingual Knowledge Integration | Not required | No multi-language corpus processing |

Aileron's primary user base (developer power-users) is English-dominant. Internationalization may be considered as a V2.0 concern based on user demand.

---

## 6. Domain Boundaries and Interfaces

### External Interfaces

| Interface | Protocol | Direction | Description |
|-----------|----------|-----------|-------------|
| Servo Embedder API | Rust API (in-process) | Bidirectional | Web content rendering and navigation |
| wgpu | Rust API (GPU driver) | Outbound | GPU texture rendering |
| winit | Rust API (OS windowing) | Inbound | Window events, keyboard input |
| MCP Server | JSON-RPC over stdio | Inbound | LLM agent commands |
| Password Manager CLI | Shell commands (stdout/stderr) | Outbound | Credential retrieval |
| Network | HTTP/HTTPS | Bidirectional | Web content fetching (intercepted by ad-blocker) |
| Filesystem | POSIX/OS API | Outbound | Config, data, cache, session storage |

### Internal Domain Boundaries

```
┌─────────────────────────────────────────────────────┐
│                    Aileron Shell                      │
│  ┌───────────┐  ┌──────────────┐  ┌──────────────┐  │
│  │  Window    │  │   Command    │  │   Lua        │  │
│  │  Manager   │  │   Palette    │  │   Runtime    │  │
│  │  (BSP)     │  │   (nucleo)   │  │   (mlua)     │  │
│  └─────┬─────┘  └──────┬───────┘  └──────┬───────┘  │
│        │               │                 │           │
│  ┌─────┴───────────────┴─────────────────┴───────┐  │
│  │              Modal State Machine               │  │
│  │           (Normal/Insert/Command)              │  │
│  └─────────────────────┬─────────────────────────┘  │
│                        │                            │
│  ┌─────────────────────┴─────────────────────────┐  │
│  │            Event Router / Dispatcher            │  │
│  └─────┬──────────┬──────────┬──────────────┬────┘  │
│        │          │          │              │        │
│  ┌─────┴────┐ ┌───┴────┐ ┌──┴──────┐ ┌────┴─────┐ │
│  │  Servo   │ │  MCP   │ │  Ad     │ │ Storage  │ │
│  │ Embedder │ │ Server │ │ Blocker │ │ (SQLite) │ │
│  └─────┬────┘ └────────┘ └─────────┘ └──────────┘ │
│        │                                            │
│  ┌─────┴────────────────────────────────────────┐  │
│  │         Compositing Bridge (wgpu)             │  │
│  │      Servo Textures → egui Paint             │  │
│  └──────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────┘
```
