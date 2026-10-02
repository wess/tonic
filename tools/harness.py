#!/usr/bin/env python3
"""Bounded fixture and live-reference checks for successful Elixir scripts."""
import argparse
import difflib
import math
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent.parent


def normalize(output):
    # Only ExUnit's elapsed-time summary is nondeterministic in the fixtures.
    return re.sub(rb"(?m)^Finished in [0-9.]+ seconds[^\n]*$", b"Finished in <time>", output)


def execute(command, timeout):
    try:
        process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                   start_new_session=True)
    except OSError as error:
        return 127, b"", str(error).encode()
    try:
        output, errors = process.communicate(timeout=timeout)
        return process.returncode, output, errors
    except subprocess.TimeoutExpired:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        output, errors = process.communicate()
        return 124, output, errors + b"\nexecution timed out\n"


def diagnostics(label, result):
    status, output, errors = result
    print(f"{label}: exit {status}")
    if output:
        print("stdout:\n" + output.decode(errors="replace"), end="")
    if errors:
        print("stderr:\n" + errors.decode(errors="replace"), end="")


def check(mode, paths, tonic, elixir, timeout):
    passed = failed = 0
    for source in paths:
        fixture = source.with_suffix(".out")
        reference = None
        if mode in ("bless", "compare"):
            reference = execute([elixir, str(source)], timeout)
            if reference[0] != 0:
                print(f"FAIL {source} (reference failed)")
                diagnostics("elixir", reference)
                failed += 1
                continue
        if mode == "bless":
            # Replace only after a complete, successful reference execution.
            with tempfile.NamedTemporaryFile(dir=fixture.parent, delete=False) as temporary:
                temporary.write(normalize(reference[1]))
                temporary_path = Path(temporary.name)
            temporary_path.replace(fixture)
            print(f"blessed {source.stem}")
            passed += 1
            continue
        if mode == "fixtures" and not fixture.is_file():
            print(f"FAIL {source} (missing {fixture.name})")
            failed += 1
            continue
        expected = normalize(reference[1]) if reference else fixture.read_bytes()
        actual = execute([tonic, "run", str(source)], timeout)
        if actual[0] == 0 and normalize(actual[1]) == expected:
            print(f"ok   {source.stem}")
            passed += 1
        else:
            print(f"FAIL {source}")
            diagnostics("tonic", actual)
            if reference and reference[2]:
                diagnostics("elixir", reference)
            difference = difflib.unified_diff(expected.decode(errors="replace").splitlines(True),
                                             normalize(actual[1]).decode(errors="replace").splitlines(True),
                                             fromfile="expected", tofile="actual")
            print("".join(difference), end="")
            failed += 1
    print(f"{passed} passed, {failed} failed")
    return int(failed != 0)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("fixtures", "bless", "compare"))
    parser.add_argument("paths", nargs="*", type=Path)
    parser.add_argument("--timeout", type=float, default=float(os.environ.get("TONIC_TEST_TIMEOUT", "60")))
    options = parser.parse_args()
    if not math.isfinite(options.timeout) or options.timeout <= 0:
        parser.error("timeout must be positive")
    os.chdir(ROOT)
    paths = options.paths or sorted(Path("tests/cases").glob("*.exs"))
    if not paths:
        parser.error("no scripts selected")
    return check(options.mode, paths, os.environ.get("TONIC", str(ROOT / "target/release/tonic")),
                 os.environ.get("ELIXIR", "elixir"), options.timeout)


if __name__ == "__main__":
    sys.stdout.reconfigure(line_buffering=True)
    sys.exit(main())
