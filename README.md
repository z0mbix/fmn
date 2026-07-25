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
- repeated-command aggregation while preserving individual occurrences
- delete one occurrence, forget a command everywhere, or clear all history
- idempotent import of native Fish, Zsh, and Bash history files
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

Restart the shell. `Ctrl-R` now opens `fmn`; accepting a selection inserts it
into the command line without executing it.

## Use

The TUI starts from `Ctrl-R` or `fmn search`.

| Key | Action |
| --- | --- |
| Type / Backspace | refine the fuzzy query |
| Up / Down, Ctrl-P / Ctrl-N | move selection |
| Enter | insert selection without running it |
| Tab | toggle global/current-directory scope |
| F2 | inspect individual occurrences |
| Ctrl-X | open management actions |
| F1 | show help |
| Escape | cancel |

Other useful commands:

```sh
fmn doctor
fmn history list --limit 20
fmn history list --json
fmn history delete ENTRY_ID
fmn history forget COMMAND_ID
fmn history clear
```

Import existing history safely:

```sh
fmn import fish --dry-run
fmn import fish
fmn import zsh
fmn import bash
```

Imports use stable source keys, so rerunning one does not duplicate existing
occurrences. Use `--path FILE` for a non-default history file.

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
```

Filters apply to live capture and imports. History remains entirely local;
`fmn` has no network code or telemetry.

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
