---
document_id: YP-MCP-PROTOCOL-002
version: 2.0.0
status: DRAFT
supersedes: YP-MCP-PROTOCOL-001
domain: MCP Bridge & Relay Protocol
subdomains: [Session Types, Protocol State Machines, Backpressure, Resource Invariants]
applicable_standards: [JSON-RPC 2.0, MCP Specification 2025-06-18, IEEE 1016-2009]
created: 2026-10-29
author: DeepThought
confidence_level: 0.91
tqa_level: 3
research_iterations: 3
---

# YP-MCP-PROTOCOL-002: Session-Typed MCP Bridge Protocol

## YP-2: Executive Summary

**Problem Statement:**
Formalize the MCP bridge/relay protocol as a session-typed state machine such that (1) every request
receives exactly one response, (2) every command targets a pane with an attached webview or is routed
to webview creation, and (3) all result waits are bounded. Field QA (2026-10-24..29) exposed five
production defects, each a violation of one of these three properties:

| Defect | Violated Property | Root Cause |
|--------|-------------------|------------|
| Wayland: every pane tool returned "no active pane" | P2 (pane-webview binding) | handlers consulted only the X11 pane map |
| Pipeline wedge: one read_page_structure starved all commands | P3 (bounded waits) | unbounded `blocking_recv` starved the GTK pump |
| Deferred-command blackhole | P1 (no command lost) | drain-then-defer dropped the batch |
| wait_for never succeeded | P1 (payload fidelity) | JSON-encoded result compared against raw literal |
| MCP-created tabs were phantom panes | P2 | wry-engine pane creation on Wayland |

**Scope:**
- In-scope: relay state machine, command/response pairing, pane binding invariant, bounded waits,
  deferral queue semantics, result payload decoding
- Out-of-scope: transport security (loopback only), multi-client arbitration, HTTP transport

## YP-3: Nomenclature

| Symbol | Description | Domain | Source |
|--------|-------------|--------|--------|
| $S$ | Relay state | $\{Idle, Ready, Wedged\}$ | this paper |
| $C$ | Command alphabet | JSON-RPC 2.0 requests | JSON-RPC 2.0 |
| $R(c)$ | Response to command $c$ | $\mathsf{Value} \uplus \mathsf{Error}$ | JSON-RPC 2.0 |
| $W$ | Webview binding map | $\mathsf{PaneId} \hookrightarrow \mathsf{Webview}$ | this paper |
| $\phi(p)$ | Phantom predicate: $p \notin \mathrm{dom}(W)$ | Boolean | this paper |
| $\beta$ | Wait bound | $10\,s$ (JS results), $15\,s$ (navigation commit) | this paper |

## YP-4: Theoretical Foundation

**Session types (Honda/Yoshida/Carbone 1998; multiparty: Deniélou & Honda 2012).** A protocol is
well-formed when every endpoint's send/receive actions are linearly ordered per channel. The relay
multiplexes one stdio session (client) over N one-shot TCP sessions (per-request bridge dial).
Composition rule (AX-MCP-001): *responses may arrive in any order across connections, but per
connection, responses preserve request order* — guaranteed structurally because each request opens
its own connection and blocks its own line.

**Relation to other ecosystems:**
- **P language / TLA+** (Microsoft): model checking of protocol machines. We adopt the state-machine
  formalization; model checking is future work (ProofMCP.lean proves invariants statically).
- **mpst-rs / session-types crates** (Rust): encode session types in the type system via affine
  linearity. Rejected for the hot path: the relay is dynamic (JSON-RPC is not statically
  session-typed); instead the invariants are enforced in Lean and checked by tests.
- **MCP official lifecycle** (2025-06-18): `initialize` → capability negotiation → `notifications/initialized`
  → operation → shutdown. Aileron's relay deviates *by design*: it forwards lifecycle to the browser
  and tolerates browser restarts between requests (AX-MCP-002). This tolerance semantics — "the
  session survives endpoint death" — is not part of the standard session-type literature, which
  assumes endpoint liveness. It is the novel contribution of this revision.
- **Precedent check (are others solving this?):** JSON-RPC over HTTP (LSP) solves request pairing via
  IDs and drops pairing fidelity on server restart identically to pre-002 aileron. No surveyed
  implementation provides cross-restart response guarantees; ours is achieved by construction
  (AX-MCP-002), not by protocol negotiation.

**Axioms:**
- AX-MCP-001 (per-connection FIFO): responses on one connection preserve request order.
- AX-MCP-002 (restart tolerance): a command MAY be issued while the browser bridge is down; the
  relay MUST return a well-formed JSON-RPC error, never a protocol violation.
- AX-MCP-003 (payload fidelity): string results are delivered decoded (wry's `to_json` wrapper is
  an implementation detail of the transport, not the payload).

## YP-5: Algorithm Specification

**Properties (proven in Aileron/MCP/ProofMCP.lean):**
- PROP-MCP-001 (P1, no command lost): draining a command queue yields exactly the enqueued commands,
  in order; deferral re-enqueues without loss.
- PROP-MCP-002 (P2, phantom-pane binding): a command whose target pane is phantom is routed to
  webview creation, never silently dropped.
- PROP-MCP-003 (P3, bounded wait): every result wait has a finite deadline; on timeout a well-formed
  error response is emitted.
- PROP-MCP-004 (P1, payload fidelity): decoding `to_json` output is an involution on string payloads
  and idempotent on all other JSON values.

**State machine:**
```
Idle --(bridge up)--> Ready --(command batch)--> Ready   [satisfies P1-P4]
Ready --(bridge down)--> Ready   [AX-MCP-002 error responses]
any   --(wedge: no pump progress)--> watchdog restart --> Idle
```

## YP-6: Test Vector Specification

See `.specs/01_research/test_vectors/test_vectors_mcp.toml` (existing) plus QA harness
`scripts/mcp_qa.sh` covering the 2026-10 defect matrix (24-tool live pass on local fixture).

## YP-7: Domain Constraints

- Relay runs off-thread; GTK pump only from the main loop (AX-MCP-004, from YP-GFX-COMPOSITE-001).
- stdio is reserved for the JSON-RPC protocol; diagnostics MUST use stderr (AX-MCP-005).

## YP-8: Bibliography

1. Honda, Yoshida, Carbone — "Multiparty Asynchronous Session Types", POPL 2008.
2. Deniélou, Honda — "Multiparty Session Types Made Simple", PPDP 2012.
3. Microsoft Research — "P: Protocol Programming Language" (state machine checking).
4. Specification "Model Context Protocol", 2025-06-18 revision, lifecycle chapter.
5. JSON-RPC 2.0 Specification, JSON-RPC Working Group.

## YP-9: Knowledge Graph Concepts

session_types, protocol_state_machine, json_rpc_2_0, model_context_protocol,
restart_tolerance, resource_invariant, bounded_wait, backpressure

## YP-10: Quality Checklist

- [x] All five field defects mapped to violated properties
- [x] Cross-ecosystem survey (P, TLA+, mpst-rs, LSP, MCP spec) with adopt/reject rationale
- [x] Novel contribution identified (restart-tolerant relay semantics)
- [x] Lean formalization exists (PROP-MCP-001..004)
- [x] Test vectors referenced
