# Applicable Standards

**Phase:** -1 (Context Discovery)
**Project:** Aileron
**Date:** 2026-04-11

---

## Mandatory Standards

| Standard | Domain | Key Clauses | Applicability | Priority |
|----------|--------|-------------|---------------|----------|
| IEEE 1016-2009 | Software Design Descriptions | All clauses | Architecture documentation, Blue Papers | High |
| ISO/IEC 12207:2017 | Software Life Cycle Processes | All clauses | Development process governance | High |
| OWASP Top 10 (2021) | Web Application Security | All categories (A01–A10) | Browser security posture, credential handling | High |
| NIST SP 800-53 Rev 5 | Security Controls | AC (Access Control), AU (Audit), CM (Config Mgmt), IA (Identification & Auth), SC (System & Communications Protection) | MCP server interface, credential storage, browser state exposure | High |
| ISO/IEC 27001:2022 | Information Security Management | A.5 (Organizational), A.8 (Asset), A.9 (Access), A.12 (Operations) | Credential storage, browsing data, MCP server data handling | Medium |

## Domain Standards (Reference)

| Standard | Domain | Purpose | Applicability | Notes |
|----------|--------|---------|---------------|-------|
| WebGPU Specification | Graphics | wgpu API compliance and feature gates | Rendering pipeline | wgpu tracks upstream WebGPU spec |
| Model Context Protocol (MCP) Specification | AI Integration | LLM communication protocol (JSON-RPC, tools, resources, prompts) | MCP server implementation | Anthropic standard; stdio transport for V1.0 |
| XDG Base Directory Specification v0.8 | Filesystem (Linux) | Config (`~/.config`), data (`~/.local/share`), cache (`~/.cache`) paths | Linux deployment and config management | Primary platform compliance |
| HTML5 / CSS3 / ES2024 | Web Standards | Servo rendering compliance targets | Web engine integration | Servo's compliance determines actual support |
| Wayland Protocol | Display Server | Surface management, input handling, seat management | winit Wayland backend | Primary Linux backend |
| ICCCM / EWMH | X11 | Window manager hints, _NET_WM properties | winit X11 backend | Secondary Linux backend |

## Standards Justification

### IEEE 1016-2009 — Software Design Descriptions

Mandatory for all architecture and design documentation produced as Blue Papers. Ensures consistent, reviewable design artifacts across the project.

### ISO/IEC 12207:2017 — Software Life Cycle Processes

Governs the overall development process from requirements through maintenance. Provides the framework for phase gates and artifact traceability.

### OWASP Top 10 (2021) — Web Application Security

As a web browser rendering arbitrary web content, Aileron must address all OWASP categories:
- **A01 (Broken Access Control):** Credential injection, MCP server permissions
- **A02 (Cryptographic Failures):** Credential storage encryption
- **A03 (Injection):** JavaScript injection for password manager, Lua scripting sandboxing
- **A05 (Security Misconfiguration):** Default security settings
- **A07 (Identification & Auth Failures):** Password manager integration security

### NIST SP 800-53 Rev 5 — Security Controls

Selected control families applicable to Aileron:

| Family | Controls | Application |
|--------|----------|-------------|
| AC (Access Control) | AC-1, AC-3, AC-4, AC-6 | MCP server tool permissions, Lua sandbox boundaries |
| AU (Audit) | AU-2, AU-3, AU-12 | MCP server action logging, security event auditing |
| CM (Configuration Mgmt) | CM-2, CM-6 | Config file integrity, security baseline |
| IA (Identification & Auth) | IA-2, IA-5 | Password manager auth, MCP client auth |
| SC (System & Comms Protection) | SC-7, SC-8, SC-12 | Network interception, TLS enforcement, cryptographic key management |

### ISO/IEC 27001:2022 — Information Security Management

Applicable for credential storage (passwords, cookies), browsing history, and MCP server data. Aligns with OWASP and NIST controls.

## Standard Conflicts

None identified in Phase -1. Potential conflict areas to monitor:

1. **WebGPU Spec vs. wgpu Implementation:** wgpu may not implement all WebGPU features; divergence tracked via wgpu release notes.
2. **XDG Base Dir vs. macOS/Windows conventions:** Platform-specific path resolution required for cross-platform support.
3. **Servo compliance vs. HTML5/CSS3/ES2024 targets:** Servo may lag behind spec compliance; acceptance criteria must be Servo-version-specific.

Conflicts will be tracked in `STANDARD_CONFLICTS.md` at the repository root as they are identified.
