"""Floats in the layout: which blocks make one, and where each may first be set.

A figure (its picture and caption) is one float. A table is one float from its caption (`tablecap`) or first
row through the row marked `last=1`. The page builder (engine.pages) sets a float where it stands when it fits
and otherwise at the top of a later page. A float is never set before its first reference: one that comes
before the paragraph that first cites it moves to just after that paragraph, and floats keep their order.
"""
from .pages import VFloat, VLine


def opens(block, st, open_kind):
    """The kind of float this block starts ("figure" or "table"), or None."""
    if open_kind:
        return None
    if st.get("figure"):
        return "figure"
    if block.kind == "tablecap" or block.cells:
        return "table"
    return None


def closes(block, kind):
    """Whether the float of this kind ends with this block."""
    return kind == "figure" or bool(block.cells and block.attrs.get("last"))


def wrap(vlist, start, blocks, max_height=None):
    """Replace vlist[start:] with one VFloat. A float taller than max_height stays in the text, unwrapped."""
    items = vlist[start:]
    if max_height is not None and _height(items) > max_height:
        return None
    del vlist[start:]
    f = VFloat(items, list(blocks))
    vlist.append(f)
    return f


def _height(items):
    h, depth, started = 0.0, 0.0, False
    for it in items:
        if isinstance(it, VLine):
            h = it.height if not started else h + depth + it.height
            started, depth = True, it.depth
        elif started and hasattr(it, "stretch"):
            h += it.width
    return h + depth


def first_refs(doc):
    """{label: index of the first block that refers to it}, read before references are resolved."""
    out = {}
    for b in doc.blocks:
        for s in b.spans:
            if s.style == "ref" and s.ref not in out:
                out[s.ref] = b.index
    return out


def after_first_reference(vlist, floats, labels, refs, block_end):
    """Move each float that comes before its first reference to just after the referencing block's lines.

    labels: {float position in floats: label}; block_end: {block index: vlist index of its last item}.
    A later float never moves ahead of an earlier one, so numbering and order hold."""
    where = {id(f): k for k, f in enumerate(vlist) if isinstance(f, VFloat)}
    target, floor = {}, -1
    for f_n, f in enumerate(floats):
        pos = where[id(f)]
        ref = refs.get(labels.get(f_n, ""))
        want = block_end.get(ref, pos) if ref is not None and ref > f.blocks[-1] else pos
        floor = max(floor, want)
        if floor > pos:
            target[id(f)] = floor
    if not target:
        return vlist
    out, held = [], {}
    for k, it in enumerate(vlist):
        if id(it) in target:
            held.setdefault(target[id(it)], []).append(it)
            continue
        out.append(it)
        out += held.pop(k, [])
    for k in sorted(held):
        out += held[k]
    return out
