"""Document layout: blocks to lines to pages, driven by a theme."""
import copy

from .breaker import Box, Glue, Penalty, Params, break_lines, Result, SP, EJECT, INF_PENALTY
from .fonts import Registry
from .hlist import ParagraphBuilder, sp
from .hyph import Hyphenator
from .pages import VLine, VGlue, VPenalty, Page, build_pages
from .refs import resolve_refs
from .shape import Shaper, Diagnostics


CELL_ALIGN = {"l": "left", "r": "right", "c": "center"}


def merged(base, over):
    out = copy.deepcopy(base)
    for k, v in over.items():
        if isinstance(v, dict) and isinstance(out.get(k), dict):
            out[k] = merged(out[k], v)
        else:
            out[k] = v
    return out


class Layout:
    def __init__(self, doc, theme):
        self.doc, self.theme = doc, theme
        self.diag = Diagnostics()
        self.reg = Registry(theme["fonts"])
        self.shaper = Shaper(self.reg, self.diag, theme.get("ligatures", True), theme.get("kerning", True))
        self.hyph = Hyphenator()
        for w in theme.get("hyphen_exceptions", []):
            self.hyph.add_exception(w)
        self.pb = ParagraphBuilder(self.shaper, self.hyph, theme)
        pg = theme["page"]
        self.text_width = pg["width"] - pg["left"] - pg["right"]
        self.text_height = pg["height"] - pg["top"] - pg["bottom"]
        lb = theme.get("linebreak", {})
        self.params = Params(**{k: v for k, v in lb.items() if k in Params.__dataclass_fields__})
        if "emergency_stretch" in lb:
            self.params.emergency_stretch = sp(lb["emergency_stretch"])
        self.line_log = []          # (block, [break item text]) for agreement checks

    def style(self, kind):
        blocks = self.theme.get("blocks", {})
        st = merged(self.theme["base"], blocks.get(kind, {}))
        st["kind"] = kind
        return st

    # -- paragraphs -----------------------------------------------------
    def lines_for(self, block, st, lead=None):
        left = st.get("left", 0.0)
        right = st.get("right", 0.0)
        hang = st.get("hang", 0.0)              # first-line offset relative to `left`
        width = self.text_width - left - right
        align = st.get("align", "justify")
        items = self.pb.build(block.spans, st, lead, ragged=align in ("ragged", "center", "right"))
        first_w = width - hang
        lw = lambda n: sp(first_w if n == 1 else width)
        if align == "firstfit":
            res = first_fit(items, lw)
        else:
            params = self.params
            if align in ("ragged", "center", "right"):
                import dataclasses
                params = dataclasses.replace(self.params, rightskip_stretch=sp(2 * st["size"]))
            res = break_lines(items, lw, params, has_hyphens=True)
        for line, excess in res.overfull:
            self.diag.overfull.append((block.index, line, excess))
        out = []
        prev = -1
        for n, (b, r) in enumerate(zip(res.breaks, res.ratios), 1):
            runs, nat = set_line(items, prev, b, r, self.theme.get("microtype", {}).get("expansion", 0))
            x0 = left + (hang if n == 1 else 0)
            w = first_w if n == 1 else width
            if align == "center":
                x0 += (w - nat) / 2
            elif align == "right":
                x0 += w - nat
            flat, gfx = [], []
            h, d = st.get("height", st["size"] * 0.72), st.get("depth", st["size"] * 0.25)
            for x, pc, hs in runs:
                if hasattr(pc, "glyphs"):
                    flat += [(x0 + x + gx, gy, g, 1.0) for gx, gy, g in pc.glyphs]
                    gfx += [(x0 + x + rx, ry, rw, rh) for rx, ry, rw, rh in pc.rules]
                    h, d = max(h, pc.height), max(d, pc.depth)
                else:
                    flat.append((x0 + x, 0.0, pc, hs))
            ln = VLine(h, d, flat, gfx, block=block.index, line_no=n)
            out.append(ln)
            prev = b
        self.line_log.append((block.index, [_line_text(l) for l in out]))
        return out

    def display_math(self, block, st):
        box = self.pb.math_box(block.plain, st, display=True).payload[0]
        x0 = st.get("left", 0.0) + (self.text_width - st.get("left", 0.0) - st.get("right", 0.0) - box.width) / 2
        runs = [(x0 + gx, gy, g, 1.0) for gx, gy, g in box.glyphs]
        gfx = [(x0 + rx, ry, rw, rh) for rx, ry, rw, rh in box.rules]
        ln = VLine(max(box.height, st["size"] * 0.72), max(box.depth, st["size"] * 0.25), runs, gfx,
                   block=block.index, line_no=1)
        if st.get("number") and getattr(block, "number", None) is not None:
            tag = st["number"].replace("{n}", str(block.number))
            pieces = self.shaper.shape(st["fonts"]["rm"], st["size"], tag, st.get("color", ""))
            x = self.text_width - st.get("right", 0.0) - sum(p.width for p in pieces)
            for pc in pieces:
                ln.runs.append((x, 0.0, pc, 1.0))
                x += pc.width
        self.line_log.append((block.index, [block.plain]))
        return [ln]

    # -- figures and table rows -------------------------------------------
    def block_lines(self, block, st, lead):
        if st.get("display_math"):
            return self.display_math(block, st)
        if st.get("figure"):
            return self.figure_lines(block, st, lead)
        if block.cells:
            return [self.row_line(block, st)]
        self.cols = self.aligns = None
        return self.lines_for(block, st, lead)

    def figure_lines(self, block, st, lead):
        """The image as one line of its own height, then the caption, kept on one page."""
        import os
        from .pngread import read_png
        src = block.attrs.get("src", "")
        path = src if os.path.isabs(src) else os.path.join(self.doc.base, src)
        pw, ph, rgb = read_png(path, tuple(int(self.theme.get("paper", "#ffffff").lstrip("#")[i:i + 2], 16)
                                           for i in (0, 2, 4)))
        w = self.text_width * float(block.attrs.get("width", st.get("figure_width", 1.0)))
        h = w * ph / pw
        cap = self.text_height * st.get("figure_max_height", 0.62)
        if h > cap:
            w, h = w * cap / h, cap
        x0 = (self.text_width - w) / 2
        img = {"type": "image", "x": x0, "ry": 0.0, "w": w, "h": h, "px": (pw, ph), "rgb": rgb,
               "role": "figure", "alt": block.attrs.get("alt", "")}
        pic = VLine(h, st.get("caption_gap", 8.0), [], [img], block=block.index, line_no=0)
        return [pic] + self.lines_for(block, st, lead)

    def row_line(self, block, st):
        """A table row: each cell set in its own column, all cells side by side in one line of the page."""
        n = len(block.cells)
        if block.attrs.get("cols"):
            self.cols = [float(c) for c in block.attrs["cols"].split(",")]
        if block.attrs.get("align"):
            # align=l,r,c on a row holds for the rest of the table, as cols= does
            self.aligns = [CELL_ALIGN.get(a.strip(), "") for a in block.attrs["align"].split(",")]
        cols = self.cols if getattr(self, "cols", None) and len(self.cols) == n else [1.0 / n] * n
        aligns = getattr(self, "aligns", None) or []
        total, gutter = sum(cols), st.get("gutter", 8.0)
        x, runs, gfx, height, depth = 0.0, [], [], 0.0, 0.0
        from .docir import Block
        for k, (frac, spans) in enumerate(zip(cols, block.cells)):
            w = self.text_width * frac / total
            right = self.text_width - x - w + (gutter if k < n - 1 else 0.0)
            cst = dict(st, left=x, right=right, hang=0.0)
            if k < len(aligns) and aligns[k]:
                cst["align"] = aligns[k] if aligns[k] != "left" else st.get("align", "ragged")
            cell = Block(block.kind, spans or [], index=block.index)
            lines = self.lines_for(cell, cst) if cell.plain.strip() else []
            for j, ln in enumerate(lines):
                shift = -j * st["leading"]
                runs += [(rx, dy + shift, pc, hs) for rx, dy, pc, hs in ln.runs]
                gfx += [(gx, gy + shift, gw, gh) for gx, gy, gw, gh in ln.graphics]
            if lines:
                height = max(height, lines[0].height)
                depth = max(depth, (len(lines) - 1) * st["leading"] + lines[-1].depth)
            x += w
        height = height or st["size"] * 0.72
        depth = max(depth, st["size"] * 0.25) + st.get("row_pad", 2.0)
        below = st.get("rule_below") or (st.get("rule_last", 0.9) if block.attrs.get("last") else 0)
        if below:
            gfx.append((0.0, -depth, self.text_width, below))
        if block.attrs.get("last"):
            depth += st.get("table_after", 0.0)    # room under the closing rule
        if st.get("rule_above"):
            gfx.append((0.0, height + st.get("row_pad", 2.0), self.text_width, st["rule_above"]))
        return VLine(height, depth, runs, gfx, block=block.index, line_no=1)

    # -- the whole document -----------------------------------------------
    def run(self):
        resolve_refs(self.doc, self.theme)
        vlist, prev_after, prev_depth, prev_tail = [], None, 0.0, 0.0
        mode = self.theme.get("spacing", "tex")
        blocks = self.doc.blocks
        pending_lead, kept = None, False
        for i, block in enumerate(blocks):
            if block.kind == "pagebreak":
                vlist.append(VPenalty(EJECT))
                prev_after = None
                continue
            st = self.style(block.kind)
            if st.get("hidden"):
                continue
            if st.get("run_in") and i + 1 < len(blocks) and blocks[i + 1].kind in st["run_in"]:
                pending_lead = self.lead_pieces(block, st)
                continue
            lead = self.prefix(block, st)
            if pending_lead:
                lead = pending_lead + (lead or [])
                pending_lead = None
            lines = self.block_lines(block, st, lead)
            if not lines:
                continue
            before = st.get("before", 0.0)
            if "first_before" in st and (i == 0 or blocks[i - 1].kind != block.kind):
                before = st["first_before"]
            if mode == "max":
                for ln in lines:
                    ln.height, ln.depth = st["size"], st["leading"] - st["size"]
            if vlist and prev_after is not None:
                if mode == "max":
                    dist = prev_tail + max(prev_after, before) + st["size"]
                elif mode == "addvspace":
                    # within a list, the list-end skip (after) belongs only to the last item
                    same = i > 0 and blocks[i - 1].kind == block.kind and "first_before" in st
                    dist = (before if same else max(prev_after, before)) + st["leading"]
                else:
                    dist = prev_after + before + st["leading"]
                if st.get("penalty_before") is not None:
                    vlist.append(VPenalty(st["penalty_before"]))
                gap = dist - prev_depth - lines[0].height
                if gap < 0:   # a tall first line (a figure, a table row): baselines give way to lineskip
                    gap = prev_after + before + self.theme.get("lineskip", 1.0)
                vlist.append(VGlue(gap, st.get("before_stretch", 0.0), st.get("before_shrink", 0.0)))
            elif vlist:
                vlist.append(VGlue(st["leading"] - prev_depth - lines[0].height))
            if st.get("null_before") is not None:
                vlist.append(VLine(0.0, 0.0, []))
                vlist.append(VGlue(st["null_before"] - lines[0].height))
            whole =self.theme.get("keep_next_whole") and kept
            for k, ln in enumerate(lines):
                if k:
                    pen = INF_PENALTY if whole or st.get("unbreakable") else self.interline_penalty(k, len(lines), st)
                    if pen:
                        vlist.append(VPenalty(pen))
                    gap = st["leading"] - lines[k - 1].depth - ln.height
                    vlist.append(VGlue(gap if gap >= 0 else self.theme.get("lineskip", 1.0)))
                vlist.append(ln)
            kept = bool(st.get("keep_next")) or bool(block.cells and st.get("keep_table") and not block.attrs.get("last"))
            if kept:
                vlist.append(VPenalty(INF_PENALTY))
            prev_after, prev_depth = st.get("after", 0.0), lines[-1].depth
            prev_tail = st["leading"] - st["size"]
        pg = self.theme["page"]
        bodies = build_pages(vlist, self.text_height, pg.get("top_skip", 10.0), pg.get("ragged_bottom", True),
                             pg.get("max_depth", 5.0))
        pages = []
        for n, body in enumerate(bodies, 1):
            page = Page(pg["width"], pg["height"], body, n)
            if body:
                page.breaks.append((body[-1][1].block, body[-1][1].line_no))
            pages.append(page)
        return pages

    def interline_penalty(self, k, n, st):
        club = self.theme.get("club_penalty", 150)
        widow = self.theme.get("widow_penalty", 150)
        pen = self.theme.get("interline_penalty", 0)
        if k == 1:
            pen += club
        if k == n - 1:
            pen += widow
        return min(pen, INF_PENALTY)

    def prefix(self, block, st):
        label = st.get("label")
        if not label:
            return None
        text = label.replace("{n}", str(getattr(block, "number", "")))
        font = st["fonts"].get("label", st["fonts"]["rm"])
        pieces = self.shaper.shape(font, st["size"], text, st.get("color", ""))
        gap = st.get("label_sep", st["size"] * 0.5)
        if st.get("label_width"):
            pad = max(st["label_width"] - sum(p.width for p in pieces), 0) + gap
        else:
            pad = gap
        if pieces:
            pieces[-1].advs[-1] += pad
            pieces[-1].trail = pad      # blank space after the label; the PDF writer puts a real space there
        return pieces

    def lead_pieces(self, block, st):
        pieces = []
        for span in block.spans:
            font = self.pb.font_for(st, span.style or "b")
            pieces += self.shaper.shape(font, st["size"], span.text, st.get("color", ""))
        if pieces:
            pieces[-1].advs[-1] += st.get("run_in_sep", st["size"])
        return pieces


