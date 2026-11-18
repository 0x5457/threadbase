#!/bin/sh
set -eu

BASE="${THREADBASE_RELEASE_BASE:-https://github.com/0x5457/threadbase/releases/latest/download}"
BIN_DIR="${THREADBASE_BIN_DIR:-$HOME/.local/bin}"

os="$(uname -s)"
arch="$(uname -m)"
case "$os/$arch" in
  Darwin/arm64) target="aarch64-apple-darwin" ;;
  Darwin/x86_64) target="x86_64-apple-darwin" ;;
  Linux/x86_64) target="x86_64-unknown-linux-gnu" ;;
  Linux/aarch64|Linux/arm64) target="aarch64-unknown-linux-gnu" ;;
  *) echo "threadbase: unsupported platform $os/$arch" >&2; exit 1 ;;
esac

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
url="$BASE/threadbase-$target"
echo "Downloading $url"
curl -fsSL "$url" -o "$tmp/threadbase"
curl -fsSL "$url.sha256" -o "$tmp/threadbase.sha256"
expected="$(awk '{print $1}' "$tmp/threadbase.sha256")"
if command -v sha256sum >/dev/null 2>&1; then
  actual="$(sha256sum "$tmp/threadbase" | awk '{print $1}')"
else
  actual="$(shasum -a 256 "$tmp/threadbase" | awk '{print $1}')"
fi
if [ "$expected" != "$actual" ]; then
  echo "threadbase: checksum mismatch" >&2
  exit 1
fi
chmod 755 "$tmp/threadbase"
mkdir -p "$BIN_DIR"
mv "$tmp/threadbase" "$BIN_DIR/threadbase"
echo "Installed $BIN_DIR/threadbase"
echo "Add $BIN_DIR to PATH if needed, then run: threadbase"
