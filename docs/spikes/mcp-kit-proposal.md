# mcp-kit: Estate MCP Servers in Rust — Proposal

**Trigger:** the estate's third-party MCP servers (Node/Python: memory,
github, filesystem, crawlkit, osint) error and time out repeatedly under
agent load — observed continuously across the 2026-10-24..11-02 campaign:
memory tool timeouts, github server restarts mid-session, servers flapping
in and out of the tool catalog. The one server that never errored: aileron
itself — Rust, single binary, 49 tools.

## The evidence, from one session

| Failure mode | Seen | Rust answer |
|---|---|---|
| Tool-call timeouts under parallel-agent load | memory, osint | single-threaded-event-loop-free; tokio or plain threads; ~100x leaner memory so no OOM pressure |
| Server process crashes/restarts (tool catalog churn) | github | one static binary, no npx/pip/uv dependency downloads, no version drift |
| Package manager startup cost (npx/pip resolve at spawn) | several | binary starts in <10ms |
| Config/key handling per-runtime | several | estate keys pattern (spawn-time reads, 0600) uniform across all servers |

## The architecture (generalize aileron's mcp module)

`mcp-kit` = one estate crate with:

- `mcp-kit-core`: JSON-RPC framing (stdio + TCP transports), tool registry,
  typed params, audit ring, capability gate (the κ/φ lattice from
  YP-MCP-SESSIONS-002 — reuse, don't reinvent), session tagging. This is
  aileron's `src/mcp/{bridge,server,transport,tcp_bridge,tools}` minus the
  browser specifics, already proven with 49 tools and the session-typed
  protocol.
- Per-server thin crates on top: `mcp-memory` (SQLite knowledge graph —
  replaces the Node memory server; same entity/observation API),
  `mcp-filesystem`, `mcp-github` (REST client, paginated),
  `mcp-osint`, `mcp-crawlkit` (wraps the existing Rust crawlkit!).
- Each server: static binary, `--help`, keys via the estate key-file
  pattern, healthcheck subcommand for the watchdog pattern.

## Sequencing (pain-ordered)

1. `mcp-kit-core` extracted from aileron's mcp module (the code exists and
   is battle-tested).
2. `mcp-memory` first — the most-timeouted server this session; SQLite KG
   is a day of work on the core.
3. `mcp-github` — the second-painiest; REST + pagination.
4. The rest opportunistically; third-party servers stay until their Rust
   replacement reaches tool parity (per-server parity checklists).

## Non-goals

- Not replacing aileron's own MCP surface (it IS the flagship).
- Not a public product (estate-internal first, publish later if ever).
- No stdio-only limitation: kit ships both transports from day one
  (aileron's dual-transport pattern).
