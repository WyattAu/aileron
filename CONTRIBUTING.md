# Contributing to Aileron

Thanks for your interest in contributing! This is a keyboard-driven, tiling web environment built in Rust.

## Development Setup

### Prerequisites

- **Nix** (with flakes enabled) — [install guide](https://nixos.org/download)
- **Linux** (x86_64) — tested on CachyOS (Wayland + NVIDIA)
- **Vulkan-capable GPU** — required by wgpu for egui rendering

### Build & Run

```bash
# Enter the Nix dev shell (all dependencies included)
nix develop

# Build
cargo build

# Run
LD_LIBRARY_PATH="/usr/lib:$LD_LIBRARY_PATH" ./target/debug/aileron
```

### Test

```bash
# Unit tests
cargo test --lib -- --test-threads=4

# Integration tests
cargo test --test integration_smoke

# Clippy (must be zero warnings)
cargo clippy --lib -- -D warnings
```

### Code Style

- **Clippy must pass with zero warnings** — CI enforces `cargo clippy --lib -- -D warnings`
- Follow existing code patterns (pure dispatch, WryAction queue, etc.)
- All new public functions need tests
- Use `tracing` for logging (not `println!`)

## Architecture Overview

Aileron uses a **pure dispatch pattern**:

```
Key event → AppState.process_key_event()
         → KeybindingRegistry.lookup() → Action
         → dispatch_action(Action) → Vec<ActionEffect>
         → execute_action() applies effects to AppState
         → WryAction queue consumed by main.rs
```

Key modules:
- `src/app/mod.rs` — AppState (mode machine, palette, keybindings)
- `src/app/dispatch.rs` — Pure action dispatch (Action → ActionEffect)
- `src/input/` — Key mapping, mode transitions, keybinding registry
- `src/wm/` — BSP tree (tiling), rectangle math
- `src/servo/` — WryPaneManager (wry webview), PaneRenderer trait, PaneState
- `src/lua/` — Lua sandbox, API bindings
- `src/db/` — SQLite: history, bookmarks, workspaces
- `src/scripts/` — Content script system (Lua → JS injection)
- `src/mcp/` — MCP JSON-RPC server for LLM integration
- `src/net/` — Ad blocker
- `src/main.rs` — Event loop, wry pane management, egui UI

## Pull Request Process

1. Fork the repo
2. Create a feature branch: `git checkout -b feature/my-feature`
3. Make your changes with tests
4. Verify: `cargo clippy --lib -- -D warnings && cargo test --lib`
5. Open a PR with a clear description

## Reporting Issues

Please include:
- **OS/distro** and desktop environment (Wayland/X11)
- **GPU** and driver version
- **Steps to reproduce**
- **Expected vs actual behavior**
- **Logs** (run with `RUST_LOG=info` for more detail)

## Feature Requests

Feature requests are welcome! Please describe:
- The use case (what problem does this solve?)
- Proposed keybinding or command syntax
- Any relevant examples from other tools (qutebrowser, Vimium, etc.)
