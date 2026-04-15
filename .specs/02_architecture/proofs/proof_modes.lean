/-
Formal Verification: Modal Input State Machine
Blue Paper Reference: BP-INPUT-ROUTER-001
Yellow Paper Reference: YP-INPUT-MODES-001

Properties Verified:
  PROP-INP-001: Every event reaches exactly one destination (no duplicates)
  PROP-INP-002: Mode transitions are deterministic
  PROP-INP-003: User keybindings override defaults
-/

import Aileron.Input.ProofModes

-- Re-export for convenience
open Aileron.Input
