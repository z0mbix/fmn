use std::io::Write;
use std::process::{Command, Stdio};

use assert_cmd::Command as AssertCommand;
use predicates::prelude::*;

fn generated(shell: &str) -> String {
    let output = AssertCommand::cargo_bin("fmn")
        .unwrap()
        .args(["init", shell])
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap()
}

fn assert_shell_syntax(shell: &str, args: &[&str], script: &str) {
    let Ok(mut child) = Command::new(shell)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    else {
        panic!("required test shell {shell} is not installed");
    };
    child
        .stdin
        .take()
        .unwrap()
        .write_all(script.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{shell} rejected generated script:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn init_rejects_unsupported_shell() {
    AssertCommand::cargo_bin("fmn")
        .unwrap()
        .args(["init", "powershell"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unsupported shell"));
}

#[test]
fn generated_fish_is_valid() {
    assert_shell_syntax("fish", &["-n"], &generated("fish"));
}

#[test]
fn generated_zsh_is_valid() {
    assert_shell_syntax("zsh", &["-n"], &generated("zsh"));
}

#[test]
fn generated_bash_is_valid_on_macos_bash_3() {
    assert_shell_syntax("bash", &["-n"], &generated("bash"));
}
