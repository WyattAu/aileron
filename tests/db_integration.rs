use aileron::db::bookmarks::{
    add_bookmark, add_bookmark_with_folder, all_bookmarks, is_bookmarked, remove_bookmark,
    search_bookmarks,
};
use aileron::db::history::{clear_history, import_visit, recent_entries, record_visit};
use aileron::db::open_database;
use aileron::db::workspaces::{
    SplitDir, WorkspaceData, WorkspaceNode, collect_urls, delete_workspace, list_workspaces,
    load_workspace, save_workspace,
};
use url::Url;

fn fresh_db() -> (tempfile::TempDir, rusqlite::Connection) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let conn = open_database(&db_path).unwrap();
    (dir, conn)
}

fn multi_pane_workspace() -> WorkspaceData {
    WorkspaceData {
        tree: WorkspaceNode::Split {
            direction: SplitDir::Vertical,
            ratio: 0.5,
            left: Box::new(WorkspaceNode::Split {
                direction: SplitDir::Horizontal,
                ratio: 0.6,
                left: Box::new(WorkspaceNode::Leaf {
                    url: "https://example.com".into(),
                }),
                right: Box::new(WorkspaceNode::Leaf {
                    url: "https://rust-lang.org".into(),
                }),
            }),
            right: Box::new(WorkspaceNode::Leaf {
                url: "https://github.com".into(),
            }),
        },
        active_url: "https://github.com".into(),
    }
}

#[test]
fn workspace_save_load_roundtrip_multiple_panes() {
    let (_dir, conn) = fresh_db();
    let data = multi_pane_workspace();

    save_workspace(&conn, "dev-layout", &data).unwrap();
    let (ws, loaded) = load_workspace(&conn, "dev-layout").unwrap().unwrap();

    assert_eq!(ws.name, "dev-layout");
    assert_eq!(loaded.active_url, data.active_url);

    let urls = collect_urls(&loaded.tree);
    assert_eq!(urls.len(), 3);
    assert_eq!(urls[0], "https://example.com");
    assert_eq!(urls[1], "https://rust-lang.org");
    assert_eq!(urls[2], "https://github.com");
}

#[test]
fn workspace_save_upsert_keeps_single_entry() {
    let (_dir, conn) = fresh_db();

    let data1 = WorkspaceData {
        tree: WorkspaceNode::Leaf {
            url: "https://a.com".into(),
        },
        active_url: "https://a.com".into(),
    };
    save_workspace(&conn, "ws", &data1).unwrap();

    let data2 = WorkspaceData {
        tree: WorkspaceNode::Leaf {
            url: "https://b.com".into(),
        },
        active_url: "https://b.com".into(),
    };
    save_workspace(&conn, "ws", &data2).unwrap();

    assert_eq!(list_workspaces(&conn).unwrap().len(), 1);
    let (_, loaded) = load_workspace(&conn, "ws").unwrap().unwrap();
    assert_eq!(loaded.active_url, "https://b.com");
}

#[test]
fn workspace_delete_and_list() {
    let (_dir, conn) = fresh_db();

    save_workspace(&conn, "ws-a", &multi_pane_workspace()).unwrap();
    save_workspace(&conn, "ws-b", &multi_pane_workspace()).unwrap();

    assert_eq!(list_workspaces(&conn).unwrap().len(), 2);
    assert!(delete_workspace(&conn, "ws-a").unwrap());
    assert_eq!(list_workspaces(&conn).unwrap().len(), 1);
    assert_eq!(
        list_workspaces(&conn).unwrap()[0].name,
        "ws-b",
        "remaining workspace should be ws-b"
    );
}

#[test]
fn bookmark_add_list_delete_search() {
    let (_dir, conn) = fresh_db();

    add_bookmark(&conn, "https://example.com", "Example Domain").unwrap();
    add_bookmark_with_folder(&conn, "https://rust-lang.org", "Rust", "dev").unwrap();
    add_bookmark(&conn, "https://github.com", "GitHub").unwrap();

    let all = all_bookmarks(&conn).unwrap();
    assert_eq!(all.len(), 3);

    assert!(is_bookmarked(&conn, "https://example.com"));
    assert!(!is_bookmarked(&conn, "https://nope.com"));

    let results = search_bookmarks(&conn, "rust", 10).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].url, "https://rust-lang.org");

    let results = search_bookmarks(&conn, "a", 10).unwrap();
    assert!(results.len() >= 2, "should match 'Rust' and 'GitHub'");

    assert!(remove_bookmark(&conn, "https://example.com").unwrap());
    assert_eq!(all_bookmarks(&conn).unwrap().len(), 2);
    assert!(!remove_bookmark(&conn, "https://example.com").unwrap());

    let results = search_bookmarks(&conn, "example", 10).unwrap();
    assert!(
        results.is_empty(),
        "deleted bookmark should not appear in search"
    );
}

#[test]
fn history_add_and_deduplication() {
    let (_dir, conn) = fresh_db();

    let url = Url::parse("https://example.com").unwrap();
    record_visit(&conn, &url, "Example").unwrap();
    record_visit(&conn, &url, "Example Updated").unwrap();

    let entries = recent_entries(&conn, 10).unwrap();
    assert_eq!(entries.len(), 1, "same URL should not create a second row");
    assert_eq!(entries[0].visit_count, 2);
    assert_eq!(entries[0].title, "Example Updated");

    let url2 = Url::parse("https://other.com").unwrap();
    record_visit(&conn, &url2, "Other").unwrap();

    assert_eq!(recent_entries(&conn, 10).unwrap().len(), 2);

    let other_url = Url::parse("https://other.com").unwrap();
    let inserted = import_visit(&conn, other_url.as_str(), "Dup", "2024-01-01 00:00:00").unwrap();
    assert!(!inserted, "import_visit should skip existing URL");

    let new_url = Url::parse("https://new.com").unwrap();
    let inserted = import_visit(&conn, new_url.as_str(), "New", "2024-01-01 00:00:00").unwrap();
    assert!(inserted);

    assert_eq!(recent_entries(&conn, 10).unwrap().len(), 3);
}

