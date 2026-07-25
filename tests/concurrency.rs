use std::sync::{Arc, Barrier};
use std::thread;

use fmn::model::Shell;
use fmn::storage::Store;

#[test]
fn concurrent_terminal_writers_do_not_lose_entries() {
    const WRITERS: usize = 8;
    const ENTRIES_PER_WRITER: usize = 40;

    let root = tempfile::tempdir().unwrap();
    let database = root.path().join("history.db");
    Store::open(&database).unwrap();
    let barrier = Arc::new(Barrier::new(WRITERS));
    let mut handles = Vec::new();

    for writer in 0..WRITERS {
        let database = database.clone();
        let barrier = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            barrier.wait();
            let mut store = Store::open(&database).unwrap();
            for entry in 0..ENTRIES_PER_WRITER {
                let session = format!("writer-{writer}");
                let command = format!("echo writer-{writer}-{entry}");
                let started_at = (writer * ENTRIES_PER_WRITER + entry) as i64;
                store
                    .record_start(&command, started_at, "/tmp", Shell::Fish, &session)
                    .unwrap();
                store.record_finish(&session, started_at + 1, 0).unwrap();
            }
        }));
    }
    for handle in handles {
        handle.join().unwrap();
    }

    let store = Store::open(&database).unwrap();
    assert_eq!(
        store
            .entries(WRITERS * ENTRIES_PER_WRITER + 1)
            .unwrap()
            .len(),
        WRITERS * ENTRIES_PER_WRITER
    );
    assert_eq!(store.integrity_check().unwrap(), "ok");
}

#[test]
fn reopening_after_an_unfinished_entry_is_consistent() {
    let root = tempfile::tempdir().unwrap();
    let database = root.path().join("history.db");
    {
        let mut store = Store::open(&database).unwrap();
        store
            .record_start("sleep 100", 10, "/tmp", Shell::Bash, "crashed")
            .unwrap();
    }
    let mut reopened = Store::open(&database).unwrap();
    let pending = reopened.entries(10).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].finished_at, None);
    assert_eq!(
        reopened.record_finish("crashed", 20, 130).unwrap(),
        Some(pending[0].id)
    );
    assert_eq!(reopened.integrity_check().unwrap(), "ok");
}
