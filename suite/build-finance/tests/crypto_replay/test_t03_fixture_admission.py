"""T03 SYNTHETIC local fixture admission tests."""

from __future__ import annotations

import ast
import dataclasses
import hashlib
import importlib.util
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Any

import pytest

from build_finance.crypto_replay.canonical import (
    canonical_json_bytes,
    canonical_record_bytes,
    parse_canonical_record,
    sha256_hex,
)
from tests.crypto_replay.support.synthetic_local_fixture import (
    SYNTHETIC_BASE_MINT,
    SYNTHETIC_EVENT_TIME,
    SYNTHETIC_INGESTED_AT,
    SYNTHETIC_OBSERVED_AT,
    SYNTHETIC_PAYLOAD_BASE_SENTINEL,
    SYNTHETIC_PAYLOAD_SOURCE_SENTINEL,
    SYNTHETIC_SOURCE_ID,
    SYNTHETIC_TERMS_PREFIX,
    SYNTHETICEventSpec,
    SYNTHETICLocalFixture,
    reseal_SYNTHETIC_manifest,
    rewrite_SYNTHETIC_rights,
    rewrite_SYNTHETIC_witness,
    witness_for_SYNTHETIC_fixture,
    write_SYNTHETIC_local_fixture,
)

_MAX_U64 = "18446744073709551615"
_MAX_U64_PLUS_ONE = "18446744073709551616"
_HUGE_U64_CANDIDATE = "1" * 5000


def _capture_and_admit(root: Path):
    from build_finance.crypto_replay.admission import admit_local_fixture
    from build_finance.crypto_replay.local_fixture import capture_local_fixture

    captured = capture_local_fixture(root)
    return captured, admit_local_fixture(captured)


def _capture_only(root: Path):
    from build_finance.crypto_replay.local_fixture import capture_local_fixture

    return capture_local_fixture(root)


def _receipts(batch) -> tuple[dict[str, object], ...]:
    return tuple(parse_canonical_record(record) for record in batch.source_receipt_records)


def _reason_codes(batch) -> tuple[str, ...]:
    return tuple(batch.reason_codes)


def _skip_primitive_unavailable(primitive: str, error: OSError | subprocess.CalledProcessError) -> None:
    pytest.skip(f"{primitive} unavailable; explicit platform/privilege skip evidence: {type(error).__name__}: {error}")


def _make_symlink(link: Path, target: Path, *, target_is_directory: bool) -> None:
    try:
        link.symlink_to(target, target_is_directory=target_is_directory)
    except OSError as error:
        _skip_primitive_unavailable("symlink/reparse point creation", error)


def _make_windows_junction(link: Path, target: Path) -> None:
    if sys.platform != "win32":
        pytest.skip("Windows junction/reparse primitive unavailable on this platform")
    try:
        completed = subprocess.run(
            ["cmd", "/c", "mklink", "/J", os.fspath(link), os.fspath(target)],
            check=True,
            text=True,
            capture_output=True,
        )
    except subprocess.CalledProcessError as error:
        _skip_primitive_unavailable("Windows junction creation", error)
    if "Junction created" not in completed.stdout and not link.exists():
        pytest.skip(
            f"Windows junction creation returned unexpected evidence: {completed.stdout!r} {completed.stderr!r}"
        )


def _replace_directory_with_link(link: Path, target: Path, primitive: str) -> None:
    shutil.rmtree(link)
    if primitive == "symlink":
        _make_symlink(link, target, target_is_directory=True)
    elif primitive == "junction":
        _make_windows_junction(link, target)
    else:  # pragma: no cover - protects future table edits.
        raise AssertionError(f"unsupported link primitive: {primitive}")


def _reseal_first_manifest_file(fixture, **overrides: object) -> None:
    manifest = dict(fixture.manifest)
    files = list(manifest["files"])  # type: ignore[arg-type]
    files[0] = {**files[0], **overrides}
    manifest["files"] = files
    reseal_SYNTHETIC_manifest(fixture, manifest)


def _u64_case_root(tmp_path: Path, label: str, value: str) -> Path:
    return tmp_path / f"{label}-{len(value)}-{value[:8]}"


_SYNTHETIC_TARGET_EVENT_ID = "1" * 64
_SYNTHETIC_SECOND_TARGET_EVENT_ID = "2" * 64


def _assert_rejected_for_reparse_or_not_local(captured, batch) -> None:
    assert captured.capture_issues
    reason_codes = _reason_codes(batch)
    if "ADMISSION_SET_NOT_CLOSED" in reason_codes:
        assert batch.status == "QUARANTINED"
    else:
        assert batch.status == "REJECTED"
    assert "ADMISSION_NOT_LOCAL" in reason_codes or "ADMISSION_MANIFEST_MISMATCH" in reason_codes
    assert batch.candidates == ()


def _replace_file_bytes(path: Path, payload: bytes) -> None:
    replacement = path.with_suffix(path.suffix + ".replacement")
    replacement.write_bytes(payload)
    try:
        replacement.replace(path)
    except PermissionError:
        path.write_bytes(payload)


def _rewrite_manifest_without_schema_registry(fixture, manifest: dict[str, object]) -> None:
    body = dict(manifest)
    body.pop("fixture_manifest_sha256", None)
    body["fixture_manifest_sha256"] = sha256_hex(canonical_json_bytes(body))  # type: ignore[arg-type]
    fixture.manifest_path.write_bytes(canonical_record_bytes(body))  # type: ignore[arg-type]


def test_fixture_hash_mismatch_quarantines_set(tmp_path: Path) -> None:
    fixture = write_SYNTHETIC_local_fixture(tmp_path)
    fixture.payload_paths[0].write_bytes(fixture.payloads[0] + b"\nSYNTHETIC_DIGEST_MISMATCH\n")

    _captured, batch = _capture_and_admit(fixture.root)

    receipt = _receipts(batch)[0]
    assert batch.status == "QUARANTINED"
    assert _reason_codes(batch) == ("ADMISSION_HASH_MISMATCH", "ADMISSION_SET_NOT_CLOSED")
    assert receipt["status"] == "QUARANTINED"
    assert receipt["reason_codes"] == ["ADMISSION_HASH_MISMATCH", "ADMISSION_SET_NOT_CLOSED"]
    assert receipt["raw_payload_sha256"] == sha256_hex(fixture.payloads[0] + b"\nSYNTHETIC_DIGEST_MISMATCH\n")


def test_point_in_time_fields_required(tmp_path: Path) -> None:
    spec = SYNTHETICEventSpec(observed_at=None)
    fixture = write_SYNTHETIC_local_fixture(tmp_path, event_specs=(spec,))

    _captured, batch = _capture_and_admit(fixture.root)

    receipt = _receipts(batch)[0]
    assert batch.status == "REJECTED"
    assert _reason_codes(batch) == ("ADMISSION_POINT_IN_TIME_MISSING",)
    assert receipt["observed_at"] is None
    assert receipt["ingested_at"] == SYNTHETIC_INGESTED_AT


def test_duplicate_base_mint_quarantines_set(tmp_path: Path) -> None:
    first = SYNTHETICEventSpec()
    second = dataclasses.replace(
        first,
        admission_sequence="2",
        availability_slot="2",
        relative_path="payloads/quote-0002.json",
        source_native_event_id="synthetic-event-0002",
        market_id=f"{SYNTHETIC_BASE_MINT}/SYNTHETIC_ALT_QUOTE_INVALID:jupiter",
        quote_mint="SYNTHETIC_ALT_QUOTE_INVALID",
    )
    fixture = write_SYNTHETIC_local_fixture(tmp_path, event_specs=(first, second))

    _captured, batch = _capture_and_admit(fixture.root)

    assert batch.status == "QUARANTINED"
    assert _reason_codes(batch) == ("ADMISSION_UNIVERSE_BIASED", "ADMISSION_SET_NOT_CLOSED")


