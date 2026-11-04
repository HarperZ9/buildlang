"""BuildTeX: a resume and a CV from LaTeX-subset source to checked PDF and HTML, with no TeX or Lean.

Layout facts asserted: page count, no overfull line, no missing glyph, every font embedded,
every block's text back out of the PDF in source order, the press relief inside the PDF,
byte-identical rebuilds, and no TeX or Lean program started anywhere.
"""
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
sys.path.insert(0, ROOT)
from buildtex import texin  # noqa: E402
from buildtex.build import build  # noqa: E402
from buildtex.guard import FORBIDDEN, ForbiddenBinary, no_tex_or_lean, program  # noqa: E402
from engine.pdfread import PDF, read_text  # noqa: E402

FIX = os.path.join(ROOT, "tests", "fixtures", "buildtex")
RESUME = os.path.join(FIX, "resume.tex")
LOW_DPI = ["emboss.dpi=36"]  # the relief at test resolution; the shipped theme uses 150


def cv_tex(path):
    """A fictional CV: the resume plus a long published-work appendix that must break across pages."""
    with open(RESUME, encoding="utf-8") as fh:
        head = fh.read().split(r"\end{document}")[0]
    out = [head, r"\newpage", r"\section{Published Work}", r"\subsection{Software releases}", r"\begin{itemize}"]
    for i in range(1, 61):
        out.append(r"  \item tool-%02d: %d releases from v0.1.0, June %d, 2026, to v1.%d.0, October %d, 2026. "
                   r"\url{example.org/tool-%02d/releases}" % (i, i % 7 + 1, i % 28 + 1, i, i % 28 + 1, i))
    out += [r"\end{itemize}", r"\subsection{Essays}", r"\begin{itemize}"]
    for i in range(1, 31):
        out.append(r"  \item ``Essay %d'' on checking results with a second reader, September %d, 2026. "
                   r"\url{example.org/essays/%d}" % (i, i % 28 + 1, i))
    out += [r"\end{itemize}", r"\end{document}", ""]
    with open(path, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("\n".join(out))
    return path


class TexImport(unittest.TestCase):
    def test_structure(self):
        imp = texin.Importer()
        with open(RESUME, encoding="utf-8") as fh:
            meta, blocks = imp.convert(fh.read())
        kinds = [k for k, _, _ in blocks]
        self.assertEqual(meta["title"], "Avery Example")
        self.assertEqual(kinds[0], "h1")
        self.assertEqual(kinds.count("contact"), 2)
        self.assertEqual(kinds.count("h2"), 5)
        self.assertEqual(kinds.count("h3"), 2)
        self.assertEqual(kinds.count("li"), 7)
        self.assertEqual(imp.unknown, set())

    def test_inline(self):
        imp = texin.Importer()
        self.assertEqual(imp.inline(r"\emph{a} \textbf{b} 2014--2024 ``q''"), "{i:a} {b:b} 2014\u20132024 \u201cq\u201d")
        self.assertEqual(imp.inline(r"\texttt{--json} \url{a--b}"), "{tt:--json} {tt:a--b}")
        self.assertEqual(imp.inline(r"$x^2$ and \href{https://x.org}{site}"), "{m:x^2} and site")
        self.assertEqual(imp.inline(r"100\% \& co\_op"), "100% & co_op")

    def test_unknown_commands_are_reported(self):
        imp = texin.Importer()
        imp.convert("\\begin{document}\\section{A}\\fancything{x} text\\end{document}")
        self.assertEqual(imp.unknown, {"fancything"})

    def test_unbalanced_group_is_an_error(self):
        with self.assertRaises(texin.TexImportError):
            texin.Importer().convert("\\begin{document}\\section{A\\end{document}")

    def test_program_round_trips_the_blocks(self):
        with open(RESUME, encoding="utf-8") as fh:
            meta, blocks = texin.Importer().convert(fh.read())
        prog = texin.to_program("resume", meta, blocks, "press")
        self.assertIn("mod doc;", prog)
        self.assertEqual(prog.count("blk(&mut d,"), len(blocks))


class Guard(unittest.TestCase):
    def test_program_names(self):
        self.assertEqual(program(["C:\\texlive\\bin\\pdflatex.exe", "x.tex"]), "pdflatex")
        self.assertEqual(program("lake build"), "lake")
        self.assertEqual(program([b"/usr/bin/lean"]), "lean")

    def test_tex_and_lean_are_refused(self):
        for name in ("pdflatex", "xelatex", "latexmk", "lean", "lake"):
            with no_tex_or_lean():
                with self.assertRaises(ForbiddenBinary):
                    subprocess.run([name, "--version"])
                with self.assertRaises(ForbiddenBinary):
                    os.system(name + " --version")

    def test_other_programs_are_recorded(self):
        with no_tex_or_lean() as started:
            subprocess.run([sys.executable, "-c", "pass"], check=True)
        self.assertEqual(len(started), 1)

    def test_no_source_names_a_tex_or_lean_program_in_a_call(self):
        call = re.compile(r"(?:subprocess\.\w+|os\.system|os\.exec\w*|os\.spawn\w*|Popen)\(\s*\[?\s*[\"']([\w.-]+)")
        for top in ("engine", "buildtex", "kp"):
            for dirpath, _, files in os.walk(os.path.join(ROOT, top)):
                for f in files:
                    if f.endswith(".py"):
                        with open(os.path.join(dirpath, f), encoding="utf-8") as fh:
                            for name in call.findall(fh.read()):
                                self.assertNotIn(program([name]), FORBIDDEN, f"{f} calls {name}")


class Builds(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.dir = tempfile.mkdtemp()
        cls.r = {}
        cv = cv_tex(os.path.join(cls.dir, "cv.tex"))
        jobs = {"resume-press": (RESUME, "press", LOW_DPI, True), "resume-ats": (RESUME, "ats", [], False),
                "cv-press": (cv, "press", LOW_DPI, False), "cv-ats": (cv, "ats", [], False)}
        for name, (src, theme, sets, html) in jobs.items():
            cls.r[name] = build(src, os.path.join(cls.dir, name), theme, sets, html=html)

    @classmethod
    def tearDownClass(cls):
        shutil.rmtree(cls.dir, ignore_errors=True)

    def test_page_counts(self):
        self.assertEqual(self.r["resume-press"]["facts"]["pages"], 1)
        self.assertEqual(self.r["resume-ats"]["facts"]["pages"], 1)
        for name in ("cv-press", "cv-ats"):
            f = self.r[name]["facts"]
            self.assertEqual(f["pages"], f["pages_layout"], name)
            self.assertGreaterEqual(f["pages"], 3, name)

    def test_every_build_passes_its_checks(self):
        for name, r in self.r.items():
            f = r["facts"]
            self.assertEqual(r["failed"], [], name)
            self.assertEqual(f["overfull"], [], name)
            self.assertEqual(f["missing_glyphs"], [], name)
            self.assertTrue(f["fonts"] and all(x["embedded"] for x in f["fonts"]), name)
            self.assertTrue(f["reading_order"], name)

    def test_no_external_program_ran(self):
        for name, r in self.r.items():
            self.assertEqual(r["external_programs"], [], name)

    def test_press_relief_is_in_the_pdf_and_the_name_is_text(self):
        f = self.r["resume-press"]["facts"]
        self.assertEqual(f["images"], 1)
        text = read_text(os.path.join(self.dir, "resume-press.pdf"))[0]
        self.assertTrue(text.startswith("Avery Example"))
        self.assertEqual(self.r["resume-ats"]["facts"]["images"], 0)

    def test_blind_finish_keeps_the_name_in_the_text_layer(self):
        out = os.path.join(self.dir, "blind")
        r = build(RESUME, out, "press", LOW_DPI + ["emboss.deboss_finish=blind"], html=False)
        self.assertEqual(r["facts"]["blind_text_runs"], 1)
        self.assertIn("Avery Example", read_text(out + ".pdf")[0])
        with open(out + ".pdf", "rb") as fh:
            pdf = PDF(fh.read())
        content = pdf.stream(pdf.ref(pdf.pages()[0], b"Contents"))
        self.assertIn(b"0 Tr", content)  # the render mode is reset, so later text is inked

    def test_resume_finish_marks_decoration_and_clears_the_body(self):
        from engine.render import render
        from engine.theme import load_theme
        out = os.path.join(self.dir, "press-resume")
        r = build(RESUME, out, "press-resume", LOW_DPI, html=False)
        self.assertEqual(r["failed"], [])
        with open(out + ".pdf", "rb") as fh:
            pdf = PDF(fh.read())
        self.assertIn(b"/Artifact BMC", pdf.stream(pdf.ref(pdf.pages()[0], b"Contents")))
        with open(RESUME, encoding="utf-8") as fh:
            meta, blocks = texin.Importer().convert(fh.read())
        ir = os.path.join(self.dir, "pr.tsv")
        with open(ir, "w", encoding="utf-8") as fh:
            fh.write(texin.to_ir(meta, blocks, "press-resume"))
        theme = load_theme("press-resume")
        theme["emboss"]["dpi"] = 36
        path = os.path.join(self.dir, "pr-theme.json")
        with open(path, "w", encoding="utf-8") as fh:
            json.dump(theme, fh)
        _, lay, pages = render(ir, os.path.join(self.dir, "pr"), path, formats=("pdf",))
        page = pages[0]
        relief = [g for g in page.furniture_under if g.get("role") == "relief"][0]
        first_h2 = min(page.top + y for y, line in page.items if lay.doc.blocks[line.block].kind == "h2")
        self.assertLess(relief["y"] + relief["h"], first_h2 - 10.6)  # clear of the heading's ascent

    def test_html_comes_from_the_same_layout(self):
        with open(os.path.join(self.dir, "resume-press.html"), encoding="utf-8") as fh:
            page = fh.read()
        self.assertIn("data:image/png;base64,", page)
        for phrase in ("Avery", "harnesses", "Riverside"):
            self.assertIn(phrase, page)

    def test_math_without_a_math_font_is_text_with_a_note(self):
        src = os.path.join(self.dir, "m.tsv")
        with open(src, "w", encoding="utf-8") as fh:
            fh.write("#buildlang.doc/v0\nmeta\ttitle\tM\ntheme\tats\nblock\tp\tA formula {m:x^2} here.\n")
        r = build(src, os.path.join(self.dir, "m"), "ats", [], html=False)
        self.assertEqual(r["failed"], [])
        self.assertTrue(any("no math font" in n for n in r["notes"]))

    def test_rebuild_is_byte_identical(self):
        out = os.path.join(self.dir, "again")
        r = build(RESUME, out, "ats", [], html=False)
        self.assertEqual(r["outputs"]["again.pdf"], self.r["resume-ats"]["outputs"]["resume-ats.pdf"])

    def test_receipt_on_disk(self):
        with open(os.path.join(self.dir, "resume-ats.buildtex.json"), encoding="utf-8") as fh:
            rec = json.load(fh)
        self.assertEqual(rec["schema"], "buildlang.buildtex-receipt/v0")
        self.assertEqual(rec["theme"], "ats")


if __name__ == "__main__":
    unittest.main()
