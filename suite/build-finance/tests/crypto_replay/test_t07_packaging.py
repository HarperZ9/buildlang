"""Task 7 packaging and promotion-state contracts for crypto replay."""

from __future__ import annotations

import io
import json
import tarfile
import zipfile
from pathlib import Path

import pytest

try:
    import tomllib
except ModuleNotFoundError:  # Python 3.10 test support only.
    import tomli as tomllib

ROOT = Path(__file__).resolve().parents[2]


def test_promotion_status_records_honest_blocked_replay_state() -> None:
    """Catches accidental promotion/profitability claims in the machine-readable gate."""

    status = json.loads((ROOT / "docs" / "crypto-replay" / "promotion-status.json").read_text(encoding="utf-8"))

    assert status["p0"] == "BLOCKED"
    assert status["p1"] == "PASS"
    assert status["p2"] == "FAIL_ZERO_ADMITTED_FIXTURE"
    assert status["p5"] == "PASS"
    assert status["replay_subpackage_confinement"] == "PASS"
    assert status["real_fixture_manifest_sha256"] is None
    assert status["next_authorized_node"] is None

    receipt_path = ROOT / status["p5_receipt"]
    receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
    assert receipt["receipt"] == "crypto-replay-p5-whole-repository"
    assert receipt["outcome"] == "pass"
    assert receipt["does_not_prove"] == [
        "real fixture admission",
        "T04 or live/paper engine behavior",
        "profitability",
        "publication approval",
        "trading authority",
    ]
    command_results = {command["name"]: command["result"] for command in receipt["commands"]}
    expected_results = {
        "ruff check": "PASS",
        "ruff format check": "PASS",
        "mypy": "PASS",
        "pytest full repository": "PASS",
        "exact artifact verification": "PASS",
        "exact offline wheel import": "PASS",
    }
    assert expected_results.items() <= command_results.items()
    assert command_results["linux crypto replay package slice"] == "PASS"


def test_project_metadata_keeps_replay_runtime_dependencies_offline() -> None:
    """Catches package metadata drift that adds replay-relevant provider/broker deps."""

    pyproject = tomllib.loads((ROOT / "pyproject.toml").read_text(encoding="utf-8"))
    project = pyproject["project"]

    assert project["dependencies"] == ["numpy>=1.24", "pandas>=2.0", "scipy>=1.10"]
    optional_dependencies = project["optional-dependencies"]
    assert optional_dependencies["test"] == [
        "pytest>=8.0",
        "pytest-cov>=5",
        "jsonschema>=4.23,<5",
        "tomli>=2; python_version < '3.11'",
    ]
    assert optional_dependencies["dev"] == [
        "pytest>=8.0",
        "pytest-cov>=5",
        "jsonschema>=4.23,<5",
        "tomli>=2; python_version < '3.11'",
        "ruff>=0.6",
        "mypy>=1.10",
        "build>=1.2",
    ]

    all_requirements = "\n".join((*project["dependencies"], *sum(optional_dependencies.values(), []))).lower()
    for forbidden in ("alpaca", "anchorpy", "binance", "ccxt", "coinbase", "jupiter", "kraken", "solana", "web3"):
        assert forbidden not in all_requirements


def test_crypto_replay_package_data_declares_authority_resources() -> None:
    """Catches wheels that drop generated schemas, bundle digests, or formula resources."""

    pyproject = tomllib.loads((ROOT / "pyproject.toml").read_text(encoding="utf-8"))

    assert pyproject["tool"]["setuptools"]["package-data"]["build_finance.crypto_replay"] == [
        "resources/*.json",
        "resources/*.sha256",
        "resources/schemas/*.json",
        "resources/formulas/*.txt",
    ]


