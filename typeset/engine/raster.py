"""A small coverage rasterizer and PNG writer (stdlib only).

Paths are flattened to polygons and filled with the nonzero rule, with
4x vertical supersampling and exact horizontal span coverage. Strokes
become one quad per segment plus round joins.
"""
import math
import struct
import zlib


def flatten(d, tol=0.25):
    polys, cur, cx, cy = [], [], 0.0, 0.0
    for c in d:
        op = c[0]
        if op == "M":
            if len(cur) > 1:
                polys.append(cur)
            cur = [(c[1], c[2])]
            cx, cy = c[1], c[2]
        elif op == "L":
            cur.append((c[1], c[2]))
            cx, cy = c[1], c[2]
        elif op in ("Q", "C"):
            pts = [(cx, cy)] + [(c[i], c[i + 1]) for i in range(1, len(c), 2)]
            n = 8
            for k in range(1, n + 1):
                t = k / n
                if op == "Q":
                    (x0, y0), (x1, y1), (x2, y2) = pts
                    x = (1 - t) ** 2 * x0 + 2 * (1 - t) * t * x1 + t * t * x2
                    y = (1 - t) ** 2 * y0 + 2 * (1 - t) * t * y1 + t * t * y2
                else:
                    (x0, y0), (x1, y1), (x2, y2), (x3, y3) = pts
                    a, b, cc, dd = (1 - t) ** 3, 3 * (1 - t) ** 2 * t, 3 * (1 - t) * t * t, t ** 3
                    x = a * x0 + b * x1 + cc * x2 + dd * x3
                    y = a * y0 + b * y1 + cc * y2 + dd * y3
                cur.append((x, y))
            cx, cy = cur[-1]
        elif op == "Z":
            if len(cur) > 1:
                polys.append(cur)
            cur = []
    if len(cur) > 1:
        polys.append(cur)
    return polys


def stroke_polys(d, width):
    out, hw = [], width / 2
    for poly in flatten(d):
        for (x0, y0), (x1, y1) in zip(poly, poly[1:]):
            dx, dy = x1 - x0, y1 - y0
            ln = math.hypot(dx, dy) or 1e-9
            nx, ny = -dy / ln * hw, dx / ln * hw
            out.append([(x0 + nx, y0 + ny), (x1 + nx, y1 + ny), (x1 - nx, y1 - ny), (x0 - nx, y0 - ny)])
    return out


class Canvas:
    def __init__(self, w, h):
        self.w, self.h = w, h
        self.px = [0.0] * (w * h)

    def fill(self, polys, value=1.0, ss=4):
        edges = []
        for poly in polys:
            for (x0, y0), (x1, y1) in zip(poly, poly[1:] + poly[:1]):
                if y0 != y1:
                    edges.append((x0, y0, x1, y1, 1 if y1 > y0 else -1))
        if not edges:
            return
        ymin = max(0, int(min(min(e[1], e[3]) for e in edges)))
        ymax = min(self.h - 1, int(max(max(e[1], e[3]) for e in edges)) + 1)
        for row in range(ymin, ymax + 1):
            acc = {}
            for s in range(ss):
                y = row + (s + 0.5) / ss
                xs = []
                for x0, y0, x1, y1, wdir in edges:
                    lo, hi = (y0, y1) if y0 < y1 else (y1, y0)
                    if lo <= y < hi:
                        xs.append((x0 + (y - y0) * (x1 - x0) / (y1 - y0), wdir))
                xs.sort()
                wind = 0
                for k in range(len(xs) - 1):
                    wind += xs[k][1]
                    if wind != 0:
                        a, b = xs[k][0], xs[k + 1][0]
                        ia, ib = max(0, int(a)), min(self.w - 1, int(b))
                        for i in range(ia, ib + 1):
                            cov = min(b, i + 1) - max(a, i)
                            if cov > 0:
                                acc[i] = acc.get(i, 0.0) + cov / ss
            base = row * self.w
            for i, c in acc.items():
                v = self.px[base + i]
                self.px[base + i] = max(v, min(1.0, c) * value)


def box_blur(px, w, h, r):
    if r < 1:
        return list(px)
    out = list(px)
    for _ in range(2):
        tmp = [0.0] * (w * h)
        for y in range(h):
            row, s = y * w, 0.0
            win = 2 * r + 1
            for x in range(-r, r + 1):
                s += out[row + min(max(x, 0), w - 1)]
            for x in range(w):
                tmp[row + x] = s / win
                s += out[row + min(x + r + 1, w - 1)] - out[row + max(x - r, 0)]
        for x in range(w):
            s = 0.0
            win = 2 * r + 1
            for y in range(-r, r + 1):
                s += tmp[min(max(y, 0), h - 1) * w + x]
            for y in range(h):
                out[y * w + x] = s / win
                s += tmp[min(y + r + 1, h - 1) * w + x] - tmp[max(y - r, 0) * w + x]
    return out


def write_png(path, w, h, rows, mode="RGB", bits=8):
    """rows: list of bytes per row. mode RGB (3 ch) or L (1 ch); bits 8 or 16."""
    ctype = {"RGB": 2, "L": 0}[mode]
    raw = b"".join(b"\0" + r for r in rows)

    def chunk(tag, data):
        c = tag + data
        return struct.pack(">I", len(data)) + c + struct.pack(">I", zlib.crc32(c) & 0xFFFFFFFF)

    png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, bits, ctype, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(raw, 6)) + chunk(b"IEND", b"")
    with open(path, "wb") as fh:
        fh.write(png)
