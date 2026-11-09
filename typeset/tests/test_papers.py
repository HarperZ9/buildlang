"""BuildTeX papers: figures, tables, numbered equations, cross references and references in the paper theme.

What is held: the paper build passes every BuildTeX check; the embellished build (emblem, dot band, relief,
paper fill) extracts to the same words as a plain build of the same source; figures land in the PDF as images;
references, equation numbers and labels read as text with their spaces; a table row's cells come out in order;
the PDF names an author and a subject only when the document asks; the PNG reader agrees with the PNG writer.
"""
import os
import shutil
import struct
import sys
import tempfile
import unittest
import zlib

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
sys.path.insert(0, ROOT)
from buildtex.build import build  # noqa: E402
from buildtex.check import same_text, words  # noqa: E402
from engine.pdfread import PDF, read_text  # noqa: E402
from engine.pngread import read_png  # noqa: E402
from engine.raster import write_png  # noqa: E402

LOW_DPI = ["emboss.dpi=36"]
SEP = "¦"


def _bytes(path):
    with open(path, "rb") as fh:
        return fh.read()


def rgba_png(path, w, h):
    """An RGBA PNG with a transparent half, written by hand so the reader is tested on a type the writer lacks."""
    raw = b"".join(b"\x00" + b"".join(bytes((200, 40, 40, 255 if x < w // 2 else 0)) for x in range(w)) for _ in range(h))
    chunk = lambda tag, data: struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data))
    with open(path, "wb") as fh:
        fh.write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
                 + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b""))


def paper_ir(folder, author=True):
    rows = [b"".join(bytes((x * 6 % 256, 90, 160)) for x in range(40)) for _ in range(20)]
    write_png(os.path.join(folder, "chart.png"), 40, 20, rows)
    lines = ["#buildlang.doc/v0", "meta\ttitle\tA fictional paper on held-out checks",
             "meta\tsubject\tA test of the paper theme", "meta\tfooter\tA fictional paper, version 1.0"]
    if author:
        lines.append("meta\tpdf_author\tAvery Example")
    lines += ["theme\tpaper",
              "block\ttitle\tA fictional paper on held-out checks",
              "block\tsubtitle\tWhat a planted fault says about a passing test",
              "block\tauthor\tAvery Example",
              "block\tmeta\t9 November 2026, version 1.0",
              "block\tabstracthead\tAbstract",
              "block\tabstract\tWe plant 40 faults and count the ones a check misses {cite:knuth}.",
              "block\tsection\tMethod\tsec:method",
              "block\tp\tThe score {m:B = (p - o)^2} is averaged as in Equation {ref:eq:brier}; Figure {ref:fig:chart} "
              "and Table {ref:tab:counts} give the counts, and Section {ref:sec:method} the method.",
              "block\teq\tB = \\frac{1}{N} \\sum_{i} (p_i - o_i)^2\teq:brier",
              "block\tfigure\tA small chart drawn for the test.\tfig:chart\tsrc=chart.png;width=0.5;alt=bars",
              "block\ttablecap\tFaults planted and caught.\ttab:counts",
              f"block\tthead\tCheck{SEP}Planted{SEP}Caught\t\tcols=0.5,0.25,0.25",
              f"block\trow\tUnit tests{SEP}40{SEP}31",
              f"block\trow\tProperty tests with a longer name that wraps in its column{SEP}40{SEP}38\t\tlast=1",
              "block\tsection\tReferences",
              "block\tbib\tKnuth, D. E. (1984). The TeXbook. Addison-Wesley.\tknuth"]
    path = os.path.join(folder, "paper.doc.tsv")
    with open(path, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("\n".join(lines) + "\n")
    return path


class Papers(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.mkdtemp(prefix="buildtex-papers-")
        cls.src = paper_ir(cls.tmp)
        cls.paper = build(cls.src, os.path.join(cls.tmp, "out", "paper"), "paper", LOW_DPI, html=False)
        cls.plain = build(cls.src, os.path.join(cls.tmp, "out", "plain"), "paper-plain", [], html=False)
        cls.pdf = os.path.join(cls.tmp, "out", "paper.pdf")
        cls.text = " ".join(read_text(cls.pdf))

    @classmethod
    def tearDownClass(cls):
        shutil.rmtree(cls.tmp, ignore_errors=True)

    def test_builds_pass_their_checks(self):
        self.assertEqual(self.paper["failed"], [])
        self.assertEqual(self.plain["failed"], [])
        self.assertEqual(self.paper["external_programs"], [])

    def test_embellished_and_plain_builds_extract_the_same_words(self):
        same, diff = same_text(self.pdf, os.path.join(self.tmp, "out", "plain.pdf"))
        self.assertTrue(same, diff)

    def test_the_embellished_build_carries_its_decoration(self):
        self.assertGreater(self.paper["facts"]["images"], self.plain["facts"]["images"])

    def test_references_numbers_and_labels_read_as_text(self):
        w = words(self.text)
        runs = lambda n: [w[i:i + n] for i in range(len(w))]
        for phrase in ("as in Equation 1", "Figure 1 and Table 1", "and Section 1 the method", "1 Method",
                       "2 References", "Figure 1 A small chart", "Table 1 Faults planted", "1 Knuth D E",
                       "check misses 1"):
            self.assertIn(phrase.split(), runs(len(phrase.split())), phrase)

    def test_math_extracts_as_plain_letters_with_its_number(self):
        w = words(self.text)
        k = w.index("Faults") if "Faults" in w else len(w)
        self.assertIn(["B", "1", "N"], [w[i:i + 3] for i in range(k)])
        self.assertIn("(1)", self.text)

    def test_table_cells_come_out_in_row_order(self):
        w = words(self.text)
        self.assertIn(["Unit", "tests", "40", "31"], [w[i:i + 4] for i in range(len(w))])

    def test_figure_is_an_image(self):
        self.assertGreaterEqual(self.plain["facts"]["images"], 1)

    def test_author_and_subject_only_on_request(self):
        raw = _bytes(self.pdf)
        info = PDF(raw)
        self.assertIn(b"/Author (Avery Example)", raw)
        self.assertIn(b"/Subject (A test of the paper theme)", raw)
        folder = os.path.join(self.tmp, "anon")
        os.makedirs(folder)
        build(paper_ir(folder, author=False), os.path.join(folder, "out"), "paper-plain", [], html=False)
        self.assertNotIn(b"/Author", _bytes(os.path.join(folder, "out.pdf")))
        self.assertTrue(info.pages())

    def test_png_reader(self):
        path = os.path.join(self.tmp, "rgba.png")
        rgba_png(path, 4, 2)
        w, h, rgb = read_png(path, background=(10, 20, 30))
        self.assertEqual((w, h), (4, 2))
        self.assertEqual(rgb[:3], bytes((200, 40, 40)))      # opaque half keeps its colour
        self.assertEqual(rgb[9:12], bytes((10, 20, 30)))     # transparent half takes the background
        w, h, rgb = read_png(os.path.join(self.tmp, "chart.png"))
        self.assertEqual((w, h, len(rgb)), (40, 20, 40 * 20 * 3))
        self.assertEqual(rgb[3:6], bytes((6, 90, 160)))


if __name__ == "__main__":
    unittest.main()
