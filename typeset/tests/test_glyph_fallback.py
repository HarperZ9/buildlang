"""Glyph fallback: a character the theme's text font lacks is set in a font shipped in data/fonts that has it.

What is held: Hanken Grotesk has no U+0151 (o with double acute) or U+0160 and U+0161 (S and s with caron); the
build sets them in a shipped font that has them, names that font in the receipt, embeds it, and passes; the text
extracts as typed.
A character no shipped font has stays a missing glyph and fails the build.
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
from engine.sfnt import Font  # noqa: E402


def one_para(folder, name, text):
    path = os.path.join(folder, name + ".doc.tsv")
    with open(path, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("\n".join(["#buildlang.doc/v0", "meta\ttitle\tFallback", "theme\tpaper-plain",
                            f"block\tp\t{text}"]) + "\n")
    return path


class GlyphFallback(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.mkdtemp(prefix="buildtex-fallback-")
        out = os.path.join(cls.tmp, "out")
        cls.ok = build(one_para(cls.tmp, "ok", "Erdős, Šimon and Dušek checked the planted faults."),
                       os.path.join(out, "ok"), "paper-plain", [], html=False)
        cls.text = "".join(read_text(os.path.join(out, "ok.pdf")))
        cls.bad = build(one_para(cls.tmp, "bad", "A character no shipped font has: 漢."),
                        os.path.join(out, "bad"), "paper-plain", [], html=False)

    @classmethod
    def tearDownClass(cls):
        shutil.rmtree(cls.tmp, ignore_errors=True)

    def test_the_text_font_lacks_them(self):
        hanken = Font(os.path.join(ROOT, "data", "fonts", "HankenGrotesk-Regular.ttf"), 0)
        self.assertFalse(hanken.glyph("ő"))
        self.assertFalse(hanken.glyph("š"))

    def test_fallback_sets_them_and_the_build_passes(self):
        self.assertEqual(self.ok["failed"], [])
        self.assertEqual(self.ok["facts"]["missing_glyphs"], [])
        self.assertIn("Erdős", self.text)
        self.assertIn("Šimon", self.text)

    def test_receipt_names_the_fallback(self):
        used = {f["char"]: f["font_used"] for f in self.ok["facts"]["glyph_fallbacks"]}
        self.assertEqual(set(used), {"U+0151", "U+0160", "U+0161"})
        for name in used.values():
            font = Font(os.path.join(ROOT, "data", "fonts", name), 0)
            self.assertTrue(font.glyph("ő") and font.glyph("Š"), name)
        self.assertTrue(any(n.startswith("glyph fallback: U+0151") for n in self.ok["notes"]), self.ok["notes"])
        self.assertTrue(all(f["embedded"] for f in self.ok["facts"]["fonts"]))
        self.assertGreater(len(self.ok["facts"]["fonts"]), 1)

    def test_no_shipped_font_stays_a_failure(self):
        self.assertIn("U+6F22", self.bad["facts"]["missing_glyphs"])
        self.assertTrue(any("missing glyphs" in f for f in self.bad["failed"]))
        self.assertEqual(self.bad["facts"]["glyph_fallbacks"], [])


if __name__ == "__main__":
    unittest.main()
