#![cfg(unix)]

use std::io::Write;
use std::path::Path;
use std::process::Command;

use fmn::config::{Config, Theme};

fn run_flow(root: &Path, config: &Path, use_override: bool, shell_integration: bool, flow: &str) {
    let setup = r#"
set timeout 10
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
    spawn -noecho sh -c {stty rows 24 cols 80; exec "$@"} fmn-theme-test {*}$::argv
    see "\033\[?25l"
}
# Both search and the theme filter show a cursor. Returning to the main
# search input (row 5, column 6) signals that a save/cancel has completed.
proc finish {expected} {
    global spawn_id
    expect {
        eof {}
        timeout {error "Timed out waiting for fmn to exit"}
    }
    set status [wait]
    unset spawn_id
    if {[lindex $status 2] != 0 || [lindex $status 3] != $expected} {
        error "Unexpected fmn exit: $status"
    }
}
proc contents {} {
    set file [open $::env(FMN_TEST_CONFIG) r]
    set data [read $file]
    close $file
    return $data
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
        .args(["search", "--cwd", "/repo"])
        .env("FMN_DATA_DIR", root.join("data"))
        .env("XDG_CONFIG_HOME", root.join("xdg"))
        .env("FMN_TEST_CONFIG", config)
        .env("TERM", "xterm-256color")
        .env_remove("NO_COLOR")
        .env_remove("FMN_CONFIG");
    if use_override {
        driver.env("FMN_CONFIG", config);
    }
    if shell_integration {
        driver.arg("--shell-integration");
    }
    let output = driver.output().expect("expect must be installed");
    assert!(
        output.status.success(),
        "Theme persistence PTY failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn enter_saves_override_preserves_live_edits_and_is_loaded_on_next_search() {
    let root = tempfile::tempdir().unwrap();
    let config = root.path().join("custom.toml");
    let initial = "# my preferences\n[history]\nignore_patterns = ['secret *']\n\n[ui]\ntheme = 'mint' # appearance\nshow_inspector = false\nexecute_on_select = false\n";
    std::fs::write(&config, initial).unwrap();
    run_flow(
        root.path(),
        &config,
        true,
        true,
        r#"
set original [contents]
launch
send -- "\024"
see "Colour theme"
send -- "\033\[B"
see "2/40"
if {[contents] ne $original} {error "preview saved before confirmation"}
send -- "\033"
see "\033\[5;6H"
if {[contents] ne $original} {error "cancelled preview changed the config"}
# Emulate editing another setting while the TUI is open.
set file [open $env(FMN_TEST_CONFIG) w]
puts -nonewline $file [string map [list {execute_on_select = false} {execute_on_select = true}] $original]
close $file
send -- "\024"
see "Colour theme"
send -- "\033\[F"
see "40/40"
send -- "\r"
see "\033\[5;6H"
if {![string match {*everforest-light-soft*} [contents]]} {error "theme was not saved"}
send -- "\033"
finish 1
launch
send -- "\024"
see "Colour theme"
see "40/40"
send -- "\033\[H"
see "1/40"
send -- "\033"
see "\033\[5;6H"
send -- "\033"
finish 1
"#,
    );
    let expected = initial
        .replace("'mint'", "\"everforest-light-soft\"")
        .replace("execute_on_select = false", "execute_on_select = true");
    assert_eq!(std::fs::read_to_string(&config).unwrap(), expected);
    assert_eq!(
        Config::load(&config).unwrap().ui.theme,
        Theme::EverforestLightSoft
    );
    assert!(!root.path().join("xdg/fmn/config.toml").exists());
}

#[test]
fn previews_do_not_create_config_and_mouse_confirmation_saves_to_xdg() {
    let root = tempfile::tempdir().unwrap();
    let config = root.path().join("xdg/fmn/config.toml");
    run_flow(
        root.path(),
        &config,
        false,
        false,
        r#"
launch
send -- "\024"
see "Colour theme"
send -- "\033\[F"
see "40/40"
if {[file exists $env(FMN_TEST_CONFIG)]} {error "preview created a config file"}
send -- "\033"
see "\033\[5;6H"
send -- "\033"
finish 1
if {[file exists $env(FMN_TEST_CONFIG)]} {error "cancel created a config file"}
launch
send -- "\024"
see "Colour theme"
send -- "\033\[H"
send -- "\033\[B"
see "2/40"
# Click Rose at the second visible row (coordinates are one-based).
send -- "\033\[<0;11;7M"
see "\033\[5;6H"
if {![file exists $env(FMN_TEST_CONFIG)]} {error "mouse confirmation did not save"}
send -- "\033"
finish 1
"#,
    );
    assert_eq!(Config::load(&config).unwrap().ui.theme, Theme::Rose);
}

#[test]
fn fuzzy_filter_previews_cancel_and_empty_results_never_save_but_confirmation_does() {
    let root = tempfile::tempdir().unwrap();
    let config = root.path().join("custom.toml");
    let initial = "# appearance\n[ui]\ntheme = 'mint'\n";
    std::fs::write(&config, initial).unwrap();
    run_flow(
        root.path(),
        &config,
        true,
        true,
        r#"
set original [contents]
launch
send -- "\024"
see "Colour theme"
send -- "RP-D"
see "1/1"
if {[contents] ne $original} {error "fuzzy preview changed the config"}
send -- "\033"
see "\033\[5;6H"
if {[contents] ne $original} {error "cancelled fuzzy preview changed the config"}
send -- "\024"
see "Colour theme"
see "1/40"
send -- "zzzzzzz"
see "No matching themes"
# Wait for the completed filter frame; terminal diffs may draw the counter's
# digits separately rather than emit the literal string "0/0".
see "\033\[4;27H"
send -- "\r"
# A following edit proves Enter left the picker open with no selection to save.
send -- "\025"
see "\033\[4;20H"
if {[contents] ne $original} {error "empty results changed the config"}
send -- "RP-D"
see "1/1"
send -- "\r"
see "\033\[5;6H"
if {![string match {*rose-pine-dawn*} [contents]]} {error "fuzzy selection was not saved"}
send -- "\033"
finish 1
"#,
    );
    assert_eq!(Config::load(&config).unwrap().ui.theme, Theme::RosePineDawn);
}

#[test]
fn clicking_a_filtered_row_saves_its_theme_instead_of_the_unfiltered_row() {
    let root = tempfile::tempdir().unwrap();
    let config = root.path().join("xdg/fmn/config.toml");
    run_flow(
        root.path(),
        &config,
        false,
        false,
        r#"
launch
send -- "\024"
see "Colour theme"
send -- "\033\[200~nord\033\[201~"
see "1/1"
# Nord is the first filtered row, while Mint is first in the full catalogue.
send -- "\033\[<0;11;6M"
see "\033\[5;6H"
if {![file exists $env(FMN_TEST_CONFIG)]} {error "filtered click did not save"}
send -- "\033"
finish 1
"#,
    );
    assert_eq!(Config::load(&config).unwrap().ui.theme, Theme::Nord);
}

#[test]
fn unwritable_config_reports_failure_and_restores_terminal_without_replacing_it() {
    let root = tempfile::tempdir().unwrap();
    let config = root.path().join("custom.toml");
    let initial = "# do not replace\n[ui]\ntheme = 'mint'\n";
    std::fs::write(&config, initial).unwrap();
    run_flow(
        root.path(),
        &config,
        true,
        false,
        r#"
launch
file attributes $env(FMN_TEST_CONFIG) -permissions 0400
send -- "\024"
see "Colour theme"
send -- "\033\[B"
see "2/40"
send -- "\r"
see "\033\[?2004l"
see "\033\[?1049l"
see "\033\[?25h"
see "could not save configuration to"
see $env(FMN_TEST_CONFIG)
finish 2
"#,
    );
    assert_eq!(std::fs::read_to_string(&config).unwrap(), initial);
}
