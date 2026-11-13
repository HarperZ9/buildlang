"""Qualitative contract for causal G2 grouping and feature snapshots."""

from __future__ import annotations

import copy
import hashlib
from collections.abc import Mapping, Sequence
from typing import Any

import pytest

from build_finance.crypto_replay.canonical import canonical_record_bytes, parse_canonical_record
from build_finance.crypto_replay.content_ids import seal_content_id as seal_replay_content_id
from build_finance.crypto_replay.schema_definitions import SELF_ID_FIELDS as REPLAY_SELF_ID_FIELDS
from build_finance.crypto_replay.schema_registry import require_valid_contract as require_valid_replay_contract
from build_finance.live_paper.content_ids import canonical_record_bytes as live_record_bytes
from build_finance.live_paper.content_ids import seal_content_id as seal_live_content_id
from build_finance.live_paper.features import derive_feature_snapshot
from build_finance.live_paper.grouping import EventGroup, group_committed_events
from build_finance.live_paper.profiles import PaperKernelProfiles
from build_finance.live_paper.registry import require_valid_contract as require_valid_live_contract
from build_finance.live_paper.run_input_builder import build_verified_run_inputs
from tests.live_paper.support.g2_vectors import G2Vector, build_g2_vector, parse_live_record


def _profiles(vector: G2Vector) -> PaperKernelProfiles:
    return PaperKernelProfiles(**vector.profile_records)  # type: ignore[arg-type]


def _build(vector: G2Vector, *, reverse: bool = False) -> Any:
    candidates = vector.admitted_candidates[::-1] if reverse else vector.admitted_candidates
    receipts = vector.source_receipt_records[::-1] if reverse else vector.source_receipt_records
    return build_verified_run_inputs(
        vector.run_receipt_record,
        candidates,
        receipts,
        vector.resolver,
        _profiles(vector),
    )


def _merkle_root(event_ids: Sequence[str]) -> str:
    level = [hashlib.sha256(b"\x00" + bytes.fromhex(event_id)).digest() for event_id in event_ids]
    if not level:
        raise ValueError("test Merkle input must be non-empty")
    while len(level) > 1:
        if len(level) % 2:
            level.append(level[-1])
        level = [
            hashlib.sha256(b"\x01" + level[index] + level[index + 1]).digest()
            for index in range(0, len(level), 2)
        ]
    return level[0].hex()


def _snapshot(record: bytes) -> dict[str, Any]:
    document = parse_canonical_record(record)
    require_valid_replay_contract(document, expected_schema="trading.feature-snapshot/v1")
    return document


def _reseal_event(document: Mapping[str, Any]) -> bytes:
    body = copy.deepcopy(dict(document))
    body.pop(REPLAY_SELF_ID_FIELDS["trading.raw-event/v1"], None)
    sealed = seal_replay_content_id(body)
    require_valid_replay_contract(sealed, expected_schema="trading.raw-event/v1")
    return canonical_record_bytes(sealed)


def _revision_event(
    prior_record: bytes,
    *,
    kind: str,
    target_id: str,
    sequence: int,
    quote_amount_atoms: str,
) -> bytes:
    event = parse_canonical_record(prior_record)
    event["admission_sequence"] = str(sequence)
    event["source_sequence"] = str(sequence)
    event["ingest_sequence"] = str(sequence)
    event["equal_time_group"] = str(sequence)
    event["replay_clock_ns"] = str((sequence - 1) * 1_000_000_000)
    event["event_time"] = f"2026-01-01T00:00:0{sequence}.000000000Z"
    event["observed_at"] = f"2026-01-02T00:00:0{sequence}.000000000Z"
    event["ingested_at"] = f"2026-01-02T00:00:0{sequence}.000000001Z"
    position = copy.deepcopy(event["source_position"])
    assert isinstance(position, dict)
    position["slot"] = str(sequence)
    position["source_native_event_id"] = f"synthetic-revision-{sequence}"
    event["source_position"] = position
    event["revision"] = {
        "kind": kind,
        "supersedes_event_id": target_id if kind == "CORRECTION" else None,
        "retracts_event_id": target_id if kind == "RETRACTION" else None,
        "availability_slot": str(sequence),
        "availability_admission_sequence": str(sequence),
    }
    market = copy.deepcopy(event["market"])
    assert isinstance(market, dict)
    market["quote_amount_atoms"] = quote_amount_atoms
    event["market"] = market
    event["executable"] = kind != "RETRACTION"
    event["quality_flags"] = ["PROVIDER_REVISION"] if kind == "CORRECTION" else ["RETRACTED_SOURCE"]
    return _reseal_event(event)


