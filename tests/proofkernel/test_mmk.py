# SPDX-License-Identifier: AGPL-3.0-only
"""mmk, the Metamath verifier in tools/proofkernel, on databases it must accept and planted faults it must refuse.

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
PTX_MP = (Path(__file__).with_name("ptx_mp.mm")).read_text(encoding="utf-8")

# The same program without its fences: thread 0 writes x then y, thread 1 reads y then x.
UNFENCED = """
${
  mpu.1 $e |- po Wx Wy $.
  mpu.2 $e |- rf Wy Ry $.
  mpu.3 $e |- po Ry Rx $.
  mp-unfenced $p |- sees Rx Wx $=
    %s $.
$}
"""


def _buildc():
    env = os.environ.get("BUILDC")
    for cand in ([env] if env else []) + [str(ROOT / "compiler/target/release/buildc"), str(ROOT / "compiler/target/release/buildc.exe")]:
        if cand and Path(cand).is_file():
            return str(Path(cand).resolve())  # the build runs in a temp folder, so a relative path breaks
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

    # PTX message passing (ptx_mp.mm): the causality axiom proves the fenced
    # program safe; a proof of the unfenced program must not get through.

    def test_accepts_the_fenced_message_passing_proof(self):
        rc, out = self.check(PTX_MP)
        self.assertEqual(rc, 0, out)
        self.assertIn("OK statements=29 proofs=1", out)

    def test_refuses_the_unfenced_proof_that_passes_program_order_as_fenced(self):
        proof = "tWx tRx tWx tWy tRy tRx mpu.1 mpu.2 mpu.3 ax-causality ax-visible"
        rc, out = self.check(PTX_MP + UNFENCED % proof)
        self.assertNotEqual(rc, 0, out)
        self.assertIn("hypothesis-mismatch mp-unfenced", out)

    def test_refuses_an_unfenced_proof_that_cites_a_missing_axiom(self):
        # ax-po-cause ("program order alone orders the threads") is not in the model.
        proof = "tWx tRx tWx tRx mpu.1 mpu.2 mpu.3 ax-po-cause ax-visible"
        rc, out = self.check(PTX_MP + UNFENCED % proof)
        self.assertNotEqual(rc, 0, out)
        self.assertIn("bad-proof-label mp-unfenced", out)

    def test_refuses_the_fenced_proof_when_the_causality_axiom_is_removed(self):
        start = PTX_MP.index("${\n  cau.1")
        end = PTX_MP.index("$}", start) + 2
        rc, out = self.check(PTX_MP[:start] + PTX_MP[end:])
        self.assertNotEqual(rc, 0, out)
        self.assertIn("mp-fenced", out)

    def test_refuses_the_fenced_proof_when_the_reader_has_no_fence(self):
        rc, out = self.check(PTX_MP.replace("  mpf.6 $e |- isfence Fb $.\n", ""))
        self.assertNotEqual(rc, 0, out)
        self.assertIn("mp-fenced", out)


if __name__ == "__main__":
    sys.exit(unittest.main())
