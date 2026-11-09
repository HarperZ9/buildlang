"""Speed comparison harness: BuildLang (buildc C output, gcc -O2 and -O3),
Rust (rustc -C opt-level=3) and C (gcc -O2), same machine.

Each round runs every variant once, in an order shuffled by a fixed seed.
Every run records wall time and the SHA-256 of stdout; all variants of one
benchmark must print the same bytes. Reported per variant: median wall time
and a percentile-bootstrap 95 % interval of the median (10,000 resamples,
seed 1), and the ratio of medians against the C -O2 reference with its own
bootstrap interval over paired rounds.

Usage: python run.py --runs 10 --out results.json BENCH:ARG ...
where each BENCH directory holds the executables named in VARIANTS.
"""

import argparse
import hashlib
import json
import random
import statistics
import subprocess
import sys
import time
from pathlib import Path

import psutil

VARIANTS = ["bl_O2", "bl_O3", "bl_O3n", "rust", "rust_n", "c_O2", "c_O3", "c_O3n"]


def run_once(exe, arg):
    t0 = time.perf_counter()
    r = subprocess.run([str(exe), arg], capture_output=True)
    dt = time.perf_counter() - t0
    if r.returncode != 0:
        raise SystemExit(f"{exe} exited {r.returncode}: {r.stderr[:300]!r}")
    return dt, hashlib.sha256(r.stdout.replace(b"\r\n", b"\n")).hexdigest()


def boot(xs, stat, n=10000, seed=1):
    rng = random.Random(seed)
    vals = sorted(stat([rng.choice(xs) for _ in xs]) for _ in range(n))
    return vals[int(0.025 * n)], vals[int(0.975 * n) - 1]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--runs", type=int, default=10)
    ap.add_argument("--out", required=True)
    ap.add_argument("benches", nargs="+")
    a = ap.parse_args()
    here = Path(__file__).resolve().parent
    report = {"runs": a.runs, "cpu_count": psutil.cpu_count(), "benches": {}}
    rng = random.Random(7)
    for spec in a.benches:
        name, arg = spec.split(":")
        d = here / name
        times = {v: [] for v in VARIANTS}
        hashes = {v: set() for v in VARIANTS}
        loads = []
        for v in VARIANTS:  # warm-up, not recorded
            run_once(d / f"{v}.exe", arg)
        for _ in range(a.runs):
            loads.append(psutil.cpu_percent(interval=0.5))
            order = VARIANTS[:]
            rng.shuffle(order)
            for v in order:
                dt, h = run_once(d / f"{v}.exe", arg)
                times[v].append(dt)
                hashes[v].add(h)
        all_hashes = set().union(*hashes.values())
        rows = {}
        ref = times["c_O2"]
        for v in VARIANTS:
            xs = times[v]
            ratios = [x / r for x, r in zip(xs, ref)]
            rows[v] = {
                "median_s": statistics.median(xs),
                "ci95_s": boot(xs, statistics.median),
                "ratio_vs_c_O2": statistics.median(ratios),
                "ratio_ci95": boot(ratios, statistics.median),
                "times_s": xs,
            }
        report["benches"][name] = {
            "arg": arg,
            "same_stdout": len(all_hashes) == 1,
            "cpu_percent_before_rounds": loads,
            "variants": rows,
        }
        print(name, arg, "same stdout" if len(all_hashes) == 1 else "STDOUT DIFFERS")
        for v in VARIANTS:
            r = rows[v]
            print(f"  {v:6s} median {r['median_s']:.3f} s  ci {r['ci95_s'][0]:.3f}-{r['ci95_s'][1]:.3f}"
                  f"  x c_O2 {r['ratio_vs_c_O2']:.3f} ({r['ratio_ci95'][0]:.3f}-{r['ratio_ci95'][1]:.3f})")
        sys.stdout.flush()
    Path(a.out).write_text(json.dumps(report, indent=2))


main()
