"""Closed deterministic fusion for the model-disabled offline G2 kernel."""

from __future__ import annotations

from collections.abc import Sequence
from dataclasses import dataclass
from typing import Any, cast

from build_finance.crypto_replay.canonical import parse_canonical_record
from build_finance.crypto_replay.content_ids import verify_content_id as verify_replay_content_id
from build_finance.crypto_replay.schema_registry import require_valid_contract as require_valid_replay_contract
from build_finance.live_paper.algorithms import derive_algorithm_candidates
from build_finance.live_paper.content_ids import canonical_record_bytes, seal_content_id
from build_finance.live_paper.grouping import EventGroup
from build_finance.live_paper.model_validation import ValidatedModelEvidence
from build_finance.live_paper.registry import require_valid_contract as require_valid_live_contract

_ALGORITHM_IDS = (
    "g2-momentum",
    "g2-mean-reversion-guard",
    "g2-liquidity-quality-veto",
)
_DIRECTIONAL_ACTIONS = ("OPEN_LONG", "CLOSE_LONG")


@dataclass(frozen=True, slots=True)
class FusionResult:
    """Exact retained fusion and final decision-group manifest records."""

    fusion_decision_record: bytes
    decision_group_manifest_record: bytes


def _utf8_sorted(values: Sequence[str]) -> list[str]:
    return sorted(values, key=lambda value: value.encode("utf-8"))


def _verified_group_receipt(event_group: EventGroup) -> dict[str, Any]:
    if not isinstance(event_group, EventGroup):
        raise ValueError("fusion requires an EventGroup")
    receipt = cast(dict[str, Any], parse_canonical_record(event_group.normalization_receipt_record))
    require_valid_live_contract(receipt, expected_schema="trading.normalization-receipt/v1")
    if receipt["status"] != "PASS" or receipt["reason_codes"] != []:
        raise ValueError("fusion requires an unqualified PASS normalization receipt")
    if receipt["source_batch_id"] != f"g2-decision-group-{event_group.group_sequence}":
        raise ValueError("normalization receipt batch does not match the event group")
    if receipt["normalized_event_ids"] != list(event_group.event_ids):
        raise ValueError("normalization receipt does not bind the exact event group")
    return receipt


def _verified_snapshot(event_group: EventGroup, record: bytes) -> dict[str, Any]:
    snapshot = cast(dict[str, Any], parse_canonical_record(bytes(record)))
    require_valid_replay_contract(snapshot, expected_schema="trading.feature-snapshot/v1")
    if not verify_replay_content_id(snapshot):
        raise ValueError("feature snapshot ID does not match its retained record")
    if (
        snapshot["decision_sequence"] != event_group.group_sequence
        or snapshot["equal_time_group"] != event_group.group_sequence
        or snapshot["as_of_event_id"] not in event_group.event_ids
        or snapshot["feature_code_sha256"] != event_group.feature_code_sha256
    ):
        raise ValueError("feature snapshot does not bind the exact event group")
    return snapshot


def _verified_candidates(
    records: Sequence[bytes],
    *,
    event_group: EventGroup,
    feature_snapshot_record: bytes,
    receipt_id: str,
    snapshot_id: str,
) -> dict[str, dict[str, Any]]:
    if len(records) != len(_ALGORITHM_IDS):
        raise ValueError("fusion requires exactly three G2 algorithm candidates")
    expected_records = {
        cast(str, parse_canonical_record(record)["algorithm_id"]): record
        for record in derive_algorithm_candidates(event_group, feature_snapshot_record)
    }
    by_algorithm: dict[str, dict[str, Any]] = {}
    expected_group_key = f"g2-group-{event_group.group_sequence}"
    expected_inputs = [receipt_id, snapshot_id]
    for record in records:
        if not isinstance(record, bytes):
            raise ValueError("algorithm candidate records must be exact bytes")
        candidate = cast(dict[str, Any], parse_canonical_record(record))
        require_valid_live_contract(candidate, expected_schema="trading.algorithm-candidate/v1")
        algorithm_id = cast(str, candidate["algorithm_id"])
        if algorithm_id not in _ALGORITHM_IDS or algorithm_id in by_algorithm:
            raise ValueError("algorithm candidate identities must be exact and unique")
        if candidate["algorithm_version"] != "1.0.0":
            raise ValueError("algorithm candidate version is not the fixed G2 version")
        if expected_records.get(algorithm_id) != record:
            raise ValueError("algorithm candidate does not match fixed G2 derivation")
        if (
            candidate["normalization_receipt_id"] != receipt_id
            or candidate["feature_snapshot_id"] != snapshot_id
            or candidate["decision_group_key"] != expected_group_key
            or candidate["decision_sequence"] != event_group.group_sequence
            or candidate["equal_time_group"] != event_group.group_sequence
            or candidate["input_content_ids"] != expected_inputs
        ):
            raise ValueError("algorithm candidate does not bind the exact decision evidence")
        by_algorithm[algorithm_id] = candidate
    if set(by_algorithm) != set(_ALGORITHM_IDS):
        raise ValueError("fusion requires the complete fixed G2 algorithm set")
    return by_algorithm


