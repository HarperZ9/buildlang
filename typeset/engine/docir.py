"""Reader for buildlang.doc/v0, the IR a BuildLang document program saves."""
import os
from dataclasses import dataclass, field

HEADER = "#buildlang.doc/v0"
ROW_KINDS = ("row", "thead")      # table rows; cells are separated by CELL_SEP
CELL_SEP = "¦"               # broken bar, which no cell text needs


@dataclass
class Span:
    text: str
    style: str = ""        # "", "i", "b", "sc", "tt", "m", "bi"
    ref: str = ""          # ref or cite target
    start: int = 0         # char offset in the block's plain text


@dataclass
class Block:
    kind: str
    spans: list
    label: str = ""
    index: int = 0
    attrs: dict = field(default_factory=dict)   # block attributes (figure src and width, table columns)
    cells: list = field(default_factory=list)   # table rows: one span list per cell

    @property
    def plain(self):
        return "".join(s.text for s in self.spans)


@dataclass
class Mark:
    device: str
    layer: str
    block: int
    start: int
    end: int
    score: float
    err: float
    meter: str
    group: str


@dataclass
class Rel:
    group: str
    a: str
    b: str
    mapped: bool
    layer: str


@dataclass
class Document:
    meta: dict = field(default_factory=dict)
    theme: str = "plain"
    blocks: list = field(default_factory=list)
    marks: list = field(default_factory=list)
    rels: list = field(default_factory=list)
    base: str = ""             # folder of the IR file; figure sources resolve against it


class IRError(ValueError):
    pass


def parse_inline(text, outer=""):
    """Split {x:...} spans. Nested spans combine styles (b inside i -> bi)."""
    spans, buf, i = [], [], 0

    def flush():
        if buf:
            spans.append(Span("".join(buf), outer))
            buf.clear()

    while i < len(text):
        c = text[i]
        if text.startswith("{{", i) or text.startswith("}}", i):
            buf.append(c)
            i += 2
            continue
        if c == "{":
            colon = text.find(":", i)
            depth, j = 1, i + 1
            while j < len(text) and depth:
                if text.startswith("{{", j) or text.startswith("}}", j):
                    j += 2
                    continue
                depth += {"{": 1, "}": -1}.get(text[j], 0)
                j += 1
            if colon < 0 or depth:
                raise IRError(f"unbalanced inline span at {i}: {text[i:i + 30]!r}")
            tag, body = text[i + 1:colon], text[colon + 1:j - 1]
            flush()
            if tag in ("ref", "cite"):
                spans.append(Span("", tag, body))
            else:
                style = "".join(sorted(set(outer + tag))) if tag in ("i", "b") and outer in ("i", "b") else tag
                spans.extend(parse_inline(body, style) if tag != "m" else [Span(body, "m")])
            i = j
            continue
        buf.append(c)
        i += 1
    flush()
    return spans


def parse_attrs(text):
    """Block attributes, the fifth field: key=value pairs separated by ';'."""
    out = {}
    for part in text.split(";"):
        if part.strip():
            k, _, v = part.partition("=")
            out[k.strip()] = v.strip()
    return out


def load(path):
    with open(path, encoding="utf-8") as fh:
        lines = fh.read().split("\n")
    if not lines or lines[0].strip() != HEADER:
        raise IRError(f"{path}: not {HEADER}")
    doc = Document(base=os.path.dirname(os.path.abspath(path)))
    for n, line in enumerate(lines[1:], 2):
        if not line:
            continue
        f = line.split("\t")
        if f[0] == "meta":
            doc.meta[f[1]] = f[2] if len(f) > 2 else ""
        elif f[0] == "theme":
            doc.theme = f[1]
        elif f[0] == "block":
            text = f[2] if len(f) > 2 else ""
            label = f[3] if len(f) > 3 else ""
            attrs = parse_attrs(f[4]) if len(f) > 4 else {}
            cells = []
            try:
                if f[1] == "eq":
                    spans = [Span(text, "m")]
                elif f[1] in ROW_KINDS:
                    cells = [parse_inline(c.strip()) for c in text.split(CELL_SEP)]
                    spans = []
                    for k, c in enumerate(cells):
                        spans += ([Span(" ")] if k else []) + c
                else:
                    spans = parse_inline(text)
            except IRError as e:
                raise IRError(f"{path}:{n}: {e}") from e
            off = 0
            for sp in spans:
                sp.start = off
                off += len(sp.text)
            doc.blocks.append(Block(f[1], spans, label, len(doc.blocks), attrs, cells))
        elif f[0] == "mark":
            doc.marks.append(Mark(f[1], f[2], int(f[3]), int(f[4]), int(f[5]),
                                  float(f[6]), float(f[7]), f[8], f[9]))
        elif f[0] == "rel":
            doc.rels.append(Rel(f[1], f[2], f[3], f[4] == "mapped", f[5]))
        else:
            raise IRError(f"{path}:{n}: unknown record {f[0]!r}")
    return doc
