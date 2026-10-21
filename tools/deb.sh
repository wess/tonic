#!/usr/bin/env bash
# Repackage a Linux x86-64 release bundle as a Debian package.
# usage: tools/deb.sh dist/tonic<version>.linuxx64.tar.gz [output directory]
set -euo pipefail

bundle="$1"
out="${2:-$(dirname "$bundle")}"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

tar -xzf "$bundle" -C "$work"
src="$(find "$work" -mindepth 1 -maxdepth 1 -type d -name 'tonic*')"
version="$(sed -n 's/.*"version": *"\([^"]*\)".*/\1/p' "$src/manifest.json" | head -n 1)"
[ -n "$version" ] || { echo "no version in $src/manifest.json" >&2; exit 1; }
# 0.1.0-beta.3 -> 0.1.0~beta.3 so prereleases sort before the final release
debversion="${version/-/\~}"

root="$work/root"
lib="$root/usr/lib/tonic"
mkdir -p "$lib/bin" "$lib/lib" "$lib/share/tonic" "$root/usr/bin" "$root/usr/share/doc/tonic" "$root/DEBIAN"
install -m 755 "$src/bin/tonic" "$lib/bin/tonic"
install -m 644 "$src/lib/libtonic_rt.a" "$lib/lib/libtonic_rt.a"
install -m 644 "$src/share/tonic/tonic.ez" "$lib/share/tonic/tonic.ez"
# the compiler finds ../lib/libtonic_rt.a from its resolved path
ln -s ../lib/tonic/bin/tonic "$root/usr/bin/tonic"
install -m 644 "$src/license" "$root/usr/share/doc/tonic/copyright"
install -m 644 "$src/notice" "$src/README.md" "$src/hello.exs" "$root/usr/share/doc/tonic/"
cp -R "$src/licenses" "$src/docs" "$root/usr/share/doc/tonic/"

size="$(du -sk "$root/usr" | cut -f 1)"
cat > "$root/DEBIAN/control" <<EOF
Package: tonic
Version: $debversion
Architecture: amd64
Maintainer: Wess Cope <me@wess.io>
Installed-Size: $size
Depends: libc6 (>= 2.39), clang (>= 1:18)
Section: devel
Priority: optional
Homepage: https://wess.io/tonic/
Description: Elixir-to-native compiler
 Tonic compiles Elixir scripts and Mix projects to native executables via
 LLVM, with a moving garbage collector and an Erlang-style process runtime.
 The Mix tasks archive is in /usr/lib/tonic/share/tonic/tonic.ez.
EOF

mkdir -p "$out"
deb="$out/tonic$version.linuxx64.deb"
dpkg-deb --root-owner-group --build "$root" "$deb" > /dev/null
(cd "$out" && sha256sum "$(basename "$deb")" > "$(basename "$deb").sha256")
echo "$deb"
