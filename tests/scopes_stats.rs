use fmn::model::Shell;
use fmn::scope::{SearchContext, SearchScope};
use fmn::storage::Store;

const DAY: i64 = 86_400_000;

fn record(
    store: &mut Store,
    command: &str,
    cwd: &str,
    session: &str,
    time: i64,
    outcome: Option<(i32, i64)>,
) -> i64 {
    let id = store
        .record_start(command, time, cwd, Shell::Fish, session)
        .unwrap();
    if let Some((exit, duration)) = outcome {
        store.record_finish(session, time + duration, exit).unwrap();
    }
    id
}

fn context() -> SearchContext {
    SearchContext {
        cwd: "/repo %_/src".into(),
        repository: Some("/repo %_".into()),
        session: Some("current".into()),
    }
}

#[test]
fn scopes_use_matching_occurrences_for_counts_latest_metadata_and_inspection() {
    let mut store = Store::open_in_memory().unwrap();
    let first = record(
        &mut store,
        "cargo test",
        "/repo %_/src",
        "current",
        DAY,
        Some((0, 100)),
    );
    record(
        &mut store,
        "cargo test",
        "/repo %_/tests",
        "old",
        2 * DAY,
        Some((1, 200)),
    );
    record(
        &mut store,
        "cargo test",
        "/elsewhere",
        "old",
        3 * DAY,
        Some((2, 300)),
    );
    record(
        &mut store,
        "not in this repo",
        "/repo %_sibling",
        "old",
        DAY,
        None,
    );
    record(
        &mut store,
        "other project in session",
        "/elsewhere",
        "current",
        DAY,
        Some((0, 50)),
    );
    let context = context();
    for (scope, count, uses, latest, exit) in [
        (SearchScope::Global, 3, 3, "/elsewhere", 2),
        (SearchScope::Directory, 1, 1, "/repo %_/src", 0),
        (SearchScope::Repository, 1, 2, "/repo %_/tests", 1),
        (SearchScope::Session, 2, 1, "/repo %_/src", 0),
    ] {
        let candidates = store.scoped_search_candidates(&context, scope).unwrap();
        assert_eq!(candidates.len(), count, "{scope:?}");
        let cargo = candidates
            .iter()
            .find(|item| item.command == "cargo test")
            .unwrap();
        assert_eq!(cargo.use_count, uses, "{scope:?}");
        assert_eq!(cargo.latest_cwd, latest);
        assert_eq!(cargo.latest_exit_code, Some(exit));
        let entries = store
            .scoped_entries_for_command(cargo.id, -1, &context, scope)
            .unwrap();
        assert_eq!(entries.len() as i64, uses);
        assert_eq!(entries.last().unwrap().id, first);
        assert_eq!(
            store
                .scoped_entries_for_command(cargo.id, 1, &context, scope)
                .unwrap()
                .len(),
            1
        );
    }
    let unavailable = SearchContext {
        repository: None,
        session: None,
        ..context
    };
    assert!(
        store
            .scoped_search_candidates(&unavailable, SearchScope::Session)
            .unwrap()
            .is_empty()
    );
    assert!(
        store
            .scoped_search_candidates(&unavailable, SearchScope::Repository)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn statistics_handle_unknown_outcomes_activity_boundaries_and_scope_without_changing_history() {
    let mut store = Store::open_in_memory().unwrap();
    let today = 50 * DAY;
    record(
        &mut store,
        "fast",
        "/repo %_/src",
        "current",
        today,
        Some((0, 100)),
    );
    record(
        &mut store,
        "fast",
        "/repo %_/tests",
        "old",
        today - DAY,
        Some((0, 200)),
    );
    record(
        &mut store,
        "slow",
        "/repo %_/src",
        "current",
        today - 29 * DAY,
        Some((2, 1_000)),
    );
    record(
        &mut store,
        "pending",
        "/repo %_/src",
        "pending",
        today - 30 * DAY,
        None,
    );
    record(
        &mut store,
        "outside",
        "/repo %_sibling",
        "old",
        today,
        Some((5, 9_000)),
    );
    record(
        &mut store,
        "future",
        "/repo %_/src",
        "future",
        today + DAY,
        None,
    );
    let before = store.entries(100).unwrap();
    let stats = store
        .history_stats(&context(), SearchScope::Repository, today)
        .unwrap();
    assert_eq!(
        (stats.total, stats.unique, stats.completed, stats.failed),
        (5, 4, 3, 1)
    );
    assert!((stats.average_duration_ms.unwrap() - 1300.0 / 3.0).abs() < 0.001);
    assert_eq!(stats.activity.iter().sum::<u64>(), 3);
    assert_eq!(stats.activity[0], 1);
    assert_eq!(stats.activity[28], 1);
    assert_eq!(stats.activity[29], 1);
    assert_eq!(stats.top_commands[0].command, "fast");
    assert_eq!(stats.top_commands[0].count, 2);
    assert_eq!(stats.failed_commands.len(), 1);
    assert_eq!(stats.failed_commands[0].command, "slow");
    assert_eq!(stats.slow_commands[0].command, "slow");
    let session = store
        .history_stats(&context(), SearchScope::Session, today)
        .unwrap();
    assert_eq!((session.total, session.failed), (2, 1));
    let empty = SearchContext {
        cwd: "/empty".into(),
        ..context()
    };
    let stats = store
        .history_stats(&empty, SearchScope::Directory, today)
        .unwrap();
    assert_eq!(stats, Default::default());
    assert_eq!(store.entries(100).unwrap(), before);
}

#[test]
fn scope_cycle_skips_missing_context_and_git_detection_supports_subdirectories() {
    let mut context = SearchContext {
        cwd: "/not-a-repo".into(),
        repository: None,
        session: None,
    };
    assert_eq!(
        context.next(SearchScope::Global, false),
        SearchScope::Directory
    );
    assert_eq!(
        context.next(SearchScope::Directory, false),
        SearchScope::Global
    );
    assert_eq!(
        context.next(SearchScope::Global, true),
        SearchScope::Directory
    );
    assert_eq!(
        context.effective(SearchScope::Repository),
        SearchScope::Global
    );
    context.repository = Some("/repo".into());
    context.session = Some("session".into());
    assert_eq!(
        context.next(SearchScope::Directory, false),
        SearchScope::Repository
    );
    assert_eq!(
        context.next(SearchScope::Repository, false),
        SearchScope::Session
    );
    assert_eq!(
        context.next(SearchScope::Global, true),
        SearchScope::Session
    );
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("a repo");
    std::fs::create_dir_all(repo.join("src/deep")).unwrap();
    assert!(
        std::process::Command::new("git")
            .args(["init", "-q"])
            .arg(&repo)
            .status()
            .unwrap()
            .success()
    );
    let detected = SearchContext::detect(repo.join("src/deep").to_string_lossy().into_owned());
    assert_eq!(detected.repository.as_deref(), Some(repo.to_str().unwrap()));
    assert!(
        SearchContext::detect(root.path().to_string_lossy().into_owned())
            .repository
            .is_none()
    );
}

#[cfg(unix)]
#[test]
fn repository_scope_includes_logical_and_physical_paths_without_prefix_collisions() {
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().unwrap();
    let real = root.path().join("real");
    let alias = root.path().join("alias");
    std::fs::create_dir_all(real.join("src")).unwrap();
    assert!(
        std::process::Command::new("git")
            .args(["init", "-q"])
            .arg(&real)
            .status()
            .unwrap()
            .success()
    );
    symlink(&real, &alias).unwrap();
    let context = SearchContext::detect(alias.join("src").to_string_lossy().into_owned());
    assert_eq!(context.repository.as_deref(), alias.to_str());
    let mut store = Store::open_in_memory().unwrap();
    record(
        &mut store,
        "logical",
        alias.join("src").to_str().unwrap(),
        "one",
        1,
        None,
    );
    record(
        &mut store,
        "physical",
        real.canonicalize().unwrap().join("src").to_str().unwrap(),
        "two",
        2,
        None,
    );
    record(
        &mut store,
        "sibling",
        root.path().join("alias-other").to_str().unwrap(),
        "three",
        3,
        None,
    );
    let candidates = store
        .scoped_search_candidates(&context, SearchScope::Repository)
        .unwrap();
    assert_eq!(candidates.len(), 2);
    assert_eq!(
        store
            .history_stats(&context, SearchScope::Repository, DAY)
            .unwrap()
            .total,
        2
    );
}
