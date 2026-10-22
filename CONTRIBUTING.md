# Contributing

## Setup

Install stable Rust, Python 3, `just`, Fish, Zsh, Bash, `expect`, and ShellCheck. Then run:

```sh
just check
cargo run -- doctor
```

Keep changes focused and include tests at the lowest useful layer. Parser,
ranking, privacy, and TUI-state changes should normally include property or
table-driven tests. Shell changes must keep the syntax and PTY suites green.
Schema changes require an append-only migration and upgrade tests.

Terminal lifecycle changes must keep the recovery suite green:

```sh
cargo test --test terminal_recovery
cargo test --lib tui::terminal
```

These tests use isolated PTYs to check terminal settings, cursor visibility,
alternate screen, mouse capture, and bracketed paste after shutdown signals
and panic unwinding. They run as part of `just check` on macOS and Linux.

## Extended verification

Install the optional tools:

```sh
cargo install cargo-llvm-cov cargo-fuzz cargo-mutants cargo-deny
rustup component add llvm-tools-preview
```

Run `just coverage`, `just stress`, and the relevant fuzz target. Benchmarks are
regression signals rather than hard timing gates because hosts differ.

## Releases

1. Run `just release-check`.
2. Update `CHANGELOG.md` and the version in `Cargo.toml`.
3. Run `dist plan` and inspect the artifact matrix.
4. Commit the release, create an annotated `vX.Y.Z` tag, and push it.
5. Verify checksums, GitHub artifact attestations, and installation on one
   clean macOS and Linux host.

The generated release workflow is owned by `dist-workspace.toml`; regenerate it
with pinned dist 0.31.0 after changing release configuration.

Release tags run the full release checks before building. Each packaged binary
is checksum-verified and runs `--version` and `doctor` on its native macOS or
Linux architecture before the GitHub release is published.

Stable releases then update `Formula/fmn.rb` in `z0mbix/homebrew-tap`. The
publisher downloads the public archives and verifies all four SHA-256 hashes
before generating the formula. It uses a dedicated write-enabled deploy key
for the tap, stored here as the `HOMEBREW_TAP_SSH_KEY` Actions secret. Prereleases
leave the tap on the latest stable version. To retry tap publication for an
existing release, run the **Publish Homebrew formula** workflow with its tag.
