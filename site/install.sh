#!/bin/sh
# Install the latest Tonic release bundle:
#   curl -fsSL https://wess.io/tonic/install.sh | sh
# TONIC_VERSION=0.1.0-beta.3 pins a version, TONIC_PREFIX picks the
# installation prefix (default ~/.local), TONIC_NO_MIX=1 skips the Mix tasks.
set -eu

repo=wess/tonic
prefix="${TONIC_PREFIX:-$HOME/.local}"

fail() {
  echo "tonic: $*" >&2
  exit 1
}

case "$(uname -s)/$(uname -m)" in
  Darwin/arm64) platform=macosarm64 ;;
  Linux/x86_64 | Linux/amd64) platform=linuxx64 ;;
  *) fail "no release bundle for $(uname -s) $(uname -m); bundles exist for macOS arm64 and Linux x86-64" ;;
esac

command -v curl > /dev/null 2>&1 || fail "curl is required"
command -v bash > /dev/null 2>&1 || fail "bash is required"

version="${TONIC_VERSION:-}"
if [ -z "$version" ]; then
  # every release so far is a prerelease, which releases/latest skips
  version="$(curl -fsSL "https://api.github.com/repos/$repo/releases?per_page=1" |
    sed -n 's/.*"tag_name": *"v\([^"]*\)".*/\1/p' | head -n 1)"
  [ -n "$version" ] || fail "could not find the latest release"
fi
version="${version#v}"

name="tonic$version.$platform"
base="https://github.com/$repo/releases/download/v$version"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT INT TERM

echo "Downloading Tonic $version for $platform"
curl -fsSL -o "$work/$name.tar.gz" "$base/$name.tar.gz" || fail "download failed: $base/$name.tar.gz"
curl -fsSL -o "$work/$name.tar.sha256" "$base/$name.tar.sha256" || fail "download failed: $base/$name.tar.sha256"

expected="$(cut -d ' ' -f 1 < "$work/$name.tar.sha256")"
if command -v sha256sum > /dev/null 2>&1; then
  actual="$(sha256sum "$work/$name.tar.gz" | cut -d ' ' -f 1)"
else
  actual="$(shasum -a 256 "$work/$name.tar.gz" | cut -d ' ' -f 1)"
fi
[ "$expected" = "$actual" ] || fail "checksum mismatch for $name.tar.gz"

tar -xzf "$work/$name.tar.gz" -C "$work"
bash "$work/tonic$version/install.sh" "$prefix"

if ! command -v "${TONIC_CC:-clang}" > /dev/null 2>&1; then
  echo "Tonic needs clang to compile. Install the Xcode Command Line Tools (macOS) or clang 18+ (Linux)." >&2
fi
