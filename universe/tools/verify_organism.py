"""Run every verifiable component listed in components.toml and report ground
truth -- what actually builds and passes on this machine right now. This is the
organism health check: one command, no claims, just observed results.

    python tools/verify_organism.py            # run all
    python tools/verify_organism.py --quick    # skip heavy components
    python tools/verify_organism.py --json      # machine-readable summary

    python tools/verify_organism.py --no-skip   # a skipped component counts as a failure

Exit code is the number of failures (0 = healthy), so it doubles as a CI gate.

A component that is known to fail carries `xfail = "<issue and reason>"` in
components.toml. It still runs. While it fails it is reported as XFAIL and does
not count as a failure. When it starts to pass it is reported as XPASS and
counts as a failure, so the mark has to be removed in the same change. A
component with `covered_by` is not run here because another CI job runs it; it
is reported as ELSEWHERE with that job named.
"""
import argparse
import glob
import json
import os
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(HERE)
MANIFEST = os.path.join(HERE, "components.toml")
sys.path.insert(0, os.path.join(HERE, "coherence"))
import freshness  # noqa: E402


def load_manifest():
    import tomllib
    with open(MANIFEST, "rb") as f:
        return tomllib.load(f).get("component", [])


def has_msvc():
    pats = [
        "C:/Program Files/Microsoft Visual Studio/*/*/VC/Auxiliary/Build/vcvars64.bat",
        "C:/Program Files (x86)/Microsoft Visual Studio/*/*/VC/Auxiliary/Build/vcvars64.bat",
    ]
    return any(glob.glob(p) for p in pats)


# Where a built compiler can be. This tree lives in HarperZ9/buildlang as universe/, so the
# compiler is one level up; the nested buildlang/ folder is the layout of the old repository.
COMPILER_TARGETS = [
    os.path.join(os.path.dirname(REPO), "compiler", "target"),
    os.path.join(REPO, "buildlang", "compiler", "target"),
]


def buildc_dirs():
    """Folders that may hold buildc. BUILDC_DIR names one directly, for a compiler built elsewhere."""
    dirs = []
    if os.environ.get("BUILDC_DIR"):
        dirs.append(os.environ["BUILDC_DIR"])
    for target in COMPILER_TARGETS:
        dirs.append(os.path.join(target, "release"))
        dirs.append(os.path.join(target, "debug"))
    return dirs


def has_buildc():
    for d in buildc_dirs():
        for exe in ("buildc.exe", "buildc"):
            if os.path.isfile(os.path.join(d, exe)):
                return True
    return False


def build_env():
    env = dict(os.environ)
    extra = [os.path.expanduser("~/.cargo/bin")] + buildc_dirs()
    paths = [p for p in extra if os.path.isdir(p)]
    if paths:
        env["PATH"] = os.pathsep.join(paths) + os.pathsep + env.get("PATH", "")
    return env


def run_component(c, env, avail, quick):
    if c.get("covered_by"):
        return ("ELSEWHERE", 0.0, [])
    path = os.path.join(REPO, c["path"].replace("/", os.sep))
    if not os.path.isdir(path):
        return ("SKIP:absent", 0.0, [])
    req = c.get("requires")
    if req and not avail.get(req):
        return ("SKIP:no-" + req, 0.0, [])
    if c.get("heavy") and quick:
        return ("SKIP:quick", 0.0, [])
    start = time.monotonic()
    proc = subprocess.run(c["verify"], shell=True, cwd=path, env=env,
                          stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    dur = time.monotonic() - start
    tail = proc.stdout.decode("utf-8", "replace").splitlines()[-10:]
    if c.get("xfail"):
        if proc.returncode == 0:
            return ("XPASS", dur, ["passes now: remove `xfail` from components.toml (" + c["xfail"] + ")"])
        return ("XFAIL", dur, tail[-3:])
    if proc.returncode == 0:
        return ("PASS", dur, [])
    return ("FAIL", dur, tail)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--quick", action="store_true", help="skip heavy components")
    ap.add_argument("--json", action="store_true", help="machine-readable output")
    ap.add_argument("--no-skip", action="store_true",
                    help="count a skipped component as a failure (CI uses this)")
    args = ap.parse_args()

    comps = load_manifest()
    by_name = {c["name"]: c for c in comps}
    env = build_env()
    avail = {"msvc": has_msvc(), "buildc": has_buildc()}

    results = []
    for c in comps:
        cdir = os.path.join(REPO, c["path"].replace("/", os.sep))
        fresh = "-"
        if os.path.isdir(cdir) and not c.get("covered_by"):
            fcode, _w = freshness.adjudicate(c["name"], by_name)
            fresh = {0: "fresh", 1: "STALE", 2: "new"}[fcode]
        status, dur, tail = run_component(c, env, avail, args.quick)
        if status == "PASS":
            freshness.record(c["name"], by_name)
        results.append({
            "name": c["name"], "language": c.get("language", "?"),
            "tier": c.get("tier", "?"), "expect": c.get("expect", "tested"),
            "status": status, "seconds": round(dur, 1), "tail": tail, "fresh": fresh,
            "xfail": c.get("xfail", ""), "covered_by": c.get("covered_by", ""),
        })

    passed = sum(1 for r in results if r["status"] == "PASS")
    skipped = sum(1 for r in results if r["status"].startswith("SKIP"))
    xfailed = sum(1 for r in results if r["status"] == "XFAIL")
    xpassed = sum(1 for r in results if r["status"] == "XPASS")
    elsewhere = sum(1 for r in results if r["status"] == "ELSEWHERE")
    failures = sum(1 for r in results if r["status"] == "FAIL") + xpassed
    if args.no_skip:
        failures += skipped

    if args.json:
        print(json.dumps({"passed": passed, "failed": failures, "skipped": skipped,
                          "xfailed": xfailed, "xpassed": xpassed, "elsewhere": elsewhere,
                          "components": results}, indent=2))
        sys.exit(failures)

    print("")
    print("Build organism -- verifiable components (ground truth)")
    print("")
    width = max(len(r["name"]) for r in results)
    header = "  " + "component".ljust(width) + "  lang    tier  expect   result        fresh  time"
    print(header)
    print("  " + "-" * (width + 44))
    for r in results:
        secs = (str(r["seconds"]) + "s").rjust(6) if r["seconds"] else "   -  "
        line = ("  " + r["name"].ljust(width) + "  "
                + str(r["language"]).ljust(6) + "  T" + str(r["tier"]).ljust(3) + "  "
                + str(r["expect"]).ljust(7) + "  " + r["status"].ljust(12) + "  "
                + str(r.get("fresh", "-")).ljust(5) + "  " + secs)
        print(line)
    for r in results:
        if r["tail"]:
            print("")
            print("  -- " + r["name"] + " output tail --")
            for ln in r["tail"]:
                print("    " + ln)
    print("")
    print("ORGANISM: " + str(passed) + " passed, " + str(failures)
          + " failed, " + str(xfailed) + " expected failures, " + str(xpassed)
          + " unexpected passes, " + str(skipped) + " skipped, " + str(elsewhere)
          + " run by another job")
    print("")
    sys.exit(failures)


if __name__ == "__main__":
    main()
