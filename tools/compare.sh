#!/usr/bin/env bash
# Compare selected scripts (or all fixtures) with live reference Elixir.
set -eu
cd "$(dirname "$0")/.."
exec python3 tools/harness.py compare "$@"
