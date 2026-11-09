#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Run sqllogictest scripts against qdb (or SQLite) and score every record.

sqllogictest is D. Richard Hipp's SQL conformance corpus. Its expected
results are SQLite's. This runner reads the test files, applies the corpus's
own conditionals for the engine name `sqlite` (so it scores exactly the
records SQLite is scored on), runs each record and classifies it:

  query records      PASS      the engine answered and the values match
                     WRONG     the engine answered with other values
                     REJECTED  the engine printed an error or exited non-zero
  statement records  OK        `statement ok` succeeded or `statement error` failed
                     BAD       anything else

qdb runs one process per record on a database file in a temporary directory,
the way its CLI is used. Values are rendered the corpus's way: NULL as NULL,
the empty string as (empty), I columns as integers, R columns with three
decimals. qdb stores NULL and '' as the same empty cell, so an empty cell is
rendered NULL.

Two self-checks guard the runner itself:
  --engine sqlite   must score 100 percent of the records it runs, or the
                    parsing, sorting or hashing here is wrong;
  planted check     after a qdb run, the first passing query is re-scored
                    with one value altered and must come out WRONG.

  python slt_run.py CORPUS_DIR --qdb QDB_EXE [--json OUT] [--files a.test,b.test]
  python slt_run.py CORPUS_DIR --engine sqlite
