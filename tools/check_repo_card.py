"""check_repo_card.py -- gates for the drawing of a record.

The card draws a record this tool hands back, one field to a row. These guard
the drawing: text that fits the column it is drawn into, colour that still
says one thing, and values that carry the shape of a field rather than one
run's worth of digits. A picture with a hash in it is wrong by the next
commit, so a hash may not be drawn at all.

Whether the drawn fields are TRUE of the record is a different question, and
it is asked where the record lives rather than here.

Kept beside the art gates rather than inside them so neither file outgrows
what one person can hold at once.
"""
from __future__ import annotations

import json
import re
from pathlib import Path

import repo_card as CARD

ROOT = Path(__file__).resolve().parents[1]
ART = ROOT / "docs" / "art"

# A run of hex long enough to be a digest, and a number long enough to be a
# byte count. Either one in a value column dates the picture to one checkout.
DIGEST = re.compile(r"[0-9a-f]{12,}")
BIG_NUMBER = re.compile(r"\d{5,}")

# The two mono columns, in characters. A monospace advance is about 0.6em, so
# these count characters against the width each column actually has.
KEY_BUDGET = int((CARD.KEY_W + CARD.GUTTER - 16) / 7.8)
VAL_BUDGET = int(CARD.VAL_W / 7.2)

# The three column heads, in characters. They are set at 11px with a sixth of
# an em of tracking, so they run wider per character than the columns under
# them and get their own count.
HEAD_BUDGETS = (int((CARD.KEY_W + CARD.GUTTER - 16) / 8.4),
                int((CARD.VAL_W + CARD.GUTTER) / 8.4),
                int(CARD.NOTE_W / 8.4))


def _cards() -> list[dict]:
    return [card for path in sorted(ART.glob("*.art.json"))
            for card in json.loads(path.read_text(encoding="utf-8"))
            .get("cards", [])]


def values_that_are_not_shapes(cards: list[dict]) -> list[str]:
    """A value column holds the shape of a field, never a run of its digits."""
    bad = []
    for card in cards:
        for field in card["fields"]:
            value = field["value"]
            if DIGEST.search(value):
                bad.append(f'{card["file"]}: {field["key"]} draws a digest, '
                           f"and a digest is true for one checkout: {value!r}")
            if BIG_NUMBER.search(value):
                bad.append(f'{card["file"]}: {field["key"]} draws a number '
                           f"that moves with the commit: {value!r}")
    return bad


def text_that_overflows(cards: list[dict]) -> list[str]:
    """Nothing is drawn wider than the column it is drawn into. The key and
    the value are single unwrapped lines, so they run into their neighbour
    rather than being clipped; the note and the footnote wrap by measured
    width and then drop what will not fit instead of growing the drawing."""
    bad = []
    for card in cards:
        for field in card["fields"]:
            if len(field["key"]) > KEY_BUDGET:
                bad.append(f'{card["file"]}: the {field["key"]} name runs '
                           f"into the value column")
            if len(field["value"]) > VAL_BUDGET:
                bad.append(f'{card["file"]}: the value on {field["key"]} runs '
                           f"into the note column")
            drawn = " ".join(CARD._wrap(field["note"]))
            if drawn != " ".join(field["note"].split()):
                bad.append(f'{card["file"]}: the note on {field["key"]} cuts '
                           f'off at "{drawn}"')
        heads = card.get("heads", CARD.HEADS)
        if len(heads) != 3:
            bad.append(f'{card["file"]} names {len(heads)} columns, and the '
                       f"drawing has three")
        for head, budget in zip(heads, HEAD_BUDGETS):
            if len(head) > budget:
                bad.append(f'{card["file"]}: the {head!r} column head runs '
                           f"into the column beside it")
        foot = " ".join(CARD._wrap(card["footnote"], CARD.FOOT_BUDGET,
                                   CARD.FOOT_LINES))
        if foot != " ".join(card["footnote"].split()):
            bad.append(f'{card["file"]}: the footnote cuts off at "{foot}"')
    return bad


def alt_text_that_drifted(cards: list[dict]) -> list[str]:
    """The README alt attribute is the whole of what a reader who cannot see
    the card gets. GitHub draws it as an <img>, and an <img> hides whatever
    description the SVG carries inside it, so the long one in the spec has to
    reach the README as it is written. Without this, a row can be re-worded
    and the sentence describing it to a screen reader still says what the card
    used to say."""
    shown = (ROOT / "README.md").read_text(encoding="utf-8")
    return [f'{card["file"]}: the README describes it as something it is no '
            f"longer, because the spec alt is not the alt in the README"
            for card in cards if card["alt"] not in shown]


def wrong_number_of_marks(cards: list[dict]) -> list[str]:
    """Colour says one thing here. Two accents and it says nothing."""
    bad = []
    for card in cards:
        hot = [f["key"] for f in card["fields"]
               if f.get("tone", "none") != "none"]
        if len(hot) != 1:
            bad.append(f'{card["file"]} accents {len(hot)} rows, and one hot '
                       f"mark per view is the whole of the colour rule")
    return bad


def checks() -> list[tuple]:
    """The card gates, in the order the receipt reports them."""
    return [
        ("art.card_draws_shapes_not_digits",
         lambda _unused: values_that_are_not_shapes(_cards())),
        ("art.card_text_fits_its_column",
         lambda _unused: text_that_overflows(_cards())),
        ("art.card_carries_one_mark",
         lambda _unused: wrong_number_of_marks(_cards())),
        ("art.card_alt_reaches_the_readme",
         lambda _unused: alt_text_that_drifted(_cards())),
    ]


# A card built to break every one of those at once: a digest and a byte count
# in the value column, a name and a value too wide for their columns, a
# clipped note, a clipped footnote, and two hot marks where the rule allows
# one.
#
# The fourth row is the shape that got past an earlier version of this file. A
# budget counted in characters read that note as two comfortable lines and let
# it through, and it drew forty pixels past the edge of the page, because
# capitals are wider than the lowercase prose the count was calibrated on. It
# stays here so a return to counting characters fails rather than ships.
CONTROL = [{
    "file": "control.svg",
    "alt": "a description of a drawing that is in no README anywhere",
    "footnote": "word " * 200,
    "heads": ["z" * (HEAD_BUDGETS[0] + 1), "ok", "ok", "one column too many"],
    "fields": [
        {"key": "head", "value": "9f2c4ab71de0", "note": "ok",
         "tone": "verified"},
        {"key": "bytes", "value": "104857 bytes", "note": "ok",
         "tone": "drift"},
        {"key": "z" * (KEY_BUDGET + 1), "value": "z" * (VAL_BUDGET + 1),
         "note": "word " * 40},
        {"key": "caps", "value": "ok", "note": " ".join(["UNVERIFIABLE"] * 10)},
    ],
}]


def control_failures() -> list[str]:
    """Feed each card gate input it has to reject, and say what got past."""
    return [f"the gate missed {what}" for caught, what in (
        (len(values_that_are_not_shapes(CONTROL)) == 2,
         "a digest and a byte count drawn as values"),
        (len(text_that_overflows(CONTROL)) == 7,
         "an over-wide name, an over-wide value, a clipped note, a row of "
         "capitals that fits a character count and not the column, a fourth "
         "column, an over-wide column head and a clipped footnote"),
        (len(wrong_number_of_marks(CONTROL)) == 1,
         "a card wearing two hot marks"),
        (len(alt_text_that_drifted(CONTROL)) == 1,
         "a description that reaches no README at all"),
    ) if not caught]