def test_artifact_verifier_rejects_archives_without_replay_contracts(tmp_path: Path) -> None:
    """Catches a verifier that merely reports bad archives instead of failing closed."""

    from scripts.verify_crypto_replay_artifacts import VerificationError, verify_artifacts

    wheel = tmp_path / "build_finance-1.0.1-py3-none-any.whl"
    with zipfile.ZipFile(wheel, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        archive.writestr(
            "build_finance-1.0.1.dist-info/METADATA",
            "\n".join(
                (
                    "Metadata-Version: 2.1",
                    "Name: build-finance",
                    "Version: 1.0.1",
                    "Requires-Dist: numpy>=1.24",
                    "Requires-Dist: pandas>=2.0",
                    "Requires-Dist: scipy>=1.10",
                    'Requires-Dist: jsonschema<5,>=4.23; extra == "test"',
                    'Requires-Dist: jsonschema<5,>=4.23; extra == "dev"',
                    "",
                )
            ),
        )
        archive.writestr("build_finance/__init__.py", "")

    sdist = tmp_path / "build_finance-1.0.1.tar.gz"
    with tarfile.open(sdist, "w:gz") as archive:
        metadata = b"Metadata-Version: 2.1\nName: build-finance\nVersion: 1.0.1\n"
        info = tarfile.TarInfo("build_finance-1.0.1/PKG-INFO")
        info.size = len(metadata)
        archive.addfile(info, fileobj=__import__("io").BytesIO(metadata))

    with pytest.raises(VerificationError, match="wheel missing required member"):
        verify_artifacts(wheel=wheel, sdist=sdist)


def _write_probe_wheel(path: Path, members: dict[str, bytes]) -> Path:
    with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for name, payload in members.items():
            archive.writestr(name, payload)
    return path


def _write_probe_sdist(path: Path, members: dict[str, bytes]) -> Path:
    with tarfile.open(path, "w:gz") as archive:
        for name, payload in members.items():
            info = tarfile.TarInfo(f"build_finance-1.0.1/{name}")
            info.size = len(payload)
            archive.addfile(info, fileobj=io.BytesIO(payload))
    return path


@pytest.mark.parametrize(
    ("archive_kind", "reader_name", "writer_name", "archive_name"),
    (
        ("wheel", "_read_wheel_members", "_write_probe_wheel", "budget.whl"),
        ("sdist", "_read_sdist_members", "_write_probe_sdist", "budget.tar.gz"),
    ),
)
def test_artifact_reader_rejects_member_count_budget_before_payload_reads(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    archive_kind: str,
    reader_name: str,
    writer_name: str,
    archive_name: str,
) -> None:
    """Catches archive readers that ignore declared member-count budgets."""

    from scripts import verify_crypto_replay_artifacts as verifier

    monkeypatch.setattr(verifier, "MAX_ARCHIVE_MEMBER_COUNT", 1, raising=False)
    writer = globals()[writer_name]
    archive = writer(tmp_path / archive_name, {"one.txt": b"1", "two.txt": b"2"})
    reader = getattr(verifier, reader_name)

    with pytest.raises(verifier.VerificationError, match=f"{archive_kind} member count exceeds"):
        reader(archive)


@pytest.mark.parametrize(
    ("archive_kind", "reader_name", "writer_name", "archive_name"),
    (
        ("wheel", "_read_wheel_members", "_write_probe_wheel", "member-size.whl"),
        ("sdist", "_read_sdist_members", "_write_probe_sdist", "member-size.tar.gz"),
    ),
)
def test_artifact_reader_rejects_individual_uncompressed_size_budget(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    archive_kind: str,
    reader_name: str,
    writer_name: str,
    archive_name: str,
) -> None:
    """Catches archive readers that allocate a member larger than the configured budget."""

    from scripts import verify_crypto_replay_artifacts as verifier

    monkeypatch.setattr(verifier, "MAX_ARCHIVE_MEMBER_SIZE", 2, raising=False)
    writer = globals()[writer_name]
    archive = writer(tmp_path / archive_name, {"oversized.txt": b"123"})
    reader = getattr(verifier, reader_name)

    with pytest.raises(verifier.VerificationError, match=f"{archive_kind} member exceeds"):
        reader(archive)


@pytest.mark.parametrize(
    ("archive_kind", "reader_name", "writer_name", "archive_name"),
    (
        ("wheel", "_read_wheel_members", "_write_probe_wheel", "aggregate.whl"),
        ("sdist", "_read_sdist_members", "_write_probe_sdist", "aggregate.tar.gz"),
    ),
)
def test_artifact_reader_rejects_aggregate_uncompressed_size_budget(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    archive_kind: str,
    reader_name: str,
    writer_name: str,
    archive_name: str,
) -> None:
    """Catches archive readers that do not cap total uncompressed bytes."""

    from scripts import verify_crypto_replay_artifacts as verifier

    monkeypatch.setattr(verifier, "MAX_ARCHIVE_TOTAL_SIZE", 5, raising=False)
    writer = globals()[writer_name]
    archive = writer(tmp_path / archive_name, {"left.txt": b"123", "right.txt": b"456"})
    reader = getattr(verifier, reader_name)

    with pytest.raises(verifier.VerificationError, match=f"{archive_kind} aggregate uncompressed size exceeds"):
        reader(archive)


def test_artifact_reader_still_rejects_non_regular_sdist_members(tmp_path: Path) -> None:
    """Catches tar hardening changes that accidentally allow links or devices."""

    from scripts.verify_crypto_replay_artifacts import VerificationError, _read_sdist_members

    sdist = tmp_path / "non-regular.tar.gz"
    with tarfile.open(sdist, "w:gz") as archive:
        info = tarfile.TarInfo("build_finance-1.0.1/link")
        info.type = tarfile.SYMTYPE
        info.linkname = "target"
        archive.addfile(info)

    with pytest.raises(VerificationError, match="sdist contains non-regular member"):
        _read_sdist_members(sdist)
