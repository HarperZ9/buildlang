"""Embossing and debossing as a height map, shown lit on screen and exported as print plates.

Height map: the emboss shapes rise, the deboss shapes (glyph outlines of
chosen blocks) sink, each edge softened over a bevel. The screen render
lights that relief on a paper fibre normal map; foil areas take a metal
colour from the materials library (spectral Fresnel of gold, from
measured n and k), with a specular lobe. Plates are separate vector PDFs,
one Separation colour each: the emboss die mask, the deboss die mask, the
foil, and the spot colour.
"""
import math
import os
import sys

from . import pdfgfx
from .glyf import contour_path
from .raster import Canvas, flatten, stroke_polys, box_blur, write_png

MATERIALS = os.environ.get("MATERIALS_DIR", "")  # superframe packages/materials; optional


def metal_lut(material="gold"):
    """sRGB (0-1) of a metal's specular reflectance at 0..85 degrees, from materials.optics."""
    try:
        if MATERIALS not in sys.path:
            sys.path.insert(0, MATERIALS)
        import materials as M
        from materials import optics, spectral
        rec = M.get(material)
        n, k = optics.nk(rec)
        lut = []
        for deg in range(0, 90, 5):
            c = math.cos(math.radians(deg))
            spec = [abs(optics.fresnel(complex(n[i], k[i]), c)) for i in range(len(n))]
            rgb = spectral.reflectance_to_srgb(spec)
            lut.append(tuple(v / 255 if max(rgb) > 1.5 else v for v in rgb))
        return lut, "materials.optics fresnel(n,k) for " + material
    except Exception as e:  # the library is optional; report and fall back
        print(f"emboss: materials library unavailable ({e}); using a fixed gold", file=sys.stderr)
        return [(0.83, 0.69, 0.22)] * 18, "fallback constant"


def shapes(layout, pages, theme):
    spec = theme["emboss"]
    page = pages[0]
    up, down, foil, spot = [], [], [], []
    for g in getattr(page, "furniture_under", []) + page.furniture:
        role = g.get("role")
        if g.get("type") != "path":
            continue
        if role in spec.get("emboss_roles", []) and g.get("emboss"):
            up.append(g)
        if role in spec["foil"]["roles"]:
            foil.append(g)
        if g.get("plate"):
            spot.append(g)
    kinds = set(spec.get("deboss_blocks", []))
    for y, line in page.items:
        if layout.doc.blocks[line.block].kind in kinds:
            down += glyph_paths(layout, line, page.left, page.top + y)
    return up, down, foil, spot


def glyph_paths(layout, line, left, base):
    out = []
    for x, dy, pc, hs in line.runs:
        font = layout.reg[pc.font]
        s = pc.size / font.upem
        gx = left + x
        for gid, adv in zip(pc.gids, pc.advs):
            d = []
            for contour in font.outline(gid):
                for c in contour_path(contour):
                    if c[0] == "Z":
                        d.append(("Z",))
                    elif c[0] in "ML":
                        d.append((c[0], gx + c[1] * s * hs, base - dy - c[2] * s))
                    else:
                        d.append(("Q", gx + c[1] * s * hs, base - dy - c[2] * s,
                                  gx + c[3] * s * hs, base - dy - c[4] * s))
            if d:
                out.append({"type": "path", "d": d, "fill": "0 0 0", "role": "deboss"})
            gx += adv * hs
    return out


def rasterize(gs, x0, y0, w, h, scale):
    cv = Canvas(w, h)
    for g in gs:
        moved = [tuple([c[0]] + [(v - (x0 if i % 2 == 0 else y0)) * scale for i, v in enumerate(c[1:])])
                 for c in g["d"]]
        if g.get("fill") is not None:
            cv.fill(flatten(moved))
        if g.get("stroke") is not None:
            cv.fill(stroke_polys(moved, max(g.get("width", 0.5) * scale, 1.0)))
    return cv.px


def height_map(layout, pages, theme):
    spec = theme["emboss"]
    x0, y0, x1, y1 = spec["region"]
    scale = spec["dpi"] / 72
    w, h = int((x1 - x0) * scale), int((y1 - y0) * scale)
    up, down, foil, spot = shapes(layout, pages, theme)
    r = max(1, int(spec.get("bevel_pt", 1.0) * scale / 2))
    hu = box_blur(rasterize(up, x0, y0, w, h, scale), w, h, r)
    hd = box_blur(rasterize(down, x0, y0, w, h, scale), w, h, r)
    fm = rasterize(foil, x0, y0, w, h, scale)
    height = [spec.get("relief", 1.0) * (a - 0.7 * b) for a, b in zip(hu, hd)]
    return w, h, height, fm, (up, down, foil, spot)


