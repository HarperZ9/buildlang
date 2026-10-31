"""The measured-annotation overlay: literary devices drawn as physical figures.

A typed layer over the laid-out page. It reads `mark` and `rel` records
(device, layer, span, score in bits, error, meter) and draws:

  light         bits carried: a glow under the span, opacity = score / max
  heat          residual and uncertainty: a warm error bar, length = err
  transparency  the incompressible part: the span's hollow stays clear
  symbol        a point source: radial falloff, and a margin glyph
  motif         interference fringes from the Fourier spectrum of the
                motif's positions, plus arcs linking each recurrence
  theme         the lowest standing-wave mode across the work, in the margin
  analogy       relation arcs; mapped relations lit, unmapped ones cold
  hypocrisy     crossed-polarizer photoelastic fringes; fringe count = gap
  contrarian    one streamline crossing a field of consensus streamlines
Layers: physical marks are solid, potential marks are dashed ghosts,
metaphorical marks are drawn at half weight. Every mark carries its meter
and error bar in the margin. With the layer off nothing here runs.
"""
import math

WARM = "0.784 0.196 0.169"      # red: heat
GOLD = "0.843 0.620 0.208"      # ochre: light
NAVY = "0.114 0.145 0.384"
COLD = "0.55 0.62 0.72"
SOFT = "0.29 0.267 0.231"


def char_boxes(layout, pages, doc):
    """Map (block, char offset) to (page, x0, x1, baseline y, size) for every glyph cluster."""
    out, cursors = {}, {}
    for page in pages:
        for y, line in page.items:
            if line.block < 0:
                continue
            block = doc.blocks[line.block]
            plain = block.plain
            cur = cursors.get(line.block, 0)
            for x, dy, pc, hs in line.runs:
                gx = page.left + x
                for cl, adv in zip(pc.clusters, pc.advs):
                    w = adv * hs
                    k = cur
                    while k < len(plain) and plain[k].isspace():
                        k += 1
                    if k < len(plain) and plain.startswith(cl.replace(" ", " "), k):
                        for j in range(len(cl)):
                            out[(line.block, k + j)] = (page, gx + w * j / len(cl), gx + w * (j + 1) / len(cl),
                                                        page.top + y, pc.size)
                        cur = k + len(cl)
                    gx += w
            cursors[line.block] = cur
    return out


def span_rects(boxes, m):
    """Per line segment: (page, x0, x1, y, size)."""
    segs = {}
    for k in range(m.start, m.end):
        b = boxes.get((m.block, k))
        if not b:
            continue
        page, x0, x1, y, size = b
        key = (id(page), round(y, 2))
        if key in segs:
            s = segs[key]
            segs[key] = (page, min(s[1], x0), max(s[2], x1), y, size)
        else:
            segs[key] = (page, x0, x1, y, size)
    return list(segs.values())


def _rect(x0, y0, x1, y1):
    return [("M", x0, y0), ("L", x1, y0), ("L", x1, y1), ("L", x0, y1), ("Z",)]


def _circle(cx, cy, r, n=40):
    d = [("M" if i == 0 else "L", cx + r * math.cos(i / n * math.tau), cy + r * math.sin(i / n * math.tau))
         for i in range(n + 1)]
    return d + [("Z",)]


def _layer_style(g, layer):
    if layer == "potential":
        g["dash"] = [2.0, 1.6]
        g["alpha"] = g.get("alpha", 1.0) * 0.55
    elif layer == "metaphorical":
        g["alpha"] = g.get("alpha", 1.0) * 0.5
        if g.get("width"):
            g["width"] *= 0.6
    return g


