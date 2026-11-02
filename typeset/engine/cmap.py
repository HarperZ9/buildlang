"""cmap subtables 4 and 12: Unicode code point to glyph id."""
from .sfnt import u16, s16, u32, FontError


def read_cmap(self):
    d, o = self.table("cmap")
    best = None
    for i in range(u16(d, o + 2)):
        r = o + 4 + 8 * i
        pid, eid, sub = u16(d, r), u16(d, r + 2), o + u32(d, r + 4)
        fmt = u16(d, sub)
        rank = {12: 3, 4: 2}.get(fmt, 0) if pid in (0, 3) else 0
        if rank and (best is None or rank > best[0]):
            best = (rank, fmt, sub)
    if best is None:
        raise FontError(f"{self.path}: no Unicode cmap (format 4 or 12)")
    _, fmt, sub = best
    cmap = {}
    if fmt == 4:
        segx2 = u16(d, sub + 6)
        ends, starts = sub + 14, sub + 16 + segx2
        deltas, ranges = starts + segx2, starts + 2 * segx2
        for s in range(segx2 // 2):
            end, start = u16(d, ends + 2 * s), u16(d, starts + 2 * s)
            delta, ro = s16(d, deltas + 2 * s), u16(d, ranges + 2 * s)
            for c in range(start, end + 1):
                if c == 0xFFFF:
                    continue
                if ro == 0:
                    g = (c + delta) & 0xFFFF
                else:
                    g = u16(d, ranges + 2 * s + ro + 2 * (c - start))
                    g = (g + delta) & 0xFFFF if g else 0
                if g:
                    cmap[c] = g
    else:
        for k in range(u32(d, sub + 12)):
            r = sub + 16 + 12 * k
            start, end, g = u32(d, r), u32(d, r + 4), u32(d, r + 8)
            for c in range(start, end + 1):
                cmap[c] = g + (c - start)
    return cmap