@pytest.mark.parametrize(
    ("field", "value"),
    (
        ("schema", "trading.fixture-manifest/v2"),
        ("network", "solana-devnet"),
        ("venue_profile", "solana-jupiter-live/v1"),
        ("universe_policy", "PARTIAL_DECLARED_SOURCE_UNIVERSE"),
        ("point_in_time_mode", "WALL_CLOCK_CAPTURE"),
        ("selection_failures_retained", False),
        ("no_route_observations_retained", False),
        ("inactive_assets_retained", False),
        ("gaps_retained", False),
        ("allowed_markets", []),
    ),
)
def test_fixture_manifest_universe_policy_and_retention_weakening_quarantines_set(
    tmp_path: Path,
    field: str,
    value: object,
) -> None:
    fixture = write_SYNTHETIC_local_fixture(tmp_path / field)
    manifest = dict(fixture.manifest)
    manifest[field] = value  # type: ignore[assignment]
    _rewrite_manifest_without_schema_registry(fixture, manifest)

    _captured, batch = _capture_and_admit(fixture.root)

    assert batch.status == "QUARANTINED"
    assert _reason_codes(batch) == ("ADMISSION_UNIVERSE_BIASED", "ADMISSION_SET_NOT_CLOSED")


def test_base_mint_cannot_equal_quote_mint(tmp_path: Path) -> None:
    spec = SYNTHETICEventSpec(
        quote_mint=SYNTHETIC_BASE_MINT, market_id=f"{SYNTHETIC_BASE_MINT}/{SYNTHETIC_BASE_MINT}:jupiter"
    )
    fixture = write_SYNTHETIC_local_fixture(tmp_path, event_specs=(spec,))

    _captured, batch = _capture_and_admit(fixture.root)

    receipt = _receipts(batch)[0]
    assert batch.status == "REJECTED"
    assert _reason_codes(batch) == ("ADMISSION_PROFILE_MISMATCH",)
    assert receipt["market_id"] == spec.market_id


def test_malformed_but_resealed_jupiter_payload_is_rejected_not_silently_admitted(tmp_path: Path) -> None:
    fixture = write_SYNTHETIC_local_fixture(tmp_path)
    malformed_payload = json.dumps(
        {**json.loads(fixture.payloads[0].decode("utf-8")), "source_position": "SYNTHETIC_NOT_AN_OBJECT"},
        ensure_ascii=False,
        separators=(",", ":"),
    ).encode("utf-8")
    fixture.payload_paths[0].write_bytes(malformed_payload)
    malformed_sha256 = sha256_hex(malformed_payload)
    manifest = dict(fixture.manifest)
    files = list(manifest["files"])  # type: ignore[arg-type]
    files[0] = {**files[0], "raw_payload_sha256": malformed_sha256, "byte_length": str(len(malformed_payload))}
    manifest["files"] = files
    reseal_SYNTHETIC_manifest(fixture, manifest)
    witness = parse_canonical_record(fixture.witness_paths[0].read_bytes())
    witness["raw_payload_sha256"] = malformed_sha256
    witness["byte_length"] = str(len(malformed_payload))
    rewrite_SYNTHETIC_witness(fixture, 0, witness)

    _captured, batch = _capture_and_admit(fixture.root)

    receipt = _receipts(batch)[0]
    assert batch.status == "REJECTED"
    assert _reason_codes(batch) == ("ADMISSION_PROFILE_MISMATCH",)
    assert batch.candidates == ()
    assert receipt["raw_payload_sha256"] == malformed_sha256
    assert receipt["source_id"] == SYNTHETIC_SOURCE_ID


def test_partial_rights_evidence_is_preserved(tmp_path: Path) -> None:
    fixture = write_SYNTHETIC_local_fixture(tmp_path)
    rights = dict(fixture.rights_manifest)
    rights["rights_review_date"] = None
    rewrite_SYNTHETIC_rights(fixture, rights)
    manifest = dict(fixture.manifest)
    manifest["rights_manifest_sha256"] = sha256_hex(fixture.rights_manifest_path.read_bytes())
    reseal_SYNTHETIC_manifest(fixture, manifest)

    _captured, batch = _capture_and_admit(fixture.root)

    receipt = _receipts(batch)[0]
    assert batch.status == "REJECTED"
    assert _reason_codes(batch) == ("ADMISSION_RIGHTS_MISSING",)
    assert receipt["rights_role"] == "offline_research_replay"
    assert receipt["rights_effective_date"] == "2026-01-01"
    assert receipt["rights_review_date"] is None
    assert receipt["terms_sha256"] == sha256_hex(fixture.terms_payload)


def test_conflicting_source_position_rejected(tmp_path: Path) -> None:
    first = SYNTHETICEventSpec()
    second = dataclasses.replace(
        first,
        admission_sequence="2",
        relative_path="payloads/quote-0002.json",
        route_capacity_base_atoms="999999",
    )
    fixture = write_SYNTHETIC_local_fixture(tmp_path, event_specs=(first, second))

    _captured, batch = _capture_and_admit(fixture.root)

    assert batch.status == "QUARANTINED"
    assert _reason_codes(batch) == ("ADMISSION_POSITION_CONFLICT",)
    assert batch.candidates == ()
    assert {tuple(receipt["reason_codes"]) for receipt in _receipts(batch)} == {("ADMISSION_POSITION_CONFLICT",)}


def test_same_version_different_raw_bytes_quarantines_participating_receipts(tmp_path: Path) -> None:
    first = SYNTHETICEventSpec()
    conflicting_duplicate = dataclasses.replace(
        first,
        admission_sequence="2",
        relative_path="payloads/quote-0002.json",
        revision_availability_admission_sequence="1",
        route_capacity_base_atoms="999999",
    )
    unrelated = dataclasses.replace(
        first,
        admission_sequence="3",
        relative_path="payloads/quote-0003.json",
        source_native_event_id="synthetic-event-unrelated",
    )
    fixture = write_SYNTHETIC_local_fixture(tmp_path, event_specs=(unrelated, conflicting_duplicate, first))

    _captured, batch = _capture_and_admit(fixture.root)

    receipts = {str(receipt["relative_path"]): receipt for receipt in _receipts(batch)}
    assert batch.status == "QUARANTINED"
    assert _reason_codes(batch) == ("ADMISSION_POSITION_CONFLICT",)
    assert batch.candidates == ()
    assert receipts["payloads/quote-0001.json"]["status"] == "QUARANTINED"
    assert receipts["payloads/quote-0001.json"]["reason_codes"] == ["ADMISSION_POSITION_CONFLICT"]
    assert receipts["payloads/quote-0002.json"]["status"] == "QUARANTINED"
    assert receipts["payloads/quote-0002.json"]["reason_codes"] == ["ADMISSION_POSITION_CONFLICT"]
    assert receipts["payloads/quote-0003.json"]["status"] == "ADMITTED"
    assert receipts["payloads/quote-0003.json"]["reason_codes"] == []


def test_same_slot_different_market_is_not_conflict(tmp_path: Path) -> None:
    first = SYNTHETICEventSpec()
    second = dataclasses.replace(
        first,
        admission_sequence="2",
        relative_path="payloads/quote-0002.json",
        source_native_event_id="synthetic-event-0002",
        market_id="SYNTHETIC_OTHER_BASE_INVALID/SYNTHETIC_OTHER_QUOTE_INVALID:jupiter",
        base_mint="SYNTHETIC_OTHER_BASE_INVALID",
        quote_mint="SYNTHETIC_OTHER_QUOTE_INVALID",
    )
    fixture = write_SYNTHETIC_local_fixture(tmp_path, event_specs=(first, second))

    _captured, batch = _capture_and_admit(fixture.root)

    assert batch.status == "ADMITTED"
    assert _reason_codes(batch) == ()
    assert len(batch.candidates) == 2


