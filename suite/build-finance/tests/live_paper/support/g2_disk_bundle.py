"""Test-only disk materializer for the synthetic G2 replay envelope."""

from __future__ import annotations

import copy
from collections.abc import Mapping
from dataclasses import dataclass
from pathlib import Path
from typing import Any, cast

from build_finance.crypto_replay.admission import admit_local_fixture
from build_finance.crypto_replay.canonical import (
    JsonObject,
    JsonValue,
    canonical_json_bytes,
    canonical_record_bytes,
    parse_canonical_record,
    sha256_hex,
)
from build_finance.crypto_replay.content_ids import seal_content_id
from build_finance.crypto_replay.local_fixture import capture_local_fixture
from tests.live_paper.support.g2_vectors import build_g2_vector, reseal_replay_document

EXPECTED_ENVELOPE = {
    "layout": "content-addressed-v1",
    "model_signal_mode": "DISABLED",
    "paper_core_distribution": "build-finance-paper-core",
    "paper_core_version": "1.1.0",
    "schema": "build-finance.paper-core.replay-envelope/v1",
}

_SOURCE_ID = "synthetic-local-fixture"
_SOURCE_KIND = "solana-jupiter-quote"
_SOURCE_REVISION = "synthetic-revision-1"
_MARKET_ID = "synthetic-base/synthetic-quote:jupiter"
_BASE_MINT = "synthetic-base"
_QUOTE_MINT = "synthetic-quote"
_BASE_DECIMALS = 9
_QUOTE_DECIMALS = 6
_TERMS_PAYLOAD = b"SYNTHETIC G2 DISK BUNDLE TERMS - NOT MARKET DATA\n"


@dataclass(frozen=True, slots=True)
class G2DiskBundle:
    """Paths and identities for one materialized synthetic disk bundle."""

    fixture_root: Path
    run_receipt_path: Path
    envelope_path: Path
    record_content_ids: tuple[str, ...]
    byte_sha256s: tuple[str, ...]
    linked_member_target_sha256: str


def write_g2_disk_bundle(tmp_path: Path) -> G2DiskBundle:
    """Write one admitted source fixture plus replay envelope under ``tmp_path``."""

    root = tmp_path / "g2-disk-fixture"
    root.mkdir(parents=True)
    _write_source_fixture(root)

    captured = capture_local_fixture(root)
    admission = admit_local_fixture(captured)
    if admission.status != "ADMITTED" or admission.reason_codes:
        raise AssertionError(f"test fixture admission failed: {admission.status} {admission.reason_codes}")

    graph = _build_replay_graph(captured.manifest_payload, admission.source_receipt_records)
    envelope_root = root / "replay-envelope"
    records_dir = envelope_root / "records"
    bytes_dir = envelope_root / "bytes"
    profiles_dir = envelope_root / "profiles"
    records_dir.mkdir(parents=True)
    bytes_dir.mkdir(parents=True)
    profiles_dir.mkdir(parents=True)

    envelope_path = envelope_root / "envelope.json"
    envelope_path.write_bytes(canonical_json_bytes(EXPECTED_ENVELOPE))
    run_receipt_path = envelope_root / "run-receipt.json"
    run_receipt_path.write_bytes(graph.run_receipt_record)

    for content_id, record in graph.records_by_content_id.items():
        (records_dir / f"{content_id}.json").write_bytes(record)
    for digest, payload in graph.bytes_by_sha256.items():
        (bytes_dir / f"{digest}.bin").write_bytes(payload)

    profiles_dir.joinpath("feature.bin").write_bytes(graph.profile_files["feature"])
    profiles_dir.joinpath("algorithm-0.bin").write_bytes(graph.profile_files["algorithm-0"])
    profiles_dir.joinpath("model-validation.bin").write_bytes(graph.profile_files["model-validation"])
    profiles_dir.joinpath("fusion.bin").write_bytes(graph.profile_files["fusion"])
    profiles_dir.joinpath("fill.bin").write_bytes(graph.profile_files["fill"])

    return G2DiskBundle(
        fixture_root=root,
        run_receipt_path=run_receipt_path,
        envelope_path=envelope_path,
        record_content_ids=tuple(graph.records_by_content_id),
        byte_sha256s=tuple(graph.bytes_by_sha256),
        linked_member_target_sha256=next(iter(graph.bytes_by_sha256)),
    )


