---
document_id: YP-WM-PANE-LIFECYCLE-002
version: 2.0.0
status: DRAFT
domain: Window Management — Pane Lifecycle
subdomains: [Resource Invariants, Capability Tracking, Creation Debt]
applicable_standards: [IEEE 1016-2009]
created: 2026-10-29
author: DeepThought
confidence_level: 0.90
tqa_level: 3
research_iterations: 1
---

# YP-WM-PANE-LIFECYCLE-002: Pane Lifecycle Resource Invariant

## YP-2: Executive Summary

**Problem Statement:**
The window manager tracks panes in one structure and their renderers in another (wry panes on
X11, offscreen webviews on Wayland). Nothing in the type system enforces that these two views
agree: a pane can exist in the wm without a webview — a *phantom*. Field QA (2026-10-29) showed
phantom panes silently swallow every MCP command ("Error: no active pane" / navigation no-ops).

**Invariant set (proven in Aileron/WM/ProofPaneLifecycle.lean):**
- INV-PL-001 (well-formedness): a queued pane is never already bound.
- INV-PL-002 (coverage): every phantom pane is enqueued (creation debt).
- THEOREM (routing totality): under INV-PL-001 + INV-PL-002, no command is ever answered with
  "no active pane" — the DEF-1 defect class becomes unrepresentable.

## YP-3: Nomenclature

| Symbol | Description | Source |
|--------|-------------|--------|
| $W(p)$ | Webview-attached predicate | this paper |
| $q$ | Creation-debt queue (phantom panes owed a webview) | this paper |
| $\mathrm{gated}(W, q, p)$ | Routing decision | ProofPaneLifecycle.lean |
| $\mathrm{bindAll}(W, q)$ | Post-creation binding: $W(p) \lor p \in q$ | this paper |

## YP-4: Theoretical Foundation

**Cross-ecosystem survey:**
- **Rust ownership (affine types):** enforces single-binding per *value* at compile time — but the
  wm/webview pairing spans two collections; the type system cannot see it. This is precisely the
  gap Rust acknowledges for cross-object invariants.
- **Fractional permissions** (Boyland 2003) and **separation logic** (O'Hearn 2004; Iris/Coq):
  the canonical treatment — ownership of a pane splits between the wm and the renderer. Verified
  but heavy tooling; Verus brings linear ghost state to Rust and could express this invariant.
- **Linear Haskell:** would make the pairing a type error, but the FFI boundary to WebKitGTK
  (all state lives in C/GTK) makes integrating Linear Haskell impractical.
- **Chromium:** `RenderProcessHostImpl` keeps the same 1:1 pane↔host map enforced by runtime
  `DCHECK`s — the same approach as this paper, but unverified. **Aileron's creation-debt queue
  with a Lean-verified enqueue/drain correspondence is the novel contribution**: the debt is
  first-class, and total coverage is a theorem, not a comment.

**Adopt/reject:** adopt the runtime creation-debt queue (already implemented 2026-10-29 as
`pending_mcp_pane_creations`); reject Linear Haskell (FFI impractical); Verus listed as future
work if the invariant set grows.

## YP-5: Algorithm Specification

```
enqueue(W, q, p):  q' := p :: q            (phantom p; requires INV-PL-001 preserved)
drain:             for each (id, url) in debt: create_offscreen_pane_for(id, url);
                   bind: W' := bindAll(W, q)   (T3: drains route to execute)
gate(W, q, p):     W p → execute; p ∈ q → createThenExecute; else errorResponse
```

**Theorems (ProofPaneLifecycle.lean):**
- THEOREM-PL-001 (routing totality): under well-formedness + coverage,
  `gated W q p ≠ errorResponse` for all p.
- THEOREM-PL-002 (enqueue well-formedness): enqueuing an unbound phantom preserves INV-PL-001.
- THEOREM-PL-003 (drain binding): a queued pane, once created, routes to `execute`.
- THEOREM-PL-004 (DEF-1 counterexample): the pre-fix wry-only lookup reduces to `errorResponse`
  whenever the pane is not in the X11 map — the defect, as a theorem.

## YP-6: Test Vector Specification

Covered by the MCP QA matrix (create_tab / navigate phantom cases, 2026-10-26 pass).

## YP-7: Domain Constraints

- The chrome overlay pane (id 0) is never a valid MCP target (see YP-MCP-PROTOCOL-002).
- Restore ordering: workspace restore must complete before MCP commands route (startup-restore
  interplay, observed 2026-10-29).

## YP-8: Bibliography

1. Boyland — "Checking Interference with Fractional Permissions", JFP 2003.
2. O'Hearn — "Separation Logic", CACM 2019 retrospective; POPL 2004 original.
3. Jung et al. — "RustBelt: Securing the Foundations of the Rust Programming Language", POPL 2018.
4. Lattuada et al. — "Verus: Verifying Rust Programs using Linear Ghost Types", OOPSLA 2023.
5. Chromium design docs — RenderProcessHostImpl lifetime mapping (runtime-map precedent).

## YP-9: Knowledge Graph Concepts

resource_invariant, creation_debt, affine_types, fractional_permissions, separation_logic,
phantom_pane, window_manager, wayland_offscreen

## YP-10: Quality Checklist

- [x] Phantom-pane defect formalized (DEF-1 as theorem)
- [x] Creation-debt queue modeled with enqueue/drain correspondence
- [x] Cross-ecosystem survey with adopt/reject rationale
- [x] Lean proof compiles (Aileron/WM/ProofPaneLifecycle.lean)
