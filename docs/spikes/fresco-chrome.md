# Spike: fresco as aileron's chrome layer

**Status:** PROPOSED (discussion outcome 2026-11-12; execution gated on
stage 3 step 2 + fresco consumer acceptance, see WyattAu/fresco#8)
**Verdict:** adopt fresco as the target chrome layer of aileron-headed,
sequenced status-bar → palette → omnibox, with the omnibox native-text
stack as the explicit kill-switch. Not a compositor replacement; web
content pixels stay WebKitGTK's.

## Current headed stack (what fresco replaces)

| Today | Replaced by |
|---|---|
| Leptos WASM chrome over `chrome_bridge` CDP-style IPC | widget-kit in-process widgets emitting the same intents (the bridge's ChromeRequest enum is the chrome's full behavioral surface) |
| `gfx/renderer.rs` (273 lines, wgpu BGRA blit + present) | webview-texture nodes in a fresco scene graph + window-kit present; webview blits stay GPU wgpu-side |
| egui remnants in `input/router.rs`, popup, terminal grid | input-kit + widget-kit focus chain |
| `accessibility_text: String` (one flat string) | a11y-kit → AccessKit bridge |
| hand-rolled `set_theme` | theme-kit CVD-verified variants |

## The omnibox risk (kill-switch)

Browser-grade url bar = IME composition + autocomplete reflow at 60 Hz.
Fresco's next lever (native strip text, vello glyph runs + system fonts)
is NOT landed; labels are micro-font + Parley runs. Port order:
status bar + find bar (no IME) → palette → omnibox last. If native text
+ IME quality isn't there at omnibox time, Leptos chrome stays and
fresco takes only non-text chrome. Real exit ramp.

## Not in scope

- Web content rendering (WebKitGTK owns its pixels).
- GPU Vello backend as launch requirement (ADR-0004 unlanded; chrome-only
  damage on strip is plausibly sufficient; webview blits stay wgpu).
- Servo/Architecture D: later union — Servo renders to textures, fresco
  scene-kit composites external-texture nodes; texture_share is the stub
  seam already.

## Sequencing

1. Stage 3 step 2 (headed crate extraction, chrome untouched) — the
   crate boundary IS the fresco seam. GATE-1 closes unblocked.
2. Spike inside aileron-headed: status bar + find bar in fresco over
   live webview textures (single damage region, no IME). First
   fresco-in-browser damage-region present; report frame pacing to
   fresco#8 as consumer evidence.
3. Palette port (TextField + listbox over core fuzzy search).
4. Omnibox port = go/no-go on Leptos retirement.
5. Retire Leptos/WASM + chrome_bridge IPC; chrome widgets emit the
   ChromeRequest intents in-process (enum already defines the surface).
6. Architecture D union: Servo → wgpu textures → fresco external-
   texture nodes.

## Dogfood components riding along

theme-kit (CVD variants), a11y-kit + AccessKit (chrome a11y upgrade;
walker gains chrome coverage), input-kit + ui-text-edit (omnibox),
color-kit tokens, render-conformance (chrome golden-image oracle).

## Consumer-gate alignment (fresco ADR-0005)

Filed as WyattAu/fresco#8: aileron stresses fresco harder than kestrel
on browser-scale IME, sustained small-region damage, and external-
texture compositing under a UI engine. Fresco's own sizing ("a real
estate app runs on fresco": 8–13k own lines + ~250k consumed) — a
browser chrome is the flagship dogfood for that claim.

## Prerequisite on the aileron side

Stage 3 step 2 (headed crate extraction, current chrome untouched, this
branch). The aileron-agent thin bin (2d-3) is unaffected: core+engine
only, no UI.
