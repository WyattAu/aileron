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

## Evidence refresh (2026-11-15, fresco repo HEAD fc9e4eb)

Verified against the fresco tree today:

- **Kill-switch lever still absent**: text-kit = atlas, microfont,
  parley_backend, fake — no IME/composition, no native strip text
  editor. widget-kit = tree/listbox/focus/checkbox-class + strip
  renderer + a11y + AccessKit bridge — **no TextField widget**. So
  the palette port (needs TextField) and omnibox port (needs IME)
  are both gated on fresco-side text work. Status bar + find bar
  remain unblocked.
- **ADR-0004 ACCEPTED but Vello not in tree**: paint-kit ships
  raster (CPU), strip, surface — no Vello backend module. Chrome on
  CPU strip remains the launch path, per the spike's original call.
- **fresco-android M3 landed**: damage-tracked rendering at 60 fps
  on device (8fe2112) + M4 interactive widget settings screen. New
  positive evidence: the damage-region present model is proven at
  60 fps on mobile-class hardware — directly relevant to aileron
  chrome damage regions over live webview textures.
- **fresco#8 still OPEN, 1 comment (our own)** — no fresco-side
  triage yet; solid-fx component asks (glass/blur, spinner/progress,
  skeletons) unanswered.

**Sequencing update**: stage 3 step 2 is COMPLETE (10 wedges + bin
cutover, gate branch 215e166) — the crate boundary seam exists.
Sequencing step 2 (status bar + find bar spike inside
aileron-headed) is now unblocked from the aileron side; it produces
the first consumer evidence fresco#8 asks for. Palette/omnibox stay
gated on fresco text-kit/widget-kit work landing.

## Mobile implications (2026-11-15 discussion)

Two-layer split:

- **UI layer: Android proven.** fresco-android (winit
  android-activity backend + window-kit softbuffer software present)
  runs damage-tracked at 60 fps on device (M3, 8fe2112) with an
  interactive widget screen (M4). The chrome widgets aileron builds
  for desktop would run on this shell unchanged - scene/paint/widget
  stacks are platform-agnostic by construction (ADR-0003 facade,
  headless fake runner, CPU reference backend). Present path on
  Android is CPU (softbuffer) - consistent with the chrome-on-CPU-
  strip launch decision. iOS: winit nominally compiles there but no
  fresco shell exists; unproven, not claimed.
- **Content layer: the real gap.** WebKitGTK does not exist on
  Android. Options: (a) Android System WebView via wry's JNI backend
  - renders fine but is a native view, NOT compositable as a wgpu
  texture, so chrome-over-content scene compositing breaks; also no
  offscreen BGRA capture path for the agent walker. (b) Servo on
  Android - renders to wgpu textures, composites as fresco
  external-texture nodes, keeps the walker's pixel-readback model.
  (b) is Architecture D - which makes Architecture D the mobile
  unlock, not just a desktop union.

Implication: desktop chrome spike (status bar -> palette -> omnibox)
is unaffected. An Android aileron is "Architecture D on Android":
Servo content + fresco chrome, both Android-native. iOS additionally
needs a content engine (WKWebView via ObjC or Servo-on-iOS, both
unsolved in-estate) - not a 2027 item.

## Spike EXECUTED (2026-11-15): status bar + find bar equivalent

Status: PROPOSED -> IN-PROGRESS (first wedge live in aileron-headed).

`headed/src/chrome/mod.rs` (feature `fresco-chrome`, non-default):
fresco widget-kit tree -> paint-kit CPU strip -> RGBA Surface,
alpha-composited over the webview frame's top 28 rows before
gfx presents (CPU blend, no GPU pipeline change). Wired into the
live present path in app_handler (status from core.ui.status_message).

### Frame-pacing evidence (fresco#8 consumer datum, RELEASE builds)

| Phase | Release | Debug (confound — do not ship numbers from debug) |
|---|---|---|
| font/atlas warm-up (one-time) | 1.6 ms | 22–88 ms |
| tree layout (2 labels) | 3 µs | 22 µs |
| surface fill 1280×28 | 7 µs | 12 ms |
| paint_tree | **918 µs** | 37 ms |
| alpha composite over frame | **61 µs** | 1.4–2.6 ms |
| **total damage cost** | **~1 ms** (5% of 16.6 ms budget) | ~50 ms |

Verdict: damage-region present model is a PASS for chrome bars.
paint_tree at ~1 ms for a 1280×28 strip is the fresco-side
optimization surface (dirty-rect label raster), but already fits the
budget with 15x headroom at damage rates. CAUTION for future
consumer evidence: cargo test defaults to debug — always measure in
--release.

### Kill-switch correction (2026-11-15, supersedes earlier note)

widget-kit DOES have a TextField widget (caret, key_nav:
Backspace/Delete/Home/End, field_text/field_caret) — my earlier
"no TextField" claim was wrong (grep missed the WidgetKind::TextField
variant + field_* methods). **Palette is portable NOW.** What is
still absent: IME/preedit composition (grep: no ime/composition/
preedit in widget-kit or text-kit). Kill-switch therefore narrows to
the omnibox only, gated on fresco IME landing.

### Servo fork option (user decision 2026-11-15)

For Architecture D: if upstream Servo stalls on the Android/texture
path, fork Servo into the estate (the estate already pins
webkit2gtk =2.0.2 — fork-and-pin is established posture). Recorded
as the risk mitigation for Architecture D; revisit when D starts.

## Palette port EXECUTED (2026-11-15): renderer wedge live

`chrome::PaletteOverlay`: query TextField + results Select(open),
painted from `CommandPalette` state and composited over the webview
frame when `headed().palette.open`. Wired into the present path in
app_handler. Logic stays in the tested CommandPalette (fresco is the
renderer; input unification waits on IME).

### Pacing finding (fresco#8): Select open-render is the hot path

| Surface | Release paint | Budget verdict |
|---|---|---|
| status bar (2 Labels, 1280×28) | 918 µs | PASS (5% budget) |
| palette (TextField + Select 10 open rows, 1280×268) | **~61 ms** | **FAIL (4× over)** |

Per-row cost ~6 ms — the Select open-render path re-rasterizes rows
without reuse. Composite stays trivial (61 µs). Until fresco
optimizes, the palette ships at damage-rate acceptable (61 ms on
open/keystroke, not per frame) but is not 60 Hz-animation ready.
Regression guard in the test suite: <100 ms release; tighten to
<16.6 ms when fresco lands the fix.

### Implementation notes

- TextField construction needs `ui-text-edit` crate
  (`TextEditField::new(StringBuffer::new())`) — not re-exported from
  widget-kit; consumers add it as a dep.
- Query text set via `tree.key_char` per char after `set_focus`
  (no `set_field_text` yet — a fresco-side convenience worth adding).
- Surface recreated on height change (row count); fine at damage rate.
