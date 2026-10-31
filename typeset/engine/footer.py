"""Page furniture: a footer line and the page number, set as ordinary lines."""
from .pages import VLine


def footer_lines(layout, page, spec, total):
    out = []
    text = layout.doc.meta.get("footer", "")
    y = spec["baseline"]
    if text:
        x, runs = 0.0, []
        space = layout.theme.get("space", {}).get("width", 0.333) * spec["size"]
        for word in text.split():
            for pc in layout.shaper.shape(spec["text_font"], spec["size"], word, spec.get("color", ""), ("rm",)):
                runs.append((x, 0.0, pc, 1.0))
                x += pc.width
            x += space
        out.append({"type": "line", "line": VLine(spec["size"], 0, runs, block=-2), "y": y, "page": page})
    num = layout.shaper.shape(spec["page_font"], spec["page_size"], str(page.number))
    w = sum(p.width for p in num)
    runs, x = [], layout.text_width - w
    for pc in num:
        runs.append((x, 0.0, pc, 1.0))
        x += pc.width
    out.append({"type": "line", "line": VLine(spec["page_size"], 0, runs, block=-3), "y": y, "page": page})
    return out
