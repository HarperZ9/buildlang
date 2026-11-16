#!/usr/bin/env python3
"""Distance to green for every file in CHECK-FAILS.txt.

    python3 universe/tools/census.py <path to buildc> [--out DIR] [--max-steps N] [file ...]

One run of `buildc check` does not show everything a file holds. An unresolved import
(BL0611) hides the type errors of the code that uses the name. Type errors hide the C
backend gate, which runs only on a file with none. The gate (BL0901) reports one
refusal per function (before PR 302 it reported one per file), and none of the next
layer. So "files whose only error is X" cannot be read from one run, and a fix
chosen from first errors alone can turn no file green.

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
    CRASH     the compiler stopped with no diagnostic: it was killed by a signal, or it
              asked for more memory than --memory-mb allows (a compiler fault; the
              detail column has the last line it printed)
    PARSE, LEX, TIMEOUT, LIMIT (more than --max-steps steps, or the file's time budget),
    BUDGET (the run's time budget ended before the file)

One line per file goes to stderr when its walk starts and when it ends, so a run that
is stopped from outside still shows which files were in flight.

PASS is necessary for "these diagnostics are all of it", not sufficient: a stubbed function
shows every type error it has, but only the first refusal of the C backend gate inside it.
A stub that would break the parse is not applied (the walker's own limit, not the file's);
the diagnostic it was for then ends the walk as ITEM.

Two tab-separated tables go to stdout between marker lines, and to DIR when --out is
given: `census.tsv` (one row per file) and `diagnostics.tsv` (one row per diagnostic,
with the walk step it appeared in). The script never changes a tracked file and removes
every scratch copy it writes.
"""
import argparse
import concurrent.futures
import os
import re
import shutil
import subprocess
import sys
import time

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
# Address space one `buildc check` may take, in MiB (0: no cap). Set from --memory-mb.
# A check that runs away would otherwise take the machine down with it.
MEMORY_MB = 0


def check_command(buildc, rel):
    """`buildc check rel`, under `prlimit` when a memory cap is set and the tool exists."""
    command = [buildc, "check", rel]
    prlimit = shutil.which("prlimit") if MEMORY_MB > 0 else None
    if prlimit:
        command = [prlimit, f"--as={MEMORY_MB * 1024 * 1024}"] + command
    return command


def run_check(buildc, rel):
    try:
        proc = subprocess.run(
            check_command(buildc, rel),
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
    if not diags and (proc.returncode < 0 or proc.returncode > 128 or "memory allocation" in out):
        tail = out.strip().splitlines()[-15:] or ["no output"]
        shown = "".join("\n  " + line for line in tail)
        print(f"census: crash in {rel} (exit {proc.returncode}):{shown}", file=sys.stderr, flush=True)
        message = next((line for line in tail if "panicked" in line or "memory allocation" in line), tail[-1])
        return 125, [], f"exit {proc.returncode}: {message.strip()}"
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


def walk(buildc, rel, max_steps, deadline):
    """Returns (stage, steps, terminal, detail, imports out, functions stubbed, rows)."""
    source = os.path.join(HERE, rel)
    scratch_rel = rel[: -len(".bld")] + SCRATCH
    scratch = os.path.join(HERE, scratch_rel)
    with open(source, encoding="utf-8", errors="replace", newline="") as fh:
        lines = fh.read().split("\n")
    rows = []
    steps = imports = stubbed = 0
    refused = set()  # bodies whose stub broke the parse: the walker's fault, left alone

    def check(text_lines):
        with open(scratch, "w", encoding="utf-8", newline="") as fh:
            fh.write("\n".join(text_lines))
        return run_check(buildc, scratch_rel)

    def parses(result):
        if result[0] == 125:
            return True  # the copy parsed and the compiler then crashed: the walk ends there
        return result[0] == 0 or (result[0] != 124 and stage_of(result[2]) == "types")

    try:
        result = check(lines)
        stage = "types" if result[0] in (0, 124, 125) else stage_of(result[2])

        def done(terminal, detail=""):
            return stage, steps, terminal, detail, imports, stubbed, rows

        while True:
            code, diags, out = result
            if code == 0:
                return done("PASS")
            if code == 124:
                return done("TIMEOUT")
            if code == 125:
                return done("CRASH", out[:300])
            if not diags:
                last = (out.strip().splitlines() or ["no output"])[-1]
                return done("NODIAG", last[:200])
            for line_no, bl, message in diags:
                rows.append((rel, steps, line_no, bl, message))
            if stage != "types":
                return done("PARSE" if stage == "parse" else "LEX", str(len(diags)))
            if steps >= max_steps:
                return done("LIMIT")
            if time.monotonic() > deadline:
                return done("LIMIT", "time budget for one file")

            clean = cleaned(lines)
            by_use = {}
            bodies = set()
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
                    if body is None or body[:2] in refused:
                        outside.append(f"line {line_no}: [{bl}] {message}")
                    else:
                        bodies.add(body)

            # Imports first on their own copy; a statement the walker cannot rewrite stays.
            current = list(lines)
            dropped = 0
            for line_no, paths in by_use.items():
                if drop_imports(current, clean, line_no, paths):
                    dropped += len(paths)
                else:
                    outside.append(f"line {line_no}: [BL0611] {sorted(paths)[0]}")
            next_result = None
            if dropped:
                trial = check(current)
                if parses(trial):
                    next_result = trial
                else:
                    current = list(lines)
                    dropped = 0
                    outside.append("line 0: [BL0611] the walker could not rewrite a `use` statement")

            applied = 0
            if bodies:
                together = list(current)
                stub_bodies(together, bodies)
                trial = check(together)
                if parses(trial):
                    current, next_result, applied = together, trial, len(bodies)
                else:
                    # One stub broke the parse. Apply them one at a time and keep the sound ones.
                    for body in sorted(bodies, reverse=True)[:120]:
                        if time.monotonic() > deadline:
                            break
                        single = list(current)
                        stub_bodies(single, [body])
                        trial = check(single)
                        if parses(trial):
                            current, next_result = single, trial
                            applied += 1
                        else:
                            refused.add(body[:2])
            if not dropped and not applied:
                if outside:
                    return done("ITEM", outside[0][:300])
                return done("NOLINE", no_line[0][:600])
            lines = current
            imports += dropped
            stubbed += applied
            result = next_result
            steps += 1
    finally:
        if os.path.exists(scratch):
            os.remove(scratch)


BASELINE = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "CENSUS-BASELINE.txt")


