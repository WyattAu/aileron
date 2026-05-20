use criterion::{BatchSize, Criterion, black_box, criterion_group, criterion_main};

criterion_group!(frame_pipeline, frame_bench, startup_benchmarks);

fn frame_bench(c: &mut Criterion) {
    frame_capture_benchmarks(c);
    bsp_tree_frame_benchmarks(c);
    input_routing_benchmarks(c);
    event_dispatch_benchmarks(c);
    multi_pane_benchmarks(c);
    tab_display_cache_benchmarks(c);
}

// ── 1. Frame capture latency ─────────────────────────────────────────────────

fn frame_capture_benchmarks(c: &mut Criterion) {
    let width: usize = 1920;
    let height: usize = 1080;
    let rowstride: u32 = (width * 4) as u32;
    let pixel_count = width * height * 4;

    c.bench_function("frame_data_create_1080p", |b| {
        b.iter_batched(
            || pixel_count,
            |n| {
                black_box(aileron::offscreen_webview::FrameData {
                    width: width as u32,
                    height: height as u32,
                    rowstride,
                    pixels: vec![0u8; n],
                })
            },
            BatchSize::SmallInput,
        )
    });

    let frame = aileron::offscreen_webview::FrameData {
        width: width as u32,
        height: height as u32,
        rowstride,
        pixels: vec![0u8; pixel_count],
    };

    c.bench_function("frame_data_clone_1080p", |b| {
        b.iter(|| black_box(frame.clone()))
    });

    c.bench_function("frame_bgra_to_rgba_copy_swap_1080p", |b| {
        b.iter_batched(
            || {
                let mut buf = vec![0u8; pixel_count];
                for chunk in buf.chunks_exact_mut(4) {
                    chunk[0] = 0xBB;
                    chunk[1] = 0x11;
                    chunk[2] = 0x22;
                    chunk[3] = 0xFF;
                }
                buf
            },
            |bgra| {
                let mut rgba = Vec::with_capacity(pixel_count);
                let stride = rowstride as usize;
                let row_bytes = width * 4;
                for row in 0..height {
                    let src_start = row * stride;
                    rgba.extend_from_slice(&bgra[src_start..src_start + row_bytes]);
                }
                for chunk in rgba.chunks_exact_mut(4) {
                    chunk.swap(0, 2);
                }
                black_box(rgba)
            },
            BatchSize::SmallInput,
        )
    });

    c.bench_function("frame_bgra_to_rgba_push_reorder_1080p", |b| {
        b.iter_batched(
            || {
                let mut buf = vec![0u8; pixel_count];
                for chunk in buf.chunks_exact_mut(4) {
                    chunk[0] = 0xBB;
                    chunk[1] = 0x11;
                    chunk[2] = 0x22;
                    chunk[3] = 0xFF;
                }
                buf
            },
            |bgra| {
                let mut rgba = Vec::with_capacity(pixel_count);
                let stride = rowstride as usize;
                let row_bytes = width * 4;
                for row in 0..height {
                    let src_start = row * stride;
                    let row_data = &bgra[src_start..src_start + row_bytes];
                    for chunk in row_data.chunks_exact(4) {
                        rgba.push(chunk[2]);
                        rgba.push(chunk[1]);
                        rgba.push(chunk[0]);
                        rgba.push(chunk[3]);
                    }
                }
                black_box(rgba)
            },
            BatchSize::SmallInput,
        )
    });

    let frame_padded = aileron::offscreen_webview::FrameData {
        width: width as u32,
        height: height as u32,
        rowstride: (width * 4 + 16) as u32,
        pixels: vec![0u8; height * (width * 4 + 16)],
    };

    c.bench_function("frame_bgra_to_rgba_with_stride_padding_1080p", |b| {
        b.iter(|| {
            let f = &frame_padded;
            let mut rgba = Vec::with_capacity(f.width as usize * f.height as usize * 4);
            let stride = f.rowstride as usize;
            let row_bytes = f.width as usize * 4;
            let height = f.height as usize;
            for row in 0..height {
                let src_start = row * stride;
                rgba.extend_from_slice(&f.pixels[src_start..src_start + row_bytes]);
            }
            for chunk in rgba.chunks_exact_mut(4) {
                chunk.swap(0, 2);
            }
            black_box(rgba)
        })
    });

    let _dirty_frame = aileron::offscreen_webview::FrameData {
        width: width as u32,
        height: height as u32,
        rowstride,
        pixels: vec![0u8; pixel_count],
    };
    c.bench_function("frame_dirty_check", |b| {
        let mut dirty = true;
        b.iter(|| {
            dirty = !dirty;
            black_box(dirty)
        })
    });
}

