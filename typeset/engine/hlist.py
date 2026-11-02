"""Paragraph to Knuth-Plass item list: words, hyphenation, TeX spacing, microtype."""
import re

from .breaker import Box, Glue, Penalty, INF_PENALTY, EJECT, SP

# TeX space factors (plain.tex \nonfrenchspacing, sfcodes)
SFCODE = {".": 3000, "?": 3000, "!": 3000, ":": 2000, ";": 1500, ",": 1250}
CLOSERS = set(")]'’”\"")

# Protrusion factors (fraction of glyph width), after microtype's default set.
PRO_RIGHT = {".": 0.7, ",": 0.5, "-": 0.7, "–": 0.5, "—": 0.3, ":": 0.5, ";": 0.5,
             "!": 0.2, "?": 0.2, "’": 0.7, "”": 0.5, ")": 0.05}
PRO_LEFT = {"‘": 0.7, "“": 0.5, "(": 0.05, "A": 0.05, "T": 0.05, "V": 0.05,
            "W": 0.05, "Y": 0.05}

WORD = re.compile(r"[ \t\n]+|[^ \t\n]+")


def sp(x):
    return int(round(x * SP))


class ParagraphBuilder:
    def __init__(self, shaper, hyphenator, theme):
        self.shaper = shaper
        self.hyph = hyphenator
        self.theme = theme

    def font_for(self, style, span_style):
        roles = style["fonts"]
        if span_style in roles:
            return roles[span_style]
        if span_style == "bi" and "b" in roles:
            return roles["b"]
        return roles["rm"]

    def build(self, spans, style, lead=None, ragged=False):
        """Return items for one paragraph. `lead` is a list of pieces set first."""
        size = style["size"]
        tex = self.theme.get("tex_spacing", True)
        micro = self.theme.get("microtype", {})
        exp = micro.get("expansion", 0.0)
        protrude = micro.get("protrusion", False)
        hyphenate = style.get("hyphenate", self.theme.get("hyphenate", True))
        space_font = self.font_for(style, "")
        sp_font = self.shaper.reg[space_font]
        dims = self.theme.get("space", {"width": 0.333, "stretch": 0.1667, "shrink": 0.1111, "extra": 0.1111})
        em = size
        items = []
        if style.get("indent"):
            items.append(Box(sp(style["indent"])))
        for piece in lead or []:
            items.append(Box(sp(piece.width), payload=[piece]))
        words = []           # list of [ (span_style, text, color) ... ] per word
        cur, gap_before = [], False
        for span in spans:
            if span.style in ("ref", "cite"):
                cur.append(("", span.text, span.ref))
                continue
            if span.style == "m":
                cur.append(("m", span.text, ""))
                continue
            for tok in WORD.findall(span.text):
                if tok.isspace():
                    if cur:
                        words.append(cur)
                        cur = []
                else:
                    cur.append((span.style, tok.replace("~", " "), ""))
        if cur:
            words.append(cur)
        last_char = ""
        for wi, word in enumerate(words):
            if wi > 0:
                sf = 1000
                if tex:
                    k = len(last_char) - 1
                    while k >= 0 and last_char[k] in CLOSERS:
                        k -= 1
                    ch = last_char[k] if k >= 0 else ""
                    sf = SFCODE.get(ch, 1000)
                    if sf == 3000 and k >= 1 and last_char[k - 1].isupper():
                        sf = 999 if not (k >= 2 and last_char[k - 2].isupper()) else 1000
                w = dims["width"] * em
                st, sh = dims["stretch"] * em, dims["shrink"] * em
                if sf >= 2000:
                    w += dims["extra"] * em
                st, sh = st * sf / 1000, sh * 1000 / sf
                if ragged:
                    # stretch lives in the rightskip (Params.rightskip_stretch), so spaces stay natural
                    items.append(Glue(sp(dims["width"] * em), 0, sp(sh), payload="space"))
                else:
                    items.append(Glue(sp(w), sp(st), sp(sh), payload="space"))
            for si, (sty, text, color) in enumerate(word):
                if sty == "m":
                    items.append(self.math_box(text, style))
                    continue
                font = self.font_for(style, sty)
                if sty == "sc" and "sc" not in style["fonts"]:
                    text = text.upper()
                pieces = self.shaper.shape(font, size, text, color or style.get("color", ""),
                                           style.get("fallbacks", ()))
                for pi, piece in enumerate(pieces):
                    first = si == 0 and pi == 0
                    self._emit_word(items, piece, hyphenate and first and sty not in ("tt", "m"),
                                    exp, protrude, style)
            last_char = word[-1][1]
        items.append(Penalty(0, INF_PENALTY))
        items.append(Glue(0, sp(style.get("parfill", 0)) or 1, 0, stretch_order=1 if not style.get("parfill") else 0))
        items.append(Penalty(0, EJECT))
        return items

    def math_box(self, src, style, display=False):
        from .mathlayout import MathLayout, parse, D, T
        key = style["fonts"].get("math", "math")
        size = style["size"] * self.theme.get("math_scale", 1.0)
        ml = self.math_cache.get((key, size)) if hasattr(self, "math_cache") else None
        if ml is None:
            self.math_cache = getattr(self, "math_cache", {})
            ml = self.math_cache[(key, size)] = MathLayout(self.shaper.reg[key], key, size)
        box = ml.layout(parse(src), D if display else T)
        return Box(sp(box.width), payload=[box])

    def _emit_word(self, items, piece, hyphenate, exp, protrude, style):
        text = "".join(piece.clusters)
        cuts = set()
        if hyphenate:
            # TeX hyphenates only the first run of letters after the glue (TeXbook ch. 14)
            m = re.match(r"[^A-Za-z]*([A-Za-z]+)", text)
            if m and len(m.group(1)) >= 5:
                for p in self.hyph.points(m.group(1)):
                    cuts.add(m.start(1) + p)
        url = "://" in text or re.search(r"[A-Za-z0-9-]+\.[a-z]{2,}/", text) is not None
        for m in re.finditer(r"[.:/\-_?&=#]" if url else r"[-–—/]", text):
            if url and text[m.end():m.end() + 1] == "/":
                continue          # url.sty keeps // together
            if 0 < m.end() < len(text):
                cuts.add(m.end())
        bounds, pos = [0], 0
        for i, c in enumerate(piece.clusters):
            pos += len(c)
            if pos in cuts and i + 1 < len(piece.clusters):
                bounds.append(i + 1)
        bounds.append(len(piece.clusters))
        hy = None
        if len(bounds) > 2:
            hy = self.shaper.shape(self.font_for(style, ""), piece.size, "-", "",
                                   tuple(style.get("fallbacks", ())) + ("rm",))[0]
        for k in range(len(bounds) - 1):
            a, b = bounds[k], bounds[k + 1]
            frag = _slice(piece, a, b)
            w = frag.width
            pl = PRO_LEFT.get(frag.clusters[0][:1], 0) * frag.advs[0] if protrude and frag.advs else 0
            pr = PRO_RIGHT.get(frag.clusters[-1][-1:], 0) * frag.advs[-1] if protrude and frag.advs else 0
            items.append(Box(sp(w), sp(w * exp), sp(w * exp), sp(pl), sp(pr), payload=[frag]))
            if k < len(bounds) - 2:
                explicit = frag.clusters[-1][-1:] in "-–—/"
                if url:
                    # url.sty: break after . / - _ ? & = # at \UrlBreakPenalty, with no hyphen
                    items.append(Penalty(0, self.theme.get("url_break_penalty", 700), False, payload="url"))
                elif explicit:
                    items.append(Penalty(0, self.theme.get("exhyphenpenalty", 50), True, payload="exhyph"))
                else:
                    hpr = sp(PRO_RIGHT["-"] * hy.width) if protrude else 0
                    items.append(Penalty(sp(hy.width), self.theme.get("hyphenpenalty", 50), True,
                                         hpr, payload="hyph"))
                    items[-1].hyphen = hy


def _slice(piece, a, b):
    from .shape import Piece
    clusters = piece.clusters[a:b]
    return Piece(piece.font, piece.size, piece.gids[a:b], piece.advs[a:b], "".join(clusters),
                 clusters, piece.color, piece.smallcaps)
