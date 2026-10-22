#![cfg(unix)]

use std::io::Write;
use std::path::Path;
use std::process::Command;

use fmn::config::Config;
use fmn::keybindings::{KeyAction, KeyBinding};
use fmn::model::Shell;
use fmn::scope::SearchScope;
use fmn::storage::Store;

fn setup(root: &Path) -> std::path::PathBuf {
    let repo = root.join("repo");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::create_dir_all(root.join("data")).unwrap();
    assert!(
        Command::new("git")
            .args(["init", "-q"])
            .arg(&repo)
            .status()
            .unwrap()
            .success()
    );
    std::fs::write(
        root.join("config.toml"),
        "# keep my settings\n[history]\nignore_patterns = ['secret *']\n[ui]\ntheme = 'mint'\n",
    )
    .unwrap();
    let mut store = Store::open(&root.join("data/history.db")).unwrap();
    for (command, cwd, session) in [
        ("inside-source", repo.join("src"), "current"),
        ("repo-root", repo.clone(), "old"),
        ("repo-tests", repo.join("tests"), "old"),
        ("outside-current", root.join("outside"), "current"),
        ("outside-old", root.join("outside"), "old"),
    ] {
        store
            .record_start(command, 1, cwd.to_str().unwrap(), Shell::Fish, session)
            .unwrap();
        store.record_finish(session, 101, 0).unwrap();
    }
    repo.join("src")
}

