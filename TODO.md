# Tonic TODO

Tonic 0.0.1 is experimental. The next priority is broader Elixir source and
runtime compatibility, backed by comparisons against Elixir/OTP. Supporting
`.ex` and `.exs` files does not establish full BEAM compatibility.

## Publish 0.0.1

- [x] Set the compiler, runtime, shared crate, and Mix archive to 0.0.1.
- [x] Package the compiler, static runtime, Mix archive, installer, checksums,
  license, and third-party notices.
- [x] Pass actual Linux and macOS CI, including package downloads, clean
  installation, native execution, and sustained supervision under GC stress.
- [ ] Push the prepared website and Pages workflow to `main`; verify deployment
  at https://wess.io/tonic/ on desktop and mobile.
- [ ] Create and push `v0.0.1`; verify that the release workflow publishes the
  experimental prerelease and both platform bundles.
- [ ] Update the website download button to the published release instead of
  temporary CI artifacts.

Git commits, pushes, and tags remain user-owned under the shared project guidance.

## Source compatibility

- [ ] Expand `.ex` / `.exs` differential tests for top-level expressions,
  module-level execution, evaluation order, and compile-time versus runtime
  effects. Compare standalone commands and Mix project builds.
- [ ] Expand native macro-host coverage for nested quoting, unquote splicing,
  caller environments, variable hygiene, generated definitions, and callbacks.
  Verify that compile-time effects run exactly once.
- [ ] Complete and test `Record` support: evaluated field defaults,
  `Record.is_record`, and record extraction from Erlang headers.
- [ ] Audit the remaining `:array` operations, including map/fold and sparse
  variants, against OTP. Preserve persistence and exact error behavior.
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
- [ ] Decide and document support for umbrella roots, `rebar3` dependencies,
  custom compilers, and packages requiring native libraries before implementing
  those build paths.

## Larger runtime decisions

These are outside the current release. They need explicit design work and
dedicated interoperability tests before any compatibility claims.

- [ ] Define a ports and external-process interface.
- [ ] Decide whether to support NIFs or provide a separate native extension API.
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
- [Verified 0.0.1 bundles](https://github.com/wess/tonic/actions/runs/37469587923)

Existing tests live in `compiler/tests/`, `tests/cases/`, `tests/fixtures/`, and
`mix/tests/`. Every compatibility fix should include a focused regression and,
where practical, a reference Elixir/OTP comparison.
