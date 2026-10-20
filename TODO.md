# Tonic TODO

Tonic is in public beta (0.1.0-beta.2), for scripts and applications within
the supported boundary in docs/compatibility.md. The next priority is broader
Elixir source and runtime compatibility, backed by comparisons against
Elixir/OTP. Supporting `.ex` and `.exs` files does not establish full BEAM
compatibility.

## Releases

- [x] 0.1.0-beta.1 (2026-10-16): first public release.
- [x] 0.1.0-beta.2: generational collector, tree-shaken executables, smaller
  process footprint.

## Beta follow-ups

Known differences are listed in docs/compatibility.md. In priority order:

- [ ] Grow process stacks on demand (or catch overflow per process) instead of
  ending the program; the BEAM handles ten million frames.
- [x] Generational collection: long-lived data is no longer copied on every
  collection, and heap growth copies once.
- [ ] Per-process baseline: a blocked process holds a stack page (16 KB on
  Apple Silicon). Copying small stacks out on block would approach the BEAM's
  ~3 KB.
- [ ] Executable size: PCRE2 and the inspect/exception machinery are always
  linked (hello world is ~4.5 MB); make regex and exception formatting
  reachable only when used.
- [ ] Make `:calendar.local_time_to_universal_time/1` use the offset in effect
  at that local time, as the BEAM does.
- [ ] Emit the remaining compile warnings (ungrouped clauses, duplicate map
  keys, unused variables in dependency macro output) and reject the invalid
  programs still accepted.

## Source compatibility

- [ ] Expand `.ex` / `.exs` differential tests for top-level expressions,
  module-level execution, evaluation order, and compile-time versus runtime
  effects. Compare standalone commands and Mix project builds.
- [ ] Expand native macro-host coverage for nested quoting, unquote splicing,
  caller environments, variable hygiene, generated definitions, and callbacks.
  Verify that compile-time effects run exactly once.
- [ ] Complete and test `Record` support: evaluated field defaults,
  `Record.is_record`, and record extraction from Erlang headers.
- [x] Audit the remaining `:array` operations: the bundled module is now OTP
  27.3's own `array.erl`.
- [ ] Audit Erlang preprocessing conditions. Unsupported expressions must
  produce diagnostics rather than silently evaluate to false.
- [ ] Continue adding file-and-line diagnostics for unsupported constructs;
  avoid accepting code whose executable behavior is discarded.

## Runtime and standard library

- [ ] Build an API coverage inventory for the bundled Elixir and Erlang library:
  implemented, partially implemented, unsupported, and intentionally different.
  Link entries to runnable tests.
- [ ] Broaden process tests for mailbox ordering, links, monitors, exit signals,
  cancellation, timers, and supervisor restart strategies under concurrent load.
- [ ] Extend GC testing beyond the current 60-second soak: long-running retained
  data, process churn, ETS, closures, binaries, and memory growth. Record peak
  memory and cleanup behavior.
- [ ] Expand file, Unicode, date/time, exception, stack-trace, and regex parity
  tests. Document PCRE2 differences from the reference OTP regex engine.
- [ ] Measure cold/warm compilation, executable startup, binary size, and
  representative runtime workloads. Publish reproducible commands and results.

## Mix and package compatibility

- [ ] Expand coverage beyond the current Decimal, Jason, NimbleOptions, and
  DeepMerge scenarios. Exercise more APIs within each package as well as more
  real packages; keep exact versions and lockfiles.
- [ ] Test representative applications with dependencies, startup callbacks,
  runtime configuration, and supervised services—not only isolated functions.
- [ ] Strengthen environment/target, optional dependency, protocol, and stale
  cache coverage across project and dependency sources.
- [ ] Decide and document support for umbrella roots and custom compilers.
- [x] Turn `examples/helloweb` into an automated test (`compiler/tests/helloweb.rs`,
  run in CI): build it natively and exercise its HTTP endpoints.
- [x] Refresh `licenses/cargo` for the crates tokio's `net` feature added
  (mio, socket2, ...).

## Larger runtime decisions

These are outside the current release. They need explicit design work and
dedicated interoperability tests before any compatibility claims.

- [x] Ports to external programs (`spawn`, `spawn_executable`), with `System.cmd/3` and `:os.cmd/1` over them. Port drivers, `{:fd, _, _}` and `:erlang.port_control/3` remain unsupported.
- [ ] Extend the `erl_nif` subset (maps, monitors, select, dirty schedulers) and
  release resources when no term references them. The same release would
  free unclosed `:zlib` streams and `:atomics` arrays.
- [ ] SSL sockets, `:gen_udp.connect` and UDP multicast.
- [ ] Define the scope of distributed-node support and BEAM interoperability.
- [ ] Decide how dynamic module loading and hot code replacement would work
  with native compilation.
- [ ] Document the native equivalents and limits of BEAM-specific tooling,
  introspection, and compilation artifacts.

## Keep documentation accurate

- [ ] Update the README, website, and compatibility report together as support
  changes. Separate tested behavior from intended compatibility.
- [ ] Refresh generated notices whenever imported sources, Cargo dependencies,
  or the release toolchain change.
- [ ] Add an upgrade and troubleshooting guide based on actual user reports.

## Existing evidence

- [Compatibility report](docs/compatibility.md)
- [Installation and release guide](docs/release.md)
- [Linux/macOS CI](https://github.com/wess/tonic/actions/runs/37013537408)

Existing tests live in `compiler/tests/`, `tests/cases/`, `tests/fixtures/`, and
`mix/tests/`. Every compatibility fix should include a focused regression and,
where practical, a reference Elixir/OTP comparison.