def _manual_group(
    event_record: bytes,
    *,
    group_sequence: int,
    feature_code_sha256: str,
    normalization_code_sha256: str,
) -> EventGroup:
    event = parse_canonical_record(event_record)
    event_id = str(event["event_id"])
    source_id = str(event["source_admission_receipt_id"])
    receipt = seal_live_content_id(
        {
            "schema": "trading.normalization-receipt/v1",
            "source_batch_id": f"g2-decision-group-{group_sequence}",
            "normalization_code_sha256": normalization_code_sha256,
            "input_content_ids": [source_id],
            "input_count": "1",
            "normalized_event_ids": [event_id],
            "normalized_event_count": "1",
            "output_merkle_root_sha256": _merkle_root((event_id,)),
            "status": "PASS",
            "reason_codes": [],
            "total_evidence": "TOTAL_INPUT_CLOSURE",
        }
    )
    require_valid_live_contract(receipt, expected_schema="trading.normalization-receipt/v1")
    return EventGroup(
        group_sequence=str(group_sequence),
        event_ids=(event_id,),
        availability_slot=str(group_sequence),
        event_records=(event_record,),
        normalization_receipt_record=live_record_bytes(receipt),
        feature_code_sha256=feature_code_sha256,
    )


def test_two_groups_are_causal_and_emit_exact_repeatable_features() -> None:
    vector = build_g2_vector()
    groups = group_committed_events(_build(vector))

    assert tuple(group.group_sequence for group in groups) == ("1", "2")
    assert tuple(group.event_ids for group in groups) == tuple(
        (str(event["event_id"]),) for event in vector.replay_vector.raw_events
    )
    for group in groups:
        receipt = parse_live_record(group.normalization_receipt_record)
        require_valid_live_contract(receipt, expected_schema="trading.normalization-receipt/v1")
        events = tuple(parse_canonical_record(record) for record in group.event_records)
        assert receipt["input_content_ids"] == [event["source_admission_receipt_id"] for event in events]
        assert receipt["normalized_event_ids"] == list(group.event_ids)
        assert receipt["output_merkle_root_sha256"] == _merkle_root(group.event_ids)
        assert group.feature_code_sha256 == vector.run_receipt["feature_code_sha256"]

    first_bytes = derive_feature_snapshot(groups[:1])
    second_bytes = derive_feature_snapshot(groups)
    assert first_bytes == derive_feature_snapshot(groups[:1])
    assert second_bytes == derive_feature_snapshot(groups)
    first = _snapshot(first_bytes)
    second = _snapshot(second_bytes)

    assert first["as_of_event_id"] == groups[0].event_ids[0]
    assert first["input_merkle_root_sha256"] == _merkle_root(groups[0].event_ids)
    assert first["features"] == {
        "mid_price_q18": "1000001000000000000",
        "return_1_q18": None,
        "ema_fast_price_q18": None,
        "ema_slow_price_q18": None,
        "rsi_14_q18": None,
        "atr_14_price_q18": None,
        "breakout_high_20_price_q18": None,
        "breakout_low_20_price_q18": None,
        "volume_20_base_atoms": None,
        "liquidity_quote_atoms": "10000000",
        "stale_age_ns": "0",
        "route_impact_bps": 25,
        "history_count": 1,
    }
    assert first["missing_features"] == [
        "atr_14_price_q18",
        "breakout_high_20_price_q18",
        "breakout_low_20_price_q18",
        "ema_fast_price_q18",
        "ema_slow_price_q18",
        "return_1_q18",
        "rsi_14_q18",
        "volume_20_base_atoms",
    ]
    assert second["input_merkle_root_sha256"] == _merkle_root(groups[0].event_ids + groups[1].event_ids)
    assert second["features"]["mid_price_q18"] == "1000002000000000000"
    assert second["features"]["return_1_q18"] == "999999000000"
    assert second["features"]["liquidity_quote_atoms"] == "10000000"
    assert second["features"]["route_impact_bps"] == 25
    assert second["features"]["stale_age_ns"] == "0"
    assert second["features"]["history_count"] == 2
    assert "return_1_q18" not in second["missing_features"]
    assert first["snapshot_id"] != second["snapshot_id"]


