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
   clean macOS and Linux host, plus FreeBSD 15.0 and 15.1 on both architectures.

The generated release workflow is owned by `dist-workspace.toml`; regenerate it
with pinned dist 0.31.0 after changing release configuration.

Release tags run the full release checks before building. Each packaged binary
is checksum-verified and runs `--version` and `doctor` on its native macOS or
Linux architecture before the GitHub release is published.

FreeBSD amd64 and arm64 use the reusable `freebsd.yml` workflow. It builds on
15.0, runs the complete native test suite, and packages bundled SQLite together
with the documentation and palette notices. Fresh 15.0 and 15.1 VMs then verify
the archives, base-system linkage, history round trips, shell integration, and
terminal recovery. These jobs gate publication. Initial native qualification is
pending; see [the FreeBSD release notes](docs/freebsd.md).

FreeBSD assets are registered through `extra-artifacts`; they are deliberately
absent from cargo-dist's builtin target matrix and shell installer. The global
build verifies and collects both archives and checksums from the downloaded
workflow artifacts. Dot-prefixed input copies survive cargo-dist's global output
cleanup; the generated publishing glob excludes those copies from release assets.
Provenance attestations run at the host stage for all release assets after the
native checks pass. PRs use upload mode to rehearse builds and artifact handoff
without publishing. The **Build FreeBSD releases** workflow can also be run
manually without creating a tag or release.

Stable releases then update `Formula/fmn.rb` in `z0mbix/homebrew-tap`. The
publisher downloads the public archives and verifies all four SHA-256 hashes
before generating the formula. It uses a dedicated write-enabled deploy key
for the tap, stored here as the `HOMEBREW_TAP_SSH_KEY` Actions secret. Prereleases
leave the tap on the latest stable version. To retry tap publication for an
existing release, run the **Publish Homebrew formula** workflow with its tag.
