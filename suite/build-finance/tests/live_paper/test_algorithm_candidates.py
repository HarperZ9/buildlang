"""Qualitative contract for fixed G2 algorithm evidence."""

from __future__ import annotations

import copy
import hashlib
from dataclasses import replace
from typing import Any

import pytest

from build_finance.crypto_replay.canonical import canonical_record_bytes, parse_canonical_record
from build_finance.crypto_replay.content_ids import seal_content_id as seal_replay_content_id
from build_finance.live_paper.algorithms import derive_algorithm_candidates
from build_finance.live_paper.content_ids import canonical_json_bytes as live_canonical_json_bytes
from build_finance.live_paper.content_ids import canonical_record_bytes as live_canonical_record_bytes
from build_finance.live_paper.content_ids import seal_content_id as seal_live_content_id
from build_finance.live_paper.features import derive_feature_snapshot
from build_finance.live_paper.grouping import EventGroup, group_committed_events
from build_finance.live_paper.profiles import PaperKernelProfiles
from build_finance.live_paper.registry import require_valid_contract as require_valid_live_contract
from build_finance.live_paper.run_input_builder import build_verified_run_inputs
from tests.live_paper.support.g2_vectors import G2Vector, build_g2_vector, parse_live_record

_CANDIDATE_KEYS = {
    "algorithm_candidate_id",
    "algorithm_id",
    "algorithm_version",
    "can_execute",
    "can_size",
    "candidate_action",
    "confidence_q18",
    "decision_group_key",
    "decision_sequence",
    "equal_time_group",
    "feature_snapshot_id",
    "input_content_ids",
    "normalization_receipt_id",
    "rationale_code",
    "schema",
}


def _profiles(vector: G2Vector) -> PaperKernelProfiles:
    return PaperKernelProfiles(**vector.profile_records)  # type: ignore[arg-type]


def _groups(vector: G2Vector) -> tuple[EventGroup, ...]:
    verified = build_verified_run_inputs(
        vector.run_receipt_record,
        vector.admitted_candidates,
        vector.source_receipt_records,
        vector.resolver,
        _profiles(vector),
    )
    return group_committed_events(verified)


def _documents(records: tuple[bytes, ...]) -> tuple[dict[str, Any], ...]:
    documents = tuple(parse_live_record(record) for record in records)
    for document in documents:
        require_valid_live_contract(document, expected_schema="trading.algorithm-candidate/v1")
        body = dict(document)
        supplied_id = body.pop("algorithm_candidate_id")
        assert supplied_id == hashlib.sha256(live_canonical_json_bytes(body)).hexdigest()
    return documents


def _candidate_body(
    *,
    normalization_receipt_id: str,
    snapshot_id: str,
    algorithm_id: str,
    action: str,
    rationale: str,
    confidence: str,
) -> dict[str, Any]:
    return {
        "schema": "trading.algorithm-candidate/v1",
        "normalization_receipt_id": normalization_receipt_id,
        "feature_snapshot_id": snapshot_id,
        "algorithm_id": algorithm_id,
        "algorithm_version": "1.0.0",
        "decision_group_key": "g2-group-2",
        "decision_sequence": "2",
        "equal_time_group": "2",
        "input_content_ids": [normalization_receipt_id, snapshot_id],
        "candidate_action": action,
        "rationale_code": rationale,
        "confidence_q18": confidence,
        "can_size": False,
        "can_execute": False,
    }


def test_positive_group_emits_three_exact_repeatable_evidence_candidates() -> None:
    vector = build_g2_vector()
    groups = _groups(vector)
    snapshot_record = derive_feature_snapshot(groups)
    snapshot = parse_canonical_record(snapshot_record)
    receipt = parse_live_record(groups[1].normalization_receipt_record)

    records = derive_algorithm_candidates(groups[1], snapshot_record)
    assert records == derive_algorithm_candidates(groups[1], snapshot_record)
    documents = _documents(records)
    normalization_id = str(receipt["normalization_receipt_id"])
    snapshot_id = str(snapshot["snapshot_id"])
    expected_bodies = (
        _candidate_body(
            normalization_receipt_id=normalization_id,
            snapshot_id=snapshot_id,
            algorithm_id="g2-momentum",
            action="OPEN_LONG",
            rationale="OPEN_IF_FLAT",
            confidence="999999000000",
        ),
        _candidate_body(
            normalization_receipt_id=normalization_id,
            snapshot_id=snapshot_id,
            algorithm_id="g2-mean-reversion-guard",
            action="HOLD",
            rationale="NO_ACTION",
            confidence="0",
        ),
        _candidate_body(
            normalization_receipt_id=normalization_id,
            snapshot_id=snapshot_id,
            algorithm_id="g2-liquidity-quality-veto",
            action="ABSTAIN",
            rationale="NO_ACTION",
            confidence="0",
        ),
    )
    assert len(documents) == 3
    for document, expected_body in zip(documents, expected_bodies, strict=True):
        assert set(document) == _CANDIDATE_KEYS
        assert {key: value for key, value in document.items() if key != "algorithm_candidate_id"} == expected_body


def test_warmup_abstains_and_one_low_liquidity_snapshot_vetoes() -> None:
    vector = build_g2_vector()
    first_group = _groups(vector)[0]
    warmup_record = derive_feature_snapshot((first_group,))

    warmup = _documents(derive_algorithm_candidates(first_group, warmup_record))
    assert tuple((row["candidate_action"], row["confidence_q18"]) for row in warmup) == (
        ("ABSTAIN", "0"),
        ("ABSTAIN", "0"),
        ("ABSTAIN", "0"),
    )

    low_liquidity = copy.deepcopy(parse_canonical_record(warmup_record))
    low_liquidity.pop("snapshot_id")
    features = low_liquidity["features"]
    assert isinstance(features, dict)
    features["liquidity_quote_atoms"] = "999999"
    low_liquidity_record = canonical_record_bytes(seal_replay_content_id(low_liquidity))
    vetoed = _documents(derive_algorithm_candidates(first_group, low_liquidity_record))

    assert tuple((row["candidate_action"], row["confidence_q18"]) for row in vetoed) == (
        ("ABSTAIN", "0"),
        ("ABSTAIN", "0"),
        ("HOLD", "1000000000000000000"),
    )


def test_stale_or_cross_group_evidence_bindings_are_rejected() -> None:
    vector = build_g2_vector()
    groups = _groups(vector)
    first_snapshot = derive_feature_snapshot(groups[:1])
    second_snapshot = derive_feature_snapshot(groups)

    stale_snapshot = copy.deepcopy(parse_canonical_record(second_snapshot))
    stale_snapshot["snapshot_id"] = "0" * 64
    with pytest.raises(ValueError, match="snapshot"):
        derive_algorithm_candidates(groups[1], canonical_record_bytes(stale_snapshot))

    miskeyed_receipt = parse_live_record(groups[1].normalization_receipt_record)
    miskeyed_receipt.pop("normalization_receipt_id")
    miskeyed_receipt["source_batch_id"] = "g2-decision-group-1"
    miskeyed_group = replace(
        groups[1],
        normalization_receipt_record=live_canonical_record_bytes(seal_live_content_id(miskeyed_receipt)),
    )
    with pytest.raises(ValueError, match="group"):
        derive_algorithm_candidates(miskeyed_group, second_snapshot)

    with pytest.raises(ValueError, match="group"):
        derive_algorithm_candidates(groups[1], first_snapshot)
