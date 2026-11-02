"""Vertical material and the page builder (after TeX's build_page, tex.web §980-1028).

The page is filled with lines, glue and penalties; each legal breakpoint
gets a cost from the page badness plus its penalty, and the least-cost
break so far is taken when the page overflows. Club, widow and
keep-with-next penalties arrive from the layout as Penalty items.
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
class Page:
    width: float
    height: float
    items: list = field(default_factory=list)     # placed (y_baseline, VLine)
    number: int = 1
    furniture: list = field(default_factory=list)
    breaks: list = field(default_factory=list)    # (block, line) of the last line


def _sp(x):
    return int(round(x * 65536))


def build_pages(vlist, text_height, top_skip, ragged_bottom=True, max_depth=5.0):
    """Split a vertical list into page bodies; returns lists of (baseline y, VLine)."""
    pages, i, n = [], 0, len(vlist)
    while i < n:
        while i < n and not isinstance(vlist[i], VLine):
            i += 1
        if i >= n:
            break
        h = st = sh = prev_depth = 0.0
        best, best_cost, started, j, forced = None, AWFUL, False, i, False
        while j < n:
            it = vlist[j]
            if isinstance(it, VLine):
                h = max(top_skip, it.height) if not started else h + prev_depth + it.height
                started, prev_depth = True, it.depth
                j += 1
                continue
            legal = isinstance(it, VPenalty) and it.penalty < INF_PENALTY or                 isinstance(it, VGlue) and isinstance(vlist[j - 1], VLine)
            if legal:
                pi = it.penalty if isinstance(it, VPenalty) else 0
                hh = h + max(0.0, prev_depth - max_depth)
                if hh <= text_height + 1e-6:
                    b = badness(_sp(text_height - hh), _sp(st))
                elif hh - text_height > sh:
                    b = AWFUL
                else:
                    b = badness(_sp(hh - text_height), _sp(sh))
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
            if isinstance(it, VGlue):
                h += it.width
                st += it.stretch
                sh += it.shrink
            j += 1
        if not forced:
            best = n
        elif best is None:
            best = max(j, i + 1)
        pages.append(_place(vlist[i:best], top_skip))
        i = best
    return pages


def _is_eject(vlist, i):
    return i < len(vlist) and isinstance(vlist[i], VPenalty) and vlist[i].penalty <= EJECT


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
