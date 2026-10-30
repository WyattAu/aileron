---
document_id: YP-MCP-CONTRACTS-001
version: 1.0.0
status: DRAFT
domain: MCP Tool-Surface Contracts
subdomains: [Design by Contract, Refinement Types, Preconditions]
created: 2026-10-30
author: DeepThought
confidence_level: 0.90
tqa_level: 3
research_iterations: 1
---

# YP-MCP-CONTRACTS-001: Per-Tool Pre/Post-Condition Contracts

## YP-2: Executive Summary

**Problem Statement:** the 24-tool surface is validated by the QA matrix (behavioral testing),
but individual tool contracts (which arguments are required, what a tool promises on success)
are implicit in handler code. Contracts make them explicit, machine-checkable, and the basis of
the tool-schema generation.

**Survey:** design-by-contract (Meyer, Eiffel), refinement types (LiquidHaskell — pre/post
per function), Pact (consumer-driven contract testing), and the MCP spec's own inputSchema
(JSON Schema per tool — ALREADY present: every aileron tool declares required args and types).
**Verdict:** the JSON-Schema layer already covers argument shape; what is missing is the
*semantic* layer — state preconditions (e.g., navigate requires an attached webview — the
phantom-pane class) and result postconditions (e.g., get_text returns the committed page —
the staleness class). Both were formalized this campaign: P2 (phantom binding) and
read-your-writes (YP-MCP-NAV-EPOCH-001) ARE the contracts, proven in Lean.

**Adopted:** contracts as Lean theorems over the routing/wait model (done: PROP-MCP-002,
navigation commit-hold, freshness gate), with the JSON-Schema layer as the syntax check.
**Rejected:** runtime contract-checking wrappers per call (double validation, latency).

## YP-3: Nomenclature

| Symbol | Description |
|--------|-------------|
| $\mathrm{pre}(t)$ | Tool t's state precondition |
| $\mathrm{post}(t)$ | Tool t's result postcondition |

## YP-4: Theoretical Foundation

The contract stack, per tool:
1. **Syntax** (JSON-Schema, shipped): argument presence and types.
2. **State precondition** (Lean, proven): the pane binding invariant — every command targets
   a bound or creation-queued pane (PROP-MCP-002 / THEOREM-PL-001).
3. **Timing precondition** (Lean, proven): pump-waiting commands wait for navigation commit
   (the settle-hold, THEOREM-NE-002 read-your-writes).
4. **Result postcondition** (QA matrix, behavioral): documented per tool in the test vectors.

**Cross-ecosystem verdict:** Eiffel/LiquidHaskell verify contracts on first-party code; MCP
tools span a process boundary, so verification belongs to the protocol model (Lean) plus the
behavioral matrix (QA), not runtime assertion wrappers.

## YP-5: Algorithm Specification

Contract registry = the tool list + per-tool (pre, post) pairs documented in the YP and
executed by the QA matrix. No runtime wrapper.

## YP-6: Test Vector Specification

The 24-tool matrix IS the contract test suite (QA 2026-10-26..30 passes).

## YP-7: Domain Constraints

Tool schemas are generated from handler definitions; adding semantic contracts to the schema
layer (JSON-Schema extensions) is future work if agents misbehave.

## YP-8: Bibliography

1. Meyer — "Object-Oriented Software Construction" (Design by Contract).
2. LiquidHaskell — refinement type verification.
3. Pact — consumer-driven contract testing.
4. MCP Specification — tool schema requirements.

## YP-9: Knowledge Graph Concepts

design_by_contract, refinement_types, json_schema, tool_contracts, mcp_surface

## YP-10: Quality Checklist

- [x] Contract stack layered (syntax → state → timing → behavioral)
- [x] Lean layer proven (PROP-MCP-002, navigation commit-hold)
- [x] Behavioral layer: the 24-tool QA matrix
- [x] Runtime wrappers rejected with rationale
