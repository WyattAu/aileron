
### Layer 1: The Rendering & Graphics Pipeline (The Core)
*This is the heaviest layer. It is responsible for parsing the web and painting pixels.*

*   **`servo` (The Web Engine):** The heart of the project. It handles HTML parsing, the DOM, and layout. 
    *   *Note:* Servo uses Mozilla's **SpiderMonkey** for JavaScript execution and **Stylo** for CSS styling. You will interface with Servo via its `Embedder` API.
*   **`wgpu` (The Graphics Backend):** A cross-platform, safe, pure-Rust graphics API based on the WebGPU standard. It translates your draw commands into Vulkan (Linux), Metal (macOS), or DirectX 12 (Windows). 
    *   *Why:* Both Servo and our chosen UI framework can use `wgpu`. This allows us to let Servo render a webpage directly into a `wgpu::Texture` in GPU memory, which Aileron can then manipulate instantly.
*   **`winit` (The OS Interop):** The standard Rust window creation and event loop library. 
    *   *Why:* It catches all raw keyboard strokes, mouse movements, and OS window resizing events. Aileron will intercept these events, check if we are in "Normal Mode" or "Insert Mode", and either consume them or pass them down to Servo.

### Layer 2: The User Interface & Window Manager
*This layer defines the "chrome" of Aileron: the tiling manager, command palette, and status bar.*

*   **`egui` + `egui-wgpu` + `egui-winit` (The GUI Framework):** A highly performant, immediate-mode GUI for Rust.
    *   *Why:* Traditional retained-mode GUIs (like Qt or GTK) are bloated. `egui` compiles in seconds, renders in under 1 millisecond, and makes drawing floating overlays (like your Command Palette) over a 3D texture trivial. 
*   **`egui_tiles` (Tiling Engine):** A crate specifically designed to build split-pane layouts inside `egui`. 
    *   *Why:* Instead of writing complex Binary Space Partitioning math from scratch, this handles dragging, dropping, and resizing tiled Servo webviews out-of-the-box.
*   **`nucleo` (Fuzzy Finder):** A blazingly fast fuzzy-matcher (often used in modern Rust terminal tools like `helix` or `zellij`).
    *   *Why:* When the user hits `Ctrl+P` to search their history or run commands, `nucleo` will filter 100,000 SQLite records in less than a millisecond, giving that instant Neovim/VSCode feel.

### Layer 3: The Nervous System (Concurrency & State)
*Browsers are heavily multi-threaded. The UI cannot freeze while a page is loading or a database is querying.*

*   **`tokio` (The Async Runtime):** The industry standard async runtime for Rust. 
    *   *Why:* Aileron’s main thread will be locked to the 60fps/144fps `winit` event loop. `tokio` will handle background tasks like the AI MCP server, querying the password manager CLI, and reading local files.
*   **`crossbeam-channel` or `tokio::sync::mpsc` (Message Passing):**
    *   *Why:* Servo runs on its own background threads. When Servo finishes loading a page, it needs to tell Aileron to update the tab title. MPSC (Multi-Producer, Single-Consumer) channels allow safe communication between Servo, the background Tokio tasks, and the main UI thread.
*   **`rusqlite` (Database):** Ergonomic bindings to SQLite.
    *   *Why:* Local history, bookmarks, and site-specific configurations will be stored in a local SQLite database. 
*   **`directories` (Filesystem compliance):** 
    *   *Why:* Neovim users will hate you if you put config files in the wrong place. This crate ensures Aileron strictly follows the XDG Base Directory spec (`~/.config/aileron/init.lua`, `~/.local/share/aileron/history.db`).

### Layer 4: Extensibility & "Killer Features"
*The crates that give Aileron its power-user capabilities.*

*   **`mlua` (The Lua Engine):** Safe Rust bindings to Lua 5.4 or LuaJIT. 
    *   *Why:* This is what reads the user's `init.lua` file. It allows users to remap keys, change theme colors, and write custom browser automation scripts without recompiling Aileron.
*   **`mcp-core` / `mcp-rust-sdk` (The AI Protocol):** The Model Context Protocol implementation.
    *   *Why:* To expose your browser's state locally via standard input/output (`stdio`) or Server-Sent Events (`SSE`), allowing AI assistants like Claude Desktop or Cursor to "see" your browser.
*   **`adblock` (By Brave):** Brave Browser’s official Rust library for network filtering.
    *   *Why:* You feed it standard EasyList/uBlock rulesets on startup. You hook it into Servo's network fetcher, achieving native, zero-overhead ad blocking without needing WebExtensions.
*   **`pulldown-cmark` (Markdown parsing):** 
    *   *Why:* When the user navigates to `local://readme.md`, this crate instantly compiles the markdown to HTML before handing it to Servo. 

---

### The Architecture: How Data Flows

To visualize how this stack actually runs, here is the lifecycle of a single keystroke in Aileron:

1. **User presses `j`** on their keyboard.
2. **`winit`** catches the OS-level key press event on the Main UI Thread.
3. **Aileron Core** checks its State Machine:
   * *Are we in Insert Mode?* Route the event via channel to **`servo`** to type 'j' into an HTML text box.
   * *Are we in Normal Mode?* Route the event to the Lua runtime (**`mlua`**). 
4. **`mlua`** recognizes `j` is bound to the `scroll_down` command. 
5. Aileron sends a "Scroll" command to **`servo`**.
6. **`servo`** recalculates the DOM/Layout on its background threads and draws the new frame to a **`wgpu::Texture`**.
7. The Main Thread ticks. **`egui`** draws the UI (status bars, borders) and paints the new Servo texture onto the screen.

