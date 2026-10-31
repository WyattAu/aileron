# Servo Evaluation Note (Era-3, engine hedge)

**Status:** evaluation framework — no migration decision. Revisit trigger:
WebKitGTK instability that the watchdog cannot contain, or upstream
abandonment signals.

## Why a hedge exists at all

Every UI-process SIGSEGV in the 2026-10 campaign traced to libwebkit2gtk
under NVIDIA shared-GL. The watchdog contains the failure mode; it cannot
reduce it. WebKitGTK remains the right engine today (mature, GTK-native,
offscreen-proven in this codebase), but the roadmap carries servo as the
exit hedge.

## Evaluation dimensions (score both engines before any decision)

| Dimension | What to measure | Method |
|---|---|---|
| Stability | UI-process crash rate under the campaign repro suite | 72h soak, both engines, identical scripts |
| Offscreen rendering | snapshot capture fidelity + rate | the frame QA harness |
| MCP compatibility | all 45 tools behave identically | the QA matrix, both engines |
| ACI walker | evaluate_script + callback parity | corpus runs |
| Session isolation | epoch/settle semantics survive | per-engine epoch tests |
| Wayland+NVIDIA | shared-GL path stability | the known-issue repro |
| Packaging | distro availability, pinning story | distro surveys |

## Migration surface (if ever triggered)

The engine boundary is already an abstraction: `servo/` (wry engine) vs
`offscreen_webview.rs` (direct WebKitGTK). The wry layer hides most
WebKit specifics; the direct downcasts (`webview_is_loading`, cookie
future work, PDF) are the enumerated porting points. Session/relay/ACI
layers are engine-agnostic by design — the split decision record
(`agent-native-split.md`) keeps the agent profile portable.