def corrupt_first_byte_payload(bundle: G2DiskBundle) -> None:
    """Rewrite one digest-addressed payload so its filename no longer matches its bytes."""

    target = bundle.fixture_root / "replay-envelope" / "bytes" / f"{bundle.byte_sha256s[0]}.bin"
    target.write_bytes(b"SYNTHETIC DIGEST MISMATCH\n")


def corrupt_first_record_payload(bundle: G2DiskBundle) -> None:
    """Rewrite one content-addressed record so its filename no longer matches its body."""

    target = bundle.fixture_root / "replay-envelope" / "records" / f"{bundle.record_content_ids[0]}.json"
    document = parse_canonical_record(target.read_bytes())
    document["schema"] = "trading.raw-event/v1"
    target.write_bytes(canonical_record_bytes(cast(JsonObject, document)))


def rewrite_envelope_mode(bundle: G2DiskBundle, mode: str) -> None:
    """Rewrite the envelope with a non-DISABLED model mode."""

    changed = dict(EXPECTED_ENVELOPE)
    changed["model_signal_mode"] = mode
    bundle.envelope_path.write_bytes(canonical_json_bytes(changed))


def replace_first_byte_with_symlink(bundle: G2DiskBundle, tmp_path: Path) -> bool:
    """Replace one envelope byte member with a symlink when the platform allows it."""

    target = bundle.fixture_root / "replay-envelope" / "bytes" / f"{bundle.linked_member_target_sha256}.bin"
    outside = tmp_path / "outside-linked-payload.bin"
    outside.write_bytes(target.read_bytes())
    target.unlink()
    try:
        target.symlink_to(outside)
    except OSError:
        outside.unlink(missing_ok=True)
        return False
    return True


@dataclass(frozen=True, slots=True)
class _ReplayGraph:
    run_receipt_record: bytes
    records_by_content_id: Mapping[str, bytes]
    bytes_by_sha256: Mapping[str, bytes]
    profile_files: Mapping[str, bytes]


