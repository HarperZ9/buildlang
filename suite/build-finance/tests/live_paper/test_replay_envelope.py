"""Disk replay-envelope tests for the G2 offline paper core."""

from __future__ import annotations

import hashlib
import importlib
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Any

import pytest

from build_finance.crypto_replay.admission import admit_local_fixture
from build_finance.crypto_replay.canonical import canonical_record_bytes, parse_canonical_record
from build_finance.crypto_replay.content_ids import seal_content_id
from build_finance.crypto_replay.local_fixture import capture_local_fixture
from build_finance.live_paper.kernel import run_offline_paper_kernel
from build_finance.live_paper.run_input_builder import build_verified_run_inputs
from tests.live_paper.support.g2_disk_bundle import (
    corrupt_first_byte_payload,
    corrupt_first_record_payload,
    replace_first_byte_with_symlink,
    rewrite_envelope_mode,
    write_g2_disk_bundle,
)


def _checksum_rows(payload: bytes) -> list[tuple[str, str]]:
    rows: list[tuple[str, str]] = []
    for line in payload.decode("utf-8").splitlines():
        digest, relative_path = line.split("  ", 1)
        assert len(digest) == 64
        rows.append((digest, relative_path))
    return rows


def _paper_core_loader() -> Any:
    try:
        return importlib.import_module("build_finance.paper_core_loader")
    except ModuleNotFoundError as error:
        if error.name != "build_finance.paper_core_loader":
            raise
        pytest.fail(f"public paper-core replay loader is absent: {error}")


def _replace_directory_with_link(link: Path, target: Path) -> None:
    shutil.rmtree(link)
    if os.name == "nt":
        completed = subprocess.run(
            ["cmd", "/d", "/c", "mklink", "/J", os.fspath(link), os.fspath(target)],
            check=False,
            capture_output=True,
            text=True,
        )
        if completed.returncode != 0:
            pytest.skip(f"Windows junction creation unavailable: {completed.stderr.strip()}")
        return
    try:
        os.symlink(target, link, target_is_directory=True)
    except OSError as error:
        pytest.skip(f"directory symlink creation unavailable: {type(error).__name__}: {error}")


def test_replay_envelope_loader_builds_positive_g2_context(tmp_path: Path) -> None:
    """Breaks if the disk envelope cannot feed production admission, verification, and kernel replay."""

    bundle = write_g2_disk_bundle(tmp_path)
    captured = capture_local_fixture(bundle.fixture_root)
    admission = admit_local_fixture(captured)
    assert admission.status == "ADMITTED"
    assert admission.reason_codes == ()
    assert all(not file.relative_path.startswith("replay-envelope/") for file in captured.files)

    loader = _paper_core_loader()
    context = loader.load_replay_envelope_context(bundle.fixture_root, captured, admission)
    verified = build_verified_run_inputs(
        context.run_receipt_record,
        admission.candidates,
        admission.source_receipt_records,
        context.resolver,
        context.profiles,
    )
    result = run_offline_paper_kernel(verified, context.resolver, context.profiles)

    assert result.closure.status == "CLOSED"
    assert result.closure.reason_codes == ()
    assert tuple(row.disposition for row in result.projection.model_validation) == ("ABSTAIN", "ABSTAIN")
    assert tuple(row.reason_code for row in result.projection.model_validation) == (
        "MODEL_DISABLED",
        "MODEL_DISABLED",
    )


def test_g2_evaluation_export_is_reproducible_and_contains_positive_projection(tmp_path: Path) -> None:
    """Breaks if the synthetic exporter varies bytes, reaches a provider, or omits the positive replay."""

    from scripts.export_g2_evaluation_bundle import export_g2_evaluation_bundle
    from scripts.run_network_denied import deny_network

    blocked_calls: list[str] = []
    with deny_network(blocked_calls):
        first = export_g2_evaluation_bundle(tmp_path / "first")
        second = export_g2_evaluation_bundle(tmp_path / "second")

    assert blocked_calls == []
    first_checksums = first.checksum_manifest.read_bytes()
    assert first_checksums == second.checksum_manifest.read_bytes()
    checksum_rows = _checksum_rows(first_checksums)
    relative_paths = [relative_path for _digest, relative_path in checksum_rows]
    assert relative_paths == sorted(relative_paths)
    assert "SHA256SUMS" not in relative_paths
    for digest, relative_path in checksum_rows:
        assert hashlib.sha256(first.destination.joinpath(relative_path).read_bytes()).hexdigest() == digest

    projection_bytes = first.kernel_projection.read_bytes()
    assert projection_bytes == second.kernel_projection.read_bytes()
    projection = parse_canonical_record(projection_bytes)
    assert projection["data_classification"] == "SYNTHETIC"
    assert projection["execution"] == "SIMULATED"
    assert projection["mode"] == "PAPER_ONLY"
    assert projection["profitability_claim"] is False
    assert projection["closure_status"] == "CLOSED"
    assert [row["disposition"] for row in projection["projection"]["model_validation"]] == [
        "ABSTAIN",
        "ABSTAIN",
    ]
    assert [row["reason_code"] for row in projection["projection"]["model_validation"]] == [
        "MODEL_DISABLED",
        "MODEL_DISABLED",
    ]