def overlay(layout, pages, doc, theme):
    boxes = char_boxes(layout, pages, doc)
    top = max((m.score for m in doc.marks), default=1.0) or 1.0
    by_id = {}
    margin_x = theme["page"]["width"] - theme["page"]["right"] + 10
    for idx, m in enumerate(doc.marks):
        segs = span_rects(boxes, m)
        if not segs:
            layout.diag.notes.append(f"mark {idx} ({m.device}) has no laid-out characters")
            continue
        by_id.setdefault(m.group, []).append((m, segs))
        page = segs[0][0]
        under, over = page.furniture_under, page.furniture
        light = m.score / top
        for (pg, x0, x1, y, size) in segs:
            # light: bits carried; the hollow (1 - light) stays clear
            pg.furniture_under.append(_layer_style({"type": "path", "d": _rect(x0 - 1, y - size * 0.82, x1 + 1, y + size * 0.28),
                                                    "fill": GOLD, "alpha": 0.08 + 0.32 * light}, m.layer))
        draw = DEVICES.get(m.device)
        if draw:
            draw(m, segs, under, over, light, theme)
        _meter(m, segs[0], over, margin_x, layout)
    for rel in doc.rels:
        _relation(rel, by_id)
    _motif_fringes(doc, by_id, pages, theme)
    _theme_mode(doc, pages, theme)


def _meter(m, seg, over, mx, layout):
    page, x0, x1, y, size = seg
    used = page.__dict__.setdefault("_meter_y", [])
    while any(abs(y - u) < 8.5 for u in used):
        y += 8.5
    used.append(y)
    err = m.err
    # heat: a warm error bar, 6 pt per bit of uncertainty
    over.append(_layer_style({"type": "path", "d": [("M", mx, y - 2.5), ("L", mx + 6 * err, y - 2.5)],
                              "stroke": WARM, "width": 1.6, "alpha": 0.9}, m.layer))
    over.append({"type": "path", "d": [("M", mx, y - 4.5), ("L", mx, y - 0.5)], "stroke": WARM, "width": 0.5})
    label = f"{m.device} {m.score:.1f}±{m.err:.1f} b {m.meter}"
    pieces = layout.shaper.shape(layout.theme.get("annotation_font", "tt"), 5.2, label, SOFT)
    from .pages import VLine
    runs, x = [], mx - page.left
    for pc in pieces:
        runs.append((x, 0.0, pc, 1.0))
        x += pc.width
    over.append({"type": "line", "line": VLine(5.2, 0, runs, block=-4), "y": y + 4.5 - page.top, "page": page})


def _symbol(m, segs, under, over, light, theme):
    page, x0, x1, y, size = segs[0]
    cx, cy = (x0 + x1) / 2, y - size * 0.3
    for k in range(7, 0, -1):
        under.append(_layer_style({"type": "path", "d": _circle(cx, cy, size * 0.35 * k), "fill": GOLD,
                                   "alpha": 0.05 * light * (8 - k) / 7 + 0.02}, m.layer))
    star = []
    mx = theme["page"]["left"] - 12
    for i in range(9):
        r = 3.2 if i % 2 == 0 else 1.2
        a = -math.pi / 2 + i * math.pi / 4
        star.append(("M" if i == 0 else "L", mx + r * math.cos(a), cy + r * math.sin(a)))
    star.append(("Z",))
    over.append(_layer_style({"type": "path", "d": star, "fill": NAVY}, m.layer))


def _hypocrisy(m, segs, under, over, light, theme):
    # isochromatic fringes between crossed polarizers: retardation grows across the span,
    # one full colour cycle per fringe, fringe count = round(score)
    cycle = ["0.10 0.10 0.12", "0.55 0.45 0.30", "0.86 0.80 0.62", "0.85 0.62 0.25", "0.70 0.18 0.30",
             "0.35 0.20 0.62", "0.20 0.45 0.75", "0.35 0.70 0.55", "0.88 0.84 0.40"]
    n = max(1, round(m.score))
    for (page, x0, x1, y, size) in segs:
        steps = 9 * n
        w = (x1 - x0) / steps
        for s in range(steps):
            under.append(_layer_style({"type": "path", "d": _rect(x0 + s * w, y + size * 0.32, x0 + (s + 1) * w + 0.05,
                                                                 y + size * 0.62), "fill": cycle[s % 9], "alpha": 0.85},
                                      m.layer))


