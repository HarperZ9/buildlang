"""Run every verifiable component listed in components.toml and report ground
truth -- what actually builds and passes on this machine right now. This is the
organism health check: one command, no claims, just observed results.

    python tools/verify_organism.py            # run all
    python tools/verify_organism.py --quick    # skip heavy components
    python tools/verify_organism.py --json      # machine-readable summary

Exit code is the number of failures (0 = healthy), so it doubles as a CI gate.
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


def has_buildc():
    for sub in ("debug", "release"):
        if os.path.isfile(os.path.join(REPO, "buildlang", "compiler", "target", sub, "buildc.exe")):
            return True
    return False


def build_env():
    env = dict(os.environ)
    extra = [
        os.path.expanduser("~/.cargo/bin"),
        os.path.join(REPO, "buildlang", "compiler", "target", "debug"),
        os.path.join(REPO, "buildlang", "compiler", "target", "release"),
    ]
    paths = [p for p in extra if os.path.isdir(p)]
    if paths:
        env["PATH"] = os.pathsep.join(paths) + os.pathsep + env.get("PATH", "")
    return env


def run_component(c, env, avail, quick):
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
    if proc.returncode == 0:
        return ("PASS", dur, [])
    tail = proc.stdout.decode("utf-8", "replace").splitlines()[-10:]
    return ("FAIL", dur, tail)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--quick", action="store_true", help="skip heavy components")
    ap.add_argument("--json", action="store_true", help="machine-readable output")
    args = ap.parse_args()

    comps = load_manifest()
    by_name = {c["name"]: c for c in comps}
    env = build_env()
    avail = {"msvc": has_msvc(), "buildc": has_buildc()}

    results = []
    for c in comps:
        cdir = os.path.join(REPO, c["path"].replace("/", os.sep))
        fresh = "-"
        if os.path.isdir(cdir):
            fcode, _w = freshness.adjudicate(c["name"], by_name)
            fresh = {0: "fresh", 1: "STALE", 2: "new"}[fcode]
        status, dur, tail = run_component(c, env, avail, args.quick)
        if status == "PASS":
            freshness.record(c["name"], by_name)
        results.append({
            "name": c["name"], "language": c.get("language", "?"),
            "tier": c.get("tier", "?"), "expect": c.get("expect", "tested"),
            "status": status, "seconds": round(dur, 1), "tail": tail, "fresh": fresh,
        })

    failures = sum(1 for r in results if r["status"] == "FAIL")
    passed = sum(1 for r in results if r["status"] == "PASS")
    skipped = sum(1 for r in results if r["status"].startswith("SKIP"))

    if args.json:
        print(json.dumps({"passed": passed, "failed": failures,
                          "skipped": skipped, "components": results}, indent=2))
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
          + " failed, " + str(skipped) + " skipped")
    print("")
    sys.exit(failures)


if __name__ == "__main__":
    main()
