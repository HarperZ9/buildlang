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
    return re.findall(r"[0-9A-Za-zÀ-ÖØ-öø-ɏ]+", text)


def facts(pdf_path, result, doc):
    with open(pdf_path, "rb") as fh:
        data = fh.read()
    pdf = PDF(data)
    pages = [pdf.page_text(p) for p in pdf.pages()]
    flat = _letters(" ".join(pages))
    diag = result["layout"].diag
    doc = getattr(result["layout"], "doc", doc)   # the laid-out copy, with references and citations resolved
    # Reading order: every block's letters and digits appear in the PDF text, each block after the one before.
    # Letters, not words: a hyphenated line end, an inline variable before a suffix ("{m:n}th") and implicit
    # products in math ("b n") all join or split words without changing the characters a reader gets.
    pos, order_ok, lost = 0, True, []
    for block in doc.blocks:
        want = _letters(_plain_for_text(block))
        if not want:
            continue
        k = flat.find(want, pos)
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


def _plain_for_text(block):
    """A block's text as a reader extracts it: math source loses its commands (frac, sum and the rest), which
    print as layout or symbols rather than letters."""
    return "".join(re.sub(r"\\([A-Za-z]+)", _command_text, s.text) if s.style == "m" else s.text
                   for s in block.spans)


def _command_text(m):
    """A math command that prints as letters (ln, log) keeps them; the rest print as symbols or layout."""
    from engine.mathparse import SYMBOLS
    sym = SYMBOLS.get(m.group(1))
    return " " + sym[0] + " " if sym and sym[0].isalpha() else " "


def _letters(text):
    return "".join(words(text))


def same_text(pdf_a, pdf_b):
    """The words of two PDFs, page by page, compared: an embellished build and a plain build of one source must
    extract to the same text. Returns (equal, first difference or None)."""
    def pages(path):
        with open(path, "rb") as fh:
            pdf = PDF(fh.read())
        return [words(pdf.page_text(p)) for p in pdf.pages()]
    a, b = pages(pdf_a), pages(pdf_b)
    if len(a) != len(b):
        return False, f"{len(a)} pages against {len(b)}"
    for n, (wa, wb) in enumerate(zip(a, b), 1):
        if wa != wb:
            k = next((i for i, (x, y) in enumerate(zip(wa, wb)) if x != y), min(len(wa), len(wb)))
            return False, f"page {n}, word {k}: {wa[k:k + 6]} against {wb[k:k + 6]}"
    return True, None


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
