"""PDF writer: subset embedded fonts, searchable text in reading order, PDF/A-2b intent.

Text is written line by line, top to bottom, left to right, so text
extraction (pdftotext, ATS parsers) reads the document in order. Every
glyph code maps back to its source characters through ToUnicode.
"""
import hashlib
import zlib

from .icc import srgb_profile
from .subset import subset_ttf
from . import pdfgfx


def _hex(b):
    return "<" + b.hex() + ">"


def _pdfstr(s):
    if any(ord(c) > 126 for c in s):
        return "<FEFF" + s.encode("utf-16-be").hex().upper() + ">"
    return "(" + s.replace("\\", "\\\\").replace("(", "\\(").replace(")", "\\)") + ")"


class Writer:
    def __init__(self):
        self.objs = []

    def reserve(self):
        self.objs.append(None)
        return len(self.objs)

    def put(self, n, body):
        self.objs[n - 1] = body if isinstance(body, bytes) else body.encode("latin-1")

    def add(self, body):
        n = self.reserve()
        self.put(n, body)
        return n

    def stream(self, data, extra="", compress=True):
        if compress:
            data = zlib.compress(data, 9)
            extra += " /Filter /FlateDecode"
        return self.add(f"<< /Length {len(data)}{extra} >>\nstream\n".encode() + data + b"\nendstream")

    def serialize(self, root, info, ident):
        out = bytearray(b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n")
        offs = []
        for i, body in enumerate(self.objs, 1):
            offs.append(len(out))
            out += f"{i} 0 obj\n".encode() + body + b"\nendobj\n"
        xref = len(out)
        out += f"xref\n0 {len(self.objs) + 1}\n0000000000 65535 f \n".encode()
        for o in offs:
            out += f"{o:010d} 00000 n \n".encode()
        out += (f"trailer\n<< /Size {len(self.objs) + 1} /Root {root} 0 R /Info {info} 0 R "
                f"/ID [<{ident}> <{ident}>] >>\nstartxref\n{xref}\n%%EOF\n").encode()
        return bytes(out)


class FontUse:
    def __init__(self, key, font):
        self.key, self.font = key, font
        self.used = {}        # gid -> text
        self.name = "F%d" % 0


def write_pdf(path, pages, registry, meta, plates=None, graphics_only=False, pdfa=True):
    w = Writer()
    uses = {}
    contents = []
    for page in pages:
        ops = []
        for g in page.furniture_under if hasattr(page, "furniture_under") else []:
            ops.append(pdfgfx.draw(g, page.height))
        if not graphics_only:
            for y, line in page.items:
                ops.append(_line_ops(line, y, page, registry, uses))
        for g in page.furniture:
            ops.append(pdfgfx.draw(g, page.height, registry, uses, _line_ops))
        contents.append("\n".join(o for o in ops if o))
    pages_n = w.reserve()
    font_refs = {}
    for i, (key, use) in enumerate(sorted(uses.items())):
        use.name = "F%d" % (i + 1)
        font_refs[use.name] = _embed_font(w, use)
    gs = pdfgfx.extgstates(w)
    font_dict = " ".join(f"/{n} {r} 0 R" for n, r in font_refs.items())
    res = f"<< /Font << {font_dict} >> /ExtGState {gs} /ColorSpace {pdfgfx.colorspaces(w)} >>"
    kids = []
    for page, ops in zip(pages, contents):
        for key, use in uses.items():
            ops = ops.replace(f"/@{key}@ ", f"/{use.name} ")
        c = w.stream(ops.encode("latin-1"))
        kids.append(w.add(f"<< /Type /Page /Parent {pages_n} 0 R /MediaBox [0 0 {page.width:.2f} {page.height:.2f}] "
                          f"/Resources {res} /Contents {c} 0 R >>"))
    w.put(pages_n, f"<< /Type /Pages /Kids [{' '.join(f'{k} 0 R' for k in kids)}] /Count {len(kids)} >>")
    title = meta.get("title", "")
    author = meta.get("author", "")
    xmp = _xmp(title, author, pdfa).encode("utf-8")
    md = w.stream(xmp, " /Type /Metadata /Subtype /XML", compress=False)
    icc = srgb_profile()
    icc_n = w.stream(icc, " /N 3")
    intent = w.add(f"<< /Type /OutputIntent /S /GTS_PDFA1 /OutputConditionIdentifier (sRGB) "
                   f"/Info (sRGB IEC61966-2.1) /DestOutputProfile {icc_n} 0 R >>")
    root = w.add(f"<< /Type /Catalog /Pages {pages_n} 0 R /Metadata {md} 0 R /OutputIntents [{intent} 0 R] "
                 f"/MarkInfo << /Marked false >> /Lang (en-US) >>")
    info = w.add(f"<< /Title {_pdfstr(title)} /Author {_pdfstr(author)} /Producer (buildlang typeset) "
                 f"/Creator (buildlang typeset) /CreationDate (D:20261031000000Z) >>")
    ident = hashlib.md5("\n".join(contents).encode("latin-1", "replace")).hexdigest()
    data = w.serialize(root, info, ident)
    with open(path, "wb") as fh:
        fh.write(data)
    return data


def _line_ops(line, y, page, registry, uses, left=None):
    pg_left = page.left if hasattr(page, "left") else 0
    base_y = page.height - (page.top if hasattr(page, "top") else 0) - y
    out = ["BT"]
    cur = None
    color = None
    prev_end = None
    for x, dy, pc, hs in line.runs:
        use = uses.setdefault(pc.font, FontUse(pc.font, registry[pc.font]))
        font = use.font
        space = font.glyph(" ")
        if prev_end is not None and space and x - prev_end > 0.12 * pc.size and cur:
            use_prev = uses[cur[0]]
            sg = use_prev.font.glyph(" ")
            if sg:
                use_prev.used.setdefault(sg, " ")
                out.append(f"1 0 0 1 {pg_left + prev_end:.3f} {base_y + dy:.3f} Tm <{sg:04x}> Tj")
        for g, c in zip(pc.gids, pc.clusters):
            use.used.setdefault(g, c)
        col = pc.color or "0 0 0"
        if col != color:
            out.append(f"{col} rg")
            color = col
        state = (pc.font, pc.size, round(hs, 4))
        if state != cur:
            out.append(f"/@{pc.font}@ {pc.size:.4f} Tf {100 * hs:.3f} Tz")
            cur = state
        out.append(f"1 0 0 1 {pg_left + x:.3f} {base_y + dy:.3f} Tm")
        scale = pc.size / font.upem
        parts, buf = [], b""
        for g, adv in zip(pc.gids, pc.advs):
            buf += g.to_bytes(2, "big")
            natural = font.advance(g) * scale
            if abs(adv - natural) > 1e-4:
                parts.append(_hex(buf))
                parts.append(f"{-(adv - natural) * 1000 / pc.size:.2f}")
                buf = b""
        if buf:
            parts.append(_hex(buf))
        out.append("[" + " ".join(parts) + "] TJ")
        prev_end = x + pc.width * hs
    out.append("ET")
    for rx, ry, rw, rh in line.graphics:
        out.append(f"0 0 0 rg {pg_left + rx:.3f} {base_y + ry:.3f} {rw:.3f} {rh:.3f} re f")
    return "\n".join(out)


def _embed_font(w, use):
    font = use.font
    gids = sorted(use.used)
    tag = hashlib.sha1((font.name + ",".join(map(str, gids))).encode()).hexdigest()
    prefix = "".join(chr(65 + int(c, 16) % 26) for c in tag[:6])
    base = f"{prefix}+{font.name.replace(' ', '')}"
    scale = 1000 / font.upem
    widths = " ".join(f"{g} [{font.advance(g) * scale:.1f}]" for g in gids)
    asc, desc = font.ascent * scale, font.descent * scale
    bbox = " ".join(f"{v * scale:.0f}" for v in font.bbox)
    flags = 4 | (64 if font.italic_angle else 0)
    if getattr(font, "kind", "ttf") == "cff":
        ff = w.stream(font.program, " /Subtype /CIDFontType0C", compress=True)
        ffkey, sub, c2g = "FontFile3", "CIDFontType0", ""
    else:
        prog, _ = subset_ttf(font, gids)
        ff = w.stream(prog, f" /Length1 {len(prog)}")
        ffkey, sub, c2g = "FontFile2", "CIDFontType2", " /CIDToGIDMap /Identity"
    fd = w.add(f"<< /Type /FontDescriptor /FontName /{base} /Flags {flags} /FontBBox [{bbox}] "
               f"/ItalicAngle {font.italic_angle:.1f} /Ascent {asc:.0f} /Descent {desc:.0f} "
               f"/CapHeight {font.cap_height * scale:.0f} /StemV 80 /{ffkey} {ff} 0 R >>")
    cid = w.add(f"<< /Type /Font /Subtype /{sub} /BaseFont /{base} /CIDSystemInfo << /Registry (Adobe) "
                f"/Ordering (Identity) /Supplement 0 >> /FontDescriptor {fd} 0 R /W [{widths}] /DW 0{c2g} >>")
    tu = w.stream(_tounicode(use.used).encode("latin-1"))
    return w.add(f"<< /Type /Font /Subtype /Type0 /BaseFont /{base} /Encoding /Identity-H "
                 f"/DescendantFonts [{cid} 0 R] /ToUnicode {tu} 0 R >>")


def _tounicode(used):
    lines = ["/CIDInit /ProcSet findresource begin", "12 dict begin", "begincmap",
             "/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def",
             "/CMapName /Adobe-Identity-UCS def", "/CMapType 2 def",
             "1 begincodespacerange", "<0000> <FFFF>", "endcodespacerange"]
    items = sorted(used.items())
    for k in range(0, len(items), 100):
        chunk = items[k:k + 100]
        lines.append(f"{len(chunk)} beginbfchar")
        for g, text in chunk:
            lines.append(f"<{g:04X}> <{text.encode('utf-16-be').hex().upper()}>")
        lines.append("endbfchar")
    lines += ["endcmap", "CMapName currentdict /CMap defineresource pop", "end", "end"]
    return "\n".join(lines)


def _xmp(title, author, pdfa=True):
    esc = lambda s: s.replace("&", "&amp;").replace("<", "&lt;")
    return f"""<?xpacket begin="﻿" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
<rdf:Description rdf:about="" xmlns:pdfaid="http://www.aiim.org/pdfa/ns/id/" xmlns:dc="http://purl.org/dc/elements/1.1/"
 xmlns:pdf="http://ns.adobe.com/pdf/1.3/" xmlns:xmp="http://ns.adobe.com/xap/1.0/">
{"<pdfaid:part>2</pdfaid:part><pdfaid:conformance>B</pdfaid:conformance>" if pdfa else ""}
<dc:title><rdf:Alt><rdf:li xml:lang="x-default">{esc(title)}</rdf:li></rdf:Alt></dc:title>
<dc:creator><rdf:Seq><rdf:li>{esc(author)}</rdf:li></rdf:Seq></dc:creator>
<pdf:Producer>buildlang typeset</pdf:Producer><xmp:CreatorTool>buildlang typeset</xmp:CreatorTool>
<xmp:CreateDate>2026-10-31T00:00:00Z</xmp:CreateDate>
</rdf:Description></rdf:RDF></x:xmpmeta>
<?xpacket end="w"?>"""
