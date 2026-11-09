"""Math source parser: a TeX-like subset to atoms (classes per TeXbook ch. 17).

A command outside the subset is an error: parse() records it in the caller's error list and sets nothing for
it, so raw source never reaches the page.
"""
import unicodedata
from dataclasses import dataclass

ORD, OP, BIN, REL, OPEN, CLOSE, PUNCT, INNER = range(8)
_LOWER = ["alpha", "beta", "gamma", "delta", "varepsilon", "zeta", "eta", "theta", "iota", "kappa", "lambda",
          "mu", "nu", "xi", "omicron", "pi", "rho", "varsigma", "sigma", "tau", "upsilon", "varphi", "chi", "psi",
          "omega"]
GREEK = {name: 0x1D6FC + k for k, name in enumerate(_LOWER)}       # mathematical italic small alpha onwards
GREEK.update({"epsilon": 0x1D700, "phi": 0x1D711, "vartheta": 0x1D717, "varkappa": 0x1D718,
              "varrho": 0x1D71A, "varpi": 0x1D71B, "ell": 0x2113, "imath": 0x1D6A4, "jmath": 0x1D6A5})
# capital Greek is upright in TeX (plain.tex \mathchardef \Gamma="0100 and on)
CAPITAL = {"Gamma": "Γ", "Delta": "Δ", "Theta": "Θ", "Lambda": "Λ", "Xi": "Ξ", "Pi": "Π", "Sigma": "Σ",
           "Upsilon": "Υ", "Phi": "Φ", "Psi": "Ψ", "Omega": "Ω"}
SYMBOLS = {"le": ("≤", REL), "leq": ("≤", REL), "ge": ("≥", REL), "geq": ("≥", REL), "ne": ("≠", REL),
           "neq": ("≠", REL), "to": ("→", REL), "rightarrow": ("→", REL), "leftarrow": ("←", REL),
           "Rightarrow": ("⇒", REL), "Leftrightarrow": ("⇔", REL), "mapsto": ("↦", REL), "in": ("∈", REL),
           "notin": ("∉", REL), "subset": ("⊂", REL), "subseteq": ("⊆", REL), "approx": ("≈", REL),
           "sim": ("∼", REL), "simeq": ("≃", REL), "equiv": ("≡", REL), "propto": ("∝", REL), "ll": ("≪", REL),
           "gg": ("≫", REL), "mid": ("∣", REL), "cdot": ("⋅", BIN), "times": ("×", BIN), "div": ("÷", BIN),
           "pm": ("±", BIN), "mp": ("∓", BIN), "ast": ("∗", BIN), "circ": ("∘", BIN), "cup": ("∪", BIN),
           "cap": ("∩", BIN), "setminus": ("∖", BIN), "sum": ("∑", OP), "prod": ("∏", OP), "int": ("∫", OP),
           "oint": ("∮", OP), "infty": ("∞", ORD), "partial": ("∂", ORD), "nabla": ("∇", ORD), "hbar": ("ℏ", ORD),
           "forall": ("∀", ORD), "exists": ("∃", ORD), "emptyset": ("∅", ORD), "prime": ("′", ORD),
           "ldots": ("…", INNER), "dots": ("…", INNER), "cdots": ("⋯", INNER), "langle": ("⟨", OPEN),
           "rangle": ("⟩", CLOSE), "lvert": ("|", OPEN), "rvert": ("|", CLOSE), "|": ("‖", ORD),
           "{": ("{", OPEN), "}": ("}", CLOSE), "%": ("%", ORD), "_": ("_", ORD), "#": ("#", ORD)}
# named operators, upright with operator spacing (plain.tex \log, \cos and the rest); LIMITS take limits in display
OPERATORS = ["log", "ln", "lg", "exp", "sin", "cos", "tan", "cot", "sec", "csc", "sinh", "cosh", "tanh", "coth",
             "arcsin", "arccos", "arctan", "arg", "deg", "dim", "hom", "ker", "max", "min", "sup", "inf", "lim",
             "liminf", "limsup", "det", "gcd", "Pr"]
LIMITS = {"max", "min", "sup", "inf", "lim", "liminf", "limsup", "det", "gcd", "Pr"}
SYMBOLS.update({name: (name, OP) for name in OPERATORS})
SPACES = {",": 3, ":": 4, ">": 4, ";": 5, " ": 6, "!": -3, "quad": 18, "qquad": 36}
# "bar" is a rule over the base; the rest are combining marks centred over it
ACCENTS = {"bar": "bar", "overline": "bar", "hat": "̂", "widehat": "̂", "tilde": "̃",
           "widetilde": "̃", "dot": "̇", "ddot": "̈", "vec": "⃗"}
TEXT = {"text", "textrm", "mbox", "mathrm", "operatorname"}   # upright letters; \text keeps its spaces
BOLD = {"mathbf", "boldsymbol", "bm"}
DELIMS = {"left", "right", "big", "Big", "bigl", "bigr", "Bigl", "Bigr"}
CLASS = {"+": BIN, "-": BIN, "*": BIN, "=": REL, "<": REL, ">": REL, "(": OPEN, "[": OPEN,
         ")": CLOSE, "]": CLOSE, ",": PUNCT, ";": PUNCT, "|": ORD, "/": ORD}


