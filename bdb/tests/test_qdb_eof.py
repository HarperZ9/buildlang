#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""End of input ends the qdb REPL.

Before this test a script with no `.quit` never ended: read_line returns ""
at end of input, the REPL took it for a blank line and asked again, forever.
Now end of input leaves the REPL the way `.quit` does (the WAL is folded into
the .db; inside an open BEGIN the WAL gets HOLD and the next process resumes it).

Each case feeds a script on stdin and waits at most --timeout seconds. A run
that does not end is killed and counted as a failure. On POSIX the child also
runs under an address-space limit, so a regression cannot fill the machine.

Cases:
  no_quit        statements, no .quit: exit 0, and stdout and the .db file are
                 byte-equal to the same script with .quit
  no_newline     the last statement has no line feed: it still runs
  empty          empty stdin: exit 0, the prompt once
  blank_lines    blank lines inside the script do not end the session
  open_begin     input ends inside BEGIN: exit 0; the next process resumes the
                 transaction (its row is visible, ROLLBACK removes it), byte-equal
                 to what .quit inside BEGIN leaves

  python test_qdb_eof.py QDB [--timeout 10]
"""
import argparse
import os
import subprocess
import sys
import tempfile

LIMIT_BYTES = 1 << 30
NL = bytes([10])


def limit():
    try:
        import resource
        resource.setrlimit(resource.RLIMIT_AS, (LIMIT_BYTES, LIMIT_BYTES))
    except Exception:
        pass


def run(qdb, db, script, timeout):
    """(returncode, stdout) or (None, b"") when the run had to be killed."""
    kw = {} if os.name == "nt" else {"preexec_fn": limit}
    p = subprocess.Popen([qdb, db], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                         stderr=subprocess.PIPE, **kw)
    try:
        out, _ = p.communicate(script, timeout=timeout)
    except subprocess.TimeoutExpired:
        p.kill()
        p.communicate()
        return None, b""
    return p.returncode, out.replace(b"\r\n", b"\n")


def has(out, row):
    """A result row such as b"1|a", whatever the column padding."""
    return row in out.replace(b" ", b"")


def read(path):
    with open(path, "rb") as f:
        return f.read()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("qdb")
    ap.add_argument("--timeout", type=float, default=10.0)
    a = ap.parse_args()
    qdb = os.path.abspath(a.qdb)
    fails = []
    checks = 0

    def check(name, ok, detail=""):
        nonlocal checks
        checks += 1
        if not ok:
            fails.append(name)
        print("%-34s %s%s" % (name, "ok" if ok else "FAIL", (" " + detail) if detail and not ok else ""))

    body = (b"CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT);\n"
            b"INSERT INTO t VALUES (1, 'a');\n"
            b"INSERT INTO t VALUES (2, 'b');\n"
            b"SELECT id, v FROM t ORDER BY id;\n")

    with tempfile.TemporaryDirectory() as d:
        ref_db = os.path.join(d, "ref.db")
        rc_ref, out_ref = run(qdb, ref_db, body + b".quit\n", a.timeout)
        check("reference with .quit exits 0", rc_ref == 0, "rc %r" % rc_ref)

        db = os.path.join(d, "noquit.db")
        rc, out = run(qdb, db, body, a.timeout)
        check("no_quit ends", rc is not None, "killed after %.0f s" % a.timeout)
        check("no_quit exit 0", rc == 0, "rc %r" % rc)
        check("no_quit stdout equal", rc is not None and out == out_ref)
        check("no_quit .db equal", rc is not None and os.path.exists(db) and read(db) == read(ref_db))
        check("no_quit WAL folded", rc is not None and
              (not os.path.exists(db + ".wal") or os.path.getsize(db + ".wal") == 0))

        db = os.path.join(d, "nonl.db")
        rc, out = run(qdb, db, body[:-1], a.timeout)
        check("no_newline ends with 0", rc == 0, "rc %r" % rc)
        check("no_newline last statement ran", has(out, b"1|a") and has(out, b"2|b"))

        db = os.path.join(d, "empty.db")
        rc, out = run(qdb, db, b"", a.timeout)
        check("empty ends with 0", rc == 0, "rc %r" % rc)
        check("empty prompts once", out.count(b"qdb> ") == 1, "%d prompts" % out.count(b"qdb> "))

        db = os.path.join(d, "blank.db")
        blank = body.replace(b";\n", b";\n\n\r\n   \n")
        rc, out = run(qdb, db, blank, a.timeout)
        check("blank_lines ends with 0", rc == 0, "rc %r" % rc)
        check("blank_lines all statements ran", has(out, b"1|a") and has(out, b"2|b"))

        tx = body + b"BEGIN;\nINSERT INTO t VALUES (3, 'c');\n"
        db_q = os.path.join(d, "txq.db")
        db_e = os.path.join(d, "txe.db")
        rc_q, _ = run(qdb, db_q, tx + b".quit\n", a.timeout)
        rc_e, _ = run(qdb, db_e, tx, a.timeout)
        check("open_begin ends with 0", rc_e == 0 and rc_q == 0, "rc %r and %r" % (rc_e, rc_q))
        sel = b"SELECT id, v FROM t ORDER BY id;" + NL + b"ROLLBACK;" + NL + b"SELECT id, v FROM t ORDER BY id;" + NL + b".quit" + NL
        _, again_q = run(qdb, db_q, sel, a.timeout)
        _, again_e = run(qdb, db_e, sel, a.timeout)
        rows = again_e.replace(b" ", b"")
        check("open_begin resumed, row 3 visible", rows.count(b"3|c") == 1 and rows.count(b"2|b") == 2,
              "%d and %d" % (rows.count(b"3|c"), rows.count(b"2|b")))
        check("open_begin same as .quit", rc_e is not None and again_e == again_q)
        _, final = run(qdb, db_e, b"SELECT id, v FROM t ORDER BY id;" + NL + b".quit" + NL, a.timeout)
        check("open_begin rolled back for good", not has(final, b"3|c") and has(final, b"2|b"))

    print("%d checks, %d failures" % (checks, len(fails)))
    print("EOF PASS" if not fails else "EOF FAIL")
    return 0 if not fails else 1


if __name__ == "__main__":
    sys.exit(main())
