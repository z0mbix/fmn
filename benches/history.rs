use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use fmn::model::{SearchCandidate, Shell};
use fmn::search;
use fmn::storage::Store;

fn ranking_benchmark(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("ranking");
    for size in [1_000usize, 10_000, 100_000] {
        let candidates: Vec<_> = (0..size)
            .map(|index| SearchCandidate {
                id: index as i64,
                command: format!("git checkout feature-{index}"),
                first_seen_at: index as i64,
                last_seen_at: index as i64,
                use_count: 1 + (index % 100) as i64,
                cwd_use_count: (index % 5 == 0) as i64,
                cwd_last_seen_at: (index % 5 == 0).then_some(index as i64),
                latest_cwd: "/repo".into(),
                latest_exit_code: Some(0),
            })
            .collect();
        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(size),
            &candidates,
            |bencher, candidates| {
                bencher.iter(|| search::rank(candidates, "gco fea", false, 100_000))
            },
        );
        let mut ranker = search::Ranker::default();
        let mut matches = Vec::new();
        group.bench_with_input(
            BenchmarkId::new("reused", size),
            &candidates,
            |bencher, candidates| {
                bencher.iter(|| {
                    ranker.rank_into(
                        std::hint::black_box(candidates),
                        "gco fea",
                        false,
                        100_000,
                        &mut matches,
                    );
                    std::hint::black_box(&matches);
                })
            },
        );
    }
    group.finish();
}

fn recording_benchmark(criterion: &mut Criterion) {
    criterion.bench_function("record_start_finish", |bencher| {
        let mut store = Store::open_in_memory().unwrap();
        let mut index = 0_i64;
        bencher.iter(|| {
            index += 1;
            let session = format!("bench-{index}");
            store
                .record_start("echo benchmark", index, "/tmp", Shell::Fish, &session)
                .unwrap();
            store.record_finish(&session, index + 1, 0).unwrap();
        });
    });
}

fn disk_reads_benchmark(criterion: &mut Criterion) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("history.db");
    Store::open(&path).unwrap();
    let mut connection = rusqlite::Connection::open(&path).unwrap();
    let transaction = connection.transaction().unwrap();
    transaction
        .execute_batch(
            "WITH RECURSIVE seq(n) AS (VALUES(1) UNION ALL SELECT n+1 FROM seq WHERE n<5000)
         INSERT INTO commands SELECT n, 'git checkout feature-' || n, n, n+495000, 100 FROM seq;
         WITH RECURSIVE seq(n) AS (VALUES(1) UNION ALL SELECT n+1 FROM seq WHERE n<500000)
         INSERT INTO entries(command_id, started_at, cwd, shell, session_id)
         SELECT (n-1)%5000+1, n, '/repo', 'bash', 'bench' FROM seq;
         INSERT INTO command_directories SELECT command_id, cwd, COUNT(*), MAX(started_at)
         FROM entries GROUP BY command_id, cwd;",
        )
        .unwrap();
    transaction.commit().unwrap();
    drop(connection);
    let store = Store::open(&path).unwrap();
    criterion.bench_function("disk_search_500k_occurrences", |bencher| {
        bencher.iter(|| store.search_candidates("/repo").unwrap())
    });
    criterion.bench_function("disk_list_100_of_500k", |bencher| {
        bencher.iter(|| store.entries(100).unwrap())
    });
}

criterion_group!(
    benches,
    ranking_benchmark,
    recording_benchmark,
    disk_reads_benchmark
);
criterion_main!(benches);
