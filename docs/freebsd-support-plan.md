# FreeBSD support implementation plan

Status: **planned; not implemented or qualified**. Written 2026-10-21.

## Objective and initial scope

Build, test, and publish fmn for **FreeBSD/amd64** while keeping the existing
Rust implementation and macOS/Linux release paths. A successful compilation
alone is not sufficient: shell integration, SQLite behaviour, and terminal
recovery must pass on FreeBSD before binaries are advertised as supported.

The first delivery should include:

- a repeatable FreeBSD VM job in GitHub Actions;
- native tests for Fish, Zsh, and Bash;
- a release archive, checksum, and provenance attestation;
- verification of the packaged binary in a clean FreeBSD environment;
- documented installation instructions and an explicit supported OS baseline.

FreeBSD/arm64, an official FreeBSD port/package, OpenBSD, and a language rewrite
are outside this first delivery. Support in the generated shell installer is a
separate acceptance item; an archive download must not imply installer support.

## Starting point

Recheck these files when implementation starts; versions and workflows may have
changed since this plan was written.

| Area | Current repository entry point |
| --- | --- |
| Rust version and native dependencies | [`Cargo.toml`](../Cargo.toml): Rust 1.88 minimum, bundled SQLite C code |
| CI gates | [`.github/workflows/ci.yml`](../.github/workflows/ci.yml), [`justfile`](../justfile) |
| Release configuration | [`dist-workspace.toml`](../dist-workspace.toml): pinned cargo-dist 0.31.0; macOS/Linux targets |
| Generated release orchestration | [`.github/workflows/release.yml`](../.github/workflows/release.yml) |
| Release qualification | [`.github/workflows/release-check.yml`](../.github/workflows/release-check.yml), [`.github/workflows/verify-artifacts.yml`](../.github/workflows/verify-artifacts.yml) |
| Terminal lifecycle | [`src/tui/terminal.rs`](../src/tui/terminal.rs), [`tests/terminal_recovery.rs`](../tests/terminal_recovery.rs), [`tests/support/terminal_recovery.py`](../tests/support/terminal_recovery.py) |
| Shell integration | [`src/shell.rs`](../src/shell.rs), [`tests/pty_shells.rs`](../tests/pty_shells.rs), [`tests/selection_execution.rs`](../tests/selection_execution.rs) |
| SQLite behaviour | [`src/storage.rs`](../src/storage.rs), [`src/storage/reports.rs`](../src/storage/reports.rs), [`tests/concurrency.rs`](../tests/concurrency.rs) |
| Homebrew publication | [`.github/workflows/publish-homebrew.yml`](../.github/workflows/publish-homebrew.yml), [`scripts/homebrew_formula.py`](../scripts/homebrew_formula.py) |

The existing checkout may contain unfinished local work. Preserve it and keep
FreeBSD changes scoped to the files needed for each milestone.

## 1. Establish the native build environment

1. Choose an explicitly supported FreeBSD release for amd64. Prefer the oldest
   supported release we intend to distribute binaries for, then verify those
   binaries on a newer supported release. Record exact releases and minimum
   runtime requirements rather than assuming compatibility across all versions.
2. Use a FreeBSD VM on an Ubuntu GitHub Actions runner, initially in a dedicated
   `.github/workflows/freebsd.yml` workflow with manual dispatch and PR/push CI.
   Pin the VM action to a reviewed commit and pin the guest release. Avoid a
   moving `latest` guest for release builds.
3. Install and record the toolchain and test dependencies. Expected requirements
   are Rust/Cargo, rustfmt, Clippy, the base C compiler/linker, Git, Python 3,
   `just`, Fish, Zsh, Bash, Expect, ShellCheck, and archive utilities. Verify actual
   package names, available components, and `/usr/local/bin` paths in the guest.
4. Use packaged Rust or rustup as appropriate for the chosen release. Confirm
   that the compiler meets fmn's current minimum version; retain the existing
   Linux MSRV gate. Do not raise the project MSRV just to simplify VM setup.
5. Run a native release build using the existing bundled SQLite configuration:

   ```sh
   cargo build --locked --release --bin fmn
   ```

6. Record `freebsd-version`, `uname -m`, `rustc -Vv`, Cargo, C compiler, and shell
   versions in the job log. Add a cache keyed by guest release, architecture,
   Rust version, and `Cargo.lock`, plus an explicit timeout.

**Gate:** a reproducible native amd64 build from the lockfile. Compilation is
evidence of buildability, not yet evidence of supported runtime behaviour.