def test_g2_evaluation_export_refuses_a_non_empty_destination(tmp_path: Path) -> None:
    """Breaks if export can overwrite or mingle with material outside its owned empty directory."""

    from scripts.export_g2_evaluation_bundle import ExportError, export_g2_evaluation_bundle

    destination = tmp_path / "occupied"
    destination.mkdir()
    marker = destination / "keep.txt"
    marker.write_text("user-owned\n", encoding="utf-8")

    with pytest.raises(ExportError, match="empty"):
        export_g2_evaluation_bundle(destination)

    assert marker.read_text(encoding="utf-8") == "user-owned\n"


def test_g2_evaluation_export_refuses_a_file_symlink_destination(tmp_path: Path) -> None:
    """Breaks if a linked destination can redirect export writes outside the owned path."""

    from scripts.export_g2_evaluation_bundle import ExportError, export_g2_evaluation_bundle

    target = tmp_path / "outside.txt"
    target.write_text("user-owned\n", encoding="utf-8")
    destination = tmp_path / "linked-export"
    try:
        destination.symlink_to(target)
    except OSError as error:
        pytest.skip(f"file symlink creation unavailable: {type(error).__name__}: {error}")

    with pytest.raises(ExportError):
        export_g2_evaluation_bundle(destination)

    assert target.read_text(encoding="utf-8") == "user-owned\n"


@pytest.mark.skipif(os.name != "nt", reason="Windows directory-junction regression")
def test_g2_evaluation_export_refuses_an_empty_directory_junction(tmp_path: Path) -> None:
    """Breaks if a Windows reparse directory can redirect an apparently empty export."""

    from scripts.export_g2_evaluation_bundle import ExportError, export_g2_evaluation_bundle

    destination = tmp_path / "junction-export"
    destination.mkdir()
    outside = tmp_path / "outside"
    outside.mkdir()
    _replace_directory_with_link(destination, outside)

    with pytest.raises(ExportError, match="link|reparse"):
        export_g2_evaluation_bundle(destination)

    assert list(outside.iterdir()) == []


@pytest.mark.parametrize(
    "mutation",
    ("missing_row", "unexpected_file"),
)
def test_g2_evaluation_checksum_verifier_requires_exact_file_set(tmp_path: Path, mutation: str) -> None:
    """Breaks if verification trusts an incomplete manifest or ignores an unsealed file."""

    from scripts.export_g2_evaluation_bundle import (
        ExportError,
        export_g2_evaluation_bundle,
        verify_g2_evaluation_bundle,
    )

    exported = export_g2_evaluation_bundle(tmp_path / mutation)
    if mutation == "missing_row":
        rows = exported.checksum_manifest.read_bytes().splitlines(keepends=True)
        exported.checksum_manifest.write_bytes(b"".join(rows[1:]))
    else:
        exported.destination.joinpath("unexpected.txt").write_text("not sealed\n", encoding="utf-8")

    with pytest.raises(ExportError, match="file set"):
        verify_g2_evaluation_bundle(exported.destination)


@pytest.mark.parametrize(
    ("limit_name", "protected_kind"),
    (
        ("_MAX_CHECKSUM_MANIFEST_BYTES", "manifest"),
        ("_MAX_FILE_BYTES", "member"),
    ),
)
def test_g2_evaluation_verifier_rejects_oversized_inputs_before_path_read(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    limit_name: str,
    protected_kind: str,
) -> None:
    """Breaks if an oversized manifest or member reaches an unbounded path-based allocation."""

    import scripts.export_g2_evaluation_bundle as exporter

    exported = exporter.export_g2_evaluation_bundle(tmp_path / protected_kind)
    original_read_bytes = Path.read_bytes

    def reject_unbounded_read(path: Path) -> bytes:
        is_manifest = path == exported.checksum_manifest
        if (protected_kind == "manifest" and is_manifest) or (
            protected_kind == "member" and path.is_relative_to(exported.destination) and not is_manifest
        ):
            pytest.fail(f"oversized {protected_kind} reached Path.read_bytes")
        return original_read_bytes(path)

    monkeypatch.setattr(exporter, limit_name, 1, raising=False)
    monkeypatch.setattr(Path, "read_bytes", reject_unbounded_read)

    with pytest.raises(exporter.ExportError, match="exceeds the bounded read size"):
        exporter.verify_g2_evaluation_bundle(exported.destination)