@dataclass
class Atom:
    cls: int
    nucleus: object  # str, list of Atoms, ("frac", a, b), ("sqrt", a), ("accent", mark, a) or ("text", s)
    sup: object = None
    sub: object = None
    sup_first: bool = False    # the source wrote ^ before _, so the text layer gives the superscript first


def parse(src, errors=None):
    """Atoms for src. Unknown commands and malformed groups are appended to errors (a list) when given."""
    errors = [] if errors is None else errors
    atoms, _ = _parse_list(_tokens(src, errors), 0, errors)
    return atoms


def _tokens(src, errors):
    toks, i = [], 0
    while i < len(src):
        c = src[i]
        if c == "\\":
            j = i + 1
            while j < len(src) and src[j].isalpha():
                j += 1
            name = src[i + 1:j] if j > i + 1 else src[i + 1:i + 2]
            i = j if j > i + 1 else i + 2
            if name in TEXT:
                raw, i = _raw_group(src, i, name, errors)
                toks.append(("text", raw if name in ("text", "textrm", "mbox") else raw.replace(" ", "")))
            else:
                toks.append("\\" + name)
        elif not c.isspace():
            toks.append(c)
            i += 1
        else:
            i += 1
    return toks


def _raw_group(src, i, name, errors):
    while i < len(src) and src[i].isspace():
        i += 1
    if i >= len(src) or src[i] != "{":
        errors.append(f"\\{name} needs a braced argument")
        return "", i
    depth, j = 1, i + 1
    while j < len(src) and depth:
        depth += {"{": 1, "}": -1}.get(src[j], 0)
        j += 1
    if depth:
        errors.append(f"\\{name}: unbalanced braces")
        return src[i + 1:j], j
    return src[i + 1:j - 1], j


def _parse_list(toks, i, errors, until=None):
    out = []
    while i < len(toks) and toks[i] != until:
        t = toks[i]
        i += 1
        if isinstance(t, tuple):
            out.append(Atom(ORD, ("text", t[1])))
        elif t in ("^", "_"):
            arg, i = _parse_arg(toks, i, errors)
            if not out:
                out.append(Atom(ORD, ""))
            if t == "^" and out[-1].sub is None:
                out[-1].sup_first = True
            setattr(out[-1], "sup" if t == "^" else "sub", arg)
        elif t == "{":
            inner, i = _parse_list(toks, i, errors, "}")
            out.append(Atom(ORD, inner))
            i += 1
        elif t == "}":
            errors.append("unbalanced }")
        elif t.startswith("\\"):
            i = _command(t[1:], toks, i, out, errors)
        elif t.isascii() and t.isalpha():
            cp = 0x210E if t == "h" else (0x1D44E + ord(t) - 97 if t.islower() else 0x1D434 + ord(t) - 65)
            out.append(Atom(ORD, chr(cp)))
        else:
            out.append(Atom(CLASS.get(t, ORD), "−" if t == "-" else t))
    if until and i >= len(toks):
        errors.append("unbalanced {")
    return out, i


def _command(name, toks, i, out, errors):
    if name in ("frac", "sqrt") or name in ACCENTS or name in BOLD:
        a, i = _parse_arg(toks, i, errors)
        if name == "frac":
            b, i = _parse_arg(toks, i, errors)
            out.append(Atom(INNER, ("frac", a, b)))
        elif name == "sqrt":
            out.append(Atom(ORD, ("sqrt", a)))
        elif name in ACCENTS:
            out.append(Atom(ORD, ("accent", ACCENTS[name], a)))
        else:
            out.append(Atom(ORD, [_bold(x) for x in a]))
    elif name in DELIMS:
        # delimiters are set at their natural size; \left. and \right. set none
        if i < len(toks) and toks[i] == ".":
            i += 1
    elif name in GREEK:
        out.append(Atom(ORD, chr(GREEK[name])))
    elif name in CAPITAL:
        out.append(Atom(ORD, CAPITAL[name]))
    elif name in SYMBOLS:
        ch, cls = SYMBOLS[name]
        out.append(Atom(cls, ch))
    elif name in SPACES:
        out.append(Atom(-1, SPACES[name]))
    else:
        errors.append(f"unknown math command \\{name}")
    return i


def _bold(atom):
    """Mathematical bold for a Latin letter or digit; anything else is kept."""
    n = atom.nucleus
    if isinstance(n, str) and len(n) == 1:
        p = "h" if n == "ℎ" else unicodedata.normalize("NFKC", n)
        if p.isascii() and p.isalpha():
            atom.nucleus = chr((0x1D41A + ord(p) - 97) if p.islower() else (0x1D400 + ord(p) - 65))
        elif p.isascii() and p.isdigit():
            atom.nucleus = chr(0x1D7CE + ord(p) - 48)
    return atom


def _parse_arg(toks, i, errors):
    if i >= len(toks):
        errors.append("a command or script is missing its argument")
        return [], i
    if toks[i] == "{":
        inner, i = _parse_list(toks, i + 1, errors, "}")
        return inner, i + 1
    atoms, _ = _parse_list([toks[i]], 0, errors)
    return atoms, i + 1
