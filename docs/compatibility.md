# Compatibility

Tonic 0.0.1 compiles supported Elixir and Erlang source to LLVM and links a native runtime. `.exs` scripts use the same compiler. Native executables do not contain BEAM bytecode.

## Package coverage

The package integration tests fetch exact Hex versions into an isolated project, preserve the checked-in lockfile, and compare native output with Elixir. Package fetching is explicit. Native project loading runs with Hex offline.

| Package | Version | Exercised behavior |
| --- | --- | --- |
| Decimal | 3.0.0 | Decimal construction, addition and string conversion |
| Jason | 1.4.4 | JSON encoding and decoding nested data |
| NimbleOptions | 1.1.1 | Required options, defaults, type validation and rejection |
| DeepMerge | 1.0.0 | Recursive map merge and replacement precedence |

The web example in `examples/helloweb` was built natively and exercised over HTTP (create, list, fetch, validation errors, 404s), matching the same project on the BEAM. It covers Plug 1.20.3, Bandit 1.12.5, ThousandIsland 1.5.0, Ecto 3.14.2, Ecto SQL 3.14.0, ecto_sqlite3 0.25.0, exqlite 0.42.0, DBConnection 2.10.2, Jason 1.4.5 and Telemetry 1.4.2. This is a manual verification, not yet an automated test.

These are specific compatibility checks, not a claim that every function in these packages or every Hex package works. The package fixtures are in `tests/fixtures/packages`; the Decimal example is in `examples/packages`.

Decimal uses the patched 3.0.0 release. Earlier versions below 3.0.0 are affected by [CVE-2026-32686](https://github.com/advisories/GHSA-rhv4-8758-jx7v).

```sh
TONIC_TEST_HEX=1 cargo test -p tonic --test packages -- --ignored
TONIC_TEST_HEX=1 cargo test -p tonic --test packagesextra -- --ignored
```

## Macros and compilation callbacks

User macros can execute compile-time code through the macro host. Coverage includes caller environments, computed expansions, default macro arguments, map splicing, generated definitions, `use`, and chained overridable functions and macros.

`@on_definition` callbacks receive definition metadata and can update attributes seen by subsequent definitions. `@before_compile` callbacks can inject definitions and attributes. Native `@after_compile` callbacks run after LLVM generation; their second argument is the emitted LLVM IR binary. It is not a BEAM module binary. Interpreted `Code.eval_*` modules receive an empty binary because they do not emit a compiled artifact.

## Concurrent applications

The supervised load test runs four producers against a GenServer, checks every result, monitors producer exits, kills the worker, and verifies that its supervisor starts a new worker. Requests retain closures, maps, lists, strings and tuples while moving garbage collection is forced on every allocation.

The ordinary test runs for five seconds per engine. The release gate extends each engine to sixty seconds. A cycle includes sixty validated requests and one automatic worker restart; the test prints completed request and restart counts. No throughput or memory ceiling is implied by this check.

```sh
cargo test -p tonic --test soak -- --nocapture
TONIC_SOAK_SECONDS=60 cargo test -p tonic --test soak -- --nocapture
```

## Supported project boundaries

Mix evaluates project metadata, dependency selection and compile configuration. Coverage includes computed metadata, transitive path dependencies, `only`, `targets`, `runtime: false`, application startup ordering, version checks, lock checks, and runtime configuration that reads the executable's environment.

Tonic accepts the supported standard Mix source compilation pipeline. `rebar3` dependencies are compiled from their `src/*.erl` sources and `.app.src` specification. Mix tasks shipped in dependencies (`lib/mix/`) are not compiled into the program. Other custom dependency compilers, prebuilt native shared libraries and other build managers are rejected with diagnostics. Fetch dependencies with `mix deps.get` or `mix tonic.build --deps-get`; native compilation does not fetch missing packages.

## NIFs

Dependencies built with `elixir_make` whose `c_src/` contains a C NIF (`ERL_NIF_INIT`) are compiled from source against Tonic's `erl_nif.h` and linked into the executable. The `-D` options of the dependency's Makefile `CFLAGS` are applied; compiled objects are cached. A module's `@on_load` callback runs at startup, `:erlang.load_nif/2` binds the linked library, and functions whose body is `:erlang.nif_error/1` call into it.

Tonic implements a subset of the `erl_nif` API: terms, binaries, resources, environments, messages, mutexes and monotonic time. NIF calls run on the calling process's scheduler thread. Resources referenced from terms are never released. Rust NIFs (Rustler), prebuilt `.so`/`.dylib` files and dirty-scheduler semantics are not supported.

## Networking

`:gen_tcp` provides TCP listen, accept, connect, send, recv, close, shutdown and controlling process, in passive, `active: :once`, `active: true` and `active: N` modes. `:inet` covers `setopts`, `getopts`, `sockname`, `peername`, `getstat`, `port`, address parsing and formatting. Waiting socket operations run on the runtime's I/O driver and do not block scheduler threads. Only `:raw` packets are supported; `:erlang.decode_packet/3` handles raw, length-prefixed, line and HTTP packets. SSL, UDP and ports are not available.

Distributed BEAM nodes, BEAM bytecode loading and full compatibility with undocumented OTP internals are outside this release's supported surface. Passing the source and runtime tests does not establish those capabilities.
