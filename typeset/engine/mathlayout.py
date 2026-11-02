"""Math layout after TeXbook Appendix G, with OpenType MATH constants.

Implemented: atom classes and inter-atom spacing (Rules 5, 6, 20 and the
spacing table of Chapter 18), the four styles and cramping, scripts
(Rule 18 a-f), fractions (Rule 15 a-e), radicals (Rule 11), large
operators with display limits (Rule 13a). Not implemented: delimiters
grown by \\left/\\right, accents, matrices, italic-correction kerning.
"""
from dataclasses import dataclass, field

from .mathfont import read_math_constants, glyph_bounds
from .shape import Piece
from .mathparse import Atom, parse  # noqa: F401  (parse re-exported)

D, T, S, SS = 0, 1, 2, 3
ORD, OP, BIN, REL, OPEN, CLOSE, PUNCT, INNER = range(8)
SPACE = [  # rows: left class, columns: right class; 1 thin 2 med 3 thick, negative: not in scripts
    [0, 1, -2, -3, 0, 0, 0, -1], [1, 1, 0, -3, 0, 0, 0, -1], [-2, -2, 0, 0, -2, 0, 0, -2],
    [-3, -3, 0, 0, -3, 0, 0, -3], [0, 0, 0, 0, 0, 0, 0, 0], [0, 1, -2, -3, 0, 0, 0, -1],
    [-1, -1, 0, -1, -1, -1, -1, -1], [-1, 1, -2, -3, -1, 0, -1, -1]]
MU = {1: 3, 2: 4, 3: 5}
@dataclass
class MBox:
    width: float = 0.0
    height: float = 0.0
    depth: float = 0.0
    glyphs: list = field(default_factory=list)   # (dx, dy_up, Piece)
    rules: list = field(default_factory=list)    # (dx, dy_up_bottom, w, h)

    def add(self, other, dx, dy):
        self.glyphs += [(dx + x, dy + y, p) for x, y, p in other.glyphs]
        self.rules += [(dx + x, dy + y, w, h) for x, y, w, h in other.rules]


