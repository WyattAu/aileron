# ADR-001: Servo Embedder API Stability Risk

## Status
Accepted

## Context
Aileron depends on Servo's Embedder API to integrate the web engine. Servo is under active development by the Servo project (sponsored by Igalia and the Linux Foundation). The Embedder API may change between versions, potentially requiring code changes in Aileron.

## Decision
We will pin Servo to a specific git commit hash in Cargo.toml and track the `main` branch. When Servo's Embedder API changes:
1. Update the pinned commit
2. Fix any compilation errors
3. Run full test suite
4. Update CHANGELOG.md

Additionally, we architect the Servo integration behind a trait (`WebEngine`), allowing future replacement with alternative engines (e.g., system WebView via Tauri's `wry` crate) if Servo's API becomes unstable.

## Consequences
- **Positive:** Ability to track latest Servo improvements; trait abstraction provides escape hatch
- **Negative:** May require periodic maintenance when Servo API changes; pinned commit may lack latest fixes
- **Risks:** CPR-001 — Servo Embedder API breaking changes could block development

## Alternatives Considered
1. **Use system WebView (wry) instead of Servo:** Rejected — loses the performance and control advantages of Servo; the entire project thesis depends on Servo
2. **Contribute to Servo Embedder API stabilization:** Long-term goal, but not viable for V1.0 timeline
3. **Fork Servo:** Rejected — maintenance burden too high for solo/small team

## Related Standards
N/A

## Related ADRs
ADR-002 (Multi-Engine Architecture)

## Date
2026-04-11

## Author
Nexus (Principal Systems Architect)
