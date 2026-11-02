"""Press finish: the lit emboss and deboss relief placed in the document itself.

The plates path (emboss.export_plates) writes print plates beside the
document for a real die. This path is for the screen and office printers:
it lights the same height map and places it on the first page as an
image, so the PDF and HTML show the blind deboss and the foil without a
press run. In the blind finish the name stays in the text layer through PDF render mode 3,
so extraction and reading order do not change. Vector furniture that the
relief now shows (the embossed aperture rings and core) leaves the page.

Theme keys, under "emboss":
  "mode": "inline"     turn this path on (default "plates" leaves pages alone)
  "dpi", "relief", "bevel_pt", "light", "foil", "emboss_roles",
  "deboss_blocks", "region"   as for the plates path
  "deboss_finish": "inked"    the debossed line also prints in ink (default); "blind" prints relief only
  "match_paper": true         flat paper in the relief keeps the page paper colour
Theme key "paper_fill": true paints the theme's paper colour on every page.
"""
from . import pdfgfx
from .emboss import height_map, metal_lut, render_relief


def _rgb(hexcol):
    h = hexcol.lstrip("#")
    return "%.4f %.4f %.4f" % tuple(int(h[i:i + 2], 16) / 255 for i in (0, 2, 4))


def apply(layout, pages, theme):
    """Mutates pages in place. Returns a short list of notes for the layout report."""
    notes = []
    if theme.get("paper_fill"):
        fill = _rgb(theme.get("paper", "#ffffff"))
        for page in pages:
            page.furniture_under.insert(0, {"type": "rect", "w": page.width, "h": page.height, "fill": fill,
                                            "role": "paper"})
    spec = theme.get("emboss") or {}
    if spec.get("mode") != "inline" or not pages:
        return notes
    w, h, height, foil, _ = height_map(layout, pages, theme)
    lut, source = metal_lut(spec["foil"].get("material", "gold"))
    rows = render_relief(None, w, h, height, foil, theme, lut)
    x0, y0, x1, y1 = spec["region"]
    page = pages[0]
    roles = set(spec.get("emboss_roles", [])) | set(spec["foil"].get("roles", []))
    keep = lambda g: not (g.get("type") == "path" and g.get("role") in roles and g.get("emboss", True))
    page.furniture_under = [g for g in page.furniture_under if keep(g)]
    page.furniture = [g for g in page.furniture if keep(g)]
    at = 1 if page.furniture_under and page.furniture_under[0].get("role") == "paper" else 0
    page.furniture_under.insert(at, pdfgfx.image(x0, y0, x1 - x0, y1 - y0, w, h, b"".join(rows), role="relief"))
    # "inked" (the default) prints the debossed line in ink inside its relief, as letterpress does,
    # so the name keeps full contrast. "blind" leaves only the relief and keeps the text layer.
    kinds = set(spec.get("deboss_blocks", [])) if spec.get("deboss_finish", "inked") == "blind" else set()
    blind = 0
    for _, line in page.items:
        if layout.doc.blocks[line.block].kind in kinds:
            line.blind = True
            blind += 1
    notes.append(f"press: relief {w}x{h} px at {spec['dpi']} dpi; {blind} blind line(s); foil colour source: {source}")
    return notes
