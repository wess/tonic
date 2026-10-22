#!/usr/bin/env python3
"""Check conformance classification and evidence validation."""
import importlib.util
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("coverage", ROOT / "tools/coverage.py")
coverage = importlib.util.module_from_spec(spec)
spec.loader.exec_module(coverage)

API = {"module": "Elixir.Kernel", "name": "is_integer", "arity": 1, "kind": "function"}
NATIVE = {"version": "test", "exports": [dict(API, source="compiler/src/bifs.rs")]}
REFERENCE = {"elixir": "1.18.3", "otp": "27", "exports": [API]}


def rejects(rule, message):
    try:
        coverage.report(NATIVE, REFERENCE, {"apis": [rule]})
    except ValueError as error:
        assert message in str(error), str(error)
    else:
        raise AssertionError("invalid evidence was accepted")


def main():
    import json
    snapshot = json.loads((ROOT / "tests/coverage/reference.json").read_text())
    modules = {row["module"] for row in snapshot["exports"]}
    assert {"erlang", "atomics", "counters", "persistent_term", "zlib"} <= modules
    ordinary = coverage.report(NATIVE, REFERENCE, {"apis": []})
    assert ordinary["exports"][0]["status"] == "partial"
    missing = coverage.report({"version": "test", "exports": []}, REFERENCE, {"apis": []})
    assert missing["exports"][0]["status"] == "unsupported"
    rule = dict(API, status="implemented", reason="bounded reference regression",
                tests=["tests/cases/portability.exs"])
    assert coverage.report(NATIVE, REFERENCE, {"apis": [rule]})["exports"][0]["status"] == "implemented"
    rejects(dict(rule, tests=[]), "needs a declaration and regression")
    rejects(dict(rule, tests=["tests/missing.exs"]), "missing regression")
    rejects(dict(rule, tests=["compiler/tests/inventory.rs#missing_test"]), "missing test name")
    rejects(dict(rule, status="complete"), "missing status/reason")
    rejects(dict(rule, name="missing"), "unknown reviewed API")
    try:
        coverage.report(NATIVE, REFERENCE, {"apis": [rule, rule]})
    except ValueError as error:
        assert "duplicate" in str(error)
    else:
        raise AssertionError("duplicate rule was accepted")
    print("conformance classification and evidence checks passed")


if __name__ == "__main__":
    main()
