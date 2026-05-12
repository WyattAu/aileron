# Capability Matrix

## Available vs. Required Capabilities

| Tool | Required | Available | Version | Notes |
|------|----------|-----------|---------|-------|
| Rust (stable) | Yes | OK | 1.94.0 (4a4ef493e) | Built from source tarball |
| Cargo | Yes | OK | 1.94.0 (85eff7c80) | — |
| Lean 4 | Conditional | OK | 4.29.0 | Formal verification available |
| Valgrind | Conditional | MISSING | — | Not installed; update reproducibility.nix |
| Clippy | Yes | OK | 0.1.94 | — |
| Rustfmt | Yes | OK | 1.8.0 | — |
| cmake | Yes | OK | 4.1.2 | Servo build dependency |
| pkg-config | Yes | OK | 0.29.2 | — |
| python3 | Yes | OK | 3.13.12 | Servo build dependency |
| Vulkan loader | Yes | OK | Available | Minor ICD warnings, functional |
| Wayland libs | Yes | OK | 1.24.0 | winit backend |
| X11 libs | Conditional | MISSING | — | Not in PKG_CONFIG_PATH; Wayland-primary |

## Status Legend
- OK: Available and meets requirements
- WARN: Available but version may not meet requirements
- MISSING: Not available
- UNKNOWN: Not yet checked

## Missing Capabilities Requiring Action
1. **Valgrind:** Not installed. Add to flake.nix for memory leak analysis in Phase 3.5.
2. **X11 libs:** Not configured. Wayland is primary; X11 support requires additional PKG_CONFIG_PATH configuration.
