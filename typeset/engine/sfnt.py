"""OpenType (sfnt) reader: metrics, cmap, kerning, ligatures, outlines.

Dependency-free. Reads TrueType-flavoured fonts (glyf) and TrueType
collections. Kerning comes from GPOS PairPos (formats 1 and 2, through
Extension lookups) and falls back to the legacy kern table. Ligatures come
from GSUB LigatureSubst. Outlines come from glyf, including composites.
The MATH table reader lives in mathfont.py.
"""
import struct

U16 = struct.Struct(">H")
S16 = struct.Struct(">h")
U32 = struct.Struct(">I")


def u16(b, o):
    return U16.unpack_from(b, o)[0]


def s16(b, o):
    return S16.unpack_from(b, o)[0]


def u32(b, o):
    return U32.unpack_from(b, o)[0]


class FontError(Exception):
    pass


class Font:
    def __init__(self, path, index=0, data=None):
        self.path = path
        if data is None:
            with open(path, "rb") as fh:
                data = fh.read()
        self.data = data
        base = 0
        if self.data[:4] == b"ttcf":
            base = u32(self.data, 12 + 4 * index)
        self.tables = {}
        n = u16(self.data, base + 4)
        for i in range(n):
            rec = base + 12 + 16 * i
            tag = self.data[rec:rec + 4].decode("latin-1")
            self.tables[tag] = (u32(self.data, rec + 8), u32(self.data, rec + 12))
        for need in ("head", "hhea", "maxp", "hmtx", "cmap"):
            if need not in self.tables:
                raise FontError(f"{path}: missing required table {need}")
        self._read_head()
        self._read_hmtx()
        self.cmap = self._read_cmap()
        self._read_os2()
        self.name = self._read_name(6) or "Unnamed"
        self.kern_pairs, self.kern_classes = {}, []
        self._read_gpos_kern()
        if not self.kern_pairs and not self.kern_classes:
            self._read_kern_table()
        self.ligatures = self._read_gsub_liga()
        self._glyph_cache = {}

    def table(self, tag):
        if tag not in self.tables:
            return None, 0
        off, length = self.tables[tag]
        return self.data, off

    def table_bytes(self, tag):
        off, length = self.tables[tag]
        return self.data[off:off + length]

    # -- basic tables -------------------------------------------------
    def _read_head(self):
        d, o = self.table("head")
        self.upem = u16(d, o + 18)
        self.bbox = tuple(s16(d, o + 36 + 2 * i) for i in range(4))
        self.loca_long = s16(d, o + 50) == 1
        d, o = self.table("hhea")
        self.ascent, self.descent, self.line_gap = s16(d, o + 4), s16(d, o + 6), s16(d, o + 8)
        self.num_hmetrics = u16(d, o + 34)
        d, o = self.table("maxp")
        self.num_glyphs = u16(d, o + 4)

    def _read_hmtx(self):
        d, o = self.table("hmtx")
        adv = [u16(d, o + 4 * i) for i in range(self.num_hmetrics)]
        last = adv[-1] if adv else 0
        self.advances = adv + [last] * (self.num_glyphs - len(adv))
        lsb = [s16(d, o + 4 * i + 2) for i in range(self.num_hmetrics)]
        rest = o + 4 * self.num_hmetrics
        lsb += [s16(d, rest + 2 * i) for i in range(self.num_glyphs - self.num_hmetrics)]
        self.lsbs = lsb

    def _read_os2(self):
        self.cap_height = int(self.ascent * 0.7)
        self.x_height = int(self.ascent * 0.5)
        self.fs_type = 0
        self.weight = 400
        if "OS/2" in self.tables:
            d, o = self.table("OS/2")
            version = u16(d, o)
            self.weight = u16(d, o + 4)
            self.fs_type = u16(d, o + 8)
            self.typo_ascender = s16(d, o + 68)
            self.typo_descender = s16(d, o + 70)
            if version >= 2:
                self.x_height = s16(d, o + 86)
                self.cap_height = s16(d, o + 88)
        self.italic_angle = 0.0
        if "post" in self.tables:
            d, o = self.table("post")
            self.italic_angle = struct.unpack_from(">i", d, o + 4)[0] / 65536.0
            self.is_fixed = u32(d, o + 12) != 0

    def _read_name(self, name_id):
        if "name" not in self.tables:
            return None
        d, o = self.table("name")
        count, strings = u16(d, o + 2), o + u16(d, o + 4)
        for i in range(count):
            r = o + 6 + 12 * i
            pid, eid, nid = u16(d, r), u16(d, r + 2), u16(d, r + 6)
            ln, off = u16(d, r + 8), u16(d, r + 10)
            if nid != name_id:
                continue
            raw = d[strings + off:strings + off + ln]
            if pid == 3 or pid == 0:
                return raw.decode("utf-16-be", "replace")
            return raw.decode("latin-1")
        return None

    def _read_cmap(self):
        from .cmap import read_cmap
        return read_cmap(self)

    # -- layout tables ------------------------------------------------
    def _lookups(self, tag, feature, want_type, ext_type):
        """Yield subtable offsets of lookups reached from `feature`."""
        if tag not in self.tables:
            return
        d, o = self.table(tag)
        flist, llist = o + u16(d, o + 6), o + u16(d, o + 8)
        idx = set()
        for i in range(u16(d, flist)):
            r = flist + 2 + 6 * i
            if d[r:r + 4] == feature.encode():
                f = flist + u16(d, r + 4)
                for j in range(u16(d, f + 2)):
                    idx.add(u16(d, f + 4 + 2 * j))
        for li in sorted(idx):
            lk = llist + u16(d, llist + 2 + 2 * li)
            ltype = u16(d, lk)
            for s in range(u16(d, lk + 4)):
                st = lk + u16(d, lk + 6 + 2 * s)
                t = ltype
                if ltype == ext_type:
                    t = u16(d, st + 2)
                    st = st + u32(d, st + 4)
                if t == want_type:
                    yield st

    def _coverage(self, off):
        d = self.data
        fmt, out = u16(d, off), []
        if fmt == 1:
            out = [u16(d, off + 4 + 2 * i) for i in range(u16(d, off + 2))]
        else:
            for i in range(u16(d, off + 2)):
                r = off + 4 + 6 * i
                out.extend(range(u16(d, r), u16(d, r + 2) + 1))
        return out

    def _classdef(self, off):
        d, cls = self.data, {}
        fmt = u16(d, off)
        if fmt == 1:
            start = u16(d, off + 2)
            for i in range(u16(d, off + 4)):
                cls[start + i] = u16(d, off + 6 + 2 * i)
        else:
            for i in range(u16(d, off + 2)):
                r = off + 4 + 6 * i
                for g in range(u16(d, r), u16(d, r + 2) + 1):
                    cls[g] = u16(d, r + 4)
        return cls

    def _value_size(self, vf):
        return 2 * bin(vf & 0xFF).count("1")

    def _x_adv(self, off, vf):
        """XAdvance from a ValueRecord (bit 0x4), skipping earlier fields."""
        if not vf & 0x4:
            return 0
        skip = 2 * bin(vf & 0x3).count("1")
        return s16(self.data, off + skip)

    def _read_gpos_kern(self):
        d = self.data
        for st in self._lookups("GPOS", "kern", 2, 9):
            fmt, cov = u16(d, st), self._coverage(st + u16(d, st + 2))
            vf1, vf2 = u16(d, st + 4), u16(d, st + 6)
            s1, s2 = self._value_size(vf1), self._value_size(vf2)
            if fmt == 1:
                for i, first in enumerate(cov):
                    ps = st + u16(d, st + 10 + 2 * i)
                    for k in range(u16(d, ps)):
                        r = ps + 2 + k * (2 + s1 + s2)
                        pair = (first, u16(d, r))
                        if pair not in self.kern_pairs:
                            self.kern_pairs[pair] = self._x_adv(r + 2, vf1)
            elif fmt == 2:
                c1 = self._classdef(st + u16(d, st + 8))
                c2 = self._classdef(st + u16(d, st + 10))
                n1, n2 = u16(d, st + 12), u16(d, st + 14)
                rec, table = s1 + s2, []
                for a in range(n1):
                    row = st + 16 + a * n2 * rec
                    table.append([self._x_adv(row + b * rec, vf1) for b in range(n2)])
                self.kern_classes.append((set(cov), c1, c2, table))

    def _read_kern_table(self):
        if "kern" not in self.tables:
            return
        d, o = self.table("kern")
        if u16(d, o) != 0:
            return
        sub = o + 4
        for _ in range(u16(d, o + 2)):
            length, cov = u16(d, sub + 2), u16(d, sub + 4)
            if cov >> 8 == 0:
                for k in range(u16(d, sub + 6)):
                    r = sub + 14 + 6 * k
                    self.kern_pairs[(u16(d, r), u16(d, r + 2))] = s16(d, r + 4)
            sub += length

    def _read_gsub_liga(self):
        d, ligs = self.data, {}
        for st in self._lookups("GSUB", "liga", 4, 7):
            cov = self._coverage(st + u16(d, st + 2))
            for i, first in enumerate(cov):
                ls = st + u16(d, st + 6 + 2 * i)
                for k in range(u16(d, ls)):
                    lg = ls + u16(d, ls + 2 + 2 * k)
                    lig, n = u16(d, lg), u16(d, lg + 2)
                    comps = tuple([first] + [u16(d, lg + 4 + 2 * j) for j in range(n - 1)])
                    ligs.setdefault(comps, lig)
        return ligs

    # -- queries ------------------------------------------------------
    def glyph(self, ch):
        return self.cmap.get(ord(ch), 0)

    def advance(self, gid):
        return self.advances[gid] if gid < self.num_glyphs else 0

    def kern(self, left, right):
        v = self.kern_pairs.get((left, right))
        if v is not None:
            return v
        for cov, c1, c2, table in self.kern_classes:
            if left in cov:
                a, b = c1.get(left, 0), c2.get(right, 0)
                if a < len(table) and b < len(table[a]) and table[a][b]:
                    return table[a][b]
        return 0

    def outline(self, gid):
        """Glyph outline as a list of contours of (x, y, on_curve) points."""
        if gid in self._glyph_cache:
            return self._glyph_cache[gid]
        from .glyf import read_glyph
        pts = read_glyph(self, gid, 0)
        self._glyph_cache[gid] = pts
        return pts
