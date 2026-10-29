/-
Formal Verification: Pane Lifecycle Resource Invariant (pointwise core)
Yellow Paper Reference: YP-WM-PANE-LIFECYCLE-002

Properties Verified (pointwise Bool model):
  THEOREM-PL-001 (routing totality): under coverage, a phantom pane is never
    answered with "no active pane"
  THEOREM-PL-004 (DEF-1 counterexample): the pre-fix wry-only lookup reduces
    to errorResponse whenever the pane is absent from the X11 map

PENDING (next iteration): the list-level enqueue/drain theorems
  (THEOREM-PL-002/003 are specified in the YP; their proofs require
  membership-rewrite plumbing and are deferred to keep this file
  sorry-free and compiling).
-/

import Mathlib.Tactic

set_option autoImplicit false

namespace Aileron.WM

/-! ### Model (pointwise Bool decisions — matches the gate's runtime shape) -/

/-- Routing decision for a command targeting pane `p`. -/
inductive Route where
  | execute : Route
  | createThenExecute : Route
  | errorResponse : Route

/-- The gated routing decision: bound → execute; queued debt → create; else error. -/
def gated (bound queued : Bool) : Route :=
  if bound then Route.execute
  else if queued then Route.createThenExecute
  else Route.errorResponse

/-- INV-PL-002 (coverage), pointwise: an unbound pane carries creation debt. -/
def covered (bound queued : Bool) : Prop :=
  bound = false → queued = true

/-! ### THEOREM-PL-001: routing totality under coverage -/

theorem routing_total (bound queued : Bool) (hcov : covered bound queued) :
    gated bound queued ≠ Route.errorResponse := by
  unfold gated
  by_cases hb : bound
  · simp only [if_pos hb]
    intro hcon
    exact Route.execute.noConfusion hcon
  · have hq := hcov (by simpa using hb)
    simp only [if_neg hb, if_pos hq]
    intro hcon
    exact Route.createThenExecute.noConfusion hcon

/-! ### THEOREM-PL-004: the DEF-1 defect, as a theorem -/

/--
The pre-fix lookup: only the X11 (wry) pane map was consulted. When the
pane lives offscreen (Wayland), `inWry = false` and the lookup collapses
to errorResponse - exactly the DEF-1 field behavior.
-/
def oldLookup (inWry : Bool) : Route :=
  if inWry then Route.execute else Route.errorResponse

theorem def1_collapse (inWry : Bool) (hinWry : inWry = false) :
    oldLookup inWry = Route.errorResponse := by
  unfold oldLookup; rw [hinWry]; simp
end Aileron.WM
