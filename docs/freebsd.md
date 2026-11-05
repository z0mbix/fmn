# FreeBSD releases

The release workflow cross-compiles static FreeBSD amd64 and arm64 binaries
with Go and verifies the exact archives on FreeBSD 15.0 and 15.1. Initial
native qualification of the Go release archives is pending.

Download the archive for your architecture and its matching SHA-256 file from
the [GitHub release](https://github.com/z0mbix/fmn/releases).

| FreeBSD architecture | Archive |
| --- | --- |
| amd64 | `fmn-x86_64-unknown-freebsd.tar.xz` |
| arm64 | `fmn-aarch64-unknown-freebsd.tar.xz` |

Run these commands in `sh` after downloading both files. Use
`aarch64-unknown-freebsd` on arm64.

```sh
target=x86_64-unknown-freebsd
archive="fmn-$target.tar.xz"
test "$(sha256 -q "$archive")" = "$(awk '{ print $1 }' "$archive.sha256")" &&
  tar -xf "$archive" &&
  install -m 755 "fmn-$target/fmn" "$HOME/.local/bin/fmn"
```

Create `~/.local/bin` and put it on your shell's PATH before installing there.
The release also provides `fmn-installer.sh`, which detects both FreeBSD
architectures. Set `FMN_INSTALL_DIR` to choose its destination.

To build from a checkout with the Go version in `go.mod`, run
`CGO_ENABLED=0 go install ./cmd/fmn`. Run `fmn doctor` and then set up Fish,
Zsh, or Bash as shown in the [README](../README.md#install).

The 15.0 VM uses base-system tools to check the checksum, static linkage,
version, and SQLite capture. The 15.1 VM runs the CLI and terminal suite with
Fish, Zsh, and Bash. Both architectures must pass before a tag run publishes.
A manual Release workflow run exercises those checks without a tag.
