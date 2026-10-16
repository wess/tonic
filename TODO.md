# Tonic TODO

Tonic 0.1.0-beta.1 is the first public release: a beta for scripts and small
applications. The next priority is broader Elixir source and runtime
compatibility, backed by comparisons against Elixir/OTP. Supporting `.ex` and
`.exs` files does not establish full BEAM compatibility.

## Publish 0.1.0-beta.1

- [x] Set the compiler, runtime, shared crate, and Mix archive to 0.1.0-beta.1.
- [x] Record provenance for the imported EEx, `base64` and `array` sources and
  regenerate notices (also covers the crates tokio's `net` feature added).
- [x] Package and verify a clean macOS installation locally
  (`tools/release.py`, `tests/release.py`).
- [ ] Commit and push to `main`; confirm Linux and macOS CI pass.
- [ ] Verify the website deployment at https://wess.io/tonic/ on desktop and
  mobile; its download button points at the `v0.1.0-beta.1` release.
- [ ] Create and push `v0.1.0-beta.1`; verify that the release workflow
  publishes the prerelease and both platform bundles.

Git commits, pushes, and tags remain user-owned under the shared project guidance.

## Beta follow-ups

Known differences are listed in docs/compatibility.md. In priority order:

- [ ] Grow process stacks on demand (or catch overflow per process) instead of
  ending the program; the BEAM handles ten million frames.
- [ ] Reduce heap growth peaks (copying into three times live data while the old
  space is still mapped) and the per-process baseline.
- [ ] Match OTP's `:erlang.phash2/1,2`.
- [ ] Expand Kernel macros in `Macro.expand/2`; read `@moduledoc`/`@doc` in
  function bodies; port `:calendar` RFC 3339 functions.
- [ ] Match `URI.new/1` / `:uri_string.parse/1` validation.
- [ ] Emit Elixir's compile warnings (unused variables, ungrouped clauses) and
  reject the invalid programs still accepted; fix runtime stack trace lines.

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

- [ ] Define a ports and external-process interface.
- [ ] Extend the `erl_nif` subset (maps, monitors, select, dirty schedulers) and
  release resources when no term references them.
- [ ] SSL and UDP sockets.
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