class MathLayout:
    def __init__(self, font, key, size):
        self.font, self.key, self.size = font, key, size
        self.c = read_math_constants(font)
        self.u = font.upem

    def scale(self, style):
        if style >= SS:
            return self.c["ScriptScriptPercentScaleDown"] / 100.0
        if style >= S:
            return self.c["ScriptPercentScaleDown"] / 100.0
        return 1.0

    def k(self, name, style):
        return self.c[name] * self.size * self.scale(style) / self.u

    def char(self, ch, style):
        size = self.size * self.scale(style)
        box = MBox()
        for c in ch:
            g = self.font.glyph(c)
            if not g:
                g = self.font.glyph({"ℎ": "h"}.get(c, c))
            adv = self.font.advance(g) * size / self.u
            x0, y0, x1, y1 = glyph_bounds(self.font, g)
            box.glyphs.append((box.width, 0.0, Piece(self.key, size, [g], [adv], c, [c])))
            box.width += adv
            box.height = max(box.height, y1 * size / self.u)
            box.depth = max(box.depth, -y0 * size / self.u)
        return box

    def layout(self, atoms, style=T, cramped=False):
        atoms = _fix_bins(atoms)
        box, prev = MBox(), None
        mu = self.size * self.scale(style) / 18.0
        for a in atoms:
            if a.cls == -1:
                box.width += a.nucleus * mu
                continue
            if prev is not None:
                sp = SPACE[prev][a.cls]
                if sp > 0 or (sp < 0 and style < S):
                    box.width += MU[abs(sp)] * mu
            b = self.atom(a, style, cramped)
            box.add(b, box.width, 0.0)
            box.width += b.width
            box.height, box.depth = max(box.height, b.height), max(box.depth, b.depth)
            prev = a.cls
        return box

    def atom(self, a, style, cramped):
        n = a.nucleus
        if isinstance(n, str):
            big = a.cls == OP and style == D and len(n) == 1
            nuc = self._big_op(n, style) if big else self.char(n, style)
        elif isinstance(n, tuple) and n[0] == "frac":
            nuc = self.fraction(n[1], n[2], style, cramped)
        elif isinstance(n, tuple) and n[0] == "sqrt":
            nuc = self.radical(n[1], style)
        else:
            nuc = self.layout(n, style, cramped)
        if a.sup is None and a.sub is None:
            return nuc
        if a.cls == OP and style == D and isinstance(n, str) and len(n) == 1:
            return self.limits(nuc, a, style, cramped)
        return self.scripts(nuc, a, style, cramped, isinstance(n, str))

    def _big_op(self, ch, style):
        # Display operators are drawn at DisplayOperatorMinHeight via scaling (no MathVariants lookup).
        box = self.char(ch, style)
        target = self.c["DisplayOperatorMinHeight"] * self.size / self.u
        tall = box.height + box.depth
        if tall and tall < target:
            f = target / tall
            big = self.char(ch, style)
            for i, (x, y, p) in enumerate(big.glyphs):
                p.size *= f
                p.advs = [v * f for v in p.advs]
            big.width *= f
            axis = self.k("AxisHeight", style)
            mid = (box.height - box.depth) / 2 * f
            shift = axis - mid
            big.glyphs = [(x, y + shift, p) for x, y, p in big.glyphs]
            big.height, big.depth = box.height * f + shift, box.depth * f - shift
            return big
        return box

    def scripts(self, nuc, a, style, cramped, is_char):
        sst = S if style < S else SS
        out = MBox(nuc.width, nuc.height, nuc.depth)
        out.add(nuc, 0, 0)
        u = 0.0 if is_char else nuc.height - self.k("SuperscriptBaselineDropMax", sst)
        v = 0.0 if is_char else nuc.depth + self.k("SubscriptBaselineDropMin", sst)
        sup = self.layout(a.sup, sst, cramped) if a.sup is not None else None
        sub = self.layout(a.sub, sst, True) if a.sub is not None else None
        if sup is None:
            v = max(v, self.k("SubscriptShiftDown", style), sub.height - self.k("SubscriptTopMax", style))
        else:
            up = self.k("SuperscriptShiftUpCramped" if cramped else "SuperscriptShiftUp", style)
            u = max(u, up, sup.depth + self.k("SuperscriptBottomMin", style))
            if sub is not None:
                v = max(v, self.k("SubscriptShiftDown", style))
                gap = (u - sup.depth) - (sub.height - v)
                need = self.k("SubSuperscriptGapMin", style)
                if gap < need:
                    v += need - gap
                    psi = self.k("SuperscriptBottomMaxWithSubscript", style) - (u - sup.depth)
                    if psi > 0:
                        u += psi
                        v -= psi
        x = nuc.width
        w = 0.0
        if sup is not None:
            out.add(sup, x, u)
            out.height = max(out.height, sup.height + u)
            w = sup.width
        if sub is not None:
            out.add(sub, x, -v)
            out.depth = max(out.depth, sub.depth + v)
            w = max(w, sub.width)
        out.width = x + w + self.k("SpaceAfterScript", style)
        return out

    def limits(self, nuc, a, style, cramped):
        sup = self.layout(a.sup, S, cramped) if a.sup is not None else None
        sub = self.layout(a.sub, S, True) if a.sub is not None else None
        w = max(nuc.width, sup.width if sup else 0, sub.width if sub else 0)
        out = MBox(w, nuc.height, nuc.depth)
        out.add(nuc, (w - nuc.width) / 2, 0)
        if sup:
            rise = max(self.k("UpperLimitBaselineRiseMin", style), self.k("UpperLimitGapMin", style) + sup.depth)
            out.add(sup, (w - sup.width) / 2, nuc.height + rise)
            out.height = nuc.height + rise + sup.height
        if sub:
            drop = max(self.k("LowerLimitBaselineDropMin", style), self.k("LowerLimitGapMin", style) + sub.height)
            out.add(sub, (w - sub.width) / 2, -(nuc.depth + drop))
            out.depth = nuc.depth + drop + sub.depth
        return out

    def fraction(self, num_atoms, den_atoms, style, cramped):
        sub_style = {D: T, T: S, S: SS, SS: SS}[style]
        num = self.layout(num_atoms, sub_style, cramped)
        den = self.layout(den_atoms, sub_style, True)
        disp = style == D
        u = self.k("FractionNumeratorDisplayStyleShiftUp" if disp else "FractionNumeratorShiftUp", style)
        v = self.k("FractionDenominatorDisplayStyleShiftDown" if disp else "FractionDenominatorShiftDown", style)
        theta = self.k("FractionRuleThickness", style)
        axis = self.k("AxisHeight", style)
        gn = self.k("FractionNumDisplayStyleGapMin" if disp else "FractionNumeratorGapMin", style)
        gd = self.k("FractionDenomDisplayStyleGapMin" if disp else "FractionDenominatorGapMin", style)
        if (u - num.depth) - (axis + theta / 2) < gn:
            u = gn + axis + theta / 2 + num.depth
        if (axis - theta / 2) - (den.height - v) < gd:
            v = gd - axis + theta / 2 + den.height
        w = max(num.width, den.width)
        out = MBox(w, u + num.height, v + den.depth)
        out.add(num, (w - num.width) / 2, u)
        out.add(den, (w - den.width) / 2, -v)
        out.rules.append((0.0, axis - theta / 2, w, theta))
        return out

    def radical(self, atoms, style):
        body = self.layout(atoms, style, True)
        gap = self.k("RadicalDisplayStyleVerticalGap" if style == D else "RadicalVerticalGap", style)
        theta = self.k("RadicalRuleThickness", style)
        extra = self.k("RadicalExtraAscender", style)
        sign = self.char("√", style)
        tall = body.height + gap + theta + body.depth
        f = max(1.0, tall / max(sign.height + sign.depth, 1e-6))
        for x, y, p in sign.glyphs:
            p.size *= f
            p.advs = [a * f for a in p.advs]
        sw = sign.width * f
        shift = (body.height + gap + theta) - sign.height * f
        out = MBox(sw + body.width, body.height + gap + theta + extra, max(body.depth, sign.depth * f - shift))
        out.glyphs += [(x * f, y + shift, p) for x, y, p in sign.glyphs]
        out.add(body, sw, 0)
        out.rules.append((sw - theta * 0.5, body.height + gap, body.width + theta * 0.5, theta))
        return out


def _fix_bins(atoms):
    """Rules 5 and 6: a Bin with no left operand, or before Rel/Close/Punct, becomes Ord."""
    out, prev = [], None
    for k, a in enumerate(atoms):
        if a.cls == BIN and (prev is None or prev in (BIN, OP, REL, OPEN, PUNCT)):
            a = Atom(ORD, a.nucleus, a.sup, a.sub)
        if a.cls in (REL, CLOSE, PUNCT) and out and out[-1].cls == BIN:
            out[-1] = Atom(ORD, out[-1].nucleus, out[-1].sup, out[-1].sub)
        out.append(a)
        if a.cls != -1:
            prev = a.cls
    return out