def _write_source_fixture(root: Path) -> None:
    rights = {
        "schema": "build-finance.synthetic-rights-manifest/v1",
        "rights_role": "offline_research_replay",
        "rights_effective_date": "2026-01-01",
        "rights_review_date": "2026-01-02",
        "retention_posture": "LOCAL_RESEARCH_RETENTION",
        "redistribution_posture": "NO_REDISTRIBUTION",
        "provenance_requirements": [
            "CITE_SOURCE_ID",
            "PRESERVE_RAW_SHA256",
            "PRESERVE_TERMS_SHA256",
        ],
        "terms": [{"relative_path": "terms/synthetic-terms.txt", "terms_sha256": sha256_hex(_TERMS_PAYLOAD)}],
    }
    _write(root / "terms" / "synthetic-terms.txt", _TERMS_PAYLOAD)
    rights_payload = canonical_record_bytes(cast(JsonObject, rights))
    _write(root / "rights" / "rights-manifest.json", rights_payload)

    payloads = tuple(_payload(index) for index in (1, 2))
    file_rows: list[JsonObject] = []
    for index, payload in enumerate(payloads, start=1):
        admission_sequence = str(1 if index == 1 else 3)
        relative_path = f"payloads/group-{index}.json"
        digest = sha256_hex(payload)
        _write(root / relative_path, payload)
        _write(
            root / "witnesses" / f"witness-{int(admission_sequence):04d}.json",
            canonical_record_bytes(
                {
                    "schema": "build-finance.local-fixture-witness/v1",
                    "admission_sequence": admission_sequence,
                    "relative_path": relative_path,
                    "raw_payload_sha256": digest,
                    "byte_length": str(len(payload)),
                    "observed_at": f"2026-01-02T00:00:0{index}.000000000Z",
                    "ingested_at": f"2026-01-02T00:00:0{index}.000000001Z",
                }
            ),
        )
        file_rows.append(
            {
                "relative_path": relative_path,
                "raw_payload_sha256": digest,
                "byte_length": str(len(payload)),
                "admission_sequence": admission_sequence,
                "availability_slot": str(index),
                "media_type": "application/json",
                "source_id": _SOURCE_ID,
                "source_kind": _SOURCE_KIND,
                "source_revision": _SOURCE_REVISION,
                "market_id": _MARKET_ID,
            }
        )

    manifest = seal_content_id(
        {
            "schema": "trading.fixture-manifest/v1",
            "network": "solana-mainnet",
            "venue_profile": "solana-jupiter-fixture/v1",
            "quote_mint": _QUOTE_MINT,
            "quote_decimals": _QUOTE_DECIMALS,
            "initial_quote_atoms": "1000000",
            "replay_tick_ns": "1000000000",
            "universe_policy": "FULL_DECLARED_SOURCE_UNIVERSE",
            "point_in_time_mode": "PROVEN_AVAILABILITY_SLOT",
            "run_end_position_policy": "LEAVE_MARKED_OPEN",
            "session_start_availability_slot": "1",
            "session_end_availability_slot": "2",
            "rights_manifest_sha256": sha256_hex(rights_payload),
            "selection_failures_retained": True,
            "no_route_observations_retained": True,
            "inactive_assets_retained": True,
            "gaps_retained": True,
            "allowed_markets": [
                {
                    "market_id": _MARKET_ID,
                    "base_mint": _BASE_MINT,
                    "quote_mint": _QUOTE_MINT,
                    "base_decimals": _BASE_DECIMALS,
                    "quote_decimals": _QUOTE_DECIMALS,
                }
            ],
            "files": file_rows,
        }
    )
    _write(root / "manifest.json", canonical_record_bytes(manifest))


def _payload(index: int) -> bytes:
    payload: JsonObject = {
        "schema": "build-finance.live-paper.synthetic-jupiter-normalization-input/v1",
        "source_id": _SOURCE_ID,
        "source_kind": _SOURCE_KIND,
        "source_revision": _SOURCE_REVISION,
        "market_id": _MARKET_ID,
        "base_mint": _BASE_MINT,
        "quote_mint": _QUOTE_MINT,
        "base_decimals": _BASE_DECIMALS,
        "quote_decimals": _QUOTE_DECIMALS,
        "source_position": {
            "slot": str(index),
            "transaction_index": 0,
            "instruction_index": 0,
            "event_index": 0,
            "source_native_event_id": f"synthetic-quote-{index}",
            "source_subsequence": "0",
        },
        "revision": {
            "kind": "ORIGINAL",
            "supersedes_event_id": None,
            "retracts_event_id": None,
            "availability_slot": str(index),
            "availability_admission_sequence": str(1 if index == 1 else 3),
        },
        "event_time": f"2026-01-01T00:00:0{index}.000000000Z",
        "route": {"route_capacity_base_atoms": "5000000000"},
        "liquidity": {"liquidity_quote_atoms": "10000000"},
        "fees": {"venue_fee_quote_atoms": "2500", "priority_fee_quote_atoms": "0"},
        "event_kind": "ROUTE_QUOTE",
        "executable": True,
        "quality_flags": [],
        "source_sequence": str(index),
        "ingest_sequence": str(index),
        "equal_time_group": str(index),
        "replay_clock_ns": str((index - 1) * 1_000_000_000),
        "base_amount_atoms": "1000000000",
        "quote_amount_atoms": str(1_000_000 + index),
        "route_impact_bps": 25,
    }
    return canonical_json_bytes(payload)


