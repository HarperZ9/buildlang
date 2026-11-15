#!/usr/bin/env python3
"""Distance to green for every file in CHECK-FAILS.txt.

    python3 universe/tools/census.py <path to buildc> [--out DIR] [--max-steps N] [file ...]

`buildc check` lists every type error of a file, but only the FIRST refusal of the C
backend (BL0901). A file that shows one error can hide a chain of them, so "files whose
only error is X" cannot be read from one run. This script makes the chain visible.

For each failing file it checks a scratch copy written beside the file. While the first
diagnostic is a BL0901 with a line, it replaces the body of the function around that line
with `{ loop { } }` and checks again. The stub is a measuring device, not a repair: it
answers "what else does the compiler refuse in this file", nothing more.

Per file it reports the stage, the number of functions stubbed, the refusals seen on the
way (as classes: names and numbers blanked), and where the walk ended:

    PASS      nothing else is refused once those functions are out of the way
    TYPES     type errors (the checker lists all of them; each is in the diagnostics table)
    NOLINE    a backend refusal with no line, usually functions the program calls that
              no C function stands behind (the names are in the detail column)
    ITEM      a backend refusal on a line outside any function body (a `const`, a field)
    PARSE, LEX, TIMEOUT, LIMIT (more than --max-steps functions stubbed)

Two tables go to stdout between marker lines, tab-separated, and to DIR when --out is
given: `census.tsv` (one row per file) and `diagnostics.tsv` (one row per diagnostic).
The script never changes a tracked file and removes every scratch copy it writes.
"""
import argparse
import os
import re
import subprocess
import sys

HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DIAG = re.compile(r"^\s*(\d+):(\d+): \[(BL\d+)\] (.*)$")
DIAG_NO_LINE = re.compile(r"^\s*\[(BL\d+)\] (.*)$")
DIAG_PARSE = re.compile(r"^error\[[^\]]*:(\d+):(\d+)\]: \[(BL\d+)\] (.*)$")
DIAG_LEX = re.compile(r"^Lexer error \[(BL\d+)\]:? (.*)$")
FN_HEAD = re.compile(
    r"^\s*(pub(\([a-z]+\))?\s+)?((const|async|unsafe|extern\s+\"C\")\s+)*fn\s+\w+"
)
SCRATCH = "__census.bld"


def run_check(buildc, rel):
    try:
        proc = subprocess.run(
            [buildc, "check", rel],
            cwd=HERE,
            capture_output=True,
            text=True,
            timeout=150,
            encoding="utf-8",
            errors="replace",
        )
    except subprocess.TimeoutExpired:
        return 124, [], ""
    out = proc.stdout + proc.stderr
    diags = []
    for line in out.splitlines():
        m = DIAG.match(line)
        if m:
            diags.append((int(m.group(1)), m.group(3), m.group(4).strip()))
            continue
        m = DIAG_PARSE.match(line)
        if m:
            diags.append((int(m.group(1)), m.group(3), m.group(4).strip()))
            continue
        m = DIAG_NO_LINE.match(line) or DIAG_LEX.match(line)
        if m:
            diags.append((0, m.group(1), m.group(2).strip()))
    return proc.returncode, diags, out


def stage_of(out):
    if "Parsing... OK" in out:
        return "types"
    if "Lexing... OK" in out:
        return "parse"
    return "lex"


def klass(message):
    """A diagnostic with its names and numbers blanked, so equal causes compare equal."""
    message = re.sub(r"`[^`]*`", "`_`", message)
    message = re.sub(r"\(also: [^)]*\)", "(also: _)", message)
    message = re.sub(r"\d+", "N", message)
    return message[:120]


def blank_strings_and_comments(line, in_block):
    """`line` with string contents and comments removed, keeping column positions."""
    out = []
    i, n = 0, len(line)
    while i < n:
        if in_block:
            j = line.find("*/", i)
            if j < 0:
                out.append(" " * (n - i))
                return "".join(out), True
            out.append(" " * (j + 2 - i))
            i = j + 2
            in_block = False
            continue
        if line.startswith("//", i):
            out.append(" " * (n - i))
            break
        if line.startswith("/*", i):
            in_block = True
            out.append("  ")
            i += 2
            continue
        ch = line[i]
        if ch == '"':
            j = i + 1
            while j < n and line[j] != '"':
                j += 2 if line[j] == "\\" else 1
            j = min(j + 1, n)
            out.append('"' + " " * (j - i - 2) + '"' if j - i >= 2 else " " * (j - i))
            i = j
            continue
        if ch == "'":
            # a char literal ('x', '\n', '\u{1F600}'); a lifetime ('a) has no closing quote
            m = re.match(r"'(\\.[^']*|[^'\\])'", line[i:])
            if m:
                out.append(" " * len(m.group(0)))
                i += len(m.group(0))
                continue
        out.append(ch)
        i += 1
    return "".join(out), in_block