## 2. Qualify behaviour and fix portability issues

Run the same core gates used elsewhere, inside the FreeBSD VM:

```sh
just check
cargo run --locked --quiet -- init bash | shellcheck -s bash -
cargo test --locked --test terminal_recovery
cargo test --locked --lib tui::terminal
```

The two terminal commands are useful focused reproductions during bring-up;
their tests also run through `just check`. Keep existing Linux/macOS jobs green.
Coverage and fuzzing can remain on Linux initially; FreeBSD's required gate is
the native behavioural suite, not availability of every optional developer tool.

Investigate failures at the smallest relevant layer:

- **Shell capture and selection:** Fish, Zsh, Bash, Bash vi mode, multiline and
  quoted commands, preserved exit status, cancellation, and explicit execution
  opt-in. Confirm default selections only insert text. Use the existing
  `FMN_TEST_BASH` override if the guest's Bash path needs to be explicit.
- **PTY and terminal recovery:** raw mode, cursor visibility, alternate screen,
  bracketed paste, mouse modes, resize, panic unwinding, and shutdown signals.
  Check BSD `stty` behaviour, Expect setup, `/dev/fd` availability, and how the
  test driver inherits control-pipe descriptors. Fix a driver assumption rather
  than weakening the terminal-restoration contract.
- **Signal handling:** preserve the guard ordering that restores the terminal
  before emulating the default signal action. Repeated signals must not interrupt
  cleanup or emit a selected command.
- **Terminal state comparisons:** the existing `PENDIN` exception is specific to
  macOS. Do not apply it to FreeBSD without native evidence that the same bit is
  transient there. Avoid blanket masks or platform-wide test skips.
- **SQLite and privacy:** recording, imports, concurrent writers, migrations,
  deletion, WAL truncation, and purge retry behaviour. Continue using bundled
  SQLite unless a demonstrated FreeBSD issue requires a separately reviewed
  change; do not silently switch to the system library.
- **Recent UI features:** repository/session scopes, scoped counts and inspector
  details, statistics, theme selection, saved settings, and execution preference
  changes made during a search. Keep terminal-control escaping intact.
- **Filesystem behaviour:** owner-only data/config creation, config symlinks,
  permissions, atomic preference saves, and path handling under FreeBSD layouts.

Add regression coverage for real portability fixes. Keep timeouts bounded and
ensure failed PTY drivers terminate their children and return useful logs.

**Gate:** all required tests pass natively with no unexplained skips, alongside
the existing macOS/Linux checks. Capture the tested platform/toolchain matrix.

## 3. Package and verify the release binary

1. Produce a candidate archive such as
   `fmn-x86_64-unknown-freebsd.tar.xz`, using a top-level directory matching the
   archive's stem. Include `fmn`, the project license and documentation, the
   theme catalogue, and all bundled palette notices.
2. Generate a SHA-256 checksum in a documented, portable format. FreeBSD's
   checksum utilities differ from GNU tools; use verified commands rather than
   copying Linux-only flags into the guest script.
3. Inspect the executable with `file` and `ldd`. Record its dynamic library and
   minimum OS requirements. Bundling SQLite does not make the entire FreeBSD
   executable static. Do not publish an archive that requires undeclared build
   machine libraries or arbitrary packages left over in the build VM.
4. Transfer the archive to a clean verification VM. Extract it, verify its hash,
   and run the packaged executable, not the Cargo build-tree copy:
   - check that `--version` exactly matches the proposed release;
   - run `doctor` with isolated `FMN_DATA_DIR` and `FMN_CONFIG` paths;
   - record, finish, and list a command, checking its outcome;
   - exercise Ctrl-R insertion and opt-in execution in the supported shells;
   - exercise terminal recovery using the packaged executable;
   - verify all expected documentation and notices are present.
5. Run the baseline archive on the newer supported FreeBSD release selected in
   milestone 1. Only claim the OS range actually verified.

**Gate:** the exact archive intended for publication passes native runtime tests
in clean environments, with its checksum and dependency requirements recorded.

## 4. Integrate with the existing release workflow

The release YAML is generated. Make orchestration changes through
`dist-workspace.toml` and reusable workflows, then regenerate with the pinned
dist version. Do not hand-edit generated YAML as the persistent implementation.

Prototype the integration on a branch before changing the production release
path. The preferred first approach is a custom native FreeBSD build job using
cargo-dist's `local-artifacts-jobs` hook, with packaging and verification completed
before publication. Confirm the pinned dist version's generated dependencies,
artifact discovery, and manifest requirements; do not assume the hook alone
registers a new platform with generated installers.

