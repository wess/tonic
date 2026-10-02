#!/usr/bin/env bash
# Import modules from the Elixir sources (docs/specs stripped, line numbers
# preserved) into lib/ex/, then re-apply tonic's local patches.
#   tools/import_elixir.sh /path/to/elixir/lib/elixir/lib
set -e
src=$1
here="$(cd "$(dirname "$0")/.." && pwd)"
while read -r out rel; do
  [ -z "$out" ] && continue
  python3 "$here/tools/strip_docs.py" "$here/lib/ex/$out" "$src/$rel" "$rel"
done < "$here/tools/imports.txt"
python3 "$here/tools/patch_imports.py" "$here/lib/ex"