def step0_count(table):
    """Diagnostics at step 0: what one `buildc check` of each file reports."""
    return sum(1 for row in table[1:] if row.split("\t")[1:2] == ["0"])


def print_removed(table):
    """Print the step-0 count beside the committed baseline. Not a gate."""
    now = step0_count(table)
    try:
        with open(BASELINE, encoding="utf-8") as fh:
            base = int(fh.read().split()[-1])
    except (OSError, ValueError, IndexError) as err:
        print(f"census: step-0 diagnostics {now}; no baseline ({err})")
        return
    print(f"census: step-0 diagnostics {now}, baseline {base}, removed {base - now}")


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
    ap.add_argument("--max-steps", type=int, default=120)
    ap.add_argument("--file-seconds", type=float, default=40.0, help="time budget for one file")
    ap.add_argument("--jobs", type=int, default=os.cpu_count() or 2, help="walks at a time")
    ap.add_argument("--total-seconds", type=float, default=690.0, help="time budget for the run")
    ap.add_argument(
        "--memory-mb",
        type=int,
        default=3072,
        help="address space one check may take, through `prlimit` where it exists (0: no cap)",
    )
    args = ap.parse_args()
    global MEMORY_MB
    MEMORY_MB = args.memory_mb
    buildc = os.path.abspath(args.buildc)
    files = args.files or listed_files()

    census = [
        "file\tstage\tsteps\tterminal\tdetail\timports taken out\tfunctions stubbed\tdiagnostics by code"
    ]
    table = ["file\tstep\tline\tcode\tmessage"]
    started = time.monotonic()

    def one(rel):
        if time.monotonic() - started > args.total_seconds:
            return rel, None
        began = time.monotonic()
        print(f"census: start {rel}", file=sys.stderr, flush=True)
        result = walk(buildc, rel, args.max_steps, began + args.file_seconds)
        print(
            f"census: end {rel} {result[2]} after {result[1]} steps, {time.monotonic() - began:.0f} s",
            file=sys.stderr,
            flush=True,
        )
        return rel, result

    # The walks are independent (each has its own scratch copy) and spend their time in
    # `buildc`, so threads are enough. Results are written in the order of the list.
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as pool:
        results = list(pool.map(one, files))
    for rel, result in results:
        if result is None:
            census.append(f"{rel}	?	0	BUDGET	the run's time budget ended before this file	0	0	")
            continue
        stage, steps, terminal, detail, imports, stubbed, rows = result
        counts = {}
        for row in rows:
            counts[row[3]] = counts.get(row[3], 0) + 1
        codes = " ".join(f"{k}:{n}" for k, n in sorted(counts.items(), key=lambda kv: (-kv[1], kv[0])))
        detail = detail.replace("	", " ")
        census.append(f"{rel}	{stage}	{steps}	{terminal}	{detail}	{imports}	{stubbed}	{codes}")
        for row in rows:
            message = row[4].replace("	", " ")
            table.append(f"{row[0]}	{row[1]}	{row[2]}	{row[3]}	{message}")

    print("=== CENSUS BEGIN ===")
    print("\n".join(census))
    print("=== CENSUS END ===")
    print("=== DIAGNOSTICS BEGIN ===")
    print("\n".join(table))
    print("=== DIAGNOSTICS END ===")
    print_removed(table)
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
