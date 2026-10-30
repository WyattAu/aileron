---
document_id: YP-GFX-SCHED-002
version: 1.0.0
status: IMPLEMENTED
supersedes: YP-GFX-COMPOSITE-001 (compositing sections)
domain: Graphics — Event Loop & Idle Scheduling
subdomains: [Compositing, Backpressure, Power]
applicable_standards: [IEEE 1016-2009]
created: 2026-10-30
author: DeepThought
confidence_level: 0.92
tqa_level: 3
research_iterations: 1
---

# YP-GFX-SCHED-002: Event-Loop Idle Scheduling (Compositing Gate)

## YP-2: Executive Summary

**Problem Statement:** the Wayland about_to_wait path composited and requested redraw on every
iteration unconditionally — an unthrottled ~1000fps loop while idle (measured: ~720k Frame-log
records, browser RSS growth correlation, constant CPU burn).

**Model (shipped 2026-10-29, compositing gate in app_handler):** the composite+present stage runs
only when at least one offscreen pane was dirty this iteration, a terminal pane is open, or the
first frame is pending; otherwise about_to_wait returns immediately and the last presented frame
persists. Idle CPU drops to near zero (measured: 22 ticks / 10s vs thousands).

**Cross-ecosystem survey:** this is the standard dirty-rect/model invalidation discipline
(Skia's damage tracking, Chromium's BeginFrame scheduling, Android's Choreographer). Aileron's
defect was not the absence of the pattern but an unconditional `request_redraw` fighting it —
the gate restores the invariant "no dirty ⇒ no frame".

## YP-3: Nomenclature

| Symbol | Description |
|--------|-------------|
| $D$ | Dirty-pane count this iteration (`capture_dirty_frames()` return) |
| $T$ | Terminal panes open (egui-rendered; continuous by design) |
| $F_0$ | First-frame pending |

## YP-4: Theoretical Foundation

**Theorem (idle bound, verified by measurement):** with $D = 0 \land \neg T \land \neg F_0$,
about_to_wait performs no composite, no present, and requests no redraw; the loop parks until an
external event. With any of the three set, exactly one frame is produced per event cycle.

**Relation to other ecosystems:** Chromium's BeginFrame delimiters, Android Choreographer, and
Skia damage tracking implement the same contract with vsync alignment; aileron's wgpu present
path currently lacks vsync registration (future work: YP-GFX-PACING-001).

## YP-5: Algorithm Specification

```
D := capture_dirty_frames()
if ¬wry_panes ∧ D = 0 ∧ ¬F0: return            # idle: no work
composite(content ∪ chrome); present; request_redraw
```

Backpressure (per-pane): snapshot stall streaks drive an exponential capture cooldown; the
`webview_is_loading` promotion rule never promotes on an unstarted load (read-your-writes).

## YP-6: Test Vector Specification

QA 2026-10-29: idle CPU 22 ticks/10s (was thousands); navigate+read rounds flat at 3.2–4.2GB RSS
(was 50GB/hour unbounded). Overnight cycles 3–4 clean.

## YP-7: Domain Constraints

Terminal panes opt out of the gate (continuous egui rendering by design). The gate returns before
any GTK pump — the idle path costs zero syscalls.

## YP-8: Bibliography

1. Chromium design docs — "BeginFrame: Frozen Frames & Damage Tracking".
2. Android Open Source — Choreographer frame pacing.
3. Skia — damage-rect compositor notes.

## YP-9: Knowledge Graph Concepts

dirty_rect, damage_tracking, begin_frame, idle_gate, backpressure, vsync

## YP-10: Quality Checklist

- [x] Shipped and soak-verified (commits 57a8dc0, bad004b lineage)
- [x] Idle CPU measured near-zero; RSS flat across navigate+read rounds
- [ ] Future: vsync registration for the present path (YP-GFX-PACING-001)
