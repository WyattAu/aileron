import Lake
open Lake DSL

package aileron_proofs where
  leanOptions := #[⟨`autoImplicit, false⟩]

@[default_target]
lean_lib Aileron where

require mathlib from git
  "https://github.com/leanprover-community/mathlib4.git" @ "v4.29.0"
