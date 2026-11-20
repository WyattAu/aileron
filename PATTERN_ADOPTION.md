# PATTERN_ADOPTION: aileron-gate1

Relationship to the WyattAu pattern canon
(`github.com/WyattAu/pattern-standards`), validated by
`scripts/adoption-check.py` per `references/adoption-format.md`.

aileron-gate1 is the GATE-1 experiment: the aileron monolith split into
workspace crates (core, engine, agent, headed, chrome) so the agent-facing
build compiles only the crates it needs; default-members in Cargo.toml
excludes the Leptos chrome from that build.

| Pattern | Status | Evidence |
|---|---|---|
| T13 | adopted | agent/src/runtime.rs:2 (headless MCP-only daemon; the module doc states there is no AppState or headed shell: state, MCP stack and dispatch are core, pane mechanics are engine) |
| T1 | adopted | core/src/net/filter_list.rs:79 (the EasyList parser lives inside the core crate and parses into typed filter values) |
| T48 | n/a | (the agent crate exposes MCP to LLM agents per agent/src/runtime.rs:2; gate1 constrains no LLM decoding, so grammar-constrained decoding has no surface here) |
