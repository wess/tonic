# Tonic 0.0.1

Experimental Elixir-to-native compiler with LLVM code generation, a moving
garbage collector, and a concurrent process runtime. Supports `.ex` modules,
`.exs` scripts, fetched Mix/Hex dependency sources, and `mix tonic.build`,
`mix tonic.run`, and `mix tonic.check`.

This release is intended for scripts and small applications. Compatibility is
limited to the language and runtime APIs Tonic implements. See
[compatibility.md](compatibility.md) for packages and supervision scenarios
verified by the release tests. Distributed BEAM nodes, hot code loading, ports,
arbitrary NIFs, custom dependency build systems, and umbrella root builds are
outside the current support boundary.

## Install

Download the bundle matching your machine from the release assets:

* `tonic0.0.1.linuxx64.tar.gz`: Linux x86-64, built on Ubuntu 24.04; glibc 2.39
  or newer is the supported baseline.
* `tonic0.0.1.macosarm64.tar.gz`: Apple Silicon macOS, built on macOS 14.

Compilation requires clang/LLVM 18 or newer. Mix project loading and archive
tasks additionally require Elixir 1.18 / OTP 27; native programs run without
Elixir/OTP. PCRE2 is included in the static runtime library.

Verify the matching `.tar.sha256` file before extracting (Linux: `sha256sum -c`;
macOS: `shasum -a 256 -c`). Then:

```sh
tar -xzf tonic0.0.1.macosarm64.tar.gz
cd tonic0.0.1
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
a `v0.0.1` tag. A manual workflow run verifies and uploads build artifacts
without publishing a release. Tag and manifest versions must match.
