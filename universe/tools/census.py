#!/usr/bin/env python3
"""Distance to green for every file in CHECK-FAILS.txt.

    python3 universe/tools/census.py <path to buildc> [--out DIR] [--max-steps N] [file ...]

One run of `buildc check` does not show everything a file holds. An unresolved import
(BL0611) hides the type errors of the code that uses the name. Type errors hide the C
backend gate, which runs only on a file with none. The gate (BL0901) reports only its
first refusal. So "files whose only error is X" cannot be read from one run, and a
fix chosen from first errors alone can turn no file green.

This script walks past each layer on a scratch copy written beside the file:

  * an unresolved `use` name is taken out of its `use` statement;
  * a diagnostic inside a function body replaces that body with `{ loop { } }`;

and checks again, until the copy passes or nothing more can be taken out. The edits are
a measuring device, not a repair. They answer one question: what does the compiler
refuse in this file, in total?

Per file it reports the stage, the number of walk steps, the unresolved import names
taken out, the functions stubbed, the diagnostics seen by code, and where the walk ended:

    PASS      the copy passes once those names and functions are out of the way: the
              diagnostics listed for the file are everything that stands between it
              and green
    ITEM      a diagnostic on a line outside any function body (a `const`, a field
              type, an `impl` header) stops the walk
    NOLINE    a refusal with no line, usually functions the program calls that have
              no C function behind them (the names are in the detail column)
    PARSE, LEX, TIMEOUT, LIMIT (more than --max-steps steps)

Two tab-separated tables go to stdout between marker lines, and to DIR when --out is
given: `census.tsv` (one row per file) and `diagnostics.tsv` (one row per diagnostic,
with the walk step it appeared in). The script never changes a tracked file and removes
every scratch copy it writes.
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
UNRESOLVED = re.compile(r"^unresolved import `([^`]+)`")
SCRATCH = "__census.bld"
STUB = "{ loop { } }"


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
        m = DIAG.match(line) or DIAG_PARSE.match(line)
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


def blank_strings_and_comments(line, in_block):
    """`line` with string contents and comments blanked, keeping column positions."""
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
            out.append(" " * (j - i))
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


def cleaned(lines):
    out = []
    in_block = False
    for line in lines:
        text, in_block = blank_strings_and_comments(line, in_block)
        out.append(text)
    return out


def function_body(clean, line_no):
    """(open_row, open_col, close_row, close_col) of the body around 1-based `line_no`."""
    head = min(line_no - 1, len(clean) - 1)
    while head >= 0 and not FN_HEAD.match(clean[head]):
        head -= 1
    if head < 0:
        return None
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
                return None  # a declaration without a body
            elif ch == "{" and depth == 0:
                opened = (row, col)
                break
        if opened is None:
            row += 1
    if opened is None:
        return None
    open_row, open_col = opened
    depth = 0
    row, col = open_row, open_col
    while row < len(clean):
        text = clean[row]
        while col < len(text):
            if text[col] == "{":
                depth += 1
            elif text[col] == "}":
                depth -= 1
                if depth == 0:
                    if head <= line_no - 1 <= row:
                        return (open_row, open_col, row, col)
                    return None
            col += 1
        row += 1
        col = 0
    return None


def stub_bodies(lines, bodies):
    """Replace each body, last first so earlier positions stay valid."""
    for open_row, open_col, close_row, close_col in sorted(set(bodies), reverse=True):
        tail = lines[close_row][close_col + 1 :]
        lines[open_row] = lines[open_row][:open_col] + STUB + tail
        for row in range(open_row + 1, close_row + 1):
            lines[row] = ""


def drop_imports(lines, clean, line_no, paths):
    """Take the unresolved `paths` out of the `use` statement that starts on `line_no`."""
    first = line_no - 1
    if first >= len(clean):
        return False
    if re.match(r"^\s*(pub\s+)?mod\s+\w+\s*;", clean[first]):
        lines[first] = ""
        return True
    if not re.match(r"^\s*(pub(\([a-z]+\))?\s+)?use\b", clean[first]):
        return False
    last = first
    while last < len(clean) and ";" not in clean[last]:
        last += 1
    if last >= len(clean):
        return False
    statement = " ".join(clean[first : last + 1])
    m = re.match(r"^(\s*(?:pub(?:\([a-z]+\))?\s+)?use\s+)([^{};]*)\{([^{}]*)\}\s*;(.*)$", statement)
    replacement = ""
    if m:
        prefix, base, body, rest = m.groups()
        leaves = {p.split("::")[-1] for p in paths}
        kept = []
        for item in body.split(","):
            name = item.strip()
            if not name:
                continue
            leaf = name.split(" as ")[0].strip().split("::")[-1]
            whole = (base.strip() + name.split(" as ")[0].strip())
            if leaf in leaves or whole in paths:
                continue
            kept.append(name)
        # A path that names the module itself takes every item with it.
        if base.strip().rstrip(":") in paths:
            kept = []
        if kept:
            replacement = f"{prefix}{base}{{{', '.join(kept)}}};{rest}"
    lines[first] = replacement
    for row in range(first + 1, last + 1):
        lines[row] = ""
    return True


def walk(buildc, rel, max_steps):
    """Returns (stage, steps, terminal, detail, imports out, functions stubbed, rows)."""
    source = os.path.join(HERE, rel)
    scratch_rel = rel[: -len(".bld")] + SCRATCH
    scratch = os.path.join(HERE, scratch_rel)
    with open(source, encoding="utf-8", errors="replace", newline="") as fh:
        lines = fh.read().split("\n")
    rows = []
    stage = "types"
    steps = imports = stubbed = 0

    def done(terminal, detail=""):
        return stage, steps, terminal, detail, imports, stubbed, rows

    try:
        while True:
            with open(scratch, "w", encoding="utf-8", newline="") as fh:
                fh.write("\n".join(lines))
            code, diags, out = run_check(buildc, scratch_rel)
            if code == 0:
                return done("PASS")
            if code == 124:
                return done("TIMEOUT")
            now = stage_of(out)
            if steps == 0:
                stage = now
            if not diags:
                last = (out.strip().splitlines() or ["no output"])[-1]
                return done("NODIAG", last[:200])
            for line_no, bl, message in diags:
                rows.append((rel, steps, line_no, bl, message))
            if now != "types":
                return done("PARSE" if now == "parse" else "LEX", str(len(diags)))
            if steps >= max_steps:
                return done("LIMIT")

            clean = cleaned(lines)
            by_use = {}
            bodies = []
            outside = []
            no_line = []
            for line_no, bl, message in diags:
                m = UNRESOLVED.match(message) if bl == "BL0611" else None
                if line_no == 0:
                    no_line.append(message)
                elif m:
                    by_use.setdefault(line_no, set()).add(m.group(1))
                else:
                    body = function_body(clean, line_no)
                    if body is None:
                        outside.append(f"line {line_no}: [{bl}] {message}")
                    else:
                        bodies.append(body)
            changed = False
            # Bodies first: dropping an import rewrites lines, which would move columns.
            if bodies:
                stub_bodies(lines, bodies)
                stubbed += len(set(bodies))
                changed = True
            for line_no, paths in by_use.items():
                if drop_imports(lines, clean, line_no, paths):
                    imports += len(paths)
                    changed = True
                else:
                    outside.append(f"line {line_no}: [BL0611] {sorted(paths)[0]}")
            if not changed:
                if outside:
                    return done("ITEM", outside[0][:300])
                return done("NOLINE", no_line[0][:600])
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
    ap.add_argument("--max-steps", type=int, default=40)
    args = ap.parse_args()
    buildc = os.path.abspath(args.buildc)
    files = args.files or listed_files()

    census = [
        "file\tstage\tsteps\tterminal\tdetail\timports taken out\tfunctions stubbed\tdiagnostics by code"
    ]
    table = ["file\tstep\tline\tcode\tmessage"]
    for rel in files:
        stage, steps, terminal, detail, imports, stubbed, rows = walk(buildc, rel, args.max_steps)
        counts = {}
        for row in rows:
            counts[row[3]] = counts.get(row[3], 0) + 1
        codes = " ".join(f"{k}:{n}" for k, n in sorted(counts.items(), key=lambda kv: (-kv[1], kv[0])))
        detail = detail.replace("\t", " ")
        census.append(f"{rel}\t{stage}\t{steps}\t{terminal}\t{detail}\t{imports}\t{stubbed}\t{codes}")
        for row in rows:
            message = row[4].replace("\t", " ")
            table.append(f"{row[0]}\t{row[1]}\t{row[2]}\t{row[3]}\t{message}")
        print(f"{rel}: {stage}, {steps} steps, {terminal}", file=sys.stderr, flush=True)

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
