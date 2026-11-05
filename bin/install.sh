#!/bin/sh
set -eu

case "$(uname -s)/$(uname -m)" in
    Darwin/arm64) target=aarch64-apple-darwin ;;
    Darwin/x86_64) target=x86_64-apple-darwin ;;
    Linux/aarch64|Linux/arm64) target=aarch64-unknown-linux-musl ;;
    Linux/x86_64) target=x86_64-unknown-linux-musl ;;
    FreeBSD/arm64|FreeBSD/aarch64) target=aarch64-unknown-freebsd ;;
    FreeBSD/amd64) target=x86_64-unknown-freebsd ;;
    *) printf '%s\n' 'Unsupported fmn platform' >&2; exit 1 ;;
esac

base=https://github.com/z0mbix/fmn/releases/latest/download
archive="fmn-$target.tar.xz"
destination=${FMN_INSTALL_DIR:-${CARGO_HOME:-"$HOME/.cargo"}/bin}
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
trap 'exit 1' HUP INT TERM

curl --proto '=https' --tlsv1.2 -fsSL "$base/$archive" -o "$tmp/$archive"
curl --proto '=https' --tlsv1.2 -fsSL "$base/sha256.sum" -o "$tmp/sha256.sum"
expected=$(awk -v archive="$archive" '$2 == archive { print $1 }' "$tmp/sha256.sum")
if command -v sha256sum >/dev/null 2>&1; then
    actual=$(sha256sum "$tmp/$archive" | awk '{print $1}')
elif command -v shasum >/dev/null 2>&1; then
    actual=$(shasum -a 256 "$tmp/$archive" | awk '{print $1}')
else
    actual=$(sha256 -q "$tmp/$archive")
fi
if [ "$actual" != "$expected" ]; then
    printf '%s\n' "Checksum mismatch for $archive" >&2
    exit 1
fi

tar -xOf "$tmp/$archive" "fmn-$target/fmn" > "$tmp/fmn"
test -s "$tmp/fmn"
mkdir -p "$destination"
install -m 755 "$tmp/fmn" "$destination/fmn"
printf 'Installed fmn in %s\n' "$destination"
