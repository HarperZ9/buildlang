"""Qualitative contract for closed deterministic G2 fusion."""

from __future__ import annotations

import copy
import hashlib
from collections.abc import Callable
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
from build_finance.live_paper.fusion import FusionResult, fuse_signal_evidence
from build_finance.live_paper.grouping import EventGroup, group_committed_events
from build_finance.live_paper.model_validation import ValidatedModelEvidence, validate_model_signal
from build_finance.live_paper.profiles import PaperKernelProfiles
from build_finance.live_paper.registry import require_valid_contract as require_valid_live_contract
from build_finance.live_paper.run_input_builder import build_verified_run_inputs
from tests.live_paper.support.g2_vectors import G2Vector, build_g2_vector, parse_live_record

_FUSION_KEYS = {
    "algorithm_candidate_id",
    "can_execute",
    "can_size",
    "candidate_action",
    "decision_sequence",
    "equal_time_group",
    "fused_action",
    "fusion_decision_id",
    "fusion_policy",
    "input_content_ids",
    "model_action",
    "model_mode",
    "model_probability_abstain_q18",
    "model_score_q18",
    "model_signal_id",
    "normalization_receipt_id",
    "schema",
}
_MANIFEST_KEYS = {
    "algorithm_candidate_ids",
    "decision_group_key",
    "decision_group_manifest_id",
    "fusion_decision_ids",
    "member_content_ids",
    "member_count",
    "normalization_receipt_id",
    "schema",
    "sealed_by",
}


def _profiles(vector: G2Vector) -> PaperKernelProfiles:
    return PaperKernelProfiles(**vector.profile_records)  # type: ignore[arg-type]


def _case() -> tuple[EventGroup, bytes, tuple[bytes, ...], ValidatedModelEvidence]:
    vector = build_g2_vector()
    verified = build_verified_run_inputs(
        vector.run_receipt_record,
        vector.admitted_candidates,
        vector.source_receipt_records,
        vector.resolver,
        _profiles(vector),
    )
    groups = group_committed_events(verified)
    snapshot_record = derive_feature_snapshot(groups)
    return (
        groups[-1],
        snapshot_record,
        derive_algorithm_candidates(groups[-1], snapshot_record),
        validate_model_signal(None, None, None, snapshot_record),
    )


def _mutated_snapshot(
    snapshot_record: bytes,
    mutation: Callable[[dict[str, Any]], None],
) -> bytes:
    snapshot = copy.deepcopy(parse_canonical_record(snapshot_record))
    snapshot.pop("snapshot_id")
    features = snapshot["features"]
    assert isinstance(features, dict)
    mutation(features)
    return canonical_record_bytes(seal_replay_content_id(snapshot))


def _assert_self_id(document: dict[str, Any], field: str) -> None:
    body = dict(document)
    supplied = body.pop(field)
    assert supplied == hashlib.sha256(live_canonical_json_bytes(body)).hexdigest()


def test_positive_vertical_is_closed_order_invariant_and_repeatable() -> None:
    group, snapshot_record, candidates, model_evidence = _case()

    result = fuse_signal_evidence(group, snapshot_record, candidates, model_evidence)
    assert result == fuse_signal_evidence(group, snapshot_record, candidates[::-1], model_evidence)
    assert isinstance(result, FusionResult)

    fusion = parse_live_record(result.fusion_decision_record)
    manifest = parse_live_record(result.decision_group_manifest_record)
    require_valid_live_contract(fusion, expected_schema="trading.fusion-decision/v1")
    require_valid_live_contract(manifest, expected_schema="trading.decision-group-manifest/v1")
    _assert_self_id(fusion, "fusion_decision_id")
    _assert_self_id(manifest, "decision_group_manifest_id")

    candidate_documents = tuple(parse_live_record(record) for record in candidates)
    candidate_ids = sorted(
        (str(candidate["algorithm_candidate_id"]) for candidate in candidate_documents),
        key=lambda value: value.encode("utf-8"),
    )
    momentum_id = next(
        str(candidate["algorithm_candidate_id"])
        for candidate in candidate_documents
        if candidate["algorithm_id"] == "g2-momentum"
    )
    receipt_id = str(parse_live_record(group.normalization_receipt_record)["normalization_receipt_id"])
    snapshot_id = str(parse_canonical_record(snapshot_record)["snapshot_id"])
    expected_inputs = sorted([receipt_id, snapshot_id, *candidate_ids], key=lambda value: value.encode("utf-8"))

    assert set(fusion) == _FUSION_KEYS
    assert fusion == {
        "schema": "trading.fusion-decision/v1",
        "fusion_decision_id": fusion["fusion_decision_id"],
        "normalization_receipt_id": receipt_id,
        "algorithm_candidate_id": momentum_id,
        "model_signal_id": None,
        "decision_sequence": "2",
        "equal_time_group": "2",
        "input_content_ids": expected_inputs,
        "model_mode": "DISABLED_ABSTAIN",
        "model_action": "ABSTAIN",
        "model_score_q18": "0",
        "model_probability_abstain_q18": "1000000000000000000",
        "candidate_action": "OPEN_LONG",
        "fused_action": "OPEN_LONG",
        "fusion_policy": "MODEL_ABSTAINS_USE_ALGORITHM_CANDIDATE",
        "can_size": False,
        "can_execute": False,
    }
    fusion_id = str(fusion["fusion_decision_id"])
    expected_members = sorted([receipt_id, *candidate_ids, fusion_id], key=lambda value: value.encode("utf-8"))
    assert set(manifest) == _MANIFEST_KEYS
    assert manifest == {
        "schema": "trading.decision-group-manifest/v1",
        "decision_group_manifest_id": manifest["decision_group_manifest_id"],
        "decision_group_key": "g2-group-2",
        "normalization_receipt_id": receipt_id,
        "algorithm_candidate_ids": candidate_ids,
        "fusion_decision_ids": [fusion_id],
        "member_content_ids": expected_members,
        "member_count": "5",
        "sealed_by": "CONTENT_ID_REGISTRY_V1",
    }


