from __future__ import annotations

import hashlib
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Any

import pytest

ENGINE_ROOT = Path(__file__).resolve().parents[1]
UPSTREAM_ROOT = Path(
    os.environ.get(
        "BUILD_FINANCE_G2_ROOT",
        ENGINE_ROOT.parent / "build-finance-g2-offline-paper",
    )
)
EXPORTED_BUNDLE = Path(
    os.environ.get(
        "BUILD_FINANCE_G2_BUNDLE",
        UPSTREAM_ROOT
        / ".artifacts"
        / "g2-evaluation"
        / "task-2-fix1-0e4f43de6b75"
        / "g2-disk-fixture",
    )
)
EXPECTED_PROJECTION_ID = "8eeac26fa52de406952a3e6e9ba4d358fcb86206155952e99a7a15494fcf8f34"
EXPECTED_CLOSURE_ID = "f32b1b906c6b08e4ef5763a44c4a36db57bbe4dd0c7665e05c020b864c3fabb7"
EXPECTED_RECEIPT_IDS = [
    "45f72ae77056d46da24491b4c5db70fe232843058c51a2aef2f35a41f66b5e87",
    "beaefe675ff68a2e6179644d9fbbadf1df4aebc7b69c380842dd9bc47b5f4acf",
    "c91d860fc4e98144d3ed69be9bb750b001bcdf8a19b49b7ffc26ea2b1b29c3d1",
    "f9f4eee2edad7d4b595c46ed139183873da95218658a8483c48894da0960190d",
    EXPECTED_CLOSURE_ID,
]

pytestmark = pytest.mark.skipif(
    not (UPSTREAM_ROOT / "build_finance" / "paper_core_loader.py").is_file()
    or not EXPORTED_BUNDLE.is_dir(),
    reason="reviewed Build Finance G2 checkout and exported bundle are unavailable",
)


def _fresh_fixture(destination: Path) -> Path:
    return Path(shutil.copytree(EXPORTED_BUNDLE, destination))


def _run_capital_replay(
    fixture: Path,
    *,
    cwd: Path,
    receipt_output: Path | None = None,
) -> subprocess.CompletedProcess[str]:
    command = [
        sys.executable,
        "-m",
        "build_engine.cli",
        "capital-replay",
        "--fixture",
        os.fspath(fixture),
    ]
    if receipt_output is not None:
        command.extend(("--receipt-output", os.fspath(receipt_output)))
    environment = os.environ.copy()
    environment["PYTHONPATH"] = os.pathsep.join(
        (
            os.fspath(UPSTREAM_ROOT),
            os.fspath(ENGINE_ROOT),
            environment.get("PYTHONPATH", ""),
        )
    )
    return subprocess.run(
        command,
        cwd=cwd,
        env=environment,
        check=False,
        capture_output=True,
        text=True,
    )


def _canonical_json(value: Any) -> bytes:
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"), sort_keys=True).encode("utf-8")


def test_capital_replay_uses_actual_disk_envelope_without_injected_context(tmp_path: Path) -> None:
    """Breaks if the product shell does not bind the reviewed disk-envelope loader."""

    fixture = _fresh_fixture(tmp_path / "fixture")
    runner = tmp_path / "runner"
    runner.mkdir()

    completed = _run_capital_replay(fixture, cwd=runner)

    assert completed.returncode == 0, completed.stderr
    assert completed.stdout.splitlines()[0] == "PAPER ONLY / SIMULATED / MODEL DISABLED"
    assert f"Projection ID : {EXPECTED_PROJECTION_ID}" in completed.stdout
    assert f"Closure ID    : {EXPECTED_CLOSURE_ID}" in completed.stdout
    assert "Closure status: CLOSED" in completed.stdout
    assert "Model evidence: decision 1: ABSTAIN / MODEL_DISABLED" in completed.stdout
    assert "Model evidence: decision 2: ABSTAIN / MODEL_DISABLED" in completed.stdout


