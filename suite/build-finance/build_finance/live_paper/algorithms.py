"""Fixed, non-authoritative algorithm evidence for the offline G2 paper kernel."""

from __future__ import annotations

from collections.abc import Mapping
from typing import Any, cast

from build_finance.crypto_replay.canonical import parse_canonical_record
from build_finance.crypto_replay.content_ids import verify_content_id as verify_replay_content_id
from build_finance.crypto_replay.schema_registry import require_valid_contract as require_valid_replay_contract
from build_finance.live_paper.content_ids import canonical_record_bytes, seal_content_id
from build_finance.live_paper.grouping import EventGroup
from build_finance.live_paper.registry import require_valid_contract as require_valid_live_contract

_Q18 = 1_000_000_000_000_000_000
_MEAN_REVERSION_THRESHOLD_Q18 = 20_000_000_000_000_000
_MIN_LIQUIDITY_QUOTE_ATOMS = 1_000_000
_MAX_STALE_AGE_NS = 1_000_000_000
_MAX_ROUTE_IMPACT_BPS = 100


def _parse_inputs(event_group: EventGroup, feature_snapshot_record: bytes) -> tuple[dict[str, Any], dict[str, Any]]:
    if not isinstance(event_group, EventGroup):
        raise ValueError("algorithm evidence requires an EventGroup")
    receipt = cast(dict[str, Any], parse_canonical_record(event_group.normalization_receipt_record))
    require_valid_live_contract(receipt, expected_schema="trading.normalization-receipt/v1")
    if receipt["status"] != "PASS" or receipt["reason_codes"] != []:
        raise ValueError("group normalization receipt must record an unqualified PASS")
    if receipt["source_batch_id"] != f"g2-decision-group-{event_group.group_sequence}":
        raise ValueError("group normalization receipt batch identity does not match the event group")
    if receipt["normalized_event_ids"] != list(event_group.event_ids):
        raise ValueError("group normalization receipt does not bind the exact event group")

    snapshot = cast(dict[str, Any], parse_canonical_record(bytes(feature_snapshot_record)))
    require_valid_replay_contract(snapshot, expected_schema="trading.feature-snapshot/v1")
    if not verify_replay_content_id(snapshot):
        raise ValueError("feature snapshot ID does not match its retained record")
    if snapshot["decision_sequence"] != event_group.group_sequence:
        raise ValueError("feature snapshot decision sequence does not match the event group")
    if snapshot["equal_time_group"] != event_group.group_sequence:
        raise ValueError("feature snapshot equal-time sequence does not match the event group")
    if snapshot["as_of_event_id"] not in event_group.event_ids:
        raise ValueError("feature snapshot as-of event is not a member of the event group")
    if snapshot["feature_code_sha256"] != event_group.feature_code_sha256:
        raise ValueError("feature snapshot code identity does not match the event group")
    return receipt, snapshot


def _bounded_confidence(value: int) -> str:
    return str(min(abs(value), _Q18))


def _momentum(value: object) -> tuple[str, str, str]:
    if value is None:
        return "ABSTAIN", "NO_ACTION", "0"
    parsed = int(cast(str, value))
    if parsed > 0:
        return "OPEN_LONG", "OPEN_IF_FLAT", _bounded_confidence(parsed)
    if parsed < 0:
        return "CLOSE_LONG", "CLOSE_IF_LONG", _bounded_confidence(parsed)
    return "HOLD", "NO_ACTION", "0"


def _mean_reversion(value: object) -> tuple[str, str, str]:
    if value is None:
        return "ABSTAIN", "NO_ACTION", "0"
    parsed = int(cast(str, value))
    if parsed > _MEAN_REVERSION_THRESHOLD_Q18:
        return "CLOSE_LONG", "CLOSE_IF_LONG", _bounded_confidence(parsed)
    if parsed < -_MEAN_REVERSION_THRESHOLD_Q18:
        return "OPEN_LONG", "OPEN_IF_FLAT", _bounded_confidence(parsed)
    return "HOLD", "NO_ACTION", "0"


def _liquidity_quality_veto(features: Mapping[str, Any]) -> tuple[str, str, str]:
    liquidity = features["liquidity_quote_atoms"]
    stale_age = features["stale_age_ns"]
    route_impact = features["route_impact_bps"]
    veto = (
        liquidity is None
        or stale_age is None
        or route_impact is None
        or int(cast(str, liquidity)) < _MIN_LIQUIDITY_QUOTE_ATOMS
        or int(cast(str, stale_age)) > _MAX_STALE_AGE_NS
        or cast(int, route_impact) > _MAX_ROUTE_IMPACT_BPS
    )
    if veto:
        return "HOLD", "NO_ACTION", str(_Q18)
    return "ABSTAIN", "NO_ACTION", "0"


def _candidate(
    *,
    receipt_id: str,
    snapshot_id: str,
    group_sequence: str,
    algorithm_id: str,
    decision: tuple[str, str, str],
) -> bytes:
    action, rationale, confidence = decision
    document = seal_content_id(
        {
            "schema": "trading.algorithm-candidate/v1",
            "normalization_receipt_id": receipt_id,
            "feature_snapshot_id": snapshot_id,
            "algorithm_id": algorithm_id,
            "algorithm_version": "1.0.0",
            "decision_group_key": f"g2-group-{group_sequence}",
            "decision_sequence": group_sequence,
            "equal_time_group": group_sequence,
            "input_content_ids": [receipt_id, snapshot_id],
            "candidate_action": action,
            "rationale_code": rationale,
            "confidence_q18": confidence,
            "can_size": False,
            "can_execute": False,
        }
    )
    require_valid_live_contract(document, expected_schema="trading.algorithm-candidate/v1")
    return canonical_record_bytes(document)


def derive_algorithm_candidates(
    event_group: EventGroup,
    feature_snapshot_record: bytes,
) -> tuple[bytes, ...]:
    """Return three sealed non-authoritative ``trading.algorithm-candidate/v1`` records."""
    receipt, snapshot = _parse_inputs(event_group, feature_snapshot_record)
    features = snapshot["features"]
    if not isinstance(features, Mapping):
        raise ValueError("feature snapshot has no feature mapping")
    receipt_id = cast(str, receipt["normalization_receipt_id"])
    snapshot_id = cast(str, snapshot["snapshot_id"])
    group_sequence = event_group.group_sequence
    return (
        _candidate(
            receipt_id=receipt_id,
            snapshot_id=snapshot_id,
            group_sequence=group_sequence,
            algorithm_id="g2-momentum",
            decision=_momentum(features["return_1_q18"]),
        ),
        _candidate(
            receipt_id=receipt_id,
            snapshot_id=snapshot_id,
            group_sequence=group_sequence,
            algorithm_id="g2-mean-reversion-guard",
            decision=_mean_reversion(features["return_1_q18"]),
        ),
        _candidate(
            receipt_id=receipt_id,
            snapshot_id=snapshot_id,
            group_sequence=group_sequence,
            algorithm_id="g2-liquidity-quality-veto",
            decision=_liquidity_quality_veto(features),
        ),
    )
