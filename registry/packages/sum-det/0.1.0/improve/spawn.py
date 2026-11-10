"""Process spawner for the improvement kernel (improve/kernel).

The kernel is written in BuildLang, and BuildLang has no builtin that starts
a process. This script is that missing part and nothing more: it builds the
kernel, asks it for a plan, runs each plan line, and asks again until the
kernel prints "done". It judges nothing and writes no ledger line.

Usage, from anywhere:
    python improve/spawn.py --buildc BUILDC [--memwatch SCRIPT] PROPOSAL...

Before the first plan it refuses to start unless every path listed in
improve/FROZEN.sha256 has the listed digest and is unchanged from git HEAD.
--memwatch names a script called as `python SCRIPT --out FILE -- CMD...` that
prints a line holding `rc N` and `rss_B N`; without it the peak is polled here.
"""
import hashlib
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path

PKG = Path(__file__).resolve().parent.parent
RUN_TIMEOUT_S = 900


def below_normal():
    if os.name == "nt":
        import ctypes
        k32 = ctypes.windll.kernel32
        k32.GetCurrentProcess.restype = ctypes.c_void_p
        k32.SetPriorityClass(ctypes.c_void_p(k32.GetCurrentProcess()), 0x4000)
    else:
        os.nice(5)


def frozen_check():
    """Exit unless the frozen set matches its digests and git HEAD."""
    listed = [row.split("  ", 1) for row in (PKG / "improve/FROZEN.sha256").read_text().splitlines() if row]
    for digest, rel in listed:
        if hashlib.sha256((PKG / rel).read_bytes()).hexdigest() != digest:
            sys.exit(f"frozen file changed: {rel}")
    paths = ["improve/FROZEN.sha256"] + [rel for _, rel in listed]
    for cmd in (["git", "ls-files", "--error-unmatch", "--"], ["git", "diff", "--quiet", "HEAD", "--"]):
        if subprocess.run(cmd + paths, cwd=PKG, capture_output=True).returncode != 0:
            sys.exit("frozen set differs from git HEAD; commit it through review first")


def tree(dest, slot):
    dest = PKG / dest
    shutil.rmtree(dest, ignore_errors=True)
    shutil.copytree(PKG / "src", dest / "src")
    for name in ("Build.toml", "Build.lock"):
        shutil.copy(PKG / name, dest / name)
    if slot != "-":
        shutil.copy(PKG / slot, dest / "src" / "i8.bld")


def command(args, buildc):
    if args[0] == "BUILDC":
        return [buildc] + args[1:]
    exe = PKG / args[0]
    return [str(exe if exe.exists() else exe.with_suffix(""))] + args[1:]


def run(out, cwd, args, buildc):
    out = PKG / out
    out.parent.mkdir(parents=True, exist_ok=True)
    with open(out, "wb") as sink:
        try:
            rc = subprocess.run(command(args, buildc), cwd=PKG / cwd, stdout=sink, stderr=subprocess.STDOUT,
                                stdin=subprocess.DEVNULL, timeout=RUN_TIMEOUT_S).returncode
        except (OSError, subprocess.TimeoutExpired) as err:
            rc = 124
            sink.write(str(err).encode())
    Path(str(out) + ".rc").write_text(f"{rc}\n")


def polled(cmd, out, cwd):
    """Run cmd and return a line with its exit code and polled peak resident bytes."""
    import psutil
    with open(out, "wb") as sink:
        child = subprocess.Popen(cmd, cwd=cwd, stdout=sink, stdin=subprocess.DEVNULL)
        peak, proc = 0, psutil.Process(child.pid)
        while child.poll() is None:
            try:
                peak = max(peak, proc.memory_info().rss)
            except psutil.Error:
                break
            time.sleep(0.02)
    return f"rc {child.wait()} rss_B {peak}"


def bench(out, cwd, args, buildc, memwatch):
    out = PKG / out
    out.parent.mkdir(parents=True, exist_ok=True)
    cmd = command(args, buildc)
    line = "REFUSED"
    for _ in range(40):
        if not memwatch:
            line = polled(cmd, out, PKG / cwd)
        else:
            watch = [sys.executable, memwatch, "--out", str(out), "--"] + cmd
            line = subprocess.run(watch, cwd=PKG / cwd, capture_output=True, text=True).stdout.strip()
        if not line.startswith("REFUSED"):
            break
        time.sleep(30)
    else:
        sys.exit(f"memory watch refused to start a benchmark: {line}")
    words = line.split()
    rc = words[words.index("rc") + 1] if "rc" in words else "125"
    Path(str(out) + ".mw").write_text(line + "\n")
    Path(str(out) + ".rc").write_text(rc + "\n")


def main():
    argv, buildc, memwatch = sys.argv[1:], None, None
    while argv and argv[0].startswith("--"):
        flag, value = argv.pop(0), argv.pop(0)
        buildc, memwatch = (value, memwatch) if flag == "--buildc" else (buildc, value)
    if not buildc:
        sys.exit(__doc__)
    below_normal()
    frozen_check()
    kdir = PKG / "improve" / "kernel"
    subprocess.run([buildc, "pkg", "build", "--release"], cwd=kdir, check=True, capture_output=True)
    kernel = str(kdir / "target" / "pkg" / ("improve-kernel.exe" if os.name == "nt" else "improve-kernel"))
    work = f"target/improve/run-{int(time.time())}"
    (PKG / work).mkdir(parents=True)
    while True:
        plan = subprocess.run([kernel, "next", work] + argv, cwd=PKG, capture_output=True, text=True, check=True)
        for line in plan.stdout.splitlines():
            f = line.split("|")
            if f[0] == "tree":
                tree(f[1], f[2])
            elif f[0] == "run":
                run(f[1], f[2], f[3:], buildc)
            elif f[0] == "bench":
                bench(f[1], f[2], f[3:], buildc, memwatch)
            else:
                print(line, flush=True)
            if line == "done":
                return


if __name__ == "__main__":
    main()
