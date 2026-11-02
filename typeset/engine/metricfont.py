"""A font known by metrics plus a CFF program (CID keyed, Identity glyph ids).

Used for the Latin Modern comparison: the metrics come from
data/lm/*.json, the program from data/lm/*.cff.
Same query surface as sfnt.Font, so the layout engine does not care.
"""
import json
import os

LIGS = {"ﬀ": "ff", "ﬁ": "fi", "ﬂ": "fl", "ﬃ": "ffi", "ﬄ": "ffl"}


class MetricFont:
    kind = "cff"

    def __init__(self, json_path):
        with open(json_path, encoding="utf-8") as fh:
            meta = json.load(fh)
        self.name = meta["name"]
        self.upem = 1000
        self.widths = {int(k): v for k, v in meta["widths"].items()}
        self.cmap, self.ligatures, self.text_ligs = {}, {}, {}
        for cid, text in meta["unicode"].items():
            cid = int(cid)
            text = LIGS.get(text, text)
            if len(text) == 1:
                self.cmap.setdefault(ord(text), cid)
        for cid, text in meta["unicode"].items():
            text = LIGS.get(text, text)
            if len(text) > 1:
                self.text_ligs[text] = int(cid)
            if len(text) > 1 and all(ord(c) in self.cmap for c in text):
                self.ligatures[tuple(self.cmap[ord(c)] for c in text)] = int(cid)
        self.kern_pairs = {}
        for key, v in meta["kerns"].items():
            a, b = key.split(",")
            self.kern_pairs[(int(a), int(b))] = v
        self.ascent = meta.get("ascent") or 800
        self.descent = meta.get("descent") or -200
        self.bbox = tuple(meta.get("bbox") or (0, -250, 1000, 900))
        self.cap_height, self.x_height = 683, 431
        self.italic_angle = -15.0 if "Italic" in self.name else 0.0
        self.fs_type = 0
        cff = os.path.splitext(json_path)[0] + ".cff"
        self.program = None
        if os.path.exists(cff):
            with open(cff, "rb") as fh:
                self.program = fh.read()
        self.num_glyphs = max(self.widths) + 1 if self.widths else 1

    def glyph(self, ch):
        return self.cmap.get(ord(ch), 0)

    def advance(self, gid):
        return self.widths.get(gid, 0)

    def kern(self, a, b):
        return self.kern_pairs.get((a, b), 0)

    def outline(self, gid):
        return []
