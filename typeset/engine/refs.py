"""Numbering and cross references: sections, enumerations, bibliography, cites."""


def resolve_refs(doc, theme):
    numbered = set(theme.get("numbered", ["section"]))
    counters, labels, bib = {}, {}, {}
    prev_kind = None
    for b in doc.blocks:
        if b.kind in numbered:
            counters[b.kind] = counters.get(b.kind, 0) + 1
            b.number = counters[b.kind]
        elif b.kind == "item":
            counters["item"] = counters.get("item", 0) + 1 if prev_kind == "item" else 1
            b.number = counters["item"]
        elif b.kind == "bib":
            counters["bib"] = counters.get("bib", 0) + 1
            b.number = counters["bib"]
            bib[b.label] = b.number
        if b.label and hasattr(b, "number"):
            labels[b.label] = b.number
        prev_kind = b.kind
    cite_fmt = theme.get("cite_format", "[{n}]")
    for b in doc.blocks:
        for s in b.spans:
            if s.style == "ref":
                s.text = str(labels.get(s.ref, "??"))
                s.style = ""
            elif s.style == "cite":
                nums = [str(bib.get(k.strip(), "?")) for k in s.ref.split(",")]
                s.text = cite_fmt.replace("{n}", ",".join(nums))
                s.style = ""
    return labels
