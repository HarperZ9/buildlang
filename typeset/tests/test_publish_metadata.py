"""BuildTeX output carries no personal metadata, so it passes a publish-metadata check.

Asserted on the fixture resume, whose \author line holds an address and an email:
no /Author, /Creator or XMP packet in the PDF; a date only from SOURCE_DATE_EPOCH;
--pdfa writes the PDF/A packet without the author; and no output file names the
build directory. The PDF rules mirror Flywheel's scripts/check_publish_metadata.py.
"""
import glob
import json
import os
import re
import shutil
import sys
import tempfile
import unittest
from unittest import mock

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
sys.path.insert(0, ROOT)
from buildtex.build import build  # noqa: E402
from engine.pdfout import source_date  # noqa: E402

RESUME = os.path.join(ROOT, "tests", "fixtures", "buildtex", "resume.tex")
PDF_STR = rb"(\((?:\.|[^\)])*\)|<[0-9A-Fa-f\s]*>)"
PDF_KEYS = re.compile(rb"/(Author|Creator)\s*" + PDF_STR)
XMP = re.compile(rb"<x:xmpmeta.*?</x:xmpmeta>", re.S)


def publish_findings(data):
    """The PDF findings the publish-metadata check would report."""
    found = [m.group(1).decode() for m in PDF_KEYS.finditer(data) if m.group(2)[1:-1].strip()]
    return found + (["XMP"] if XMP.search(data) else [])


class PublishMetadata(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.dir = tempfile.mkdtemp()

    @classmethod
    def tearDownClass(cls):
        shutil.rmtree(cls.dir, ignore_errors=True)

    def make(self, name, epoch=None, pdfa=False, html=False):
        env = {k: v for k, v in os.environ.items() if k != "SOURCE_DATE_EPOCH"}
        if epoch is not None:
            env["SOURCE_DATE_EPOCH"] = epoch
        out = os.path.join(self.dir, name)
        with mock.patch.dict(os.environ, env, clear=True):
            r = build(RESUME, out, "ats", [], html=html, pdfa=pdfa)
        with open(out + ".pdf", "rb") as fh:
            return r, fh.read()

    def test_default_pdf_has_no_author_creator_xmp_or_date(self):
        r, data = self.make("plain")
        self.assertEqual(publish_findings(data), [])
        self.assertNotIn(b"/CreationDate", data)
        self.assertNotIn(b"avery@example.org", data)
        self.assertEqual(r["pdf_metadata"], {"pdfa": False, "creation_date": None})
        self.assertEqual(r["failed"], [])

    def test_date_comes_from_source_date_epoch(self):
        r, data = self.make("dated", epoch="1700000000")
        self.assertIn(b"/CreationDate (D:20231114221320Z)", data)
        self.assertEqual(r["pdf_metadata"]["creation_date"], "D:20231114221320Z")
        _, again = self.make("dated-again", epoch="1700000000")
        self.assertEqual(data, again)

    def test_a_malformed_epoch_is_refused(self):
        with mock.patch.dict(os.environ, {"SOURCE_DATE_EPOCH": "yesterday"}):
            with self.assertRaises(ValueError):
                source_date()

    def test_pdfa_packet_has_no_author(self):
        r, data = self.make("pdfa", epoch="0", pdfa=True)
        packet = XMP.search(data).group(0)
        self.assertIn(b"pdfaid:part", packet)
        self.assertIn(b"1970-01-01T00:00:00Z", packet)
        self.assertNotIn(b"dc:creator", packet)
        self.assertNotIn(b"CreatorTool", packet)
        self.assertEqual(publish_findings(data), ["XMP"])
        self.assertTrue(r["pdf_metadata"]["pdfa"])

    def test_no_output_names_the_build_directory(self):
        self.make("paths", html=True)
        where = os.path.abspath(self.dir)
        forms = {where, where.replace("\\", "/"), where.replace("\\", "\\\\")}
        files = glob.glob(os.path.join(self.dir, "paths*"))
        self.assertGreaterEqual(len(files), 4)
        for path in files:
            with open(path, "rb") as fh:
                data = fh.read()
            for form in forms:
                self.assertNotIn(form.encode("utf-8"), data, os.path.basename(path))
        with open(os.path.join(self.dir, "paths.layout.json"), encoding="utf-8") as fh:
            self.assertIn("paths.pdf", json.load(fh)["files"])


if __name__ == "__main__":
    unittest.main()
