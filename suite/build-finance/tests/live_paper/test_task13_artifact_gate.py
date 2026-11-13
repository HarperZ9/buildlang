"""Focused regression tests for the durable Task 13 package gate."""

from __future__ import annotations

import base64
import csv
import hashlib
import io
import json
import socket
from pathlib import Path

import pytest

from scripts.build_paper_core_artifacts import build_artifacts
from scripts.run_network_denied import NetworkDenied, deny_network
from scripts.run_network_denied import main as network_denied_main
from scripts.verify_crypto_replay_artifacts import (
    ArchiveMembers,
    VerificationError,
    _read_sdist_members,
    _read_wheel_members,
)
from scripts.verify_live_paper_artifacts import (
    CRITICAL_IMPLEMENTATION_PATHS,
    PAPER_CORE_DIST_INFO,
    PRESCRIBED_GATE_COMMANDS,
    _derived_promotion,
    _verify_ast_closure,
    _verify_complete_member_sets,
    _verify_wheel_record,
    verify_artifacts,
    verify_gate_evidence,
)


def test_gate_freshness_and_static_checks_cover_loader_and_evaluation_exporter() -> None:
    """Breaks if evidence can survive drift or static checks omit either changed implementation surface."""

    changed_surfaces = {
        "build_finance/paper_core_loader.py",
        "scripts/export_g2_evaluation_bundle.py",
    }
    assert changed_surfaces <= set(CRITICAL_IMPLEMENTATION_PATHS)
    ruff_command = next(command for command in PRESCRIBED_GATE_COMMANDS if "ruff" in command)
    mypy_command = next(command for command in PRESCRIBED_GATE_COMMANDS if "mypy" in command)
    assert changed_surfaces <= set(ruff_command)
    assert changed_surfaces <= set(mypy_command)


def _record_payload(members: dict[str, bytes], record_name: str) -> bytes:
    stream = io.StringIO(newline="")
    writer = csv.writer(stream, lineterminator="\n")
    for name, payload in sorted(members.items()):
        digest = base64.urlsafe_b64encode(hashlib.sha256(payload).digest()).rstrip(b"=").decode("ascii")
        writer.writerow((name, f"sha256={digest}", len(payload)))
    writer.writerow((record_name, "", ""))
    return stream.getvalue().encode()


def _otherwise_valid_members() -> tuple[ArchiveMembers, ArchiveMembers, dict[str, str]]:
    source_name = "build_finance/live_paper/kernel.py"
    source_payload = b"MODE = 'OFFLINE_PAPER_ONLY'\n"
    expected = {source_name: hashlib.sha256(source_payload).hexdigest()}
    dist_info = PAPER_CORE_DIST_INFO
    wheel_members = {
        source_name: source_payload,
        f"{dist_info}/METADATA": b"Name: build-finance-paper-core\nVersion: 1.0.1\n",
        f"{dist_info}/WHEEL": b"Wheel-Version: 1.0\n",
        f"{dist_info}/top_level.txt": b"build_finance\n",
    }
    record_name = f"{dist_info}/RECORD"
    wheel_members[record_name] = _record_payload(wheel_members, record_name)
    sdist_members = {
        source_name: source_payload,
        "PKG-INFO": b"Name: build-finance-paper-core\nVersion: 1.0.1\n",
        "README.md": b"paper core\n",
        "build_finance_paper_core.egg-info/PKG-INFO": b"Name: build-finance-paper-core\n",
        "build_finance_paper_core.egg-info/SOURCES.txt": b"",
        "build_finance_paper_core.egg-info/dependency_links.txt": b"",
        "build_finance_paper_core.egg-info/requires.txt": b"",
        "build_finance_paper_core.egg-info/top_level.txt": b"build_finance\n",
        "pyproject.toml": b"[build-system]\n",
        "setup.cfg": b"[metadata]\n",
    }
    return ArchiveMembers("wheel", wheel_members), ArchiveMembers("sdist", sdist_members), expected


@pytest.mark.parametrize(
    ("archive_kind", "injected_name"),
    (
        ("wheel", "requests.py"),
        ("wheel", f"{PAPER_CORE_DIST_INFO}.data/scripts/send-order"),
        ("sdist", "setup.py"),
    ),
)
def test_complete_archive_closure_rejects_non_allowlisted_members(
    archive_kind: str,
    injected_name: str,
) -> None:
    wheel, sdist, expected = _otherwise_valid_members()
    target = wheel if archive_kind == "wheel" else sdist
    target.members[injected_name] = b"raise RuntimeError('must never execute')\n"

    with pytest.raises(VerificationError, match="archive allowlist mismatch"):
        _verify_complete_member_sets(wheel, sdist, expected)


