"""Vertical material and the page builder (after TeX's build_page, tex.web §980-1028).

The page is filled with lines, glue and penalties; each legal breakpoint
gets a cost from the page badness plus its penalty, and the least-cost
break so far is taken when the page overflows. Club, widow and
keep-with-next penalties arrive from the layout as Penalty items.

Floats (a figure with its caption, a table with its caption) arrive as
VFloat, kept whole. As in LaTeX's output routine, a float that fits in the
space left on the page is set where it stands; one that does not is
deferred: the text after it fills the page, and the float goes at the top
of the next page. Deferred floats keep their order, and a float that
arrives while others wait joins the queue behind them.
"""
from dataclasses import dataclass, field

from .breaker import badness, INF_BAD, INF_PENALTY, EJECT

AWFUL = 1 << 30
DEPLORABLE = 100000


@dataclass
class VLine:
    height: float
    depth: float
    runs: list           # (x, y_offset, Piece, hscale)
    graphics: list = field(default_factory=list)
    block: int = -1
    line_no: int = 0
    links: list = field(default_factory=list)


@dataclass
class VGlue:
    width: float
    stretch: float = 0.0
    shrink: float = 0.0


@dataclass
class VPenalty:
    penalty: int


@dataclass
class VFloat:
    items: list                                   # the float's own glue, penalties and lines, never broken
    blocks: list = field(default_factory=list)    # the document blocks it holds

    def lines(self):
        return [x for x in self.items if isinstance(x, VLine)]


@dataclass
class Page:
    width: float
    height: float
    items: list = field(default_factory=list)     # placed (y_baseline, VLine), in reading order
    number: int = 1
    furniture: list = field(default_factory=list)
    breaks: list = field(default_factory=list)    # (block, line) of the last line


def _sp(x):
    return int(round(x * 65536))


class _Fill:
    """The natural height, stretch and shrink of the page so far."""

    def __init__(self, top_skip):
        self.top_skip, self.h, self.st, self.sh, self.depth, self.started = top_skip, 0.0, 0.0, 0.0, 0.0, False

    def take(self, it):
        if isinstance(it, VLine):
            self.h = max(self.top_skip, it.height) if not self.started else self.h + self.depth + it.height
            self.started, self.depth = True, it.depth
        elif isinstance(it, VGlue) and self.started:
            self.h, self.st, self.sh = self.h + it.width, self.st + it.stretch, self.sh + it.shrink

    def copy(self):
        out = _Fill(self.top_skip)
        out.__dict__.update(self.__dict__)
        return out

    def total(self, max_depth):
        return self.h + max(0.0, self.depth - max_depth)


def build_pages(vlist, text_height, top_skip, ragged_bottom=True, max_depth=5.0, float_sep=12.0):
    """Split a vertical list into page bodies; returns lists of (baseline y, VLine), in reading order."""
    pages, i, n, queue, last_block = [], 0, len(vlist), [], None
    float_lines = {id(ln) for f in vlist if isinstance(f, VFloat) for ln in f.lines()}
    while i < n or queue:
        while i < n and isinstance(vlist[i], (VGlue, VPenalty)):
            i += 1
        fill, top = _Fill(top_skip), []
        while queue and (not top or _fits(fill, _lead(queue[0].items, float_sep), text_height, max_depth)):
            f = queue.pop(0)
            for it in _lead(f.items, float_sep if top else 0.0):
                fill.take(it)
            top.append(f)
        if top and i < n:
            fill.take(VGlue(float_sep))
        best, inline, deferred = _fill_page(vlist, i, fill, text_height, max_depth, bool(queue))
        body = []
        for k in range(i, best):
            it = vlist[k]
            if isinstance(it, VFloat):
                if k in inline:
                    body += it.items
                else:
                    queue.append(it)
            else:
                body.append(it)
        while body and not isinstance(body[0], VLine):
            body.pop(0)
        head = [x for f_n, f in enumerate(top) for x in _lead(f.items, float_sep if f_n else 0.0)]
        if head and body:
            head.append(VGlue(float_sep))
        placed = _place(head + body, top_skip)
        placed = _reading_order(placed, sum(len(f.lines()) for f in top), last_block)
        text = [ln.block for _, ln in placed if id(ln) not in float_lines]
        last_block = text[-1] if text else last_block
        pages.append(placed)
        i = best
    return pages


