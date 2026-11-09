#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Damage fuzzer for qdb's WAL (and, with --db-cases, its .db file).

A session commits 24 transactions and then exits the way a crash does, so the
WAL holds every record. Each case damages that WAL in one place (a mutation
drawn from a fixed seed), restores the .db, reopens qdb and reads the table.

What a damaged WAL may do (the invariants):
  I1  qdb ends inside the time limit with exit code 0 or 1. A crash, a hang or
      any other code is a failure.
  I2  every recovered row is byte-equal to a row the session committed, and no
      row appears twice. A changed or invented row is a silent wrong answer.
  I3  a transaction is recovered whole or not at all.
  I4  a transaction whose bytes the damage did not touch (with one byte of
      margin) is recovered.
I2 to I4 apply when qdb exits 0. An exit 1 is a clean refusal and is counted.

The .db file carries no checksum yet, so --db-cases only counts: crashes and
hangs, clean refusals, and answers that changed with no error. None of them
fails the run until the checksum lands (bdb/COMPLETE.md, fuzzing plan).

Every case is (seed, index); `--case N` replays one. A failing mutation is
shrunk (shorter span, fewer inserted bytes) while it still fails, and printed
as a REGRESSIONS entry. REGRESSIONS run first on every run.

Planted defect: --write-planted writes a db.bld whose replay accepts a COMMIT
record whatever its hash. With --planted BUILD the fuzzer must catch it.

  python fuzz_qdb_wal.py QDB [--cases 2000] [--db-cases 500] [--seed 20261109]
                             [--planted PLANTED_QDB] [--case N] [--keep DIR]
  python fuzz_qdb_wal.py --write-planted DB_BLD OUT_BLD
