# Elixir semantics and the Tonic compiler

Write idiomatic Elixir that behaves the same on the BEAM and Tonic's LLVM
backend. Tonic's goal is 100% Elixir compatibility. The current beta implements
a subset; [known differences](compatibility.md) describe gaps to close, not a
separate language specification.

This guide covers application code and compiler contributions. Use the
[reference](../README.md) for CLI and Mix details, and the
[roadmap](../TODO.md) for work still ahead.

## Establish the reference version

The test baseline is Elixir 1.18.3 / OTP 27. CI installs OTP 27.2.4. Some bundled
Erlang library sources come from OTP 27.3; their provenance is recorded in
[notice](../notice) and [licenses](../licenses/). Record the exact versions
when investigating behavior that differs between OTP patch releases.

Use versioned upstream references:

- [Elixir 1.18.3 documentation](https://hexdocs.pm/elixir/1.18.3/)
- [Elixir 1.18.3 source and tests](https://github.com/elixir-lang/elixir/tree/v1.18.3)
- [Erlang/OTP 27 documentation](https://www.erlang.org/docs/27/)
- [Erlang/OTP 27.2.4 source and tests](https://github.com/erlang/otp/tree/OTP-27.2.4)

Upstream documentation defines documented behavior; pinned source and small
reference programs resolve edge cases. Tonic's tests establish the behavior
actually exercised. An exported function or an accepted option alone does not
establish correct implementation. Consult the compatibility report for partial
implementations, unsupported options, and backend limits.

## Preserve Elixir semantics

- Values are immutable. Rebinding a variable gives it a new value; it does not
  mutate an earlier value. `=` matches patterns, and `^value` matches against an
  existing binding. Clause order, guards, and variable scope affect behavior.
- Only `false` and `nil` are falsy. `and`, `or`, and `not` require a boolean
  left operand or argument; `&&`, `||`, and `!` accept truthy values. Preserve
  short-circuiting and the returned operand.
- `==` permits numeric equality between integers and floats; `===` distinguishes
  their types. Integer arithmetic must retain arbitrary precision. Map keys use
  exact equality; `1` and `1.0` can be distinct keys. Do not turn Elixir term
  ordering into host-language ordering.
- Strings are UTF-8 binaries; charlists are lists of code points. Byte counts,
  code-point counts, and grapheme counts differ. Binary pattern sizes and
  bitstring segment sizes must follow Elixir's rules.
- Lists are linked lists, and improper lists are possible. Tuples, lists, maps,
  structs, and keyword lists are distinct. Keyword lists preserve ordering and
  duplicate keys; structs retain their module identity.
- Guards have restricted operations and special failure behavior. An operation
  that fails in a guard does not behave like the same operation in a function
  body. Match upstream acceptance and rejection as well as successful results.
- Preserve left-to-right effects, pattern failure, exception class and reason,
  `try`/`rescue`/`catch`/`else`/`after`, and tail-call behavior. A program producing
  the expected final number can still have incorrect observable behavior.
- Do not depend on map enumeration order, numeric pid/reference identities, or
  scheduling between independent senders. Test the documented guarantees, and
  isolate representation-dependent output when comparing engines.

For ordinary application code, use pattern matching, small functions, explicit
data, and standard protocols. Add a process when state ownership, concurrency,
or failure isolation calls for one; a module does not need a process merely to
organize functions. Avoid converting untrusted strings into new atoms.

## Understand macros and compilation phases

`alias` changes name resolution, `import` brings functions or macros into local
scope, `require` enables a module's macros, and `use` invokes its `__using__/1`
macro. These have different effects. Resolve local, imported, and remote calls
with the correct arity and visibility.

`quote` constructs AST and `unquote` inserts a value into it. Preserve variable
hygiene, AST metadata, caller environments, aliases, and `unquote_splicing`.
Module attributes are evaluated in their compilation context; they are not
runtime global variables. Generated definitions, `defoverridable`, `super`,
protocols, and compilation callbacks must retain their phase and ordering.

Tonic parses and expands source in its own compiler. Computed macros execute
through the native macro host and bundled evaluator. Mix evaluates project
metadata and compile configuration for project builds. Distinguish those two
paths when diagnosing compile-time behavior.

The current `@after_compile` callback receives LLVM IR rather than a BEAM module
binary; interpreted modules receive an empty binary. `tonic check` parses and
expands without LLVM generation or after-compilation callbacks. See the
[callback and macro limitations](compatibility.md#macros-and-compilation-callbacks)
before assuming artifact-dependent tooling is portable.

## Understand BEAM and OTP behavior

A BEAM process is a lightweight runtime process with its own state and mailbox,
not an operating-system process. Tonic implements Elixir processes as stackful
coroutines driven by tokio tasks, with per-process heaps and copied messages.
The implementation differs; the observable process contract is the target.

`send/2` is asynchronous. Selective `receive` scans for matching messages while
retaining unmatched messages. Signals from one sender to one destination have
ordering guarantees; independent senders do not establish a global order.
Test synchronization with messages and monitors rather than arbitrary sleeps.
The [OTP 27 process reference](https://www.erlang.org/docs/27/system/ref_man_processes.html)
describes the signal and mailbox model.

Links propagate exit signals; monitors report termination with `:DOWN` messages.
Trapping exits changes how eligible exit signals are handled, while `:kill` is
untrappable. A GenServer call is a request/reply protocol that can time out or
exit; a cast does not acknowledge successful processing. Preserve these
distinctions and test races explicitly.

Use supervisors for restart policy and lifecycle ownership. Choose restart
strategies and child specifications according to the dependency between child
processes. A supervisor cannot make an unsafe external side effect reversible.
Keep application startup and runtime configuration in the appropriate phase.
See [GenServer](https://hexdocs.pm/elixir/1.18.3/GenServer.html) and
[Supervisor](https://hexdocs.pm/elixir/1.18.3/Supervisor.html).

BEAM features such as distributed nodes, dynamic bytecode loading, hot code
replacement, dirty NIF schedulers, and introspection require dedicated native
implementations or interoperability work. They are current compatibility gaps.
Do not claim that native executables load `.beam` files, or that compiling an
Erlang module establishes distributed-node interoperability.

## Keep Mix and Hex workflows ordinary

Use `mix.exs`, `mix.lock`, configuration files, and `mix deps.get` as usual.
Tonic compiles fetched dependency sources; it does not link their BEAM files.
Fetching a package establishes availability, not native compatibility. Keep the
lockfile and package version when reporting a failure.

```sh
mix deps.get
tonic check .
tonic build . -o program
./program
```

With Tonic's Mix archive installed, `mix tonic check`, `mix tonic build`, and
`mix tonic run` invoke the native compiler. Ordinary `mix compile`, `mix run`,
and `mix release` use the project's configured Mix pipeline. Opting into the
`:tonic` compiler produces a native artifact; it does not produce BEAM modules
for `mix run` or `mix release`. The development `Tonic.native/1` project adapter
routes ordinary run/test/release tasks to native equivalents. `tonic test` and
`mix tonic test` run ExUnit natively, and `mix tonic release` bundles the native
executable with relocatable application assets and runtime configuration.

Current project boundaries, supported dependency compilers, NIF restrictions,
and application startup behavior are in the
[compatibility report](compatibility.md#supported-project-boundaries).

## Find the implementation

Paths are relative to the repository root.

| Concern | Implementation |
| --- | --- |
| Elixir syntax and AST | `compiler/src/lexer.rs`, `parser.rs`, `ast.rs` |
| Modules, imports, macros, expansion | `compiler/src/collect.rs`, `expand.rs`, `quote.rs`, `ctime.rs`, `macrohost.rs` |
| Native macro evaluation | `lib/eval.ex`, `lib/macro_host.ex` |
| Erlang front end | `compiler/src/erl_lex.rs`, `erl_parse.rs`, `erl_pp.rs`, `erl_trans.rs` |
| Core IR and LLVM | `compiler/src/core.rs`, `codegen.rs` |
| CLI, native linking, caches | `compiler/src/main.rs`, `native.rs`, `cache.rs`, `prelude.rs` |
| Mix projects and NIF builds | `compiler/src/project.rs`, `project.exs`, `nif.rs` |
| Term encoding and equality | `shared/src/lib.rs`, `runtime/src/term.rs`, `cmp.rs`, `num.rs`, `map.rs` |
| Moving GC and roots | `runtime/src/heap.rs`, `compiler/src/codegen.rs` |
| Processes and stack growth | `runtime/src/sched.rs`, `bif_proc.rs`, `stackseg.rs` |
| Runtime BIF dispatch | `compiler/src/bifs.rs`, `runtime/src/resolve.rs`, `runtime/src/bif_*.rs` |
| Sockets, TLS, ports, NIFs | `runtime/src/net.rs`, `udp.rs`, `port/`, `nif.rs`, `runtime/tls/` |
| Bundled standard library | `lib/`, including imported sources under `lib/ex/` and `lib/erl/` |

Compiled code and runtime code share a term ABI and process context. Heap values
live across calls through shadow-stack slots; native BIFs must root live terms
across allocations and reload values after moving collection. Retaining a raw
heap pointer across allocation can become a use-after-move bug.

The code generator uses LLVM `tailcc` calls for tail positions. Exceptions use
pending state in the process context and a zero return sentinel. Changes to
calling conventions, context offsets, term tags, or root layout must agree
across compiler and runtime. Read the source before changing an ABI detail.

## Reproduce and close a compatibility gap

1. Reduce the issue to a small `.exs` program or locked Mix project. Record the
   Tonic commit, OS, architecture, Elixir/OTP versions, and compiler settings.
2. Run the same input under reference Elixir and Tonic. Capture exit status,
   stdout, stderr, exceptions, and relevant effects. Identify whether failure
   occurs in parsing, expansion, macro execution, LLVM generation, linking,
   runtime behavior, or project loading.
3. Check the pinned upstream documentation and source. Separate documented
   language semantics from an incidental runtime representation or race.
4. Add a focused regression to `tests/cases/` or `compiler/tests/`, then fix the
   responsible layer. Successful execution fixtures need a reference `.out`.
   Use Rust integration tests for compile failures, diagnostics, exit status,
   and behavior the stdout fixture runner does not assert.
5. Run the focused checks on both engines. Exercise affected GC/process paths
   under stress. Update the compatibility report and roadmap when support
   changes; keep any remaining limitations explicit.

Build before running native comparisons:

```sh
cargo build --workspace --locked
cargo test --workspace --locked
cargo build --release --workspace --locked
bash tools/compare.sh tests/cases/basics.exs
bash tests/run.sh tests/cases/basics.exs
TONIC_GC_STRESS=1 TONIC_GC_VERIFY=1 bash tests/run.sh tests/cases/basics.exs
```

For a new successful fixture, `bash tests/run.sh --bless tests/cases/example.exs`
records output from reference Elixir. Review that output, then run both fixture
and live-reference checks. Never bless native output as the reference.

The runners require successful exits and compare stdout, normalizing only
ExUnit's elapsed-time summary. They retain stderr for failure diagnostics;
they do not assert matching stderr or all external effects. Concurrent tests
need assertions for protocol behavior, process exits, and cleanup.

The [CI workflow](../.github/workflows/ci.yml) is the authoritative list of
release checks. It includes Linux/macOS builds, Rust tests, Mix archive tests,
locked Hex comparisons, native HTTP checks, fixtures, live Elixir parity, GC
stress, installation verification, and a supervision soak. Passing these
checks supports their covered scenarios; it does not establish full parity.
