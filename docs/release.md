# Tonic 0.1.0-beta.3

Beta Elixir-to-native compiler with LLVM code generation, a moving
garbage collector, and a concurrent process runtime. Supports `.ex` modules,
`.exs` scripts, fetched Mix/Hex dependency sources, and the Mix tasks
`mix tonic build`, `mix tonic run`, and `mix tonic check`.

This release is intended for scripts and small applications. Compatibility is
limited to the language and runtime APIs Tonic implements. See
[compatibility.md](compatibility.md) for packages and supervision scenarios
verified by the release tests. Distributed BEAM nodes, hot code loading, port
drivers, arbitrary NIFs, custom dependency build systems, and umbrella root builds are
outside the current support boundary.

## What changed in 0.1.0-beta.3

- TLS: `:ssl` client and server on rustls (TLS 1.2 and 1.3), with
  verification against the system trust store, ALPN, client certificates and
  the `:public_key` functions HTTP clients use. Mint builds natively and
  fetches over HTTPS (HTTP/2).
- New runtime APIs: `:gen_udp`, ports to external programs (`Port.open/2`;
  `System.cmd/3`, `System.shell/2` and `:os.cmd/1,2` now run on ports and no
  longer block a scheduler thread), `:zlib` (byte-identical to OTP), and
  `:inet.getaddr/2`, `getaddrs/2`, `gethostbyname/1,2`, `gethostname/0`.
- OTP-exact values: `:erlang.phash2/1,2`, `URI` and `:uri_string` parsing
  (OTP's own `uri_string`), and `:calendar` RFC 3339 conversion.
- Deep recursion no longer ends the program: process stacks grow in segments,
  so body recursion (such as `Enum.map/2` over millions of elements in a Task)
  is bounded by memory, as on the BEAM.
- Compiler: remote macros require `require`, `import only:`/`except:` filter
  macros, `@doc`/`@moduledoc` and `Module.put_attribute/3` values are
  readable, `Macro.expand/2` expands Kernel and imported macros, and quoted
  aliases keep their lexical alias.
- Elixir's unused variable, ungrouped clause and duplicate map key warnings,
  and more invalid programs rejected with Elixir's errors.
- Errors: stack traces carry lines for failed matches, `case` and `cond`; the
  main process killed by an exit signal prints Elixir's `** (EXIT from ...)`
  banner; FunctionClauseError output matches Elixir's layout.
- Smaller executables: hello world is about 4.5 MB.

## What changed in 0.1.0-beta.2

- Generational garbage collection, as on the BEAM: a small young heap with
  inline bump allocation, promotion after two survivals, and major collections
  only when the old heap is full. A process holding large long-lived state no
  longer copies it on every collection; peak memory for large live data drops
  by about a third.
- Smaller executables: only the standard library functions a program can reach
  are linked, and symbols are stripped. A hello world is about 4.5 MB instead
  of 11.5 MB. `TONIC_KEEP_ALL=1` keeps everything for programs that build
  function names at run time.
- Processes start with a 233-word heap (was 2048): 100,000 idle processes use
  about 40% less memory.
- Fixed races that could drop the first Logger report or return unset
  application settings when several processes started at once.

## Stability in the beta

The first public release (0.1.0-beta.1) followed a differential bug hunt against Elixir
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

* `tonic0.1.0-beta.3.linuxx64.tar.gz`: Linux x86-64, built on Ubuntu 24.04; glibc 2.39
  or newer is the supported baseline.
* `tonic0.1.0-beta.3.macosarm64.tar.gz`: Apple Silicon macOS, built on macOS 14.

Compilation requires clang/LLVM 18 or newer. Mix project loading and archive
tasks additionally require Elixir 1.18 / OTP 27; native programs run without
Elixir/OTP. PCRE2 is included in the static runtime library.

Verify the matching `.tar.sha256` file before extracting (Linux: `sha256sum -c`;
macOS: `shasum -a 256 -c`). Then:

```sh
tar -xzf tonic0.1.0-beta.3.macosarm64.tar.gz
cd tonic0.1.0-beta.3
bash install.sh "$HOME/.local"
export PATH="$HOME/.local/bin:$PATH"
tonic run hello.exs
```

Use the Linux filename on Linux. Install clang 18 with your distribution's
package manager, or LLVM 18 on macOS. Set `TONIC_CC` to its clang executable if
the default `clang` is older. When Mix is installed, `install.sh` also installs
the Mix archive, so `mix tonic build|run|check` work right away; set
`TONIC_NO_MIX=1` to skip it, or install it later with
`mix archive.install "$HOME/.local/share/tonic/tonic.ez" --force`. A
user-specified prefix also works; keep `bin/` and `lib/` together. Install
subsequent versions into the same prefix to upgrade the compiler, its runtime
and the Mix tasks together.

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
a `v0.1.0-beta.3` tag. A manual workflow run verifies and uploads build artifacts
without publishing a release. Tag and manifest versions must match.