// ── 2. BSP tree operations (per-frame queries) ───────────────────────────────

fn bsp_tree_frame_benchmarks(c: &mut Criterion) {
    let viewport = aileron::wm::Rect::new(0.0, 0.0, 1920.0, 1080.0);
    let initial_url = url::Url::parse("https://example.com").unwrap();

    c.bench_function("frame_bsp_active_pane_lookup", |b| {
        let tree = aileron::wm::BspTree::new(viewport, initial_url.clone());
        b.iter(|| black_box(tree.active_pane_id()))
    });

    c.bench_function("frame_bsp_panes_iteration_4pane", |b| {
        let mut tree = aileron::wm::BspTree::new(viewport, initial_url.clone());
        let id1 = tree.active_pane_id();
        let id2 = tree
            .split(id1, aileron::wm::SplitDirection::Vertical, 0.5)
            .unwrap();
        let _ = tree
            .split(id1, aileron::wm::SplitDirection::Horizontal, 0.5)
            .ok();
        let _ = tree
            .split(id2, aileron::wm::SplitDirection::Horizontal, 0.5)
            .ok();
        b.iter(|| black_box(tree.panes()))
    });

    c.bench_function("frame_bsp_pane_ids_4pane", |b| {
        let mut tree = aileron::wm::BspTree::new(viewport, initial_url.clone());
        let id1 = tree.active_pane_id();
        let id2 = tree
            .split(id1, aileron::wm::SplitDirection::Vertical, 0.5)
            .unwrap();
        let _ = tree
            .split(id1, aileron::wm::SplitDirection::Horizontal, 0.5)
            .ok();
        let _ = tree
            .split(id2, aileron::wm::SplitDirection::Horizontal, 0.5)
            .ok();
        b.iter(|| black_box(tree.pane_ids()))
    });

    c.bench_function("frame_bsp_get_rect_active", |b| {
        let tree = aileron::wm::BspTree::new(viewport, initial_url.clone());
        let active = tree.active_pane_id();
        b.iter(|| black_box(tree.get_rect(active)))
    });

    c.bench_function("frame_bsp_get_rect_4pane", |b| {
        let mut tree = aileron::wm::BspTree::new(viewport, initial_url.clone());
        let id1 = tree.active_pane_id();
        let id2 = tree
            .split(id1, aileron::wm::SplitDirection::Vertical, 0.5)
            .unwrap();
        let id3 = tree
            .split(id1, aileron::wm::SplitDirection::Horizontal, 0.5)
            .unwrap();
        let id4 = tree
            .split(id2, aileron::wm::SplitDirection::Horizontal, 0.5)
            .unwrap();
        let ids = [id1, id2, id3, id4];
        let mut idx = 0usize;
        b.iter(|| {
            idx = (idx + 1) & 3;
            black_box(tree.get_rect(ids[idx]))
        })
    });

    c.bench_function("frame_bsp_leaf_count", |b| {
        let mut tree = aileron::wm::BspTree::new(viewport, initial_url.clone());
        let id1 = tree.active_pane_id();
        let id2 = tree
            .split(id1, aileron::wm::SplitDirection::Vertical, 0.5)
            .unwrap();
        let _ = tree
            .split(id1, aileron::wm::SplitDirection::Horizontal, 0.5)
            .ok();
        let _ = tree
            .split(id2, aileron::wm::SplitDirection::Horizontal, 0.5)
            .ok();
        b.iter(|| black_box(tree.leaf_count()))
    });

    c.bench_function("frame_bsp_split_close_cycle", |b| {
        b.iter_batched(
            || {
                let url = url::Url::parse("https://example.com").unwrap();
                let mut tree = aileron::wm::BspTree::new(viewport, url);
                let active = tree.active_pane_id();
                let id = tree
                    .split(active, aileron::wm::SplitDirection::Vertical, 0.5)
                    .unwrap();
                (tree, id)
            },
            |(mut tree, id)| {
                let _ = black_box(tree.close(id));
            },
            BatchSize::SmallInput,
        )
    });
}

