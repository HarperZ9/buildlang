"""An overfull line inside a table cell is reported by its table row and column, not as "line 1" of the block.

What is held: a word too wide for its column in the second body row, third column, comes back in the receipt
with row 2 and column 3, and the failure message names the row and column; the same in a header cell names the
header row; an overfull line in a paragraph still names its line.
"""
import os
import shutil
import sys
import tempfile
import unittest

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
sys.path.insert(0, ROOT)
from buildtex.build import build  # noqa: E402

SEP = "¦"
WIDE = "Counterrevolutionaries_of_the_planted_fault_registry"


class OverfullCell(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.mkdtemp(prefix="buildtex-overfull-")
        lines = ["#buildlang.doc/v0", "meta\ttitle\tOverfull", "theme\tpaper-plain",
                 "block\ttablecap\tA table with one cell too narrow for its word.\ttab:o",
                 f"block\tthead\tCheck{SEP}Planted{SEP}{WIDE}\t\tcols=0.6,0.2,0.2",
                 f"block\trow\tUnit tests{SEP}40{SEP}31",
                 f"block\trow\tProperty tests{SEP}40{SEP}{WIDE}",
                 f"block\trow\tFuzzing{SEP}3{SEP}1\t\tlast=1",
                 "block\tp\tA plain paragraph after the table, with nothing too wide in it."]
        src = os.path.join(cls.tmp, "o.doc.tsv")
        with open(src, "w", encoding="utf-8", newline="\n") as fh:
            fh.write("\n".join(lines) + "\n")
        cls.r = build(src, os.path.join(cls.tmp, "out", "o"), "paper-plain", [], html=False)
        cls.over = cls.r["facts"]["overfull"]

    @classmethod
    def tearDownClass(cls):
        shutil.rmtree(cls.tmp, ignore_errors=True)

    def test_body_cell_names_row_and_column(self):
        cell = [o for o in self.over if o["block"] == 3]
        self.assertEqual(len(cell), 1, self.over)
        self.assertEqual((cell[0]["row"], cell[0]["column"]), (2, 3))
        self.assertEqual(cell[0]["where"], "block 3, table row 2, column 3")

    def test_header_cell_names_the_header(self):
        head = [o for o in self.over if o["block"] == 1]
        self.assertEqual(len(head), 1, self.over)
        self.assertEqual((head[0]["row"], head[0]["column"]), ("header", 3))

    def test_failure_message_names_the_cells(self):
        msg = next(f for f in self.r["failed"] if "overfull" in f)
        self.assertIn("table row 2, column 3", msg)
        self.assertIn("the header row, column 3", msg)
        self.assertNotIn("line 1", msg)

    def test_paragraph_still_names_its_line(self):
        from engine.shape import overfull_where
        self.assertEqual(overfull_where((7, 2, 1.5)), "block 7 line 2")


if __name__ == "__main__":
    unittest.main()
