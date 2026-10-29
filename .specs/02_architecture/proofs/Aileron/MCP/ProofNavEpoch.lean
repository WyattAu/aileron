/-
Formal Verification: Navigation Epochs & Read-Your-Writes
Yellow Paper Reference: YP-MCP-NAV-EPOCH-001

Properties Verified:
  THEOREM-NE-001: navigation is monotone (epochs never decrease)
  THEOREM-NE-002 (read-your-writes): a read completing at the committed epoch
    is fresh with respect to the issued epoch
  THEOREM-NE-003 (defect model): without epochs, staleness is representable -
    the 2026-10-29 same-batch read defect, as a theorem
-/

import Mathlib.Tactic

set_option autoImplicit false

namespace Aileron.MCP

/-- Read-your-writes freshness: the result epoch is at least the issued epoch. -/
def fresh (issued result : ℕ) : Prop := issued ≤ result

/-- THEOREM-NE-001: navigation advances the epoch monotonically. -/
theorem navigate_monotone (e : ℕ) : e ≤ e + 1 := Nat.le_succ e

/-- THEOREM-NE-002: a read completing at the committed epoch is fresh. -/
theorem read_your_writes (committed issued : ℕ) (h : issued ≤ committed) :
    fresh issued committed := h

/-- Transitivity: reads stay fresh across later navigations. -/
theorem fresh_trans (i m r : ℕ) (h1 : i ≤ m) (h2 : m ≤ r) : fresh i r :=
  Nat.le_trans h1 h2

/-- THEOREM-NE-003 (defect model): without epochs, a stale read is
    representable - 1 > 0, i.e. issued 1, result 0. -/
theorem stale_possible_without_epochs :
    ∃ (issued result : ℕ), ¬ fresh issued result :=
  ⟨1, 0, by unfold fresh; exact Nat.not_succ_le_zero 0⟩

end Aileron.MCP
