# FreeBSD releases

Status: **implemented in the release pipeline; initial native CI qualification
pending**. These changes have not been published in a release yet.

The release pipeline builds on FreeBSD **15.0** for both **amd64** and **arm64**,
then verifies those exact archives on fresh **15.0** and **15.1** VMs. It does not
claim compatibility with FreeBSD 14 or other architectures. SQLite is bundled;
the verifier rejects linkage to libraries outside the FreeBSD base system.

## Installation

Once a release containing the FreeBSD assets has passed qualification, download
the archive and matching SHA-256 file from the GitHub release. The asset names
use Rust's architecture names:

| FreeBSD architecture | Archive |
| --- | --- |
| amd64 | `fmn-x86_64-unknown-freebsd.tar.xz` |
| arm64 | `fmn-aarch64-unknown-freebsd.tar.xz` |

In a POSIX shell, install under your own account:

```sh
case "$(uname -m)" in
  amd64) target=x86_64-unknown-freebsd ;;
  arm64|aarch64) target=aarch64-unknown-freebsd ;;
  *) echo 'Unsupported architecture' >&2; exit 1 ;;
esac
archive="fmn-${target}.tar.xz"
base=https://github.com/z0mbix/fmn/releases/latest/download
fetch "$base/$archive"
fetch "$base/$archive.sha256"
test "$(sha256 -q "$archive")" = "$(awk '{print $1}' "$archive.sha256")" || exit 1
tar -xf "$archive"
mkdir -p "$HOME/.local/bin"
install -m 755 "${archive%.tar.xz}/fmn" "$HOME/.local/bin/fmn"
"$HOME/.local/bin/fmn" --version
"$HOME/.local/bin/fmn" doctor
```

Add `$HOME/.local/bin` to your shell's `PATH`, then follow the Fish, Zsh, or Bash
setup in [the README](../README.md#install). `fmn` does not integrate with the
FreeBSD base `sh` or `csh`. Its generated release shell installer currently
supports macOS/Linux only; use the FreeBSD archives directly.

You can also build from a checkout using Rust 1.88 or newer and the base C
compiler. On FreeBSD 15.0, use an [upstream Rust toolchain](https://doc.rust-lang.org/rustc/platform-support/freebsd.html)
for your architecture. The Rust package built against 15.1 needs libc symbols
unavailable on 15.0.

```sh
cargo install --locked --path .
```

Cargo installation can run as your normal user.

## Release qualification

- `freebsd.yml` runs formatting, Clippy, all Rust tests (including SQLite
  concurrency, shell PTYs, and signal/panic terminal recovery), Python release
  tests, and ShellCheck before building with the locked dependencies and dist
  profile. It installs pinned, checksum-verified Rust components from Rust's
  upstream distribution for both architectures. The toolchain, OS,
  architecture, and shell versions are logged.
- Archives contain the executable, README, changelog, MIT license, theme
  catalogue, and all bundled palette notices. Checksums use the portable
  `SHA256  filename` format.
- `verify-freebsd.yml` boots fresh VMs for all four OS/architecture combinations.
  It verifies checksums, archive contents, dynamic linkage, version, diagnostics,
  and a record/finish/list round trip. It uses the unpacked executable for Fish,
  Zsh, and Bash insertion, opt-in execution, cancellation, and terminal recovery
  after interrupt, hangup, and repeated termination signals.
- cargo-dist 0.31.0 registers these archives and checksum files as additional
  release assets. Its global build requires both validated architectures, and
  its host stage waits for native qualification before attesting and publishing.
  Dot-prefixed archive inputs survive cargo-dist's global output cleanup and are
  excluded by its publishing glob. Artifact handoff uses the generated download
  steps, with no additional API permissions or credentials in the FreeBSD VM.
  Existing macOS/Linux targets and the four Homebrew formula inputs are retained.
- PR release workflows build and upload artifacts without publishing. The
  FreeBSD workflow can also run through `workflow_dispatch`; it uploads the
  candidate archives and verifies them without a tag or GitHub release.
  The `custom-freebsd / FreeBSD qualification` check reports one result for all
  builds and archive checks. Require it on `main` to prevent an early merge.

The VM action is pinned to a commit and the guest releases are explicit.
Keep the 15.0 baseline while it is needed for the target servers; update it
deliberately when server compatibility requirements change.

Before calling the first release qualified, record successful native CI runs
and check search, Unicode input, resize, overlays, and shell insertion over SSH
in a real terminal. After an authorized release, download and verify the public
archives and their GitHub attestations. No initial native CI or public-release
result is claimed by this document.
