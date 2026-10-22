# fmn

`fmn` is a small, local-first shell history manager for Fish, Zsh, and Bash.
It records commands and outcomes in a private SQLite database and provides a
fast full-screen Ctrl-R search without accounts, sync, or a background service.

## Status

The utility is implemented and tested on macOS and Linux. Database migrations
are automatic; back up your database before upgrades.

## Features

- Fish, Zsh, and Bash capture with command, working directory, exit code,
  duration, shell, and session metadata
- fuzzy Ctrl-R search with global, directory, Git repository, or terminal-session scope
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
- local history statistics with activity, popular commands, failures, and durations
- settings overlay that saves preferences without replacing other config values

## Install

Install with Homebrew on macOS or Linux:

```sh
brew install z0mbix/tap/fmn
```

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
| Tab / Shift-Tab | cycle available search scopes forward / backward |
| Ctrl-D | inspect individual occurrences |
| Ctrl-T | fuzzy search, preview, and choose a colour theme |
| Ctrl-O | show/hide the inspector |
| Ctrl-G | open history statistics |
| Ctrl-S | open settings |
| Ctrl-X | open management actions |
| ? | show help |
| Escape | cancel |
| Click / mouse wheel | select / scroll results or occurrences |

In the actions menu, use Up/Down and Enter, or `i` to inspect, `t` for themes,
`g` for statistics, `s` for settings, and `f` to forget a command. While inspecting occurrences,
`d` deletes the selected occurrence. Removal requires typing the confirmation
word. Escape closes a dialog and returns to the previous view.

Tab cycles through **all directories → this directory → this repository → this
terminal session**. Repository scope includes the current Git working tree and
its subdirectories; session scope follows commands captured by the current
shell, even when you change directory. Unavailable scopes are skipped. Outside
a Git working tree, or without `FMN_SESSION_ID`, a configured unavailable default
falls back to global scope. Counts, latest execution metadata, and inspected
occurrences all follow the active scope. Scope changes with Tab last only for
the current search; choose a default in settings to keep it.

Ctrl-G opens a scrollable statistics view for the active scope: execution and unique
command counts, failure rate among known outcomes, average recorded duration,
activity over the last 30 UTC calendar days, and the five most used, most failed,
and slowest commands. Unknown outcomes are shown separately. Tab changes scope,
`r` refreshes the snapshot, and Escape returns to search. Opening statistics
does not change history.

Ctrl-S opens settings for the theme, inspector, default scope, execution preference,
and keybindings. Use Up/Down to select and Enter or Left/Right to change a value.
Each change applies immediately and saves only that preference, keeping other
settings and comments. Escape closes settings; saved changes remain.

Select **Keybindings**, press Enter, select an action, and press Enter again to
capture its new shortcut. Conflicting or reserved keys display an error. Escape
cancels capture; Backspace resets the selected binding and `r` resets all bindings.
The help screen and footer show your current bindings. Shortcut choices support
Ctrl plus a letter or Tab; Help can also use `?`. Ctrl-C, Ctrl-U, Ctrl-P/N, and keys that
alias Enter or Backspace are reserved for cancellation, editing, and navigation.

Ctrl-I and Tab share an encoding in traditional terminals, so they use the same
binding. Tab cycles scopes, Shift-Tab goes backward, and Ctrl-O toggles the inspector.
Typing `?` opens help; pasting inserts literal text, including question
marks, without activating shortcuts. Function keys are no longer required or
assigned. The inspector shortcut also saves its visibility preference.
Whole-history clearing remains available through `fmn history clear`.

All 37 built-in kqt themes are available, including Catppuccin, Rosé Pine,
Nord, Dracula, Tokyo Night, Ayu, Everforest, and accessible dark/light presets.
Mint remains the default, with Rose and Iris also retained: 40 themes in total.
Terminal follows your terminal's ANSI palette; No colour uses symbols and
reverse video. Press Ctrl-T and type to fuzzy search themes; matching letters are
highlighted. Searches ignore case and accept config names such as `tokyo-night`
or abbreviations such as `rpd` for Rosé Pine Dawn. Backspace edits the filter;
Ctrl-U clears it. Use Up/Down, Page Up/Down, Home/End, or the mouse wheel to
preview matches. Enter or a click applies the theme and saves it as the
default in your config file, creating the file if needed. Other settings and
comments are preserved. Escape cancels the preview without saving. A nonempty
`NO_COLOR` starts with No colour. See the [theme catalogue and palette sources](src/tui/THEMES.md)
for every config name and attribution notice.

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
theme = "mint" # e.g. catppuccin-mocha, nord, accessible-dark, no-color
show_inspector = true
execute_on_select = false
default_scope = "global" # global, directory, repository, session

# All bindings are optional; omitted actions keep their defaults.
[ui.keybindings]
inspector = "ctrl-o"
themes = "ctrl-t"
details = "ctrl-d"
statistics = "ctrl-g"
settings = "ctrl-s"
scope = "tab"
actions = "ctrl-x"
help = "?"
```

Legacy `[ui] keymap = "function"` or `"control"` values are accepted for config
compatibility; `[ui.keybindings]` now determines the shortcuts. Duplicate and
reserved bindings are rejected without replacing the config file.

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