def test_same_slot_same_market_different_native_id_is_not_conflict(tmp_path: Path) -> None:
    first = SYNTHETICEventSpec()
    second = dataclasses.replace(
        first,
        admission_sequence="2",
        relative_path="payloads/quote-0002.json",
        source_native_event_id="synthetic-event-0002",
    )
    fixture = write_SYNTHETIC_local_fixture(tmp_path, event_specs=(second, first))

    _captured, batch = _capture_and_admit(fixture.root)

    assert batch.status == "ADMITTED"
    assert _reason_codes(batch) == ()
    assert len(batch.candidates) == 2


@pytest.mark.parametrize(
    ("field", "value"),
    (
        ("transaction_index", 1),
        ("instruction_index", 1),
        ("event_index", 1),
    ),
)
def test_distinct_source_position_indices_are_not_conflicts(tmp_path: Path, field: str, value: int) -> None:
    first = SYNTHETICEventSpec()
    second = dataclasses.replace(
        first,
        admission_sequence="2",
        relative_path="payloads/quote-0002.json",
        **{field: value},
    )
    fixture = write_SYNTHETIC_local_fixture(tmp_path, event_specs=(second, first))

    _captured, batch = _capture_and_admit(fixture.root)

    assert batch.status == "ADMITTED"
    assert _reason_codes(batch) == ()
    assert len(batch.candidates) == 2


def test_correction_revision_for_same_lineage_is_not_position_conflict(tmp_path: Path) -> None:
    original = SYNTHETICEventSpec()
    correction = dataclasses.replace(
        original,
        admission_sequence="2",
        availability_slot="2",
        relative_path="payloads/quote-0002.json",
        source_position_slot="1",
        source_revision="synthetic-revision-v2",
        revision_kind="CORRECTION",
        supersedes_event_id=_SYNTHETIC_TARGET_EVENT_ID,
        revision_availability_slot="2",
        revision_availability_admission_sequence="2",
        route_capacity_base_atoms="999999",
    )
    fixture = write_SYNTHETIC_local_fixture(tmp_path, event_specs=(correction, original))

    _captured, batch = _capture_and_admit(fixture.root)

    assert batch.status == "ADMITTED"
    assert _reason_codes(batch) == ()
    assert len(batch.candidates) == 2


@pytest.mark.parametrize(
    "spec",
    (
        dataclasses.replace(
            SYNTHETICEventSpec(), revision_kind="ORIGINAL", supersedes_event_id=_SYNTHETIC_TARGET_EVENT_ID
        ),
        dataclasses.replace(SYNTHETICEventSpec(), revision_kind="CORRECTION"),
        dataclasses.replace(
            SYNTHETICEventSpec(),
            revision_kind="RETRACTION",
            supersedes_event_id=_SYNTHETIC_TARGET_EVENT_ID,
            retracts_event_id=_SYNTHETIC_SECOND_TARGET_EVENT_ID,
        ),
        dataclasses.replace(SYNTHETICEventSpec(), revision_kind="ORIGINAL", revision_availability_slot="2"),
    ),
)
def test_revision_tuple_mismatch_is_revision_causality(tmp_path: Path, spec: SYNTHETICEventSpec) -> None:
    fixture = write_SYNTHETIC_local_fixture(tmp_path, event_specs=(spec,))

    _captured, batch = _capture_and_admit(fixture.root)

    receipt = _receipts(batch)[0]
    assert batch.status == "QUARANTINED"
    assert _reason_codes(batch) == ("ADMISSION_REVISION_CAUSALITY",)
    assert batch.candidates == ()
    assert receipt["status"] == "QUARANTINED"
    assert receipt["reason_codes"] == ["ADMISSION_REVISION_CAUSALITY"]


def test_multiple_revisions_of_same_target_are_revision_fork(tmp_path: Path) -> None:
    original = SYNTHETICEventSpec()
    first_correction = dataclasses.replace(
        original,
        admission_sequence="2",
        availability_slot="2",
        relative_path="payloads/quote-0002.json",
        source_position_slot="1",
        source_revision="synthetic-revision-v2",
        revision_kind="CORRECTION",
        supersedes_event_id=_SYNTHETIC_TARGET_EVENT_ID,
        revision_availability_slot="2",
        revision_availability_admission_sequence="2",
        route_capacity_base_atoms="999999",
    )
    second_correction = dataclasses.replace(
        original,
        admission_sequence="3",
        availability_slot="3",
        relative_path="payloads/quote-0003.json",
        source_position_slot="1",
        source_revision="synthetic-revision-v3",
        revision_kind="CORRECTION",
        supersedes_event_id=_SYNTHETIC_TARGET_EVENT_ID,
        revision_availability_slot="3",
        revision_availability_admission_sequence="3",
        liquidity_quote_atoms="3333333",
    )
    fixture = write_SYNTHETIC_local_fixture(tmp_path, event_specs=(second_correction, original, first_correction))

    _captured, batch = _capture_and_admit(fixture.root)

    receipts = {str(receipt["relative_path"]): receipt for receipt in _receipts(batch)}
    assert batch.status == "QUARANTINED"
    assert _reason_codes(batch) == ("ADMISSION_REVISION_FORK",)
    assert batch.candidates == ()
    assert receipts["payloads/quote-0001.json"]["status"] == "ADMITTED"
    assert receipts["payloads/quote-0001.json"]["reason_codes"] == []
    assert receipts["payloads/quote-0002.json"]["status"] == "QUARANTINED"
    assert receipts["payloads/quote-0002.json"]["reason_codes"] == ["ADMISSION_REVISION_FORK"]
    assert receipts["payloads/quote-0003.json"]["status"] == "QUARANTINED"
    assert receipts["payloads/quote-0003.json"]["reason_codes"] == ["ADMISSION_REVISION_FORK"]


def test_same_target_corrections_with_different_native_ids_are_revision_fork(tmp_path: Path) -> None:
    original = SYNTHETICEventSpec()
    first_correction = _revision_spec(
        original,
        admission_sequence="2",
        relative_path="payloads/quote-0002.json",
        source_revision="synthetic-revision-v2",
        revision_kind="CORRECTION",
        target_event_id=_SYNTHETIC_TARGET_EVENT_ID,
        route_capacity_base_atoms="999999",
    )
    second_correction = _revision_spec(
        original,
        admission_sequence="3",
        relative_path="payloads/quote-0003.json",
        source_native_event_id="synthetic-event-correction-alt-native",
        source_revision="synthetic-revision-v3",
        revision_kind="CORRECTION",
        target_event_id=_SYNTHETIC_TARGET_EVENT_ID,
        liquidity_quote_atoms="3333333",
    )
    fixture = write_SYNTHETIC_local_fixture(tmp_path, event_specs=(second_correction, original, first_correction))

    _captured, batch = _capture_and_admit(fixture.root)

    _assert_revision_fork_batch(batch, fork_paths=("payloads/quote-0002.json", "payloads/quote-0003.json"))


def test_same_target_corrections_with_different_position_index_are_revision_fork(tmp_path: Path) -> None:
    original = SYNTHETICEventSpec()
    first_correction = _revision_spec(
        original,
        admission_sequence="2",
        relative_path="payloads/quote-0002.json",
        source_revision="synthetic-revision-v2",
        revision_kind="CORRECTION",
        target_event_id=_SYNTHETIC_TARGET_EVENT_ID,
        route_capacity_base_atoms="999999",
    )
    second_correction = _revision_spec(
        original,
        admission_sequence="3",
        relative_path="payloads/quote-0003.json",
        transaction_index=1,
        source_revision="synthetic-revision-v3",
        revision_kind="CORRECTION",
        target_event_id=_SYNTHETIC_TARGET_EVENT_ID,
        liquidity_quote_atoms="3333333",
    )
    fixture = write_SYNTHETIC_local_fixture(tmp_path, event_specs=(second_correction, original, first_correction))

    _captured, batch = _capture_and_admit(fixture.root)

    _assert_revision_fork_batch(batch, fork_paths=("payloads/quote-0002.json", "payloads/quote-0003.json"))


