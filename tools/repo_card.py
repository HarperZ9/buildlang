"""repo_card.py -- the artifact a tool hands back, drawn field by field.

Every command here writes a receipt, and until now the README said so in a
sentence. A reader deciding whether to trust the tool wants to see the thing:
what fields come back, which one carries the verdict, and how they would check
each field themselves. This draws that from a spec, so the picture is data in
the repository and a gate can hold it against a receipt the tool actually
emits.

Color still says one thing. Exactly one row carries the verdict and takes the
verified green; a row that reports drift takes the drift iris. Every other row
is ink and a hairline, because a field is structure and structure is not news.

The value column shows a literal only where the literal is stable. A hash or a
byte count changes with the checkout, so those rows carry the shape of the
value instead: how many entries, how many keys. A picture that shows a hash is
a picture that is wrong by the next commit.
"""
from __future__ import annotations

from repo_art import GROTESK, MONO, _esc, _num

W = 960
PAD = 44
ROW_H = 46
TOP = 142
KEY_W = 186
VAL_W = 258
GUTTER = 26
NOTE_X = PAD + KEY_W + GUTTER + VAL_W + GUTTER
NOTE_W = W - PAD - NOTE_X

# The same two palettes the schematics use, so the whole set reads as one hand.
STYLE = """
  :root{ --void:#f4f3ef; --bone:#0b0c0e; --muted:#43474e;
    --hairline:rgba(11,12,14,.16); --card:rgba(255,255,255,.66);
    --verified:#1f7a52; --drift:#3a2bd6; }
  @media (prefers-color-scheme: dark){
    :root{ --void:#0b0e0f; --bone:#eef1ee; --muted:#9aa39c;
      --hairline:rgba(238,241,238,.18); --card:rgba(255,255,255,.05);
      --verified:#5fae93; --drift:#a99cf5; } }
  .bg{ fill:var(--void); }
  .row{ fill:var(--card); stroke:var(--hairline); stroke-width:1.2; }
  .key{ fill:var(--bone); font-size:13px; font-weight:650; }
  .val{ fill:var(--muted); font-size:12px; }
  .s{ fill:var(--muted); font-size:11.5px; }
  .k{ fill:var(--muted); font-size:11px; letter-spacing:.16em; }
  .h{ fill:var(--bone); font-size:21px; font-weight:700; }
  .thin{ stroke:var(--hairline); stroke-width:1.2; fill:none; }
"""

TONE = {"verified": "var(--verified)", "drift": "var(--drift)",
        "none": "var(--hairline)"}

# The column heads say what a row of this drawing is. A receipt reads as a
# field and what comes back in it; something else in the repository reads as
# something else, so a spec may name its own three and these are the default.
HEADS = ("field", "what comes back", "how you check it")

# What one character draws in the note and footnote columns, in pixels at
# 11.5px. These are measured off a rendered probe rather than assumed, because
# a budget counted in characters cannot tell an uppercase line from a
# lowercase one: capitals run about a quarter wider, so a row of verdict
# tokens fits a character count and still draws off the edge of the page.
#
# The weights round up. The drawing ships to readers whose machine resolves a
# different face than the one measured, so a line that stops a little short is
# a smaller defect than one that runs past the rule.
UPPER, LOWER, DIGIT, SPACE, NARROW = 7.1, 5.75, 6.3, 3.2, 2.7
_NARROW = frozenset(".,;:'!|")


def _advance(char: str) -> float:
    if char == " ":
        return SPACE
    if char.isupper():
        return UPPER
    if char.islower():
        return LOWER
    if char.isdigit():
        return DIGIT
    return NARROW if char in _NARROW else LOWER


def text_width(text: str) -> float:
    """What a line of note or footnote prose draws, in pixels."""
    return sum(_advance(char) for char in text)


# One line of the note column, in pixels. Two lines fit the row.
NOTE_BUDGET = NOTE_W
NOTE_LINES = 2

# The footnote runs the width of the page at the same size, so it holds more.
FOOT_BUDGET = W - PAD * 2
FOOT_LINES = 3


