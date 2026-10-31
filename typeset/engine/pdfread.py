"""A small PDF reader for round-trip checks of our own output (stdlib only).

Reads the classic xref table, inflates streams, follows the page tree,
decodes text through each font's ToUnicode CMap, and returns the text of
each page in content-stream order. It reads what pdfout writes; it is not
a general PDF parser.
"""
import re
import zlib

OBJ = re.compile(rb"(\d+) 0 obj\n(.*?)\nendobj", re.S)


class PDF:
    def __init__(self, data):
        self.data = data
        self.objs = {}
        for m in OBJ.finditer(data):
            self.objs[int(m.group(1))] = m.group(2)
        start = int(re.search(rb"startxref\n(\d+)", data).group(1))
        if not data[start:start + 4] == b"xref":
            raise ValueError("startxref does not point at an xref table")
        trailer = data[data.index(b"trailer", start):]
        self.root = int(re.search(rb"/Root (\d+) 0 R", trailer).group(1))

    def dict_of(self, n):
        body = self.objs[n]
        return body.split(b"stream\n", 1)[0]

    def stream(self, n):
        body = self.objs[n]
        head, rest = body.split(b"stream\n", 1)
        raw = rest.rsplit(b"\nendstream", 1)[0]
        return zlib.decompress(raw) if b"/FlateDecode" in head else raw

    def ref(self, n, key):
        m = re.search(rb"/" + key + rb" (\d+) 0 R", self.dict_of(n))
        return int(m.group(1)) if m else None

    def pages(self):
        pages_n = self.ref(self.root, b"Pages")
        kids = re.search(rb"/Kids \[([^\]]*)\]", self.dict_of(pages_n)).group(1)
        return [int(k) for k in re.findall(rb"(\d+) 0 R", kids)]

    def fonts(self, page):
        d = self.dict_of(page)
        m = re.search(rb"/Font << (.*?) >>", d)
        out = {}
        for name, ref in re.findall(rb"/(F\d+) (\d+) 0 R", m.group(1) if m else b""):
            tu = self.ref(int(ref), b"ToUnicode")
            out[name.decode()] = cmap(self.stream(tu)) if tu else {}
        return out

    def page_text(self, page):
        fonts = self.fonts(page)
        content = self.stream(self.ref(page, b"Contents")).decode("latin-1")
        out, font = [], None
        last_y = None
        for tok in re.finditer(r"/(F\d+) [\d.]+ Tf|1 0 0 1 [-\d.]+ ([-\d.]+) Tm|\[(.*?)\] TJ|<([0-9a-fA-F]+)> Tj", content):
            if tok.group(1):
                font = fonts.get(tok.group(1), {})
            elif tok.group(2):
                y = float(tok.group(2))
                if last_y is not None and abs(y - last_y) > 0.5:
                    out.append("\n")
                last_y = y
            else:
                body = tok.group(3) if tok.group(3) is not None else "<" + tok.group(4) + ">"
                for hexs in re.findall(r"<([0-9a-fA-F]+)>", body):
                    for k in range(0, len(hexs), 4):
                        out.append(font.get(int(hexs[k:k + 4], 16), "�"))
        return "".join(out)


def cmap(data):
    text = data.decode("latin-1")
    out = {}
    for code, uni in re.findall(r"<([0-9A-Fa-f]{4})> <([0-9A-Fa-f]+)>", text):
        out[int(code, 16)] = bytes.fromhex(uni).decode("utf-16-be")
    return out


def read_text(path):
    with open(path, "rb") as fh:
        pdf = PDF(fh.read())
    return [pdf.page_text(p) for p in pdf.pages()]
