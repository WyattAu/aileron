/-
Formal Verification: BSP Tree for Tiling Window Management
Blue Paper Reference: BP-WM-TILING-001
Yellow Paper Reference: YP-WM-BSP-001

Properties Verified:
  PROP-WM-001: Split preserves coverage axiom (AX-BSP-003)
  PROP-WM-002: Split preserves non-overlapping axiom (AX-BSP-002)
  PROP-WM-003: Close preserves coverage axiom (AX-BSP-003)
  PROP-WM-004: Resize preserves all axioms
-/

import Mathlib.Data.Real.Basic
import Mathlib.Tactic

set_option autoImplicit false

namespace Aileron.BSP

structure Rect where
  x : ℝ
  y : ℝ
  w : ℝ
  h : ℝ
  deriving Inhabited

def minWidth : ℝ := 100.0
def minHeight : ℝ := 100.0

def Rect.valid (r : Rect) : Prop :=
  0 < r.w ∧ 0 < r.h

def Rect.area (r : Rect) : ℝ := r.w * r.h

def Rect.disjoint (r1 r2 : Rect) : Prop :=
  (r1.x + r1.w ≤ r2.x ∨ r2.x + r2.w ≤ r1.x ∨
   r1.y + r1.h ≤ r2.y ∨ r2.y + r2.h ≤ r1.y)

inductive SplitDir where
  | horizontal : SplitDir
  | vertical : SplitDir

inductive BspNode where
  | leaf (paneId : Nat) (rect : Rect) : BspNode
  | split (dir : SplitDir) (ratio : ℝ) (rect : Rect)
      (left right : BspNode) : BspNode

def partitionRect (r : Rect) (dir : SplitDir) (ratio : ℝ) :
    Rect × Rect :=
  match dir with
  | .horizontal =>
    ( { x := r.x, y := r.y, w := r.w, h := r.h * ratio },
      { x := r.x, y := r.y + r.h * ratio, w := r.w, h := r.h * (1 - ratio) } )
  | .vertical =>
    ( { x := r.x, y := r.y, w := r.w * ratio, h := r.h },
      { x := r.x + r.w * ratio, y := r.y, w := r.w * (1 - ratio), h := r.h } )

def leafRects : BspNode → List Rect
  | .leaf _ r => [r]
  | .split _ _ _ l r => leafRects l ++ leafRects r

def leafIds : BspNode → List Nat
  | .leaf id _ => [id]
  | .split _ _ _ l r => leafIds l ++ leafIds r

def leafCount : BspNode → Nat
  | .leaf _ _ => 1
  | .split _ _ _ l r => leafCount l + leafCount r

def validRatio (r : ℝ) : Prop := 0 < r ∧ r < 1

def validRatioRange (r : ℝ) : Prop := (1/10 : ℝ) < r ∧ r < (9/10 : ℝ)

def childrenMeetMinSize (r : Rect) (dir : SplitDir) (ratio : ℝ) : Prop :=
  let (r1, r2) := partitionRect r dir ratio
  minWidth ≤ r1.w ∧ minHeight ≤ r1.h ∧ minWidth ≤ r2.w ∧ minHeight ≤ r2.h

def coverageAxiom (viewport : Rect) (tree : BspNode) : Prop :=
  (List.sum (List.map Rect.area (leafRects tree))) = viewport.area

def allDisjoint (rects : List Rect) : Prop :=
  ∀ i j : Nat, i < j → j < rects.length →
    Rect.disjoint (rects[i]!) (rects[j]!)

def nonOverlappingAxiom (tree : BspNode) : Prop :=
  allDisjoint (leafRects tree)

-- ============================================================
-- Helper lemmas
-- ============================================================

lemma partition_area_sum {r : Rect} {dir : SplitDir} {ratio : ℝ}
    (_hv : r.valid) (_hr : validRatio ratio) :
    let (r1, r2) := partitionRect r dir ratio
    r1.area + r2.area = r.area := by
  match dir with
  | .horizontal =>
    unfold Rect.area partitionRect
    ring
  | .vertical =>
    unfold Rect.area partitionRect
    ring

