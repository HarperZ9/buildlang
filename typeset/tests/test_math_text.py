"""Math in the PDF text layer: glyph runs go out in source order, so extraction and the reading-order check agree.

A base with both scripts reads base, subscript, superscript (e_1^2 extracts as e12); a large operator with both
limits reads operator, lower limit, upper limit; a multiplication sign is a symbol, not a letter, so
1.6 \\times 10^{-15} compares as 1 6 10 15.
"""
import os
import shutil
import sys
import tempfile
import unittest

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
sys.path.insert(0, ROOT)
from buildtex.build import build  # noqa: E402
from buildtex.check import words  # noqa: E402
from engine.pdfread import read_text  # noqa: E402

CASES = [r"e_1^2", r"\sigma_{test}^2", r"x_{i}^{k+1}", r"\sum_{j=1}^{m} x_j", r"\int_{-\pi}^{\pi} f",
         r"1.6 \times 10^{-15}"]


def math_ir(folder, cases):
    lines = ["#buildlang.doc/v0", "meta\ttitle\tMath order", "theme\tpaper-plain"]
    for c in cases:
        # a space before the closing brace: "}}" inside a span is a literal brace
        lines.append(f"block\tp\tInline {{m:{c} }} then words.")
        lines.append(f"block\teq\t{c}")
    path = os.path.join(folder, "math.doc.tsv")
    with open(path, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("\n".join(lines) + "\n")
    return path


class MathTextOrder(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.mkdtemp(prefix="buildtex-math-")
        cls.receipt = build(math_ir(cls.tmp, CASES), os.path.join(cls.tmp, "out", "math"), "paper-plain", [],
                            html=False)
        cls.text = "".join(read_text(os.path.join(cls.tmp, "out", "math.pdf")))
        cls.flat = "".join(words(cls.text))

    @classmethod
    def tearDownClass(cls):
        shutil.rmtree(cls.tmp, ignore_errors=True)

    def test_reading_order_passes(self):
        self.assertTrue(self.receipt["facts"]["reading_order"], self.receipt["facts"]["blocks_missing_from_text"])
        self.assertEqual(self.receipt["failed"], [])

    def test_base_subscript_superscript(self):
        self.assertIn("Inlinee12then", self.flat)
        self.assertIn("test2", self.flat)
        self.assertIn("xik1", self.flat)

    def test_operator_lower_then_upper_limit(self):
        self.assertIn("j1mxj", self.flat)
        lower, upper = self.text.index("−π"), self.text.index("π", self.text.index("−π") + 2)
        self.assertLess(lower, upper)

    def test_times_is_not_a_letter(self):
        self.assertEqual(words("1.6 × 10−15 ÷ 2"), ["1", "6", "10", "15", "2"])
        self.assertIn("Inline161015then", self.flat)


class MathBoxOrder(unittest.TestCase):
    def test_script_glyph_order(self):
        from engine.mathlayout import MathLayout, parse, D, T
        from engine.sfnt import Font
        font = Font(os.path.join(ROOT, "data", "fonts", "NotoSansMath-Regular.ttf"), 0)
        ml = MathLayout(font, "math", 10.0)
        plain = lambda box: "".join(p.text for _, _, p in box.glyphs)
        self.assertEqual(plain(ml.layout(parse("e_1^2"), T)), "\U0001D452" + "12")
        self.assertEqual(plain(ml.layout(parse(r"\sum_{j}^{m}"), D)), "∑\U0001D457\U0001D45A")


if __name__ == "__main__":
    unittest.main()
