#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Receipts and AS OF: a second process reproduces every result (P16).

For each seed, the P2 workload of test_qdb_sqlite_oracle.py (two tables,
200 SELECTs) runs in one REPL (process A) with QDB_HISTORY=1 and
QDB_RECEIPTS=1, with a seeded mutation (UPDATE, DELETE or INSERT on a or b)
after every 10 queries, so the queries see 21 different states. Each SELECT
prints its result and then

  RECEIPT <txid> <state sha256> <query sha256> <result sha256>

Process B starts after A has exited and runs every query as
"<query> AS OF <txid from A's receipt>", which rebuilds that state from the
history. A query is reproduced when B prints the same result lines and the
same receipt.

Overhead: the same queries against each seed's final state, one REPL per
seed, receipts off and on, median of --reps runs.

Planted checks: (a) one of A's receipts altered must count as not
reproduced; (b) one UPDATE in seed 1's history gets another value and a
recomputed hash (a history rewritten consistently): B must then fail to
reproduce the receipts that read the changed row, and the count is reported.

  python test_qdb_receipts.py QDB [--seeds 1,2,3,4,5] [--reps 5] [--json OUT]
"""
import argparse
import json
import os
import random
import re
import shutil
import statistics
import subprocess
import sys
import tempfile
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import test_qdb_sqlite_oracle as p2  # noqa: E402

NAMES = ["ann", "bob", "cy", "dee", "ed", "flo"]


def mutation(rnd, next_a, next_b):
    k = rnd.randint(0, 4)
    if k == 0:
        return "UPDATE a SET score = %.1f WHERE id = %d" % (rnd.uniform(-10, 90), rnd.randint(1, 40))
    if k == 1:
        return "UPDATE a SET grp = %d WHERE id = %d" % (rnd.randint(1, 5), rnd.randint(1, 40))
    if k == 2:
        return "DELETE FROM b WHERE id = %d" % rnd.randint(1, 20)
    if k == 3:
        return "INSERT INTO a VALUES (%d, '%s', %.1f, %d)" % (next_a, rnd.choice(NAMES),
                                                            rnd.uniform(-10, 90), rnd.randint(1, 5))
    return "INSERT INTO b VALUES (%d, %d, '%s')" % (next_b, rnd.randint(1, 45), rnd.choice("xyz"))


def workload(seed):
    setup, qs = p2.gen(seed)
    rnd = random.Random(1000 + seed)
    script, queries = list(setup), []
    next_a, next_b = 41, 21
    for i, (q, _ordered) in enumerate(qs):
        if i > 0 and i % 10 == 0:
            m = mutation(rnd, next_a, next_b)
            next_a += m.startswith("INSERT INTO a")
            next_b += m.startswith("INSERT INTO b")
            script.append(m)
        script.append(q)
        queries.append(q)
    return script, queries


def repl(qdb, db, stmts, env_extra):
    env = dict(os.environ)
    for k in ("QDB_HISTORY", "QDB_RECEIPTS", "QDB_CRASH_AT", "QDB_TRACE_WRITES"):
        env.pop(k, None)
    env.update(env_extra)
    t0 = time.perf_counter()
    p = subprocess.run([qdb, db], input=("\n".join(stmts) + "\n.quit\n").encode(),
                       capture_output=True, env=env, timeout=900)
    dt = time.perf_counter() - t0
    out = p.stdout.decode(errors="replace").replace("\r", "")
    return p.returncode, out.split("qdb> ")[1:], dt


def split_result(chunk):
    lines = [l for l in chunk.split("\n") if l != ""]
    rec = [l for l in lines if l.startswith("RECEIPT ")]
    body = [l for l in lines if not l.startswith("RECEIPT ")]
    return body, (rec[-1] if rec else None)


def fnv(b):
    h = 2166136261
    for c in b:
        h = ((h ^ c) * 16777619) % 4294967296
    return "%08x" % h


def plant_history(hist_path):
    """Change the value of the first 'UPDATE a SET score' record and re-hash it."""
    lines = open(hist_path, "rb").read().decode().split("\n")
    for i, l in enumerate(lines):
        m = re.match(r"OP UPDATE a SET score = (-?[0-9.]+) WHERE id = (\d+)$", l)
        if m and i > 0 and lines[i - 1].startswith("TXID ") and lines[i + 1].startswith("COMMIT "):
            lines[i] = "OP UPDATE a SET score = %.1f WHERE id = %s" % (float(m.group(1)) + 1.0, m.group(2))
            body = (lines[i - 1] + "\n" + lines[i] + "\n").encode()
            lines[i + 1] = "COMMIT " + fnv(body)
            open(hist_path, "wb").write("\n".join(lines).encode())
            return int(lines[i - 1][5:]), m.group(2)
    return None, None


def run_b(qdb, db, queries, receipts):
    stmts = ["%s AS OF %s" % (q, r.split(" ")[1]) for q, r in zip(queries, receipts)]
    rc, chunks, dt = repl(qdb, db, stmts, {"QDB_RECEIPTS": "1"})
    return [split_result(c) for c in chunks[:len(queries)]], dt


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("qdb")
    ap.add_argument("--seeds", default="1,2,3,4,5")
    ap.add_argument("--reps", type=int, default=5)
    ap.add_argument("--json")
    a = ap.parse_args()
    qdb = os.path.abspath(a.qdb)
    if not os.path.isfile(qdb):
        print("SKIP: %s not found" % qdb)
        return 0
    seeds = [int(x) for x in a.seeds.split(",")]
    total = reproduced = 0
    time_a = time_b = 0.0
    off_t, on_t = [], []
    planted_a = planted_b = None
    fails = []
    with tempfile.TemporaryDirectory() as wd:
        for seed in seeds:
            db = os.path.join(wd, "s%d.db" % seed)
            script, queries = workload(seed)
            rc, chunks, dt = repl(qdb, db, script, {"QDB_HISTORY": "1", "QDB_RECEIPTS": "1"})
            time_a += dt
            res_a = [split_result(c) for c in chunks if "RECEIPT " in c]
            if rc != 0 or len(res_a) != len(queries):
                print("FAIL seed %d: process A exit %d, %d receipts for %d queries" % (seed, rc, len(res_a), len(queries)))
                return 1
            receipts = [r for _, r in res_a]
            res_b, dt = run_b(qdb, db, queries, receipts)
            time_b += dt
            for i, (ra, rb) in enumerate(zip(res_a, res_b)):
                total += 1
                if ra == rb:
                    reproduced += 1
                elif len(fails) < 5:
                    fails.append((seed, i, queries[i], ra, rb))
            if seed == seeds[0]:
                # (a) one altered receipt must not reproduce.
                alt = list(res_a)
                body, r = alt[0]
                alt[0] = (body, r[:-1] + ("0" if r[-1] != "0" else "1"))
                planted_a = sum(1 for x, y in zip(alt, res_b) if x != y)
                # (b) a history rewritten consistently.
                hp = db + ".hist"
                keep = hp + ".keep"
                shutil.copyfile(hp, keep)
                txid, row = plant_history(hp)
                res_p, _ = run_b(qdb, db, queries, receipts)
                diff = [i for i, (x, y) in enumerate(zip(res_a, res_p)) if x != y]
                after = [i for i, (_, r) in enumerate(res_a) if int(r.split(" ")[1]) >= (txid or 0)]
                planted_b = {"txid": txid, "row": row, "not_reproduced": len(diff),
                             "at_or_after": len(after),
                             "any_before": any(int(res_a[i][1].split(" ")[1]) < txid for i in diff)}
                shutil.copyfile(keep, hp)
            # Overhead on the final state.
            for rep in range(a.reps):
                for on in ((False, True) if rep % 2 == 0 else (True, False)):
                    env = {"QDB_RECEIPTS": "1"} if on else {}
                    _, _, dt = repl(qdb, db, queries, env)
                    (on_t if on else off_t).append((seed, dt))
    for f in fails:
        seed, i, q, ra, rb = f
        print("NOT REPRODUCED seed %d query %d: %s" % (seed, i, q))
        print("   A: %s" % (ra[1],))
        print("   B: %s" % (rb[1],))
    per_seed = []
    for seed in seeds:
        off = statistics.median(dt for s, dt in off_t if s == seed)
        on = statistics.median(dt for s, dt in on_t if s == seed)
        per_seed.append((seed, off, on))
    off_sum = sum(o for _, o, _ in per_seed)
    on_sum = sum(n for _, _, n in per_seed)
    overhead = (on_sum - off_sum) / off_sum * 100.0
    print("RECEIPTS: %d of %d results reproduced by a second process (AS OF)" % (reproduced, total))
    print("time: process A %.2f s (history and receipts on, with mutations); process B %.2f s (AS OF)" % (time_a, time_b))
    for seed, off, on in per_seed:
        print("  seed %d: %d queries, receipts off %.3f s, on %.3f s (median of %d)" % (seed, len(workload(seed)[1]), off, on, a.reps))
    print("overhead of receipts: %.1f%% (%.3f s off, %.3f s on)" % (overhead, off_sum, on_sum))
    print("planted (a) one altered receipt: %s" % ("caught" if planted_a == 1 else "MISSED (%s)" % planted_a))
    pb = planted_b or {}
    caught_b = pb.get("not_reproduced", 0) > 0 and not pb.get("any_before")
    print("planted (b) history rewritten at txid %s (row %s): %d of %d later receipts not reproduced, none before: %s"
          % (pb.get("txid"), pb.get("row"), pb.get("not_reproduced", 0), pb.get("at_or_after", 0),
             "caught" if caught_b else "MISSED"))
    if a.json:
        with open(a.json, "w") as f:
            json.dump({"total": total, "reproduced": reproduced, "overhead_pct": overhead,
                       "time_a": time_a, "time_b": time_b, "per_seed": per_seed,
                       "planted_a": planted_a, "planted_b": planted_b}, f, indent=1)
    ok = reproduced == total and planted_a == 1 and caught_b
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
