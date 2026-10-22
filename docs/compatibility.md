# Compatibility

Tonic 0.1.1-beta.1 development source compiles supported Elixir and Erlang source to LLVM and links a native runtime. `.exs` scripts use the same compiler. Native executables do not contain BEAM bytecode.

This report follows the current development source. The API inventory, Req
coverage, managed native-resource lifetimes, expanded socket semantics, and
native Mix test/release workflows below are changes after the published
0.1.0-beta.3 bundles. Build the current checkout to exercise them.

The [API conformance inventory](coverage.md) compares reference exports with
native declarations and reviewed test evidence. A declaration alone defaults
to partial support; counts do not measure a percentage of Elixir compatibility.

## Package coverage

The package integration tests fetch exact Hex versions into an isolated project, preserve the checked-in lockfile, and compare native output with Elixir. Package fetching is explicit. Native project loading runs with Hex offline.

| Package | Version | Exercised behavior |
| --- | --- | --- |
| Decimal | 3.0.0 | Decimal construction, addition and string conversion |
| Jason | 1.4.4 | JSON encoding and decoding nested data |
| NimbleOptions | 1.1.1 | Required options, defaults, type validation and rejection |
| DeepMerge | 1.0.0 | Recursive map merge and replacement precedence |

The web example in `examples/helloweb` was built natively and exercised over HTTP (create, list, fetch, validation errors, 404s), matching the same project on the BEAM. It covers Plug 1.20.3, Bandit 1.12.5, ThousandIsland 1.5.0, Ecto 3.14.2, Ecto SQL 3.14.0, ecto_sqlite3 0.25.0, exqlite 0.42.0, DBConnection 2.10.2, Jason 1.4.5 and Telemetry 1.4.2. `compiler/tests/helloweb.rs` now builds and exercises the native HTTP service automatically in Linux/macOS CI; the BEAM comparison was verified manually.

Mint 1.11.0 builds natively and fetches over HTTPS (HTTP/2). Req 0.7.5 now compiles unchanged: remote function captures stored in compile-time module attributes retain their module/name/arity identity in the native program. `compiler/tests/req.rs` compares GET with query/header handling, JSON POST, redirects and HTTP error status against the same locked project on the BEAM. A native Req HTTPS request to Hex was also verified manually. Optional Req adapters, archive/compression codecs, and all package APIs are not covered by those checks.

These are specific compatibility checks, not a claim that every function in these packages or every Hex package works. The package fixtures are in `tests/fixtures/packages`; the Decimal example is in `examples/packages`.

