"""Knuth-Plass optimal line breaking, following TeX's line_break (tex.web §813-890).

Units are scaled points (sp, 65536 per pt), integers, so badness and
demerits match TeX's integer arithmetic. Boxes may carry expansion
capacity (pdfTeX font expansion) and margin protrusion amounts.
"""
from dataclasses import dataclass, field

INF_BAD = 10000
INF_PENALTY = 10000
EJECT = -10000
SP = 65536


@dataclass
class Box:
    width: int
    stretch: int = 0      # font-expansion capacity
    shrink: int = 0
    pro_left: int = 0     # margin protrusion if first on a line
    pro_right: int = 0    # margin protrusion if last on a line
    payload: object = None


@dataclass
class Glue:
    width: int
    stretch: int = 0
    shrink: int = 0
    stretch_order: int = 0  # 0 finite, 1 fil, 2 fill
    payload: object = None


@dataclass
class Penalty:
    width: int
    penalty: int
    flagged: bool = False
    pro_right: int = 0
    payload: object = None


@dataclass
class Params:
    tolerance: int = 200
    pretolerance: int = 100
    line_penalty: int = 10
    adj_demerits: int = 10000
    double_hyphen_demerits: int = 10000
    final_hyphen_demerits: int = 5000
    emergency_stretch: int = 0
    rightskip_stretch: int = 0   # ragged right: finite stretch on every line (ragged2e RaggedRight)
    looseness: int = 0


def badness(t, s):
    """TeX's integer badness approximation 100 (t/s)^3 (tex.web §108)."""
    if t == 0:
        return 0
    if s <= 0:
        return INF_BAD
    if t <= 7230584:
        r = (t * 297) // s
    elif s >= 1663497:
        r = t // (s // 297)
    else:
        r = t
    if r > 1290:
        return INF_BAD
    return (r * r * r + 0o400000) // 0o1000000


@dataclass
class Active:
    pos: int
    line: int
    fitness: int
    demerits: int
    flagged: bool
    prev: object
    artificial: bool = False


@dataclass
class Result:
    breaks: list
    ratios: list
    badness: list
    demerits: int
    passes: int
    overfull: list = field(default_factory=list)
    fitness: list = field(default_factory=list)


def _sums(items):
    n = len(items)
    w, st, sh = [0] * (n + 1), [[0] * (n + 1) for _ in range(3)], [0] * (n + 1)
    for i, it in enumerate(items):
        w[i + 1], sh[i + 1] = w[i], sh[i]
        for o in range(3):
            st[o][i + 1] = st[o][i]
        if isinstance(it, Box):
            w[i + 1] += it.width
            st[0][i + 1] += it.stretch
            sh[i + 1] += it.shrink
        elif isinstance(it, Glue):
            w[i + 1] += it.width
            st[min(it.stretch_order, 2)][i + 1] += it.stretch
            sh[i + 1] += it.shrink
    return w, st, sh


def _after_break(items, pos):
    """First index after a break at `pos`, discarding glue and penalties."""
    j = pos + 1 if pos >= 0 else 0
    while j < len(items) - 1 and not isinstance(items[j], Box):
        j += 1
    return j


def _feasible(items, line_width, params, threshold, background, final):
    w, st, sh = _sums(items)
    root = Active(-1, 0, 2, 0, False, None)
    active = [root]
    for b, it in enumerate(items):
        if isinstance(it, Penalty):
            if it.penalty >= INF_PENALTY:
                continue
            pen, flagged, extra = it.penalty, it.flagged, it.width
        elif isinstance(it, Glue) and b > 0 and isinstance(items[b - 1], Box):
            pen, flagged, extra = 0, False, 0
        else:
            continue
        cands = [None] * 4
        keep = []
        for ai, a in enumerate(active):
            start = _after_break(items, a.pos)
            width = w[b] - w[start] + extra
            first = items[start] if start < len(items) else None
            if isinstance(first, Box):
                width -= first.pro_left
            last = it if isinstance(it, Penalty) and it.width else (items[b - 1] if b > 0 else None)
            if isinstance(last, (Box, Penalty)):
                width -= last.pro_right
            target = line_width(a.line + 1)
            fil = st[1][b] - st[1][start] + st[2][b] - st[2][start]
            if width < target:
                stretch = st[0][b] - st[0][start] + background + params.rightskip_stretch
                if fil > 0:
                    bad, fit = 0, 2
                else:
                    bad = badness(target - width, stretch)
                    fit = 0 if bad > 99 else 1 if bad > 12 else 2
            else:
                shrink = sh[b] - sh[start]
                if width - target > shrink:
                    bad = INF_BAD + 1
                else:
                    bad = badness(width - target, shrink)
                fit = 3 if bad > 12 else 2
            deactivate = bad > INF_BAD or pen == EJECT
            artificial = False
            if bad > threshold:
                if final and deactivate and not keep and ai == len(active) - 1 \
                        and all(c is None for c in cands):
                    artificial = True
                else:
                    if not deactivate:
                        keep.append(a)
                    continue
            d = 0 if artificial else _demerits(params, bad, pen, flagged, a, fit, it)
            d += a.demerits
            c = cands[fit]
            if c is None or d < c[0]:
                cands[fit] = (d, a, bad if bad <= INF_BAD else INF_BAD, artificial)
            if not deactivate:
                keep.append(a)
        active = keep
        best = min((c[0] for c in cands if c), default=None)
        for fit, c in enumerate(cands):
            if c is None or c[0] > best + params.adj_demerits:
                continue
            node = Active(b, c[1].line + 1, fit, c[0], flagged, c[1], c[3])
            node.bad = c[2]
            active.append(node)
        if not active:
            return None
    return active


