use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use fmn::storage::Store;

fn binary_dir() -> PathBuf {
    assert_cmd::cargo::cargo_bin("fmn")
        .parent()
        .unwrap()
        .to_path_buf()
}

fn run_expect(command: &[&str], data_dir: &Path, extra_env: &[(&str, &Path)]) {
    let script = r#"
set timeout 15
log_user 0
spawn -noecho {*}$argv
expect "FMN> "
send -- "printf 'fmn-pty-marker\\n'\r"
expect "fmn-pty-marker"
expect "FMN> "
send -- "false\r"
expect "FMN> "
send -- "exit\r"
expect eof
"#;
    let mut script_file = tempfile::NamedTempFile::new().unwrap();
    script_file.write_all(script.as_bytes()).unwrap();
    let mut expect = Command::new("expect");
    expect.arg(script_file.path()).args(command);
    let path = format!(
        "{}:{}",
        binary_dir().display(),
        std::env::var("PATH").unwrap_or_default()
    );
    expect
        .env("PATH", path)
        .env("FMN_DATA_DIR", data_dir)
        .env("FMN_CONFIG", data_dir.join("config.toml"));
    for (name, value) in extra_env {
        expect.env(name, value);
    }
    let output = expect.output().expect("expect must be installed");
    assert!(
        output.status.success(),
        "PTY driver failed for {command:?}:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn assert_captured(data_dir: &Path, shell: &str) {
    let store = Store::open(&data_dir.join("history.db")).unwrap();
    let entries = store.entries(20).unwrap();
    let marker: Vec<_> = entries
        .iter()
        .filter(|entry| entry.command.contains("fmn-pty-marker"))
        .collect();
    assert_eq!(marker.len(), 1, "{shell} installed duplicate hooks");
    assert_eq!(marker[0].exit_code, Some(0));
    let failure = entries
        .iter()
        .find(|entry| entry.command == "false")
        .unwrap_or_else(|| panic!("{shell} did not capture the failed command"));
    assert_eq!(failure.exit_code, Some(1));
}

fn run_ctrl_r_expect(command: &[&str], data_dir: &Path, extra_env: &[(&str, &Path)]) {
    let script = r#"
set timeout 5
log_user 0
spawn -noecho {*}$argv
expect "FMN> "
send -- "echo ctrl-r-found\r"
expect "ctrl-r-found"
expect "FMN> "
send -- "\022"
after 500
send -- "\r"
after 200
send -- "\r"
expect "ctrl-r-found"
expect "FMN> "
send -- "exit\r"
expect eof
"#;
    let mut script_file = tempfile::NamedTempFile::new().unwrap();
    script_file.write_all(script.as_bytes()).unwrap();
    let path = format!(
        "{}:{}",
        binary_dir().display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let mut expect = Command::new("expect");
    expect.arg(script_file.path()).args(command);
    expect
        .env("PATH", path)
        .env("FMN_DATA_DIR", data_dir)
        .env("FMN_CONFIG", data_dir.join("config.toml"));
    for (name, value) in extra_env {
        expect.env(name, value);
    }
    let output = expect.output().unwrap();
    assert!(
        output.status.success(),
        "Ctrl-R PTY flow failed for {command:?}:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let store = Store::open(&data_dir.join("history.db")).unwrap();
    let count = store
        .entries(20)
        .unwrap()
        .into_iter()
        .filter(|entry| entry.command == "echo ctrl-r-found")
        .count();
    assert_eq!(
        count,
        2,
        "selection must wait for the second Enter\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn bash_interactive_capture_preserves_exit_status() {
    let root = tempfile::tempdir().unwrap();
    let rc = root.path().join("bashrc");
    std::fs::write(
        &rc,
        "PS1='FMN> '\neval \"$(fmn init bash)\"\neval \"$(fmn init bash)\"\n",
    )
    .unwrap();
    let data = root.path().join("data");
    run_expect(
        &[
            "bash",
            "--noprofile",
            "--rcfile",
            rc.to_str().unwrap(),
            "-i",
        ],
        &data,
        &[],
    );
    assert_captured(&data, "bash");
}

#[test]
fn zsh_interactive_capture_preserves_exit_status() {
    let root = tempfile::tempdir().unwrap();
    let zdotdir = root.path().join("zsh");
    std::fs::create_dir(&zdotdir).unwrap();
    std::fs::write(
        zdotdir.join(".zshrc"),
        "PS1='FMN> '\neval \"$(fmn init zsh)\"\neval \"$(fmn init zsh)\"\n",
    )
    .unwrap();
    let data = root.path().join("data");
    run_expect(&["zsh", "-d", "-i"], &data, &[("ZDOTDIR", &zdotdir)]);
    assert_captured(&data, "zsh");
}

#[test]
fn fish_interactive_capture_preserves_exit_status() {
    let root = tempfile::tempdir().unwrap();
    let config_home = root.path().join("config");
    let fish_config = config_home.join("fish");
    let fish_data = root.path().join("fish-data");
    std::fs::create_dir_all(&fish_config).unwrap();
    std::fs::write(
        fish_config.join("config.fish"),
        "function fish_prompt; printf 'FMN> '; end\n\
         fmn init fish | source\n\
         fmn init fish | source\n",
    )
    .unwrap();
    let data = root.path().join("data");
    run_expect(
        &["fish", "-i"],
        &data,
        &[
            ("XDG_CONFIG_HOME", &config_home),
            ("XDG_DATA_HOME", &fish_data),
        ],
    );
    assert_captured(&data, "fish");
}

#[test]
fn fish_ctrl_r_inserts_without_executing_until_enter() {
    let root = tempfile::tempdir().unwrap();
    let config_home = root.path().join("config");
    let fish_config = config_home.join("fish");
    let fish_data = root.path().join("fish-data");
    std::fs::create_dir_all(&fish_config).unwrap();
    std::fs::write(
        fish_config.join("config.fish"),
        "function fish_prompt; printf 'FMN> '; end\nfmn init fish | source\n",
    )
    .unwrap();
    let data = root.path().join("data");
    let script = r#"
set timeout 15
log_user 0
spawn -noecho {*}$argv
expect "FMN> "
send -- "bind | string match '*__fmn_search*'\r"
expect "__fmn_search"
expect "FMN> "
send -- "echo ctrl-r-found\r"
expect "ctrl-r-found"
expect "FMN> "
send -- "\022"
after 200
send -- "ctrl-r-found"
after 100
send -- "\r"
after 200
send -- "\r"
expect "FMN> "
send -- "exit\r"
expect eof
"#;
    let mut script_file = tempfile::NamedTempFile::new().unwrap();
    script_file.write_all(script.as_bytes()).unwrap();
    let path = format!(
        "{}:{}",
        binary_dir().display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let output = Command::new("expect")
        .arg(script_file.path())
        .args(["fish", "-i"])
        .env("PATH", path)
        .env("FMN_DATA_DIR", &data)
        .env("FMN_CONFIG", data.join("config.toml"))
        .env("XDG_CONFIG_HOME", &config_home)
        .env("XDG_DATA_HOME", &fish_data)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "Ctrl-R PTY flow failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(
        data.join("history.db").exists(),
        "Ctrl-R PTY flow did not create a database:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let store = Store::open(&data.join("history.db")).unwrap();
    let count = store
        .entries(20)
        .unwrap()
        .into_iter()
        .filter(|entry| entry.command == "echo ctrl-r-found")
        .count();
    assert_eq!(
        count,
        2,
        "the selected command should execute only after the second Enter\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn bash_ctrl_r_inserts_without_executing_until_enter() {
    let root = tempfile::tempdir().unwrap();
    let rc = root.path().join("bashrc");
    std::fs::write(&rc, "PS1='FMN> '\neval \"$(fmn init bash)\"\n").unwrap();
    run_ctrl_r_expect(
        &[
            "bash",
            "--noprofile",
            "--rcfile",
            rc.to_str().unwrap(),
            "-i",
        ],
        &root.path().join("data"),
        &[],
    );
}
