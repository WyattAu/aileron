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

import Aileron.BSP.ProofBSP

-- Re-export for convenience
open Aileron.BSP
