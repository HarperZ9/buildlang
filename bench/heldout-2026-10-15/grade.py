"""Grade bench4. Usage: python grade.py <gen_dir> <out_jsonl>

Environment: BENCH_BUILDC is the buildc binary under test (released 1.6.0).
BUILDLANG_STDLIB is left unset so the stdlib embedded in buildc is used.

Per task, in a fresh temporary directory holding the fixtures:
  1. buildc check <task>.bld            -> nonzero exit: outcome check_fail
  2. copy to main.bld; buildc build .   -> nonzero exit or no
                                           target/debug/main.exe: build_fail
  3. run main.exe (timeout 20 s)        -> timeout or exit code outside 0..255:
                                           crash
  4. compare exit code, stdout and any required output files with
     expected.json                      -> mismatch: wrong; else pass

`buildc run` is not used for steps 2 and 3: it maps the child's exit code to
one byte, so an access violation (0xC0000005) reads as exit 5, and
`buildc FILE -o prog.exe` writes C source, not an executable (both found
while validating this grader with buildc 1.6.0).

Stdout comparison: CRLF becomes LF, trailing spaces at line ends are removed,
leading and trailing empty lines are removed. Leading spaces are kept.
check_pass_but_fail = build_fail + crash (the preregistered M1 metric).
"""
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from reference import FIXTURES  # noqa: E402

BUILDC = os.environ["BENCH_BUILDC"]
EXPECTED = json.loads((HERE / "expected.json").read_text(encoding="utf-8"))


def norm(text):
    lines = [l.rstrip() for l in text.replace("\r\n", "\n").split("\n")]
    while lines and lines[0] == "":
        lines.pop(0)
    while lines and lines[-1] == "":
        lines.pop()
    return "\n".join(lines)


def run(cmd, cwd, timeout):
    env = dict(os.environ)
    env.pop("BUILDLANG_STDLIB", None)
    try:
        p = subprocess.run(cmd, cwd=cwd, capture_output=True, timeout=timeout, env=env)
        dec = lambda b: b.decode("utf-8", "replace")
        return p.returncode, dec(p.stdout), dec(p.stderr)
    except subprocess.TimeoutExpired:
        return None, "", "timeout"


def grade_task(task, src):
    exp = EXPECTED[task]
    work = Path(tempfile.mkdtemp(prefix=f"b4_{task}_"))
    try:
        for name, body in FIXTURES.items():
            (work / name).write_text(body, encoding="utf-8", newline="\n")
        shutil.copy(src, work / src.name)
        rc, out, err = run([BUILDC, "check", "--error-format=json", src.name], work, 120)
        if rc != 0:
            return {"outcome": "check_fail", "detail": out + err}
        shutil.copy(src, work / "main.bld")
        (work / src.name).unlink()
        rc, out, err = run([BUILDC, "build", "."], work, 300)
        exe = work / "target" / "debug" / "main.exe"
        if rc != 0 or not exe.exists():
            return {"outcome": "build_fail", "detail": f"rc={rc}\n{out}\n{err}"}
        rc, out, err = run([str(exe)], work, 20)
        if rc is None or rc < 0 or rc > 255:
            return {"outcome": "crash", "detail": f"rc={rc}\nstdout:\n{out}\nstderr:\n{err}"}
        problems = []
        if rc != exp["exit"]:
            problems.append(f"exit {rc}, expected {exp['exit']}")
        if norm(out) != exp["stdout"]:
            problems.append(f"stdout:\n{out}\nexpected:\n{exp['stdout']}")
        for name, body in exp["files"].items():
            f = work / name
            got = norm(f.read_text(encoding="utf-8", errors="replace")) if f.exists() else None
            if got != body:
                problems.append(f"file {name}: {got!r}, expected {body!r}")
        if problems:
            return {"outcome": "wrong", "detail": "\n".join(problems) + f"\nstderr:\n{err}"}
        return {"outcome": "pass", "detail": ""}
    finally:
        shutil.rmtree(work, ignore_errors=True)


def main():
    gen, out_path = Path(sys.argv[1]), Path(sys.argv[2])
    rows = []
    for task in EXPECTED:
        src = gen / f"{task}.bld"
        r = {"outcome": "missing", "detail": ""} if not src.exists() else grade_task(task, src)
        r["task"] = task
        rows.append(r)
        print(task, r["outcome"], flush=True)
    out_path.write_text("".join(json.dumps(r) + "\n" for r in rows), encoding="utf-8")


if __name__ == "__main__":
    main()
