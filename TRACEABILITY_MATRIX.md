# Cross-Paper Traceability Matrix

> **Status:** Populated after Phases 0-4 of R&D lifecycle.
> **Last Updated:** 2026-04-11

## Yellow Paper to Yellow Paper Dependencies

| Source YP | Target YP | Dependency Type | Elements |
|-----------|-----------|-----------------|----------|
| — | — | — | — (all YPs are independent for V1.0) |

## Yellow Paper to Blue Paper Mapping

| Yellow Paper | Blue Paper | Elements Used | Verification |
|--------------|------------|---------------|--------------|
| YP-WM-BSP-001 | BP-WM-TILING-001 | THM-BSP-001, THM-BSP-002, THM-BSP-003, ALG-BSP-001..003 | Unit tests + Lean4 proof |
| YP-GFX-COMPOSITE-001 | BP-GFX-COMPOSITOR-001 | THM-GFX-001, THM-GFX-002, ALG-GFX-001..002 | Benchmarks |
| YP-INPUT-MODES-001 | BP-INPUT-ROUTER-001 | THM-MODE-001..003, ALG-MODE-001 | Unit tests + Lean4 proof |
| YP-MCP-PROTOCOL-001 | BP-MCP-SERVER-001 | THM-MCP-001, THM-MCP-002, ALG-MCP-001..002 | Integration tests |
| YP-NET-ADBLOCK-001 | BP-ADBLOCK-001 | THM-AD-001, THM-AD-002, ALG-AD-001..002 | Unit tests |
| YP-FUZZY-MATCH-001 | BP-UI-CMD-PALETTE-001 | THM-FZ-001..003, ALG-FZ-001 | Benchmarks |

## Blue Paper to Blue Paper Dependencies

| Blue Paper | Depends On | Interface |
|------------|------------|-----------|
| BP-APP-CORE-001 | BP-WM-TILING-001, BP-GFX-COMPOSITOR-001, BP-INPUT-ROUTER-001 | IF-APP-INIT-001 |
| BP-WM-TILING-001 | BP-SERVO-INTEGRATION-001 | IF-WM-PANE-001 |
| BP-GFX-COMPOSITOR-001 | BP-SERVO-INTEGRATION-001 | IF-GFX-TEXTURE-001 |
| BP-INPUT-ROUTER-001 | BP-LUA-ENGINE-001 | IF-INPUT-KEYBIND-001 |
| BP-MCP-SERVER-001 | BP-SERVO-INTEGRATION-001 | Servo DOM access |
| BP-UI-CMD-PALETTE-001 | BP-DB-STORAGE-001 | History search |
| BP-ADBLOCK-001 | BP-SERVO-INTEGRATION-001 | IF-AD-FILTER-001 |

## Requirement Coverage by Papers

| Requirement | Yellow Paper(s) | Blue Paper(s) | Test Coverage |
|-------------|-----------------|---------------|---------------|
| REQ-GFX-001..007 | YP-GFX-COMPOSITE-001 | BP-GFX-COMPOSITOR-001, BP-APP-CORE-001 | TV-GFX-001..005 |
| REQ-WM-001..006 | YP-WM-BSP-001 | BP-WM-TILING-001 | TV-BSP-001..008 |
| REQ-MODE-001..006 | YP-INPUT-MODES-001 | BP-INPUT-ROUTER-001 | TV-MODE-001..008 |
| REQ-SERVO-001..006 | YP-GFX-COMPOSITE-001 | BP-SERVO-INTEGRATION-001 | Integration tests |
| REQ-CP-001..004 | YP-FUZZY-MATCH-001 | BP-UI-CMD-PALETTE-001 | TV-FZ-001..006 |
| REQ-AD-001..004 | YP-NET-ADBLOCK-001 | BP-ADBLOCK-001 | TV-AD-001..005 |
| REQ-MCP-001..005 | YP-MCP-PROTOCOL-001 | BP-MCP-SERVER-001 | TV-MCP-001..006 |
| REQ-LUA-001..004 | — | BP-LUA-ENGINE-001 | AC-LUA-001..004 |
| REQ-SEC-001..004 | — | All components | ST-SEC-001..014 |
| REQ-DB-001..004 | — | BP-DB-STORAGE-001 | AC-DB-001..004 |
| REQ-PLAT-001..003 | — | BP-APP-CORE-001 | Platform CI |

## Formal Verification Coverage

| Property | Yellow Paper Theorem | Lean4 Proof | Status |
|----------|---------------------|-------------|--------|
| PROP-WM-001: Split preserves coverage | THM-BSP-001 | proof_bsp.lean: split_preserves_coverage | ✅ VERIFIED |
| PROP-WM-002: Split preserves non-overlapping | THM-BSP-001 | proof_bsp.lean: partition_disjoint | ✅ VERIFIED |
| PROP-WM-003: Close preserves coverage | THM-BSP-002 | proof_bsp.lean: close_preserves_coverage | ✅ VERIFIED |
| PROP-WM-004: Resize preserves axioms | THM-BSP-003 | proof_bsp.lean (structural) | ✅ VERIFIED |
| PROP-INP-001: Every event reaches one destination | THM-MODE-001 | proof_modes.lean: route_exhaustive | ✅ VERIFIED |
| PROP-INP-002: Transitions are deterministic | THM-MODE-002 | proof_modes.lean: transition_deterministic | ✅ VERIFIED |
| PROP-GFX-001: Frame time within budget | THM-GFX-001 | Benchmark (not proof) | ⏳ PENDING |
| PROP-APP-001: Init without panic | — | Unit test | ⏳ PENDING |
| PROP-APP-002: Graceful shutdown | — | Valgrind | ⏳ PENDING (needs Valgrind) |

## Execution Plan to Test Mapping

| Task | Test Vectors | Acceptance Criteria |
|------|-------------|-------------------|
| TASK-007 (BSP tree) | TV-BSP-001..008 | AC-WM-001..005 |
| TASK-008 (Mode machine) | TV-MODE-001..008 | AC-MODE-001..005 |
| TASK-006 (Compositor) | TV-GFX-001..005 | AC-GFX-001..006 |
| TASK-022 (MCP server) | TV-MCP-001..006 | AC-MCP-001..004 |
| TASK-021 (Ad-blocker) | TV-AD-001..005 | AC-AD-001..003 |
| TASK-015 (Fuzzy search) | TV-FZ-001..006 | AC-CP-001..004 |

## Security Requirements Mapping

| Threat ID | Requirement | Test | Status |
|-----------|-------------|------|--------|
| T-INFO-001 | REQ-SEC-001 | ST-SEC-001..003 | PLANNED |
| T-INFO-002 | REQ-SEC-003 | ST-SEC-006 | PLANNED |
| T-ELEV-001 | REQ-SEC-002 | ST-SEC-013..014 | PLANNED |
| T-ELEV-002 | — | ST-SEC-007 | PLANNED |
| T-TAMP-001 | — | ST-SEC-008..010 | PLANNED |
