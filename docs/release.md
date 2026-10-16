# Tonic 0.1.0-beta.1

Beta Elixir-to-native compiler with LLVM code generation, a moving
garbage collector, and a concurrent process runtime. Supports `.ex` modules,
`.exs` scripts, fetched Mix/Hex dependency sources, and `mix tonic.build`,
`mix tonic.run`, and `mix tonic.check`.

This release is intended for scripts and small applications. Compatibility is
limited to the language and runtime APIs Tonic implements. See
[compatibility.md](compatibility.md) for packages and supervision scenarios
verified by the release tests. Distributed BEAM nodes, hot code loading, ports,
arbitrary NIFs, custom dependency build systems, and umbrella root builds are
outside the current support boundary.

## Stability in this beta

This first public release follows a differential bug hunt against Elixir
1.18.3 / OTP 27 across the core language, standard library, processes and OTP,
the CLI, and Mix integration. Fixes include:

- Runtime crashes: `:ets.update_counter` (and `:counters`/`:atomics`) under
  garbage collection, `Process.list/0`, stale arguments in BIF error frames,
  `:erlang.float/1` on huge integers, invalid radixes, and unbounded memory in
  `bsl` (now `SystemLimitError` as on the BEAM).
- Processes: atomic `:atomics.compare_exchange/4`, cancelled timers never
  deliver, exit signals are visible to the sender immediately, monitors taken
  by name report `{name, node}`, ETS `:protected`/`:private` access rights,
  chunked `:ets.select/3` and `match/3`, and the rest of `:timer`.
- Output and signals: piped output appears promptly and survives `SIGTERM`;
  `tonic run` and `mix tonic.run` pass on signals, stdin and exit status and
  leave no programs or temporary files behind; scripts exiting with
  `:normal`/`:shutdown`/`{:shutdown, n}` exit quietly with Elixir's status.
- Language: accumulated module attributes in function bodies, `var!` in
  macro-generated definitions, captured guards and macros (`&is_digit/1`),
  `Record` keyword conversion and `is_record`, functions defined by any
  module-level call (`Enum.reduce`), macro and multi-letter sigils, binary generators, Erlang map comprehensions, `-0.0`,
  float printing and `float_to_binary(decimals:)` matching OTP exactly, and
  linear-time binary appends.
- Compiler robustness: large expressions and deeply nested literals compile,
  clear errors for unknown commands and options, and `-o` never overwrites a
  source file. Defining a module twice is now a compile error.

Remaining differences are listed in [compatibility.md](compatibility.md#known-differences).

## Install

Download the bundle matching your machine from the release assets:

* `tonic0.1.0-beta.1.linuxx64.tar.gz`: Linux x86-64, built on Ubuntu 24.04; glibc 2.39
  or newer is the supported baseline.
* `tonic0.1.0-beta.1.macosarm64.tar.gz`: Apple Silicon macOS, built on macOS 14.

Compilation requires clang/LLVM 18 or newer. Mix project loading and archive
tasks additionally require Elixir 1.18 / OTP 27; native programs run without
Elixir/OTP. PCRE2 is included in the static runtime library.

Verify the matching `.tar.sha256` file before extracting (Linux: `sha256sum -c`;
macOS: `shasum -a 256 -c`). Then:

```sh
tar -xzf tonic0.1.0-beta.1.macosarm64.tar.gz
cd tonic0.1.0-beta.1
bash install.sh "$HOME/.local"
export PATH="$HOME/.local/bin:$PATH"
tonic run hello.exs
mix archive.install "$HOME/.local/share/tonic/tonic.ez" --force
```

Use the Linux filename on Linux. Install clang 18 with your distribution's
package manager, or LLVM 18 on macOS. Set `TONIC_CC` to its clang executable if
the default `clang` is older. The archive install is optional for standalone
scripts. A user-specified prefix also works; keep `bin/` and `lib/` together.
Install subsequent versions into the same prefix to upgrade both the compiler
and its runtime, then reinstall the Mix archive.

## Build a release from source

Use Python 3.12+ and install Rust's standard-library attribution documents with
`rustup component add rust-docs`. The packaging command fetches the complete
locked Cargo graph, then generates notices from the authenticated crate
archives and the active Rust toolchain. `python3 tools/notices.py --check`
verifies the generated attribution against those inputs.

To package with an existing same-version Mix archive and cached Cargo packages,
use `python3 tools/release.py --archive mix/tonic.ez --offline`. This checks the
archive's version and embedded legal documents; its compiled task code remains
the responsibility of the archive's build. CI always rebuilds the archive and
runs the full installation check.

```sh
python3 tools/release.py
python3 tests/release.py dist/*.tar.gz
```

The bundle contains the compiler, static runtime, Mix archive, installer,
documentation, license, and third-party notices. Its manifest records file
SHA-256 hashes, and each compressed archive has a separate checksum file.
Archive ownership and timestamps are normalized; `SOURCE_DATE_EPOCH` can set
the archive timestamp. This does not claim reproducible machine code across
different toolchain versions.

The GitHub release workflow first requires the full Linux/macOS CI suite,
builds and verifies both installation bundles, then publishes a prerelease on
a `v0.1.0-beta.1` tag. A manual workflow run verifies and uploads build artifacts
without publishing a release. Tag and manifest versions must match.
