---
document_id: YP-GFX-PACING-001
version: 1.0.0
status: DRAFT
domain: Graphics — Frame Pacing
subdomains: [Presentation, Vsync, Idle Power]
created: 2026-10-30
author: DeepThought
confidence_level: 0.88
tqa_level: 3
research_iterations: 1
---

# YP-GFX-PACING-001: Present-Path Frame Pacing

## YP-2: Executive Summary

**Problem Statement:** the wgpu present path lacks vsync registration (FIFO swapchain mode or
surface present-mode negotiation). The idle compositing gate (YP-GFX-SCHED-002) bounds idle work;
the pacing spec bounds ACTIVE work: at most one present per refresh interval under continuous
damage, and zero presents when idle.

**Survey:** Chromium BeginFrame (frame-source discipline), Android Choreographer (vsync-aligned
callbacks), wgpu surface present modes (FIFO = vsync, Mailbox = triple-buffer, Immediate = uncapped).
Adopted: FIFO semantics as the target; Mailbox as the low-latency opt-in.

**Novel aspect:** pacing composes with the dirty-gate — presents are gated by damage FIRST
(power), then aligned to the display interval SECOND (quality). Uncapped-then-damage-gated
pipelines (the pre-fix aileron) get the worst of both: unbounded spin AND visual tearing.

## YP-3: Nomenclature

| Symbol | Description |
|--------|-------------|
| $B$ | Frame budget: one display refresh interval (16.67ms @ 60Hz) |
| $P$ | Presents per wall-clock second |
| $D$ | Damage events per second |

## YP-4: Theoretical Foundation

**Theorem (active bound):** under damage-driven scheduling, $P \le D$ — presents cannot exceed
damage events. **Theorem (idle bound):** with $D = 0$, $P = 0$ — the shipped compositing gate.
**Open (vsync registration):** aligning the present to the display's refresh interrupt removes
tearing; requires surface present-mode negotiation (FIFO) and a frame callback from the display
compositor. Tracked as the post-spike graphics work item.

## YP-5: Algorithm Specification

```
on damage: mark dirty; schedule frame at next vsync (FIFO)
on frame:  if dirty: composite, present once; clear dirty
           else: skip present (gate: YP-GFX-SCHED-002)
```

## YP-6: Test Vector Specification

Idle: 60s no-input → P = 0. Active: continuous input → P ≤ refresh rate. Video playback:
P tracks content rate without tearing (visual QA).

## YP-7: Domain Constraints

NVIDIA shared-GL fallback (no DMA-BUF) may force Mailbox → tearing possible on the fallback
path; documented, not gated.

## YP-8: Bibliography

1. Khronos — WGPU/Surface present modes (FIFO, Mailbox, Immediate).
2. Chromium — BeginFrame scheduling design docs.
3. Android — Choreographer vsync alignment.

## YP-9: Knowledge Graph Concepts

frame_pacing, vsync, fifo_swapchain, damage_tracking, present_modes, tearing

## YP-10: Quality Checklist

- [x] Present bound theorems stated (active + idle)
- [x] Present-mode survey with aileron-specific NVIDIA constraint
- [ ] vsync registration implementation (post-spike graphics work)