"""
import argparse
import hashlib
import json
import re
import os
import shutil
import sqlite3
import subprocess
import sys
import tempfile
import time

DEFAULT_FILES = ["select1.test", "select2.test", "select3.test", "select4.test", "select5.test",
                 "evidence/in1.test", "evidence/in2.test", "evidence/slt_lang_aggfunc.test",
                 "evidence/slt_lang_createtrigger.test", "evidence/slt_lang_createview.test",
                 "evidence/slt_lang_dropindex.test", "evidence/slt_lang_droptable.test",
                 "evidence/slt_lang_droptrigger.test", "evidence/slt_lang_dropview.test",
                 "evidence/slt_lang_reindex.test", "evidence/slt_lang_replace.test",
                 "evidence/slt_lang_update.test"]
ENGINE_NAME = "sqlite"
SEMI_NL = ";" + chr(10)


def parse_records(text):
    """Yield (kind, header_words, sql, expected_lines, conditions, lineno)."""
    lines = text.replace("\r", "").split("\n")
    i, n = 0, len(lines)
    while i < n:
        while i < n and (lines[i].strip() == "" or lines[i].startswith("#")):
            i += 1
        if i >= n:
            break
        start = i
        conds = []
        while i < n and lines[i].split()[:1] and lines[i].split()[0] in ("skipif", "onlyif"):
            conds.append(tuple(lines[i].split()[:2]))
            i += 1
        if i >= n:
            break
        words = lines[i].split()
        i += 1
        kind = words[0]
        if kind in ("hash-threshold", "halt"):
            yield kind, words, "", [], conds, start + 1
            continue
        body = []
        while i < n and lines[i].strip() != "" and lines[i] != "----":
            body.append(lines[i])
            i += 1
        expected = []
        if i < n and lines[i] == "----":
            i += 1
            while i < n and lines[i].strip() != "":
                expected.append(lines[i])
                i += 1
        yield kind, words, "\n".join(body), expected, conds, start + 1


def applies(conds):
    for c, eng in conds:
        if c == "skipif" and eng == ENGINE_NAME:
            return False
        if c == "onlyif" and eng != ENGINE_NAME:
            return False
    return True


def render(v, t):
    """Render one value the sqllogictest way for column type t (I, R or T)."""
    if v is None:
        return "NULL"
    if t == "I":
        if isinstance(v, (int, float)):
            return str(int(v))
        try:
            return str(int(float(v)))
        except ValueError:
            return "0"
    if t == "R":
        try:
            return "%.3f" % float(v)
        except ValueError:
            return "0.000"
    s = str(v)
    if s == "":
        return "(empty)"
    return "".join(c if " " <= c <= "~" else "@" for c in s)


def score_values(rows, types, sortmode, expected, threshold):
    """rows: list of tuples of raw values. Returns True on a match."""
    nc = len(types)
    rendered = [[render(r[j] if j < len(r) else None, types[j]) for j in range(nc)] for r in rows]
    if sortmode == "rowsort":
        rendered.sort()
    values = [v for r in rendered for v in r]
    if sortmode == "valuesort":
        values.sort()
    if len(expected) == 1 and " values hashing to " in expected[0]:
        want_n, want_h = expected[0].split(" values hashing to ")
        h = hashlib.md5("".join(v + "\n" for v in values).encode()).hexdigest()
        return int(want_n) == len(values) and h == want_h.strip()
    if threshold > 0 and len(values) > threshold:
        # The script was generated with a threshold yet lists values; compare directly.
        pass
    exp = list(expected)
    if exp == values:
        return True
    toks = [t for line in exp for t in line.split()]
    return toks == values


class SqliteEngine:
    def __init__(self):
        self.con = sqlite3.connect(":memory:", isolation_level=None)

    def statement(self, sql):
        try:
            self.con.execute(sql)
            return True
        except sqlite3.Error:
            return False

    def query(self, sql, ncols):
        try:
            return self.con.execute(sql).fetchall()
        except sqlite3.Error:
            return None


class QdbEngine:
    def __init__(self, exe, wd):
        self.exe = exe
        self.db = os.path.join(wd, "slt.db")
        for p in (self.db, self.db + ".wal", self.db + ".ckpt"):
            if os.path.exists(p):
                os.remove(p)

    def run(self, sql):
        one = " ".join(sql.split("\n"))
        try:
            p = subprocess.run([self.exe, self.db, one], capture_output=True, timeout=60)
        except subprocess.TimeoutExpired:
            return False, ""
        out = p.stdout.decode(errors="replace").replace("\r", "")
        errs = [l for l in out.split("\n") if l.startswith("Error")]
        ok = p.returncode == 0 and not errs
        if not ok:
            why = errs[0] if errs else "exit %d" % p.returncode
            why = re.sub(r"'[^']*'", "'_'", why)
            self.last_error = re.sub(r"[0-9]+", "N", why)
        return ok, out

    def statement(self, sql):
        return self.run(sql)[0]

    def query(self, sql, ncols):
        ok, out = self.run(sql)
        if not ok:
            return None
        lines = out.split("\n")
        if len(lines) < 2:
            return None
        rows = []
        for line in lines[2:]:
            if line == "":
                continue
            cells = [c.strip() for c in line.split(" | ")]
            rows.append(tuple(None if c == "" else c for c in cells))
        return rows


def run_file(path, engine, stats, first_pass):
    threshold = 0
    text = open(path, encoding="utf-8", errors="replace").read()
    for kind, words, sql, expected, conds, lineno in parse_records(text):
        if kind == "hash-threshold":
            threshold = int(words[1])
            continue
        if kind == "halt":
            if applies(conds):
                break
            continue
        if not applies(conds):
            stats["skipped"] += 1
            continue
        if kind == "statement":
            want_ok = words[1] == "ok"
            # A record may hold several statements, one per line ending in ';'.
            parts = sql.split(SEMI_NL) if SEMI_NL in sql else [sql]
            got = all(engine.statement(q.strip().rstrip(";")) for q in parts if q.strip())
            stats["stmt_ok" if got == want_ok else "stmt_bad"] += 1
        elif kind == "query":
            types, sortmode = words[1], (words[2] if len(words) > 2 else "nosort")
            rows = engine.query(sql, len(types))
            if rows is None:
                stats["rejected"] += 1
                why = getattr(engine, "last_error", "error")
                stats["reasons"][why] = stats["reasons"].get(why, 0) + 1
            elif score_values(rows, types, sortmode, expected, threshold):
                stats["pass"] += 1
                if first_pass is not None and not first_pass:
                    first_pass.append((rows, types, sortmode, expected, threshold, path, lineno))
            else:
                stats["wrong"] += 1
                if len(stats["wrong_examples"]) < 8:
                    stats["wrong_examples"].append("%s:%d %s" % (os.path.basename(path), lineno, sql[:120]))


def new_stats():
    return {"pass": 0, "wrong": 0, "rejected": 0, "stmt_ok": 0, "stmt_bad": 0,
            "skipped": 0, "wrong_examples": [], "reasons": {}}


def planted_check(fp):
    """Alter one value of a passing answer; the scorer must now reject it."""
    rows, types, sortmode, expected, threshold, path, lineno = fp
    if not rows or not rows[0]:
        return None
    bad = [list(r) for r in rows]
    v = bad[0][0]
    bad[0][0] = "7777777" if v != "7777777" else "1"
    caught = not score_values([tuple(r) for r in bad], types, sortmode, expected, threshold)
    return {"record": "%s:%d" % (os.path.basename(path), lineno), "caught": caught}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("corpus")
    ap.add_argument("--qdb")
    ap.add_argument("--engine", choices=["qdb", "sqlite"], default="qdb")
    ap.add_argument("--files", default=",".join(DEFAULT_FILES))
    ap.add_argument("--json")
    a = ap.parse_args()
    if a.engine == "qdb" and not (a.qdb and os.path.isfile(a.qdb)):
        print("SKIP: qdb binary not given or not found")
        return 0
    files = a.files.split(",")
    per_file = {}
    total = new_stats()
    first_pass = []
    t0 = time.time()
    for f in files:
        path = os.path.join(a.corpus, f)
        st = new_stats()
        wd = tempfile.mkdtemp(prefix="slt_")
        try:
            engine = SqliteEngine() if a.engine == "sqlite" else QdbEngine(a.qdb, wd)
            run_file(path, engine, st, first_pass)
        finally:
            shutil.rmtree(wd, ignore_errors=True)
        per_file[f] = st
        for k in ("pass", "wrong", "rejected", "stmt_ok", "stmt_bad", "skipped"):
            total[k] += st[k]
        for k, v in st["reasons"].items():
            total["reasons"][k] = total["reasons"].get(k, 0) + v
        nq = st["pass"] + st["wrong"] + st["rejected"]
        print("%-40s queries %5d pass %5d wrong %5d rejected %5d | statements ok %4d bad %4d"
              % (f, nq, st["pass"], st["wrong"], st["rejected"], st["stmt_ok"], st["stmt_bad"]))
        sys.stdout.flush()
    nq = total["pass"] + total["wrong"] + total["rejected"]
    print("TOTAL %s: %d of %d query records pass (%.2f%%), %d wrong, %d rejected; "
          "statements %d of %d as expected; %d records skipped by conditionals; %.0f s"
          % (a.engine, total["pass"], nq, 100.0 * total["pass"] / max(1, nq), total["wrong"],
             total["rejected"], total["stmt_ok"], total["stmt_ok"] + total["stmt_bad"],
             total["skipped"], time.time() - t0))
    for why, cnt in sorted(total["reasons"].items(), key=lambda kv: -kv[1])[:12]:
        print("  rejected %5d  %s" % (cnt, why))
    plant = planted_check(first_pass[0]) if first_pass else None
    print("planted check: %s" % (plant if plant else "no passing query to plant on"))
    if a.json:
        with open(a.json, "w") as f:
            json.dump({"engine": a.engine, "files": per_file, "total": total, "planted": plant}, f, indent=1)
    if plant is not None and not plant["caught"]:
        return 2
    if a.engine == "sqlite" and total["pass"] != nq:
        return 3
    return 0


if __name__ == "__main__":
    sys.exit(main())
