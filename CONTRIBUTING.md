# Contributing

## Setup

Install stable Rust, `just`, Fish, Zsh, Bash, `expect`, and ShellCheck. Then run:

```sh
just check
cargo run -- doctor
```

Keep changes focused and include tests at the lowest useful layer. Parser,
ranking, privacy, and TUI-state changes should normally include property or
table-driven tests. Shell changes must keep the syntax and PTY suites green.
Schema changes require an append-only migration and upgrade tests.

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