// ── 3. Input routing latency ────────────────────────────────────────────────

fn input_routing_benchmarks(c: &mut Criterion) {
    use aileron::input::mode::{Key, KeyEvent, Mode, Modifiers};
    use aileron::input::{KeybindingRegistry, route_event};

    let registry = KeybindingRegistry::default();

    c.bench_function("frame_keybinding_lookup_bound_j", |b| {
        b.iter(|| black_box(registry.lookup(Mode::Normal, Modifiers::none(), Key::Character('j'))))
    });

    c.bench_function("frame_keybinding_lookup_unbound", |b| {
        b.iter(|| black_box(registry.lookup(Mode::Normal, Modifiers::none(), Key::Character('z'))))
    });

    c.bench_function("frame_keybinding_lookup_ctrl_combo", |b| {
        b.iter(|| black_box(registry.lookup(Mode::Normal, Modifiers::ctrl(), Key::Character('w'))))
    });

    let char_event = KeyEvent {
        key: Key::Character('j'),
        modifiers: Modifiers::none(),
        physical_key: None,
    };
    let esc_event = KeyEvent {
        key: Key::Escape,
        modifiers: Modifiers::none(),
        physical_key: None,
    };
    let ctrl_event = KeyEvent {
        key: Key::Character('e'),
        modifiers: Modifiers::ctrl(),
        physical_key: None,
    };

    c.bench_function("frame_route_normal_char", |b| {
        b.iter(|| black_box(route_event(Mode::Normal, &char_event)))
    });

    c.bench_function("frame_route_insert_char", |b| {
        b.iter(|| black_box(route_event(Mode::Insert, &char_event)))
    });

    c.bench_function("frame_route_command_char", |b| {
        b.iter(|| black_box(route_event(Mode::Command, &char_event)))
    });

    c.bench_function("frame_route_escape_all_modes", |b| {
        let modes = [Mode::Normal, Mode::Insert, Mode::Command];
        let mut i = 0usize;
        b.iter(|| {
            i = (i + 1) % 3;
            black_box(route_event(modes[i], &esc_event))
        })
    });

    c.bench_function("frame_route_ctrl_combo", |b| {
        b.iter(|| black_box(route_event(Mode::Normal, &ctrl_event)))
    });
}

// ── 4. Event dispatch ────────────────────────────────────────────────────────

fn event_dispatch_benchmarks(c: &mut Criterion) {
    use aileron::input::Action;

    c.bench_function("frame_dispatch_scroll_down", |b| {
        b.iter(|| black_box(aileron::app::dispatch::dispatch_action(&Action::ScrollDown)))
    });

    c.bench_function("frame_dispatch_split_vertical", |b| {
        b.iter(|| {
            black_box(aileron::app::dispatch::dispatch_action(
                &Action::SplitVertical,
            ))
        })
    });

    c.bench_function("frame_dispatch_multi_effect", |b| {
        b.iter(|| {
            black_box(aileron::app::dispatch::dispatch_action(
                &Action::NavigateBack,
            ))
        })
    });

    c.bench_function("frame_dispatch_all_actions", |b| {
        b.iter(|| {
            let actions = [
                Action::ScrollUp,
                Action::ScrollDown,
                Action::SplitHorizontal,
                Action::SplitVertical,
                Action::NavigateBack,
                Action::NavigateForward,
                Action::Reload,
                Action::BookmarkToggle,
                Action::ToggleReaderMode,
                Action::ToggleMinimalMode,
            ];
            for action in &actions {
                let _ = aileron::app::dispatch::dispatch_action(action);
            }
        })
    });
}

// ── 5. Multi-pane rendering simulation ────────────────────────────────────────