def _build_replay_graph(
    fixture_manifest_record: bytes | None,
    source_receipt_records: tuple[bytes, ...],
) -> _ReplayGraph:
    if fixture_manifest_record is None:
        raise AssertionError("captured fixture manifest bytes are required")
    vector = build_g2_vector()
    fixture = parse_canonical_record(fixture_manifest_record)
    source_receipts = tuple(parse_canonical_record(record) for record in source_receipt_records)
    source_ids = sorted(str(receipt["source_admission_receipt_id"]) for receipt in source_receipts)
    raw_events = tuple(_raw_event(fixture, receipt, index) for index, receipt in enumerate(source_receipts, start=1))

    documents = {
        "trading.fixture-manifest/v1": fixture,
        "trading.config-admission-receipt/v1": copy.deepcopy(
            vector.replay_vector.documents["trading.config-admission-receipt/v1"]
        ),
        "trading.replay-risk-config/v1": copy.deepcopy(
            vector.replay_vector.documents["trading.replay-risk-config/v1"]
        ),
    }

    attachments = copy.deepcopy(vector.replay_vector.attachments)
    config_id = str(documents["trading.config-admission-receipt/v1"]["config_admission_receipt_id"])
    availability = {
        "schema": "trading.availability-schedule/v1",
        "fixture_manifest_sha256": fixture["fixture_manifest_sha256"],
        "config_admission_receipt_id": config_id,
        "source_admission_receipt_ids": source_ids,
        "availability_groups": [
            {"availability_slot": "1", "equal_time_group": "1", "admission_cutoff": "2"},
            {"availability_slot": "2", "equal_time_group": "2", "admission_cutoff": "3"},
        ],
    }
    normalized_set = {
        "schema": "trading.normalized-event-set/v1",
        "fixture_manifest_sha256": fixture["fixture_manifest_sha256"],
        "raw_event_count": "2",
        "event_ids": [event["event_id"] for event in raw_events],
    }
    normalized_payload = canonical_json_bytes(normalized_set)
    counter = {
        **copy.deepcopy(vector.replay_vector.attachments["trading.counter-capacity/v1"]),
        "normalized_event_set_sha256": sha256_hex(normalized_payload),
    }
    availability_payload = canonical_json_bytes(cast(JsonObject, availability))
    counter_payload = canonical_json_bytes(cast(JsonObject, counter))
    attachments["trading.availability-schedule/v1"] = cast(dict[str, Any], availability)
    attachments["trading.normalized-event-set/v1"] = normalized_set
    attachments["trading.counter-capacity/v1"] = cast(dict[str, Any], counter)

    closure = copy.deepcopy(vector.replay_vector.documents["trading.run-closure-receipt/v1"])
    closure.update(
        {
            "fixture_manifest_sha256": fixture["fixture_manifest_sha256"],
            "config_admission_receipt_id": config_id,
            "source_admission_receipt_ids": source_ids,
            "availability_schedule_sha256": sha256_hex(availability_payload),
            "counter_capacity_sha256": sha256_hex(counter_payload),
        }
    )
    closure = reseal_replay_document(closure)
    documents["trading.run-closure-receipt/v1"] = closure

    run = copy.deepcopy(vector.replay_vector.documents["trading.run-receipt/v1"])
    run.update(
        {
            "fixture_manifest_sha256": fixture["fixture_manifest_sha256"],
            "source_admission_receipt_ids": source_ids,
            "availability_schedule_sha256": sha256_hex(availability_payload),
            "run_closure_receipt_id": closure["run_closure_receipt_id"],
        }
    )
    run = reseal_replay_document(run)
    documents["trading.run-receipt/v1"] = run

    records = _records_by_content_id((*documents.values(), *source_receipts, *raw_events))
    bytes_by_sha256 = _bytes_by_sha256(
        (
            vector.replay_vector.raw_config_bytes,
            availability_payload,
            normalized_payload,
            counter_payload,
            vector.replay_vector.attachment_payloads["trading.source-tree/v1"],
            vector.replay_vector.public_seed_bytes,
            *tuple(_payload(index) for index in (1, 2)),
            *vector.replay_vector.code_preimages.values(),
        )
    )
    profiles = vector.profile_records
    profile_files = {
        "feature": bytes(cast(bytes, profiles["feature_profile_record"])),
        "algorithm-0": bytes(cast(tuple[bytes, ...], profiles["algorithm_profile_records"])[0]),
        "model-validation": bytes(cast(bytes, profiles["model_validation_profile_record"])),
        "fusion": bytes(cast(bytes, profiles["fusion_profile_record"])),
        "fill": bytes(cast(bytes, profiles["fill_profile_record"])),
    }
    return _ReplayGraph(
        run_receipt_record=canonical_record_bytes(run),
        records_by_content_id=records,
        bytes_by_sha256=bytes_by_sha256,
        profile_files=profile_files,
    )