fn run(root: &Path, cwd: &Path, shell_integration: bool, flow: &str) {
    let setup = r#"
set timeout 12
log_user 1
proc see {text} {
    global spawn_id
    expect {
        -exact $text {}
        timeout {error "Timed out waiting for $text"}
        eof {error "fmn exited while waiting for $text"}
    }
}
proc launch {} {
    global spawn_id spawn_out
    spawn -noecho sh -c {stty rows 24 cols 80; exec "$@"} fmn-features {*}$::argv
    see "\033\[5;6H"
}
proc finish {expected} {
    global spawn_id
    expect { eof {} timeout {error "fmn did not exit"} }
    set status [wait]
    unset spawn_id
    if {[lindex $status 2] != 0 || [lindex $status 3] != $expected} {error "Unexpected fmn exit: $status"}
}
proc cleanup {} {
    global spawn_id
    if {[info exists spawn_id]} {
        catch {exec /bin/kill -KILL [exp_pid] 2>@ stderr}
        catch {close}
        catch {wait}
    }
}
"#;
    let mut script = tempfile::NamedTempFile::new().unwrap();
    write!(script, "{setup}\nif {{[catch {{\n{flow}\n}} message]}} {{\nputs stderr $message\ncleanup\nexit 1\n}}\n").unwrap();
    let mut driver = Command::new("expect");
    driver
        .arg(script.path())
        .arg(assert_cmd::cargo::cargo_bin!("fmn"))
        .arg("search")
        .arg("--cwd")
        .arg(cwd)
        .env("FMN_DATA_DIR", root.join("data"))
        .env("FMN_CONFIG", root.join("config.toml"))
        .env("FMN_SESSION_ID", "current")
        .env("TERM", "xterm-256color")
        .env_remove("NO_COLOR");
    if shell_integration {
        driver.arg("--shell-integration");
    }
    let output = driver.output().expect("expect must be installed");
    assert!(
        output.status.success(),
        "PTY feature test failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn scopes_and_statistics_use_repository_and_session_context_in_a_real_terminal() {
    let root = tempfile::tempdir().unwrap();
    let cwd = setup(root.path());
    run(
        root.path(),
        &cwd,
        false,
        r#"
launch
send -- "\t"
see "This directory"
see "\033\[5;6H"
send -- "\t"
see "repo"
see "\033\[5;6H"
send -- "\007"
see "History statistics"
see "3 executions"
send -- "\t"
see "session"
send -- "\033"
see "\033\[5;6H"
send -- "outside"
see "\033\[5;13H"
send -- "\r"
see "outside-current"
finish 0
"#,
    );
    assert_eq!(
        Config::load(&root.path().join("config.toml"))
            .unwrap()
            .ui
            .default_scope,
        SearchScope::Global
    );
}

#[test]
fn settings_save_and_custom_shortcuts_work_after_reopening_the_ui() {
    let root = tempfile::tempdir().unwrap();
    let cwd = setup(root.path());
    run(
        root.path(),
        &cwd,
        true,
        r#"
launch
send -- "\030"
see "Command actions"
send -- "c"
send -- "s"
see "Settings"
send -- "\r"
see "Colour theme"
send -- "\033\[200~nord\033\[201~"
see "1/1"
send -- "\r"
see "Settings"
send -- "\033\[B"
send -- "\r"
send -- "\033\[B"
see "Applies now and when you next open fmn"
send -- "\033\[C"
send -- "\033\[B"
see "using shell integration"
send -- "\033\[B"
send -- "\r"
see "Keybindings"
send -- "\r"
send -- "\024"
see "already used"
send -- "\033"
send -- "\r"
send -- "\005"
# Rebind Settings to Ctrl-A, then prove it works after restarting fmn.
send -- "\033\[B\033\[B\033\[B\033\[B"
send -- "\r"
send -- "\001"
send -- "\033"
see "Settings"
send -- "\033"
see "\033\[5;6H"
send -- "\033"
finish 1
launch
send -- "\001"
see "Settings"
send -- "\033\[F"
send -- "\r"
see "Keybindings"
see "Ctrl-E"
see "Ctrl-A"
send -- "\033"
see "Settings"
send -- "\033"
see "\033\[5;6H"
send -- "\007"
see "History statistics"
see "1 execution"
send -- "\033"
see "\033\[5;6H"
send -- "\033"
finish 1
"#,
    );
    let text = std::fs::read_to_string(root.path().join("config.toml")).unwrap();
    assert!(text.starts_with("# keep my settings\n"));
    let config = Config::load(&root.path().join("config.toml")).unwrap();
    assert_eq!(config.ui.theme, fmn::config::Theme::Nord);
    assert!(!config.ui.show_inspector);
    assert_eq!(config.ui.default_scope, SearchScope::Directory);
    assert_eq!(
        config.ui.keybindings.get(KeyAction::Inspector),
        KeyBinding::Ctrl('e')
    );
    assert_eq!(
        config.ui.keybindings.get(KeyAction::Settings),
        KeyBinding::Ctrl('a')
    );
    assert!(!config.ui.execute_on_select);
    assert_eq!(config.history.ignore_patterns, ["secret *"]);
}

#[test]
fn tab_and_shift_tab_cycle_scopes_and_ctrl_o_saves_inspector() {
    let root = tempfile::tempdir().unwrap();
    let cwd = setup(root.path());
    run(
        root.path(),
        &cwd,
        false,
        r#"
launch
send -- "\017"
see "\033\[5;6H"
send -- "\t"
see "This directory"
see "\033\[5;6H"
send -- "\033\[Z"
see "All directories"
see "\033\[5;6H"
send -- "\033\[200~outside-old\033\[201~"
see "\033\[5;17H"
send -- "\r"
see "outside-old"
finish 0
"#,
    );
    let config = Config::load(&root.path().join("config.toml")).unwrap();
    assert!(!config.ui.show_inspector);
    assert_eq!(config.ui.default_scope, SearchScope::Global);
}

#[test]
fn changing_execution_in_settings_affects_this_selection_but_standalone_only_prints() {
    for shell_integration in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let cwd = setup(root.path());
        let expected = if shell_integration { 10 } else { 0 };
        run(
            root.path(),
            &cwd,
            shell_integration,
            &format!(
                r#"
launch
send -- "\023"
see "Settings"
send -- "\033\[B\033\[B\033\[B"
see "using shell integration"
send -- "\r"
send -- "\033"
see "\033\[5;6H"
send -- "\r"
finish {expected}
"#
            ),
        );
        assert!(
            Config::load(&root.path().join("config.toml"))
                .unwrap()
                .ui
                .execute_on_select
        );
    }
}
