#!/usr/bin/env bash
set -eu
bundle="$(cd "$(dirname "$0")" && pwd)"
prefix="${1:-${HOME}/.local}"
if [ "$#" -gt 1 ]; then
  echo 'usage: bash install.sh [installation prefix]' >&2
  exit 2
fi
if [ ! -f "$bundle/bin/tonic" ] || [ ! -f "$bundle/lib/libtonic_rt.a" ]; then
  echo 'run install.sh from an extracted Tonic release bundle' >&2
  exit 1
fi
mkdir -p "$prefix/bin" "$prefix/lib" "$prefix/share/tonic"
cp "$bundle/bin/tonic" "$prefix/bin/tonic"
chmod 755 "$prefix/bin/tonic"
cp "$bundle/lib/libtonic_rt.a" "$prefix/lib/libtonic_rt.a"
cp "$bundle/share/tonic/tonic.ez" "$prefix/share/tonic/tonic.ez"
cp "$bundle/license" "$bundle/notice" "$bundle/manifest.json" "$prefix/share/tonic/"
cp -R "$bundle/licenses" "$bundle/docs" "$prefix/share/tonic/"
"$prefix/bin/tonic" --version
printf 'Installed in %s\nAdd %s/bin to PATH.\n' "$prefix" "$prefix"
printf 'For Mix tasks: mix archive.install "%s/share/tonic/tonic.ez" --force\n' "$prefix"
