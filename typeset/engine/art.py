"""Generative artwork layers, drawn as vectors under (or over) the text.

The aperture: a luminous orb made of fine line-work, rings whose radii
follow a slow logarithmic spiral and whose phase drifts with a seed taken
from the document, so each document gets its own figure. A dot-screen
band (halftone) and a hairline rule complete the set. Everything is a
path: nothing here becomes text, so extraction and ATS order stay clean.
"""
import hashlib
import math


def _seed(text):
    return int(hashlib.sha256(text.encode("utf-8")).hexdigest()[:8], 16)


def _rgb(hexcol):
    h = hexcol.lstrip("#")
    return "%.4f %.4f %.4f" % tuple(int(h[i:i + 2], 16) / 255 for i in (0, 2, 4))


def aperture(cx, cy, r, color, seed, rings=34, spokes=0, width=0.35, alpha=0.9, emboss=False):
    out = []
    rnd = seed
    for k in range(rings):
        t = k / max(rings - 1, 1)
        rr = r * (0.18 + 0.82 * t ** 0.85)
        rnd = (rnd * 1103515245 + 12345) & 0x7FFFFFFF
        phase = (rnd / 0x7FFFFFFF) * math.tau
        squash = 0.96 + 0.04 * math.sin(phase * 3)
        d, n = [], 96
        for i in range(n + 1):
            a = phase + i / n * math.tau
            wob = 1 + 0.012 * math.sin(a * 5 + phase) * (1 - t)
            x = cx + rr * wob * math.cos(a)
            y = cy + rr * wob * squash * math.sin(a)
            d.append(("M" if i == 0 else "L", x, y))
        out.append({"type": "path", "d": d, "stroke": color, "width": width * (0.6 + 0.8 * (1 - t)),
                    "alpha": alpha * (0.35 + 0.65 * (1 - t)), "role": "aperture-ring", "emboss": emboss})
    core = []
    for i in range(65):
        a = i / 64 * math.tau
        core.append(("M" if i == 0 else "L", cx + r * 0.16 * math.cos(a), cy + r * 0.16 * math.sin(a)))
    core.append(("Z",))
    out.append({"type": "path", "d": core, "fill": color, "alpha": alpha, "role": "aperture-core", "emboss": emboss})
    return out


def halftone(x, y, w, h, color, cell=4.2, alpha=0.5):
    out = []
    cols, rows = int(w / cell), int(h / cell)
    for j in range(rows):
        for i in range(cols):
            t = i / max(cols - 1, 1)
            rad = cell * 0.48 * (1 - t) ** 1.6
            if rad < 0.12:
                continue
            px, py = x + i * cell + cell / 2, y + j * cell + cell / 2
            k = 0.5523 * rad
            d = [("M", px + rad, py), ("C", px + rad, py + k, px + k, py + rad, px, py + rad),
                 ("C", px - k, py + rad, px - rad, py + k, px - rad, py),
                 ("C", px - rad, py - k, px - k, py - rad, px, py - rad),
                 ("C", px + k, py - rad, px + rad, py - k, px + rad, py), ("Z",)]
            out.append({"type": "path", "d": d, "fill": color, "alpha": alpha, "role": "halftone"})
    return out


def art_layer(spec, page, theme):
    if spec.get("first_page_only", True) and page.number != 1:
        return []
    colors = theme.get("colors", {})
    col = _rgb(colors.get(spec.get("color", "ink"), "#16130f"))
    seed = _seed(theme.get("_seed_text", "") + spec.get("kind", ""))
    k = spec["kind"]
    if k == "aperture":
        return aperture(spec["cx"], spec["cy"], spec["r"], col, seed, spec.get("rings", 34),
                        width=spec.get("width", 0.35), alpha=spec.get("alpha", 0.9),
                        emboss=spec.get("emboss", False))
    if k == "halftone":
        return halftone(spec["x"], spec["y"], spec["w"], spec["h"], col, spec.get("cell", 4.2), spec.get("alpha", 0.5))
    if k == "rule":
        return [{"type": "path", "d": [("M", spec["x"], spec["y"]), ("L", spec["x"] + spec["w"], spec["y"])],
                 "stroke": col, "width": spec.get("width", 0.6), "role": "rule", "plate": spec.get("plate"),
                 "plate_cmyk": spec.get("plate_cmyk")}]
    raise ValueError(f"unknown art layer kind {k!r}")