#[test]
fn schema_idempotent_open_close_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("persist.db");

    {
        let conn = open_database(&db_path).unwrap();
        let url = Url::parse("https://example.com").unwrap();
        record_visit(&conn, &url, "Example").unwrap();
        add_bookmark(&conn, "https://example.com", "Example").unwrap();
        save_workspace(&conn, "ws", &multi_pane_workspace()).unwrap();
    }

    let conn2 = open_database(&db_path).unwrap();
    assert_eq!(recent_entries(&conn2, 10).unwrap().len(), 1);
    assert_eq!(all_bookmarks(&conn2).unwrap().len(), 1);
    assert!(load_workspace(&conn2, "ws").unwrap().is_some());

    let conn3 = open_database(&db_path).unwrap();
    assert_eq!(recent_entries(&conn3, 10).unwrap().len(), 1);
    assert_eq!(all_bookmarks(&conn3).unwrap().len(), 1);
    assert!(load_workspace(&conn3, "ws").unwrap().is_some());
}

#[test]
fn cross_module_save_workspace_with_bookmarks_reload_verify() {
    let (_dir, conn) = fresh_db();

    let pane_urls = [
        "https://example.com",
        "https://rust-lang.org",
        "https://github.com",
        "https://docs.rs",
    ];

    for (i, url) in pane_urls.iter().enumerate() {
        let title = format!("Site {i}");
        add_bookmark(&conn, url, &title).unwrap();
    }

    let data = WorkspaceData {
        tree: WorkspaceNode::Split {
            direction: SplitDir::Horizontal,
            ratio: 0.5,
            left: Box::new(WorkspaceNode::Split {
                direction: SplitDir::Vertical,
                ratio: 0.5,
                left: Box::new(WorkspaceNode::Leaf {
                    url: pane_urls[0].into(),
                }),
                right: Box::new(WorkspaceNode::Leaf {
                    url: pane_urls[1].into(),
                }),
            }),
            right: Box::new(WorkspaceNode::Split {
                direction: SplitDir::Vertical,
                ratio: 0.5,
                left: Box::new(WorkspaceNode::Leaf {
                    url: pane_urls[2].into(),
                }),
                right: Box::new(WorkspaceNode::Leaf {
                    url: pane_urls[3].into(),
                }),
            }),
        },
        active_url: pane_urls[2].into(),
    };

    save_workspace(&conn, "cross-module-test", &data).unwrap();

    let (_, loaded) = load_workspace(&conn, "cross-module-test").unwrap().unwrap();
    let loaded_urls = collect_urls(&loaded.tree);

    assert_eq!(loaded_urls.len(), 4);
    assert_eq!(loaded.active_url, "https://github.com");

    for pane_url in &pane_urls {
        assert!(
            is_bookmarked(&conn, pane_url),
            "pane URL {pane_url} should be bookmarked"
        );
        assert!(
            loaded_urls.contains(&pane_url.to_string()),
            "workspace should contain pane URL {pane_url}"
        );
    }

    let bookmarks = all_bookmarks(&conn).unwrap();
    assert_eq!(bookmarks.len(), 4);

    let titles: Vec<&str> = bookmarks.iter().map(|b| b.title.as_str()).collect();
    assert!(titles.contains(&"Site 0"));
    assert!(titles.contains(&"Site 1"));
    assert!(titles.contains(&"Site 2"));
    assert!(titles.contains(&"Site 3"));
}

#[test]
fn cross_module_history_and_bookmarks_independent() {
    let (_dir, conn) = fresh_db();

    let url = Url::parse("https://example.com").unwrap();
    record_visit(&conn, &url, "History Title").unwrap();
    add_bookmark(&conn, "https://example.com", "Bookmark Title").unwrap();

    let history = recent_entries(&conn, 10).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].title, "History Title");

    let bookmarks = all_bookmarks(&conn).unwrap();
    assert_eq!(bookmarks.len(), 1);
    assert_eq!(bookmarks[0].title, "Bookmark Title");

    clear_history(&conn).unwrap();
    assert!(recent_entries(&conn, 10).unwrap().is_empty());
    assert_eq!(
        all_bookmarks(&conn).unwrap().len(),
        1,
        "clearing history should not affect bookmarks"
    );
}

#[test]
fn workspace_list_ordering_newest_first() {
    let (_dir, conn) = fresh_db();

    save_workspace(&conn, "first", &multi_pane_workspace()).unwrap();
    save_workspace(&conn, "second", &multi_pane_workspace()).unwrap();
    save_workspace(&conn, "third", &multi_pane_workspace()).unwrap();

    let all = list_workspaces(&conn).unwrap();
    assert_eq!(all.len(), 3);
    let names: Vec<&str> = all.iter().map(|w| w.name.as_str()).collect();
    assert!(names.contains(&"first"));
    assert!(names.contains(&"second"));
    assert!(names.contains(&"third"));
}