def _verify_model_evidence(model_evidence: ValidatedModelEvidence, snapshot_id: str) -> None:
    if type(model_evidence) is not ValidatedModelEvidence:
        raise ValueError("fusion requires exact validated model evidence")
    if (
        model_evidence.accepted_signal_record is not None
        or model_evidence.feature_snapshot_id != snapshot_id
        or model_evidence.disposition != "ABSTAIN"
        or model_evidence.reason_code != "MODEL_DISABLED"
    ):
        raise ValueError("model evidence must be the exact feature-bound disabled ABSTAIN")


def _select_candidate(candidates: dict[str, dict[str, Any]]) -> dict[str, Any]:
    momentum = candidates["g2-momentum"]
    mean_reversion = candidates["g2-mean-reversion-guard"]
    veto = candidates["g2-liquidity-quality-veto"]
    if veto["candidate_action"] == "HOLD":
        return veto
    if (
        momentum["candidate_action"] in _DIRECTIONAL_ACTIONS
        and mean_reversion["candidate_action"] in _DIRECTIONAL_ACTIONS
        and momentum["candidate_action"] != mean_reversion["candidate_action"]
    ):
        return veto
    return momentum


def fuse_signal_evidence(
    event_group: EventGroup,
    feature_snapshot_record: bytes,
    algorithm_candidate_records: Sequence[bytes],
    model_evidence: ValidatedModelEvidence,
) -> FusionResult:
    """Fuse one complete G2 evidence group without sizing or execution authority."""
    receipt = _verified_group_receipt(event_group)
    snapshot = _verified_snapshot(event_group, feature_snapshot_record)
    receipt_id = cast(str, receipt["normalization_receipt_id"])
    snapshot_id = cast(str, snapshot["snapshot_id"])
    candidates = _verified_candidates(
        algorithm_candidate_records,
        event_group=event_group,
        feature_snapshot_record=feature_snapshot_record,
        receipt_id=receipt_id,
        snapshot_id=snapshot_id,
    )
    _verify_model_evidence(model_evidence, snapshot_id)

    selected = _select_candidate(candidates)
    candidate_ids = _utf8_sorted(
        [cast(str, candidate["algorithm_candidate_id"]) for candidate in candidates.values()]
    )
    fusion = seal_content_id(
        {
            "schema": "trading.fusion-decision/v1",
            "normalization_receipt_id": receipt_id,
            "algorithm_candidate_id": selected["algorithm_candidate_id"],
            "model_signal_id": None,
            "decision_sequence": event_group.group_sequence,
            "equal_time_group": event_group.group_sequence,
            "input_content_ids": _utf8_sorted([receipt_id, snapshot_id, *candidate_ids]),
            "model_mode": "DISABLED_ABSTAIN",
            "model_action": "ABSTAIN",
            "model_score_q18": "0",
            "model_probability_abstain_q18": "1000000000000000000",
            "candidate_action": selected["candidate_action"],
            "fused_action": selected["candidate_action"],
            "fusion_policy": "MODEL_ABSTAINS_USE_ALGORITHM_CANDIDATE",
            "can_size": False,
            "can_execute": False,
        }
    )
    require_valid_live_contract(fusion, expected_schema="trading.fusion-decision/v1")
    fusion_id = cast(str, fusion["fusion_decision_id"])
    manifest = seal_content_id(
        {
            "schema": "trading.decision-group-manifest/v1",
            "decision_group_key": f"g2-group-{event_group.group_sequence}",
            "normalization_receipt_id": receipt_id,
            "algorithm_candidate_ids": candidate_ids,
            "fusion_decision_ids": [fusion_id],
            "member_content_ids": _utf8_sorted([receipt_id, *candidate_ids, fusion_id]),
            "member_count": "5",
            "sealed_by": "CONTENT_ID_REGISTRY_V1",
        }
    )
    require_valid_live_contract(manifest, expected_schema="trading.decision-group-manifest/v1")
    return FusionResult(
        fusion_decision_record=canonical_record_bytes(fusion),
        decision_group_manifest_record=canonical_record_bytes(manifest),
    )
