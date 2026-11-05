# FreeBSD releases

FreeBSD archives are available starting with **v0.3.0**.

The release pipeline builds on FreeBSD **15.0** for both **amd64** and **arm64**,
then verifies those exact archives on fresh **15.0** and **15.1** VMs. It does not
claim compatibility with FreeBSD 14 or other architectures. SQLite is bundled;
the verifier rejects linkage to libraries outside the FreeBSD base system.

## Installation

Download the archive and matching SHA-256 file from the GitHub release. The
asset names use Rust's architecture names:

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

- `freebsd.yml` builds on the 15.0 base system with locked dependencies and the
  dist profile. It installs pinned, checksum-verified Rust components from
  Rust's upstream distribution for both architectures. The Linux runner
  packages the resulting native binaries.
- Archives contain the executable, README, changelog, MIT license, theme
  catalogue, and all bundled palette notices. Checksums use the portable
  `SHA256  filename` format.
- `verify-freebsd.yml` checks the archives on fresh VMs for all four
  OS/architecture combinations. The 15.0 checks use base tools to verify
  checksums, linkage, version, diagnostics, and a history round trip. The
  current package repository supplies Fish and Rust built against 15.1's libc,
  so formatting, Clippy, Rust and Python tests, ShellCheck, and the packaged
  binary's shell and terminal tests run on 15.1 on both architectures.
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

For each release, check search, Unicode input, resize, overlays, and shell
insertion over SSH in a real terminal. Download the public archives and verify
their GitHub attestations after publication.