def test_built_paper_core_includes_loader_and_excludes_execution_capability_members(tmp_path: Path) -> None:
    """Breaks if the dedicated artifact omits replay loading or gains live execution surfaces."""

    wheel_path, sdist_path = build_artifacts(tmp_path / "dist")
    archives = (_read_wheel_members(wheel_path), _read_sdist_members(sdist_path))
    forbidden_tokens = (
        "aiohttp",
        "alpaca",
        "autotrader",
        "binance",
        "broker",
        "ccxt",
        "coinbase",
        "credential",
        "ibapi",
        "kraken",
        "order-transport",
        "order_transport",
        "provider",
        "requests",
        "signer",
        "wallet",
        "web3",
        "websocket",
    )

    for archive in archives:
        assert "build_finance/paper_core_loader.py" in archive.members
        forbidden_members = sorted(
            name for name in archive.members if any(token in name.lower() for token in forbidden_tokens)
        )
        assert forbidden_members == []
    verify_artifacts(wheel=wheel_path, sdist=sdist_path)


def test_wheel_record_rejects_a_payload_hash_mismatch() -> None:
    wheel, _sdist, _expected = _otherwise_valid_members()
    wheel.members["build_finance/live_paper/kernel.py"] = b"tampered = True\n"

    with pytest.raises(VerificationError, match="RECORD digest mismatch"):
        _verify_wheel_record(wheel)


@pytest.mark.parametrize(
    "source",
    (
        "import importlib as loader\nloader.import_module('requests')\n",
        "from importlib import import_module as load\nload('requests')\n",
        "__builtins__.__import__('socket')\n",
        "from build_finance import broker\n",
        "from ... import broker\n",
        "import os as process\nvalue = process.environ\n",
        "import os\nprocess = os\nvalue = process.getenv('TOKEN')\n",
        "from os import environ as process_environment\nvalue = process_environment\n",
        "from os import getenv as read_env\nvalue = read_env('TOKEN')\n",
    ),
)
def test_artifact_ast_closure_rejects_dynamic_imports_aliases_and_environment(source: str) -> None:
    wheel = ArchiveMembers("wheel", {"build_finance/live_paper/escape.py": source.encode()})

    with pytest.raises(VerificationError):
        _verify_ast_closure(wheel)


def test_network_denial_fails_closed_at_udp_socket_construction() -> None:
    events: list[str] = []

    with deny_network(events), pytest.raises(NetworkDenied):
        socket.socket(socket.AF_INET, socket.SOCK_DGRAM)

    assert events == ["socket.socket"]


def test_network_denial_covers_socket_factory_surfaces() -> None:
    events: list[str] = []

    with deny_network(events), pytest.raises(NetworkDenied):
        socket.socketpair()

    assert events == ["socket.socketpair"]


def test_network_denied_missing_import_emits_canonical_summary(capsys: pytest.CaptureFixture[str]) -> None:
    exit_code = network_denied_main(["--import", "task13_module_that_does_not_exist"])

    summary = json.loads(capsys.readouterr().out)
    assert exit_code == 1
    assert summary["action"] == "import"
    assert summary["error_type"] == "ModuleNotFoundError"
    assert summary["exit_code"] == 1
    assert summary["network"] == "DENIED"
    assert summary["self_test"] == "PASS"


def test_network_denied_raising_import_emits_canonical_summary(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
) -> None:
    (tmp_path / "task13_raising_module.py").write_text("raise RuntimeError('synthetic import failure')\n")
    monkeypatch.syspath_prepend(str(tmp_path))

    exit_code = network_denied_main(["--import", "task13_raising_module"])

    summary = json.loads(capsys.readouterr().out)
    assert exit_code == 1
    assert summary["error"] == "synthetic import failure"
    assert summary["error_type"] == "RuntimeError"
    assert summary["exit_code"] == 1