def _lead(items, sep):
    """A float's items from its first line on, with sep of glue in front when sep is not zero."""
    k = next((n for n, x in enumerate(items) if isinstance(x, VLine)), len(items))
    return ([VGlue(sep)] if sep else []) + items[k:]


def _fits(fill, items, text_height, max_depth):
    trial = fill.copy()
    for it in items:
        trial.take(it)
    return trial.total(max_depth) <= text_height + 1e-6


def _fill_page(vlist, i, fill, text_height, max_depth, waiting):
    """TeX's least-cost page break from vlist[i] on. Returns (break index, inline float indices, deferred)."""
    n = len(vlist)
    best, best_cost, j, forced = None, AWFUL, i, False
    inline, deferred = set(), []
    while j < n:
        it = vlist[j]
        if isinstance(it, VFloat):
            if not waiting and not deferred and _fits(fill, it.items, text_height, max_depth):
                for x in it.items:
                    fill.take(x)
                inline.add(j)
            else:
                deferred.append(j)
            j += 1
            continue
        if isinstance(it, VLine):
            fill.take(it)
            j += 1
            continue
        legal = isinstance(it, VPenalty) and it.penalty < INF_PENALTY or \
            isinstance(it, VGlue) and isinstance(vlist[j - 1], (VLine, VFloat))
        if legal and fill.started:
            pi = it.penalty if isinstance(it, VPenalty) else 0
            hh = fill.total(max_depth)
            if hh <= text_height + 1e-6:
                b = badness(_sp(text_height - hh), _sp(fill.st))
            elif hh - text_height > fill.sh:
                b = AWFUL
            else:
                b = badness(_sp(hh - text_height), _sp(fill.sh))
            if b >= AWFUL:
                c = AWFUL
            elif pi <= EJECT:
                c = pi
            else:
                c = b + pi if b < INF_BAD else DEPLORABLE
            if c <= best_cost:
                best, best_cost = j, c
            if c == AWFUL or pi <= EJECT:
                forced = True
                break
        fill.take(it)
        j += 1
    if not forced:
        best = n
    elif best is None:
        best = max(j, i + 1)
    return best, inline, deferred


def _reading_order(placed, n_top, last_block):
    """Floats set at the top of a page read after the lines that finish a paragraph split from the page
    before, so that paragraph reads whole."""
    if not n_top or last_block is None:
        return placed
    top, body = placed[:n_top], placed[n_top:]
    k = 0
    while k < len(body) and body[k][1].block == last_block:
        k += 1
    return body[:k] + top + body[k:]


def float_moves(pages, floats):
    """[(float blocks, block it now reads after)] for every float that reads later than its source position."""
    owner = {b: f_n for f_n, f in enumerate(floats) for b in f.blocks}
    moves, seen, last = [], set(), -1
    for page in pages:
        for _, ln in page:
            f_n = owner.get(ln.block)
            if f_n is None:
                last = ln.block
            elif f_n not in seen:
                seen.add(f_n)
                if last > floats[f_n].blocks[0]:
                    moves.append((list(floats[f_n].blocks), last))
    return moves


def _place(items, top_skip):
    placed, y, prev_depth, first = [], 0.0, 0.0, True
    for it in items:
        if isinstance(it, VLine):
            if first:
                y = max(top_skip, it.height)
                first = False
            else:
                y += prev_depth + it.height
            placed.append((y, it))
            prev_depth = it.depth
        elif isinstance(it, VGlue) and not first:
            y += it.width
    return placed
