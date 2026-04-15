# Capability Matrix

## Available vs. Required Capabilities

| Tool | Required | Available | Version | Notes |
|------|----------|-----------|---------|-------|
| Rust (stable) | Yes | ✅ | 1.94.0 (4a4ef493e) | Built from source tarball |
| Cargo | Yes | ✅ | 1.94.0 (85eff7c80) | — |
| Lean 4 | Conditional | ✅ | 4.29.0 | Formal verification available |
| Valgrind | Conditional | ❌ | — | Not installed; update reproducibility.nix |
| Clippy | Yes | ✅ | 0.1.94 | — |
| Rustfmt | Yes | ✅ | 1.8.0 | — |
| cmake | Yes | ✅ | 4.1.2 | Servo build dependency |
| pkg-config | Yes | ✅ | 0.29.2 | — |
| python3 | Yes | ✅ | 3.13.12 | Servo build dependency |
| Vulkan loader | Yes | ✅ | Available | Minor ICD warnings, functional |
| Wayland libs | Yes | ✅ | 1.24.0 | winit backend |
| X11 libs | Conditional | ❌ | — | Not in PKG_CONFIG_PATH; Wayland-primary |

## Status Legend
- ✅ Available and meets requirements
- ⚠️ Available but version may not meet requirements
- ❌ Not available
- ❓ Not yet checked

## Missing Capabilities Requiring Action
1. **Valgrind:** Not installed. Add to flake.nix for memory leak analysis in Phase 3.5.
2. **X11 libs:** Not configured. Wayland is primary; X11 support requires additional PKG_CONFIG_PATH configuration.
