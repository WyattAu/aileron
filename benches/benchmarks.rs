//! Performance benchmarks for Aileron subsystems.

use criterion::{criterion_group, criterion_main, Criterion};

criterion_group!(
    benches,
    bench_bsp_tree_operations,
    bench_fuzzy_search,
    bench_pane_state,
    bench_dispatch,
);

fn bench_bsp_tree_operations(c: &mut Criterion) {
    let viewport = aileron::wm::Rect::new(0.0, 0.0, 1920.0, 1080.0);
    let initial_url = url::Url::parse("https://example.com").unwrap();

    c.bench_function("bsp_create", |b| {
        b.iter(|| aileron::wm::BspTree::new(viewport, initial_url.clone()));
    });

    c.bench_function("bsp_split_vertical", |b| {
        let mut tree = aileron::wm::BspTree::new(viewport, initial_url.clone());
        let active = tree.active_pane_id();
        b.iter(|| {
            let _id = uuid::Uuid::new_v4();
            tree.split(active, aileron::wm::SplitDirection::Vertical, 0.5)
                .ok();
        });
    });

    c.bench_function("bsp_split_horizontal", |b| {
        let mut tree = aileron::wm::BspTree::new(viewport, initial_url.clone());
        let active = tree.active_pane_id();
        b.iter(|| {
            let _id = uuid::Uuid::new_v4();
            tree.split(active, aileron::wm::SplitDirection::Horizontal, 0.5)
                .ok();
        });
    });

    c.bench_function("bsp_navigate_4pane_grid", |b| {
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
        b.iter(|| tree.panes().len());
    });

    c.bench_function("bsp_close", |b| {
        b.iter(|| {
            let url = url::Url::parse("https://example.com").unwrap();
            let mut tree = aileron::wm::BspTree::new(viewport, url);
            let active = tree.active_pane_id();
            let id = tree
                .split(active, aileron::wm::SplitDirection::Vertical, 0.5)
                .unwrap();
            tree.close(id).ok();
        });
    });

    c.bench_function("bsp_resize", |b| {
        let url = url::Url::parse("https://example.com").unwrap();
        let mut tree = aileron::wm::BspTree::new(viewport, url);
        let active = tree.active_pane_id();
        let id = tree
            .split(active, aileron::wm::SplitDirection::Vertical, 0.5)
            .unwrap();
        b.iter(|| tree.resize_pane(id, 0.05));
    });
}

fn bench_fuzzy_search(c: &mut Criterion) {
    let mut engine = aileron::ui::FuzzySearch::new();

    for i in 0..100 {
        engine.upsert(aileron::ui::SearchItem {
            id: format!("entry_{}", i),
            label: format!("entry_{}", i),
            description: format!("Entry {} description for testing", i),
            category: aileron::ui::SearchCategory::History,
        });
    }

    c.bench_function("fuzzy_search_short", |b| {
        b.iter(|| engine.search("entry_42", 10));
    });

    c.bench_function("fuzzy_search_long", |b| {
        b.iter(|| engine.search("description testing for", 10));
    });

    c.bench_function("fuzzy_search_no_match", |b| {
        b.iter(|| engine.search("zzzznotfound", 10));
    });
}

fn bench_pane_state(c: &mut Criterion) {
    c.bench_function("pane_state_create", |b| {
        b.iter(|| {
            let mut mgr = aileron::servo::PaneStateManager::new();
            mgr.create_pane(
                uuid::Uuid::new_v4(),
                url::Url::parse("https://example.com").unwrap(),
            );
        });
    });

    c.bench_function("pane_state_navigate", |b| {
        let mut mgr = aileron::servo::PaneStateManager::new();
        let id = uuid::Uuid::new_v4();
        mgr.create_pane(id, url::Url::parse("https://example.com").unwrap());
        let url = url::Url::parse("https://rust-lang.org").unwrap();
        b.iter(|| mgr.get_mut(&id).map(|e| e.navigate(&url)));
    });
}

fn bench_dispatch(c: &mut Criterion) {
    use aileron::input::Action;

    c.bench_function("dispatch_all_actions", |b| {
        b.iter(|| {
            let actions = vec![
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
        });
    });
}

criterion_main!(benches);
