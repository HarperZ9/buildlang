"""Glyph shaping: cmap lookup, fallback chain, ligatures, pair kerning."""
import os
from dataclasses import dataclass, field


@dataclass
class Piece:
    font: str            # font key in the registry
    size: float          # pt
    gids: list
    advs: list           # pt, kerning folded into the advance of the left glyph
    text: str            # source text (for ToUnicode and extraction)
    clusters: list       # text slice per glyph
    color: str = ""
    smallcaps: bool = False

    @property
    def width(self):
        return sum(self.advs)


@dataclass
class Diagnostics:
    missing: list = field(default_factory=list)   # (char, font key)
    overfull: list = field(default_factory=list)  # (block index, line, excess pt[, table cell])
    underfull: list = field(default_factory=list)
    notes: list = field(default_factory=list)
    math_errors: list = field(default_factory=list)  # (math source, error)
    fallbacks: list = field(default_factory=list)    # (char, font key asked for, shipped font file used)

    def report(self):
        out = []
        for ch, key, file in sorted(set(self.fallbacks)):
            out.append(f"glyph fallback: U+{ord(ch):04X} {ch!r} not in {key}, set in {file}")
        for ch, key in self.missing:
            out.append(f"missing glyph U+{ord(ch):04X} {ch!r} in {key} (and its fallbacks)")
        for entry in self.overfull:
            out.append(f"overfull line: {overfull_where(entry)} by {entry[2]:.2f}pt")
        for b, line, bad in self.underfull:
            out.append(f"underfull line: block {b} line {line} badness {bad}")
        for src, err in self.math_errors:
            out.append(f"math error: {err} in {src!r}")
        return out + list(self.notes)


class Shaper:
    def __init__(self, registry, diagnostics, ligatures=True, kerning=True):
        self.reg = registry
        self.diag = diagnostics
        self.ligatures = ligatures
        self.kerning = kerning

    def shape(self, key, size, text, color="", fallbacks=()):
        """Shape `text` in font `key`; characters the font lacks use fallbacks."""
        out, chain = [], [key] + [f for f in fallbacks if f != key]
        run_key, run = None, ""
        tl = getattr(self.reg[key], "text_ligs", None) if self.ligatures else None
        units, i = [], 0
        while i < len(text):
            for n in (3, 2):
                if tl and text[i:i + n] in tl:
                    units.append(text[i:i + n])
                    i += n
                    break
            else:
                units.append(text[i])
                i += 1
        for ch in units:
            use = None
            for k in chain:
                f = self.reg[k]
                if (len(ch) > 1 and ch in getattr(f, "text_ligs", {})) or f.glyph(ch[0]) or ch in "  ":
                    use = k
                    break
            if use is None and len(ch) == 1 and hasattr(self.reg, "glyph_fallback"):
                # no theme font has it: a font shipped in data/fonts that does, named in the receipt
                use = self.reg.glyph_fallback(ch, chain)
                if use is not None:
                    self.diag.fallbacks.append((ch, key, os.path.basename(self.reg.paths[use])))
            if use is None:
                self.diag.missing.append((ch, key))
                use = key
            if use != run_key and run:
                out.append(self._shape_run(run_key, size, run, color))
                run = ""
            run_key = use
            run += ch
        if run:
            out.append(self._shape_run(run_key, size, run, color))
        return out

    def _shape_run(self, key, size, text, color):
        font = self.reg[key]
        scale = size / font.upem
        tl = getattr(font, "text_ligs", {}) if self.ligatures else {}
        clusters, i = [], 0
        while i < len(text):
            for n in (3, 2):
                if text[i:i + n] in tl:
                    clusters.append(text[i:i + n])
                    i += n
                    break
            else:
                clusters.append(text[i])
                i += 1
        gids = [tl[c] if len(c) > 1 else font.glyph(" " if c == " " else c) for c in clusters]
        if self.ligatures and font.ligatures:
            gids, clusters = _ligate(font.ligatures, gids, clusters)
        advs = [font.advance(g) * scale for g in gids]
        if self.kerning:
            for i in range(len(gids) - 1):
                k = font.kern(gids[i], gids[i + 1])
                if k:
                    advs[i] += k * scale
        return Piece(key, size, gids, advs, text, clusters, color)


def _ligate(ligs, gids, clusters):
    longest = max((len(k) for k in ligs), default=0)
    out_g, out_c, i = [], [], 0
    while i < len(gids):
        for n in range(min(longest, len(gids) - i), 1, -1):
            lig = ligs.get(tuple(gids[i:i + n]))
            if lig is not None:
                out_g.append(lig)
                out_c.append("".join(clusters[i:i + n]))
                i += n
                break
        else:
            out_g.append(gids[i])
            out_c.append(clusters[i])
            i += 1
    return out_g, out_c


def overfull_where(entry):
    """Where an overfull line is: "block 12 line 3", or for a table cell "block 12, table row 3, column 2"."""
    b, line = entry[0], entry[1]
    cell = entry[3] if len(entry) > 3 else None
    if not cell:
        return f"block {b} line {line}"
    row = "the header row" if cell["row"] == "header" else f"table row {cell['row']}"
    tail = f", line {cell['cell_line']} of the cell" if cell.get("cell_line", 1) > 1 else ""
    return f"block {b}, {row}, column {cell['column']}{tail}"
