use assert_cmd::Command;
use predicates::prelude::*;
use rusqlite::Connection;

fn fmn(data_dir: &std::path::Path) -> Command {
    let mut command = Command::cargo_bin("fmn").unwrap();
    command.env("FMN_DATA_DIR", data_dir);
    command.env("FMN_CONFIG", data_dir.join("config.toml"));
    command
}

#[test]
fn session_returns_uuid() {
    Command::cargo_bin("fmn")
        .unwrap()
        .arg("_session")
        .assert()
        .success()
        .stdout(predicate::str::is_match(r"^[0-9a-f-]{36}\n$").unwrap());
}

#[test]
fn record_and_list_json() {
    let root = tempfile::tempdir().unwrap();
    fmn(root.path())
        .args([
            "_record-start",
            "--shell",
            "fish",
            "--session",
            "test-session",
            "--cwd",
            "/tmp",
            "--started-at",
            "100",
        ])
        .write_stdin("echo hello")
        .assert()
        .success();
    fmn(root.path())
        .args([
            "_record-finish",
            "--session",
            "test-session",
            "--exit-code",
            "0",
            "--finished-at",
            "125",
        ])
        .assert()
        .success();
    fmn(root.path())
        .args(["history", "list", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"command\": \"echo hello\""))
        .stdout(predicate::str::contains("\"duration_ms\": 25"));
}

#[test]
fn privacy_filter_applies_to_cli_capture() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("config.toml"),
        "[history]\nignore_patterns = ['secret *']\n",
    )
    .unwrap();
    fmn(root.path())
        .args([
            "_record-start",
            "--shell",
            "zsh",
            "--session",
            "test-session",
            "--cwd",
            "/tmp",
        ])
        .write_stdin("secret show")
        .assert()
        .success();
    fmn(root.path())
        .args(["history", "list", "--json"])
        .assert()
        .success()
        .stdout(predicate::eq("[]\n"));
}

#[test]
fn clear_requires_yes_when_noninteractive() {
    let root = tempfile::tempdir().unwrap();
    fmn(root.path())
        .args(["history", "clear"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("requires --yes"));
}

#[test]
fn search_requires_an_interactive_terminal() {
    let root = tempfile::tempdir().unwrap();
    fmn(root.path())
        .args(["search", "--query", "git"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "search requires an interactive terminal",
        ));
}

#[test]
fn import_dry_run_and_repeat_are_idempotent() {
    let root = tempfile::tempdir().unwrap();
    let history = root.path().join("bash_history");
    std::fs::write(&history, "#100\necho one\n#101\necho one\n").unwrap();

    fmn(root.path())
        .args([
            "import",
            "bash",
            "--path",
            history.to_str().unwrap(),
            "--dry-run",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("would_import=2"));
    fmn(root.path())
        .args(["import", "bash", "--path", history.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("imported=2"));
    fmn(root.path())
        .args(["import", "bash", "--path", history.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("duplicates=2"));
    fmn(root.path())
        .args(["history", "list", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"source\": \"import\""));
}

#[test]
fn import_applies_privacy_filters() {
    let root = tempfile::tempdir().unwrap();
    let history = root.path().join("fish_history");
    std::fs::write(
        &history,
        "- cmd:  echo hidden\n  when: 100\n- cmd: echo visible\n  when: 101\n",
    )
    .unwrap();
    fmn(root.path())
        .args(["import", "fish", "--path", history.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("imported=1"))
        .stdout(predicate::str::contains("ignored=1"));
}

#[test]
fn atuin_import_is_idempotent_and_preserves_metadata() {
    let root = tempfile::tempdir().unwrap();
    let history = root.path().join("atuin.db");
    let connection = Connection::open(&history).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE history (
                id TEXT PRIMARY KEY,
                timestamp INTEGER NOT NULL,
                duration INTEGER NOT NULL,
                exit INTEGER NOT NULL,
                command TEXT NOT NULL,
                cwd TEXT NOT NULL,
                session TEXT NOT NULL,
                hostname TEXT NOT NULL,
                deleted_at INTEGER,
                shell TEXT
            );
            INSERT INTO history VALUES (
                'atuin-entry', 1700000000123000000, 250000000, 3,
                'cargo nextest run', '/code/fmn', 'atuin-session', 'host:user',
                NULL, 'zsh'
            );",
        )
        .unwrap();
    drop(connection);

    fmn(root.path())
        .args([
            "import",
            "atuin",
            "--path",
            history.to_str().unwrap(),
            "--dry-run",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("would_import=1"));
    fmn(root.path())
        .args(["import", "atuin", "--path", history.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("imported=1"));
    fmn(root.path())
        .args(["import", "atuin", "--path", history.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("duplicates=1"));
    fmn(root.path())
        .args(["history", "list", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "\"command\": \"cargo nextest run\"",
        ))
        .stdout(predicate::str::contains("\"started_at\": 1700000000123"))
        .stdout(predicate::str::contains("\"duration_ms\": 250"))
        .stdout(predicate::str::contains("\"exit_code\": 3"))
        .stdout(predicate::str::contains("\"cwd\": \"/code/fmn\""))
        .stdout(predicate::str::contains("\"shell\": \"zsh\""))
        .stdout(predicate::str::contains("\"source\": \"import\""));
}

#[test]
fn doctor_reports_database_health() {
    let root = tempfile::tempdir().unwrap();
    fmn(root.path())
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("[ok] database integrity: ok"))
        .stdout(predicate::str::contains("[ok] journal mode: wal"))
        .stdout(predicate::str::contains("[warn] shell integration:"));
}