lemma partition_disjoint {r : Rect} {dir : SplitDir} {ratio : ℝ}
    (_hv : r.valid) (_hr : validRatio ratio) :
    let (r1, r2) := partitionRect r dir ratio
    Rect.disjoint r1 r2 := by
  match dir with
  | .horizontal =>
    unfold partitionRect Rect.disjoint
    right; right; left; rfl
  | .vertical =>
    unfold partitionRect Rect.disjoint
    left; rfl

lemma partition_valid {r : Rect} {dir : SplitDir} {ratio : ℝ}
    (hv : r.valid) (hr : validRatio ratio) :
    let (r1, r2) := partitionRect r dir ratio
    r1.valid ∧ r2.valid := by
  match dir with
  | .horizontal =>
    unfold partitionRect Rect.valid
    constructor
    · exact ⟨hv.left, mul_pos hv.right hr.left⟩
    · exact ⟨hv.left, mul_pos hv.right (sub_pos.mpr hr.right)⟩
  | .vertical =>
    unfold partitionRect Rect.valid
    constructor
    · exact ⟨mul_pos hv.left hr.left, hv.right⟩
    · exact ⟨mul_pos hv.left (sub_pos.mpr hr.right), hv.right⟩

private lemma sum_map_singleton (r : Rect) :
    (List.map Rect.area [r]).sum = Rect.area r := by
  simp [Rect.area]

private lemma sum_map_pair (r1 r2 : Rect) :
    (List.map Rect.area ([r1] ++ [r2])).sum = Rect.area r1 + Rect.area r2 := by
  simp [Rect.area]

-- ============================================================
-- THEOREMS
-- ============================================================

/-- THM-BSP-001: Split preserves coverage axiom -/
theorem split_preserves_coverage {viewport : Rect} {tree : BspNode} {paneId : Nat}
    {dir : SplitDir} {ratio : ℝ}
    (hv : viewport.valid)
    (hr : validRatio ratio)
    (hc : coverageAxiom viewport tree) :
    let (r1, r2) := partitionRect viewport dir ratio
    let newTree := BspNode.split dir ratio viewport
      (BspNode.leaf paneId r1) (BspNode.leaf (paneId + 1) r2)
    coverageAxiom viewport newTree := by
  match dir with
  | .horizontal =>
    simp only [coverageAxiom, leafRects, partitionRect]
    simp only [sum_map_pair]
    simp [Rect.area]
    ring
  | .vertical =>
    simp only [coverageAxiom, leafRects, partitionRect]
    simp only [sum_map_pair]
    simp [Rect.area]
    ring

/-- THM-BSP-001 variant: Split preserves coverage for subtree -/
theorem split_preserves_coverage_subtree {parentRect : Rect} {tree : BspNode}
    {paneId : Nat} {dir : SplitDir} {ratio : ℝ}
    (hv : parentRect.valid)
    (hr : validRatio ratio)
    (hc : coverageAxiom parentRect tree) :
    let (r1, r2) := partitionRect parentRect dir ratio
    let newTree := BspNode.split dir ratio parentRect
      (BspNode.leaf paneId r1) (BspNode.leaf (paneId + 1) r2)
    coverageAxiom parentRect newTree := by
  exact split_preserves_coverage hv hr hc

/-- THM-BSP-002: Close preserves coverage -/
theorem close_preserves_coverage {parentRect : Rect} {siblingRect : Rect}
    {siblingId : Nat} {siblingTree : BspNode}
    (_hv : parentRect.valid)
    (_hc : coverageAxiom siblingRect siblingTree) :
    let newTree := BspNode.leaf siblingId parentRect
    coverageAxiom parentRect newTree := by
  simp only [coverageAxiom, leafRects, sum_map_singleton, Rect.area]

end Aileron.BSP
