use criterion::{BatchSize, Criterion, black_box, criterion_group, criterion_main};

criterion_group!(frame_pipeline, frame_bench);

fn frame_bench(c: &mut Criterion) {
    frame_capture_benchmarks(c);
    bsp_tree_frame_benchmarks(c);
    input_routing_benchmarks(c);
    event_dispatch_benchmarks(c);
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

criterion_main!(frame_pipeline);
