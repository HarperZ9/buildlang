"""Vector graphics for the PDF writer: paths, alpha, spot (Separation) colours."""

_ALPHAS = set()
_SEPS = {}


def reset():
    _ALPHAS.clear()
    _SEPS.clear()


def spot(name, cmyk):
    """Register a Separation colour, e.g. spot('Foil-Gold', (0, .2, .6, .1))."""
    _SEPS[name] = cmyk
    return "CS_" + name.replace("-", "_")


def _path(d, H):
    out, cx, cy = [], 0.0, 0.0
    for c in d:
        op = c[0]
        if op == "Q":
            qx, qy, x, y = c[1], c[2], c[3], c[4]
            c = ("C", cx + 2 / 3 * (qx - cx), cy + 2 / 3 * (qy - cy), x + 2 / 3 * (qx - x), y + 2 / 3 * (qy - y), x, y)
            op = "C"
        if op in "MLC":
            cx, cy = c[-2], c[-1]
        if op == "M":
            out.append(f"{c[1]:.3f} {H - c[2]:.3f} m")
        elif op == "L":
            out.append(f"{c[1]:.3f} {H - c[2]:.3f} l")
        elif op == "C":
            out.append(f"{c[1]:.3f} {H - c[2]:.3f} {c[3]:.3f} {H - c[4]:.3f} {c[5]:.3f} {H - c[6]:.3f} c")
        elif op == "Z":
            out.append("h")
    return " ".join(out)


def draw(g, H, registry=None, uses=None, line_ops=None):
    t = g.get("type", "path")
    if t == "line" and line_ops:
        page = g["page"]
        return line_ops(g["line"], g["y"], page, registry, uses)
    if t != "path":
        return ""
    ops = ["q"]
    alpha = g.get("alpha", 1.0)
    if alpha < 1.0:
        a = round(alpha, 2)
        _ALPHAS.add(a)
        ops.append(f"/GSa{int(a * 100)} gs")
    sep = g.get("spot")
    if sep:
        name = spot(sep, g.get("spot_cmyk", (0, 0, 0, 1)))
        tint = g.get("tint", 1.0)
        if g.get("fill") is not None:
            ops.append(f"/{name} cs {tint:.3f} scn")
        if g.get("stroke") is not None:
            ops.append(f"/{name} CS {tint:.3f} SCN")
    else:
        if g.get("fill"):
            ops.append(f"{g['fill']} rg")
        if g.get("stroke"):
            ops.append(f"{g['stroke']} RG")
    if g.get("width"):
        ops.append(f"{g['width']:.3f} w 1 J 1 j")
    if g.get("dash"):
        ops.append("[" + " ".join(f"{v:.2f}" for v in g["dash"]) + "] 0 d")
    ops.append(_path(g["d"], H))
    f, s = g.get("fill") is not None, g.get("stroke") is not None
    ops.append("B" if f and s else "f" if f else "S")
    ops.append("Q")
    return "\n".join(ops)


def extgstates(w):
    entries = " ".join(f"/GSa{int(a * 100)} << /Type /ExtGState /ca {a} /CA {a} >>" for a in sorted(_ALPHAS))
    return f"<< {entries} >>"


def colorspaces(w):
    entries = []
    for name, (c, m, y, k) in _SEPS.items():
        fn = f"<< /FunctionType 2 /Domain [0 1] /C0 [0 0 0 0] /C1 [{c} {m} {y} {k}] /N 1 >>"
        entries.append(f"/CS_{name.replace('-', '_')} [/Separation /{name} /DeviceCMYK {fn}]")
    return "<< " + " ".join(entries) + " >>"