def test_same_target_retractions_with_different_native_ids_are_revision_fork(tmp_path: Path) -> None:
    original = SYNTHETICEventSpec()
    first_retraction = _revision_spec(
        original,
        admission_sequence="2",
        relative_path="payloads/quote-0002.json",
        source_revision="synthetic-revision-retract-v2",
        revision_kind="RETRACTION",
        target_event_id=_SYNTHETIC_TARGET_EVENT_ID,
        route_capacity_base_atoms="0",
    )
    second_retraction = _revision_spec(
        original,
        admission_sequence="3",
        relative_path="payloads/quote-0003.json",
        source_native_event_id="synthetic-event-retraction-alt-native",
        source_revision="synthetic-revision-retract-v3",
        revision_kind="RETRACTION",
        target_event_id=_SYNTHETIC_TARGET_EVENT_ID,
        liquidity_quote_atoms="0",
    )
    fixture = write_SYNTHETIC_local_fixture(tmp_path, event_specs=(second_retraction, original, first_retraction))

    _captured, batch = _capture_and_admit(fixture.root)

    _assert_revision_fork_batch(batch, fork_paths=("payloads/quote-0002.json", "payloads/quote-0003.json"))


def test_distinct_revision_targets_are_not_revision_fork(tmp_path: Path) -> None:
    original = SYNTHETICEventSpec()
    first_correction = _revision_spec(
        original,
        admission_sequence="2",
        relative_path="payloads/quote-0002.json",
        source_native_event_id="synthetic-event-correction-one",
        source_revision="synthetic-revision-v2",
        revision_kind="CORRECTION",
        target_event_id=_SYNTHETIC_TARGET_EVENT_ID,
        route_capacity_base_atoms="999999",
    )
    second_correction = _revision_spec(
        original,
        admission_sequence="3",
        relative_path="payloads/quote-0003.json",
        source_native_event_id="synthetic-event-correction-two",
        source_revision="synthetic-revision-v3",
        revision_kind="CORRECTION",
        target_event_id=_SYNTHETIC_SECOND_TARGET_EVENT_ID,
        liquidity_quote_atoms="3333333",
    )
    fixture = write_SYNTHETIC_local_fixture(tmp_path, event_specs=(second_correction, original, first_correction))

    _captured, batch = _capture_and_admit(fixture.root)

    assert batch.status == "ADMITTED"
    assert _reason_codes(batch) == ()
    assert len(batch.candidates) == 3


def test_correction_and_retraction_sharing_target_are_distinct_revision_kinds(tmp_path: Path) -> None:
    original = SYNTHETICEventSpec()
    correction = _revision_spec(
        original,
        admission_sequence="2",
        relative_path="payloads/quote-0002.json",
        source_native_event_id="synthetic-event-correction",
        source_revision="synthetic-revision-correction",
        revision_kind="CORRECTION",
        target_event_id=_SYNTHETIC_TARGET_EVENT_ID,
        route_capacity_base_atoms="999999",
    )
    retraction = _revision_spec(
        original,
        admission_sequence="3",
        relative_path="payloads/quote-0003.json",
        source_native_event_id="synthetic-event-retraction",
        source_revision="synthetic-revision-retraction",
        revision_kind="RETRACTION",
        target_event_id=_SYNTHETIC_TARGET_EVENT_ID,
        liquidity_quote_atoms="0",
    )
    fixture = write_SYNTHETIC_local_fixture(tmp_path, event_specs=(retraction, original, correction))

    _captured, batch = _capture_and_admit(fixture.root)

    assert batch.status == "ADMITTED"
    assert _reason_codes(batch) == ()
    assert len(batch.candidates) == 3


def _revision_spec(
    original: SYNTHETICEventSpec,
    *,
    admission_sequence: str,
    relative_path: str,
    source_revision: str,
    revision_kind: str,
    target_event_id: str,
    source_native_event_id: str | None = None,
    transaction_index: int = 0,
    route_capacity_base_atoms: str = "1000000",
    liquidity_quote_atoms: str = "2000000",
) -> SYNTHETICEventSpec:
    target_kwargs: dict[str, object]
    if revision_kind == "CORRECTION":
        target_kwargs = {"supersedes_event_id": target_event_id}
    elif revision_kind == "RETRACTION":
        target_kwargs = {"retracts_event_id": target_event_id}
    else:
        raise AssertionError(f"unsupported synthetic revision kind: {revision_kind}")
    return dataclasses.replace(
        original,
        admission_sequence=admission_sequence,
        availability_slot=admission_sequence,
        relative_path=relative_path,
        source_position_slot="1",
        transaction_index=transaction_index,
        source_native_event_id=source_native_event_id or original.source_native_event_id,
        source_revision=source_revision,
        revision_kind=revision_kind,
        revision_availability_slot=admission_sequence,
        revision_availability_admission_sequence=admission_sequence,
        route_capacity_base_atoms=route_capacity_base_atoms,
        liquidity_quote_atoms=liquidity_quote_atoms,
        **target_kwargs,
    )


def _assert_revision_fork_batch(batch, *, fork_paths: tuple[str, ...]) -> None:
    receipts = {str(receipt["relative_path"]): receipt for receipt in _receipts(batch)}
    assert batch.status == "QUARANTINED"
    assert _reason_codes(batch) == ("ADMISSION_REVISION_FORK",)
    assert batch.candidates == ()
    assert receipts["payloads/quote-0001.json"]["status"] == "ADMITTED"
    assert receipts["payloads/quote-0001.json"]["reason_codes"] == []
    for relative_path in fork_paths:
        assert receipts[relative_path]["status"] == "QUARANTINED"
        assert receipts[relative_path]["reason_codes"] == ["ADMISSION_REVISION_FORK"]


def test_terms_digest_mismatch_preserves_actual_terms_digest(tmp_path: Path) -> None:
    fixture = write_SYNTHETIC_local_fixture(tmp_path)
    actual_terms = fixture.terms_payload + b"SYNTHETIC TERMS DIGEST MISMATCH\n"
    fixture.terms_path.write_bytes(actual_terms)

    _captured, batch = _capture_and_admit(fixture.root)

    receipt = _receipts(batch)[0]
    assert batch.status == "REJECTED"
    assert _reason_codes(batch) == ("ADMISSION_RIGHTS_MISSING",)
    assert receipt["terms_sha256"] == sha256_hex(actual_terms)
    assert receipt["rights_role"] == "offline_research_replay"


def test_witness_identity_mismatch_maps_manifest_mismatch(tmp_path: Path) -> None:
    spec = SYNTHETICEventSpec()
    fixture = write_SYNTHETIC_local_fixture(tmp_path, event_specs=(spec,))
    witness = witness_for_SYNTHETIC_fixture(fixture, 0, spec)
    witness["raw_payload_sha256"] = "0" * 64
    rewrite_SYNTHETIC_witness(fixture, 0, witness)

    _captured, batch = _capture_and_admit(fixture.root)

    receipt = _receipts(batch)[0]
    assert batch.status == "REJECTED"
    assert _reason_codes(batch) == ("ADMISSION_MANIFEST_MISMATCH",)
    assert receipt["raw_payload_sha256"] == sha256_hex(fixture.payloads[0])


def test_path_confinement_rejects_traversal_absolute_drive_and_backslash(tmp_path: Path) -> None:
    unsafe_paths = ("../outside.json", "/absolute.json", "C:/absolute.json", "payloads\\quote.json")
    for unsafe_path in unsafe_paths:
        case_root = tmp_path / unsafe_path.replace("/", "_").replace("\\", "_").replace(":", "_")
        fixture = write_SYNTHETIC_local_fixture(case_root)
        manifest = dict(fixture.manifest)
        files = list(manifest["files"])  # type: ignore[arg-type]
        files[0] = {**files[0], "relative_path": unsafe_path}
        manifest["files"] = files
        reseal_SYNTHETIC_manifest(fixture, manifest)

        captured, batch = _capture_and_admit(fixture.root)

        assert captured.capture_issues
        assert batch.status == "REJECTED"
        assert "ADMISSION_MANIFEST_MISMATCH" in _reason_codes(batch)


