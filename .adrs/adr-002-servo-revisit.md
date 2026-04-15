# ADR-002: Servo Revisit Timeline and Hybrid Engine Strategy

## Status
Accepted

## Date
2026-04-14

## Context

ADR-001 documented that Servo was evaluated as a rendering backend but rejected due to:
1. Broken transitive dependencies
2. No `build_as_child` support for embedding in tiling panes
3. `!Send + !Sync` constraints conflicting with our architecture

Since ADR-001, Servo has made significant progress:
- Improved embedding API (`servo::Embedder`)
- Better WebRender integration
- Active development on compositor support

The user requested evaluation of a hybrid engine strategy:
- "Can we switch to Servo for first rendering and fallback to Chromium if needed?"
- "Or branch based on content type to decide which engine is better?"

## Decision

### Short-term (Path D-lite): Pane modes instead of engine switching

Instead of maintaining two rendering engines, we implement content-aware **pane modes** that modify how wry renders content:

1. **Reader Mode** (`Ctrl+Shift+R`, `:reader`): JS-based article extraction that strips CSS and displays clean text. Toggle on/off per pane.

2. **Minimal Mode** (`Ctrl+Shift+M`, `:minimal`): CSS injection to hide images/media and remove script tags. Toggle on/off per pane.

These provide the "fast, simple rendering" user experience benefit without the complexity of a second engine.

### Medium-term: Clean engine abstraction

We refactored the engine layer:
- Renamed `PlaceholderEngine` → `PaneState` (honest naming: URL/title metadata tracker)
- Renamed `WebEngineManager` → `PaneStateManager`
- Added `PaneRenderer` trait defining the contract for any rendering backend
- `WryPane` now implements `PaneRenderer`

This makes it trivial to swap in Servo (or any other engine) when embedding matures.

### Long-term: Servo revisit criteria

Re-evaluate Servo as a rendering backend when ALL of these are met:

| Criterion | Current Status | Required |
|-----------|---------------|----------|
| Embedding as child widget | Not supported | Must support `build_as_child` equivalent on X11 and Wayland |
| Render-to-texture | Not supported | Must render to a wgpu texture or similar |
| CSS Grid/Layout support | Partial | Must pass >95% of CSS Flexbox/Grid tests |
| JS engine completeness | Partial | Must support modern ES2024+ features |
| Stable API | Experimental | Must have a stable, versioned embedding API |
| Build reliability | Broken deps | Must compile cleanly with no transitive dep conflicts |

**Revisit date: October 2026** (6 months from this ADR)

## Consequences

### Positive
- Reader and Minimal modes provide immediate user value
- Clean `PaneRenderer` trait enables future engine swap with minimal code changes
- No maintenance burden of a second rendering engine
- Per-pane mode state is tracked in `AppState` (Send+Sync), cleanly separated from `WryPaneManager` (!Send+!Sync)

### Negative
- Reader mode JS extraction is heuristic-based, won't work perfectly on all sites
- Minimal mode can't truly disable JS (already-executed scripts remain)
- No actual performance benefit from a lighter engine (Servo)

### Alternatives Considered

1. **Servo-first with Chromium fallback**: Rejected — content-level fallback is fragile (hard to detect rendering failures)
2. **Content-type routing**: Rejected — "simple vs complex" boundary is blurry (GitHub PR pages use React)
3. **User-selected engine per tab**: Rejected — Servo can't be embedded as a child widget yet

## Related ADRs
- ADR-001: Servo evaluation (rejected)