def _contrarian(m, segs, under, over, light, theme):
    page, x0, x1, y, size = segs[0]
    left, right = theme["page"]["left"], page.width - theme["page"]["right"]
    for k in range(-3, 4):
        yy = y - size * 0.3 + k * size * 0.55
        d = [("M", left, yy)] + [("L", left + t * (right - left) / 30, yy + 0.8 * math.sin(t / 4.0)) for t in range(1, 31)]
        under.append({"type": "path", "d": d, "stroke": COLD, "width": 0.35, "alpha": 0.6})
    cx = (x0 + x1) / 2
    d = [("M", cx - 22, y - size * 2.2), ("C", cx - 6, y - size * 1.2, cx + 6, y + size * 0.4, cx + 24, y + size * 1.4)]
    over.append(_layer_style({"type": "path", "d": d, "stroke": WARM, "width": 1.1}, m.layer))
    over.append(_layer_style({"type": "path", "d": [("M", cx + 24, y + size * 1.4), ("L", cx + 19, y + size * 1.3),
                                                   ("L", cx + 22, y + size * 0.9), ("Z",)], "fill": WARM}, m.layer))


DEVICES = {"symbol": _symbol, "hypocrisy": _hypocrisy, "contrarian": _contrarian}


def _arc(a, b, color, width, alpha, dash=None):
    pa, ax0, ax1, ay, asz = a
    pb, bx0, bx1, by, bsz = b
    if pa is not pb:
        return None
    x0, y0 = (ax0 + ax1) / 2, ay - asz
    x1, y1 = (bx0 + bx1) / 2, by - bsz
    lift = 10 + 0.18 * math.hypot(x1 - x0, y1 - y0)
    g = {"type": "path", "d": [("M", x0, y0), ("C", x0, y0 - lift, x1, y1 - lift, x1, y1)],
         "stroke": color, "width": width, "alpha": alpha}
    if dash:
        g["dash"] = dash
    return pa, g


def _relation(rel, by_id):
    a, b = by_id.get(rel.a), by_id.get(rel.b)
    if not a or not b:
        return
    lit = rel.mapped
    r = _arc(a[0][1][0], b[0][1][0], GOLD if lit else COLD, 1.3 if lit else 0.6, 0.95 if lit else 0.7,
             [2, 1.6] if rel.layer == "potential" or not lit else None)
    if r:
        r[0].furniture.append(r[1])


def _motif_fringes(doc, by_id, pages, theme):
    total = sum(len(b.plain) for b in doc.blocks) or 1
    starts, acc = {}, 0
    for b in doc.blocks:
        starts[b.index] = acc
        acc += len(b.plain)
    for group, marks in by_id.items():
        motif = [(m, segs) for m, segs in marks if m.device == "motif"]
        if len(motif) < 2:
            continue
        for (m1, s1), (m2, s2) in zip(motif, motif[1:]):
            r = _arc(s1[0], s2[0], NAVY, 0.7, 0.8, [2, 1.6] if m2.layer == "potential" else None)
            if r:
                r[0].furniture.append(r[1])
        pos = [(starts[m.block] + m.start) / total for m, _ in motif]
        page = pages[0]
        x0, x1 = theme["page"]["left"], page.width - theme["page"]["right"]
        y = page.height - theme["page"]["bottom"] + 14
        steps = 160
        vals = []
        for i in range(steps):
            u = (i / steps) * 24.0                     # spatial frequency, cycles per work
            re_ = sum(math.cos(math.tau * u * p) for p in pos)
            im = sum(math.sin(math.tau * u * p) for p in pos)
            vals.append((re_ * re_ + im * im) / len(pos) ** 2)
        w = (x1 - x0) / steps
        for i, v in enumerate(vals):
            page.furniture_under.append({"type": "path", "d": _rect(x0 + i * w, y, x0 + (i + 1) * w + 0.05, y + 7),
                                         "fill": NAVY, "alpha": 0.05 + 0.85 * v})


def _theme_mode(doc, pages, theme):
    themes = [m for m in doc.marks if m.device == "theme"]
    if not themes:
        return
    amp = max(m.score for m in themes) / max(1.0, max(m.score for m in doc.marks))
    page = pages[0]
    x = theme["page"]["left"] - 26
    y0, y1 = theme["page"]["top"], page.height - theme["page"]["bottom"]
    for sign in (1, -1):
        d = [("M", x, y0)]
        for i in range(1, 41):
            t = i / 40
            d.append(("L", x + sign * 9 * amp * math.sin(math.pi * t), y0 + t * (y1 - y0)))
        page.furniture_under.append({"type": "path", "d": d, "stroke": GOLD, "width": 0.8, "alpha": 0.85})
