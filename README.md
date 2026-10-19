# fmn

`fmn` is a small, local-first shell history manager for Fish, Zsh, and Bash.
It records commands and outcomes in a private SQLite database and provides a
fast full-screen Ctrl-R search without accounts, sync, or a background service.

## Status

The core utility is implemented and tested on macOS and Linux. Version 0.1.0 is
the initial pre-release; database migrations are automatic, but back up your
database before testing pre-release upgrades.

## Features

- Fish, Zsh, and Bash capture with command, working directory, exit code,
  duration, shell, and session metadata
- fuzzy Ctrl-R search with global or current-directory scope
- Focus interface with match highlighting, a live command inspector, colour
  themes, mouse selection, and centred action/help dialogs
- repeated-command aggregation while preserving individual occurrences
- delete one occurrence, forget a command everywhere, or clear all history
- idempotent import of native Fish, Zsh, and Bash history files and Atuin's
  local database
- local SQLite storage with WAL, foreign keys, and private filesystem
  permissions
- privacy filters for leading-space commands and ignored command globs
- diagnostics through `fmn doctor`, with bounded private diagnostic logs

## Install

Build from source with Rust 1.88 or newer:

```sh
cargo install --locked --path .
```

Tagged releases provide prebuilt macOS and Linux archives and a shell
installer:

```sh
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/z0mbix/fmn/releases/latest/download/fmn-installer.sh | sh
```

Then add one line to the appropriate interactive shell configuration:

```fish
# ~/.config/fish/config.fish
fmn init fish | source
```

```zsh
# ~/.zshrc
eval "$(fmn init zsh)"
```

```bash
# ~/.bashrc
eval "$(fmn init bash)"
```

Restart the shell. `Ctrl-R` now opens `fmn`; by default, accepting a selection
inserts it into the command line without executing it.

## Use

The TUI starts from `Ctrl-R` or `fmn search`.

Focus keeps search results beside an inspector with the selected command,
execution metadata, and recent occurrences. In tall, narrow terminals the
inspector moves below the results; small terminals prioritise the result list.

| Key | Action |
| --- | --- |
| Type / Backspace | refine the fuzzy query |
| Left / Right, Home / End | move the search cursor |
| Ctrl-U | clear the query |
| Up / Down, Ctrl-P / Ctrl-N | move selection |
| Enter | insert selection; run immediately if explicitly configured |
| Tab | toggle global/current-directory scope |
| F2 | inspect individual occurrences |
| F3 | preview and choose a colour theme |
| F4 | show/hide the inspector |
| Ctrl-X | open management actions |
| F1 | show help |
| Escape | cancel |
| Click / mouse wheel | select / scroll results or occurrences |

In the actions menu, use Up/Down and Enter, or `i` to inspect, `t` for themes,
`f` to forget a command, and `c` to clear history. While inspecting occurrences,
`d` deletes the selected occurrence. Removal requires typing the confirmation
word. Escape closes a dialog and returns to the previous view.

Mint is the default dark palette; Rose and Iris provide alternatives. Terminal
uses your terminal's background and ANSI colours, and No colour uses symbols
and reverse video. Theme choices made with F3 apply to the current search.
Set `[ui].theme` in the config file for a persistent default. A nonempty
`NO_COLOR` starts with No colour.

To run accepted Ctrl-R selections immediately, opt in through the config file:

```toml
[ui]
execute_on_select = true
```

This defaults to `false`. When enabled, Enter and the first action-menu item
show **run** and execute the selection in the current Fish, Zsh, or Bash shell.
The setting is read on each search, so changes take effect without restarting.
Escape and errors never execute a selection. Standalone `fmn search` continues
to print the selected command. After upgrading fmn, restart your shell once to
load the updated integration.

Other useful commands:

```sh
fmn doctor
fmn history list --limit 20
fmn history list --json
fmn history delete ENTRY_ID
fmn history forget COMMAND_ID
fmn history clear
fmn history purge
```

Import existing history safely:

```sh
fmn import fish --dry-run
fmn import fish
fmn import zsh
fmn import bash
fmn import atuin --dry-run
fmn import atuin
```

Imports use stable source keys, so rerunning one does not duplicate existing
occurrences. Atuin imports retain timestamps, durations, exit codes, working
directories, and shell metadata when available, and skip soft-deleted entries.
By default, `fmn import atuin` follows Atuin's configured `db_path`, data
directory, and environment overrides, falling back to
`${XDG_DATA_HOME:-~/.local/share}/atuin/history.db`. Use `--path FILE` for a
non-default history file or Atuin database.

## Privacy and data

By default, data is stored under the platform data directory:

- macOS and Linux: `${XDG_DATA_HOME:-~/.local/share}/fmn/history.db`

Set `FMN_DATA_DIR` to override the data directory and `FMN_CONFIG` to override
the configuration file. Directories are created with owner-only permissions on
Unix.

The optional TOML configuration file supports:

```toml
[history]
ignore_leading_space = true
ignore_patterns = ["password *", "secret-tool *"]

[ui]
theme = "mint" # mint, rose, iris, terminal, or no-color
show_inspector = true
execute_on_select = false
```

Filters apply to live capture and imports. History remains entirely local;
`fmn` has no network code or telemetry.
Commands excluded from Bash's native history by `HISTCONTROL`, `HISTIGNORE`,
or disabled history are also excluded from fmn capture.

Deletion also checkpoints and truncates SQLite's write-ahead log. If another
reader prevents cleanup, fmn reports that the rows were removed but cleanup is
pending. Close other searches and run `fmn history purge` to retry cleanup.

## Reliability

The primary gate is:

```sh
just check
```

Additional suites:

```sh
just coverage  # requires cargo-llvm-cov and llvm-tools-preview; 90% line gate
just stress
just bench
just fuzz import_parse
just mutants
just deny
```

Tests include deterministic unit and render tests, CLI integration tests,
property tests, concurrent SQLite writers, generated-shell syntax checks, and
real interactive PTY tests for all three shells. See [CONTRIBUTING.md](CONTRIBUTING.md)
for setup and release guidance.

## License

MIT