def _raw_event(
    fixture: Mapping[str, JsonValue],
    source_receipt: Mapping[str, JsonValue],
    index: int,
) -> JsonObject:
    position = {
        "slot": str(index),
        "transaction_index": 0,
        "instruction_index": 0,
        "event_index": 0,
        "source_native_event_id": f"synthetic-quote-{index}",
        "source_subsequence": "0",
    }
    revision = {
        "kind": "ORIGINAL",
        "supersedes_event_id": None,
        "retracts_event_id": None,
        "availability_slot": str(index),
        "availability_admission_sequence": str(source_receipt["admission_sequence"]),
    }
    return seal_content_id(
        {
            "schema": "trading.raw-event/v1",
            "fixture_manifest_sha256": fixture["fixture_manifest_sha256"],
            "source_admission_receipt_id": source_receipt["source_admission_receipt_id"],
            "raw_payload_sha256": source_receipt["raw_payload_sha256"],
            "source_id": source_receipt["source_id"],
            "source_kind": source_receipt["source_kind"],
            "source_revision": source_receipt["source_revision"],
            "market_id": source_receipt["market_id"],
            "network": fixture["network"],
            "venue_profile": fixture["venue_profile"],
            "base_mint": _BASE_MINT,
            "quote_mint": _QUOTE_MINT,
            "base_decimals": _BASE_DECIMALS,
            "quote_decimals": _QUOTE_DECIMALS,
            "admission_sequence": source_receipt["admission_sequence"],
            "source_sequence": str(index),
            "ingest_sequence": str(index),
            "equal_time_group": str(index),
            "replay_clock_ns": str((index - 1) * 1_000_000_000),
            "event_time": f"2026-01-01T00:00:0{index}.000000000Z",
            "observed_at": source_receipt["observed_at"],
            "ingested_at": source_receipt["ingested_at"],
            "source_position": position,
            "revision": revision,
            "event_kind": "ROUTE_QUOTE",
            "executable": True,
            "quality_flags": [],
            "market": {
                "base_amount_atoms": "1000000000",
                "quote_amount_atoms": str(1_000_000 + index),
                "route_capacity_base_atoms": "5000000000",
                "liquidity_quote_atoms": "10000000",
                "venue_fee_quote_atoms": "2500",
                "priority_fee_quote_atoms": "0",
                "route_impact_bps": 25,
            },
        }
    )


def _records_by_content_id(documents: tuple[Mapping[str, Any], ...]) -> dict[str, bytes]:
    records: dict[str, bytes] = {}
    for document in documents:
        content_id = str(document[_self_id_field(document)])
        records[content_id] = canonical_record_bytes(cast(JsonObject, document))
    return records


def _bytes_by_sha256(payloads: tuple[bytes, ...]) -> dict[str, bytes]:
    return {sha256_hex(payload): bytes(payload) for payload in payloads}


def _self_id_field(document: Mapping[str, Any]) -> str:
    schema = str(document["schema"])
    if schema == "trading.fixture-manifest/v1":
        return "fixture_manifest_sha256"
    if schema == "trading.config-admission-receipt/v1":
        return "config_admission_receipt_id"
    if schema == "trading.replay-risk-config/v1":
        return "config_sha256"
    if schema == "trading.run-closure-receipt/v1":
        return "run_closure_receipt_id"
    if schema == "trading.run-receipt/v1":
        return "run_receipt_id"
    if schema == "trading.source-admission-receipt/v1":
        return "source_admission_receipt_id"
    if schema == "trading.raw-event/v1":
        return "event_id"
    raise AssertionError(f"unexpected test document schema: {schema}")


def _write(path: Path, payload: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(payload)
