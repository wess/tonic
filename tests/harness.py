#!/usr/bin/env python3
"""Exercise the runners with controlled executable processes."""
import os
from pathlib import Path
import subprocess
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parent.parent


class HarnessTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.source = self.root / "case.exs"
        self.source.write_text("")
        self.fixture = self.source.with_suffix(".out")
        self.fixture.write_text("expected\n")

    def runcheck(self, mode, program, reference=None, timeout="2"):
        executable = self.root / "tonic"
        executable.write_text("#!/usr/bin/env python3\n" + program)
        executable.chmod(0o755)
        elixir = self.root / "elixir"
        elixir.write_text("#!/usr/bin/env python3\n" + (reference or program))
        elixir.chmod(0o755)
        environment = dict(os.environ, TONIC=str(executable), ELIXIR=str(elixir))
        return subprocess.run(["python3", str(ROOT / "tools/harness.py"), mode,
                               str(self.source), "--timeout", timeout],
                              env=environment, capture_output=True, text=True, timeout=5)

    def testmatchingoutputwithfailurefails(self):
        result = self.runcheck("fixtures", 'print("expected"); raise SystemExit(7)')
        self.assertEqual(result.returncode, 1)
        self.assertIn("exit 7", result.stdout)

    def testfailedblesspreservesfixture(self):
        result = self.runcheck("bless", 'print("partial"); raise SystemExit(1)')
        self.assertEqual(result.returncode, 1)
        self.assertEqual(self.fixture.read_text(), "expected\n")

    def testmissingfixturefails(self):
        self.fixture.unlink()
        result = self.runcheck("fixtures", 'print("expected")')
        self.assertEqual(result.returncode, 1)
        self.assertIn("missing case.out", result.stdout)

    def testtimeoutfailswithdiagnostic(self):
        result = self.runcheck("fixtures", 'import time; time.sleep(30)', timeout="0.1")
        self.assertEqual(result.returncode, 1)
        self.assertIn("execution timed out", result.stdout)

    def testreferencetimeoutpreservesfixture(self):
        result = self.runcheck("bless", 'import time; time.sleep(30)', timeout="0.1")
        self.assertEqual(result.returncode, 1)
        self.assertEqual(self.fixture.read_text(), "expected\n")

    def testtimeoutkillsdescendants(self):
        marker = self.root / "survived"
        child = f"import time; from pathlib import Path; time.sleep(1); Path({str(marker)!r}).write_text('alive')"
        program = f"import subprocess, sys, time; subprocess.Popen([sys.executable, '-c', {child!r}]); time.sleep(30)"
        result = self.runcheck("fixtures", program, timeout="0.1")
        self.assertEqual(result.returncode, 1)
        self.assertIn("execution timed out", result.stdout)
        time.sleep(1.2)
        self.assertFalse(marker.exists())

    def testcomparechecksreferenceexit(self):
        result = self.runcheck("compare", 'print("expected")', 'print("expected"); raise SystemExit(9)')
        self.assertEqual(result.returncode, 1)
        self.assertIn("reference failed", result.stdout)

    def teststderrretainedonfailure(self):
        result = self.runcheck("fixtures", 'import sys; print("compiler diagnostic", file=sys.stderr); raise SystemExit(1)')
        self.assertEqual(result.returncode, 1)
        self.assertIn("compiler diagnostic", result.stdout)

    def testtrailingnewlinecompared(self):
        result = self.runcheck("fixtures", 'print("expected", end="")')
        self.assertEqual(result.returncode, 1)

    def testsuccessfulblessandrecheck(self):
        self.assertEqual(self.runcheck("bless", 'print("new")').returncode, 0)
        self.assertEqual(self.fixture.read_text(), "new\n")
        self.assertEqual(self.runcheck("fixtures", 'print("new")').returncode, 0)


if __name__ == "__main__":
    unittest.main()
