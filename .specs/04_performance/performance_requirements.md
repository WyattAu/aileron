# Performance Requirements

## Frame Timing

| Metric | Target | Measurement | Source |
|--------|--------|-------------|--------|
| Compositor frame rate (1 pane) | >= 60 fps | Frame counter | REQ-GFX-005 |
| Compositor frame rate (4 panes) | >= 30 fps | Frame counter | REQ-GFX-005 |
| Compositor frame rate (16 panes) | >= 15 fps | Frame counter | YP-GFX-COMPOSITE-001 |
| Frame time jitter (1σ) | < 2 ms | Statistical analysis | UX requirement |
| VSync compliance | 0 missed frames in 60s | Frame counter | Display quality |

## Startup Time

| Metric | Target | Measurement | Source |
|--------|--------|-------------|--------|
| Cold start to first paint | < 2 seconds | Stopwatch | AC-GFX-001 |
| Filter list loading (EasyList) | < 2 seconds | Stopwatch | YP-NET-ADBLOCK-001 |
| init.lua loading and execution | < 100 ms | Stopwatch | AC-LUA-001 |
| MCP server startup | < 100 ms | Stopwatch | YP-MCP-PROTOCOL-001 |

## Input Latency

| Metric | Target | Measurement | Source |
|--------|--------|-------------|--------|
| Event routing latency | < 1 ms | Instrumentation | YP-INPUT-MODES-001 |
| Mode transition latency | < 0.1 ms | Instrumentation | TC-INPUT-002 |
| Keybinding lookup | < 0.1 ms | Benchmark | YP-INPUT-MODES-001 LEM-MODE-002 |
| Servo event delivery | < 5 ms | Channel latency | Cross-thread IPC |

## Search & Filtering

| Metric | Target | Measurement | Source |
|--------|--------|-------------|--------|
| Fuzzy search 10K items | < 5 ms | Benchmark | YP-FUZZY-MATCH-001 |
| Fuzzy search 100K items | < 50 ms | Benchmark | YP-FUZZY-MATCH-001 THM-FZ-003 |
| Command palette open latency | < 100 ms | Instrumentation | AC-CP-001 |

## Network & Ad-Blocking

| Metric | Target | Measurement | Source |
|--------|--------|-------------|--------|
| Per-request ad-block check | < 1 ms | Benchmark | YP-NET-ADBLOCK-001 |
| Ad-block memory usage | < 150 MB | Process monitor | YP-NET-ADBLOCK-001 THM-AD-002 |

## MCP Server

| Metric | Target | Measurement | Source |
|--------|--------|-------------|--------|
| Tool execution latency | < 500 ms | Timeout | YP-MCP-PROTOCOL-001 THM-MCP-002 |
| DOM-to-Markdown conversion (1 page) | < 200 ms | Benchmark | ALG-MCP-002 |
| MCP server response (read_active_pane) | < 300 ms | Benchmark | AC-MCP-004 |

## Memory

| Metric | Target | Measurement | Source |
|--------|--------|-------------|--------|
| Base memory (1 pane, empty page) | < 100 MB | Process monitor | REQ-DB |
| Per-pane overhead | < 50 MB | Process monitor | Servo + texture |
| Maximum memory (16 panes) | < 1 GB | Process monitor | Resource limit |

## GPU

| Metric | Target | Measurement | Source |
|--------|--------|-------------|--------|
| Per-pane texture (1080p) | ~8 MB | GPU memory query | YP-GFX-COMPOSITE-001 |
| Per-pane texture (4K) | ~33 MB | GPU memory query | YP-GFX-COMPOSITE-001 |
| GPU VRAM total (4 panes, 1080p) | < 100 MB | GPU memory query | Memory budget |
| GPU VRAM total (16 panes, 4K) | < 600 MB | GPU memory query | Memory budget |