@pytest.mark.parametrize(
    ("limit_name", "expected_fragment"),
    (
        ("_MAX_BUNDLE_FILE_COUNT", "file count"),
        ("_MAX_BUNDLE_BYTES", "total bytes"),
    ),
)
def test_g2_evaluation_verifier_enforces_small_bundle_resource_limits(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    limit_name: str,
    expected_fragment: str,
) -> None:
    """Breaks if file-count or total-byte ceilings require a large exhaustion fixture to exercise."""

    import scripts.export_g2_evaluation_bundle as exporter

    exported = exporter.export_g2_evaluation_bundle(tmp_path / expected_fragment.replace(" ", "-"))
    monkeypatch.setattr(exporter, limit_name, 1, raising=False)

    with pytest.raises(exporter.ExportError, match=expected_fragment):
        exporter.verify_g2_evaluation_bundle(exported.destination)


def test_g2_evaluation_verifier_rejects_parent_link_swap_before_member_open(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Breaks if classification can be followed by opening a member through a swapped linked parent."""

    import scripts.export_g2_evaluation_bundle as exporter

    exported = exporter.export_g2_evaluation_bundle(tmp_path / "parent-swap")
    profiles_root = exported.fixture_root / "replay-envelope" / "profiles"
    member = profiles_root / "algorithm-0.bin"
    outside_profiles = tmp_path / "outside-profiles"
    shutil.copytree(profiles_root, outside_profiles)
    outside_member = outside_profiles / member.name
    outside_member.unlink()
    try:
        os.link(member, outside_member)
    except OSError as error:
        pytest.skip(f"hard-link creation unavailable: {type(error).__name__}: {error}")

    monkeypatch.setattr(exporter, "_can_use_openat", lambda: False, raising=False)
    original_open = exporter.os.open
    member_key = os.path.normcase(os.path.abspath(member))
    swapped = False

    def swap_parent_before_open(path: Any, *args: Any, **kwargs: Any) -> Any:
        nonlocal swapped
        if not swapped and isinstance(path, (str, bytes, os.PathLike)):
            candidate = os.path.normcase(os.path.abspath(os.fsdecode(path)))
            if candidate == member_key:
                _replace_directory_with_link(profiles_root, outside_profiles)
                swapped = True
        return original_open(path, *args, **kwargs)

    monkeypatch.setattr(exporter.os, "open", swap_parent_before_open)
    with pytest.raises(exporter.ExportError, match="prefix|link|reparse|identity"):
        exporter.verify_g2_evaluation_bundle(exported.destination)
    assert swapped


@pytest.mark.parametrize(
    "model_rows",
    (
        [
            {"disposition": "ABSTAIN", "reason_code": "WRONG_REASON"},
            {"disposition": "ABSTAIN", "reason_code": "MODEL_DISABLED"},
        ],
        [
            {"disposition": "ABSTAIN", "reason_code": "MODEL_DISABLED"},
            "not-a-mapping",
        ],
    ),
)
def test_g2_projection_seal_requires_exact_disabled_model_rows(model_rows: list[object]) -> None:
    """Breaks if DISABLED_ABSTAIN can be claimed from wrong reasons or non-mapping rows."""

    from scripts.export_g2_evaluation_bundle import ExportError, _projection_document

    result_payload = {
        "closure": {"reason_codes": [], "status": "CLOSED"},
        "projection": {"model_validation": model_rows},
    }

    with pytest.raises(ExportError, match="model abstention"):
        _projection_document(result_payload)  # type: ignore[arg-type]


def test_g2_evaluation_export_cli_loads_the_source_checkout(tmp_path: Path) -> None:
    """Breaks if direct script execution resolves an unrelated installed Build Finance package."""

    root = Path(__file__).resolve().parents[2]
    destination = tmp_path / "cli-export"
    poison = tmp_path / "poison"
    poison_package = poison / "build_finance"
    poison_package.mkdir(parents=True)
    poison_marker = tmp_path / "poison-imported.txt"
    poison_package.joinpath("__init__.py").write_text(
        "from pathlib import Path\n"
        f"Path({os.fspath(poison_marker)!r}).write_text('poisoned\\n', encoding='utf-8')\n"
        "raise RuntimeError('poisoned build_finance imported')\n",
        encoding="utf-8",
    )
    environment = os.environ.copy()
    environment["PYTHONPATH"] = os.pathsep.join((os.fspath(poison), os.fspath(root)))
    completed = subprocess.run(
        [sys.executable, "scripts/export_g2_evaluation_bundle.py", str(destination)],
        cwd=root,
        env=environment,
        check=False,
        capture_output=True,
        text=True,
    )

    assert completed.returncode == 0, completed.stderr
    summary = json.loads(completed.stdout)
    assert Path(summary["build_finance_root"]).resolve() == root / "build_finance"
    assert not poison_marker.exists()
    assert destination.joinpath("SHA256SUMS").is_file()
    assert destination.joinpath("kernel-projection.json").is_file()


def test_replay_envelope_loader_rejects_admission_from_another_captured_fixture(tmp_path: Path) -> None:
    """Breaks if an admitted batch can be cross-wired to a different captured fixture and run."""

    bundle = write_g2_disk_bundle(tmp_path / "primary")
    captured = capture_local_fixture(bundle.fixture_root)

    foreign_bundle = write_g2_disk_bundle(tmp_path / "foreign")
    foreign_manifest_path = foreign_bundle.fixture_root / "manifest.json"
    foreign_manifest = parse_canonical_record(foreign_manifest_path.read_bytes())
    foreign_manifest.pop("fixture_manifest_sha256")
    foreign_manifest["initial_quote_atoms"] = "2000000"
    foreign_manifest_path.write_bytes(canonical_record_bytes(seal_content_id(foreign_manifest)))
    foreign_captured = capture_local_fixture(foreign_bundle.fixture_root)
    foreign_admission = admit_local_fixture(foreign_captured)

    assert foreign_admission.status == "ADMITTED"
    assert foreign_admission.reason_codes == ()
    assert captured.fixture_manifest_sha256 != foreign_captured.fixture_manifest_sha256

    loader = _paper_core_loader()
    with pytest.raises(ValueError, match="captured fixture"):
        loader.load_replay_envelope_context(bundle.fixture_root, captured, foreign_admission)


def test_replay_envelope_member_read_rejects_parent_replacement_before_final_open(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Breaks if a fallback path read accepts a replaced prefix with the same final file identity."""

    bundle = write_g2_disk_bundle(tmp_path)
    captured = capture_local_fixture(bundle.fixture_root)
    admission = admit_local_fixture(captured)
    assert admission.status == "ADMITTED"

    loader = _paper_core_loader()
    monkeypatch.setattr(loader, "_can_use_openat", lambda: False, raising=False)
    profiles_root = bundle.fixture_root / "replay-envelope" / "profiles"
    fill_profile = profiles_root / "fill.bin"
    outside_profiles = tmp_path / "outside-profiles"
    outside_profiles.mkdir()
    os.link(fill_profile, outside_profiles / "fill.bin")

    original_open = loader.os.open
    fill_path = os.path.normcase(os.path.abspath(fill_profile))
    replaced = False

    def swap_parent_before_final_open(path: Any, *args: Any, **kwargs: Any) -> Any:
        nonlocal replaced
        candidate = os.path.normcase(os.path.abspath(os.fspath(path)))
        if not replaced and candidate == fill_path:
            _replace_directory_with_link(profiles_root, outside_profiles)
            replaced = True
        return original_open(path, *args, **kwargs)

    monkeypatch.setattr(loader.os, "open", swap_parent_before_final_open)
    with pytest.raises(OSError, match="path prefix identity changed"):
        loader.load_replay_envelope_context(bundle.fixture_root, captured, admission)
    assert replaced


@pytest.mark.parametrize(
    ("case", "expected_fragment"),
    (
        ("digest_mismatched_bytes", "digest-addressed bytes"),
        ("content_mismatched_record", "self-addressed record"),
        ("linked_envelope_member", "link"),
        ("model_mode_enabled", "replay envelope"),
    ),
)
def test_replay_envelope_loader_fails_closed_before_kernel(
    tmp_path: Path,
    case: str,
    expected_fragment: str,
) -> None:
    """Breaks if malformed disk authority can reach kernel execution."""

    bundle = write_g2_disk_bundle(tmp_path)
    if case == "digest_mismatched_bytes":
        corrupt_first_byte_payload(bundle)
    elif case == "content_mismatched_record":
        corrupt_first_record_payload(bundle)
    elif case == "linked_envelope_member":
        if not replace_first_byte_with_symlink(bundle, tmp_path):
            pytest.skip("symlink/reparse replacement is not supported by this filesystem")
    else:
        rewrite_envelope_mode(bundle, "CACHED_FIXTURES")

    captured = capture_local_fixture(bundle.fixture_root)
    admission = admit_local_fixture(captured)
    assert admission.status == "ADMITTED"
    loader = _paper_core_loader()

    with pytest.raises((KeyError, OSError, ValueError)) as failure:
        context = loader.load_replay_envelope_context(bundle.fixture_root, captured, admission)
        build_verified_run_inputs(
            context.run_receipt_record,
            admission.candidates,
            admission.source_receipt_records,
            context.resolver,
            context.profiles,
        )

    assert expected_fragment in str(failure.value)
