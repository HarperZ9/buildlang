"""M5 benchmark harness: serial loop vs par_for at 1 and N threads.

Builds each benchmark with `buildc build --release`, then runs rounds. Each
round runs the three modes once, in a shuffled order (fixed seed), at
BelowNormal priority on Windows. Every run records wall time, the kernel
milliseconds the program prints to stderr, the SHA-256 of stdout, and the
machine load (CPU percent over 0.5 s, GPU utilisation) before and after.

The bar metric is the kernel speed-up: per round, serial kernel ms divided by
par-N kernel ms; the median of the rounds and a percentile bootstrap 95 %
interval of that median (10,000 resamples, seed 1). Whole-process wall time is
reported beside it the same way. Byte identity: every run of every mode must
print the same stdout bytes.

Usage: python run_bench.py --buildc PATH [--runs 10] [--threads 8] [--out DIR]
"""

import argparse
import hashlib
import json
import os
import random
import re
import statistics
import subprocess
import sys
import time
from pathlib import Path

import psutil

HERE = Path(__file__).resolve().parent
BELOW_NORMAL = 0x00004000 if os.name == "nt" else 0

BENCHES = {
    "nbody": ["4096", "40"],
    "shade": ["3200", "2400"],
}


def gpu_util():
    try:
        out = subprocess.run(
            ["nvidia-smi", "--query-gpu=utilization.gpu", "--format=csv,noheader,nounits"],
            capture_output=True, text=True, timeout=10,
        ).stdout.strip()
        return int(out.splitlines()[0])
    except Exception as exc:  # no GPU tool: record the reason, not a number
        return f"unknown ({type(exc).__name__})"


def load():
    return {"cpu_percent": psutil.cpu_percent(interval=0.5), "gpu_percent": gpu_util()}


def build(buildc, name):
    d = HERE / name
    r = subprocess.run([buildc, "build", "--release", "--keep-c", "."], cwd=d,
                       capture_output=True, text=True)
    if r.returncode != 0:
        sys.exit(f"build {name} failed:\n{r.stdout}\n{r.stderr}")
    exe = d / "target" / "release" / ("main.exe" if os.name == "nt" else "main")
    return exe


def dir_digest(path):
    """SHA-256 over the sorted file names and bytes of a directory."""
    h = hashlib.sha256()
    for f in sorted(Path(path).iterdir()):
        h.update(f.name.encode() + b"\0" + f.read_bytes())
    return h.hexdigest()


def run_once(exe, mode, threads, args, out_dir=None):
    env = dict(os.environ)
    env["BUILD_PAR_THREADS"] = str(threads)
    before = load()
    t0 = time.perf_counter()
    argv = [str(exe), mode, *args] if out_dir is None else [str(exe), *args, mode]
    r = subprocess.run(argv, capture_output=True, env=env, creationflags=BELOW_NORMAL)
    wall = time.perf_counter() - t0
    after = load()
    if r.returncode != 0:
        sys.exit(f"{exe} {mode} failed: {r.stderr.decode(errors='replace')}")
    m = re.search(rb"kernel_ms (\d+)", r.stderr)
    digest = hashlib.sha256(r.stdout)
    if out_dir is not None:  # the traces the run wrote count toward byte identity
        digest.update(dir_digest(out_dir).encode())
    return {
        "mode": mode, "threads": threads, "wall_s": wall,
        "kernel_ms": int(m.group(1)) if m else None,
        "stdout_sha256": digest.hexdigest(),
        "stdout_bytes": len(r.stdout),
        "load_before": before, "load_after": after,
    }


def median_ci(xs, seed=1, n=10000):
    rng = random.Random(seed)
    meds = sorted(statistics.median(rng.choices(xs, k=len(xs))) for _ in range(n))
    return statistics.median(xs), meds[int(0.025 * n)], meds[int(0.975 * n) - 1]