def _write_json(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(json.dumps(value, separators=(",", ":"), sort_keys=True).encode() + b"\n")


def _prescribed_gate_commands() -> list[list[str]]:
    return [list(command) for command in PRESCRIBED_GATE_COMMANDS]


def test_gate_evidence_verifies_transcript_artifacts_and_derived_promotion(tmp_path: Path) -> None:
    implementation_sha = "a" * 40
    wheel = tmp_path / ".artifacts/paper-core/dist/paper.whl"
    sdist = tmp_path / ".artifacts/paper-core/dist/paper.tar.gz"
    wheel.parent.mkdir(parents=True)
    wheel.write_bytes(b"wheel")
    sdist.write_bytes(b"sdist")
    transcript_path = tmp_path / "docs/live-paper/evidence/G2-command-transcript.json"
    receipt_path = tmp_path / "docs/live-paper/evidence/G2-green.json"
    promotion_path = tmp_path / "docs/live-paper/promotion-status.json"
    outputs = [f"gate {index} passed\n" for index, _command in enumerate(_prescribed_gate_commands())]
    transcript = {
        "commands": [
            {
                "command_args": command,
                "exit_code": 0,
                "output": output,
                "stdout_sha256": hashlib.sha256(output.encode()).hexdigest(),
            }
            for command, output in zip(_prescribed_gate_commands(), outputs, strict=True)
        ],
        "implementation_sha": implementation_sha,
        "schema": "build-finance.live-paper.g2-command-transcript/v1",
    }
    _write_json(transcript_path, transcript)
    promotion = _derived_promotion(implementation_sha, "docs/live-paper/evidence/G2-green.json")
    receipt = {
        "artifacts": {
            "sdist": {
                "path": ".artifacts/paper-core/dist/paper.tar.gz",
                "sha256": hashlib.sha256(b"sdist").hexdigest(),
            },
            "wheel": {
                "path": ".artifacts/paper-core/dist/paper.whl",
                "sha256": hashlib.sha256(b"wheel").hexdigest(),
            },
        },
        "commands": [
            {
                "command_args": command,
                "exit_code": 0,
                "stdout_sha256": hashlib.sha256(output.encode()).hexdigest(),
            }
            for command, output in zip(_prescribed_gate_commands(), outputs, strict=True)
        ],
        "implementation_sha": implementation_sha,
        "paper_core_manifest_sha256": "b" * 64,
        "promotion_status": promotion,
        "schema": "build-finance.live-paper.g2-gate-receipt/v2",
        "status": "GREEN",
        "transcript": {
            "path": "docs/live-paper/evidence/G2-command-transcript.json",
            "sha256": hashlib.sha256(transcript_path.read_bytes()).hexdigest(),
        },
    }
    _write_json(receipt_path, receipt)
    _write_json(promotion_path, promotion)

    verify_gate_evidence(
        receipt_path=receipt_path,
        promotion_path=promotion_path,
        transcript_path=transcript_path,
        wheel=wheel,
        sdist=sdist,
        manifest_sha256="b" * 64,
        repo_root=tmp_path,
        verify_git=False,
    )

    valid_transcript = json.loads(json.dumps(transcript))
    valid_receipt = json.loads(json.dumps(receipt))
    arbitrary_output = "arbitrary command passed\n"
    transcript["commands"] = [
        {
            "command_args": ["python", "-c", "print('not the gate')"],
            "exit_code": 0,
            "output": arbitrary_output,
            "stdout_sha256": hashlib.sha256(arbitrary_output.encode()).hexdigest(),
        }
    ]
    _write_json(transcript_path, transcript)
    receipt["commands"] = [
        {
            "command_args": transcript["commands"][0]["command_args"],
            "exit_code": 0,
            "stdout_sha256": transcript["commands"][0]["stdout_sha256"],
        }
    ]
    receipt["transcript"]["sha256"] = hashlib.sha256(transcript_path.read_bytes()).hexdigest()
    _write_json(receipt_path, receipt)
    with pytest.raises(VerificationError, match="prescribed Task 13 command list"):
        verify_gate_evidence(
            receipt_path=receipt_path,
            promotion_path=promotion_path,
            transcript_path=transcript_path,
            wheel=wheel,
            sdist=sdist,
            manifest_sha256="b" * 64,
            repo_root=tmp_path,
            verify_git=False,
        )

    transcript = valid_transcript
    receipt = valid_receipt
    _write_json(transcript_path, transcript)
    _write_json(receipt_path, receipt)

    promotion["g2"] = "RED"
    _write_json(promotion_path, promotion)
    with pytest.raises(VerificationError, match="derived gate state"):
        verify_gate_evidence(
            receipt_path=receipt_path,
            promotion_path=promotion_path,
            transcript_path=transcript_path,
            wheel=wheel,
            sdist=sdist,
            manifest_sha256="b" * 64,
            repo_root=tmp_path,
            verify_git=False,
        )

    _write_json(promotion_path, receipt["promotion_status"])
    wheel.write_bytes(b"stale wheel")
    with pytest.raises(VerificationError, match="artifact paths or digests are stale"):
        verify_gate_evidence(
            receipt_path=receipt_path,
            promotion_path=promotion_path,
            transcript_path=transcript_path,
            wheel=wheel,
            sdist=sdist,
            manifest_sha256="b" * 64,
            repo_root=tmp_path,
            verify_git=False,
        )


def test_derived_promotion_stays_fail_closed_without_authorization() -> None:
    promotion = _derived_promotion("a" * 40, "docs/live-paper/evidence/G2-green.json")

    assert promotion["publication"] == "BLOCKED"
    assert promotion["next_authorized_node"] is None
    assert "authorization" not in promotion
    assert promotion["paper_only"] is True


def _authorization_record(implementation_sha: str) -> dict[str, str]:
    return {
        "authorized_at": "2026-08-22T00:00:00+00:00",
        "authorized_by": "operator",
        "implementation_sha": implementation_sha,
        "node": "test-node",
    }


def test_derived_promotion_records_complete_bound_authorization() -> None:
    implementation_sha = "a" * 40
    promotion = _derived_promotion(
        implementation_sha,
        "docs/live-paper/evidence/G2-green.json",
        _authorization_record(implementation_sha),
    )

    assert promotion["publication"] == "AUTHORIZED"
    assert promotion["next_authorized_node"] == "test-node"
    assert promotion["authorization"]["authorized_by"] == "operator"


def test_derived_promotion_rejects_mismatched_authorization_binding() -> None:
    with pytest.raises(VerificationError, match="different implementation"):
        _derived_promotion(
            "a" * 40,
            "docs/live-paper/evidence/G2-green.json",
            _authorization_record("b" * 40),
        )

    incomplete = _authorization_record("a" * 40)
    del incomplete["node"]
    with pytest.raises(VerificationError, match="authorization fields are invalid"):
        _derived_promotion(
            "a" * 40,
            "docs/live-paper/evidence/G2-green.json",
            incomplete,
        )


def test_gate_evidence_verifies_authorized_promotion_against_receipt(tmp_path: Path) -> None:
    implementation_sha = "c" * 40
    wheel = tmp_path / ".artifacts/paper-core/dist/paper.whl"
    sdist = tmp_path / ".artifacts/paper-core/dist/paper.tar.gz"
    wheel.parent.mkdir(parents=True)
    wheel.write_bytes(b"wheel")
    sdist.write_bytes(b"sdist")
    transcript_path = tmp_path / "docs/live-paper/evidence/G2-command-transcript.json"
    receipt_path = tmp_path / "docs/live-paper/evidence/G2-green.json"
    promotion_path = tmp_path / "docs/live-paper/promotion-status.json"
    outputs = [f"gate {index} passed\n" for index, _command in enumerate(_prescribed_gate_commands())]
    transcript = {
        "commands": [
            {
                "command_args": command,
                "exit_code": 0,
                "output": output,
                "stdout_sha256": hashlib.sha256(output.encode()).hexdigest(),
            }
            for command, output in zip(_prescribed_gate_commands(), outputs, strict=True)
        ],
        "implementation_sha": implementation_sha,
        "schema": "build-finance.live-paper.g2-command-transcript/v1",
    }
    _write_json(transcript_path, transcript)
    promotion = _derived_promotion(
        implementation_sha,
        "docs/live-paper/evidence/G2-green.json",
        _authorization_record(implementation_sha),
    )
    receipt = {
        "artifacts": {
            "sdist": {
                "path": ".artifacts/paper-core/dist/paper.tar.gz",
                "sha256": hashlib.sha256(b"sdist").hexdigest(),
            },
            "wheel": {
                "path": ".artifacts/paper-core/dist/paper.whl",
                "sha256": hashlib.sha256(b"wheel").hexdigest(),
            },
        },
        "commands": [
            {
                "command_args": command,
                "exit_code": 0,
                "stdout_sha256": hashlib.sha256(output.encode()).hexdigest(),
            }
            for command, output in zip(_prescribed_gate_commands(), outputs, strict=True)
        ],
        "implementation_sha": implementation_sha,
        "paper_core_manifest_sha256": "b" * 64,
        "promotion_status": promotion,
        "schema": "build-finance.live-paper.g2-gate-receipt/v2",
        "status": "GREEN",
        "transcript": {
            "path": "docs/live-paper/evidence/G2-command-transcript.json",
            "sha256": hashlib.sha256(transcript_path.read_bytes()).hexdigest(),
        },
    }
    _write_json(receipt_path, receipt)
    _write_json(promotion_path, promotion)

    verify_gate_evidence(
        receipt_path=receipt_path,
        promotion_path=promotion_path,
        transcript_path=transcript_path,
        wheel=wheel,
        sdist=sdist,
        manifest_sha256="b" * 64,
        repo_root=tmp_path,
        verify_git=False,
    )

    tampered = json.loads(json.dumps(promotion))
    tampered["authorization"]["authorized_by"] = "someone-else"
    _write_json(promotion_path, tampered)
    with pytest.raises(VerificationError, match="derived gate state"):
        verify_gate_evidence(
            receipt_path=receipt_path,
            promotion_path=promotion_path,
            transcript_path=transcript_path,
            wheel=wheel,
            sdist=sdist,
            manifest_sha256="b" * 64,
            repo_root=tmp_path,
            verify_git=False,
        )
