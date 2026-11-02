"""Liang's hyphenation algorithm (Liang 1983, "Word Hy-phen-a-tion by Com-put-er").

Patterns load from a hyphen .dic file (data/hyph/hyph_en_US.dic, BSD-style
licence derived from plain TeX hyphen.tex and the TUGboat exception log;
see data/hyph/README_hyph_en_US.txt). Odd values allow a break.
"""
import os

HERE = os.path.dirname(os.path.abspath(__file__))
DEFAULT = os.path.join(HERE, "..", "data", "hyph", "hyph_en_US.dic")


class Hyphenator:
    def __init__(self, path=DEFAULT, left=2, right=3):
        self.patterns, self.maxlen = {}, 0
        self.left, self.right = left, right
        self.exceptions = {}
        with open(path, encoding="utf-8") as fh:
            lines = fh.read().split("\n")
        for line in lines[1:]:
            line = line.strip()
            if not line or line.startswith("%"):
                continue
            head = line.split(" ")
            if len(head) == 2 and head[0].isupper() and head[0].replace("HYPHENMIN", "").isalpha():
                if head[0] == "LEFTHYPHENMIN":
                    self.left = max(left, int(head[1]))
                elif head[0] == "RIGHTHYPHENMIN":
                    self.right = max(right, int(head[1]))
                continue
            self.add_pattern(line)

    def add_pattern(self, pat):
        letters, values = "", [0]
        for ch in pat:
            if ch.isdigit():
                values[-1] = int(ch)
            else:
                letters += ch
                values.append(0)
        self.patterns[letters] = values
        self.maxlen = max(self.maxlen, len(letters))

    def add_exception(self, word):
        self.exceptions[word.replace("-", "").lower()] = word.lower()

    def points(self, word):
        """Positions i (1..len-1) where a hyphen may go before word[i]."""
        w = word.lower()
        if w in self.exceptions:
            out, i = [], 0
            for ch in self.exceptions[w]:
                if ch == "-":
                    out.append(i)
                else:
                    i += 1
            return out
        if len(w) < self.left + self.right:
            return []
        s = "." + w + "."
        vals = [0] * (len(s) + 1)
        for i in range(len(s)):
            for j in range(i + 1, min(len(s), i + self.maxlen) + 1):
                p = self.patterns.get(s[i:j])
                if p:
                    for k, v in enumerate(p):
                        if v > vals[i + k]:
                            vals[i + k] = v
        # vals[k] sits between s[k-1] and s[k]; word index = k - 1
        out = []
        for k in range(2, len(s) - 1):
            pos = k - 1
            if vals[k] % 2 == 1 and self.left <= pos <= len(w) - self.right:
                out.append(pos)
        return out

    def hyphenate(self, word):
        pts, parts, prev = self.points(word), [], 0
        for p in pts:
            parts.append(word[prev:p])
            prev = p
        parts.append(word[prev:])
        return "-".join(parts)
