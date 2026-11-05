# Contributing

Use the Go version in `go.mod`, Python 3, Just, Fish, Zsh, Bash, Expect, and
ShellCheck. Run `just check` before opening a PR. It formats nothing in place
and runs Go unit tests, CLI tests, and real terminal tests for the shell hooks.
Use `just build` to put the local executable at `bin/fmn`.

Use `just coverage` to combine Go unit coverage with coverage collected from
CLI and terminal subprocesses. Reports go in `bin/coverage/`. CI requires 80%
statement coverage. Run `just stress` for repeated SQLite concurrency checks.
Run `just fuzz` for the native import parser, or pass a package and fuzz target
for configuration and search.

Keep changes focused and test them at the lowest useful layer. SQLite schema
changes require an append-only migration and an upgrade test. Shell changes
must keep the syntax and interactive checks green.

## Releases

1. Update `CHANGELOG.md`.
2. Run `just release-check` to test and build all six archives in `bin/dist/`. Use
   `just dist 1.2.3` to build locally with a specific binary version.
3. Commit the release changes, then create and push an annotated `vX.Y.Z` tag.
4. Check the release workflow and verify the published checksums and attestations.

The Release workflow sets the binary version from the tag. It builds all six
archives on Linux with CGO disabled and checks the packaged binaries on each
macOS and Linux architecture. Tagged releases also require the [FreeBSD
runtime checks](docs/freebsd.md) before publication. A PR builds and verifies
the archives on macOS and Linux. A manual run includes the FreeBSD VM checks.

Stable releases update `Formula/fmn.rb` in `z0mbix/homebrew-tap` using the
`HOMEBREW_TAP_SSH_KEY` Actions secret. Prereleases leave the tap on the stable
version. To retry the tap update, run **Publish Homebrew formula** with the
existing release tag.
