use std::io::Write;
use std::process::Command;

use fmn::model::Shell;
use fmn::storage::Store;

fn run_selection_flow(shell: Shell, vi: bool) {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    std::fs::create_dir(&data).unwrap();
    let config = root.path().join("config.toml");
    std::fs::write(&config, "[ui]\nexecute_on_select = true\n").unwrap();
    // The directory change proves execution happens in the interactive shell.
    // The quoted text and newline must survive the selection protocol intact.
    let selected = "cd \"$FMN_DATA_DIR\"\nprintf '%s\\n' 'literal $(not-expanded) \"quotes\"' >> executions\nprintf '\\nFMN_EXECUTED_MARKER\\n'";
    let mut store = Store::open(&data.join("history.db")).unwrap();
    store
        .record_start(selected, 1, root.path().to_str().unwrap(), shell, "seed")
        .unwrap();
    store.record_finish("seed", 2, 0).unwrap();
    drop(store);

    let mut args = Vec::new();
    let mut env = Vec::new();
    match shell {
        Shell::Bash => {
            let rc = root.path().join("bashrc");
            std::fs::write(
                &rc,
                format!(
                    "PS1='FMN> '\nHISTCONTROL=\nHISTIGNORE=\nset -e\n{}eval \"$(fmn init bash)\"\n",
                    if vi { "set -o vi\n" } else { "" }
                ),
            )
            .unwrap();
            args.extend([
                std::env::var("FMN_TEST_BASH").unwrap_or_else(|_| "bash".into()),
                "--noprofile".into(),
                "--rcfile".into(),
                rc.to_string_lossy().into_owned(),
                "-i".into(),
            ]);
        }
        Shell::Zsh => {
            let directory = root.path().join("zsh");
            std::fs::create_dir(&directory).unwrap();
            std::fs::write(
                directory.join(".zshrc"),
                "PS1='FMN> '\nsetopt ERR_EXIT\neval \"$(fmn init zsh)\"\n",
            )
            .unwrap();
            args.extend(["zsh".into(), "-d".into(), "-i".into()]);
            env.push(("ZDOTDIR", directory));
        }
        Shell::Fish => {
            let directory = root.path().join("config");
            std::fs::create_dir_all(directory.join("fish")).unwrap();
            std::fs::write(
                directory.join("fish/config.fish"),
                "function fish_prompt; printf 'FMN> '; end\nfmn init fish | source\n",
            )
            .unwrap();
            args.extend([
                "fish".into(),
                "--no-config".into(),
                "-i".into(),
                "-C".into(),
                "source \"$FMN_TEST_FISH_CONFIG\"".into(),
            ]);
            env.extend([
                ("FMN_TEST_FISH_CONFIG", directory.join("fish/config.fish")),
                ("XDG_CONFIG_HOME", directory),
                ("XDG_DATA_HOME", root.path().join("fish-data")),
            ]);
        }
    }

    let script = r#"
set timeout 12
log_user 1
proc see {text} {
    expect {
        -exact $text {}
        timeout {puts stderr "Timed out waiting for $text"; exit 3}
        eof {exit 4}
    }
}
proc configure {value} {
    set config [open $::env(FMN_CONFIG) w]
    puts $config "\[ui\]\nexecute_on_select = $value"
    close $config
}
spawn -noecho {*}$argv
see "FMN> "
send -- "\022"
see "\033\[?25l"
send -- "\r"
see "\033\[?1049l"
# No second Enter: execution must finish and return to the prompt.
see "FMN_EXECUTED_MARKER\r\n"
see "FMN> "
set marker [file join $env(FMN_DATA_DIR) executions]
if {![file exists $marker]} {exit 5}
set initial [file size $marker]

# A cancelled search must never accept the existing shell buffer, even
# immediately after an auto-executed selection.
set cancelled [file join $env(FMN_DATA_DIR) cancelled]
set pending {printf cancelled > "$FMN_DATA_DIR/cancelled"}
send -- $pending
send -- "\022"
see "\033\[?25l"
send -- "\033"
see "\033\[?1049l"
after 200
if {[file exists $cancelled] || [file size $marker] != $initial} {exit 6}
send -- "\025"

# Config errors must also leave the buffer unexecuted.
configure {'not-a-boolean'}
send -- $pending
send -- "\022"
see "invalid type"
after 200
if {[file exists $cancelled] || [file size $marker] != $initial} {exit 7}
send -- "\025"

# Read the setting on each search: disabling it needs no shell restart.
configure false
send -- "\022"
see "\033\[?25l"
send -- "cd literal"
send -- "\r"
see "\033\[?1049l"
after 200
if {[file size $marker] != $initial} {exit 8}
send -- "\r"
see "FMN_EXECUTED_MARKER\r\n"
see "FMN> "
if {[file size $marker] != 2 * $initial} {exit 9}

# The selected `cd` must have changed the parent shell's directory.
send -- {printf '\nFMN_CWD=%s\n' "$PWD"}
send -- "\r"
see "FMN_CWD=$env(FMN_DATA_DIR)"
see "FMN> "
send -- "exit\r"
expect eof
set status [wait]
exit [lindex $status 3]
"#;
    let mut driver = tempfile::NamedTempFile::new().unwrap();
    driver.write_all(script.as_bytes()).unwrap();
    let binary_dir = assert_cmd::cargo::cargo_bin("fmn")
        .parent()
        .unwrap()
        .canonicalize()
        .unwrap();
    let mut command = Command::new("expect");
    command
        .arg(driver.path())
        .args(&args)
        .env(
            "PATH",
            format!(
                "{}:{}",
                binary_dir.display(),
                std::env::var("PATH").unwrap()
            ),
        )
        .env("FMN_DATA_DIR", &data)
        .env("FMN_CONFIG", &config)
        .env("HISTFILE", root.path().join("shell-history"))
        .env("INPUTRC", "/dev/null")
        .env("TERM", "xterm-256color");
    for (key, value) in env {
        command.env(key, value);
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "Selection flow failed for {shell:?}, vi={vi}, {args:?}: {}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(data.join("executions")).unwrap(),
        "literal $(not-expanded) \"quotes\"\nliteral $(not-expanded) \"quotes\"\n"
    );
    let store = Store::open(&data.join("history.db")).unwrap();
    let entries = store.entries(20).unwrap();
    // Bash's native history can expose the first or last line of a multiline
    // Readline buffer. Both executions must still use the normal capture hooks.
    let captured = [
        selected,
        selected.lines().next().unwrap(),
        selected.lines().last().unwrap(),
    ];
    let executions: Vec<_> = entries
        .iter()
        .filter(|entry| captured.contains(&entry.command.as_str()))
        .collect();
    assert!(
        executions.len() >= 3,
        "seed plus completed captures for both selections"
    );
    assert!(executions.iter().all(|entry| entry.exit_code == Some(0)));
}

