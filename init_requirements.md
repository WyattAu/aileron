

### Phase 1: Core Infrastructure & Tooling
*The foundational Rust stack that powers the application.*

*   [ ] **Language & Toolchain:** Rust (Latest Stable).
*   [ ] **Async Runtime:** `tokio` (Required for managing the MCP server, asynchronous network requests, and inter-process communication without blocking the UI thread).
*   [ ] **Error Handling:** `anyhow` for application-level errors, `thiserror` for library/module-level errors.
*   [ ] **Logging & Telemetry:** `tracing` and `tracing-subscriber`. You will need deep visibility into when the UI thread stalls versus when Servo stalls.
*   [ ] **Local Storage:** `rusqlite`. A lightweight SQLite database to store browsing history, bookmarks, and saved session states.

### Phase 2: Windowing & The Graphics Pipeline
*This is the most technically complex part: rendering Servo and the UI on the same GPU canvas.*

*   [ ] **Window Manager:** `winit`. Handles OS window creation, resizing, and raw keyboard/mouse events.
*   [ ] **Graphics Backend:** `wgpu`. The cross-platform GPU API.
*   [ ] **UI Framework:** `egui` (specifically `egui-wgpu` and `egui-winit`). 
*   [ ] **The Compositor Bridge:** 
    *   Configure Servo to render its web frames into an off-screen `wgpu::Texture`.
    *   Register that texture with `egui`'s texture manager.
    *   Render the texture inside an `egui::Image` or custom UI widget. This allows you to draw your Command Palette *over* the web page seamlessly.

### Phase 3: Servo Engine Integration
*Taming the web engine.*

*   [ ] **Engine Initialization:** Implement Servo's `Embedder` traits. Initialize the SpiderMonkey JS engine and the rendering pipeline on background threads.
*   [ ] **Event Translation:** Map `winit` keyboard/mouse events to Servo's internal input event structs and pass them down into the engine.
*   [ ] **Navigation Callbacks:** Listen to messages from Servo on an MPSC channel (e.g., `LoadStarted`, `LoadComplete`, `TitleChanged`, `HistoryChanged`) to update the UI state.
*   [ ] **Script Injection API:** Implement a method to execute arbitrary JavaScript within a specific pane's context (required for the Password Manager).
*   [ ] **System Browser Fallback:** Use the `open` crate. If a page breaks, hitting `Ctrl+E` immediately opens the current URL in the user's default OS browser.

### Phase 4: Aileron Core Logic (Tiling & Modality)
*The Neovim/VSCode mechanics.*

*   [ ] **BSP Window Manager (Tiling):** Implement a Binary Space Partitioning tree data structure. Each node is either a split (Horizontal/Vertical) or a leaf (a Servo webview instance).
*   [ ] **The Modal State Machine:** 
    *   Implement an `enum Mode { Normal, Insert, Command }`.
    *   **Normal Mode:** Keystrokes (`j`, `k`, `H`, `L`) are trapped by Aileron for scrolling and pane navigation.
    *   **Insert Mode:** Keystrokes are passed directly into Servo for typing in web text boxes.
*   [ ] **The Command Palette:** Use the `nucleo` or `skim` crate for blazingly fast fuzzy-finding. When triggered, open an `egui` overlay to search history, bookmarks, and execute commands.

### Phase 5: The "Killer" Features
*The tools that make Aileron a power-user environment.*

*   [ ] **Native Ad-Blocking:** 
    *   Implement a custom network resource loader in Servo.
    *   Use the `adblock` crate (by Brave) to parse standard EasyList/StevenBlack rules natively in Rust. Block tracking/ad domains *before* the HTTP request is even made.
*   [ ] **Password Manager Integration (CLI Wrapper):**
    *   Use `std::process::Command` to asynchronously call `bw` (Bitwarden) or `pass`.
    *   Create a Command Palette UI to fuzzy-search vault items matching the current domain.
    *   Use Servo’s JS injection to inject the retrieved password into the DOM.
*   [ ] **LLM MCP Server (Model Context Protocol):**
    *   Run a `stdio` or `SSE` (Server-Sent Events) server on a background `tokio` thread.
    *   Implement the `mcp-rust-sdk` (or raw JSON-RPC).
    *   **Tool:** `read_active_pane` -> Extracts the DOM from Servo, converts it to clean text/Markdown, and returns it to Claude/Cursor.
    *   **Tool:** `search_web` -> Uses a lightweight search API (or raw DuckDuckGo fetch) to return answers directly to the AI.

### Phase 6: Configuration & Scripting
*Giving power back to the user.*

*   [ ] **Lua Engine:** `mlua`.
*   [ ] **Init Script Loader:** On startup, read `~/.config/aileron/init.lua`.
*   [ ] **Rust/Lua Bindings:** Expose core API functions to the Lua context:
    *   `aileron.keymap.set("n", "<C-w>v", "split_vertical")`
    *   `aileron.theme.set({ bg = "#1e1e2e", fg = "#cdd6f4" })`
    *   `aileron.cmd.create("ClearCache", function() ... end)`

### Phase 7: Build, CI/CD, & Packaging
*Getting it into users' hands.*

*   [ ] **Cross-Platform Compilation:** Setup GitHub Actions to compile binaries for Linux, macOS (Apple Silicon), and Windows.
*   [ ] **Release Packaging:** 
    *   Linux: AppImage or AUR (Arch Linux is a massive demographic for this).
    *   macOS: `.dmg` or Homebrew tap.
    *   Windows: Standalone `.exe` / Scoop package.

---

### Suggested Order of Operations (How to actually build it without going crazy)

**Step 1: The "Hello World" Compositor (Weeks 1-2)**
Don't build tabs, don't build Lua. Just get `winit`, `egui`, and `Servo` running in the same binary. Your goal is to render a single, hardcoded website (e.g., `google.com`) onto a GPU texture and display it inside an `egui` window, and successfully pass a mouse click into it.

**Step 2: Modality & Tiling (Weeks 3-4)**
Add the state machine (Normal vs. Insert mode). Implement the BSP tree so you can hit a keybind and split the screen into two Servo instances side-by-side. 

**Step 3: History & The Command Palette (Weeks 5-6)**
Add `rusqlite`. Track the URLs you visit. Build the fuzzy-finding UI overlay so you can type to navigate, completely removing the need for a traditional URL bar.

**Step 4: The Extensibility Layer (Weeks 7-8)**
Add `mlua`. Move all your hardcoded Rust keybindings into an `init.lua` file. 

**Step 5: The Superpowers (Weeks 9-10)**
Add the AI MCP server and the native CLI password manager hooks. 

**Step 6: Polish & Launch (Weeks 11-12)**
Fix the inevitable rendering bugs, refine the `egui` styling so it looks premium, and write an incredible `README.md`.