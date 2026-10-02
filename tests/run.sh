#!/usr/bin/env bash
# Compare fixtures, or regenerate selected fixtures with reference Elixir.
set -eu
cd "$(dirname "$0")/.."
mode=fixtures
if [ "${1:-}" = "--bless" ]; then
  mode=bless
  shift
fi
exec python3 tools/harness.py "$mode" "$@"
