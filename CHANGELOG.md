# Changelog

All notable changes to this project will be documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] - 2026-10-22

### Added

- editable per-action Ctrl shortcuts with saved bindings, conflict checks, and reset controls
- Git repository and terminal-session search scopes, including scoped occurrence metadata
- scoped history statistics with 30-day activity, failures, frequency, and average durations
- settings overlay with persistent inspector, default scope, execution, and shortcut preferences
- all 37 kqt themes alongside the three original fmn palettes
- scrolling theme previews with keyboard paging, Home/End, and mouse-wheel support
- fuzzy theme search with highlighted matches, case-insensitive abbreviations, and config-name support
- automatically save confirmed theme choices to config while preserving other settings and comments
- upstream palette attribution and license notices in release archives

### Fixed

- restore terminal modes, cursor, and screen before exiting on shutdown signals
- preserve contrast for accessible-theme selections and monochrome theme previews
- honour execution preference changes made during the current search

### Changed

- replace function-key defaults with Ctrl shortcuts and `?` help; use Ctrl-O for the inspector, Ctrl-T for themes, and retain Tab / Shift-Tab for scope
- remove whole-history clearing from the Ctrl-X menu; keep `fmn history clear` available

## [0.1.0] - 2026-10-19

### Added

- local SQLite history capture for Fish, Zsh, and Bash
- fuzzy full-screen Ctrl-R search and history management
- idempotent import of native history files
- privacy filters, diagnostics, and private bounded logs
- unit, CLI, property, concurrency, PTY, fuzz, mutation, and benchmark suites
- cross-platform CI and release automation
- Focus interface with colour themes, command inspector, mouse support, and dialogs
- optional immediate command execution through shell integration (off by default)
- verified macOS and Linux binaries published to `z0mbix/homebrew-tap`

[Unreleased]: https://github.com/z0mbix/fmn/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/z0mbix/fmn/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/z0mbix/fmn/releases/tag/v0.1.0
