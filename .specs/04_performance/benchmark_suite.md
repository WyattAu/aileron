# Benchmark Suite Design

## Framework
- **Primary:** `criterion` crate for Rust microbenchmarks
- **Integration:** Custom frame timing overlay in egui
- **Memory:** `process_memory` crate for RSS measurement
- **GPU:** `wgpu::TimerQuery` for GPU timing

## Benchmark Groups

### BENCH-BSP: BSP Tree Operations
| Benchmark | Description | Input Size | Target |
|-----------|-------------|------------|--------|
| bench_split_4panes | Split to 4 panes | 1 → 4 | < 0.1 ms |
| bench_split_16panes | Split to 16 panes | 1 → 16 | < 0.5 ms |
| bench_close_4panes | Close pane in 4-pane layout | 4 → 3 | < 0.1 ms |
| bench_navigate_16panes | Navigate between 16 panes | 16 panes | < 0.01 ms |
| bench_resize_4panes | Resize viewport with 4 panes | 1920→1280 | < 0.5 ms |

### BENCH-MODE: Input Routing
| Benchmark | Description | Input | Target |
|-----------|-------------|-------|--------|
| bench_route_normal | Route event in Normal mode | Key event | < 0.01 ms |
| bench_route_insert | Route event in Insert mode | Key event | < 0.01 ms |
| bench_keybinding_lookup | HashMap keybinding lookup | 200 bindings | < 0.01 ms |
| bench_mode_transition | Rapid i/Esc cycles | 1000 cycles | < 10 ms total |

### BENCH-FUZZY: Fuzzy Search
| Benchmark | Description | Input Size | Target |
|-----------|-------------|------------|--------|
| bench_fuzzy_1k | Search 1K candidates | 1K items, 5-char query | < 0.5 ms |
| bench_fuzzy_10k | Search 10K candidates | 10K items, 5-char query | < 5 ms |
| bench_fuzzy_100k | Search 100K candidates | 100K items, 5-char query | < 50 ms |

### BENCH-ADBLOCK: Ad-Blocking
| Benchmark | Description | Input | Target |
|-----------|-------------|-------|--------|
| bench_adblock_init | Load EasyList | ~100K rules | < 2 s |
| bench_adblock_check | Check one URL | EasyList loaded | < 1 ms |
| bench_adblock_1000 | Check 1000 URLs | EasyList loaded | < 10 ms |

### BENCH-GFX: GPU Compositing
| Benchmark | Description | Configuration | Target |
|-----------|-------------|--------------|--------|
| bench_composite_1pane | Composite 1 pane | 1080p | < 2 ms |
| bench_composite_4panes | Composite 4 panes | 4×1080p | < 5 ms |
| bench_composite_16panes | Composite 16 panes | 16×1080p | < 10 ms |

### BENCH-MCP: MCP Server
| Benchmark | Description | Input | Target |
|-----------|-------------|-------|--------|
| bench_mcp_init | Initialize handshake | stdio | < 50 ms |
| bench_mcp_read_pane | read_active_pane | 10KB DOM | < 300 ms |
| bench_mcp_dom_to_md | DOM to Markdown | 10KB HTML | < 200 ms |

## Running Benchmarks

```bash
# All benchmarks
cargo bench

# Specific group
cargo bench -- bench_bsp

# With GPU profiling
RUST_LOG=info cargo bench -- bench_gfx 2>&1 | tee bench_results.txt
```

## Regression Detection

- Store benchmark results in `.specs/06_5_regression/baseline_metrics.toml`
- CI compares against baseline; fail if > 20% regression
- Track trends in `.specs/10_metrics/weekly_report.md`
