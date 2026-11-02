"""SVG and HTML from the same laid-out pages.

Glyphs are drawn from the font outlines (identical shapes to the PDF);
a transparent text layer on top keeps the page selectable and searchable.
"""
import html

from .glyf import contour_path


def _glyph_d(font, gid):
    parts = []
    for contour in font.outline(gid):
        for c in contour_path(contour):
            if c[0] == "M":
                parts.append(f"M{c[1]:.0f} {-c[2]:.0f}")
            elif c[0] == "L":
                parts.append(f"L{c[1]:.0f} {-c[2]:.0f}")
            elif c[0] == "Q":
                parts.append(f"Q{c[1]:.0f} {-c[2]:.0f} {c[3]:.0f} {-c[4]:.0f}")
            else:
                parts.append("Z")
    return "".join(parts)


def _rgb(col):
    if not col:
        return "#000"
    r, g, b = (float(v) for v in col.split())
    return "#%02x%02x%02x" % (int(r * 255), int(g * 255), int(b * 255))


def _gfx(g):
    if g.get("type", "path") != "path":
        return ""
    d = []
    for c in g["d"]:
        if c[0] == "Z":
            d.append("Z")
        else:
            d.append(c[0] + " ".join(f"{v:.2f}" for v in c[1:]))
    fill = _rgb(g["fill"]) if g.get("fill") else "none"
    stroke = _rgb(g["stroke"]) if g.get("stroke") else "none"
    extra = f' stroke-width="{g.get("width", 1):.2f}"' if g.get("stroke") else ""
    if g.get("dash"):
        extra += ' stroke-dasharray="' + " ".join(f"{v:.2f}" for v in g["dash"]) + '"'
    a = g.get("alpha", 1.0)
    if a < 1:
        extra += f' opacity="{a:.3f}"'
    return f'<path d="{"".join(d)}" fill="{fill}" stroke="{stroke}"{extra} stroke-linecap="round"/>'


def page_svg(page, registry, defs_cache):
    out, text_layer = [], []
    left, top = getattr(page, "left", 0), getattr(page, "top", 0)
    for g in getattr(page, "furniture_under", []):
        out.append(_gfx(g))
    lines = [(top + y, ln) for y, ln in page.items]
    for g in page.furniture:
        if g.get("type") == "line":
            lines.append((g["y"] + top, g["line"]))
        else:
            out.append(_gfx(g))
    for y, ln in lines:
        for rx, ry, rw, rh in ln.graphics:
            out.append(f'<rect x="{left + rx:.2f}" y="{y - ry - rh:.2f}" width="{rw:.2f}" height="{rh:.2f}"/>')
        for x, dy, pc, hs in ln.runs:
            font = registry[pc.font]
            s = pc.size / font.upem
            gx = left + x
            fill = _rgb(pc.color)
            if font.outline(pc.gids[0] if pc.gids else 0) or getattr(font, "kind", "ttf") == "ttf":
                for gid, adv in zip(pc.gids, pc.advs):
                    key = "g" + "".join(ch for ch in pc.font if ch.isalnum()) + f"_{gid}"
                    if key not in defs_cache:
                        defs_cache[key] = _glyph_d(font, gid)
                    if defs_cache[key]:
                        out.append(f'<use href="#{key}" transform="translate({gx:.2f} {y - dy:.2f}) '
                                   f'scale({s * hs:.5f} {s:.5f})" fill="{fill}"/>')
                    gx += adv * hs
                text_layer.append(_text(left + x, y - dy, pc, hs, "transparent"))
            else:
                text_layer.append(_text(left + x, y - dy, pc, hs, fill, font.name))
    return out, text_layer


def _text(x, y, pc, hs, fill, family="sans-serif"):
    xs, cx = [], x
    for c, adv in zip(pc.clusters, pc.advs):
        for k, ch in enumerate(c):
            xs.append(cx + adv * hs * k / len(c))
        cx += adv * hs
    t = html.escape("".join(pc.clusters))
    return (f'<text x="{" ".join(f"{v:.2f}" for v in xs)}" y="{y:.2f}" font-size="{pc.size:.2f}" '
            f'font-family="{html.escape(family)}, serif" fill="{fill}" xml:space="preserve">{t}</text>')


def write_svg_pages(base, pages, registry, meta, html=True):
    files, bodies, defs = [], [], {}
    rendered = [page_svg(p, registry, defs) for p in pages]
    defs_xml = "".join(f'<path id="{k}" d="{d}"/>' for k, d in defs.items() if d)
    for n, (page, (gfx, text)) in enumerate(zip(pages, rendered), 1):
        svg = (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {page.width:.2f} {page.height:.2f}" '
               f'width="{page.width:.2f}pt" height="{page.height:.2f}pt">'
               f'<rect width="100%" height="100%" fill="{getattr(page, "paper", "#ffffff")}"/>'
               f'<defs>{defs_xml}</defs>{"".join(gfx)}<g class="text">{"".join(text)}</g></svg>')
        path = f"{base}-p{n}.svg"
        with open(path, "w", encoding="utf-8") as fh:
            fh.write(svg)
        files.append(path)
        bodies.append(svg)
    if html:
        title = _esc(meta.get("title", ""))
        doc = (f'<!doctype html><html lang="en"><head><meta charset="utf-8"><title>{title}</title>'
               '<meta name="viewport" content="width=device-width, initial-scale=1">'
               '<style>body{margin:0;background:#d9d4ca;}main{display:flex;flex-direction:column;'
               'align-items:center;gap:24px;padding:24px 16px}svg{max-width:100%;height:auto;'
               'box-shadow:0 2px 12px rgba(0,0,0,.25)}text::selection{fill:#1d2562;background:#d79e35}</style>'
               f'</head><body><main>{"".join(bodies)}</main></body></html>')
        path = base + ".html"
        with open(path, "w", encoding="utf-8") as fh:
            fh.write(doc)
        files.append(path)
    return files


def _esc(s):
    return s.replace("&", "&amp;").replace("<", "&lt;")
