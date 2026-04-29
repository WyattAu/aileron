//! Performance benchmarks for Aileron subsystems.

use criterion::{Criterion, criterion_group, criterion_main};

criterion_group!(
    benches,
    bench_bsp_tree_operations,
    bench_fuzzy_search,
    bench_pane_state,
    bench_dispatch,
    bench_filter_list_parsing,
    bench_site_settings_url_matching,
    bench_content_script_matching,
    bench_adblock_domain_check,
    bench_dispatch_with_selection,
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
                None,
            );
        });
    });

    c.bench_function("pane_state_navigate", |b| {
        let mut mgr = aileron::servo::PaneStateManager::new();
        let id = uuid::Uuid::new_v4();
        mgr.create_pane(id, url::Url::parse("https://example.com").unwrap(), None);
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

fn bench_filter_list_parsing(c: &mut Criterion) {
    let easylist_sample = r#"
! Title: EasyList
! Last modified: 2026-04-17
||ads.example.com^
||tracker.example.com^$third-party
##.ad-banner
@@||safe.example.com^
example.com##.sponsored
*.cdn.example.com^$image
"#;

    c.bench_function("filter_list_parse_easylist", |b| {
        b.iter(|| {
            let _list = aileron::net::filter_list::FilterList::parse(easylist_sample);
        });
    });
}

fn bench_site_settings_url_matching(c: &mut Criterion) {
    c.bench_function("site_settings_url_match_exact", |b| {
        b.iter(|| {
            aileron::db::site_settings::url_matches_pattern(
                "https://github.com/user/repo",
                "github.com",
                "exact",
            );
        });
    });

    c.bench_function("site_settings_url_match_wildcard", |b| {
        b.iter(|| {
            aileron::db::site_settings::url_matches_pattern(
                "https://api.github.com/v1/users",
                "*.github.com",
                "wildcard",
            );
        });
    });

    c.bench_function("site_settings_url_match_regex", |b| {
        b.iter(|| {
            aileron::db::site_settings::url_matches_pattern(
                "https://github.com/user/repo/issues/42",
                r#"github\.com"#,
                "regex",
            );
        });
    });
}

fn bench_content_script_matching(c: &mut Criterion) {
    let mut manager = aileron::scripts::ContentScriptManager::new();
    for i in 0..100 {
        manager.add_script(aileron::scripts::ContentScript {
            name: format!("script_{}", i),
            match_patterns: vec![format!("https://*.example{}.com/*", i)],
            grants: vec![],
            js_code: "console.log('bench')".into(),
            enabled: true,
            run_at: aileron::scripts::RunAt::DocumentIdle,
            match_regex: None,
        });
    }

    c.bench_function("content_script_match_100_scripts", |b| {
        b.iter(|| {
            manager.scripts_for_url(
                "https://api.example50.com/path",
                aileron::scripts::RunAt::DocumentIdle,
            );
        });
    });
}

fn bench_adblock_domain_check(c: &mut Criterion) {
    let mut adblocker = aileron::net::AdBlocker::new();

    c.bench_function("adblock_check_allowed", |b| {
        let url = url::Url::parse("https://github.com").unwrap();
        b.iter(|| adblocker.should_block(&url));
    });
}

fn bench_dispatch_with_selection(c: &mut Criterion) {
    use aileron::input::Action;

    c.bench_function("dispatch_print_action", |b| {
        b.iter(|| aileron::app::dispatch::dispatch_action(&Action::Print));
    });
}
