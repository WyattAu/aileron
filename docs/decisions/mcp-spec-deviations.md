# MCP-Spec Deviation Notes (Era-2 S50)

Aileron's MCP surface vs the Model Context Protocol specification — the
places we deliberately extend or deviate, for client authors and reviewers.

| # | Area | Spec baseline | Aileron behavior | Rationale |
|---|---|---|---|---|
| 1 | Sessions | MCP has no session concept beyond the transport connection | Each stdio/TCP connection IS a session with a server-assigned UUID; capabilities (κ/φ) attach to it | Multi-agent isolation over one browser (YP-MCP-SESSIONS-002); no protocol break — standard clients work unmodified |
| 2 | Capability narrowing | Not specified | `session_scope` narrows the calling session monotonically; widening is rejected server-side | Structural taint (YP-SEC-CONFINEMENT-001): web-influenced actions can only shrink grants |
| 3 | Tool visibility vs callability | servers advertise all tools | A narrowed session still SEES all tools (tools/list unfiltered) but CALLING outside its κ returns FORBIDDEN | Alpha simplification; filtering tools/list is the Era-3 refinement |
| 4 | Scope persistence | n/a | Caps live per connection; a reconnect re-attaches fresh (unrestricted by default, or the policy profile's ceiling) | The relay error message instructs scoped clients to re-apply session_scope after reconnects |
| 5 | Policy profiles | n/a | `mcp_policy` config (permissive/balanced/strict) sets the DEFAULT ceiling for new connections | Strict = read-only default ceiling; narrowing only ever shrinks from there |
| 6 | Audit | n/a | Every dispatched or denied command is recorded with provenance (session, tool, class, pane, epoch, outcome), readable via `audit_log` | Enterprise audit trail; the ring is the durable record (cap 1000) |
| 7 | initialize as tools/call | initialize is a protocol method | Tolerated: an unknown-tool error is returned | Some test clients send it as a call; harmless |
| 8 | Element references | n/a | Snapshot-stamped `(epoch, ref)` pairs; stale refs fail closed with STALE_EPOCH + guidance | YP-AGENT-ACI-001; beyond spec, it is the product |

Formal guarantees behind these: `.specs/02_architecture/proofs/Aileron/MCP/`
(ProofMCP, ProofNavEpoch, ProofACI, ProofSessions, ProofConfine).
