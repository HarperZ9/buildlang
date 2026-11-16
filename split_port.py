"""Split port/main.bld into modules at its section markers. Run from the repo root."""
import io
import re

src = io.open("port/main.bld", newline="").read()
nl = "\r\n" if "\r\n" in src else "\n"
L = src.split(nl)


def cut(a, b):  # 1-indexed inclusive
    return L[a - 1 : b]


main_start = next(i for i, l in enumerate(L, 1) if l.startswith("fn main("))
parts = [
    ("types", cut(18, 60), [], "Type codes and the conversions C makes between number types."),
    ("lex", cut(61, 168), [], "The C lexer: source text to token kinds, texts and lines."),
    ("state", cut(169, 336), ["types"], "Parser state, the symbol tables and pass one (signatures and who prints)."),
    ("exprs", cut(337, 591), ["types", "state"], "Expressions and printf: C expression to BuildLang text and its type."),
    ("stmts", cut(592, 811), ["types", "state", "exprs"], "Statements and declarations."),
    ("file", cut(812, main_start - 1), ["types", "state", "exprs", "stmts"], "The whole file: globals, functions and the output text."),
]


def publish(lines):
    out = []
    for l in lines:
        if re.match(r"(fn|struct) ", l):
            l = "pub " + l
        out.append(l)
    while out and out[-1] == "":
        out.pop()
    return out


for name, lines, uses, about in parts:
    head = ["// SPDX-License-Identifier: MIT", "//", "// port/" + name + ".bld: " + about, ""]
    head += ["use " + u + "::*;" for u in uses]
    if uses:
        head.append("")
    body = head + publish(lines) + [""]
    io.open("port/" + name + ".bld", "w", newline="").write(nl.join(body))
    print(name, len(body))

mods = ["mod " + p[0] + ";" for p in parts] + [""] + ["use " + p[0] + "::*;" for p in parts] + [""]
main = cut(1, 17) + mods + cut(main_start, len(L))
io.open("port/main.bld", "w", newline="").write(nl.join(main))
print("main", len(main))
