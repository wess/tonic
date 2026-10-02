#!/usr/bin/env python3
"""Verify release contents and an installation with no checkout or cached objects."""
import hashlib
import json
import os
from pathlib import Path
import sys
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "tools"))
from harness import execute


def run(command, directory):
    previous = Path.cwd()
    try:
        os.chdir(directory)
        status, output, errors = execute([str(arg) for arg in command], 180)
    finally:
        os.chdir(previous)
    if status:
        raise AssertionError(f"{command}: exit {status}\n{output.decode()}\n{errors.decode()}")
    return output


def verify(artifact):
    expected = artifact.with_suffix(".sha256").read_text().split()[0]
    assert hashlib.sha256(artifact.read_bytes()).hexdigest() == expected
    with tempfile.TemporaryDirectory(prefix="tonicdistribution") as temporary:
        root = Path(temporary)
        with tarfile.open(artifact) as contents:
            contents.extractall(root, filter="data")
        bundle, = root.iterdir()
        manifest = json.loads((bundle / "manifest.json").read_text())
        for name, expected in manifest["files"].items():
            assert hashlib.sha256((bundle / name).read_bytes()).hexdigest() == expected, name
        prefix = root / "installation with spaces"
        run(["bash", bundle / "install.sh", prefix], root)
        source = root / "hello.exs"
        source.write_text((bundle / "hello.exs").read_text())
        # Remove the extracted compiler to verify the installed runtime search path.
        import shutil
        shutil.rmtree(bundle)
        os.environ.update(TONIC_CACHE=str(root / "cache"), TONIC=str(prefix / "bin/tonic"),
                          MIX_ARCHIVES=str(root / "archives"), MIX_ENV="dev", MIX_TARGET="host")
        os.environ.pop("TONIC_RUNTIME", None)
        compiler = prefix / "bin/tonic"
        assert run([compiler, "--version"], root) == f'tonic {manifest["version"]}\n'.encode()
        run([compiler, "build", source, "-o", root / "hello"], root)
        previous_path = os.environ.get("PATH", "")
        try:
            os.environ["PATH"] = str(root / "missing")
            assert run([root / "hello"], root) == b"Hello, world!\n"
        finally:
            os.environ["PATH"] = previous_path
        # Computed macro expansion forces the installed native macro host to start.
        macro = root / "macro.exs"
        macro.write_text('''defmodule NativeMacro do
  defmacro answer do
    value = Enum.sum([20, 22])
    quote do: unquote(value)
  end
end
defmodule NativeMain do
  require NativeMacro
  def answer, do: NativeMacro.answer()
end
IO.inspect({NativeMain.answer(), Regex.match?(~r/^native$/, "native")})
''')
        run([compiler, "build", macro, "-o", root / "macro"], root)
        try:
            os.environ["PATH"] = str(root / "missing")
            assert run([root / "macro"], root) == b"{42, true}\n"
        finally:
            os.environ["PATH"] = previous_path
        run(["mix", "archive.install", prefix / "share/tonic/tonic.ez", "--force"], root)
        project = root / "project"
        (project / "lib").mkdir(parents=True)
        (project / "mix.exs").write_text('''defmodule Release.MixProject do
  use Mix.Project
  def project, do: [app: :releaseprobe, version: "0.0.1", escript: [main_module: ReleaseProbe]]
end
''')
        (project / "lib/main.ex").write_text('''defmodule ReleaseProbe do
  def main(args), do: IO.inspect(args)
end
''')
        assert run(["mix", "tonic.run", "--", "installed"], project) == b'["installed"]\n'
        print("Release checksums, clean installation, native macros, standalone executable, and Mix archive passed")


if __name__ == "__main__":
    verify(Path(sys.argv[1]).resolve())
