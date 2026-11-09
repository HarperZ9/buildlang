"""Math vocabulary: Greek (lower case italic, capitals upright), \\text, accents, named operators, and errors.

What is held: every command in the cases builds with no failure and no backslash in the text layer; capital
Greek is upright; \\text keeps its spaces and sets upright letters; \\bar draws a rule over the base and \\hat a
mark above it; \\cos, \\max and the rest are upright with a thin space before an ordinary symbol, and \\max
takes limits in display; a command outside the subset is an error in the receipt and prints nothing.
"""
import os
import shutil
import sys
import tempfile
import unittest

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
sys.path.insert(0, ROOT)
from buildtex.build import build  # noqa: E402
from engine.pdfread import read_text  # noqa: E402

CASES = [r"\eta + \kappa + \nu + \ell + \xi + \zeta + \chi", r"\Delta x = \Gamma \Theta \Lambda \Xi \Pi \Sigma \Upsilon "
         r"\Phi \Psi \Omega", r"r = 3 \text{ per run}", r"\bar{x} - \hat{y}", r"\cos\theta + \sin x + \tanh x",
         r"\exp(x) \max_{i} a_i \min_{j} b_j", r"\Delta t \le \frac{\ell}{\nu}"]


def ir(folder, cases, name="vocab"):
    lines = ["#buildlang.doc/v0", "meta\ttitle\tMath vocabulary", "theme\tpaper-plain"]
    for c in cases:
        lines.append(f"block\tp\tInline {{m:{c} }} then words.")
        lines.append(f"block\teq\t{c}")
    path = os.path.join(folder, name + ".doc.tsv")
    with open(path, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("\n".join(lines) + "\n")
    return path


def layout():
    from engine.mathlayout import MathLayout
    from engine.sfnt import Font
    font = Font(os.path.join(ROOT, "data", "fonts", "NotoSansMath-Regular.ttf"), 0)
    return font, MathLayout(font, "math", 10.0)


class Vocabulary(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.mkdtemp(prefix="buildtex-vocab-")
        cls.ok = build(ir(cls.tmp, CASES), os.path.join(cls.tmp, "out", "vocab"), "paper-plain", [], html=False)
        cls.text = "".join(read_text(os.path.join(cls.tmp, "out", "vocab.pdf")))
        cls.bad = build(ir(cls.tmp, [r"a \frobnicate b"], "bad"), os.path.join(cls.tmp, "out", "bad"),
                        "paper-plain", [], html=False)
        cls.bad_text = "".join(read_text(os.path.join(cls.tmp, "out", "bad.pdf")))

    @classmethod
    def tearDownClass(cls):
        shutil.rmtree(cls.tmp, ignore_errors=True)

    def test_known_commands_build_clean(self):
        self.assertEqual(self.ok["failed"], [])
        self.assertEqual(self.ok["facts"]["math_errors"], [])
        self.assertNotIn("\\", self.text)

    def test_greek_in_text_layer(self):
        for ch in "ηκνξζχ" + "ΔΓΘΛΞΠΣΥΦΨΩ":
            self.assertIn(ch, self.text)
        self.assertIn("ν + l + ξ", self.text)          # ell extracts as l (NFKC)

    def test_text_keeps_spaces(self):
        self.assertIn("3 per run", self.text)

    def test_operators_read_as_words(self):
        for name in ("cos", "sin", "tanh", "exp", "max", "min"):
            self.assertIn(name, self.text)

    def test_unknown_command_is_an_error_not_ink(self):
        errs = self.bad["facts"]["math_errors"]
        self.assertEqual([e["error"] for e in errs], ["unknown math command \\frobnicate"] * 2)
        self.assertTrue(any("math error" in f for f in self.bad["failed"]), self.bad["failed"])
        self.assertNotIn("frobnicate", self.bad_text)
        self.assertNotIn("\\", self.bad_text)


class Boxes(unittest.TestCase):
    def setUp(self):
        self.font, self.ml = layout()

    def test_capital_greek_upright(self):
        from engine.mathlayout import parse
        self.assertEqual(parse(r"\Delta")[0].nucleus, "Δ")
        self.assertEqual(parse(r"\eta")[0].nucleus, "\U0001D702")

    def test_text_is_upright_plain_letters(self):
        from engine.mathlayout import parse, T
        box = self.ml.layout(parse(r"\text{a b}"), T)
        self.assertEqual("".join(p.text for _, _, p in box.glyphs), "a b")
        self.assertEqual(box.glyphs[0][2].gids, [self.font.glyph("a")])

    def test_bar_is_a_rule_over_the_base(self):
        from engine.mathlayout import parse, T
        box = self.ml.layout(parse(r"\bar{x}"), T)
        x = self.ml.layout(parse("x"), T)
        (rx, ry, rw, rh), = box.rules
        self.assertGreater(ry, x.height)
        self.assertGreater(rw, 0)
        self.assertLessEqual(rx + rw, x.width + 1e-9)

    def test_hat_sits_above_after_the_base(self):
        from engine.mathlayout import parse, T
        box = self.ml.layout(parse(r"\hat{x}"), T)
        self.assertEqual([p.text for _, _, p in box.glyphs], ["\U0001D465", "̂"])
        self.assertGreater(box.height, self.ml.layout(parse("x"), T).height)

    def test_operator_upright_with_thin_space(self):
        from engine.mathlayout import parse, T
        box = self.ml.layout(parse(r"\cos x"), T)
        self.assertEqual([g[2].gids[0] for g in box.glyphs[:3]], [self.font.glyph(c) for c in "cos"])
        op_end = box.glyphs[2][0] + box.glyphs[2][2].width
        self.assertAlmostEqual(box.glyphs[3][0] - op_end, 3 * 10.0 / 18, places=6)

    def test_max_takes_limits_in_display(self):
        from engine.mathlayout import parse, D, T
        disp = self.ml.layout(parse(r"\max_{i}"), D)
        sub = [g for g in disp.glyphs if g[2].text == "\U0001D456"][0]
        self.assertLess(sub[1], -disp.glyphs[0][2].size * 0.3)      # below the baseline, not a subscript drop
        inline = self.ml.layout(parse(r"\max_{i}"), T)
        sub_t = [g for g in inline.glyphs if g[2].text == "\U0001D456"][0]
        self.assertGreater(sub_t[0], inline.glyphs[0][0] + inline.glyphs[0][2].width - 1e-9)

    def test_errors_listed(self):
        from engine.mathlayout import parse
        errs = []
        self.assertEqual(parse(r"x \nope y \frac{1}", errs)[0].nucleus, "\U0001D465")
        self.assertIn("unknown math command \\nope", errs)
        self.assertIn("a command or script is missing its argument", errs)


if __name__ == "__main__":
    unittest.main()
