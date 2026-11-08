"""Planted control: a reduction whose order follows thread completion.

Takes the C that `buildc build --release --keep-c` wrote for the n-body
benchmark and patches the par_for trampoline so each chunk adds its kinetic
energy into one shared double, under the pool's lock, in whatever order the
chunks finish. The program prints that sum at exit. Floating-point addition is
not associative, so the printed bytes depend on the schedule.

The byte-identity check of the harness must catch this: the par-N output must
differ from the par-1 output (one chunk, one sum) in at least one of the runs.
If every run matched, the check would be too weak for this control.

Usage: python plant_reduction.py [--runs 10] [--threads 8]
Compiles with the same flags buildc passes for a release build.
"""

import argparse
import hashlib
import os
import shutil
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
SRC = HERE.parent / "nbody" / "target" / "release" / "main.c"
BELOW_NORMAL = 0x00004000 if os.name == "nt" else 0

OLD = "    for (int64_t i = lo; i < hi; i++) e->out[i] = advance(e->ctx, (int64_t)i);\n}"
NEW = """    for (int64_t i = lo; i < hi; i++) e->out[i] = advance(e->ctx, (int64_t)i);
    double chunk = 0.0;
    for (int64_t i = lo; i < hi; i++) {
        Body b = e->out[i];
        chunk += 0.5 * b.m * (b.vx * b.vx + b.vy * b.vy + b.vz * b.vz);
    }
    BL_PAR_LOCK();
    bl_planted_sum += chunk; /* PLANTED: completion order */
    BL_PAR_UNLOCK();
}"""
PRINTER = """
static double bl_planted_sum = 0.0;
static void bl_planted_print(void) { printf("planted_energy %.17g\\n", bl_planted_sum); }
"""


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--runs", type=int, default=10)
    ap.add_argument("--threads", type=int, default=8)
    opts = ap.parse_args()
    if not SRC.exists():
        sys.exit(f"build the n-body benchmark with --keep-c first: {SRC} missing")
    text = SRC.read_text()
    if OLD not in text:
        sys.exit("trampoline shape changed; update the patch")
    text = text.replace(OLD, NEW, 1)
    anchor = "// A vector of n zeroed elements"
    text = text.replace(anchor, PRINTER + anchor, 1)
    text = text.replace("int32_t main(", "int32_t bl_planted_real_main(")
    text += """
int main(int argc, char** argv) {
    atexit(bl_planted_print);
    return bl_planted_real_main(argc, argv);
}
"""
    work = HERE / "work"
    work.mkdir(exist_ok=True)
    c_path = work / "planted.c"
    c_path.write_text(text)
    exe = work / ("planted.exe" if os.name == "nt" else "planted")
    cc = shutil.which("gcc") or shutil.which("cc")
    libs = ["-lws2_32"] if os.name == "nt" else ["-lm", "-pthread"]
    r = subprocess.run([cc, str(c_path), "-o", str(exe), "-std=c99", "-O2",
                        "-ffp-contract=off", "-fno-fast-math", *libs],
                       capture_output=True, text=True)
    if r.returncode != 0:
        sys.exit(f"compile failed:\n{r.stderr}")
    args = ["par", "2048", "4"]

    def run(threads):
        env = dict(os.environ, BUILD_PAR_THREADS=str(threads))
        out = subprocess.run([str(exe), *args], capture_output=True, env=env,
                             creationflags=BELOW_NORMAL)
        tail = out.stdout.decode().strip().splitlines()[-1]
        return hashlib.sha256(out.stdout).hexdigest(), tail

    ref_hash, ref_tail = run(1)
    results = [run(opts.threads) for _ in range(opts.runs)]
    differ = sum(1 for h, _ in results if h != ref_hash)
    distinct = len({h for h, _ in results})
    print(f"par-1 reference: {ref_tail}")
    for h, tail in results:
        print(f"par-{opts.threads}: {tail} {'DIFFERS' if h != ref_hash else 'same'}")
    print(f"caught: {differ} of {opts.runs} runs differ from par-1; "
          f"{distinct} distinct outputs among the par-{opts.threads} runs")
    sys.exit(0 if differ > 0 else 1)


if __name__ == "__main__":
    main()