def test_symlink_payload_entry_is_rejected_when_platform_allows_symlinks(tmp_path: Path) -> None:
    fixture = write_SYNTHETIC_local_fixture(tmp_path / "entry")
    outside_payload = tmp_path / "outside-payload.json"
    outside_payload.write_bytes(fixture.payloads[0])
    fixture.payload_paths[0].unlink()
    _make_symlink(fixture.payload_paths[0], outside_payload, target_is_directory=False)

    captured, batch = _capture_and_admit(fixture.root)

    _assert_rejected_for_reparse_or_not_local(captured, batch)


def test_symlink_root_is_rejected_when_platform_allows_symlinks(tmp_path: Path) -> None:
    fixture = write_SYNTHETIC_local_fixture(tmp_path / "root-target")
    link_root = tmp_path / "root-link"
    _make_symlink(link_root, fixture.root, target_is_directory=True)

    captured, batch = _capture_and_admit(link_root)

    _assert_rejected_for_reparse_or_not_local(captured, batch)


def test_windows_reparse_payload_entry_is_rejected_when_privilege_allows(tmp_path: Path) -> None:
    if sys.platform != "win32":
        pytest.skip("Windows reparse point primitive unavailable on this platform")
    fixture = write_SYNTHETIC_local_fixture(tmp_path / "reparse-entry")
    outside_payload = tmp_path / "outside-reparse-payload.json"
    outside_payload.write_bytes(fixture.payloads[0])
    fixture.payload_paths[0].unlink()
    _make_symlink(fixture.payload_paths[0], outside_payload, target_is_directory=False)

    captured, batch = _capture_and_admit(fixture.root)

    _assert_rejected_for_reparse_or_not_local(captured, batch)


def test_windows_reparse_root_is_rejected_when_privilege_allows(tmp_path: Path) -> None:
    if sys.platform != "win32":
        pytest.skip("Windows reparse point primitive unavailable on this platform")
    fixture = write_SYNTHETIC_local_fixture(tmp_path / "reparse-root-target")
    reparse_root = tmp_path / "reparse-root-link"
    _make_symlink(reparse_root, fixture.root, target_is_directory=True)

    captured, batch = _capture_and_admit(reparse_root)

    _assert_rejected_for_reparse_or_not_local(captured, batch)


def test_windows_junction_payload_entry_is_rejected_when_platform_allows_junctions(tmp_path: Path) -> None:
    fixture = write_SYNTHETIC_local_fixture(tmp_path / "junction-entry")
    outside_dir = tmp_path / "outside-junction-payload-dir"
    outside_dir.mkdir()
    (outside_dir / "payload.json").write_bytes(fixture.payloads[0])
    fixture.payload_paths[0].unlink()
    _make_windows_junction(fixture.payload_paths[0], outside_dir)

    captured, batch = _capture_and_admit(fixture.root)

    _assert_rejected_for_reparse_or_not_local(captured, batch)


def test_windows_junction_root_is_rejected_when_platform_allows_junctions(tmp_path: Path) -> None:
    fixture = write_SYNTHETIC_local_fixture(tmp_path / "junction-root-target")
    junction_root = tmp_path / "junction-root"
    _make_windows_junction(junction_root, fixture.root)

    captured, batch = _capture_and_admit(junction_root)

    _assert_rejected_for_reparse_or_not_local(captured, batch)


@pytest.mark.parametrize("primitive", ("symlink", "junction"))
def test_payloads_reparse_parent_discards_outside_payload_bytes(tmp_path: Path, primitive: str) -> None:
    fixture = write_SYNTHETIC_local_fixture(tmp_path / primitive)
    outside_payload = fixture.payloads[0] + b"\nSYNTHETIC OUTSIDE PAYLOAD PARENT BYTES\n"
    outside_dir = tmp_path / f"outside-payloads-{primitive}"
    outside_dir.mkdir()
    (outside_dir / "quote-0001.json").write_bytes(outside_payload)
    _replace_directory_with_link(fixture.root / "payloads", outside_dir, primitive)

    captured, batch = _capture_and_admit(fixture.root)

    outside_sha256 = sha256_hex(outside_payload)
    receipt = _receipts(batch)[0]
    assert batch.status == "REJECTED"
    assert "ADMISSION_NOT_LOCAL" in _reason_codes(batch)
    assert batch.candidates == ()
    assert captured.files[0].payload is None
    assert captured.files[0].sha256 is None
    assert captured.files[0].byte_length is None
    assert receipt["raw_payload_sha256"] is None
    assert outside_payload not in [file.payload for file in captured.files]
    assert outside_sha256 not in {file.sha256 for file in captured.files}
    assert receipt["raw_payload_sha256"] != outside_sha256


@pytest.mark.parametrize("primitive", ("symlink", "junction"))
def test_terms_reparse_parent_discards_outside_terms_bytes(tmp_path: Path, primitive: str) -> None:
    fixture = write_SYNTHETIC_local_fixture(tmp_path / primitive)
    outside_terms = fixture.terms_payload + b"SYNTHETIC OUTSIDE TERMS PARENT BYTES\n"
    outside_dir = tmp_path / f"outside-terms-{primitive}"
    outside_dir.mkdir()
    (outside_dir / "synthetic-terms.txt").write_bytes(outside_terms)
    _replace_directory_with_link(fixture.root / "terms", outside_dir, primitive)

    captured, batch = _capture_and_admit(fixture.root)

    outside_sha256 = sha256_hex(outside_terms)
    receipt = _receipts(batch)[0]
    assert batch.status == "REJECTED"
    assert "ADMISSION_NOT_LOCAL" in _reason_codes(batch)
    assert batch.candidates == ()
    assert captured.terms[0].payload is None
    assert captured.terms[0].sha256 is None
    assert captured.terms[0].byte_length is None
    assert captured.terms_by_sha256 == {}
    assert receipt["terms_sha256"] is None
    assert outside_sha256 not in captured.terms_by_sha256
    assert receipt["terms_sha256"] != outside_sha256


@pytest.mark.parametrize("primitive", ("symlink", "junction"))
def test_witnesses_reparse_parent_discards_outside_witness_bytes(tmp_path: Path, primitive: str) -> None:
    spec = SYNTHETICEventSpec()
    fixture = write_SYNTHETIC_local_fixture(tmp_path / primitive, event_specs=(spec,))
    outside_witness = witness_for_SYNTHETIC_fixture(fixture, 0, spec)
    outside_witness["observed_at"] = "2026-01-01T00:00:03.000000000Z"
    outside_witness["ingested_at"] = "2026-01-01T00:00:04.000000000Z"
    outside_payload = canonical_record_bytes(outside_witness)
    outside_dir = tmp_path / f"outside-witnesses-{primitive}"
    outside_dir.mkdir()
    (outside_dir / "witness-0001.json").write_bytes(outside_payload)
    _replace_directory_with_link(fixture.root / "witnesses", outside_dir, primitive)

    captured, batch = _capture_and_admit(fixture.root)

    outside_sha256 = sha256_hex(outside_payload)
    receipt = _receipts(batch)[0]
    assert batch.status == "REJECTED"
    assert "ADMISSION_NOT_LOCAL" in _reason_codes(batch)
    assert batch.candidates == ()
    assert captured.witnesses[0].payload is None
    assert captured.witnesses[0].sha256 is None
    assert captured.witnesses[0].observed_at is None
    assert captured.witnesses[0].ingested_at is None
    assert captured.files[0].observed_at is None
    assert captured.files[0].ingested_at is None
    assert receipt["observed_at"] is None
    assert receipt["ingested_at"] is None
    assert outside_payload not in [witness.payload for witness in captured.witnesses]
    assert outside_sha256 not in {witness.sha256 for witness in captured.witnesses}


