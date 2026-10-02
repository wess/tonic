#!/usr/bin/env python3
"""Exercise the installed Mix archive with the native compiler."""
import os
from pathlib import Path
import sys
import tempfile

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "tools"))
from harness import execute


def run(command, directory):
    previous = Path.cwd()
    try:
        os.chdir(directory)
        result = execute(command, 180)
    finally:
        os.chdir(previous)
    status, output, errors = result
    if status:
        raise AssertionError(f"{command}: exit {status}\n{output.decode()}\n{errors.decode()}")
    return output


def main():
    compiler = Path(os.environ.get("TONIC", ROOT / "target/release/tonic")).resolve()
    with tempfile.TemporaryDirectory(prefix="tonicarchive") as temporary:
        root = Path(temporary)
        archive = root / "tonic.ez"
        project = root / "project"
        project.mkdir()
        (project / "lib").mkdir()
        (project / "mix.exs").write_text('''defmodule NativeArchive.MixProject do
  use Mix.Project
  def project, do: [app: :nativearchive, version: "0.1.0", compilers: [:tonic], prune_code_paths: false, consolidate_protocols: false, escript: [main_module: NativeArchive]]
end
''')
        (project / "lib/main.ex").write_text('''defmodule NativeArchive do
  def main(args), do: IO.inspect({:native, args})
end
''')
        os.environ.update({"MIX_ARCHIVES": str(root / "archives"), "TONIC": str(compiler),
                           "MIX_ENV": "dev", "MIX_TARGET": "host"})
        run(["mix", "archive.build", "-o", str(archive)], ROOT / "mix")
        run(["mix", "archive.install", str(archive), "--force"], project)
        run(["mix", "tonic.check"], project)
        assert run(["mix", "tonic.run", "--", "with spaces", "$(literal)", "--deps-get"], project) == b'{:native, ["with spaces", "$(literal)", "--deps-get"]}\n'
        run(["mix", "tonic.build"], project)
        binary = project / "_build/dev/tonic/nativearchive"
        assert run([str(binary), "standalone"], root) == b'{:native, ["standalone"]}\n'
        run(["mix", "compile", "--no-deps-check"], project)
        assert run([str(binary), "compiled"], root) == b'{:native, ["compiled"]}\n'
        assert not list(project.glob("_build/**/*.beam"))
        run(["mix", "clean"], project)
        assert not binary.exists()
        print("Mix archive native build/run/check/compile/clean passed")


if __name__ == "__main__":
    main()
