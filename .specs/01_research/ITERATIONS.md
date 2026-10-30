# Aileron Spec Research Iterations — Log & Program

Method (per iteration, per the standing directive):
1. **Research** — pull state-of-the-art papers/specs for one subsystem.
2. **Analyze** — map findings to the aileron part; classify: solved better elsewhere / solved worse / novel problem.
3. **Implement in spec** — new or revised Yellow Paper (YP-*) following house format.
4. **Adjust the Lean proof** — extend `.specs/02_architecture/proofs/Aileron/`, keep `lake build` green.
5. **Log** — record adopt/reject decisions and the novel-contribution claim here.

Status: **iterations 1–5 complete and pushed (1: protocol session-typing+proofs; 2: backpressure shipped+soaked; 3: pane lifecycle pointwise proofs; 4: nav epochs spec+proof, runtime wired; 5: idle scheduling gate spec+shipped+measured). 6: YP-WORKSPACE-002 shipped + restore-ordering guard in runtime. 7–10: YPs complete (pacing, contracts, adblock indexing, fuzzy benchmarks). Remaining: PL-002/003 list-level proofs, frame-pacing vsync implementation, adblock indexed matcher, epoch runtime exposure in read results.**

---

## Iteration 1 — MCP bridge protocol session typing ✅
- **Research:** MCP spec 2025-06-18 lifecycle; session types (Honda/Yoshida/Carbone POPL'08,
  Deniélou/Honda PPDP'12); P language state-machine checking; LSP/JSON-RPC restart semantics.
- **Classification:** restart-tolerant relay semantics = **novel** (standard session typing assumes
  endpoint liveness; LSP drops pairing on server restart exactly like pre-fix aileron).
- **Spec:** YP-MCP-PROTOCOL-002 (supersedes 001) — 5 field defects mapped to properties P1/P2/P3.
- **Proof:** `Aileron/MCP/ProofMCP.lean` — PROP-MCP-001..004 (route totality, phantom-pane
  never-execute, bounded waits, drain preservation, decode idempotence).
- **Field defects closed:** DEF-1 (Wayland "no active pane"), DEF-2 (pipeline wedge),
  DEF-3 (dropped batch), DEF-4 (quoting bug).

## Iteration 2 — Snapshot backpressure & bounded capture ✅ (spec side)
- **Research:** reactive-streams backpressure (Pivotal spec), OOM casework, wry/WebKitGTK eval
  delivery model (registry source audit).
- **Classification:** bounded-wait + stall-streak cooldown = **standard technique, novel placement**
  (backpressure applied to a UI-process snapshot queue — no surveyed browser does this).
- **Spec:** folded into YP-MCP-PROTOCOL-002 §P3 + field evidence; full YP-GFX-CAPTURE-002 pending.
- **Proof:** modelled as `waitBounded` (PROP-MCP-003); capture cooldown itself verified by soak.
- **Field defects closed:** OOM (50GB shmem), 8-minute snapshot callbacks.

## Iteration 3 — Pane lifecycle resource invariant ✅ (pointwise core; list-level PL-002/003 proofs deferred)
- **Research targets:** linear/affine capabilities for resource tracking (Rust typestate, fractional
  permissions), workspace-restore lifecycle.
- **Spec target:** YP-WM-PANE-LIFECYCLE-002 — invariant `∀ p ∈ wm, p ∈ webviews ∨ p ∈ creationQueue`
  (the phantom-pane defect was a violation).
- **Proof target:** `Aileron/WM/ProofPaneLifecycle.lean` — creation/consume correspondence.

## Iteration 4 — Navigation epoch & read-your-writes ✅ COMPLETE (spec + Lean proof + runtime, incl. the settle-hold stale-event fix 7fb19f5: flush queued load events before the hold, require this load's Started->Complete pair; trace-validated same-connection on heavy pages)
- **Research targets:** linearizability (Herlihy & Wing), session guarantees (Terry et al.),
  epoch-based reclamation.
- **Spec target:** YP-MCP-NAV-EPOCH-001 — per-pane navigation epoch counter; reads gated on epoch
  completion.
- **Proof target:** `Aileron/MCP/ProofNavEpoch.lean` — read-after-navigate monotonicity.

## Iteration 5 — Event loop & idle scheduling 📋
- **Research targets:** winit event loop semantics, libuv design, real-time scheduling (EDF),
  vsync-driven presentation.
- **Spec target:** YP-GFX-SCHED-002 — replace the compositing gate with a principled idle model
  (the ~1000fps spin bug).
- **Proof target:** liveness (renders eventually after dirty) + bounded-idle-CPU invariant.

## Iteration 6 — Workspace persistence & crash recovery 📋
- **Research targets:** write-ahead logging, SQLite WAL internals (already the storage engine),
  actor supervision trees.
- **Spec target:** YP-WORKSPACE-002 — restore-overwrite hazard (navigations racing startup restore,
  observed 2026-10-29) formalized and eliminated.

## Iteration 7 — Compositor frame pacing ✅ YP-GFX-PACING-001 (present bounds + vsync as future work)
- **Research targets:** present-mode literature (wgpu), frame pacing in Android/Chrome compositor.
- **Spec target:** YP-GFX-PACING-001 — presentation modes vs the pooled-compositing path.

## Iteration 8 — Ad-block rule indexing 📋
- **Research targets:** EasyList matcher papers (Diacritic/Trailfire), uBlock's matcher design.
- **Spec target:** revise YP-NET-ADBLOCK-001 with indexing complexity analysis (existing vectors).

## Iteration 9 — Fuzzy matching algorithm 📋
- **Research targets:** fzf algorithm, Selecta, Smith-Waterman variants (existing YP-FUZZY-MATCH-001
  has vectors; add complexity + recall benchmarks).

## Iteration 10 — MCP tool-surface contracts 📋
- **Research targets:** refinement types (LiquidHaskell), design-by-contract (D), API contract
  testing (Pact).
- **Spec target:** per-tool input/output contracts in the YP + Lean pre/post-conditions mirroring
  `read_page_structure` etc.
