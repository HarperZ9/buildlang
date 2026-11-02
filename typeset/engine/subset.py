"""TrueType subsetter for PDF embedding.

Glyph ids are kept (CIDToGIDMap Identity), so unused glyphs become empty
entries in glyf. Composite components are closed over.
"""
import struct

from .glyf import components, glyph_range

KEEP = ("cvt ", "fpgm", "prep", "hhea", "hmtx", "maxp", "OS/2")


def closure(font, gids):
    todo, seen = list(gids) + [0], set()
    while todo:
        g = todo.pop()
        if g in seen or g >= font.num_glyphs:
            continue
        seen.add(g)
        todo.extend(components(font, g))
    return seen


def _checksum(b):
    b = b + b"\0" * ((4 - len(b) % 4) % 4)
    return sum(struct.unpack(">%dI" % (len(b) // 4), b)) & 0xFFFFFFFF


def subset_ttf(font, used):
    keep = closure(font, used)
    glyf, offs = bytearray(), []
    for g in range(font.num_glyphs):
        offs.append(len(glyf))
        if g in keep:
            start, length = glyph_range(font, g)
            glyf += font.data[start:start + length]
            glyf += b"\0" * ((4 - len(glyf) % 4) % 4)
    offs.append(len(glyf))
    tables = {"glyf": bytes(glyf), "loca": struct.pack(">%dI" % len(offs), *offs)}
    head = bytearray(font.table_bytes("head"))
    head[8:12] = b"\0\0\0\0"
    head[50:52] = struct.pack(">h", 1)
    tables["head"] = bytes(head)
    for tag in KEEP:
        if tag in font.tables:
            tables[tag] = font.table_bytes(tag)
    tables["post"] = struct.pack(">IiHhIIIII", 0x00030000, 0, 0, 0, 0, 0, 0, 0, 0)
    out = _assemble(tables)
    total = _checksum(out)
    hoff = _table_offset(out, b"head")
    out = bytearray(out)
    out[hoff + 8:hoff + 12] = struct.pack(">I", (0xB1B0AFBA - total) & 0xFFFFFFFF)
    return bytes(out), keep


def _table_offset(blob, tag):
    n = struct.unpack_from(">H", blob, 4)[0]
    for i in range(n):
        r = 12 + 16 * i
        if blob[r:r + 4] == tag:
            return struct.unpack_from(">I", blob, r + 8)[0]
    raise KeyError(tag)


def _assemble(tables):
    tags = sorted(tables)
    n = len(tags)
    es = n.bit_length() - 1
    sr = (1 << es) * 16
    header = struct.pack(">IHHHH", 0x00010000, n, sr, es, n * 16 - sr)
    offset = 12 + 16 * n
    dirs, body = b"", b""
    for t in tags:
        data = tables[t]
        dirs += struct.pack(">4sIII", t.encode("latin-1"), _checksum(data), offset + len(body), len(data))
        body += data + b"\0" * ((4 - len(data) % 4) % 4)
    return header + dirs + body