fn multi_pane_benchmarks(c: &mut Criterion) {
    use aileron::wm::BspTree;

    // Build a 16-pane tree using balanced splits to avoid PaneTooSmall.
    // Strategy: split every existing pane once, producing a balanced binary tree.
    fn build_16pane_tree() -> BspTree {
        let mut tree = BspTree::new(
            aileron::wm::Rect::new(0.0, 0.0, 3840.0, 2160.0),
            url::Url::parse("aileron://new").unwrap(),
        );
        // Round 1: split root -> 2 panes
        // Round 2: split both -> 4 panes
        // Round 3: split all 4 -> 8 panes
        // Round 4: split all 8 -> 16 panes
        for round in 0..4 {
            let dir = if round % 2 == 0 {
                aileron::wm::SplitDirection::Vertical
            } else {
                aileron::wm::SplitDirection::Horizontal
            };
            let ids: Vec<_> = tree.pane_ids();
            for id in ids {
                tree.split(id, dir, 0.5).unwrap();
            }
        }
        tree
    }

    // Benchmark: iter_panes() on a 16-pane tree
    c.bench_function("multi_pane_iter_16panes", |b| {
        let tree = build_16pane_tree();
        b.iter(|| {
            let panes: Vec<_> = tree.iter_panes().collect();
            black_box(panes);
        })
    });

    // Benchmark: pane_ids() on 16-pane tree
    c.bench_function("multi_pane_pane_ids_16", |b| {
        let tree = build_16pane_tree();
        b.iter(|| {
            let ids: Vec<_> = tree.iter_pane_ids().collect();
            black_box(ids);
        })
    });

    // Benchmark: split_borders() on 16-pane tree
    c.bench_function("multi_pane_split_borders_16", |b| {
        let tree = build_16pane_tree();
        b.iter(|| {
            let borders: Vec<_> = tree.iter_split_borders().collect();
            black_box(borders);
        })
    });
}

// ── 6. Tab display cache benchmarks ──────────────────────────────────────────

fn tab_display_cache_benchmarks(c: &mut Criterion) {
    use aileron::app::TabDisplayInfo;
    use std::collections::HashMap;

    // Benchmark: building a TabDisplayInfo (simulates cache rebuild)
    c.bench_function("tab_display_info_create", |b| {
        b.iter(|| {
            black_box(TabDisplayInfo {
                title: "Very Long Page Title That Exceeds Display Width".into(),
                url: "https://example.com/very/long/path/to/page.html".into(),
                truncated_title_horizontal: "Very Long Page Titl...".into(),
                truncated_title_sidebar: "Very Long Pa...".into(),
                truncated_url: "https://example.co...".into(),
            })
        })
    });

    // Benchmark: HashMap lookup for 16 tabs (simulates per-frame cache read)
    c.bench_function("tab_display_cache_lookup_16", |b| {
        let mut cache = HashMap::new();
        for i in 0..16u32 {
            let id = uuid::Uuid::new_v4();
            cache.insert(
                id,
                TabDisplayInfo {
                    title: format!("Page Title {i}"),
                    url: format!("https://example.com/page/{i}"),
                    truncated_title_horizontal: format!("Page Title {i}"),
                    truncated_title_sidebar: format!("Page Titl{i}"),
                    truncated_url: format!("https://exa{i}"),
                },
            );
        }
        let ids: Vec<_> = cache.keys().copied().collect();
        b.iter(|| {
            for id in &ids {
                black_box(cache.get(id).map(|i| i.title.as_str()));
            }
        })
    });
}

// ── 7. Startup latency ─────────────────────────────────────────────────────

fn startup_benchmarks(c: &mut Criterion) {
    // Benchmark: Config::load() (file I/O + TOML parse + migration)
    c.bench_function("startup_config_load", |b| {
        b.iter(|| black_box(aileron::config::Config::load()))
    });

    // Benchmark: BspTree::new() (tree allocation + initial pane creation)
    c.bench_function("startup_bsp_tree_new", |b| {
        b.iter_batched(
            || {
                let viewport = aileron::wm::Rect::new(0.0, 0.0, 1920.0, 1080.0);
                let url = url::Url::parse("aileron://welcome").unwrap();
                (viewport, url)
            },
            |(viewport, url)| black_box(aileron::wm::BspTree::new(viewport, url)),
            BatchSize::SmallInput,
        )
    });

    // Benchmark: KeybindingRegistry::default() (parse all default bindings)
    c.bench_function("startup_keybinding_registry", |b| {
        b.iter(|| black_box(aileron::input::KeybindingRegistry::default()))
    });

    // Benchmark: cached_theme_colors() (theme resolution + hex parsing)
    c.bench_function("startup_theme_colors_cached", |b| {
        let config = aileron::config::Config::load();
        // First call computes, subsequent return cached
        b.iter(|| black_box(config.cached_theme_colors()))
    });
}

criterion_main!(frame_pipeline);