def test_reversed_candidate_receipt_pairs_are_byte_invariant() -> None:
    vector = build_g2_vector()
    forward = group_committed_events(_build(vector))
    reversed_pairs = group_committed_events(_build(vector, reverse=True))

    assert forward == reversed_pairs
    assert tuple(derive_feature_snapshot(forward[:index]) for index in (1, 2)) == tuple(
        derive_feature_snapshot(reversed_pairs[:index]) for index in (1, 2)
    )


def test_revision_lifecycle_changes_only_active_state_and_rejects_unknown_target() -> None:
    vector = build_g2_vector()
    original_group = group_committed_events(_build(vector))[:1]
    original = parse_canonical_record(original_group[0].event_records[0])
    feature_code = str(vector.run_receipt["feature_code_sha256"])
    normalization_code = str(vector.run_receipt["normalization_code_sha256"])
    correction_record = _revision_event(
        original_group[0].event_records[0],
        kind="CORRECTION",
        target_id=str(original["event_id"]),
        sequence=2,
        quote_amount_atoms="2000000",
    )
    correction = parse_canonical_record(correction_record)
    correction_group = _manual_group(
        correction_record,
        group_sequence=2,
        feature_code_sha256=feature_code,
        normalization_code_sha256=normalization_code,
    )
    retraction_record = _revision_event(
        correction_record,
        kind="RETRACTION",
        target_id=str(correction["event_id"]),
        sequence=3,
        quote_amount_atoms="2000000",
    )
    retraction_group = _manual_group(
        retraction_record,
        group_sequence=3,
        feature_code_sha256=feature_code,
        normalization_code_sha256=normalization_code,
    )

    first_bytes = derive_feature_snapshot(original_group)
    second_bytes = derive_feature_snapshot((*original_group, correction_group))
    third_bytes = derive_feature_snapshot((*original_group, correction_group, retraction_group))
    first = _snapshot(first_bytes)
    second = _snapshot(second_bytes)
    third = _snapshot(third_bytes)

    assert first["features"]["mid_price_q18"] == "1000001000000000000"
    assert second["features"]["mid_price_q18"] == "2000000000000000000"
    assert second["features"]["history_count"] == 1
    assert third["features"]["mid_price_q18"] is None
    assert third["features"]["history_count"] == 0
    assert third["input_merkle_root_sha256"] == _merkle_root(
        (original_group[0].event_ids[0], correction_group.event_ids[0], retraction_group.event_ids[0])
    )
    assert first_bytes == derive_feature_snapshot(original_group)
    assert second_bytes == derive_feature_snapshot((*original_group, correction_group))

    unknown_record = _revision_event(
        original_group[0].event_records[0],
        kind="CORRECTION",
        target_id="f" * 64,
        sequence=2,
        quote_amount_atoms="2000000",
    )
    unknown_group = _manual_group(
        unknown_record,
        group_sequence=2,
        feature_code_sha256=feature_code,
        normalization_code_sha256=normalization_code,
    )
    with pytest.raises(ValueError, match="active earlier"):
        derive_feature_snapshot((*original_group, unknown_group))
