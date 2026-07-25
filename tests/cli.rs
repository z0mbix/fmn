use assert_cmd::Command;
use predicates::prelude::*;

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
