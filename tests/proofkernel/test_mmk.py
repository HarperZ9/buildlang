# SPDX-License-Identifier: AGPL-3.0-only
"""mmk, the Metamath verifier in tools/proofkernel, on a database it must accept and planted faults it must refuse.

Run from the repository root:  BUILDC=compiler/target/release/buildc python tests/proofkernel/test_mmk.py
It builds mmk through the C target and gcc, and skips when either is missing.
"""
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
KERNEL = ROOT / "tools" / "proofkernel"
OK = (Path(__file__).with_name("ok.mm")).read_text(encoding="utf-8")


def _buildc():
    env = os.environ.get("BUILDC")
    for cand in ([env] if env else []) + [str(ROOT / "compiler/target/release/buildc"), str(ROOT / "compiler/target/release/buildc.exe")]:
        if cand and Path(cand).is_file():
            return cand
    return None


class Mmk(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        buildc, gcc = _buildc(), shutil.which("gcc")
        if not buildc or not gcc:
            raise unittest.SkipTest(f"needs buildc ({buildc or 'not built'}) and gcc ({gcc or 'not on PATH'})")
        cls.tmp = tempfile.TemporaryDirectory()
        work = Path(cls.tmp.name)
        for f in KERNEL.glob("*.bld"):
            shutil.copyfile(f, work / f.name)
        subprocess.run([buildc, "mmk.bld", "-o", "mmk.c", "--target", "c"], cwd=work, check=True, capture_output=True)
        exe = work / ("mmk.exe" if os.name == "nt" else "mmk")
        subprocess.run([gcc, "-O1", "-w", "-o", str(exe), "mmk.c"], cwd=work, check=True, capture_output=True)
        cls.exe, cls.work = exe, work

    @classmethod
    def tearDownClass(cls):
        cls.tmp.cleanup()

    def check(self, text):
        path = self.work / "db.mm"
        path.write_text(text, encoding="utf-8", newline="\n")
        out = subprocess.run([str(self.exe), str(path)], capture_output=True, text=True)
        return out.returncode, out.stdout

    def test_accepts_a_valid_database(self):
        rc, out = self.check(OK)
        self.assertEqual(rc, 0, out)
        self.assertIn("OK statements=10 proofs=2", out)

    def test_refuses_a_proof_of_the_wrong_statement(self):
        rc, out = self.check(OK.replace("th2 $p wff ( p -> p )", "th2 $p wff ( p -> q )"))
        self.assertNotEqual(rc, 0, out)
        self.assertIn("wrong-conclusion th2", out)

    def test_refuses_a_missing_hypothesis(self):
        rc, out = self.check(OK.replace("$= wp wq h1 h2 mp $.", "$= wp wq h1 mp $."))
        self.assertNotEqual(rc, 0, out)
        self.assertIn("ERR", out)

    def test_refuses_an_undeclared_label(self):
        rc, out = self.check(OK.replace("$= wp wp wi $.", "$= wp wp wx $."))
        self.assertNotEqual(rc, 0, out)
        self.assertIn("ERR", out)

    def test_refuses_a_swapped_hypothesis_order(self):
        rc, out = self.check(OK.replace("$= wp wq h1 h2 mp $.", "$= wp wq h2 h1 mp $."))
        self.assertNotEqual(rc, 0, out)


if __name__ == "__main__":
    sys.exit(unittest.main())
