#!/usr/bin/env bash
set -eu
cd "$(dirname "$0")/.."
mix archive.build -o tonic.ez
exec elixir tests/tests.exs
