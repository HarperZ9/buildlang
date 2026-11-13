"""Run and record the local, offline Task 13 paper-core release gate."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

try:
    from scripts.capture_live_paper_gate import _normalize_output
    from scripts.verify_live_paper_artifacts import (
        PAPER_CORE_SDIST,
        PAPER_CORE_WHEEL,
        PRESCRIBED_GATE_COMMANDS,
        _derived_promotion,
    )
except ModuleNotFoundError:  # Direct ``python scripts/...`` execution.
    from capture_live_paper_gate import _normalize_output
    from verify_live_paper_artifacts import (
        PAPER_CORE_SDIST,
        PAPER_CORE_WHEEL,
        PRESCRIBED_GATE_COMMANDS,
        _derived_promotion,
    )

ROOT = Path(__file__).resolve().parents[1]
WHEEL = ROOT / PAPER_CORE_WHEEL
SDIST = ROOT / PAPER_CORE_SDIST
TRANSCRIPT = Path("docs/live-paper/evidence/G2-command-transcript.json")
RECEIPT = Path("docs/live-paper/evidence/G2-green.json")
PROMOTION = Path("docs/live-paper/promotion-status.json")


def _commands() -> list[tuple[list[str], list[str]]]:
    commands: list[tuple[list[str], list[str]]] = []
    for prescribed in PRESCRIBED_GATE_COMMANDS:
        command_args = list(prescribed)
        executable_args = [sys.executable, *prescribed[1:]] if prescribed[0] == "python" else command_args.copy()
        commands.append((command_args, executable_args))
    return commands


def _sha256(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()


def _canonical_write(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(
        json.dumps(value, ensure_ascii=False, separators=(",", ":"), sort_keys=True).encode("utf-8") + b"\n"
    )


def _git_head() -> str:
    return subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
        encoding="utf-8",
    ).stdout.strip()


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--implementation-sha", default=None)
    parser.add_argument("--transcript", type=Path, default=TRANSCRIPT)
    parser.add_argument("--receipt", type=Path, default=RECEIPT)
    parser.add_argument("--promotion-status", type=Path, default=PROMOTION)
    parser.add_argument(
        "--authorize-node",
        default=None,
        help="Record an operator authorization for this node in the promotion state.",
    )
    parser.add_argument(
        "--authorized-by",
        default=None,
        help="Who authorized the node; required with --authorize-node.",
    )
    return parser.parse_args()


def _authorization(implementation_sha: str, args: argparse.Namespace) -> dict[str, str] | None:
    if (args.authorize_node is None) != (args.authorized_by is None):
        raise SystemExit("--authorize-node and --authorized-by must be supplied together")
    if args.authorize_node is None:
        return None
    node = args.authorize_node.strip()
    authorized_by = args.authorized_by.strip()
    if not node or not authorized_by:
        raise SystemExit("--authorize-node and --authorized-by must be non-empty")
    return {
        "authorized_at": datetime.now(timezone.utc).isoformat(timespec="seconds"),
        "authorized_by": authorized_by,
        "implementation_sha": implementation_sha,
        "node": node,
    }


def main() -> int:
    args = _parse_args()
    implementation_sha = args.implementation_sha or _git_head()
    environment = os.environ.copy()
    environment["PYTHONDONTWRITEBYTECODE"] = "1"
    rows: list[dict[str, object]] = []
    for command_args, executable_args in _commands():
        print(f"$ {' '.join(command_args)}", flush=True)
        completed = subprocess.run(
            executable_args,
            cwd=ROOT,
            env=environment,
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
        )
        output = _normalize_output(completed.stdout, ROOT)
        sys.stdout.write(output)
        rows.append(
            {
                "command_args": command_args,
                "exit_code": completed.returncode,
                "output": output,
                "stdout_sha256": _sha256(output.encode("utf-8")),
            }
        )
        if completed.returncode != 0:
            break
    transcript = {
        "commands": rows,
        "implementation_sha": implementation_sha,
        "schema": "build-finance.live-paper.g2-command-transcript/v1",
    }
    transcript_path = args.transcript if args.transcript.is_absolute() else ROOT / args.transcript
    _canonical_write(transcript_path, transcript)
    if len(rows) != len(_commands()) or any(row["exit_code"] != 0 for row in rows):
        return 1
    wheel = WHEEL
    sdist = SDIST
    if not wheel.is_file() or not sdist.is_file():
        return 1
    receipt_path = args.receipt if args.receipt.is_absolute() else ROOT / args.receipt
    promotion_path = args.promotion_status if args.promotion_status.is_absolute() else ROOT / args.promotion_status
    promotion = _derived_promotion(
        implementation_sha,
        receipt_path.relative_to(ROOT).as_posix(),
        _authorization(implementation_sha, args),
    )
    command_evidence = [
        {
            "command_args": row["command_args"],
            "exit_code": row["exit_code"],
            "stdout_sha256": row["stdout_sha256"],
        }
        for row in rows
    ]
    receipt = {
        "artifacts": {
        "sdist": {"path": PAPER_CORE_SDIST, "sha256": _sha256(sdist.read_bytes())},
        "wheel": {"path": PAPER_CORE_WHEEL, "sha256": _sha256(wheel.read_bytes())},
        },
        "commands": command_evidence,
        "implementation_sha": implementation_sha,
        "paper_core_manifest_sha256": json.loads(
            (ROOT / "build_finance/live_paper/paper_core_manifest.json").read_text(encoding="utf-8")
        )["manifest_sha256"],
        "promotion_status": promotion,
        "schema": "build-finance.live-paper.g2-gate-receipt/v2",
        "status": "GREEN",
        "transcript": {
            "path": transcript_path.relative_to(ROOT).as_posix(),
            "sha256": _sha256(transcript_path.read_bytes()),
        },
    }
    _canonical_write(receipt_path, receipt)
    _canonical_write(promotion_path, promotion)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
