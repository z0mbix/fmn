#![cfg(unix)]

use std::process::Command;

use fmn::model::Shell;
use fmn::storage::Store;

fn check_shutdown(scenario: &str, shell_integration: bool) {
    let root = tempfile::tempdir().unwrap();
    let config = root.path().join("config.toml");
    std::fs::write(&config, "[ui]\nexecute_on_select = true\n").unwrap();
    let mut store = Store::open(&root.path().join("history.db")).unwrap();
    store
        .record_start("echo should-not-run", 1, "/repo", Shell::Bash, "seed")
        .unwrap();
    store.record_finish("seed", 2, 0).unwrap();
    drop(store);

    let mut driver = Command::new("python3");
    driver
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/terminal_recovery.py"
        ))
        .arg(scenario)
        .arg(if shell_integration {
            "stdout"
        } else {
            "stderr"
        })
        .arg(assert_cmd::cargo::cargo_bin!("fmn"))
        .args(["search", "--cwd", "/repo"])
        .env("FMN_DATA_DIR", root.path())
        .env("FMN_CONFIG", config);
    if shell_integration {
        driver.arg("--shell-integration");
    }
    let output = driver.output().expect("Python 3 must be installed");
    assert!(
        output.status.success(),
        "{scenario} (shell integration: {shell_integration}):\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn sigterm_restores_terminal_without_emitting_a_command() {
    for shell_integration in [false, true] {
        check_shutdown("sigterm", shell_integration);
        check_shutdown("repeated-sigterm", shell_integration);
    }
}

#[test]
fn external_interrupt_and_hangup_restore_terminal() {
    for shell_integration in [false, true] {
        check_shutdown("sigint", shell_integration);
        check_shutdown("sighup", shell_integration);
    }
}