def test_veto_hold_and_directional_conflict_abstain() -> None:
    group, snapshot_record, _candidates, _model_evidence = _case()

    low_liquidity_record = _mutated_snapshot(
        snapshot_record,
        lambda features: features.__setitem__("liquidity_quote_atoms", "999999"),
    )
    low_liquidity_candidates = derive_algorithm_candidates(group, low_liquidity_record)
    low_liquidity_model = validate_model_signal(None, None, None, low_liquidity_record)
    veto_fusion = parse_live_record(
        fuse_signal_evidence(
            group,
            low_liquidity_record,
            low_liquidity_candidates,
            low_liquidity_model,
        ).fusion_decision_record
    )
    veto_candidate = next(
        parse_live_record(record)
        for record in low_liquidity_candidates
        if parse_live_record(record)["algorithm_id"] == "g2-liquidity-quality-veto"
    )
    assert (veto_fusion["algorithm_candidate_id"], veto_fusion["fused_action"]) == (
        veto_candidate["algorithm_candidate_id"],
        "HOLD",
    )

    conflict_record = _mutated_snapshot(
        snapshot_record,
        lambda features: features.__setitem__("return_1_q18", "20000000000000001"),
    )
    conflict_candidates = derive_algorithm_candidates(group, conflict_record)
    conflict_model = validate_model_signal(None, None, None, conflict_record)
    conflict_fusion = parse_live_record(
        fuse_signal_evidence(group, conflict_record, conflict_candidates, conflict_model).fusion_decision_record
    )
    neutral_veto = next(
        parse_live_record(record)
        for record in conflict_candidates
        if parse_live_record(record)["algorithm_id"] == "g2-liquidity-quality-veto"
    )
    assert (conflict_fusion["algorithm_candidate_id"], conflict_fusion["fused_action"]) == (
        neutral_veto["algorithm_candidate_id"],
        "ABSTAIN",
    )


def test_tampered_candidate_or_wrong_model_feature_binding_rejects() -> None:
    group, snapshot_record, candidates, model_evidence = _case()
    tampered_records = list(candidates)
    momentum_index = next(
        index
        for index, record in enumerate(tampered_records)
        if parse_live_record(record)["algorithm_id"] == "g2-momentum"
    )
    tampered = parse_live_record(tampered_records[momentum_index])
    tampered.pop("algorithm_candidate_id")
    tampered["candidate_action"] = "CLOSE_LONG"
    tampered["rationale_code"] = "CLOSE_IF_LONG"
    tampered_records[momentum_index] = live_canonical_record_bytes(seal_live_content_id(tampered))

    with pytest.raises(ValueError, match="candidate|content"):
        fuse_signal_evidence(
            group,
            snapshot_record,
            tampered_records,
            model_evidence,
        )
    with pytest.raises(ValueError, match="model|snapshot"):
        fuse_signal_evidence(
            group,
            snapshot_record,
            candidates,
            replace(model_evidence, feature_snapshot_id="f" * 64),
        )
