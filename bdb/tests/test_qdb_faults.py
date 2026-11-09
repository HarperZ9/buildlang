#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Seeded power cuts: drop, tear and reorder unflushed writes, then recover.

test_qdb_crash.py stops the process; the files keep every byte it wrote.
A power cut also loses what the operating system had not yet written back.
This harness simulates that cache, from a seed, with no change to qdb.

1. Learn. For each step of the crash workload, stop the step before each
   durable operation (QDB_CRASH_AT=k) and compare the files: each operation
   is a whole-file write, an append at an offset, a flush of one file
   (tags ending in _sync) or a rename (db_rename, .tmp over the .db).
2. Cut. A seed (splitmix64, the same stream on every platform) picks a step,
   a cut point c in 0..n, and for each issued write that no flush of its
   file follows before the cut: kept or dropped, and a kept write may be
   torn (a seeded prefix lands). Kept writes land in issue order at their
   offsets; a hole left by a dropped earlier append is zero-filled, which
   is the image of a later write reaching the device first. Flushed writes
   and completed renames always land. The step starts from a fully
   written-back image of the state before it.
3. Recover. One qdb process opens the image, issues ROLLBACK, reads t and u
   and quits. Rule: exit 0, and the state equals the state after the step
   if a commit write (wal_auto, wal_commit) was followed by a wal_sync
   before the cut, else the state before or after it.
4. Hash. sha256 of each file left after recovery, per seed, and one digest
   over all seeds. --manifest writes them; --compare checks another
   platform's manifest (planted: one altered hash must be reported).

Planted check: --plant-ignore-flush lets flushed writes drop too; the
harness must then report violations, or it cannot see a lost commit.

  python test_qdb_faults.py QDB [--seeds 10000] [--manifest OUT]
         [--compare OTHER] [--expect-digest HEX] [--jobs 2] [--json OUT]
