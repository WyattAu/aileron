# Contributing to Aileron

Thanks for your interest in contributing! This is a keyboard-driven, tiling web environment built in Rust.

## Development Setup

### Git Hooks

Install the project git hooks to run automated checks on commit and push:

```bash
git config core.hooksPath .githooks
```

- **Pre-commit** runs fast checks: formatting (`cargo fmt`), compilation (`cargo check`), clippy (`-D warnings`), lib tests, and doc tests
- **Pre-push** runs the full suite: all 13 integration test suites and documentation generation
- Hooks use `AILERON_TESTING=1` and auto-detect `xvfb-run` for headless environments

### Prerequisites

- **Nix** (with flakes enabled) -- [install guide](https://nixos.org/download)
- **Linux** (x86_64) -- tested on CachyOS (Wayland + NVIDIA)
- **Vulkan-capable GPU** -- required by wgpu for egui rendering

### Build & Run (Linux)

```bash
# Enter the Nix dev shell (all dependencies included)
nix develop

# Build
cargo build

# Run
LD_LIBRARY_PATH="/usr/lib:$LD_LIBRARY_PATH" ./target/debug/aileron
```

### Build & Run (macOS)

Aileron compiles on macOS using WKWebView (via wry). No Nix shell is required.

#### Prerequisites

- **Xcode Command Line Tools** — required for the system linker and headers:
  ```bash
  xcode-select --install
  ```
- **Rust** (stable) — install via [rustup](https://rustup.rs):
  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```
- **WebKit** — built into macOS, no additional install needed

#### Build & Install

```bash
# Build (debug)
cargo build

# Build (release, optimized)
cargo build --release

# Install to ~/.cargo/bin
cargo install --path .

# Run
./target/debug/aileron
# or, if installed:
aileron
```

#### macOS-Specific Notes

- Offscreen rendering is the default (`render_mode = "offscreen"`)
- Tab sidebar defaults to right side (`tab_sidebar_right = true`)
- Cmd is the primary modifier (instead of Ctrl on Linux/Windows)
- Standard macOS shortcuts work: Cmd+T (new tab), Cmd+W (close tab), Cmd+Q (quit)
- CI verifies compilation on both x86_64 and aarch64

#### Troubleshooting

- **"xcode-select: note: no developer tools are found"** — Run `xcode-select --install`
- **Linker errors about missing frameworks** — Ensure Xcode CLT is installed, not just Xcode.app
- **WKWebView not rendering** — Check that `WEBKIT_DISABLE_COMPOSITING_MODE` is not set

### Build & Run (Windows)

Aileron compiles on Windows using WebView2 (via wry).

#### Prerequisites

- **WebView2 Runtime** — pre-installed on Windows 11; for Windows 10, download from [Microsoft](https://developer.microsoft.com/en-us/microsoft-edge/webview2/)
- **Rust** (stable) — install via [rustup](https://rustup.rs)
- **Visual Studio Build Tools** — required for the MSVC linker

#### Build & Install

```powershell
# Build (debug)
cargo build

# Build (release, optimized)
cargo build --release

# Install to ~/.cargo/bin
cargo install --path .

# Run
.\target\debug\aileron.exe
# or, if installed:
aileron
```

#### Windows-Specific Notes

- Native rendering is the default (`render_mode = "native"`)
- Some features (adblock cosmetic filtering, offscreen mode) may behave differently
- Standard Windows shortcuts work: Ctrl+T (new tab), Ctrl+W (close tab), Alt+F4 (quit)
- CI verifies compilation on x86_64-pc-windows-msvc

#### Troubleshooting

- **"WebView2 is not installed"** — Download and install the WebView2 Runtime from Microsoft
- **Linker errors** — Ensure Visual Studio Build Tools with the "Desktop development with C++" workload are installed
- **Antivirus blocking** — Some antivirus software may block the WebView2 loader DLL

### Test

```bash
# Unit tests
cargo test --lib -- --test-threads=4

# Integration tests
cargo test --test integration_smoke

# Clippy (must be zero warnings, all targets)
cargo clippy --all-targets -- -D warnings
```
### Code Style

- **Clippy must pass with zero warnings** — CI enforces `cargo clippy --all-targets -- -D warnings`
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
- `src/main.rs` -- Event loop, wry pane management, egui UI
- `src/bootstrap.rs` -- Application bootstrap, panic hook, environment diagnostics

## Pull Request Process

1. Fork the repo
2. Create a feature branch: `git checkout -b feature/my-feature`
3. Make your changes with tests
4. Verify: `cargo clippy --all-targets -- -D warnings && cargo test --all-targets`
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