"""
import argparse
import os
import random
import subprocess
import sys
import tempfile

SEED = 20261109
TIMEOUT = 10.0
LIMIT_BYTES = 1 << 30
NL = bytes([10])

PLANT_OLD = 'let whole = wl == commit_line(txn_h) || (wl == "COMMIT" && (li + 1 < n || ends_nl));'
PLANT_NEW = 'let whole = true;'

# Minimized failures, kept as regression cases: (kind, start, removed, inserted hex).
# Found 2026-11-09 with seed 20261109 (cases 66, 579, 674, 1301, 1363): damage that leaves
# letters or a long digit run after "TXID " made the number parser overflow and qdb stop
# with exit 101. Replay now reads a transaction id with txid_text and skips a bad one.
REGRESSIONS = [
    ("printable", 2417, 1, ""),
    ("bitflip", 693, 1, ""),
    ("delete", 1556, 1, ""),
    ("zero", 2351, 1, ""),
    ("dup", 1772, 0, "31312c202776313178383731303927"),
]


def limit_posix():
    try:
        import resource
        resource.setrlimit(resource.RLIMIT_AS, (LIMIT_BYTES, LIMIT_BYTES))
    except Exception:
        pass


def limit_windows(proc):
    """Put the child in a job with a 1 GB process memory limit. Best effort."""
    try:
        import ctypes
        from ctypes import wintypes
        k = ctypes.WinDLL("kernel32", use_last_error=True)

        class BASIC(ctypes.Structure):
            _fields_ = [("PerProcessUserTimeLimit", ctypes.c_int64), ("PerJobUserTimeLimit", ctypes.c_int64),
                        ("LimitFlags", wintypes.DWORD), ("MinimumWorkingSetSize", ctypes.c_size_t),
                        ("MaximumWorkingSetSize", ctypes.c_size_t), ("ActiveProcessLimit", wintypes.DWORD),
                        ("Affinity", ctypes.c_size_t), ("PriorityClass", wintypes.DWORD),
                        ("SchedulingClass", wintypes.DWORD)]

        class IOC(ctypes.Structure):
            _fields_ = [(n, ctypes.c_uint64) for n in ("a", "b", "c", "d", "e", "f")]

        class EXT(ctypes.Structure):
            _fields_ = [("Basic", BASIC), ("Io", IOC), ("ProcessMemoryLimit", ctypes.c_size_t),
                        ("JobMemoryLimit", ctypes.c_size_t), ("PeakProcessMemoryUsed", ctypes.c_size_t),
                        ("PeakJobMemoryUsed", ctypes.c_size_t)]

        k.CreateJobObjectW.restype = wintypes.HANDLE
        k.CreateJobObjectW.argtypes = [ctypes.c_void_p, wintypes.LPCWSTR]
        k.SetInformationJobObject.argtypes = [wintypes.HANDLE, ctypes.c_int, ctypes.c_void_p, wintypes.DWORD]
        k.AssignProcessToJobObject.argtypes = [wintypes.HANDLE, wintypes.HANDLE]
        k.CloseHandle.argtypes = [wintypes.HANDLE]
        job = k.CreateJobObjectW(None, None)
        if not job:
            return None
        info = EXT()
        info.Basic.LimitFlags = 0x100  # JOB_OBJECT_LIMIT_PROCESS_MEMORY
        info.ProcessMemoryLimit = LIMIT_BYTES
        k.SetInformationJobObject(job, 9, ctypes.byref(info), ctypes.sizeof(info))
        k.AssignProcessToJobObject(job, wintypes.HANDLE(int(proc._handle)))
        return lambda: k.CloseHandle(job)
    except Exception:
        return None


def run(argv, stdin=None):
    """(returncode, stdout), or (None, b"") when the run had to be killed."""
    kw = {} if os.name == "nt" else {"preexec_fn": limit_posix}
    p = subprocess.Popen(argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, **kw)
    close = limit_windows(p) if os.name == "nt" else None
    try:
        out, _ = p.communicate(stdin, timeout=TIMEOUT)
    except subprocess.TimeoutExpired:
        p.kill()
        p.communicate()
        return None, b""
    finally:
        if close:
            close()
    return p.returncode, out.replace(b"\r\n", b"\n")


def value(i):
    return "v%dx%d" % (i, (i * 7919) % 100003)


def workload():
    """24 transactions over ids 1..48: single rows and blocks of three."""
    txns, stmts, i = [], [], 1
    for t in range(24):
        ids = [i] if t % 2 == 0 else [i, i + 1, i + 2]
        i += len(ids)
        txns.append(ids)
        ins = ["INSERT INTO t VALUES (%d, '%s');" % (k, value(k)) for k in ids]
        stmts += ins if len(ids) == 1 else ["BEGIN;"] + ins + ["COMMIT;"]
    return txns, stmts


def build(qdb, d):
    """Returns (db bytes, wal bytes, txns, record ranges in the WAL)."""
    db = os.path.join(d, "src.db")
    rc, _ = run([qdb, db], b"CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT);\n.quit\n")
    if rc != 0:
        raise SystemExit("setup: CREATE TABLE session gave %r" % rc)
    with open(db, "rb") as f:
        db_bytes = f.read()
    txns, stmts = workload()
    rc, _ = run([qdb, db], ("\n".join(stmts) + "\nNOT_A_STATEMENT\n").encode())
    if rc != 1:
        raise SystemExit("setup: expected exit 1 from the rejected statement, got %r" % rc)
    with open(db + ".wal", "rb") as f:
        wal = f.read()
    ranges, pos, start = [], 0, None
    for line in wal.split(NL):
        if line.startswith(b"TXID "):
            start = pos
        pos += len(line) + 1
        if line.startswith(b"COMMIT") and start is not None:
            ranges.append((start, pos))
            start = None
    if len(ranges) != len(txns):
        raise SystemExit("setup: %d WAL records for %d transactions" % (len(ranges), len(txns)))
    return db_bytes, wal, txns, ranges


def mutate(rng, data):
    """One mutation: (kind, start, removed length, inserted bytes)."""
    n = len(data)
    kind = rng.choice(["bitflip", "byte", "printable", "delete", "insert", "dup", "zero", "swap_lines", "truncate"])
    s = rng.randrange(n)
    if kind == "bitflip":
        return kind, s, 1, bytes([data[s] ^ (1 << rng.randrange(8))])
    if kind == "byte":
        b = rng.randrange(256)
        return kind, s, 1, bytes([b if b != data[s] else b ^ 1])
    if kind == "printable":
        b = rng.randrange(32, 127)
        return kind, s, 1, bytes([b if b != data[s] else 32 + (b - 31) % 95])
    if kind == "delete":
        return kind, s, min(rng.randrange(1, 41), n - s), b""
    if kind == "insert":
        return kind, s, 0, bytes(rng.randrange(256) for _ in range(rng.randrange(1, 9)))
    if kind == "dup":
        a = rng.randrange(n)
        return kind, s, 0, data[a:a + rng.randrange(1, 201)]
    if kind == "zero":
        ln = min(rng.randrange(1, 17), n - s)
        return kind, s, ln, bytes(ln)
    if kind == "swap_lines":
        lines = data.split(NL)
        a, b = rng.randrange(len(lines) - 1), rng.randrange(len(lines) - 1)
        if a == b or lines[a] == lines[b]:
            b = (a + 1) % (len(lines) - 1)
        lo, hi = min(a, b), max(a, b)
        s = sum(len(x) + 1 for x in lines[:lo])
        e = sum(len(x) + 1 for x in lines[:hi + 1])
        mid = lines[lo + 1:hi]
        new = NL.join([lines[hi]] + mid + [lines[lo]]) + NL
        return kind, s, e - s, new
    return "truncate", s, n - s, b""


def apply(data, m):
    _, s, removed, ins = m
    return data[:s] + ins + data[s + removed:]


def recover(qdb, db, db_bytes, wal_bytes):
    for p in (db, db + ".wal", db + ".tmp"):
        if os.path.exists(p):
            os.remove(p)
    with open(db, "wb") as f:
        f.write(db_bytes)
    with open(db + ".wal", "wb") as f:
        f.write(wal_bytes)
    return run([qdb, db, "SELECT id, v FROM t ORDER BY id"])


def rows_of(out):
    """Result rows as (id text, value text), or None when the table is gone."""
    text = out.decode(errors="replace")
    if "not found" in text:
        return None
    rows = []
    lines = text.split("\n")
    # Replay may print a line first (a duplicated record is refused by its
    # PRIMARY KEY, with a message); the rows start after the rule line.
    rule = [i for i, line in enumerate(lines) if line.startswith("---")]
    for line in lines[rule[0] + 1:] if rule else []:
        if "|" in line:
            a, b = line.split("|", 1)
            rows.append((a.strip(), b.strip()))
    return rows


def check_wal(qdb, db, db_bytes, wal, txns, ranges, m):
    """'' when the invariants hold, else the name of the first one broken."""
    rc, out = recover(qdb, db, db_bytes, apply(wal, m))
    if rc is None:
        return "I1 hang", rc
    if rc not in (0, 1):
        return "I1 exit code %d" % rc, rc
    if rc == 1:
        return "", rc
    rows = rows_of(out)
    if rows is None:
        return "I2 table lost", rc
    want = {str(i): value(i) for ids in txns for i in ids}
    seen = set()
    for a, b in rows:
        if want.get(a) != b:
            return "I2 row %s|%s was never committed" % (a, b), rc
        if a in seen:
            return "I2 row %s twice" % a, rc
        seen.add(a)
    _, s, removed, _ = m
    e = s + removed
    for ids, (a, b) in zip(txns, ranges):
        got = [str(i) in seen for i in ids]
        if any(got) and not all(got):
            return "I3 transaction of row %d recovered in part" % ids[0], rc
        if (e < a or s > b) and not all(got):
            return "I4 untouched transaction of row %d lost" % ids[0], rc
    return "", rc


def shrink(fails_fn, m):
    """Shorter spans while the case still fails."""
    kind, s, removed, ins = m
    changed = True
    while changed:
        changed = False
        for cand in ((kind, s, removed // 2, ins[:removed // 2] if len(ins) == removed else ins),
                     (kind, s, removed, ins[:len(ins) // 2])):
            if cand != (kind, s, removed, ins) and (cand[2] or cand[3]) and fails_fn(cand):
                kind, s, removed, ins = cand
                changed = True
                break
    return kind, s, removed, ins


def fuzz_wal(qdb, d, cases, seed, only, keep, quiet=False):
    db_bytes, wal, txns, ranges = build(qdb, d)
    db = os.path.join(d, "work.db")
    rc, out = recover(qdb, db, db_bytes, wal)
    rows = rows_of(out) or []
    if rc != 0 or len(rows) != sum(len(t) for t in txns):
        raise SystemExit("setup: the undamaged WAL recovered %d rows, exit %r" % (len(rows), rc))
    fails, refused, by_kind = [], 0, {}
    todo = [("regression %d" % i, (k, s, r, bytes.fromhex(h))) for i, (k, s, r, h) in enumerate(REGRESSIONS)]
    for i in ([only] if only is not None else range(cases)):
        todo.append(("case %d" % i, mutate(random.Random("%d:%d" % (seed, i)), wal)))
    for name, m in todo:
        why, rc = check_wal(qdb, db, db_bytes, wal, txns, ranges, m)
        refused += rc == 1
        if not why:
            continue
        by_kind[m[0]] = by_kind.get(m[0], 0) + 1
        fails.append((name, m, why))
        if len(fails) <= 5 and not quiet:
            small = shrink(lambda c: bool(check_wal(qdb, db, db_bytes, wal, txns, ranges, c)[0]), m)
            print("FAIL %s: %s" % (name, why))
            print("  mutation %s at byte %d, %d removed, %d inserted" % (m[0], m[1], m[2], len(m[3])))
            print("  shrunk, as a REGRESSIONS entry: %r" % ((small[0], small[1], small[2], small[3].hex()),))
            if keep:
                os.makedirs(keep, exist_ok=True)
                with open(os.path.join(keep, name.replace(" ", "_") + ".wal"), "wb") as f:
                    f.write(apply(wal, small))
                with open(os.path.join(keep, "src.db"), "wb") as f:
                    f.write(db_bytes)
    return len(todo), fails, refused, by_kind, len(wal)


def fuzz_db(qdb, d, cases, seed):
    """.db damage, counted only: (cases, crashes or hangs, refused, changed silently, same, size)."""
    db = os.path.join(d, "dsrc.db")
    _, stmts = workload()
    script = "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT);\n" + "\n".join(stmts) + "\n.quit\n"
    rc, _ = run([qdb, db], script.encode())
    if rc != 0:
        raise SystemExit("setup: .db session gave %r" % rc)
    with open(db, "rb") as f:
        good = f.read()
    work = os.path.join(d, "dwork.db")
    _, ref = recover(qdb, work, good, b"")
    bad, refused, silent, same = [], 0, 0, 0
    for i in range(cases):
        m = mutate(random.Random("%d:db:%d" % (seed, i)), good)
        rc, out = recover(qdb, work, apply(good, m), b"")
        if rc is None or rc not in (0, 1):
            bad.append((i, m, "hang" if rc is None else "exit code %d" % rc))
            if len(bad) <= 3:
                print("counted, .db case %d: %s; mutation %s at byte %d, %d removed, %d inserted"
                      % (i, bad[-1][2], m[0], m[1], m[2], len(m[3])))
        elif rc == 1:
            refused += 1
        elif out == ref:
            same += 1
        else:
            silent += 1
    return cases, bad, refused, silent, same, len(good)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("qdb", nargs="?")
    ap.add_argument("--cases", type=int, default=2000)
    ap.add_argument("--db-cases", type=int, default=500)
    ap.add_argument("--seed", type=int, default=SEED)
    ap.add_argument("--case", type=int)
    ap.add_argument("--keep")
    ap.add_argument("--planted")
    ap.add_argument("--write-planted", nargs=2, metavar=("DB_BLD", "OUT_BLD"))
    a = ap.parse_args()
    if a.write_planted:
        with open(a.write_planted[0], encoding="utf-8", newline="") as f:
            src = f.read()
        if src.count(PLANT_OLD) != 1:
            raise SystemExit("planted: the commit check was not found exactly once")
        with open(a.write_planted[1], "w", encoding="utf-8", newline="") as f:
            f.write(src.replace(PLANT_OLD, PLANT_NEW))
        print("planted source written: %s" % a.write_planted[1])
        return 0
    if not a.qdb:
        ap.error("QDB is required")
    ok = True
    with tempfile.TemporaryDirectory() as d:
        n, fails, refused, by_kind, wal_len = fuzz_wal(os.path.abspath(a.qdb), d, a.cases, a.seed, a.case, a.keep)
        print("WAL: seed %d, %d cases on a %d-byte WAL, %d failures, %d clean refusals (exit 1)%s"
              % (a.seed, n, wal_len, len(fails), refused,
                 "; by mutation: " + ", ".join("%s %d" % kv for kv in sorted(by_kind.items())) if by_kind else ""))
        ok = ok and not fails
        if a.db_cases and a.case is None:
            n, bad, refused, silent, same, db_len = fuzz_db(os.path.abspath(a.qdb), d, a.db_cases, a.seed)
            print(".db: seed %d, %d cases on a %d-byte file, %d crashes or hangs, %d clean refusals, "
                  "%d answers unchanged, %d answers changed with no error (no checksum yet; counted, not failed)"
                  % (a.seed, n, db_len, len(bad), refused, same, silent))
    if a.planted:
        with tempfile.TemporaryDirectory() as d:
            n, fails, _, _, _ = fuzz_wal(os.path.abspath(a.planted), d, a.cases, a.seed, None, None, quiet=True)
        caught = len(fails) > 0
        print("planted (replay accepts any COMMIT record): %d of %d cases fail, %s"
              % (len(fails), n, "caught" if caught else "MISSED"))
        ok = ok and caught
    print("FUZZ PASS" if ok else "FUZZ FAIL")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