"""
import argparse
import concurrent.futures
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import test_qdb_crash as crash  # noqa: E402

SUFFIXES = ("", ".wal", ".tmp", ".ckpt")
COMMIT_TAGS = ("wal_auto", "wal_commit")
WRITE_TAGS = ("db_tmp", "wal_clear", "ckpt_clear")  # durable_write(..., append=false)
TAG_PATH = {"db_tmp": ".tmp", "db_tmp_sync": ".tmp", "wal_sync": ".wal",
            "ckpt_clear": ".ckpt"}
MASK = (1 << 64) - 1


class SplitMix64:
    def __init__(self, seed):
        self.s = seed & MASK

    def next(self):
        self.s = (self.s + 0x9E3779B97F4A7C15) & MASK
        z = self.s
        z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & MASK
        z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & MASK
        return z ^ (z >> 31)

    def below(self, n):
        return self.next() % n if n > 0 else 0


def image(db):
    return {s: (open(db + s, "rb").read() if os.path.exists(db + s) else None) for s in SUFFIXES}


def put(db, img):
    for s in SUFFIXES:
        p = db + s
        if os.path.exists(p):
            os.remove(p)
        if img.get(s) is not None:
            with open(p, "wb") as f:
                f.write(img[s])


def run(qdb, db, step, env=None):
    return crash.run_step(qdb, db, step, env)


def learn_step(qdb, pre_img, step, wd):
    """The ordered operations of one step, from images before each one."""
    work = os.path.join(wd, "learn.db")
    put(work, pre_img)
    p = run(qdb, work, step, {"QDB_TRACE_WRITES": "1"})
    tags = []
    for line in p.stderr.decode(errors="replace").replace("\r", "").split("\n"):
        if line.startswith("QDB_WRITE "):
            tags.append(line.split(" ")[2])
    imgs = []
    for k in range(1, len(tags) + 1):
        put(work, pre_img)
        r = run(qdb, work, step, {"QDB_CRASH_AT": str(k)})
        if r.returncode != 77:
            raise RuntimeError("step did not stop at write %d (exit %d)" % (k, r.returncode))
        imgs.append(image(work))
    put(work, pre_img)
    run(qdb, work, step)
    imgs.append(image(work))
    ops = []
    for k, tag in enumerate(tags):
        before, after = imgs[k], imgs[k + 1]
        if tag.endswith("_sync"):
            ops.append({"kind": "flush", "path": TAG_PATH[tag], "tag": tag})
        elif tag == "db_rename":
            ops.append({"kind": "rename", "src": ".tmp", "dst": "", "tag": tag})
        elif tag in WRITE_TAGS:
            path = ".wal" if tag == "wal_clear" else TAG_PATH[tag]
            ops.append({"kind": "write", "path": path, "off": 0, "data": after[path] or b"",
                        "tag": tag})
        else:
            old = before[".wal"] or b""
            new = after[".wal"] or b""
            if not new.startswith(old):
                raise RuntimeError("append %s did not extend the WAL" % tag)
            ops.append({"kind": "append", "path": ".wal", "off": len(old),
                        "data": new[len(old):], "tag": tag})
    return ops, imgs[-1]


def covered(ops, i, c):
    path = ops[i]["path"]
    for j in range(i + 1, c):
        if ops[j]["kind"] == "flush" and ops[j]["path"] == path:
            return True
        if ops[j]["kind"] == "rename" and ops[j]["src"] == path:
            path = ops[j]["dst"]
    return False


def cut_image(pre_img, ops, c, rng, ignore_flush):
    """The device image after a power cut once operations 0..c-1 were issued."""
    img = {s: (None if v is None else bytearray(v)) for s, v in pre_img.items()}
    for i in range(c):
        op = ops[i]
        keep_draw, torn_draw, cut_draw = rng.next(), rng.next(), rng.next()
        if op["kind"] == "flush":
            continue
        if op["kind"] == "rename":
            img[op["dst"]] = img[op["src"]] if img[op["src"]] is not None else bytearray()
            img[op["src"]] = None
            continue
        data = op["data"]
        if ignore_flush or not covered(ops, i, c):
            if keep_draw % 2 == 0:
                continue  # dropped
            if torn_draw % 4 == 0 and data:
                data = data[:cut_draw % len(data)]
        path = op["path"]
        if op["kind"] == "write":
            img[path] = bytearray(data)
            continue
        cur = img[path] if img[path] is not None else bytearray()
        end = op["off"] + len(data)
        if len(cur) < op["off"]:
            cur.extend(b"\0" * (op["off"] - len(cur)))
        if len(cur) < end:
            cur.extend(b"\0" * (end - len(cur)))
        cur[op["off"]:end] = data
        img[path] = cur
    return {s: (None if v is None else bytes(v)) for s, v in img.items()}


def acknowledged(ops, c):
    for i in range(c):
        if ops[i]["tag"] in COMMIT_TAGS:
            for j in range(i + 1, c):
                if ops[j]["tag"] == "wal_sync":
                    return True
    return False


def recover(qdb, db):
    script = "ROLLBACK\nSELECT id, v FROM t ORDER BY id\nSELECT k FROM u ORDER BY k\n.quit\n"
    p = subprocess.run([qdb, db], input=script.encode(), capture_output=True, timeout=120)
    out = p.stdout.decode(errors="replace").replace("\r", "").replace("qdb> ", "")
    lines = [l.rstrip() for l in out.split("\n")
             if l.strip() and not l.startswith("Error: no transaction")]
    return p.returncode, "\n".join(lines)


def files_hash(db):
    h = hashlib.sha256()
    for s in SUFFIXES:
        p = db + s
        d = hashlib.sha256(open(p, "rb").read()).hexdigest() if os.path.exists(p) else "-"
        h.update(("%s:%s\n" % (s or ".db", d)).encode())
    return h.hexdigest()


def one_seed(qdb, steps, learned, states, seed, wd, ignore_flush):
    rng = SplitMix64(seed)
    si = rng.below(len(steps))
    pre_img, ops = learned[si]
    c = rng.below(len(ops) + 1)
    img = cut_image(pre_img, ops, c, rng, ignore_flush)
    db = os.path.join(wd, "s%d.db" % seed)
    put(db, img)
    code, got = recover(qdb, db)
    pre, post = states[si], states[si + 1]
    if acknowledged(ops, c):
        ok = got == post
    else:
        ok = got in (pre, post)
    ok = ok and code == 0
    h = files_hash(db)
    for s in SUFFIXES:
        if os.path.exists(db + s):
            os.remove(db + s)
    return {"seed": seed, "step": si, "cut": c, "ops": len(ops), "exit": code, "ok": ok,
            "hash": h, "got": None if ok else got}


def compare(mine, other_path):
    other = {}
    for line in open(other_path):
        if line.strip():
            s, h = line.split()
            other[int(s)] = h
    diff = [s for s in mine if other.get(s) != mine[s]]
    return diff, len(other)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("qdb")
    ap.add_argument("--seeds", type=int, default=10000)
    ap.add_argument("--first", type=int, default=1)
    ap.add_argument("--jobs", type=int, default=2)
    ap.add_argument("--manifest")
    ap.add_argument("--compare")
    ap.add_argument("--expect-digest")
    ap.add_argument("--plant-ignore-flush", action="store_true")
    ap.add_argument("--json")
    a = ap.parse_args()
    qdb = os.path.abspath(a.qdb)
    if not os.path.isfile(qdb):
        print("SKIP: %s not found" % qdb)
        return 0
    steps = crash.workload()
    t0 = time.time()
    with tempfile.TemporaryDirectory() as wd:
        ref = os.path.join(wd, "ref.db")
        states = [crash.committed_state(qdb, ref, wd)]
        learned = []
        for step in steps:
            pre_img = image(ref)
            ops, post_img = learn_step(qdb, pre_img, step, wd)
            learned.append((pre_img, ops))
            put(ref, post_img)
            states.append(crash.committed_state(qdb, ref, wd))
        t1 = time.time()
        seeds = range(a.first, a.first + a.seeds)
        with concurrent.futures.ThreadPoolExecutor(max_workers=a.jobs) as ex:
            results = list(ex.map(lambda s: one_seed(qdb, steps, learned, states, s, wd,
                                                     a.plant_ignore_flush), seeds))
    t2 = time.time()
    viol = [r for r in results if not r["ok"]]
    for r in viol[:10]:
        print("VIOLATION seed %d step %d cut %d of %d exit %d: %s"
              % (r["seed"], r["step"], r["cut"], r["ops"], r["exit"], (r["got"] or "").replace("\n", " / ")))
    digest = hashlib.sha256("".join("%d %s\n" % (r["seed"], r["hash"]) for r in results).encode()).hexdigest()
    nops = sum(len(o) for _, o in learned)
    distinct = len(set(r["hash"] for r in results))
    print("learned %d operations over %d steps in %.1f s" % (nops, len(steps), t1 - t0))
    print("FAULTS: %d seeds, %d violations, %d distinct recovered images, %.1f seeds/s"
          % (len(results), len(viol), distinct, len(results) / max(t2 - t1, 1e-9)))
    print("digest %s" % digest)
    rc = 1 if viol else 0
    if a.plant_ignore_flush:
        print("planted (flushes ignored): %s" % ("caught" if viol else "MISSED"))
        rc = 0 if viol else 2
    if a.manifest:
        with open(a.manifest, "w", newline="\n") as f:
            for r in results:
                f.write("%d %s\n" % (r["seed"], r["hash"]))
    if a.compare:
        mine = {r["seed"]: r["hash"] for r in results}
        diff, n_other = compare(mine, a.compare)
        print("compare: %d of %d seeds equal to %s (%d seeds there)"
              % (len(mine) - len(diff), len(mine), a.compare, n_other))
        for s in diff[:5]:
            print("  differs: seed %d" % s)
        planted = dict(mine)
        k = next(iter(planted))
        planted[k] = "0" * 64
        pdiff = [s for s in planted if mine.get(s) != planted[s]]
        print("planted (one hash altered) reported: %s" % ("caught" if pdiff == [k] else "MISSED"))
        if diff:
            rc = 1
    if a.expect_digest:
        same = a.expect_digest == digest
        print("expected digest: %s" % ("equal" if same else "DIFFERS (expected %s)" % a.expect_digest))
        if not same:
            rc = 1
    if a.json:
        with open(a.json, "w") as f:
            json.dump({"seeds": len(results), "violations": len(viol), "digest": digest,
                       "distinct": distinct, "results": results}, f, indent=0)
    return rc


if __name__ == "__main__":
    sys.exit(main())