def _demerits(params, bad, pen, flagged, a, fit, item):
    d = params.line_penalty + bad
    d = d * d
    if pen > 0:
        d += pen * pen
    elif pen > EJECT:
        d -= pen * pen
    if flagged and a.flagged:
        d += params.double_hyphen_demerits
    if abs(fit - a.fitness) > 1:
        d += params.adj_demerits
    if pen == EJECT and a.flagged:
        d += params.final_hyphen_demerits
    return d


def break_lines(items, line_width, params=None, has_hyphens=True):
    """Return a Result. `line_width(n)` gives the width of line n (1-based)."""
    params = params or Params()
    if isinstance(line_width, int):
        lw = line_width
        line_width = lambda n: lw
    plan = []
    if params.pretolerance >= 0:
        plan.append((params.pretolerance, 0, False, "pre"))
    plan.append((params.tolerance, 0, params.emergency_stretch <= 0, "second"))
    if params.emergency_stretch > 0:
        plan.append((params.tolerance, params.emergency_stretch, True, "emergency"))
    for n, (thr, bg, final, name) in enumerate(plan, 1):
        if name == "pre" and has_hyphens:
            work = [it for it in items if not (isinstance(it, Penalty) and it.flagged and it.payload == "hyph")]
            idx = [i for i, it in enumerate(items) if not (isinstance(it, Penalty) and it.flagged and it.payload == "hyph")]
        else:
            work, idx = items, list(range(len(items)))
        active = _feasible(work, line_width, params, thr, bg, final)
        ends = [a for a in (active or []) if a.pos == len(work) - 1]
        if not ends:
            continue
        best = min(ends, key=lambda a: a.demerits)
        return _result(work, idx, best, line_width, n)
    raise RuntimeError("no feasible breaks (paragraph must end with a forced break)")


def _result(work, idx, node, line_width, passes):
    chain = []
    while node.prev is not None:
        chain.append(node)
        node = node.prev
    chain.reverse()
    w, st, sh = _sums(work)
    breaks, ratios, bads, over, fits = [], [], [], [], []
    prev = -1
    for k, a in enumerate(chain, 1):
        start = _after_break(work, prev)
        it = work[a.pos]
        width = w[a.pos] - w[start] + (it.width if isinstance(it, Penalty) else 0)
        first = work[start] if start < len(work) else None
        if isinstance(first, Box):
            width -= first.pro_left
        last = it if isinstance(it, Penalty) and it.width else work[a.pos - 1]
        if isinstance(last, (Box, Penalty)):
            width -= last.pro_right
        target = line_width(k)
        fil = st[1][a.pos] - st[1][start] + st[2][a.pos] - st[2][start]
        if width < target:
            s = st[0][a.pos] - st[0][start]
            r = 0.0 if fil > 0 else ((target - width) / s if s > 0 else 0.0)
        elif width > target:
            s = sh[a.pos] - sh[start]
            r = -(width - target) / s if s > 0 else -1e9
            if width - target > s:
                over.append((k, (width - target - s) / SP))
                r = -1.0
        else:
            r = 0.0
        breaks.append(idx[a.pos])
        ratios.append(r)
        bads.append(getattr(a, "bad", 0))
        fits.append(a.fitness)
        prev = a.pos
    return Result(breaks, ratios, bads, chain[-1].demerits, passes, over, fits)
