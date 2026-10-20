# Compatibility

Tonic 0.1.0-beta.2 compiles supported Elixir and Erlang source to LLVM and links a native runtime. `.exs` scripts use the same compiler. Native executables do not contain BEAM bytecode.

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

`:gen_tcp` provides TCP listen, accept, connect, send, recv, close, shutdown and controlling process, in passive, `active: :once`, `active: true` and `active: N` modes. `:inet` covers `setopts`, `getopts`, `sockname`, `peername`, `getstat`, `port`, address parsing and formatting. Waiting socket operations run on the runtime's I/O driver and do not block scheduler threads. Only `:raw` packets are supported; `:erlang.decode_packet/3` handles raw, length-prefixed, line and HTTP packets.

`:gen_udp` provides `open/1,2`, `send/3,4` (address tuples or host names), `recv/2,3`, `close/1` and `controlling_process/2`, in passive, `active: :once`, `active: true` and `active: N` modes with `{:udp, socket, ip, port, data}` and `{:udp_passive, socket}` messages, `:list` or `:binary` delivery, and `:inet`/`:inet6`, `:ip`, `:reuseaddr`, `:reuseport`, `:broadcast`, `:recbuf` and `:sndbuf` options. `:inet.setopts/2`, `getopts/2`, `sockname/1`, `port/1`, `getstat/1` and `close/1` accept UDP sockets. SSL is not available.

## Ports

`Port.open/2` and `:erlang.open_port/2` start external programs with `{:spawn, command}` (run as `/bin/sh -c "exec command"`, as OTP does) and `{:spawn_executable, path}`. Supported options are `:binary`, `:exit_status`, `:eof`, `{:args, list}`, `{:arg0, name}`, `{:cd, dir}`, `{:env, list}`, `:stderr_to_stdout`, `:use_stdio`, `:nouse_stdio` (the program uses fds 3 and 4), `{:line, n}`, `{:packet, 1 | 2 | 4}`, `:stream`, `:in`, `:out`, `:hide` and `{:parallelism, bool}`. The owner receives `{port, {:data, data}}` (`{:eol, line}` and `{:noeol, part}` in line mode), `{port, {:exit_status, status}}`, `{port, :eof}`, `{port, :closed}` and `{port, :connected}` in OTP's order. `Port.command/2,3`, `send(port, {pid, {:command | :connect, _}})`, `send(port, {pid, :close})`, `Port.close/1`, `Port.connect/2`, `Port.info/1,2`, `Port.list/0`, `Port.monitor/1`, links, `exit(port, reason)` and `badsig` exit signals follow OTP 27; a port closes with its connected process. Ports are their own term type: `is_port/1`, printing as `#Port<0.N>` and term order match the BEAM, and ports survive `term_to_binary/1` round trips. Program output is read on the runtime's I/O driver and never blocks a scheduler thread. `System.cmd/3`, `System.shell/2` and `:os.cmd/1,2` are Elixir's and OTP's own implementations over ports.

`Port.list/0` omits the BEAM's own stdio ports (`#Port<0.0>` and `#Port<0.1>`), `{:spawn_driver, _}` and `{:fd, _, _}` are not supported, and `Port.info/2` reports an approximate `:memory` and a `:id` equal to the port number. `spawn_executable` of a directory fails with `:eacces` instead of opening a port that exits at once.

Distributed BEAM nodes, BEAM bytecode loading and full compatibility with undocumented OTP internals are outside this release's supported surface. Passing the source and runtime tests does not establish those capabilities.

## Known differences

These differ from Elixir 1.18 / OTP 27 in this release. Each is either intentional or tracked in TODO.md.

- Redefining a module (a second `defmodule` with the same name) is a compile error; Elixir replaces the module at run time.
- `:erlang.phash/2` and `:erlang.phash2/1,2` match OTP 27 for every term except pids, references and anonymous or local funs. Pids and references hash their printed numbers, which differ from the BEAM's; anonymous funs cannot reproduce OTP's per-module code checksum. External funs (`&Mod.fun/arity`) match.
- `Macro.expand/2` and `Macro.expand_once/2` expand aliases, user macros (remote, and imported ones when given `__ENV__`) and the Kernel macros `if`, `unless`, `&&`, `||`, `!`, `|>`, `is_nil`, `then`, `tap`, `to_string`, `to_charlist` and `match?`. Other Kernel macros and `defguard` guards are returned unexpanded.
- `:calendar.local_time_to_universal_time/1` applies the current UTC offset rather than the offset in effect at the given local time, so results differ across a daylight saving change.
- Unused variable warnings are emitted for code you write, but not for variables inside code a dependency's macro generates (for example the body of a `Plug.Router` route). Warnings for ungrouped clauses and duplicate map keys are not emitted, and some invalid programs Elixir rejects are still accepted.
- Stack trace frames for anonymous functions inside `with ... else` and other inlined code are named after the enclosing function, where the BEAM names a generated function such as `-w/1-fun-0-`.
- A `quote` of a sigil omits the `delimiter:` metadata.
- `:atomics` and `:counters` arrays are never freed.
- `:zlib` streams that are never closed are not freed, and closing a stream twice raises `:badarg` rather than `:not_initialized`. Compressed bytes come from stock zlib 1.3.2 and match an OTP built against stock zlib; an OTP linked to a different zlib (such as zlib-ng) can produce different bytes, though each decompresses the other's output.
- Sockets are references rather than ports, so they print as `#Reference<...>` and `is_port/1` is false for them.
- `:gen_udp.connect/2,3`, multicast group membership (`:add_membership`) and ancillary data (`:recvtos`, `:recvttl`) are not implemented; `:multicast_ttl`, `:multicast_loop`, `:recvtos` and similar options are accepted and ignored. A receive error in UDP active mode delivers `{:udp_error, socket, reason}` and stops delivery until `:inet.setopts/2` re-enables it.
- `:re` with `[:unicode]` but without `:ucp` matches `\w` and similar classes against ASCII only; use `~r/.../u` (which sets `:ucp`) for Unicode word characters.
- Converting a very large integer to a float rounds correctly (IEEE 754); the BEAM can differ in the last digit.
- `Code.string_to_quoted/1` error locations report line 0 without a column, and a compiled regex's `re_pattern` is not a real PCRE binary.
- Small maps with atom keys print keys in source order rather than atom-table order.
- Memory: each blocked process holds at least one page of its stack, so a process costs about 20 KB on Apple Silicon (16 KB pages) and about 7 KB on Linux (4 KB pages), compared to about 3 KB on the BEAM.
- Executables include only the standard library functions the compiler can see a program reach. A call whose function name is built at run time (`apply(Mod, String.to_atom(name), args)`) into a standard library function the program does not otherwise use fails with `UndefinedFunctionError`; compile with `TONIC_KEEP_ALL=1` to keep every function.
- `mix test` with `compilers: [:tonic]` cannot start the application, since no BEAM files are produced; run tests with the default compilers.

