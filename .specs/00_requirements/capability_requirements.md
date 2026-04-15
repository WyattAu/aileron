# Capability Requirements

**Phase:** -1 (Context Discovery)
**Project:** Aileron
**Date:** 2026-04-11

---

## Build Environment Requirements

| Capability | Minimum Version | Purpose | Verified | Actual Version |
|-----------|----------------|---------|----------|----------------|
| Rust (stable) | 1.75+ | Core language | ✅ | 1.94.0 |
| Cargo | 1.75+ | Build system | ✅ | 1.94.0 |
| cmake | 3.20+ | Servo build dependency | ✅ | 4.1.2 |
| python3 | 3.10+ | Servo build scripts | ✅ | 3.13.12 |
| pkg-config | 0.29+ | Native dependency discovery | ✅ | 0.29.2 |
| Vulkan ICD | 1.3+ | wgpu backend (primary) | ✅ | 1.4.341 |
| Wayland (libwayland-client) | 1.20+ | winit Wayland backend | ✅ | 1.24.0 |

## Verification & Quality Tools

| Tool | Required | Available | Version | Notes |
|------|----------|-----------|---------|-------|
| Clippy | Yes | ✅ | 0.1.94 | Lint enforcement; run in CI |
| Rustfmt | Yes | ✅ | 1.8.0 | Code formatting; enforced via `rustfmt.toml` |
| Lean 4 | Conditional | ✅ | 4.29.0 | Formal verification of BSP tree invariants and modal state machine; used in Phase 2.5 (Concurrency) |
| Valgrind | Recommended | ❌ | — | Memory leak analysis for Phase 3.5 (Resource Management); add to `flake.nix` |
| cargo-audit | Recommended | TBD | — | Dependency vulnerability scanning; add to CI pipeline |
| cargo-deny | Recommended | TBD | — | License and dependency auditing; add to CI pipeline |

## Platform-Specific Requirements

### Linux (Primary — Wayland)

| Capability | Minimum Version | Purpose | Status |
|-----------|----------------|---------|--------|
| libwayland-client | 1.20+ | Wayland protocol | ✅ 1.24.0 |
| libxkbcommon | 1.0+ | Keyboard handling | TBD |
| libegl | 1.5+ | EGL display | TBD |
| libvulkan | 1.3+ | Vulkan loader | ✅ 1.4.341 |

### Linux (Secondary — X11)

| Capability | Minimum Version | Purpose | Status |
|-----------|----------------|---------|--------|
| libx11-dev | 1.7+ | X11 protocol | ❌ Not verified |
| libxcb | 1.14+ | X11 C binding | ❌ Not verified |
| libxrandr | 1.5+ | Display configuration | ❌ Not verified |

### macOS (Apple Silicon)

| Capability | Minimum Version | Purpose | Status |
|-----------|----------------|---------|--------|
| Xcode CLT | 14.0+ | C/C++ compilation | Deferred to V1.1+ |
| Metal SDK | — | wgpu Metal backend | Available via OS |

### Windows

| Capability | Minimum Version | Purpose | Status |
|-----------|----------------|---------|--------|
| Visual Studio Build Tools | 2022 | C/C++ compilation | Deferred to V1.1+ |
| DirectX 12 | — | wgpu DX12 backend | Available via OS |

## Runtime Dependencies

| Dependency | Version | Purpose | Notes |
|-----------|---------|---------|-------|
| Servo | Latest (pinned) | Web engine | Built from source; pinned via `Cargo.toml` |
| SQLite | 3.38+ | Local storage | Bundled via `libsqlite3-sys` |
| OpenSSL / rustls | Latest | TLS | rustls preferred for no-system-dep builds |

## Missing Capabilities

| # | Capability | Priority | Phase Needed | Action |
|---|-----------|----------|-------------|--------|
| 1 | Valgrind | Medium | 3.5 (Resource Management) | Add to `flake.nix` dev shell |
| 2 | X11 development libraries (libx11-dev, libxcb, libxrandr) | Medium | 4.5 (Cross-Platform) | Add to `flake.nix` and verify X11 backend |
| 3 | cargo-audit | Medium | 1.5 (Supply Chain) | Add to `flake.nix` and CI |
| 4 | cargo-deny | Low | 1.5 (Supply Chain) | Add to `flake.nix` and CI |
| 5 | libxkbcommon | High | 0 (Prototyping) | Verify availability; add to `flake.nix` if missing |

## Reproducibility

| Aspect | Status | Details |
|--------|--------|---------|
| Nix flake | ✅ Present | `flake.nix` and `flake.lock` at repository root |
| Lock file | ✅ Present | `Cargo.lock` committed |
| Primary target | ✅ | `x86_64-linux` |
| Secondary targets | Deferred | `aarch64-darwin` (macOS), `x86_64-windows` |
| CI/CD | TBD | Pipeline configuration pending Phase 7 |

## Development Environment Notes

- The Nix flake (`flake.nix`) should provide a hermetic development shell with all build dependencies.
- Servo is built from source and pinned to a specific commit in `Cargo.toml`.
- The project targets Rust stable channel (1.75+); nightly is not required.
- wgpu requires a Vulkan-capable GPU on Linux; Metal on macOS; DX12 on Windows.
