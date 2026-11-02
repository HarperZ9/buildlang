"""TrueType glyf outlines, composite glyphs and glyph-id closure for subsetting."""
from .sfnt import u16, s16, u32


def loca(font):
    if hasattr(font, "_loca"):
        return font._loca
    d, o = font.table("loca")
    n = font.num_glyphs + 1
    if font.loca_long:
        offs = [u32(d, o + 4 * i) for i in range(n)]
    else:
        offs = [2 * u16(d, o + 2 * i) for i in range(n)]
    font._loca = offs
    return offs


def glyph_range(font, gid):
    offs = loca(font)
    if gid + 1 >= len(offs):
        return 0, 0
    base = font.tables["glyf"][0]
    return base + offs[gid], offs[gid + 1] - offs[gid]


def components(font, gid):
    """Glyph ids referenced by a composite glyph (empty for simple glyphs)."""
    start, length = glyph_range(font, gid)
    d = font.data
    if length == 0 or s16(d, start) >= 0:
        return []
    out, p = [], start + 10
    while True:
        flags, child = u16(d, p), u16(d, p + 2)
        out.append(child)
        p += 4 + (4 if flags & 1 else 2)
        p += 2 if flags & 8 else 4 if flags & 0x40 else 8 if flags & 0x80 else 0
        if not flags & 0x20:
            return out


def read_glyph(font, gid, depth):
    start, length = glyph_range(font, gid)
    if length == 0 or depth > 8:
        return []
    d = font.data
    ncont = s16(d, start)
    if ncont >= 0:
        return _simple(d, start, ncont)
    contours, p = [], start + 10
    while True:
        flags, child = u16(d, p), u16(d, p + 2)
        p += 4
        if flags & 1:
            dx, dy = s16(d, p), s16(d, p + 2)
            p += 4
        else:
            dx, dy = (d[p] ^ 0x80) - 0x80, (d[p + 1] ^ 0x80) - 0x80
            p += 2
        a, b, c, e = 1.0, 0.0, 0.0, 1.0
        if flags & 8:
            a = e = s16(d, p) / 16384.0
            p += 2
        elif flags & 0x40:
            a, e = s16(d, p) / 16384.0, s16(d, p + 2) / 16384.0
            p += 4
        elif flags & 0x80:
            a, b, c, e = (s16(d, p + 2 * i) / 16384.0 for i in range(4))
            p += 8
        for contour in read_glyph(font, child, depth + 1):
            contours.append([(a * x + c * y + dx, b * x + e * y + dy, on) for x, y, on in contour])
        if not flags & 0x20:
            return contours


def _simple(d, start, ncont):
    ends = [u16(d, start + 10 + 2 * i) for i in range(ncont)]
    npts = ends[-1] + 1 if ends else 0
    p = start + 10 + 2 * ncont
    p += 2 + u16(d, p)
    flags = []
    while len(flags) < npts:
        f = d[p]
        p += 1
        flags.append(f)
        if f & 8:
            flags.extend([f] * d[p])
            p += 1
    flags = flags[:npts]
    xs, ys = [], []
    for coords, short, same in ((xs, 2, 16), (ys, 4, 32)):
        v = 0
        for f in flags:
            if f & short:
                delta = d[p]
                p += 1
                v += delta if f & same else -delta
            elif not f & same:
                v += s16(d, p)
                p += 2
            coords.append(v)
    contours, s = [], 0
    for e in ends:
        contours.append([(xs[i], ys[i], bool(flags[i] & 1)) for i in range(s, e + 1)])
        s = e + 1
    return contours


def contour_path(contour):
    """Turn a quadratic TrueType contour into M/Q/L path commands."""
    n = len(contour)
    if n == 0:
        return []
    pts = list(contour)
    if not pts[0][2]:
        if pts[-1][2]:
            pts = [pts[-1]] + pts[:-1]
        else:
            mid = ((pts[0][0] + pts[-1][0]) / 2, (pts[0][1] + pts[-1][1]) / 2, True)
            pts = [mid] + pts
    cmds = [("M", pts[0][0], pts[0][1])]
    i, n = 1, len(pts)
    while i <= n:
        cur = pts[i % n]
        if cur[2]:
            cmds.append(("L", cur[0], cur[1]))
            i += 1
            continue
        nxt = pts[(i + 1) % n]
        if nxt[2]:
            cmds.append(("Q", cur[0], cur[1], nxt[0], nxt[1]))
            i += 2
        else:
            mx, my = (cur[0] + nxt[0]) / 2, (cur[1] + nxt[1]) / 2
            cmds.append(("Q", cur[0], cur[1], mx, my))
            i += 1
    cmds.append(("Z",))
    return cmds
