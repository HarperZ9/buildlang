"""Per-column table alignment: align=l,r,r on the first row of a table sets each column, and later rows keep it.

What is held: the numbers in a right-aligned column end at the column's right edge on every row, whatever their
width; a left column starts at the column's left edge; the setting does not leak into the next table; the
build still passes with the cells read in order.
"""
import os
import shutil
import sys
import tempfile
import unittest

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
sys.path.insert(0, ROOT)
from buildtex.build import build  # noqa: E402
from engine import docir  # noqa: E402
from engine.layout import Layout  # noqa: E402
from engine.theme import load_theme  # noqa: E402

SEP = "¦"


def table_ir(folder):
    lines = ["#buildlang.doc/v0", "meta\ttitle\tAligned table", "theme\tpaper-plain",
             "block\ttablecap\tCounts by check.\ttab:a",
             f"block\tthead\tCheck{SEP}Planted{SEP}Caught\t\tcols=0.5,0.25,0.25;align=l,r,r",
             f"block\trow\tUnit tests{SEP}40{SEP}7",
             f"block\trow\tProperty tests{SEP}1,240{SEP}1,038",
             f"block\trow\tFuzzing{SEP}3{SEP}0.25\t\tlast=1",
             "block\tp\tA paragraph between the two tables.",
             "block\ttablecap\tA second table with no alignment.\ttab:b",
             f"block\tthead\tName{SEP}Value\t\tcols=0.5,0.5",
             f"block\trow\tAlpha{SEP}1\t\tlast=1"]
    path = os.path.join(folder, "table.doc.tsv")
    with open(path, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("\n".join(lines) + "\n")
    return path


class TableAlign(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.mkdtemp(prefix="buildtex-align-")
        cls.src = table_ir(cls.tmp)
        doc = docir.load(cls.src)
        cls.theme = load_theme("paper-plain")
        cls.lay = Layout(doc, cls.theme)
        pages = cls.lay.run()
        cls.lines = [ln for page in pages for ln in _lines(page)]
        cls.receipt = build(cls.src, os.path.join(cls.tmp, "out", "table"), "paper-plain", [], html=False)

    @classmethod
    def tearDownClass(cls):
        shutil.rmtree(cls.tmp, ignore_errors=True)

    def cells(self, block):
        """[(text, x left, x right)] of the runs on block's line, one entry per run."""
        ln = next(ln for ln in self.lines if getattr(ln, "block", None) == block)
        return [("".join(pc.clusters), x, x + pc.width * hs) for x, _, pc, hs in ln.runs]

    def column_bounds(self, k, fracs):
        width = self.lay.text_width
        left = width * sum(fracs[:k]) / sum(fracs)
        right = width * sum(fracs[:k + 1]) / sum(fracs)
        gutter = self.theme["blocks"]["row"].get("gutter", 8.0) if k < len(fracs) - 1 else 0.0
        return left, right - gutter

    def test_right_columns_end_at_the_column_edge(self):
        fracs = [0.5, 0.25, 0.25]
        for block in (2, 3, 4):
            runs = self.cells(block)
            for k in (1, 2):
                start, edge = self.column_bounds(k, fracs)
                cell_runs = [r for r in runs if start - 0.01 <= r[1] < edge + 0.01]
                self.assertTrue(cell_runs, (block, k, runs))
                self.assertAlmostEqual(max(r[2] for r in cell_runs), edge, delta=0.05, msg=(block, k, runs))

    def test_left_column_starts_at_its_edge(self):
        for block in (2, 3, 4):
            self.assertAlmostEqual(self.cells(block)[0][1], 0.0, delta=0.05)

    def test_header_takes_the_alignment_too(self):
        runs = self.cells(1)
        _, edge = self.column_bounds(2, [0.5, 0.25, 0.25])
        self.assertAlmostEqual(max(r[2] for r in runs), edge, delta=0.05)

    def test_next_table_is_not_aligned(self):
        runs = self.cells(8)
        left, _ = self.column_bounds(1, [0.5, 0.5])
        value = [r for r in runs if r[0].strip() == "1"][0]
        self.assertAlmostEqual(value[1], left, delta=0.05)

    def test_build_passes(self):
        self.assertEqual(self.receipt["failed"], [])


def _lines(page):
    for item in page.items:
        ln = item[1] if isinstance(item, tuple) else item
        if hasattr(ln, "runs"):
            yield ln


if __name__ == "__main__":
    unittest.main()
