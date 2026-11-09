"""Floating figures and tables: a float that does not fit where it stands goes to the top of the next page.

What is held: with floats on (the default), a page that ended more than a third empty in front of a tall figure
now fills with the text that follows; the figure leads the next page; figures and tables keep their numbers and
their order; a float is never set before its first reference; a table stays whole with its caption; the
reading-order check passes against the order a reader meets the blocks. With "floats": false the old behaviour
is kept, which is how the test shows the page that used to end a third empty.
"""
import os
import shutil
import sys
import tempfile
import unittest

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
sys.path.insert(0, ROOT)
from buildtex.build import build  # noqa: E402
from buildtex.check import reading_order  # noqa: E402
from engine import docir  # noqa: E402
from engine.layout import Layout  # noqa: E402
from engine.pdfread import read_text  # noqa: E402
from engine.raster import write_png  # noqa: E402
from engine.theme import load_theme  # noqa: E402

SEP = "¦"
WORDS = ("A planted fault is a change to the program that a good check should catch, and the count of planted "
         "faults a check misses is the plainest measure of what a passing check is worth. ")


def para(n, k):
    return f"block\tp\tParagraph {n}. " + WORDS * k


def float_ir(folder):
    write_png(os.path.join(folder, "tall.png"), 40, 26, [bytes((x * 6 % 256, 90, 160)) * 1 * 40 for x in range(26)])
    lines = ["#buildlang.doc/v0", "meta\ttitle\tFloats", "theme\tpaper-plain",
             para(1, 9), para(2, 6),
             "block\tp\tFigure {ref:fig:a} shows the counts; Table {ref:tab:t} lists them.",
             "block\tfigure\tA tall chart that does not fit in the space left.\tfig:a\tsrc=tall.png;width=1.0;alt=bars",
             para(4, 6), para(5, 6),
             "block\tfigure\tA second chart, cited only further on.\tfig:b\tsrc=tall.png;width=0.6;alt=bars",
             para(6, 8),
             "block\tp\tFigure {ref:fig:b} is cited here, after the place it stands in the source.",
             "block\ttablecap\tFaults planted and caught, with a caption long enough to wrap onto a second line of "
             "the table caption so the caption and table must stay together.\ttab:t",
             f"block\tthead\tCheck{SEP}Planted{SEP}Caught\t\tcols=0.5,0.25,0.25;align=l,r,r",
             f"block\trow\tUnit tests{SEP}40{SEP}31", f"block\trow\tProperty tests{SEP}40{SEP}38\t\tlast=1",
             para(7, 6), para(8, 6)]
    path = os.path.join(folder, "floats.doc.tsv")
    with open(path, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("\n".join(lines) + "\n")
    return path


def lay_out(src, floats=True):
    theme = load_theme("paper-plain")
    theme["floats"] = floats
    lay = Layout(docir.load(src), theme)
    return lay, lay.run()


def fill(page, lay):
    return max(y + ln.depth for y, ln in page.items) / lay.text_height


def page_of(pages, block):
    return next(n for n, p in enumerate(pages) if any(ln.block == block for _, ln in p.items))


class Floats(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.mkdtemp(prefix="buildtex-floats-")
        cls.src = float_ir(cls.tmp)
        cls.doc = docir.load(cls.src)
        cls.idx = {b.label: b.index for b in cls.doc.blocks if b.label}
        cls.old, cls.old_pages = lay_out(cls.src, floats=False)
        cls.lay, cls.pages = lay_out(cls.src)
        cls.receipt = build(cls.src, os.path.join(cls.tmp, "out", "floats"), "paper-plain", [], html=False)
        cls.text = "\n".join(read_text(os.path.join(cls.tmp, "out", "floats.pdf")))

    @classmethod
    def tearDownClass(cls):
        shutil.rmtree(cls.tmp, ignore_errors=True)

    def test_page_that_was_a_third_empty_now_fills(self):
        self.assertLess(fill(self.old_pages[0], self.old), 2 / 3)       # the old builder left it more than 1/3 empty
        self.assertGreater(fill(self.pages[0], self.lay), 0.9)
        self.assertLessEqual(len(self.pages), len(self.old_pages))

    def test_figure_leads_the_next_page(self):
        fig = self.idx["fig:a"]
        n = page_of(self.pages, fig)
        self.assertEqual(n, 1)
        first_y = min(y for y, _ in self.pages[n].items)
        fig_y = min(y for y, ln in self.pages[n].items if ln.block == fig)
        self.assertLessEqual(fig_y, first_y + 1e-6)

    def test_numbers_and_order_hold(self):
        a, b, t = self.text.find("Figure 1."), self.text.find("Figure 2."), self.text.find("Table 1.")
        self.assertTrue(0 <= a < b, (a, b))
        self.assertGreaterEqual(t, 0)
        self.assertEqual(self.receipt["failed"], [])
        self.assertTrue(self.receipt["facts"]["reading_order"])

    def test_float_after_its_first_reference(self):
        for label, cite in (("fig:a", "Figure 1 shows"), ("fig:b", "Figure 2 is cited")):
            self.assertLess(self.text.find(cite), self.text.find("Figure " + label[-1] == "a" and "Figure 1." or
                                                                  "Figure 2."), label)
        ref_b = next(b.index for b in self.doc.blocks if "is cited here" in b.plain)
        self.assertGreaterEqual(page_of(self.pages, self.idx["fig:b"]), page_of(self.pages, ref_b))

    def test_table_whole_with_its_caption(self):
        cap = self.idx["tab:t"]
        pages = {page_of(self.pages, k) for k in range(cap, cap + 4)}
        self.assertEqual(len(pages), 1)
        cap_lines = [ln for p in self.pages for _, ln in p.items if ln.block == cap]
        self.assertGreaterEqual(len(cap_lines), 2)

    def test_only_floats_move(self):
        moved = {b for blocks, _ in self.lay.float_moves for b in blocks}
        self.assertTrue(moved)
        self.assertTrue(moved <= {self.idx["fig:a"], self.idx["fig:b"]} | set(range(self.idx["tab:t"],
                                                                                   self.idx["tab:t"] + 4)))
        order = [b.index for b in reading_order(self.doc.blocks, self.lay.float_moves) if b.index not in moved]
        self.assertEqual(order, sorted(order))


class TallTableCaption(unittest.TestCase):
    """A table taller than a page stays in the text and breaks between rows, but its two-line caption never
    splits and never leaves its header: tried with the caption starting at every line near a page foot."""

    def test_caption_stays_with_its_table(self):
        tmp = tempfile.mkdtemp(prefix="buildtex-tall-")
        try:
            for k in range(30, 52):
                lines = ["#buildlang.doc/v0", "meta\ttitle\tTall", "theme\tpaper-plain"]
                lines += [f"block\tp\tLine {n} of the text before the table." for n in range(k)]
                lines += ["block\tsubsection\tA heading kept with what follows",
                          "block\ttablecap\tA tall table whose caption is long enough to wrap onto a second line "
                          "in the caption style, so a page break could fall between its two lines.\ttab:tall",
                          f"block\tthead\tName{SEP}Value\t\tcols=0.6,0.4"]
                lines += [f"block\trow\tRow {n}{SEP}{n}" for n in range(70)] + [f"block\trow\tEnd{SEP}0\t\tlast=1"]
                src = os.path.join(tmp, f"tall{k}.doc.tsv")
                with open(src, "w", encoding="utf-8", newline="\n") as fh:
                    fh.write("\n".join(lines) + "\n")
                lay, pages = lay_out(src)
                self.assertEqual(lay.float_moves, [])          # too tall to float: it stays where it is
                cap = k + 1
                where = {page_of(pages, cap), page_of(pages, cap + 1)}
                cap_pages = {n for n, p in enumerate(pages) for _, ln in p.items if ln.block == cap}
                self.assertEqual(len([ln for p in pages for _, ln in p.items if ln.block == cap]), 2, k)
                self.assertEqual(len(cap_pages), 1, k)
                self.assertEqual(len(where), 1, k)
        finally:
            shutil.rmtree(tmp, ignore_errors=True)


if __name__ == "__main__":
    unittest.main()