def test_capital_replay_writes_path_free_canonical_stable_evaluator_receipt(tmp_path: Path) -> None:
    """Breaks if evaluator evidence varies by machine path or loses paper-only semantics."""

    first_fixture = _fresh_fixture(tmp_path / "first-fixture")
    second_fixture = _fresh_fixture(tmp_path / "second-fixture")
    first_runner = tmp_path / "first-runner"
    second_runner = tmp_path / "second-runner"
    first_runner.mkdir()
    second_runner.mkdir()
    first_receipt = first_runner / "evaluator-receipt.json"
    second_receipt = second_runner / "evaluator-receipt.json"

    first = _run_capital_replay(first_fixture, cwd=first_runner, receipt_output=first_receipt)
    second = _run_capital_replay(second_fixture, cwd=second_runner, receipt_output=second_receipt)

    assert first.returncode == 0, first.stderr
    assert second.returncode == 0, second.stderr
    first_payload = first_receipt.read_bytes()
    second_payload = second_receipt.read_bytes()
    assert first_payload == second_payload
    assert os.fspath(first_fixture.resolve()).encode() not in first_payload
    assert os.fspath(second_fixture.resolve()).encode() not in second_payload

    receipt = json.loads(first_payload)
    assert first_payload == _canonical_json(receipt) + b"\n"
    assert receipt["schema"] == "build-engine.capital.evaluator-receipt/v1"
    assert receipt["data_classification"] == "SYNTHETIC"
    assert receipt["mode"] == "PAPER_ONLY"
    assert receipt["execution"] == "SIMULATED"
    assert receipt["model_inference"] == "DISABLED_ABSTAIN"
    assert receipt["profitability_claim"] is False
    assert receipt["projection_id"] == EXPECTED_PROJECTION_ID
    assert receipt["closure_id"] == EXPECTED_CLOSURE_ID
    assert receipt["closure_status"] == "CLOSED"
    assert receipt["closure_reason_codes"] == []
    assert receipt["receipt_ids"] == EXPECTED_RECEIPT_IDS
    assert receipt["model_evidence"] == [
        {
            "decision_sequence": "1",
            "reason_codes": ["MODEL_DISABLED"],
            "status": "ABSTAIN",
        },
        {
            "decision_sequence": "2",
            "reason_codes": ["MODEL_DISABLED"],
            "status": "ABSTAIN",
        },
    ]
    assert receipt["evidence"] == [
        {
            "evidence_id": "1",
            "kind": "DECISION",
            "reason_codes": ["RISK_NO_ACTION"],
            "status": "REJECT",
        },
        {
            "evidence_id": "2",
            "kind": "DECISION",
            "reason_codes": ["RISK_SESSION_CLOSED"],
            "status": "REJECT",
        },
        {
            "evidence_id": "f9f4eee2edad7d4b595c46ed139183873da95218658a8483c48894da0960190d",
            "kind": "PORTFOLIO",
            "reason_codes": [],
            "status": "CLOSED",
        },
        {
            "evidence_id": EXPECTED_CLOSURE_ID,
            "kind": "CLOSURE",
            "reason_codes": [],
            "status": "CLOSED",
        },
    ]
    unsigned = dict(receipt)
    evaluator_receipt_id = unsigned.pop("evaluator_receipt_id")
    assert evaluator_receipt_id == hashlib.sha256(_canonical_json(unsigned)).hexdigest()


def test_capital_replay_prints_no_success_output_when_receipt_cannot_be_written(tmp_path: Path) -> None:
    """Breaks if completion is reported before the requested receipt is durable."""

    fixture = _fresh_fixture(tmp_path / "fixture")
    runner = tmp_path / "runner"
    runner.mkdir()
    receipt = runner / "missing-parent" / "evaluator-receipt.json"

    completed = _run_capital_replay(fixture, cwd=runner, receipt_output=receipt)

    assert completed.returncode != 0
    assert "unable to write evaluator receipt" in completed.stderr
    assert "PAPER ONLY / SIMULATED / MODEL DISABLED" not in completed.stdout
    assert "Capital replay complete" not in completed.stdout
    assert "Projection ID" not in completed.stdout
    assert not receipt.exists()
