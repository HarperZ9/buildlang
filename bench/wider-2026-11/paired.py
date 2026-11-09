"""Paired ratios of the fastest BuildLang variant against Rust and C, per result file.

The BuildLang variant is picked after the run, so its ratio is a best case.
"""
import json, random, statistics, sys
def boot(xs, n=10000, seed=1):
    rng = random.Random(seed)
    v = sorted(statistics.median([rng.choice(xs) for _ in xs]) for _ in range(n))
    return v[int(.025*n)], v[int(.975*n)-1]
for f in sys.argv[1:]:
    d = json.load(open(f))
    for b, r in d["benches"].items():
        t = {k: x["times_s"] for k, x in r["variants"].items()}
        best_bl = min(("bl_O2","bl_O3","bl_O3n"), key=lambda k: statistics.median(t[k]))
        for a, c in ((best_bl,"rust"),(best_bl,"rust_n"),(best_bl,"c_O3n")):
            q = [x/y for x, y in zip(t[a], t[c])]
            lo, hi = boot(q)
            print(f"{f:40s} {b:9s} {a}/{c:7s} {statistics.median(q):.3f} ({lo:.3f}-{hi:.3f})")
