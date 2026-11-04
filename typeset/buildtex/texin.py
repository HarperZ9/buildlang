"""One-way import of a LaTeX subset into buildlang.doc/v0.

BuildTeX never runs TeX. This module reads .tex source as text so an
existing document can move into BuildLang once; after that the BuildLang
program (or the IR it saves) is the source of record.

Covered:
  preamble   \\documentclass, \\usepackage, \\newcommand and similar are skipped
  title      \\title{} \\author{} \\date{} \\maketitle -> h1, contact, meta
  structure  \\section{} \\section*{} -> h2, \\subsection{} -> h3, \\paragraph{} -> h3,
             itemize and enumerate \\item -> li, \\newpage and \\clearpage -> pagebreak,
             \\label{} attached to the block it follows
  inline     \\emph \\textit -> {i:}, \\textbf -> {b:}, \\textsc -> {sc:}, \\texttt \\url -> {tt:},
             \\href{url}{text} -> text, \\ref{} \\cite{} -> {ref:} {cite:}, $..$ -> {m:}
  display    \\[ .. \\] and equation(*) -> eq
  text       TeX quotes, -- and ---, ~ ties, \\& \\% \\_ \\# \\$, \\ldots, % comments, \\\\ breaks
Anything else is collected in `unknown`, never dropped without a record.
"""
import re

INLINE = {"emph": "i", "textit": "i", "textbf": "b", "textsc": "sc", "texttt": "tt", "url": "tt"}
SKIP_ARG1 = {"label", "vspace", "hspace", "pagestyle", "thispagestyle", "setlength", "addtolength"}
DROP = {"small", "footnotesize", "large", "Large", "noindent", "centering", "par", "medskip", "smallskip",
        "bigskip", "hfill", "raggedright", "normalsize", "allowbreak", "quad", "qquad"}
TEXT = [("\\textbar", "|"), ("\\ldots", "\u2026"), ("\\dots", "\u2026"), ("\\textcopyright", "\u00a9"),
        ("\\&", "&"), ("\\%", "%"), ("\\_", "_"), ("\\#", "#"), ("---", "\u2014"), ("--", "\u2013"),
        ("``", "\u201c"), ("''", "\u201d")]
BLOCK_CMDS = re.compile(r"(\\(?:section|subsection|paragraph)\*?\{|\\begin\{[a-z*]+\}|\\end\{[a-z*]+\}|"
                        r"\\item\b|\\maketitle\b|\\newpage\b|\\clearpage\b|\\\[|\\\])")


class TexImportError(ValueError):
    pass


def group(s, i):
    """(content, end) for the brace group opening at s[i] == '{'."""
    if i >= len(s) or s[i] != "{":
        raise TexImportError(f"expected '{{' at {i}: {s[i:i + 30]!r}")
    depth, j = 0, i
    while j < len(s):
        if s[j] == "\\":
            j += 2
            continue
        depth += {"{": 1, "}": -1}.get(s[j], 0)
        if depth == 0:
            return s[i + 1:j], j + 1
        j += 1
    raise TexImportError(f"unbalanced group at {i}: {s[i:i + 40]!r}")


