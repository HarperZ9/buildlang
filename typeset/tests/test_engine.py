"""Hyphenation, math boxes, PDF round trip, planted errors, BuildLang cross-check."""
import os
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
sys.path.insert(0, ROOT)
from engine import docir  # noqa: E402
from engine.hyph import Hyphenator  # noqa: E402
from engine.render import render  # noqa: E402
from engine.pdfread import read_text  # noqa: E402

WINFONTS = os.path.join(os.environ.get("SystemRoot", ""), "Fonts")
CAMBRIA = os.path.join(WINFONTS, "cambria.ttc")
ARIAL = os.path.join(WINFONTS, "arial.ttf")


def _read(path):
    with open(path, "rb") as fh:
        return fh.read()


def write_ir(path, blocks, theme="parity", marks=()):
    lines = ["#buildlang.doc/v0", "meta\ttitle\tTest", f"theme\t{theme}"]
    lines += [f"block\t{k}\t{t}" for k, t in blocks]
    lines += ["mark\t" + "\t".join(map(str, m)) for m in marks]
    with open(path, "w", encoding="utf-8") as fh:
        fh.write("\n".join(lines) + "\n")


class Hyphenation(unittest.TestCase):
    def test_tex_answers(self):
        h = Hyphenator()
        # plain TeX with \lefthyphenmin=2 \righthyphenmin=3
        self.assertEqual(h.hyphenate("hyphenation"), "hy-phen-ation")
        self.assertEqual(h.hyphenate("computer"), "com-puter")
        self.assertEqual(h.hyphenate("independent"), "in-de-pen-dent")
        self.assertEqual(h.hyphenate("algorithm"), "al-go-rithm")
        self.assertEqual(h.hyphenate("cat"), "cat")

    def test_exception(self):
        h = Hyphenator()
        h.add_exception("ta-ble")
        self.assertEqual(h.points("table"), [2])


@unittest.skipUnless(os.path.exists(CAMBRIA), "Cambria Math not installed")
class MathBoxes(unittest.TestCase):
    def setUp(self):
        from engine.sfnt import Font
        from engine.mathlayout import MathLayout
        self.font = Font(CAMBRIA, 1)
        self.ml = MathLayout(self.font, "math", 10.0)
        self.k = lambda name: self.ml.c[name] * 10.0 / self.font.upem

    def test_superscript_shift(self):
        from engine.mathlayout import parse, T
        box = self.ml.layout(parse("x^2"), T)
        two = [g for g in box.glyphs if g[2].text == "2"][0]
        sup = self.ml.layout(parse("2"), 2)
        want = max(self.k("SuperscriptShiftUp"), sup.depth + self.k("SuperscriptBottomMin"))
        self.assertAlmostEqual(two[1], want, places=6)
        self.assertAlmostEqual(two[2].size, 10.0 * self.ml.c["ScriptPercentScaleDown"] / 100, places=6)

    def test_fraction_rule_on_axis(self):
        from engine.mathlayout import parse, D
        box = self.ml.layout(parse(r"\frac{a}{b}"), D)
        (x, y, w, h), = box.rules
        self.assertAlmostEqual(y + h / 2, self.k("AxisHeight"), places=6)
        self.assertAlmostEqual(h, self.k("FractionRuleThickness"), places=6)
        num = [g for g in box.glyphs if g[2].text == "\U0001D44E"][0]
        self.assertGreaterEqual(num[1] - self.k("AxisHeight") - h / 2, self.k("FractionNumDisplayStyleGapMin") - 1e-9)

    def test_bin_becomes_ord(self):
        from engine.mathlayout import parse, _fix_bins, ORD
        self.assertEqual(_fix_bins(parse("-x"))[0].cls, ORD)

    def test_radical_clearance(self):
        from engine.mathlayout import parse, T
        box = self.ml.layout(parse(r"\sqrt{x}"), T)
        (_, ry, _, rh), = box.rules
        body = self.ml.layout(parse("x"), T)
        self.assertAlmostEqual(ry, body.height + self.k("RadicalVerticalGap"), places=6)


@unittest.skipUnless(os.path.exists(ARIAL), "Arial not installed")
class Output(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.mkdtemp()

    def tearDown(self):
        shutil.rmtree(self.dir, ignore_errors=True)

    def test_pdf_round_trip(self):
        src = os.path.join(self.dir, "t.tsv")
        words = "Knuth and Plass break a paragraph as a whole, so the spacing stays even."
        write_ir(src, [("h1", "Round Trip"), ("p", words), ("li", "A bullet line.")])
        render(src, os.path.join(self.dir, "t"), formats=("pdf",))
        text = read_text(os.path.join(self.dir, "t.pdf"))
        self.assertEqual(len(text), 1)
        flat = " ".join(text[0].split())
        self.assertIn("Round Trip", flat)
        self.assertIn(words, flat)
        self.assertIn("\u2022 A bullet line.", flat)
        data = _read(os.path.join(self.dir, "t.pdf"))
        self.assertIn(b"/FontFile2", data)
        self.assertIn(b"pdfaid:part", data)

    def test_missing_glyph_reported(self):
        src = os.path.join(self.dir, "m.tsv")
        write_ir(src, [("p", "Arial has no \u4e2d here.")])
        out, _, _ = render(src, os.path.join(self.dir, "m"), formats=("pdf",))
        self.assertTrue(any("U+4E2D" in d for d in out["diagnostics"]), out["diagnostics"])

    def test_overfull_reported(self):
        src = os.path.join(self.dir, "o.tsv")
        write_ir(src, [("p", "x" * 160)], theme="living")
        out, _, _ = render(src, os.path.join(self.dir, "o"), formats=("pdf",))
        self.assertTrue(any("overfull" in d for d in out["diagnostics"]), out["diagnostics"])

    def test_unbalanced_span_is_an_error(self):
        src = os.path.join(self.dir, "u.tsv")
        write_ir(src, [("p", "an {i:open span")])
        with self.assertRaises(docir.IRError):
            docir.load(src)

    def test_annotation_layer_off_leaves_document_unchanged(self):
        src = os.path.join(self.dir, "a.tsv")
        write_ir(src, [("h1", "Plain"), ("p", "A plain resume line.")], marks=())
        render(src, os.path.join(self.dir, "a1"), formats=("pdf",))
        render(src, os.path.join(self.dir, "a2"), annotations=True, formats=("pdf",))
        a = _read(os.path.join(self.dir, "a1.pdf"))
        b = _read(os.path.join(self.dir, "a2.pdf"))
        self.assertEqual(a, b)


KP_EXE = os.path.join(ROOT, "kp", "knuth_plass.exe")
PAPER = os.path.join(ROOT, "proof", "docs", "paper-proof-packets.doc.tsv")


@unittest.skipUnless(os.path.exists(KP_EXE) and os.path.exists(PAPER), "BuildLang KP binary or paper IR missing")
class BuildLangKnuthPlass(unittest.TestCase):
    def test_same_breaks_as_python(self):
        sys.path.insert(0, os.path.join(ROOT, "kp"))
        from crosscheck import main
        agree, total = main(PAPER, "latex-article", KP_EXE)
        self.assertGreater(total, 10)
        self.assertEqual(agree, total)


if __name__ == "__main__":
    unittest.main()