def test_missing_extra_and_non_regular_payloads_close_set(tmp_path: Path) -> None:
    missing = write_SYNTHETIC_local_fixture(tmp_path / "missing")
    missing.payload_paths[0].unlink()
    _captured_missing, missing_batch = _capture_and_admit(missing.root)
    assert missing_batch.status == "QUARANTINED"
    assert "ADMISSION_SET_NOT_CLOSED" in _reason_codes(missing_batch)

    extra = write_SYNTHETIC_local_fixture(tmp_path / "extra")
    (extra.root / "payloads" / "unexpected.json").write_bytes(extra.payloads[0])
    _captured_extra, extra_batch = _capture_and_admit(extra.root)
    assert extra_batch.status == "QUARANTINED"
    assert "ADMISSION_SET_NOT_CLOSED" in _reason_codes(extra_batch)

    non_regular = write_SYNTHETIC_local_fixture(tmp_path / "non-regular")
    non_regular.payload_paths[0].unlink()
    non_regular.payload_paths[0].mkdir()
    _captured_non_regular, non_regular_batch = _capture_and_admit(non_regular.root)
    assert non_regular_batch.status == "QUARANTINED"
    assert "ADMISSION_SET_NOT_CLOSED" in _reason_codes(non_regular_batch)


@pytest.mark.parametrize("stage", ("pre_open", "open", "post_open"))
def test_payload_replacement_race_is_rejected_at_pre_open_and_post_identity_checks(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    stage: str,
) -> None:
    fixture = write_SYNTHETIC_local_fixture(tmp_path / stage)
    from build_finance.crypto_replay import local_fixture

    payload_path = fixture.payload_paths[0]
    target_paths = _local_fixture_open_targets(fixture, payload_path)
    replacement_payload = fixture.payloads[0] + f"\nSYNTHETIC_{stage.upper()}_REPLACEMENT\n".encode("ascii")
    replaced = False

    def is_payload_open(path: str | os.PathLike[str]) -> bool:
        return _matches_local_fixture_open_target(path, target_paths)

    def replace_once() -> None:
        nonlocal replaced
        if not replaced:
            replaced = True
            _replace_file_bytes(payload_path, replacement_payload)

    if stage == "pre_open":
        original_lstat = local_fixture.os.lstat

        def lstat_hook(path: str | os.PathLike[str], *args: Any, **kwargs: Any):
            if is_payload_open(path):
                replace_once()
            return original_lstat(path, *args, **kwargs)

        monkeypatch.setattr(local_fixture.os, "lstat", lstat_hook)
    elif stage == "open":
        original_open = local_fixture.os.open

        def open_hook(path: str | os.PathLike[str], *args: Any, **kwargs: Any):
            if is_payload_open(path):
                replace_once()
            return original_open(path, *args, **kwargs)

        monkeypatch.setattr(local_fixture.os, "open", open_hook)
    else:
        original_open = local_fixture.os.open
        original_fstat = local_fixture.os.fstat
        opened_payload_fds: set[int] = set()

        def open_hook(path: str | os.PathLike[str], *args: Any, **kwargs: Any):
            fd = original_open(path, *args, **kwargs)
            if is_payload_open(path):
                opened_payload_fds.add(fd)
            return fd

        def fstat_hook(fd: int):
            result = original_fstat(fd)
            if fd in opened_payload_fds:
                replace_once()
            return result

        monkeypatch.setattr(local_fixture.os, "open", open_hook)
        monkeypatch.setattr(local_fixture.os, "fstat", fstat_hook)

    captured = _capture_only(fixture.root)

    assert replaced
    assert captured.capture_issues
    assert any(
        issue.code in {"ADMISSION_NOT_LOCAL", "ADMISSION_HASH_MISMATCH", "ADMISSION_MANIFEST_MISMATCH"}
        for issue in captured.capture_issues
    )


