"""Render a buildlang.doc/v0 file: layout once, then PDF, SVG and HTML from the same pages.

    python -m engine.render DOC.tsv OUT_BASE [--theme NAME] [--annotations] [--plates]
"""
import argparse
import json
import os
import sys

from . import docir, pdfgfx
from .layout import Layout
from .pdfout import write_pdf
from .theme import load_theme


def furnish(layout, pages, theme):
    pg = theme["page"]
    for page in pages:
        page.left, page.top = pg["left"], pg["top"]
        page.paper = theme.get("paper", "#ffffff")
        page.furniture_under = []
    foot = theme.get("footer")
    if foot:
        from .footer import footer_lines
        for page in pages:
            page.furniture += footer_lines(layout, page, foot, len(pages))
    for layer in theme.get("art", []):
        from .art import art_layer
        for page in pages:
            g = art_layer(layer, page, theme)
            (page.furniture_under if layer.get("under", True) else page.furniture).extend(g)


def render(doc_path, out_base, theme_name=None, annotations=False, plates=False, formats=("pdf", "svg", "html")):
    doc = docir.load(doc_path)
    theme = load_theme(theme_name or doc.theme)
    theme["_seed_text"] = doc.meta.get("title", "")
    pdfgfx.reset()
    lay = Layout(doc, theme)
    pages = lay.run()
    furnish(lay, pages, theme)
    if annotations and doc.marks:
        from .annotate import overlay
        overlay(lay, pages, doc, theme)
    meta = {"title": doc.meta.get("title", ""), "author": doc.meta.get("author", "")}
    out = {"pages": len(pages), "diagnostics": lay.diag.report(), "files": []}
    if "pdf" in formats:
        write_pdf(out_base + ".pdf", pages, lay.reg, meta)
        out["files"].append(out_base + ".pdf")
    if "svg" in formats or "html" in formats:
        from .svgout import write_svg_pages
        out["files"] += write_svg_pages(out_base, pages, lay.reg, meta, html="html" in formats)
    if plates and theme.get("emboss"):
        from .emboss import export_plates
        out["files"] += export_plates(out_base, lay, pages, theme)
    out["lines"] = lay.line_log
    out["page_breaks"] = [p.breaks for p in pages]
    with open(out_base + ".layout.json", "w", encoding="utf-8") as fh:
        json.dump(out, fh, indent=1)
    return out, lay, pages


def main(argv=None):
    ap = argparse.ArgumentParser()
    ap.add_argument("doc")
    ap.add_argument("out")
    ap.add_argument("--theme")
    ap.add_argument("--annotations", action="store_true")
    ap.add_argument("--plates", action="store_true")
    a = ap.parse_args(argv)
    os.makedirs(os.path.dirname(os.path.abspath(a.out)), exist_ok=True)
    out, lay, _ = render(a.doc, a.out, a.theme, a.annotations, a.plates)
    print(f"{out['pages']} pages -> {', '.join(out['files'])}")
    for d in out["diagnostics"]:
        print("diagnostic:", d, file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