class Importer:
    def __init__(self):
        self.unknown = set()

    def inline(self, s):
        s = s.replace("\\$", "\u0000")
        # \texttt and \url arguments are verbatim: no quote or dash ligatures inside them.
        kept = []
        out, i = [], 0
        for m in re.finditer(r"\\(?:texttt|url)\{", s):
            if m.start() < i:
                continue
            arg, end = group(s, m.end() - 1)
            kept.append(arg.replace("\\_", "_").replace("\\%", "%").replace("\\&", "&").replace("\\#", "#"))
            out.append(s[i:m.end()] + "\u0001%d\u0001}" % (len(kept) - 1))
            i = end
        s = "".join(out) + s[i:]
        text = self._inline(s)
        return re.sub("\u0001(\\d+)\u0001", lambda m: kept[int(m.group(1))], text)

    def _inline(self, s):
        for a, b in TEXT:
            s = s.replace(a, b)
        s = s.replace("`", "\u2018").replace("'", "\u2019")
        out, i = [], 0
        while i < len(s):
            c = s[i]
            if c == "$":
                j = s.find("$", i + 1)
                if j < 0:
                    raise TexImportError(f"unclosed $ at {i}")
                out.append("{m:" + s[i + 1:j].replace("{", "{{").replace("}", "}}") + "}")
                i = j + 1
            elif c == "\\":
                m = re.match(r"\\([A-Za-z]+)\*?\s*", s[i:])
                if not m:
                    if s.startswith("\\\\", i):
                        out.append(" ")
                    i += 2
                    continue
                name, i = m.group(1), i + m.end()
                if name in INLINE:
                    arg, i = group(s, i)
                    body = arg.replace("{", "").replace("}", "") if INLINE[name] == "tt" else self._inline(arg)
                    out.append("{%s:%s}" % (INLINE[name], body))
                elif name == "href":
                    _, i = group(s, i)
                    arg, i = group(s, i)
                    out.append(self._inline(arg))
                elif name in ("ref", "cite"):
                    arg, i = group(s, i)
                    out.append("{%s:%s}" % (name, arg))
                elif name in SKIP_ARG1:
                    _, i = group(s, i)
                elif name not in DROP:
                    self.unknown.add(name)
            elif c == "~":
                out.append("\u00a0")
                i += 1
            elif c in "{}":
                i += 1
            else:
                out.append(c)
                i += 1
        return " ".join("".join(out).replace("\u0000", "$").split())

    def convert(self, tex):
        tex = re.sub(r"(?<!\\)%.*", "", tex)
        if "\\begin{document}" not in tex:
            raise TexImportError("no \\begin{document}")
        pre, body = tex.split("\\begin{document}", 1)
        body = body.split("\\end{document}", 1)[0]
        meta = {}
        for key in ("title", "author", "date"):
            k = pre.find("\\" + key + "{")
            if k >= 0:
                meta[key], _ = group(pre, k + len(key) + 1)
        blocks, env, i = [], [], 0
        parts = BLOCK_CMDS.split(body)
        while i < len(parts):
            tok = parts[i]
            i += 1
            if tok is None or not tok.strip():
                continue
            m = re.match(r"\\(section|subsection|paragraph)(\*?)\{$", tok)
            if m:
                rest = "{" + (parts[i] if i < len(parts) else "")
                title, end = group(rest, 0)
                kind = {"section": "h2", "subsection": "h3", "paragraph": "h3"}[m.group(1)]
                blocks.append([kind, self.inline(title), ""])
                parts[i] = rest[end:]
                continue
            if tok == "\\maketitle":
                blocks.append(["h1", self.inline(meta.get("title", "")), ""])
                for line in re.split(r"\\\\(?:\[[^\]]*\])?", meta.get("author", "")):
                    if self.inline(line):
                        blocks.append(["contact", self.inline(line), ""])
                if meta.get("date"):
                    blocks.append(["meta", self.inline(meta["date"]), ""])
                continue
            if tok in ("\\newpage", "\\clearpage"):
                blocks.append(["pagebreak", "", ""])
                continue
            m = re.match(r"\\begin\{([a-z*]+)\}", tok)
            if m:
                env.append(m.group(1))
                continue
            if tok.startswith("\\end{"):
                if env:
                    env.pop()
                continue
            if tok == "\\[":
                env.append("displaymath")
                continue
            if tok == "\\]":
                if env:
                    env.pop()
                continue
            if tok == "\\item":
                blocks.append(["li", "", ""])
                continue
            self._text(tok, env, blocks)
        return meta, [tuple(b) for b in blocks if b[0] == "pagebreak" or b[1]]

    def _text(self, tok, env, blocks):
        label = re.findall(r"\\label\{([^}]*)\}", tok)
        if env and env[-1] in ("displaymath", "equation", "equation*"):
            blocks.append(["eq", " ".join(re.sub(r"\\label\{[^}]*\}", "", tok).split()), label[0] if label else ""])
            return
        for k, para in enumerate(re.split(r"\n\s*\n", tok)):
            text = self.inline(para)
            if not text:
                continue
            if k == 0 and blocks and blocks[-1][0] == "li" and not blocks[-1][1]:
                blocks[-1][1] = text
            else:
                blocks.append(["p", text, ""])
        if label and blocks:
            blocks[-1][2] = label[0]


def to_ir(meta, blocks, theme, author=""):
    """The buildlang.doc/v0 text a BuildLang document program would save for these blocks."""
    clean = lambda s: s.replace("\t", " ").replace("\n", " ")
    imp = Importer()
    lines = ["#buildlang.doc/v0", "meta\ttitle\t" + clean(re.sub(r"\{[a-z]+:|[{}]", "",
                                                               imp.inline(meta.get("title", ""))))]
    if author:
        lines.append("meta\tauthor\t" + clean(author))
    lines.append("theme\t" + theme)
    for kind, text, label in blocks:
        lines.append("\t".join(["block", kind, clean(text)] + ([label] if label else [])))
    return "\n".join(lines) + "\n"


def to_program(name, meta, blocks, theme):
    """The same document as a BuildLang program over typeset/doc/doc.bld."""
    q = lambda s: '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'
    out = [f"// {name}: imported once from LaTeX by buildtex.texin; this program is now the source.",
           "mod doc;", "use doc::*;", "", "fn main() ~ Console + FileSystem {",
           f"    let mut d = doc_new({q(re.sub(r'[{}]', '', meta.get('title', name)))});",
           f"    theme(&mut d, {q(theme)});"]
    for kind, text, label in blocks:
        out.append(f"    labelled(&mut d, {q(kind)}, {q(label)}, {q(text)});" if label
                   else f"    blk(&mut d, {q(kind)}, {q(text)});")
    out += [f'    let ok = save(&d, "{name}.doc.tsv");', '    println!("{} blocks ({})", d.blocks, ok);', "}", ""]
    return "\n".join(out)