def summarize(rows, threads):
    by_round = {}
    for row in rows:
        by_round.setdefault(row["round"], {})[row["label"]] = row
    par = f"par{threads}"
    k_ratio = [r["serial"]["kernel_ms"] / r[par]["kernel_ms"] for r in by_round.values()]
    w_ratio = [r["serial"]["wall_s"] / r[par]["wall_s"] for r in by_round.values()]
    k_over = [r["par1"]["kernel_ms"] / r["serial"]["kernel_ms"] for r in by_round.values()]
    out = {
        "rounds": len(by_round),
        "kernel_speedup": median_ci(k_ratio),
        "wall_speedup": median_ci(w_ratio),
        "par1_over_serial_kernel": median_ci(k_over),
        "kernel_ms": {lab: median_ci([r[lab]["kernel_ms"] for r in by_round.values()])
                      for lab in ("serial", "par1", par)},
        "cpu_percent_before": median_ci([x["load_before"]["cpu_percent"] for x in rows]),
        "cpu_percent_after": median_ci([x["load_after"]["cpu_percent"] for x in rows]),
        "distinct_stdout": sorted({x["stdout_sha256"] for x in rows}),
    }
    gpus = [x["load_before"]["gpu_percent"] for x in rows
            if isinstance(x["load_before"]["gpu_percent"], int)]
    out["gpu_percent_before"] = median_ci(gpus) if gpus else "unknown"
    return out


def full_image_check(exe, threads, args):
    """Shade only: the whole image, one pixel per line, serial vs par-N."""
    a = run_once(exe, "serial-full", 1, args)
    b = run_once(exe, "par-full", threads, args)
    return {"serial_full": a["stdout_sha256"], "par_full": b["stdout_sha256"],
            "bytes": a["stdout_bytes"], "identical": a["stdout_sha256"] == b["stdout_sha256"]}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--buildc", required=True)
    ap.add_argument("--runs", type=int, default=10)
    ap.add_argument("--threads", type=int, default=8)
    ap.add_argument("--out", default=str(HERE / "results"))
    ap.add_argument("--only", default=None)
    ap.add_argument("--combat", nargs=4, metavar=("EXE", "INPUTS", "JS_DIR", "OUT_DIR"),
                    help="also time the private Low Water combat port (40 fights)")
    opts = ap.parse_args()
    out_dir = Path(opts.out)
    out_dir.mkdir(parents=True, exist_ok=True)
    rng = random.Random(20261107)
    report = {"machine": {"logical_cpus": psutil.cpu_count(),
                          "physical_cores": psutil.cpu_count(logical=False)},
              "threads": opts.threads, "runs": opts.runs, "benches": {}}
    plan = [(n, a, None) for n, a in BENCHES.items()]
    if opts.combat:
        exe, inputs, js, out = opts.combat
        Path(out).mkdir(parents=True, exist_ok=True)
        plan.append(("combat40", [inputs, js, out], (Path(exe), out)))
    for name, args, external in plan:
        if opts.only and name != opts.only:
            continue
        exe = external[0] if external else build(opts.buildc, name)
        out_dir = external[1] if external else None
        modes = [("serial", "serial", 1), ("par1", "par", 1),
                 (f"par{opts.threads}", "par", opts.threads)]
        rows = []
        for rnd in range(opts.runs):
            order = modes[:]
            rng.shuffle(order)
            for label, mode, threads in order:
                row = run_once(exe, mode, threads, args, out_dir)
                row.update({"round": rnd, "label": label})
                rows.append(row)
                print(f"{name} r{rnd} {label}: kernel {row['kernel_ms']} ms, "
                      f"wall {row['wall_s']:.3f} s, cpu {row['load_before']['cpu_percent']}"
                      f"->{row['load_after']['cpu_percent']} %", flush=True)
        summary = summarize(rows, opts.threads)
        summary["byte_identical"] = len(summary["distinct_stdout"]) == 1
        if name == "shade":
            summary["full_image"] = full_image_check(exe, opts.threads, args)
        report["benches"][name] = {"args": args, "summary": summary, "rows": rows}
        print(json.dumps(summary, indent=1), flush=True)
    stamp = time.strftime("%Y%m%d-%H%M%S")
    path = out_dir / f"bench-{stamp}.json"
    path.write_text(json.dumps(report, indent=1))
    print(f"wrote {path}")


if __name__ == "__main__":
    main()