def _wrap(text: str, width: float = NOTE_BUDGET,
          limit: int = NOTE_LINES) -> list[str]:
    """Greedy wrap by drawn width, cut to the lines the caller has room for."""
    lines: list[str] = []
    line = ""
    for word in text.split():
        candidate = f"{line} {word}".strip()
        if text_width(candidate) > width and line:
            lines.append(line)
            line = word
        else:
            line = candidate
    if line:
        lines.append(line)
    return lines[:limit]


def _row_y(index: int) -> float:
    return TOP + index * ROW_H


def _row(index: int, field: dict) -> str:
    """One field: its name, what comes back in it, and how to check it."""
    y = _row_y(index)
    tone = TONE[field.get("tone", "none")]
    accent = field.get("tone", "none") != "none"
    notes = "".join(
        f'<text class="s" x="{_num(NOTE_X)}" y="{_num(y + 20 + i * 15)}">'
        f"{_esc(line)}</text>"
        for i, line in enumerate(_wrap(field["note"])))
    rule = (f'<rect x="{_num(PAD)}" y="{_num(y)}" width="3" '
            f'height="{ROW_H - 8}" fill="{tone}"/>') if accent else ""
    return (f'<g><rect class="row" x="{_num(PAD)}" y="{_num(y)}" '
            f'width="{W - PAD * 2}" height="{ROW_H - 8}" rx="3"/>{rule}'
            f'<text class="key" x="{_num(PAD + 16)}" y="{_num(y + 24)}" '
            f'font-family="{MONO}">{_esc(field["key"])}</text>'
            f'<text class="val" x="{_num(PAD + KEY_W + GUTTER)}" '
            f'y="{_num(y + 24)}" font-family="{MONO}"'
            f'{f" style={chr(34)}fill:{tone}{chr(34)}" if accent else ""}>'
            f'{_esc(field["value"])}</text>{notes}</g>')


def _column_heads(labels: tuple[str, str, str] = HEADS) -> str:
    columns = (PAD + 16, PAD + KEY_W + GUTTER, NOTE_X)
    return "".join(
        f'<text class="k" x="{_num(x)}" y="{_num(TOP - 14)}" '
        f'font-family="{MONO}">{_esc(label.upper())}</text>'
        for label, x in zip(labels, columns))


def _footnote(text: str, top: float) -> str:
    return "".join(
        f'<text class="s" x="{PAD}" y="{_num(top + i * 16)}">{_esc(line)}</text>'
        for i, line in enumerate(_wrap(text, FOOT_BUDGET, FOOT_LINES)))


def card_svg(spec: dict) -> str:
    """A receipt drawn field by field, with the source that produced it."""
    fields = spec["fields"]
    foot = _wrap(spec["footnote"], FOOT_BUDGET, FOOT_LINES)
    rule = _row_y(len(fields)) + 12
    height = rule + 22 + len(foot) * 16
    rows = "".join(_row(i, f) for i, f in enumerate(fields))
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {W} {_num(height)}" '
        f'width="{W}" height="{_num(height)}" font-family="{GROTESK}" role="img" '
        f'aria-label="{_esc(spec["alt"])}">'
        f"<style>{STYLE}</style>"
        f'<rect class="bg" width="{W}" height="{_num(height)}"/>'
        f'<text class="k" x="{PAD}" y="40" font-family="{MONO}">'
        f'{_esc(spec["kicker"].upper())}</text>'
        f'<text class="h" x="{PAD}" y="72">{_esc(spec["title"])}</text>'
        f'<text class="s" x="{PAD}" y="94" font-family="{MONO}" '
        f'font-size="11.5">$ {_esc(spec["source"])}</text>'
        f'{_column_heads(tuple(spec.get("heads", HEADS)))}{rows}'
        f'<path class="thin" d="M{PAD} {_num(rule)}H{W - PAD}"/>'
        f'{_footnote(spec["footnote"], rule + 22)}'
        "</svg>")