def test_payload_identity_mismatch_does_not_return_replacement_bytes(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    fixture = write_SYNTHETIC_local_fixture(tmp_path)
    from build_finance.crypto_replay import local_fixture

    payload_path = fixture.payload_paths[0]
    target_paths = _local_fixture_open_targets(fixture, payload_path)
    replacement_payload = fixture.payloads[0] + b"\nSYNTHETIC_IDENTITY_MISMATCH_REPLACEMENT\n"
    replacement_path = tmp_path / "replacement-payload.json"
    replacement_path.write_bytes(replacement_payload)
    original_open = local_fixture.os.open
    original_close = local_fixture.os.close
    replacement_fd: int | None = None
    closed_fds: list[int] = []

    def open_hook(path: str | os.PathLike[str], flags: int, *args: Any, **kwargs: Any) -> int:
        nonlocal replacement_fd
        if _matches_local_fixture_open_target(path, target_paths):
            replacement_fd = original_open(replacement_path, flags, *args, **kwargs)
            return replacement_fd
        return original_open(path, flags, *args, **kwargs)

    def close_hook(fd: int) -> None:
        closed_fds.append(fd)
        original_close(fd)

    monkeypatch.setattr(local_fixture.os, "open", open_hook)
    monkeypatch.setattr(local_fixture.os, "close", close_hook)

    captured = _capture_only(fixture.root)

    assert replacement_fd is not None
    assert replacement_fd in closed_fds
    assert captured.files[0].payload is None
    assert captured.files[0].sha256 is None
    assert captured.files[0].byte_length is None
    assert sha256_hex(replacement_payload) not in {file.sha256 for file in captured.files}
    assert any(issue.code == "ADMISSION_MANIFEST_MISMATCH" for issue in captured.capture_issues)


def _local_fixture_open_targets(fixture: SYNTHETICLocalFixture, payload_path: Path) -> frozenset[str]:
    relative_path = str(fixture.manifest["files"][0]["relative_path"])
    return frozenset(
        {
            _normalized_probe_path(payload_path),
            _normalized_probe_path(os.path.abspath(os.fspath(payload_path))),
            _normalized_probe_path(relative_path),
            _normalized_probe_path(Path(relative_path).name),
        }
    )


def _matches_local_fixture_open_target(path: str | os.PathLike[str], target_paths: frozenset[str]) -> bool:
    raw_path = os.fspath(path)
    candidates = {_normalized_probe_path(raw_path)}
    if os.path.isabs(raw_path):
        candidates.add(_normalized_probe_path(os.path.abspath(raw_path)))
    return not candidates.isdisjoint(target_paths)


def _normalized_probe_path(path: str | os.PathLike[str]) -> str:
    return os.path.normcase(os.fspath(path).replace("\\", "/"))


def test_stable_receipts_across_roots(tmp_path: Path) -> None:
    left = write_SYNTHETIC_local_fixture(tmp_path / "left")
    right = write_SYNTHETIC_local_fixture(tmp_path / "right")

    _left_capture, left_batch = _capture_and_admit(left.root)
    _right_capture, right_batch = _capture_and_admit(right.root)

    assert left_batch.status == "ADMITTED"
    assert left_batch.source_receipt_records == right_batch.source_receipt_records
    assert left_batch.candidates == right_batch.candidates


def test_duplicate_version_idempotence_is_not_a_conflict(tmp_path: Path) -> None:
    spec = SYNTHETICEventSpec()
    duplicate = dataclasses.replace(
        spec,
        admission_sequence="2",
        relative_path="payloads/quote-0002.json",
        revision_availability_admission_sequence="1",
    )
    fixture = write_SYNTHETIC_local_fixture(tmp_path, event_specs=(duplicate, spec))

    _captured, batch = _capture_and_admit(fixture.root)

    assert batch.status == "ADMITTED"
    assert _reason_codes(batch) == ()
    assert len(_receipts(batch)) == 2
    assert len(batch.candidates) == 1
    assert batch.candidates[0].admission_sequence == "1"
    assert batch.candidates[0].relative_path == "payloads/quote-0001.json"
    assert batch.candidates[0].raw_payload_sha256 == sha256_hex(fixture.payloads[0])


def test_identical_candidate_collapse_uses_utf8_path_tiebreaker(tmp_path: Path) -> None:
    fixture = write_SYNTHETIC_local_fixture(tmp_path)
    captured = _capture_only(fixture.root)
    from build_finance.crypto_replay.admission import admit_local_fixture

    later_path = dataclasses.replace(captured.files[0], relative_path="payloads/z.json")
    earlier_path = dataclasses.replace(captured.files[0], relative_path="payloads/a.json")
    reordered = dataclasses.replace(captured, files=(later_path, earlier_path))

    batch = admit_local_fixture(reordered)
    receipts = _receipts(batch)

    assert batch.status == "ADMITTED"
    assert [receipt["relative_path"] for receipt in receipts] == ["payloads/a.json", "payloads/z.json"]
    assert len(batch.candidates) == 1
    assert batch.candidates[0].admission_sequence == captured.files[0].admission_sequence
    assert batch.candidates[0].relative_path == "payloads/a.json"


def test_parser_accepts_canonical_u64_maximum_values(tmp_path: Path) -> None:
    spec = SYNTHETICEventSpec(
        admission_sequence=_MAX_U64,
        availability_slot=_MAX_U64,
        source_subsequence=_MAX_U64,
        revision_availability_admission_sequence=_MAX_U64,
    )
    fixture = write_SYNTHETIC_local_fixture(tmp_path, event_specs=(spec,))
    from build_finance.crypto_replay.jupiter_fixture import parse_jupiter_fixture_payload

    parsed = parse_jupiter_fixture_payload(fixture.payloads[0])

    assert parsed.source_position_slot == _MAX_U64
    assert parsed.source_subsequence == _MAX_U64
    assert parsed.revision_availability_slot == _MAX_U64
    assert parsed.revision_availability_admission_sequence == _MAX_U64


@pytest.mark.parametrize(
    "spec",
    (
        SYNTHETICEventSpec(source_position_slot=_MAX_U64_PLUS_ONE),
        SYNTHETICEventSpec(source_subsequence=_MAX_U64_PLUS_ONE),
        SYNTHETICEventSpec(revision_availability_slot=_MAX_U64_PLUS_ONE),
        SYNTHETICEventSpec(revision_availability_admission_sequence=_MAX_U64_PLUS_ONE),
        SYNTHETICEventSpec(source_position_slot=_HUGE_U64_CANDIDATE),
        SYNTHETICEventSpec(source_subsequence=_HUGE_U64_CANDIDATE),
        SYNTHETICEventSpec(revision_availability_slot=_HUGE_U64_CANDIDATE),
        SYNTHETICEventSpec(revision_availability_admission_sequence=_HUGE_U64_CANDIDATE),
        SYNTHETICEventSpec(source_position_slot="01"),
        SYNTHETICEventSpec(source_subsequence="01"),
        SYNTHETICEventSpec(revision_availability_slot="01"),
        SYNTHETICEventSpec(revision_availability_admission_sequence="01"),
    ),
)
def test_parser_rejects_non_canonical_or_oversized_u64_values(tmp_path: Path, spec: SYNTHETICEventSpec) -> None:
    fixture = write_SYNTHETIC_local_fixture(tmp_path, event_specs=(spec,))
    from build_finance.crypto_replay.jupiter_fixture import JupiterFixtureParseError, parse_jupiter_fixture_payload

    with pytest.raises(JupiterFixtureParseError, match="canonical u64 string"):
        parse_jupiter_fixture_payload(fixture.payloads[0])


def test_admission_accepts_resealed_u64_maximum_fixture(tmp_path: Path) -> None:
    spec = SYNTHETICEventSpec(
        admission_sequence=_MAX_U64,
        availability_slot=_MAX_U64,
        source_subsequence=_MAX_U64,
        revision_availability_admission_sequence=_MAX_U64,
    )
    fixture = write_SYNTHETIC_local_fixture(tmp_path, event_specs=(spec,))

    _captured, batch = _capture_and_admit(fixture.root)

    receipt = _receipts(batch)[0]
    assert batch.status == "ADMITTED"
    assert _reason_codes(batch) == ()
    assert len(batch.candidates) == 1
    assert batch.candidates[0].admission_sequence == _MAX_U64
    assert batch.candidates[0].source_position["slot"] == _MAX_U64
    assert batch.candidates[0].source_position["source_subsequence"] == _MAX_U64
    assert batch.candidates[0].revision["availability_slot"] == _MAX_U64
    assert batch.candidates[0].revision["availability_admission_sequence"] == _MAX_U64
    assert receipt["admission_sequence"] == _MAX_U64
    assert receipt["availability_slot"] == _MAX_U64


@pytest.mark.parametrize("admission_sequence", (_MAX_U64_PLUS_ONE, _HUGE_U64_CANDIDATE, "01"))
def test_manifest_admission_sequence_defects_are_sequence_invalid(
    tmp_path: Path,
    admission_sequence: str,
) -> None:
    fixture = write_SYNTHETIC_local_fixture(_u64_case_root(tmp_path, "admission", admission_sequence))
    _reseal_first_manifest_file(fixture, admission_sequence=admission_sequence)

    captured, batch = _capture_and_admit(fixture.root)

    receipt = _receipts(batch)[0]
    assert batch.status == "QUARANTINED"
    assert _reason_codes(batch) == ("ADMISSION_SEQUENCE_INVALID",)
    assert batch.candidates == ()
    assert captured.files[0].admission_sequence == admission_sequence
    assert receipt["admission_sequence"] == admission_sequence


@pytest.mark.parametrize("availability_slot", (_MAX_U64_PLUS_ONE, _HUGE_U64_CANDIDATE, "01"))
def test_manifest_availability_slot_defects_are_revision_causality(
    tmp_path: Path,
    availability_slot: str,
) -> None:
    fixture = write_SYNTHETIC_local_fixture(_u64_case_root(tmp_path, "availability", availability_slot))
    _reseal_first_manifest_file(fixture, availability_slot=availability_slot)

    _captured, batch = _capture_and_admit(fixture.root)

    receipt = _receipts(batch)[0]
    assert batch.status == "QUARANTINED"
    assert _reason_codes(batch) == ("ADMISSION_REVISION_CAUSALITY",)
    assert batch.candidates == ()
    assert receipt["availability_slot"] == availability_slot


@pytest.mark.parametrize(
    "spec",
    (
        SYNTHETICEventSpec(source_position_slot=_MAX_U64_PLUS_ONE),
        SYNTHETICEventSpec(source_subsequence=_MAX_U64_PLUS_ONE),
        SYNTHETICEventSpec(revision_availability_slot=_MAX_U64_PLUS_ONE),
        SYNTHETICEventSpec(revision_availability_admission_sequence=_MAX_U64_PLUS_ONE),
    ),
)
def test_oversized_payload_u64_defects_do_not_emit_candidates(tmp_path: Path, spec: SYNTHETICEventSpec) -> None:
    fixture = write_SYNTHETIC_local_fixture(tmp_path, event_specs=(spec,))

    captured, batch = _capture_and_admit(fixture.root)

    assert batch.status == "REJECTED"
    assert "ADMISSION_PROFILE_MISMATCH" in _reason_codes(batch)
    assert batch.candidates == ()
    assert len(_receipts(batch)) == len(captured.files) == 1


def _module_origin(module: str) -> Path:
    spec = importlib.util.find_spec(module)
    assert spec is not None, f"{module} must be importable for parser identity closure"
    assert spec.origin is not None, f"{module} must have a local origin"
    return Path(spec.origin)


def _crypto_replay_runtime_closure(roots: tuple[str, ...]) -> tuple[dict[str, str], ...]:
    seen: set[str] = set()
    pending = list(roots)
    rows: list[dict[str, str]] = []
    while pending:
        module = pending.pop()
        if module in seen:
            continue
        seen.add(module)
        path = _module_origin(module)
        payload = path.read_bytes()
        rows.append({"module": module, "sha256": hashlib.sha256(payload).hexdigest(), "byte_length": str(len(payload))})
        tree = ast.parse(payload, filename=str(path))
        for node in ast.walk(tree):
            if isinstance(node, ast.Import):
                for alias in node.names:
                    if alias.name.startswith("build_finance.crypto_replay."):
                        pending.append(alias.name)
            elif (
                isinstance(node, ast.ImportFrom)
                and node.module
                and node.module.startswith("build_finance.crypto_replay")
            ):
                pending.append(node.module)
    return tuple(sorted(rows, key=lambda row: row["module"].encode("utf-8")))


def _local_resource_closure() -> tuple[dict[str, str], ...]:
    resources_root = Path("build_finance") / "crypto_replay" / "resources"
    rows: list[dict[str, str]] = []
    for path in sorted(resources_root.rglob("*"), key=lambda item: item.as_posix().encode("utf-8")):
        if not path.is_file():
            continue
        payload = path.read_bytes()
        rows.append(
            {
                "relative_path": path.relative_to(resources_root).as_posix(),
                "sha256": hashlib.sha256(payload).hexdigest(),
                "byte_length": str(len(payload)),
            }
        )
    assert rows, "parser identity resource closure must include local schema/formula resources"
    return tuple(rows)


def test_parser_identity_matches_independent_runtime_closure(tmp_path: Path) -> None:
    fixture = write_SYNTHETIC_local_fixture(tmp_path)
    from build_finance.crypto_replay.jupiter_fixture import PARSER_VERSION, current_parser_identity

    roots = (
        "build_finance.crypto_replay.local_fixture",
        "build_finance.crypto_replay.jupiter_fixture",
        "build_finance.crypto_replay.admission",
    )
    expected_bundle = {
        "parser_version": PARSER_VERSION,
        "runtime_modules": list(_crypto_replay_runtime_closure(roots)),
        "resources": list(_local_resource_closure()),
    }
    expected_digest = hashlib.sha256(canonical_json_bytes(expected_bundle)).hexdigest()
    identity = current_parser_identity()

    assert identity.version == "solana-jupiter-fixture-parser/v1"
    assert identity.code_sha256 == expected_digest
    assert fixture.root.is_relative_to(tmp_path)


def _artifact_files_to_scan() -> tuple[Path, ...]:
    roots = (
        Path("docs") / "crypto-replay",
        Path("build_finance") / "crypto_replay" / "resources",
    )
    files: list[Path] = [Path("pyproject.toml")]
    for root in roots:
        if root.exists():
            files.extend(path for path in root.rglob("*") if path.is_file())
    return tuple(sorted(files, key=lambda path: path.as_posix().encode("utf-8")))


def _assert_no_synthetic_or_p2_promotion_artifacts() -> None:
    forbidden_payloads = (
        b"SYNTHETIC_TEST_ONLY_INVALID",
        b"SYNTHETIC_BASE_MINT_0OIl_INVALID",
        "SYNTHETIC TEST FIXTURE — NOT MARKET DATA".encode(),
    )
    forbidden_path_parts = {
        "payloads",
        "terms",
        "witnesses",
        "candidates",
        "source-candidates",
        "source-receipts",
        "admitted",
    }
    leaks: list[str] = []
    for path in _artifact_files_to_scan():
        normalized_parts = {part.lower() for part in path.parts}
        if path.parts[:3] != ("docs", "crypto-replay", "evidence") and forbidden_path_parts.intersection(
            normalized_parts
        ):
            leaks.append(f"{path}:fixture-like artifact path")
        payload = path.read_bytes()
        for forbidden in forbidden_payloads:
            if forbidden in payload:
                leaks.append(f"{path}:synthetic sentinel bytes leaked")
    assert leaks == []

    promotion_path = Path("docs") / "crypto-replay" / "promotion-status.json"
    if promotion_path.exists():
        promotion = json.loads(promotion_path.read_text(encoding="utf-8"))
        assert (
            promotion.get("P2") == "FAIL_ZERO_ADMITTED_FIXTURE" or promotion.get("p2") == "FAIL_ZERO_ADMITTED_FIXTURE"
        )
        assert promotion.get("real_fixture_identity") is None


def test_universe_declaration_does_not_write_or_promote_real_p2_artifacts(tmp_path: Path) -> None:
    fixture = write_SYNTHETIC_local_fixture(tmp_path)
    _assert_no_synthetic_or_p2_promotion_artifacts()

    _captured, batch = _capture_and_admit(fixture.root)

    assert fixture.manifest["universe_policy"] == "FULL_DECLARED_SOURCE_UNIVERSE"
    assert batch.status == "ADMITTED"
    assert not hasattr(batch, "promotion_status")
    assert not hasattr(batch, "real_fixture_identity")
    assert not (Path("docs") / "crypto-replay" / "T03-green.json").exists()
    _assert_no_synthetic_or_p2_promotion_artifacts()
    for path in (*fixture.payload_paths, fixture.terms_path, *fixture.witness_paths):
        assert path.is_relative_to(tmp_path)
    assert SYNTHETIC_PAYLOAD_SOURCE_SENTINEL in fixture.payloads[0]
    assert SYNTHETIC_PAYLOAD_BASE_SENTINEL in fixture.payloads[0]
    assert fixture.terms_payload.decode("utf-8").startswith(SYNTHETIC_TERMS_PREFIX)
    assert SYNTHETIC_SOURCE_ID in fixture.payloads[0].decode("utf-8")
    assert SYNTHETIC_BASE_MINT in fixture.payloads[0].decode("utf-8")
    assert SYNTHETIC_EVENT_TIME in fixture.payloads[0].decode("utf-8")
    assert SYNTHETIC_OBSERVED_AT in fixture.witness_paths[0].read_text(encoding="utf-8")
    assert SYNTHETIC_INGESTED_AT in fixture.witness_paths[0].read_text(encoding="utf-8")


def test_rights_manifest_digest_mismatch_maps_manifest_mismatch(tmp_path: Path) -> None:
    fixture = write_SYNTHETIC_local_fixture(tmp_path)
    rights = dict(fixture.rights_manifest)
    rights["retention_posture"] = "LOCAL_RESEARCH_RETENTION_CHANGED"
    rewrite_SYNTHETIC_rights(fixture, rights)

    _captured, batch = _capture_and_admit(fixture.root)

    assert batch.status == "REJECTED"
    assert _reason_codes(batch) == ("ADMISSION_MANIFEST_MISMATCH",)


def test_rights_manifest_digest_must_cover_complete_canonical_lf_record(tmp_path: Path) -> None:
    fixture = write_SYNTHETIC_local_fixture(tmp_path)
    manifest = dict(fixture.manifest)
    manifest["rights_manifest_sha256"] = sha256_hex(fixture.rights_manifest_path.read_bytes()[:-1])
    reseal_SYNTHETIC_manifest(fixture, manifest)

    _captured, batch = _capture_and_admit(fixture.root)

    assert batch.status == "REJECTED"
    assert _reason_codes(batch) == ("ADMISSION_MANIFEST_MISMATCH",)


def test_noncanonical_witness_maps_point_in_time_missing(tmp_path: Path) -> None:
    spec = SYNTHETICEventSpec()
    fixture = write_SYNTHETIC_local_fixture(tmp_path, event_specs=(spec,))
    rewrite_SYNTHETIC_witness(fixture, 0, witness_for_SYNTHETIC_fixture(fixture, 0, spec), canonical=False)

    _captured, batch = _capture_and_admit(fixture.root)

    assert batch.status == "REJECTED"
    assert _reason_codes(batch) == ("ADMISSION_POINT_IN_TIME_MISSING",)


def test_fixtures_never_escape_tmp_path_when_copied(tmp_path: Path) -> None:
    fixture = write_SYNTHETIC_local_fixture(tmp_path / "original")
    copied_root = tmp_path / "copy"
    shutil.copytree(fixture.root, copied_root)

    _captured, batch = _capture_and_admit(copied_root)

    assert batch.status == "ADMITTED"
    assert copied_root.is_relative_to(tmp_path)
