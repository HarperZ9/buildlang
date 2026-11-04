"""Layout facts for a BuildTeX build, read back from the files it wrote.

The facts come from two places that do not share code paths: the layout
report (what the line breaker and page builder decided) and the PDF bytes
(what a reader will get). A build passes when both agree on the page
count, no line is overfull, no glyph is missing, every font is embedded,
and every block's text comes back out of the PDF in source order.
"""
import re

from engine.pdfread import PDF


def fonts(pdf):
    """[(base font name, embedded?)] for every Type0 font in the file."""
    out = []
    for n, body in pdf.objs.items():
        head = body.split(b"stream\n", 1)[0]
        if b"/Type /Font" not in head or b"/Subtype /Type0" not in head:
            continue
        name = re.search(rb"/BaseFont /([^\s/]+)", head).group(1).decode()
        cid = int(re.search(rb"/DescendantFonts \[(\d+) 0 R\]", head).group(1))
        fd = pdf.ref(cid, b"FontDescriptor")
        fd_head = pdf.dict_of(fd) if fd else b""
        m = re.search(rb"/FontFile[23]? (\d+) 0 R", fd_head)
        embedded = bool(m) and len(pdf.stream(int(m.group(1)))) > 0
        out.append((name, embedded))
    return sorted(out)


def words(text):
    return re.findall(r"[0-9A-Za-zÀ-ɏ]+", text)


def facts(pdf_path, result, doc):
    with open(pdf_path, "rb") as fh:
        data = fh.read()
    pdf = PDF(data)
    pages = [pdf.page_text(p) for p in pdf.pages()]
    flat = words(" ".join(pages))
    diag = result["layout"].diag
    # Reading order: every block's words appear in the PDF text, each block after the one before.
    pos, order_ok, lost = 0, True, []
    for block in doc.blocks:
        want = words(block.plain)
        if not want:
            continue
        k = _find(flat, want, pos)
        if k < 0:
            order_ok = False
            lost.append(block.index)
        else:
            pos = k + len(want)
    return {
        "pages": len(pages),
        "pages_layout": result["pages"],
        "overfull": [{"block": b, "line": ln, "excess_pt": round(x, 3)} for b, ln, x in diag.overfull],
        "underfull": len(diag.underfull),
        "missing_glyphs": sorted({f"U+{ord(ch):04X}" for ch, _ in diag.missing}),
        "fonts": [{"name": n, "embedded": e} for n, e in fonts(pdf)],
        "images": len(re.findall(rb"/Subtype /Image", data)),
        "blind_text_runs": sum(pdf.stream(pdf.ref(p, b"Contents")).count(b"3 Tr") for p in pdf.pages()),
        "reading_order": order_ok,
        "blocks_missing_from_text": lost,
        "pdf_bytes": len(data),
    }


def _find(hay, needle, start):
    n = len(needle)
    for k in range(start, len(hay) - n + 1):
        if hay[k:k + n] == needle:
            return k
    return -1


def passed(f):
    """The list of failed conditions; empty when the build passes."""
    bad = []
    if f["pages"] != f["pages_layout"]:
        bad.append(f"PDF has {f['pages']} pages, layout made {f['pages_layout']}")
    if f["overfull"]:
        bad.append(f"{len(f['overfull'])} overfull line(s)")
    if f["missing_glyphs"]:
        bad.append("missing glyphs " + ", ".join(f["missing_glyphs"]))
    if not f["fonts"] or not all(x["embedded"] for x in f["fonts"]):
        bad.append("a font is not embedded")
    if not f["reading_order"]:
        bad.append(f"blocks {f['blocks_missing_from_text']} not found in reading order")
    return bad
