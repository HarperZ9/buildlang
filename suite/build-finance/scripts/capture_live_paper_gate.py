"""Capture a deterministic G2 live-paper pytest gate receipt."""

from __future__ import annotations

import hashlib
import json
import os
import re
import subprocess
import sys
from collections.abc import Sequence
from pathlib import Path

_PYTEST_SUMMARY_TIMING_RE = re.compile(
    r"(?m)^(?P<prefix>(?:=+\s+)?(?:no tests ran|"
    r"(?:\d+ (?:failed|passed|skipped|deselected|xfailed|xpassed|warnings?|errors?|"
    r"subtests (?:passed|failed|skipped)))"
    r"(?:, \d+ (?:failed|passed|skipped|deselected|xfailed|xpassed|warnings?|errors?|"
    r"subtests (?:passed|failed|skipped)))*) in )"
    r"\d+\.\d{2}s(?: \([^\r\n)]+\))?"
    r"(?P<suffix>(?:\s+=+)?)(?=\n*\Z)"
)


def _usage() -> str:
    return "usage: capture_live_paper_gate.py NODE PHASE OUTPUT_JSON [--] PYTEST_ARG [...]"


def _git(repo_root: Path, *args: str) -> str:
    completed = subprocess.run(
        ["git", *args],
        cwd=repo_root,
        check=True,
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    return completed.stdout.strip()


def _repo_root() -> Path:
    script_root = Path(__file__).resolve().parents[1]
    return Path(_git(script_root, "rev-parse", "--show-toplevel")).resolve()


def _has_cacheprovider_disabled(args: Sequence[str]) -> bool:
    for index, arg in enumerate(args):
        if arg == "-p" and index + 1 < len(args) and args[index + 1] == "no:cacheprovider":
            return True
        if arg == "-pno:cacheprovider":
            return True
    return False


def _has_full_report(args: Sequence[str]) -> bool:
    return any(arg == "-rA" or (arg.startswith("-r") and "A" in arg[2:]) for arg in args)


def _normalize_output(output: bytes, repo_root: Path) -> str:
    normalized = output.decode("utf-8", errors="replace").replace("\r\n", "\n").replace("\r", "\n")
    roots = {
        str(repo_root),
        str(repo_root).replace("\\", "/"),
        str(repo_root).replace("/", "\\"),
    }
    for root in sorted(roots, key=len, reverse=True):
        normalized = re.sub(re.escape(root), "<repo>", normalized, flags=re.IGNORECASE)
    return _PYTEST_SUMMARY_TIMING_RE.sub(
        r"\g<prefix><pytest-duration>\g<suffix>",
        normalized,
        count=1,
    )


def _test_ids(normalized_output: str) -> tuple[list[str], list[str]]:
    failing: set[str] = set()
    passing: set[str] = set()
    for line in normalized_output.splitlines():
        for label, destination in (("FAILED", failing), ("PASSED", passing)):
            prefix = f"{label} "
            if not line.startswith(prefix):
                continue
            test_id = line[len(prefix) :].split(" - ", 1)[0].strip()
            if "::" in test_id:
                destination.add(test_id.replace("\\", "/"))
    passing.difference_update(failing)
    return sorted(failing), sorted(passing)


def main(argv: Sequence[str] | None = None) -> int:
    raw_args = list(sys.argv[1:] if argv is None else argv)
    if len(raw_args) < 4:
        print(_usage(), file=sys.stderr)
        return 2

    node, phase, output_json, *pytest_args = raw_args
    if pytest_args and pytest_args[0] == "--":
        pytest_args = pytest_args[1:]
    if not pytest_args:
        print(_usage(), file=sys.stderr)
        return 2

    if not _has_cacheprovider_disabled(pytest_args):
        pytest_args.extend(["-p", "no:cacheprovider"])
    if not _has_full_report(pytest_args):
        pytest_args.append("-rA")

    repo_root = _repo_root()
    command_args = ["python", "-m", "pytest", *pytest_args]
    environment = os.environ.copy()
    environment["PYTHONDONTWRITEBYTECODE"] = "1"

    completed = subprocess.run(
        [sys.executable, "-m", "pytest", *pytest_args],
        cwd=repo_root,
        env=environment,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    normalized_output = _normalize_output(completed.stdout, repo_root)
    sys.stdout.write(normalized_output)
    failing_test_ids, passing_test_ids = _test_ids(normalized_output)

    receipt = {
        "command_args": command_args,
        "exit_code": completed.returncode,
        "failing_test_ids": failing_test_ids,
        "git_commit": _git(repo_root, "rev-parse", "HEAD"),
        "node": node,
        "passing_test_ids": passing_test_ids,
        "phase": phase,
        "stdout_sha256": hashlib.sha256(normalized_output.encode("utf-8")).hexdigest(),
    }
    output_path = Path(output_json)
    if not output_path.is_absolute():
        output_path = repo_root / output_path
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_bytes(
        json.dumps(receipt, ensure_ascii=False, separators=(",", ":"), sort_keys=True).encode("utf-8") + b"\n"
    )
    return completed.returncode


if __name__ == "__main__":
    raise SystemExit(main())