def paper_normal(x, y, seed=7):
    """Fibre-like value noise: long thin features along x, as laid paper."""
    def hsh(i, j):
        n = (i * 374761393 + j * 668265263 + seed * 2147483647) & 0xFFFFFFFF
        n = (n ^ (n >> 13)) * 1274126177 & 0xFFFFFFFF
        return (n & 0xFFFF) / 65535.0
    def noise(u, v):
        i, j = int(math.floor(u)), int(math.floor(v))
        fu, fv = u - i, v - j
        a, b, c, d = hsh(i, j), hsh(i + 1, j), hsh(i, j + 1), hsh(i + 1, j + 1)
        return (a * (1 - fu) + b * fu) * (1 - fv) + (c * (1 - fu) + d * fu) * fv
    e = 0.5
    nx = noise(x / 9 + e, y / 2.2) - noise(x / 9 - e, y / 2.2)
    ny = noise(x / 9, y / 2.2 + e) - noise(x / 9, y / 2.2 - e)
    return nx * 0.08, ny * 0.08


def render_relief(path, w, h, height, foil, theme, lut):
    spec = theme["emboss"]
    lx, ly, lz = spec["light"]
    ln = math.sqrt(lx * lx + ly * ly + lz * lz)
    lx, ly, lz = lx / ln, ly / ln, lz / ln
    hx, hy, hz = lx, ly, lz + 1.0
    hn = math.sqrt(hx * hx + hy * hy + hz * hz)
    hx, hy, hz = hx / hn, hy / hn, hz / hn
    paper = theme.get("paper", "#f6f1e3").lstrip("#")
    pr, pg, pb = (int(paper[i:i + 2], 16) / 255 for i in (0, 2, 4))
    k = spec.get("dpi", 200) / 72 * 0.9
    rows = []
    for y in range(h):
        row = bytearray()
        for x in range(w):
            i = y * w + x
            dx = (height[i + 1] if x + 1 < w else height[i]) - (height[i - 1] if x > 0 else height[i])
            dy = (height[i + w] if y + 1 < h else height[i]) - (height[i - w] if y > 0 else height[i])
            px, py = paper_normal(x, y)
            nx, ny, nz = -dx * k + px, -dy * k + py, 1.0
            nn = math.sqrt(nx * nx + ny * ny + nz * nz)
            nx, ny, nz = nx / nn, ny / nn, nz / nn
            diff = max(0.0, nx * lx + ny * ly + nz * lz)
            spec_d = max(0.0, nx * hx + ny * hy + nz * hz)
            f = foil[i]
            ambient = 0.55
            r, g, b = (pr * (ambient + 0.5 * diff), pg * (ambient + 0.5 * diff), pb * (ambient + 0.5 * diff))
            if f > 0:
                ang = int(min(85, math.degrees(math.acos(max(min(nz, 1.0), 0.0)))) / 5)
                mr, mg, mb = lut[ang]
                s = spec_d ** 40 * 1.4 + 0.3 * diff + 0.7
                r = r * (1 - f) + f * min(1.0, mr * s)
                g = g * (1 - f) + f * min(1.0, mg * s)
                b = b * (1 - f) + f * min(1.0, mb * s)
            else:
                s = spec_d ** 24 * 0.08
                r, g, b = r + s, g + s, b + s
            row += bytes((min(255, int(r * 255)), min(255, int(g * 255)), min(255, int(b * 255))))
        rows.append(bytes(row))
    write_png(path, w, h, rows)


def export_plates(base, layout, pages, theme):
    from .pages import Page
    from .pdfout import write_pdf
    spec = theme["emboss"]
    w, h, height, foil, (up, down, foils, spots) = height_map(layout, pages, theme)
    files = []
    hrow = []
    lo, hi = min(height), max(height)
    span = (hi - lo) or 1.0
    for y in range(h):
        hrow.append(b"".join(int((height[y * w + x] - lo) / span * 65535).to_bytes(2, "big") for x in range(w)))
    write_png(base + "-emboss-height.png", w, h, hrow, mode="L", bits=16)
    files.append(base + "-emboss-height.png")
    lut, source = metal_lut(spec["foil"].get("material", "gold"))
    render_relief(base + "-emboss-render.png", w, h, height, foil, theme, lut)
    files.append(base + "-emboss-render.png")
    pg = pages[0]
    plates = [("emboss", up, "Emboss-Die", (0, 0, 0, 1)), ("deboss", down, "Deboss-Die", (0, 0, 0, 1)),
              ("foil", foils, "Foil-Gold", tuple(spec["foil"]["cmyk"])), ("spot", spots, "Spot-Ochre",
              tuple(spots[0].get("plate_cmyk") or (0, 0.3, 0.8, 0.1)) if spots else (0, 0.3, 0.8, 0.1))]
    for name, gs, sep, cmyk in plates:
        pdfgfx.reset()
        p = Page(pg.width, pg.height, [], 1)
        p.left, p.top = pg.left, pg.top
        p.furniture_under = []
        for g in gs:
            g2 = dict(g, spot=sep, spot_cmyk=cmyk, alpha=1.0)
            if g2.get("fill") is not None:
                g2["fill"] = "0 0 0"
            if g2.get("stroke") is not None:
                g2["stroke"] = "0 0 0"
            p.furniture.append(g2)
        path = f"{base}-plate-{name}.pdf"
        write_pdf(path, [p], layout.reg, {"title": f"{sep} plate", "author": ""}, graphics_only=True, pdfa=False)
        files.append(path)
    layout.diag.notes.append(f"foil colour source: {source}")
    return files