def set_line(items, prev, b, ratio, exp):
    """Place the items of one line. Returns ([(x, piece, hscale)], natural width)."""
    from .breaker import _after_break
    start = _after_break(items, prev)
    x, runs = 0.0, []
    end = b
    first = items[start] if start < len(items) else None
    if isinstance(first, Box) and first.pro_left:
        x -= first.pro_left / SP
    for j in range(start, end + 1):
        it = items[j]
        if isinstance(it, Box):
            scale = 1.0
            if exp and it.width:
                scale = 1.0 + max(-1.0, min(1.0, ratio)) * (it.stretch if ratio > 0 else it.shrink) / it.width
            for pc in it.payload or []:
                if hasattr(pc, "glyphs"):
                    runs.append((x, pc, 1.0))
                    x += pc.width
                    continue
                runs.append((x, pc, scale))
                x += pc.width * scale
            if not it.payload:
                x += it.width / SP
        elif isinstance(it, Glue) and j < end:
            g = it.width
            if ratio > 0 and it.stretch_order == 0:
                g += ratio * it.stretch
            elif ratio < 0:
                g += max(ratio, -1.0) * it.shrink
            x += g / SP
        elif isinstance(it, Penalty) and j == end and it.flagged and getattr(it, "hyphen", None):
            runs.append((x, it.hyphen, 1.0))
            x += it.hyphen.width
    return runs, x


def first_fit(items, line_width):
    """Greedy filling, as a word processor or ReportLab's Paragraph does."""
    breaks, n, last_ok, w, sh = [], 1, None, 0, 0
    for i, it in enumerate(items):
        end = isinstance(it, Penalty) and it.penalty == EJECT
        legal = isinstance(it, Glue) and i > 0 and isinstance(items[i - 1], Box)
        if legal or end:
            if w - sh > line_width(n) + 655 and last_ok is not None:  # 0.01 pt for sp rounding
                breaks.append(last_ok)
                n += 1
                seg = [x for x in items[last_ok + 1:i] if isinstance(x, (Box, Glue))]
                w = sum(x.width for x in seg)
                sh = sum(x.shrink for x in seg if isinstance(x, Glue))
                last_ok = None
            if end:
                breaks.append(i)
                break
            last_ok = i
        if isinstance(it, (Box, Glue)):
            w += it.width
            sh += it.shrink if isinstance(it, Glue) else 0
    return Result(breaks, [0.0] * len(breaks), [0] * len(breaks), 0, 1)


def _line_text(line):
    return "".join("".join(pc.clusters) + "" for _, _, pc, _ in line.runs)
