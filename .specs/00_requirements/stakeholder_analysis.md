# Stakeholder Analysis

**Phase:** 0 (Requirements Engineering)
**Project:** Aileron
**Date:** 2026-04-11
**Analyst:** Requirements Engineer

---

## Stakeholder Registry

| Stakeholder | Role | Primary Concerns | Influence | Priority |
|-------------|------|-----------------|-----------|----------|
| Power-user developers | Primary User | Keyboard-driven workflow, tiling, speed, extensibility | High | Critical |
| Neovim/VSCode users | Primary User | Modal editing, command palette, Lua config, familiar keybindings | High | Critical |
| AI/LLM power users | Primary User | MCP server integration, browser-to-AI workflow | Medium | High |
| Open-source community | Contributor | Clean architecture, documentation, contribution guides | Medium | Medium |
| Security researchers | Auditor | Credential handling, MCP attack surface, JS injection safety | Medium | High |
| Servo project team | Dependency | Embedder API stability, feedback on integration issues | Low | Medium |

---

## Stakeholder Profiles

### Power-User Developers

**Description:** Software engineers who spend significant time in terminal-based workflows, text editors (Neovim, VSCode), and tiling window managers (i3, Sway, tmux). They value keyboard efficiency, composability, and the ability to script and customize their tools.

**Key Needs:**
- Minimize context switching between browser and development tools
- Keyboard-driven navigation without reaching for the mouse
- Tiling layouts to view multiple web resources simultaneously
- Fast startup and rendering performance

**Success Metrics:**
- Aileron replaces their primary browser for development-related browsing
- Productivity improvement in web-based research and documentation workflows

### Neovim/VSCode Users

**Description:** Users deeply familiar with modal editing paradigms (hjkl navigation, command palettes, text objects). They expect software to respect modal conventions and provide Lua or Vimscript-based configuration.

**Key Needs:**
- Normal/Insert/Command mode state machine that behaves predictably
- Command palette with fuzzy finding (similar to `:`, Ctrl+P, telescope)
- Lua-based configuration (`init.lua`) for keybindings, themes, and custom commands
- Keybindings that follow Neovim conventions where applicable

**Success Metrics:**
- Zero-friction adoption of the modal input paradigm
- Ability to replicate their Neovim keybinding muscle memory in the browser

### AI/LLM Power Users

**Description:** Developers who actively use AI coding assistants (Claude Desktop, Cursor, Copilot) and value MCP-based tool integration. They want their browser to serve as a bridge between web content and AI agents.

**Key Needs:**
- MCP server that exposes browser state and web content to LLMs
- Ability to read active page content as Markdown for AI consumption
- Web search capability accessible from within AI workflows
- Localhost access so AI agents can inspect local development servers

**Success Metrics:**
- Seamless Claude/Cursor integration via MCP protocol
- Reduction in manual copy-paste between browser and AI chat

### Open-Source Community

**Description:** Potential contributors, early adopters, and developers interested in Servo integration, Rust GUI development, and browser architecture.

**Key Needs:**
- Well-structured codebase with clear module boundaries
- Comprehensive documentation (architecture, contribution guide)
- Reproducible build environment
- Clear issue tracking and roadmap

**Success Metrics:**
- Active contributor growth
- Community-reported bugs and feature requests
- Successful onboarding of new contributors

### Security Researchers

**Description:** Security-conscious users and auditors who evaluate the browser's security posture, particularly around credential handling, JavaScript injection, and the MCP server's attack surface.

**Key Needs:**
- Transparent credential handling (no plaintext logging, secure memory clearing)
- Sandboxed JavaScript injection for password manager operations
- MCP server access control and authentication
- Clear security documentation and threat model

**Success Metrics:**
- No credential leaks in log output or memory dumps
- MCP server denies unauthorized access attempts
- Passes independent security audit

### Servo Project Team

**Description:** The Servo web engine maintainers and contributors whose Embedder API Aileron depends on. They are a critical dependency stakeholder, not a direct user.

**Key Needs:**
- Constructive feedback on Embedder API usability and stability
- Bug reports with minimal reproduction cases
- Upstream contributions that improve the Embedder API for all consumers
- Adherence to Servo's supported integration patterns

**Success Metrics:**
- Stable integration with Servo releases
- Upstream contributions accepted
- Responsive to breaking API changes

---

## Stakeholder Communication Matrix

| Stakeholder | Communication Channel | Frequency | Format |
|-------------|----------------------|-----------|--------|
| Power-user developers | GitHub Issues, Discussions | Ongoing | Bug reports, feature requests |
| Neovim/VSCode users | GitHub Discussions, README | Ongoing | Keybinding docs, config examples |
| AI/LLM power users | GitHub README, MCP docs | Per release | Integration guides, API docs |
| Open-source community | CONTRIBUTING.md, GitHub | Ongoing | Architecture docs, PR reviews |
| Security researchers | SECURITY.md, GitHub | On report | Security advisories, threat model |
| Servo project team | Servo Zulip, GitHub | As needed | Bug reports, API feedback |
