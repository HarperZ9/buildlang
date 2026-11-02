"""Math source parser: a TeX-like subset to atoms (classes per TeXbook ch. 17)."""
from dataclasses import dataclass

ORD, OP, BIN, REL, OPEN, CLOSE, PUNCT, INNER = range(8)
GREEK = {"alpha": 0x1D6FC, "beta": 0x1D6FD, "gamma": 0x1D6FE, "delta": 0x1D6FF, "epsilon": 0x1D700,
         "theta": 0x1D703, "lambda": 0x1D706, "mu": 0x1D707, "pi": 0x1D70B, "rho": 0x1D70C,
         "sigma": 0x1D70E, "tau": 0x1D70F, "phi": 0x1D711, "psi": 0x1D713, "omega": 0x1D714}
SYMBOLS = {"le": ("≤", REL), "ge": ("≥", REL), "ne": ("≠", REL), "to": ("→", REL),
           "in": ("∈", REL), "approx": ("≈", REL), "cdot": ("⋅", BIN), "times": ("×", BIN),
           "pm": ("±", BIN), "sum": ("∑", OP), "prod": ("∏", OP), "int": ("∫", OP),
           "infty": ("∞", ORD), "partial": ("∂", ORD), "log": ("log", OP), "ln": ("ln", OP)}
CLASS = {"+": BIN, "-": BIN, "*": BIN, "=": REL, "<": REL, ">": REL, "(": OPEN, "[": OPEN,
         ")": CLOSE, "]": CLOSE, ",": PUNCT, ";": PUNCT, "|": ORD, "/": ORD}



@dataclass
class Atom:
    cls: int
    nucleus: object            # str (characters) or list of Atoms or ("frac", a, b) or ("sqrt", a)
    sup: object = None
    sub: object = None


def parse(src):
    toks, i = [], 0
    while i < len(src):
        c = src[i]
        if c == "\\":
            j = i + 1
            while j < len(src) and src[j].isalpha():
                j += 1
            toks.append(src[i:j] if j > i + 1 else src[i:i + 2])
            i = max(j, i + 2) if j == i + 1 else j
        elif not c.isspace():
            toks.append(c)
            i += 1
        else:
            i += 1
    atoms, _ = _parse_list(toks, 0)
    return atoms


def _parse_list(toks, i, until=None):
    out = []
    while i < len(toks) and toks[i] != until:
        t = toks[i]
        i += 1
        if t in ("^", "_"):
            arg, i = _parse_arg(toks, i)
            if not out:
                out.append(Atom(ORD, ""))
            setattr(out[-1], "sup" if t == "^" else "sub", arg)
            continue
        if t == "{":
            inner, i = _parse_list(toks, i, "}")
            out.append(Atom(ORD, inner))
            i += 1
        elif t == "\\frac":
            a, i = _parse_arg(toks, i)
            b, i = _parse_arg(toks, i)
            out.append(Atom(INNER, ("frac", a, b)))
        elif t == "\\sqrt":
            a, i = _parse_arg(toks, i)
            out.append(Atom(ORD, ("sqrt", a)))
        elif t.startswith("\\") and t[1:] in GREEK:
            out.append(Atom(ORD, chr(GREEK[t[1:]])))
        elif t.startswith("\\") and t[1:] in SYMBOLS:
            ch, cls = SYMBOLS[t[1:]]
            out.append(Atom(cls, ch))
        elif t in ("\\,", "\\;", "\\ "):
            out.append(Atom(-1, {"\\,": 3, "\\;": 5, "\\ ": 6}[t]))
        elif t.isalpha() and len(t) == 1:
            cp = 0x210E if t == "h" else (0x1D44E + ord(t) - 97 if t.islower() else 0x1D434 + ord(t) - 65)
            out.append(Atom(ORD, chr(cp)))
        else:
            out.append(Atom(CLASS.get(t, ORD), "−" if t == "-" else t))
    return out, i


def _parse_arg(toks, i):
    if toks[i] == "{":
        inner, i = _parse_list(toks, i + 1, "}")
        return inner, i + 1
    atoms, _ = _parse_list([toks[i]], 0)
    return atoms, i + 1
