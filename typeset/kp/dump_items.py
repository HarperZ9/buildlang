"""Dump paragraph item lists for the BuildLang Knuth-Plass cross-check."""
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
from engine import docir  # noqa: E402
from engine.breaker import Box, Glue, Penalty  # noqa: E402
from engine.layout import Layout  # noqa: E402
from engine.theme import load_theme  # noqa: E402


def item_lists(doc_path, theme_name):
    doc = docir.load(doc_path)
    theme = load_theme(theme_name)
    lay = Layout(doc, theme)
    out = []
    for b in doc.blocks:
        if b.kind not in ("p", "p_first", "abstract"):
            continue
        st = lay.style(b.kind)
        items = lay.pb.build(b.spans, st)
        width = int(round((lay.text_width - st.get("left", 0) - st.get("right", 0)) * 65536))
        out.append((b.index, width, items))
    return out


def to_text(width, items, tolerance):
    lines = [f"w {width}", f"t {tolerance}"]
    for it in items:
        if isinstance(it, Box):
            lines.append(f"b {it.width}")
        elif isinstance(it, Glue) and it.stretch_order > 0:
            lines.append(f"f {it.width}")
        elif isinstance(it, Glue):
            lines.append(f"g {it.width} {it.stretch if it.stretch_order == 0 else 0} {it.shrink}")
        else:
            lines.append(f"p {it.width} {it.penalty} {1 if it.flagged else 0}")
    return "\n".join(lines) + "\n"