def stub_function(lines, line_no):
    """Replace the body of the function around 1-based `line_no`. False when there is none."""
    clean = []
    in_block = False
    for line in lines:
        text, in_block = blank_strings_and_comments(line, in_block)
        clean.append(text)
    head = line_no - 1
    while head >= 0 and not FN_HEAD.match(clean[head]):
        head -= 1
    if head < 0:
        return False
    # The body opens at the first `{` outside parentheses and brackets.
    depth = 0
    opened = None
    row = head
    while row < len(clean) and opened is None:
        for col, ch in enumerate(clean[row]):
            if ch in "([":
                depth += 1
            elif ch in ")]":
                depth -= 1
            elif ch == ";" and depth == 0:
                return False  # a declaration without a body
            elif ch == "{" and depth == 0:
                opened = (row, col)
                break
        if opened is None:
            row += 1
    if opened is None:
        return False
    open_row, open_col = opened
    depth = 0
    closed = None
    row, col = open_row, open_col
    while row < len(clean) and closed is None:
        text = clean[row]
        while col < len(text):
            if text[col] == "{":
                depth += 1
            elif text[col] == "}":
                depth -= 1
                if depth == 0:
                    closed = (row, col)
                    break
            col += 1
        if closed is None:
            row += 1
            col = 0
    if closed is None:
        return False
    close_row, close_col = closed
    if not open_row <= line_no - 1 <= close_row and not head <= line_no - 1 <= close_row:
        return False
    tail = lines[close_row][close_col + 1 :]
    lines[open_row] = lines[open_row][:open_col] + "{ loop { } }" + tail
    for row in range(open_row + 1, close_row + 1):
        lines[row] = ""
    return True


def walk(buildc, rel, max_steps):
    """Returns (stage, steps, terminal, detail, diagnostics rows)."""
    source = os.path.join(HERE, rel)
    scratch_rel = rel[: -len(".bld")] + SCRATCH
    scratch = os.path.join(HERE, scratch_rel)
    with open(source, encoding="utf-8", errors="replace", newline="") as fh:
        lines = fh.read().split("\n")
    rows = []
    stage = "types"
    steps = 0
    try:
        while True:
            with open(scratch, "w", encoding="utf-8", newline="") as fh:
                fh.write("\n".join(lines))
            code, diags, out = run_check(buildc, scratch_rel)
            if code == 0:
                return stage, steps, "PASS", "", rows
            if code == 124:
                return stage, steps, "TIMEOUT", "", rows
            if steps == 0:
                stage = stage_of(out)
            if not diags:
                last = (out.strip().splitlines() or ["no output"])[-1]
                return stage, steps, "NODIAG", last[:200], rows
            line_no, bl, message = diags[0]
            if bl != "BL0901":
                kind = {"types": "TYPES", "parse": "PARSE", "lex": "LEX"}[stage_of(out)]
                for d_line, d_code, d_message in diags:
                    rows.append((rel, steps, d_line, d_code, d_message))
                return stage, steps, kind, str(len(diags)), rows
            rows.append((rel, steps, line_no, bl, message))
            if line_no == 0:
                return stage, steps, "NOLINE", message, rows
            if steps >= max_steps:
                return stage, steps, "LIMIT", "", rows
            if not stub_function(lines, line_no):
                return stage, steps, "ITEM", f"line {line_no}: {message}", rows
            steps += 1
    finally:
        if os.path.exists(scratch):
            os.remove(scratch)


def listed_files():
    files = []
    with open(os.path.join(HERE, "CHECK-FAILS.txt"), encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if line and not line.startswith("#"):
                files.append(line.split()[0])
    return files


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("buildc")
    ap.add_argument("files", nargs="*")
    ap.add_argument("--out")
    ap.add_argument("--max-steps", type=int, default=60)
    args = ap.parse_args()
    buildc = os.path.abspath(args.buildc)
    files = args.files or listed_files()

    census = ["file\tstage\tstubbed\tterminal\tdetail\trefusal classes on the way"]
    table = ["file\tstep\tline\tcode\tmessage"]
    for rel in files:
        stage, steps, terminal, detail, rows = walk(buildc, rel, args.max_steps)
        counts = {}
        for row in rows:
            if row[3] == "BL0901" and row[2] != 0:
                counts[klass(row[4])] = counts.get(klass(row[4]), 0) + 1
        classes = " | ".join(
            f"{n}x {k}" for k, n in sorted(counts.items(), key=lambda kv: (-kv[1], kv[0]))
        )
        detail = detail.replace("\t", " ")
        census.append(f"{rel}\t{stage}\t{steps}\t{terminal}\t{detail}\t{classes}")
        for row in rows:
            message = row[4].replace("\t", " ")
            table.append(f"{row[0]}\t{row[1]}\t{row[2]}\t{row[3]}\t{message}")
        print(f"{rel}: {stage}, {steps} stubbed, {terminal}", file=sys.stderr, flush=True)

    print("=== CENSUS BEGIN ===")
    print("\n".join(census))
    print("=== CENSUS END ===")
    print("=== DIAGNOSTICS BEGIN ===")
    print("\n".join(table))
    print("=== DIAGNOSTICS END ===")
    if args.out:
        os.makedirs(args.out, exist_ok=True)
        with open(os.path.join(args.out, "census.tsv"), "w", encoding="utf-8", newline="\n") as fh:
            fh.write("\n".join(census) + "\n")
        with open(
            os.path.join(args.out, "diagnostics.tsv"), "w", encoding="utf-8", newline="\n"
        ) as fh:
            fh.write("\n".join(table) + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
