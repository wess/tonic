# tonic

**0.0.1 · experimental.** Start with scripts and small native applications.
Visit the [Tonic website](https://wess.io/tonic/).
See the [release guide](docs/release.md) for installation and the
[compatibility report](docs/compatibility.md) for tested packages and limitations.
Remaining work is tracked in [TODO.md](TODO.md).

An ahead-of-time compiler for **Elixir** that produces native executables via
**LLVM**, with a precise, moving garbage collector and an Erlang-style process
runtime built on **tokio**.

```
$ cat hello.exs
defmodule Greeter do
  def greet(name), do: "Hello, #{name}!"
end

IO.puts(Greeter.greet("world"))

$ tonic run hello.exs
Hello, world!

$ tonic build hello.exs -o hello && ./hello
Hello, world!
```

The goal is to be *as close to Elixir as possible*: same syntax, same
semantics, same standard library behaviour — down to `IO.inspect` pretty
printing, float formatting and exception messages. The test suite diffs tonic's
output against the reference Elixir implementation.

## Building

Requirements: Rust (1.88+) and clang/LLVM (18+) on your `PATH`.

```
cargo build --release
./target/release/tonic run examples/hello.exs
```

`target/release/tonic` finds its runtime (`libtonic_rt.a`) next to itself.
Override with `TONIC_RUNTIME=/path/to/libtonic_rt.a` and the compiler/linker
with `TONIC_CC=clang-18`.

Release bundles include the compiler, runtime library, and Mix archive. Build
and verify a bundle with Python 3.12+, installed Mix, and the Rust/LLVM toolchain:

```sh
python3 tools/release.py
python3 tests/release.py dist/*.tar.gz
```

## Usage

```
tonic run <file.exs>... [-- args]    compile (-O0 by default) and run
tonic build <file.ex>... [-o out]    compile to an executable (-O2 by default)
tonic emit-llvm <file.ex>... [-o x.ll]
tonic check <file.ex>...             parse + expand only
```

Multiple files are compiled together as one program; top-level expressions in
the files run in order, like `elixir script.exs`.

### Mix and Hex

Tonic uses installed Mix to evaluate `mix.exs` and resolve the fetched dependency
graph. Keep using the usual `mix.exs`, `mix.lock`, and `deps/` conventions:

```sh
mix deps.get
mix deps.update decimal
mix hex.outdated
tonic build . -o program
tonic run . -- argument
```

Mix and Hex handle package resolution, fetching, checksums, and locks. Tonic
compiles dependency sources together with your application's sources. It never
links BEAM files into the executable. Unfetched, conflicting, or stale locked
dependencies fail with a diagnostic; Tonic does not silently skip them or fetch
packages during compilation. Hex, Git, and path dependencies use Mix's resolved
graph, including transitive dependencies, `only`, `targets`, and overrides.

Elixir/OTP and Mix are required on the development machine for project loading.
Set `TONIC_MIX` to select the Mix executable. Standalone `.ex` / `.exs` compilation
and the resulting native executable do not require Mix or BEAM.

Computed project metadata and configured `elixirc_paths` / `erlc_paths` are
supported. Compile-time configuration is evaluated by Mix's `Config.Reader`,
including imports and environment/target selection. Its values are embedded in
the executable. `config/runtime.exs` executes in the native program before
applications start, so runtime environment lookups remain runtime lookups.
Application specifications, startup modules, and runtime dependency edges are
preserved. A dependency with `runtime: false` is compiled but is not automatically
started. An escript `main_module` receives command-line arguments; an application
with a startup module and no escript entry keeps running.

### Mix tasks

Build and install the archive from this checkout:

```sh
cd mix
mix archive.build -o tonic.ez
mix archive.install ./tonic.ez
```

With `tonic` on `PATH`, use these in your application directory:

```sh
mix deps.get
mix tonic.check
mix tonic.build
mix tonic.run -- one two
mix tonic.build --deps-get -o ./program
```

`mix tonic.build` defaults to `_build/<env>/tonic/<app>`. Set `TONIC` to the native
compiler executable or configure `tonic: [executable: "/path/to/tonic"]` in
`project/0`. Ordinary Mix dependency, Hex publishing, and package-management
tasks continue to use Mix.

To make `mix compile` build the native executable, opt into the archive compiler:

```elixir
def project do
  [
    app: :my_app,
    version: "0.1.0",
    compilers: [:tonic],
    prune_code_paths: false,
    consolidate_protocols: false
  ]
end
```

`prune_code_paths: false` keeps the installed archive's compiler task available.
Set `consolidate_protocols: false` to disable Mix’s separate BEAM protocol
consolidation. The Tonic compiler rebuilds on each invocation so evaluated
configuration and toolchain changes cannot leave a stale native artifact.
It produces a native artifact; it does not produce application
BEAM files for `mix run` or `mix release`. Use `mix tonic.run` to run it.

The locked example in `examples/packages/` uses Decimal from Hex:

```sh
cd examples/packages
mix deps.get
mix tonic.run -- 1.25 2.50
# 3.75
mix tonic.build
./_build/dev/tonic/packages 10.5 2.25
# 12.75
```

Fetching a package does not establish native compatibility. Packages must use
language and runtime APIs implemented by Tonic. Unsupported custom compilers,
NIF libraries, non-Mix dependency build managers, and lexer/parser generation
fail explicitly. Umbrella applications must be built from each child project.

## Architecture

```
 .ex/.exs ──► lexer ──► parser ──► collect ──► expand ──► codegen ──► clang ──► native
              (tokens)   (AST)    (modules,    (Core IR:   (LLVM IR)    │
                                  protocols,   patterns,               ▼
                                  structs)     closures)          libtonic_rt.a
```

* **compiler/** (Rust) — Elixir lexer and Pratt parser (no-paren calls,
  do-blocks, stab clauses, sigils, heredocs, interpolation), module collection
  (aliases, imports, attributes, `defstruct`, `defexception`, `defprotocol`,
  `defimpl`, `defguard`, `defdelegate`, `use`, and `defmacro`),
  expansion into a small Core IR, and LLVM IR generation. The standard library
  in `lib/` is compiled together with the program (whole-program, with
  reachability pruning). The bundled library is compiled to a cached object;
  user definitions and protocol dispatch are generated for each program.
  Protocols are consolidated for `.ex` programs, while `.exs` programs retain
  script-style protocol metadata.

* **runtime/** (Rust, static library) — term representation, the garbage
  collector, bignums, persistent maps, binaries/strings/unicode, regexes, the
  process scheduler and all native built-in functions (`:erlang`, `:maps`,
  `:lists`, `:binary`, …).

* **lib/** (Elixir) — `Kernel`, `Enum`, `Stream`, `List`, `Map`, `MapSet`,
  `Keyword`, `String`, `Integer`, `Float`, `Range`, `Access`, `Inspect`
  (a port of Elixir's `Inspect.Algebra`), `String.Chars`, `Enumerable`,
  `Collectable`, `Process`, `GenServer`, `Agent`, `Task`, `Supervisor`,
  `DynamicSupervisor`, `Regex`, `File`, `Path`, `System`, `IO`, exceptions …

### Values

64-bit tagged words: 63-bit small integers, atoms, pids and `[]` are
immediates; everything else is a pointer to a boxed heap object (tuple, cons,
float, bignum, binary, sub-binary, map, closure, reference). Integers overflow
transparently into bignums. Maps use sorted flat storage for up to 32 keys and
a persistent hash array mapped trie for larger maps. Atom ordering emulates an
Elixir 1.18 / OTP 27 boot table; it does not guarantee the same iteration order
as every running BEAM VM. Sort entries when order matters.

### Garbage collection

Each process owns a private semispace heap collected by a **precise Cheney
copying collector**. Compiled code keeps every live value in a *shadow-stack
frame* (`[prev, n, function_info, line, slot…]` allocated in the native frame
and linked from the process context), so the collector knows every root exactly and can move
objects. Immediates may live in registers since they never move. Because heaps
are per process and messages are copied on send (like the BEAM), collection
never stops other processes.

### Processes on tokio

Every Elixir process is a stackful coroutine driven by a **tokio task** on the
multi-threaded runtime. When a process has to wait — `receive` on an empty
mailbox, reduction budget exhausted, `after` timeout — it suspends back into
its task, which `.await`s the matching tokio primitive (`Notify` for mail,
`sleep_until` for timeouts, `yield_now` for fairness). Processes therefore run
in parallel across cores with preemptive-feeling fairness, and `GenServer`,
`Agent`, `Task` and `Supervisor` (written in Elixir on top of
`spawn`/`send`/`receive`/links/monitors) get all of that for free.
`TONIC_SCHEDULERS` sets the number of worker threads.

### Calls, tail calls and exceptions

Functions use LLVM's `tailcc` convention and every call in tail position is a
guaranteed tail call, so recursive loops run in constant stack. Exceptions are
values: a raising function stores `{kind, reason}` in the process context and
returns 0; callers branch to the innermost `try` handler or return 0 themselves
(no unwinding tables, no setjmp).

## What's supported

Most of the everyday language: modules, multi-clause functions with guards and
default arguments, pattern matching everywhere (including binaries and map/struct
patterns, pins), `case`/`cond`/`if`/`unless`/`with`/`for` (generators, filters,
`into:`, `reduce:`, `uniq:`, bitstring generators), `try`/`rescue`/`catch`/
`else`/`after`, `receive`/`after`, anonymous functions and captures, pipes,
structs, protocols with `@fallback_to_any` and `@derive`, exceptions,
comprehensions, sigils (`~s ~S ~c ~w ~r`), binaries and bitstrings with
bit-sized segments, processes, links, monitors, registered names, timers, ETS,
and the OTP-style abstractions above. Regexes use PCRE2, including
backreferences and lookaround. Exceptions capture source-aware stack traces.

Template macros expand in the compiler. Macros that compute their expansion
run in a cached native macro host using `Tonic.Eval`; no external Elixir or
BEAM runtime is required to run a script or expand its macros.

User modules support `defoverridable` and calls to the previous implementation
with `super(...)`, including defaults and successive overrides. Compilation
hooks support `@on_definition` and `@before_compile` for inspecting definitions,
updating attributes, and injecting code. `@after_compile` runs after LLVM IR is
generated; its second argument contains the program's LLVM IR as a binary,
rather than BEAM bytecode. Interpreted modules created through `Code.eval_*`
have no emitted artifact and pass an empty binary. Compile-time output is
forwarded to stderr so it cannot corrupt the macro-host protocol or script
stdout.

`tonic check` parses and expands without generating LLVM IR, so it runs
definition and before-compilation hooks but does not run after-compilation hooks.

### Limitations

* Compile-time execution uses Tonic's evaluator and bundled library, so it is
  limited to the language and APIs implemented here. It is not a BEAM VM.
* No BEAM hot code loading, distributed nodes, ports, or arbitrary NIF loading.
  Process and system introspection cover a subset of Erlang's APIs.
* Mix project loading requires installed Mix. Native compilation supports
  fetched sources, not arbitrary BEAM dependencies or custom package build tasks.
* Map iteration order and anonymous-function identifiers can differ from
  reference Elixir. Neither is a portable ordering or identity contract.
* PCRE2 and OTP 27's PCRE engine can differ in accepted patterns, compile-error
  wording, and offsets. Common parenthesis and quantifier errors use OTP 27
  wording; this does not imply complete regex-engine equivalence.

## Tests

```
cargo build --workspace
cargo test --workspace
cargo build --release --workspace
python3 tests/harness.py
bash mix/tests/run.sh
python3 tests/archive.py
TONIC_TEST_HEX=1 cargo test -p tonic --test packages real_hex_decimal_package_compiles_and_executes -- --ignored
bash tests/run.sh
bash tools/compare.sh
TONIC_GC_STRESS=1 bash tests/run.sh tests/cases/basics.exs tests/cases/binaries.exs
```

`cargo test` covers compiler behavior, the CLI, macro-host framing, and regex
compilation. CLI tests use clang and the built runtime library, so build the
workspace first. The Python harness tests failure handling without requiring
Elixir. Fixture tests compare successful native execution with checked-in
`.out` files; live comparisons require Elixir 1.18.3 / OTP 27.

Package integration tests cover evaluated Mix metadata, transitive path
dependencies, application startup, environment/target filters, and actionable
errors. The ignored Hex test installs Hex into an isolated temporary directory,
fetches a locked Decimal package, and verifies native execution; it requires
network access and is enabled in CI. Mix archive tests install into an isolated
archive directory and leave your global Mix installation untouched.
`tests/archive.py` verifies those tasks against the real native compiler, including
standalone execution, the opt-in compiler, and cleanup.

Both script runners retain stderr, reject unsuccessful execution, and enforce
a timeout for each compiler or reference invocation. Only ExUnit's elapsed-time
summary is normalized. Fixtures sort unordered data explicitly. Set
`TONIC_TEST_TIMEOUT` (seconds, default 60), `TONIC`, or `ELIXIR` to override the
timeout or executable paths. Pass script paths to run a selected subset.

Use `bash tests/run.sh --bless [script.exs ...]` to regenerate expected output
from reference Elixir. A failed or timed-out reference run leaves the existing
fixture intact. Review fixture changes before accepting them.

The CI workflow builds and checks the workspace on Linux and macOS, runs both
fixture and live-reference comparisons, and exercises the moving collector.

The release workflow requires those checks, verifies installed bundles with a
fresh cache, and publishes an experimental GitHub prerelease for a matching
version tag. Manual release workflow runs produce artifacts without publishing.

## Website

The GitHub Pages website is in `site/`. Preview it with:

```sh
python3 -m http.server 8765 --directory site
```

The `pages` workflow publishes changes to `site/` from `main` at
[wess.io/tonic](https://wess.io/tonic/), through your account's GitHub Pages
domain. It can also be run manually.
The mage artwork is original SVG, and the bundled VT323 font includes its license
in `site/assets/fontlicense.txt`.

## License

Tonic is licensed under [Apache-2.0](license). Imported source and bundled
dependency attributions are in [notice](notice) and [licenses/](licenses/).