Resolve these integration details explicitly:

1. **Target planning:** adding `x86_64-unknown-freebsd` to `targets` must not also
   schedule an unsupported default builder. Determine whether dist 0.31.0 can
   represent the custom FreeBSD build cleanly. If not, publish the qualified BSD
   archive as an explicitly managed additional release asset while leaving the
   existing macOS/Linux target matrix intact. Record that decision and its
   installer/manifest limitations.
2. **Release gates:** inspect the generated DAG and prove that the final publish
   job waits for the FreeBSD build and archive verification. A failed BSD job
   must prevent a release advertised as supporting FreeBSD.
3. **Artifact handoff:** use the expected `artifacts-*` naming conventions and
   confirm that archives and checksums reach the final release without collisions
   or accidental inclusion of temporary manifests and test files.
4. **Provenance:** attest the exact BSD archive using the existing provenance
   policy. The host Ubuntu runner can perform the attestation after the VM has
   returned its output. Keep publishing credentials outside the guest VM.
5. **Installer behaviour:** inspect and test the generated shell installer. If it
   cannot select the qualified FreeBSD artifact, document manual archive
   installation and leave automatic installer support unclaimed until it has a
   separate verified implementation.
6. **Homebrew:** preserve the four existing macOS/Linux formula inputs and their
   checksum verification. The extra BSD assets must not alter the tap formula or
   cause its publisher to reject the release. A FreeBSD port/package is future
   work, not part of Homebrew publication.

Run `dist generate --check`, `dist plan`, and the existing `just release-check`
after integration changes. Inspect the artifact matrix and perform an unpublished
workflow dry run that proves successful artifact handoff and failure gating.
Use PR upload mode or a test workflow with host/publish jobs disabled; do not
create a release tag merely to rehearse the build.

**Gate:** a complete release rehearsal produces verified FreeBSD assets alongside
the existing assets, with correct checksums, provenance, and publication ordering.

## 5. Document, roll out, and maintain

- Update `README.md`, `CONTRIBUTING.md`, and `CHANGELOG.md` only after the native
  gates pass. State the supported architecture and tested OS releases.
- Document source installation and verified archive installation under a normal
  user account. Identify any runtime dependencies and whether the generated
  installer supports FreeBSD.
- Run one interactive check over SSH in a real terminal: search, Unicode input,
  resize, overlays, mouse handling where available, shell insertion, and exit.
- Keep FreeBSD CI required for changes affecting supported behaviour. If it
  starts as an informational bring-up job, make it required before declaring
  FreeBSD support and before enabling release publication for it.
- After a release is authorized, download the public BSD archive, verify its
  published checksum and provenance, and repeat the native smoke check. Confirm
  existing Linux/macOS assets and Homebrew publication still succeed.
- Revisit the VM action pin, guest release support window, and Rust toolchain
  regularly. Remove or update an EOL baseline rather than silently extending the
  support claim. Add arm64 only after repeating native build and runtime gates.

If qualification fails, keep FreeBSD marked as planned and fix or withdraw the
candidate asset. Do not bypass failed tests to complete a release.

## Completion checklist

- [ ] Supported FreeBSD/amd64 baseline and newer verification release recorded.
- [ ] Native CI build and complete behavioural suite pass.
- [ ] Fish, Zsh, and Bash capture/selection tests pass.
- [ ] Signal, panic, and terminal-restoration tests pass without weakened checks.
- [ ] Clean-VM verification passes for the exact packaged binary.
- [ ] Dynamic dependencies and OS requirements are documented.
- [ ] Release gating, checksums, provenance, and artifact handoff are verified.
- [ ] Installer support or manual-install limitation is documented accurately.
- [ ] Existing macOS/Linux releases and Homebrew publication remain qualified.
- [ ] Public release verification is recorded once publication is authorized.

## References

- [Rust FreeBSD target support](https://doc.rust-lang.org/rustc/platform-support/freebsd.html)
- [FreeBSD supported releases](https://www.freebsd.org/security/#sup)
- [FreeBSD VM action](https://github.com/vmactions/freebsd-vm)
- [cargo-dist custom CI jobs](https://axodotdev.github.io/cargo-dist/book/reference/config.html#custom-ci-jobs)
- [cargo-dist CI customization](https://axodotdev.github.io/cargo-dist/book/ci/customizing.html)

Recheck upstream documentation and package availability when starting this work;
this plan does not pin a future implementation to today's supported releases.