Decimal uses the patched 3.0.0 release. Earlier versions below 3.0.0 are affected by [CVE-2026-32686](https://github.com/advisories/GHSA-rhv4-8758-jx7v).

```sh
TONIC_TEST_HEX=1 cargo test -p tonic --test packages -- --ignored
TONIC_TEST_HEX=1 cargo test -p tonic --test packagesextra -- --ignored
TONIC_TEST_HEX=1 cargo test -p tonic --test req -- --ignored
```

## Macros and compilation callbacks

User macros can execute compile-time code through the macro host. Coverage includes caller environments, computed expansions, default macro arguments, map splicing, generated definitions, `use`, and chained overridable functions and macros. In a Mix project, compile-time code can call `Mix.Project.config/0` (the `:app` and `:version` of the project the code belongs to) and the functions of each project's own `mix.exs` module.

`@on_definition` callbacks receive definition metadata and can update attributes seen by subsequent definitions. `@before_compile` callbacks can inject definitions and attributes. Native `@after_compile` callbacks run after LLVM generation; their second argument is the emitted LLVM IR binary. It is not a BEAM module binary. Interpreted `Code.eval_*` modules receive an empty binary because they do not emit a compiled artifact.

Modules created through `Code.eval_string` run in Tonic's interpreter. Compiled
code can call their ordinary public functions through remote calls, `apply` and
external captures; private functions stay private. Function and macro reflection
recognize those definitions. `compiler/tests/evaluation.rs` compares these paths,
exceptions and moving-GC behavior with BEAM. This does not implement BEAM artifact
loading, full generated-module introspection, or native module replacement.

## Concurrent applications

The supervised load test runs four producers against a GenServer, checks every result, monitors producer exits, kills the worker, and verifies that its supervisor starts a new worker. Requests retain closures, maps, lists, strings and tuples while moving garbage collection is forced on every allocation.

The ordinary test runs for five seconds per engine. The release gate extends each engine to sixty seconds. A cycle includes sixty validated requests and one automatic worker restart; the test prints completed request and restart counts. No throughput or memory ceiling is implied by this check.

```sh
cargo test -p tonic --test soak -- --nocapture
TONIC_SOAK_SECONDS=60 cargo test -p tonic --test soak -- --nocapture
```

## Supported project boundaries

Mix evaluates project metadata, dependency selection and compile configuration. Coverage includes computed metadata, transitive path dependencies, `only`, `targets`, `runtime: false`, application startup ordering, version checks, lock checks, and runtime configuration that reads the executable's environment.

Tonic accepts the supported standard Mix source compilation pipeline, including umbrella roots and interdependent child applications. `rebar3` dependencies are compiled from their Erlang sources and `.app.src` specification; yecc/leex generation runs in a scratch directory. Mix source generators explicitly listed in `tonic: [generators: [:task]]` run through installed Mix before native source collection. Their host dependencies must be available. Dependency Mix tasks are not linked into the native program. Undeclared custom compiler tasks, prebuilt BEAM/object-only artifacts, prebuilt native shared libraries and other build managers are rejected with diagnostics. Fetch dependencies with `mix deps.get` or `mix tonic.build --deps-get`; native compilation does not fetch missing packages.

`tonic test`, `mix tonic test` and `mix tonic.test` compile and execute ExUnit
natively, including helpers, application startup, file/directory/file:line
selection, tag filters, seed, timeout, trace and failure-limit options.
`mix tonic release` creates a relocatable native executable with application
`priv` assets and runtime configuration. `Tonic.native/1` is an opt-in project
adapter for ordinary `mix run`, `mix test` and `mix release`; projects that do
not opt in keep their normal BEAM pipeline. Native releases do not implement
ERTS release daemon, RPC or hot-upgrade protocols. These workflows are exercised
in `compiler/tests/tooling.rs` and `compiler/tests/tooling/filtering.rs`.

## NIFs

Dependencies built with `elixir_make` whose `c_src/` contains a C NIF (`ERL_NIF_INIT`) are compiled from source against Tonic's `erl_nif.h` and linked into the executable. The `-D` options of the dependency's Makefile `CFLAGS` are applied; compiled objects are cached. A module's `@on_load` callback runs at startup, `:erlang.load_nif/2` binds the linked library, and functions whose body is `:erlang.nif_error/1` call into it.

Tonic implements a subset of the `erl_nif` API: terms, binaries, resources, environments, messages, mutexes and monotonic time. NIF calls run on the calling process's scheduler thread. Managed resource references retain native state across GC, process messages, ETS, serialization roundtrips and NIF environments; the final release invokes the native destructor. Explicit `enif_keep_resource`/`enif_release_resource` ownership is preserved independently of term lifetimes. `runtime/tests/resources.rs` and `compiler/tests/resources.rs` exercise reclamation and copies. Rust NIFs (Rustler), prebuilt `.so`/`.dylib` files and dirty-scheduler semantics are not supported.

## Networking

`:gen_tcp` provides TCP listen, accept, connect, send, recv, close, shutdown and controlling process, in passive, `active: :once`, `active: true` and `active: N` modes. List delivery is the default; binary/header delivery and raw, 1/2/4-byte length-prefixed and line packets are supported. Socket options include local address/family binding, keepalive, nodelay, receive/send buffers and send timeouts; unsupported options return errors. `:inet` covers `setopts`, `getopts`, `sockname`, `peername`, `getstat`, `port`, address parsing and formatting. Waiting socket operations run on the runtime's I/O driver and do not block scheduler threads. `:erlang.decode_packet/3` additionally handles HTTP packet decoding.

`:gen_udp` provides `open/1,2`, `send/3,4` (address tuples or host names), `recv/2,3`, `close/1` and `controlling_process/2`, in passive, `active: :once`, `active: true` and `active: N` modes with `{:udp, socket, ip, port, data}` and `{:udp_passive, socket}` messages, `:list` or `:binary` delivery, and `:inet`/`:inet6`, `:ip`, `:reuseaddr`, `:reuseport`, `:broadcast`, `:recbuf` and `:sndbuf` options. `:inet.setopts/2`, `getopts/2`, `sockname/1`, `port/1`, `getstat/1` and `close/1` accept UDP sockets.

`:ssl` provides TLS 1.2 and 1.3 clients and servers on rustls (with the ring provider; no OpenSSL): `connect/2,3,4` to a host or over a connected `:gen_tcp` socket, `listen/2`, `transport_accept/1,2`, `handshake/1,2,3`, `send/2`, `recv/2,3`, `close/1,2`, `shutdown/2`, `setopts/2`, `getopts/2`, `controlling_process/2`, `peername/1`, `sockname/1`, `getstat/1,2`, `peercert/1`, `negotiated_protocol/1` and `connection_information/1,2`, in passive, `active: :once`, `active: true` and `active: N` modes with `{:ssl, socket, data}`, `{:ssl_closed, socket}`, `{:ssl_error, socket, reason}` and `{:ssl_passive, socket}`, `:list` or `:binary` delivery and `{:packet, :raw | 0 | 1 | 2 | 4 | :line}` framing. Options: `verify` (`:verify_peer` is the client default, and without `cacerts`/`cacertfile` it fails with OTP's `{:options, :incompatible, ...}` error), `cacerts` (DER or `:public_key.cacerts_get/0` records), `cacertfile`, `cert`/`key`, `certfile`/`keyfile`, `certs_keys`, `server_name_indication`, `versions`, `alpn_advertised_protocols`, `alpn_preferred_protocols`, `fail_if_no_peer_cert`, `sni_hosts`, `customize_hostname_check`, `:inet6` and `:ip`. Handshake failures return OTP's `{:tls_alert, {alert, description}}` reasons (`:unknown_ca`, `:handshake_failure` for a host name mismatch, `:certificate_required`, `:no_application_protocol`, ...), and sockets close when their controlling process exits. `:ssl.start/0,1`, `:ssl.stop/0`, `Application.ensure_all_started(:ssl)`, `:ssl.versions/0`, `:ssl.cipher_suites/1,2,3` and `:ssl.filter_cipher_suites/2` answer as OTP 27.3 does for the suites rustls implements. `:public_key` provides `cacerts_get/0`, `cacerts_load/0,1`, `pem_decode/1`, `pem_encode/1`, `pem_entry_decode/1`, `der_decode/2` for certificates and RSA/EC keys, `pkix_decode_cert/2` (`:plain` and `:otp`) and `pkix_verify_hostname_match_fun/1`. Programs that never call `:ssl` or `:public_key` link none of the TLS code.

## Ports

`Port.open/2` and `:erlang.open_port/2` start external programs with `{:spawn, command}` (run as `/bin/sh -c "exec command"`, as OTP does) and `{:spawn_executable, path}`. Supported options are `:binary`, `:exit_status`, `:eof`, `{:args, list}`, `{:arg0, name}`, `{:cd, dir}`, `{:env, list}`, `:stderr_to_stdout`, `:use_stdio`, `:nouse_stdio` (the program uses fds 3 and 4), `{:line, n}`, `{:packet, 1 | 2 | 4}`, `:stream`, `:in`, `:out`, `:hide` and `{:parallelism, bool}`. The owner receives `{port, {:data, data}}` (`{:eol, line}` and `{:noeol, part}` in line mode), `{port, {:exit_status, status}}`, `{port, :eof}`, `{port, :closed}` and `{port, :connected}` in OTP's order. `Port.command/2,3`, `send(port, {pid, {:command | :connect, _}})`, `send(port, {pid, :close})`, `Port.close/1`, `Port.connect/2`, `Port.info/1,2`, `Port.list/0`, `Port.monitor/1`, links, `exit(port, reason)` and `badsig` exit signals follow OTP 27; a port closes with its connected process. Ports are their own term type: `is_port/1`, printing as `#Port<0.N>` and term order match the BEAM, and ports survive `term_to_binary/1` round trips. Program output is read on the runtime's I/O driver and never blocks a scheduler thread. `System.cmd/3`, `System.shell/2` and `:os.cmd/1,2` are Elixir's and OTP's own implementations over ports.

`Port.list/0` omits the BEAM's own stdio ports (`#Port<0.0>` and `#Port<0.1>`), `{:spawn_driver, _}` and `{:fd, _, _}` are not supported, and `Port.info/2` reports an approximate `:memory` and a `:id` equal to the port number. `spawn_executable` of a directory fails with `:eacces` instead of opening a port that exits at once.

Distributed BEAM nodes, BEAM bytecode loading and full compatibility with undocumented OTP internals are outside this release's supported surface. Passing the source and runtime tests does not establish those capabilities.

## Known differences

These differ from Elixir 1.18 / OTP 27 in this release. Each is either intentional or tracked in TODO.md.

- Redefining a module (a second `defmodule` with the same name) is a compile error; Elixir replaces the module at run time.
- `:erlang.phash/2` and `:erlang.phash2/1,2` match OTP 27 for every term except pids, references and anonymous or local funs. Pids and references hash their printed numbers, which differ from the BEAM's; anonymous funs cannot reproduce OTP's per-module code checksum. External funs (`&Mod.fun/arity`) match.
- `Macro.expand/2` and `Macro.expand_once/2` expand aliases, user macros (remote, and imported ones when given `__ENV__`), public `defguard` macros including defaults, and the Kernel macros `if`, `unless`, `&&`, `||`, `!`, `|>`, `is_nil`, `then`, `tap`, `to_string`, `to_charlist` and `match?`. Other Kernel macros can remain unexpanded.
- `:calendar.local_time_to_universal_time/1` applies the current UTC offset rather than the offset in effect at the given local time, so results differ across a daylight saving change.
- Elixir's unused variable, ungrouped clause and duplicate map key warnings are emitted for code you write, but not inside code a dependency's macro generates (for example the body of a `Plug.Router` route). Elixir's other warnings (deprecations, type checking, unused functions, aliases and imports) are not emitted, and some invalid programs Elixir rejects are still accepted.
- Stack trace frames for anonymous functions inside `with ... else` and other inlined code are named after the enclosing function, where the BEAM names a generated function such as `-w/1-fun-0-`.
- A `quote` of a sigil omits the `delimiter:` metadata.
- `quote bind_quoted:` supports literal keyword lists and lexical `Kernel.binding`; custom macros or attributes producing that option are not fully expanded before validation and can be rejected.
- `:zlib` state is reclaimed when its last term owner disappears, including streams never explicitly closed. Closing a stream twice still raises `:badarg` rather than `:not_initialized`. Compressed bytes come from stock zlib 1.3.2; OTP builds linked to another zlib can produce different bytes while remaining mutually decompressible.
- Sockets are references rather than ports, so they print as `#Reference<...>` and `is_port/1` is false for them.
- `:gen_udp.connect/2,3`, connected `send/2`, multicast membership, TTL/TOS/dontroute and socket option readback are implemented. Ancillary-data receive APIs remain unsupported; unsupported options return an error instead of being ignored.
- TLS runs on rustls: TLS 1.2 and 1.3 only, no DTLS, renegotiation or session resumption across independently created connections. Cipher selection/order, verification depth, local binding and socket settings are applied. Client/server `verify_fun`, `partial_chain`, custom hostname `match_fun` and dynamic `sni_fun` callbacks execute through a cancellable process bridge. Rustls/webpki rejects unknown critical X.509 extensions during certificate parsing, so a callback cannot override those failures as OTP can. Unsupported options are rejected. Nested TLS (`:cb_info`) remains unsupported.
- TLS alert descriptions say `In state handshake` where OTP names its state machine state and source line, and OTP's notice-level log line for each alert is not printed. Local errors map to the alert OTP sends for unknown CAs, host name mismatches (`:handshake_failure`), expired certificates, missing client certificates and ALPN mismatches, and to the closest alert otherwise; the alert a rustls peer sends can differ from OTP's (a tonic client rejecting a host name sends `:bad_certificate` where OTP sends `:handshake_failure`). `connection_information/1` returns `protocol`, `selected_cipher_suite`, `session_resumption`, `sni_hostname` and `versions` only, not the session or option keys.
- TLS sockets are `{:sslsocket, reference, :tls}` tuples rather than OTP's records holding a port and pids. `:ssl.getstat/1,2` counts decrypted application data with the four `recv_`/`send_` `oct` and `cnt` keys, where OTP reports the TCP socket's statistics. `:ssl` works without starting the `:ssl` application, where OTP exits.
- `:public_key.cacerts_get/0` reads the system's PEM bundle (`SSL_CERT_FILE`, then /etc/ssl/cert.pem, /etc/ssl/certs/ca-certificates.crt and the other usual paths); OTP on macOS reads the keychains instead. Decoded certificates in the `:otp` form keep extension values as DER, and `:public_key` has no signing, verification, encryption or DER encoding.
- `:re` with `[:unicode]` but without `:ucp` matches `\w` and similar classes against ASCII only; use `~r/.../u` (which sets `:ucp`) for Unicode word characters.
- Converting a very large integer to a float rounds correctly (IEEE 754); the BEAM can differ in the last digit.
- `Code.string_to_quoted/1` error locations report line 0 without a column, and a compiled regex's `re_pattern` is not a real PCRE binary.
- Small maps with atom keys print keys in source order rather than atom-table order.
- Memory: each blocked process holds at least one page of its stack, so a process costs about 20 KB on Apple Silicon (16 KB pages) and about 7 KB on Linux (4 KB pages), compared to about 3 KB on the BEAM. Deep non-tail recursion works to any depth memory allows, but a stack frame takes about four times the memory of a BEAM frame (10 million frames of `n + f(n - 1)` use about 2 GB, the BEAM about 0.6 GB).
- Tree shaking conservatively retains matching public targets when an apply, capture, spawn or dynamic call cannot determine its module/function names statically. Runtime-created names do not require `TONIC_KEEP_ALL=1`; that variable remains an explicit request to retain every public function. The standard I/O server accepts runtime-named callbacks, so current analysis retains all public bodies even for hello world (~15 MB on Apple Silicon, versus ~4.5 MB in beta.3). Export metadata is retained independently of code pointers, and private functions remain inaccessible through reflection and dynamic calls.
- A project configured only with `compilers: [:tonic]` still produces no BEAM files for ordinary Mix tasks. Use the `Tonic.native/1` adapter for native run/test/release workflows, or keep the default compilers to run on the BEAM.
- Output written by a module body (`IO.puts` inside `defmodule`) appears on stderr while the program compiles, not when it runs.
- Compile-time process state belongs to the macro host. A process dictionary entry written during compilation does not become an entry in the native executable's runtime process, unlike compilation and execution within one BEAM script process.
- Logger's `debug`/`info`/... and `log` are functions, so calling them without `require Logger` works where Elixir raises.
- Tuple keyword misuse, invalid `as:` aliases, guard/function kind conflicts, private guard access, repeated `defoverridable` and undefined `@spec` targets now have focused rejection tests. Complete equivalence with every Elixir diagnostic and warning is not established.
- Stack traces keep a frame for a function that tail-calls a function the BEAM inlines (such as `String.to_atom/1`), show each frame of repeated body recursion, and add an `erlang.erl` `apply/2` frame to Task crash reports.
- Sorting terms that compare equal but differ (`1` and `1.0`) can order them differently from `:lists.sort/1`.
- A few error messages are worded differently: binary construction (`"a" <> 1`), updating a missing map key, `:erlang.binary_to_atom/2` on invalid UTF-8, `Process.exit/2` with a non-pid, and `Base.decode64!/1` on bad padding.
- `:inet.getifaddrs/0` is not implemented.
- Quoted `bind_quoted` variables retain source line/column metadata and explicit `var!` bindings participate in unused-variable warnings. Broader macro/AST metadata equivalence still needs conformance coverage.

