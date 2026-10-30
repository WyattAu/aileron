---
document_id: YP-WORKSPACE-002
version: 1.0.0
status: IMPLEMENTED
domain: Workspace Persistence & Crash Recovery
subdomains: [Restore Ordering, Command Deferral, Crash-Only Software]
applicable_standards: [IEEE 1016-2009]
created: 2026-10-30
author: DeepThought
confidence_level: 0.91
tqa_level: 3
research_iterations: 1
---

# YP-WORKSPACE-002: Restore Ordering & MCP Command Deferral

## YP-2: Executive Summary

**Problem Statement (observed 2026-10-29):** MCP commands issued while a workspace restore is in
flight execute against panes the restore is about to destroy: the same-batch navigate landed on a
pane that the restore then wiped (navigation silently lost), and reads evaluated on the outgoing
document. Any restore — `:ws-load`, session restore, crash recovery — rebuilds the entire pane
set (`wry_panes.remove_all()` + fresh offscreen manager), invalidating every pane id in flight.

**Solution (restore-ordering rule):** MCP commands are not drained from the bridge while a
workspace restore is pending. They stay queued in the bridge channel — which is lossless — and
process against the restored pane set once the restore completes. The pending-restore flag is
consumed within a frame or two, so the deferral is bounded by the restore duration, not by page
loads.

**Cross-ecosystem survey:**
- **Crash-only software** (Beer 2004, Netflix): servers recover by restart; in-flight requests
  fail visibly rather than half-apply. Adopted: commands issued mid-restore queue, they do not
  apply to dying state.
- **Write-ahead ordering** (databases): operations against a table being rebuilt wait for the
  rebuild transaction. Same rule.
- **Actor supervision** (Erlang/OTP): messages to a restarting actor are delivered after it
  restarts; no loss. The bridge channel is exactly this mailbox.
- **Precedent check:** LSP servers queue notifications during initialization; MCP spec requires
  commands after `initialize`. Consistent.

## YP-3: Nomenclature

| Symbol | Description |
|--------|-------------|
| $R$ | Restore-in-flight predicate (`pending_workspace_restore.is_some()`) |
| $Q$ | The bridge channel (lossless mailbox) |

## YP-4: Theoretical Foundation

**Ordering rule (AX-WS-001):** commands processed against pane set $S_i$ must have been drained
when the pane set was stable. A restore transitions $S_i \to S_{i+1}$ atomically; commands drained
during the transition are ambiguous. Deferral until the transition completes restores determinism.

**Why channel-queuing is lossless:** `poll_commands()` is the only drain; not polling leaves
commands in the mpsc channel unbounded (command volume is human-scale). LoadComplete-style flags
are NOT used here — the restore flag clears deterministically (unlike loading flags on custom
schemes, which motivated the separate `is_loading` settle-wait in YP-MCP-NAV-EPOCH-001).

## YP-5: Algorithm Specification

```
process_mcp_commands():
    if restore_in_flight: return          # commands wait in Q
    cmds := Q.drain()
    [loading settle-wait per YP-MCP-NAV-EPOCH-001]
    process cmds against the restored pane set
```

## YP-6: Test Vector Specification

1. Start browser; issue navigate+read in one batch within the startup window
   → both apply to the restored pane set (2026-10-29 defect case, now pass).
2. `:ws-load` during in-flight MCP reads → reads complete or queue; no phantom panes.

## YP-7: Domain Constraints

Restore is user/agent-initiated and completes synchronously in about_to_wait; the deferral never
spans a suspend or an indefinite load.

## YP-8: Bibliography

1. Beer — "Creating Crash-only Software", USENIX LISA 2006.
2. Terry et al. — "Session Guarantees for Weakly Consistent Replicated Data", DIS 1994.
3. SQLite WAL documentation — rebuild-vs-defer ordering for concurrent access.

## YP-9: Knowledge Graph Concepts

restore_ordering, crash_only_software, command_deferral, mailbox_losslessness, workspace_persistence

## YP-10: Quality Checklist

- [x] Restore-overwrite hazard observed, formalized, guarded
- [x] Deferral is bounded (restore flag clears deterministically)
- [x] No command loss (bridge channel holds until drained)
