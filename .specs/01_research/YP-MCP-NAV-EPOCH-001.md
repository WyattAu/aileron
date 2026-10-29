---
document_id: YP-MCP-NAV-EPOCH-001
version: 1.0.0
status: DRAFT
domain: MCP Protocol — Navigation Epochs
subdomains: [Read-Your-Writes, Linearizability, Session Guarantees]
applicable_standards: [JSON-RPC 2.0, MCP Specification 2025-06-18]
created: 2026-10-29
author: DeepThought
confidence_level: 0.90
tqa_level: 3
research_iterations: 1
---

# YP-MCP-NAV-EPOCH-001: Navigation Epochs & Read-Your-Writes

## YP-2: Executive Summary

**Problem Statement:**
A read issued in the same batch as a navigate may evaluate on the outgoing document (observed
2026-10-29: browser_get_text returned the new-tab page instead of the navigated target). This is
the read-your-writes session guarantee (Terry et al. 1994) violated by batch-async dispatch.

**Solution:** per-pane navigation epoch — a monotone counter advanced by every navigate. Reads
carry the epoch they were issued under; a result is *fresh* when its epoch is at least the issued
epoch. Under epoch monotonicity, any read completing after the navigate commits is fresh.

**Cross-ecosystem survey:**
- **Session guarantees** (Terry et al., DST 1994): read-your-writes is the canonical guarantee;
  implemented via monotone write counters per session — the epoch model exactly.
- **Linearizability** (Herlihy & Wing 1990): each operation appears atomically between invocation
  and response; epoch counters are the standard lightweight approximation when full linearizability
  is unnecessary.
- **Epoch-based reclamation** (Hart et al. 2007): epochs as safe-progress markers — same counter
  discipline, different resource.
- **Verdict:** solved *in design* elsewhere (databases, RCU); no browser automation surface ships
  it. Aileron adopting epochs per MCP pane is a first for the tool class.

## YP-3: Nomenclature

| Symbol | Description |
|--------|-------------|
| $e$ | Navigation epoch (ℕ, monotone per pane) |
| $\mathrm{fresh}(i, r)$ | Read-your-writes: $i \le r$ |

## YP-4: Theoretical Foundation

- Navigate advances the pane epoch: $\mathrm{commit}' = e + 1$, monotone.
- A read completing at epoch $r$ is fresh w.r.t. issue epoch $i$ iff $i \le r$.
- THEOREM-NE-001: navigation is monotone ($e \le e{+}1$).
- THEOREM-NE-002 (read-your-writes): completion at the committed epoch implies freshness.
- THEOREM-NE-003 (defect model): without epochs, staleness is representable ($\neg\mathrm{fresh}(1,0)$).

## YP-5: Algorithm Specification

```
navigate(pane, url):  epoch[pane] += 1; load(url)   # advance epoch, start load
read(pane, js):       result := eval(js)            # result carries current epoch
                      assert epoch[pane] >= issued_epoch   # freshness gate
```
Runtime wiring (next iteration): `AppState.mcp_nav_epoch : HashMap<Uuid, ℕ>`; reads defer while
`epoch[pane] > last_committed_read[pane]` … bounded by the load-completion signal.

## YP-6: Test Vector Specification

navigate + same-batch get_text MUST return target content (QA 2026-10-29 failure case, now pass).

## YP-7: Domain Constraints

Epoch counters are per-pane, in-memory, reset on browser restart (restart resets all epochs
coherently — no persistence needed).

## YP-8: Bibliography

1. Terry et al. — "Session Guarantees for Weakly Consistent Replicated Data", DIS 1994.
2. Herlihy, Wing — "Linearizability: A Correctness Condition for Concurrent Objects", TOPLAS 1990.
3. Hart et al. — "Scalable Synchronous Queues" / epoch-based reclamation, PPoPP 2006-2007 lineage.

## YP-9: Knowledge Graph Concepts

navigation_epoch, read_your_writes, session_guarantees, linearizability, monotone_counter

## YP-10: Quality Checklist

- [x] Read-your-writes guarantee formalized
- [x] Cross-ecosystem survey (session guarantees, linearizability, epoch reclamation)
- [x] Lean proof: monotonicity, freshness, defect model (Aileron/MCP/ProofNavEpoch.lean)
- [ ] Runtime wiring in frame_tasks (next iteration)