#[test]
fn fish_executes_only_with_opt_in_and_preserves_cancellation() {
    run_selection_flow(Shell::Fish, false);
}

#[test]
fn zsh_executes_only_with_opt_in_and_preserves_cancellation() {
    run_selection_flow(Shell::Zsh, false);
}

#[test]
fn bash_executes_only_with_opt_in_and_preserves_cancellation() {
    run_selection_flow(Shell::Bash, false);
}

#[test]
fn bash_vi_executes_only_with_opt_in_and_preserves_cancellation() {
    run_selection_flow(Shell::Bash, true);
}

#[test]
fn selection_protocol_only_requests_execution_for_an_accepted_opt_in_widget() {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    std::fs::create_dir(&data).unwrap();
    let config = root.path().join("config.toml");
    let selected = "printf forbidden > \"$FMN_DATA_DIR/forbidden\"";
    let mut store = Store::open(&data.join("history.db")).unwrap();
    store
        .record_start(selected, 1, "/repo", Shell::Bash, "seed")
        .unwrap();
    drop(store);
    let script = r#"
set timeout 10
log_user 1
proc see {text} {
    expect {
        -exact $text {}
        timeout {puts stderr "Timed out waiting for $text"; exit 3}
        eof {exit 4}
    }
}
spawn -noecho sh -c {stty rows 24 cols 80; exec "$@"} fmn-test {*}$argv
see "\033\[?25l"
see $env(FMN_EXPECT_ACTION)
if {$env(FMN_ACCEPT)} {send -- "\r"} else {send -- "\033"}
see "\033\[?1049l"
if {$env(FMN_ACCEPT)} {see $env(FMN_SELECTED_COMMAND)}
expect eof
set status [wait]
if {[lindex $status 3] != $env(FMN_EXPECT_STATUS)} {exit 5}
"#;
    let mut driver = tempfile::NamedTempFile::new().unwrap();
    driver.write_all(script.as_bytes()).unwrap();
    for (widget, enabled, accept, code) in [
        (false, true, true, 0),
        (true, true, true, 10),
        (true, false, true, 0),
        (true, true, false, 1),
    ] {
        std::fs::write(&config, format!("[ui]\nexecute_on_select = {enabled}\n")).unwrap();
        let mut command = Command::new("expect");
        command
            .arg(driver.path())
            .arg(assert_cmd::cargo::cargo_bin("fmn"))
            .args(["search", "--cwd", "/repo"]);
        if widget {
            command.arg("--shell-integration");
        }
        let output = command
            .env("FMN_DATA_DIR", &data)
            .env("FMN_CONFIG", &config)
            .env(
                "FMN_EXPECT_ACTION",
                if widget && enabled { "run" } else { "insert" },
            )
            .env("FMN_ACCEPT", if accept { "1" } else { "0" })
            .env("FMN_EXPECT_STATUS", code.to_string())
            .env("FMN_SELECTED_COMMAND", selected)
            .env("TERM", "xterm-256color")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "widget={widget}, enabled={enabled}, accept={accept}: {}\n{}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            !data.join("forbidden").exists(),
            "fmn must return the command; only the parent widget executes it"
        );
    }
}
