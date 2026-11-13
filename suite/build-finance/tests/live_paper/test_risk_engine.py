"""Qualitative contract for deterministic G2 risk authority and paper intent."""

from __future__ import annotations

import copy
import hashlib
from dataclasses import replace
from typing import Any

import pytest

from build_finance.crypto_replay.canonical import (
    canonical_json_bytes,
    canonical_record_bytes,
    parse_canonical_record,
)
from build_finance.crypto_replay.content_ids import seal_content_id as seal_replay_content_id
from build_finance.crypto_replay.content_ids import verify_content_id as verify_replay_content_id
from build_finance.crypto_replay.schema_registry import require_valid_contract as require_valid_replay_contract
from build_finance.live_paper.algorithms import derive_algorithm_candidates
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

_MAX_U64 = 18_446_744_073_709_551_615


def _risk_api() -> tuple[type[Any], Any]:
    from build_finance.live_paper.risk import RiskEvaluation, evaluate_risk

    return RiskEvaluation, evaluate_risk


def _profiles(vector: G2Vector) -> PaperKernelProfiles:
    return PaperKernelProfiles(**vector.profile_records)  # type: ignore[arg-type]


def _case() -> tuple[G2Vector, EventGroup, bytes, tuple[bytes, ...], ValidatedModelEvidence, FusionResult, bytes]:
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
    candidates = derive_algorithm_candidates(groups[-1], snapshot_record)
    model_evidence = validate_model_signal(None, None, None, snapshot_record)
    fusion_result = fuse_signal_evidence(groups[-1], snapshot_record, candidates, model_evidence)
    config_record = canonical_record_bytes(copy.deepcopy(vector.run_input_bundle.replay_risk_config))
    return vector, groups[-1], snapshot_record, candidates, model_evidence, fusion_result, config_record


def _risk_config_record(vector: G2Vector, **changes: Any) -> bytes:
    document = copy.deepcopy(vector.run_input_bundle.replay_risk_config)
    document.update(changes)
    document.pop("config_sha256", None)
    sealed = seal_replay_content_id(document)
    require_valid_replay_contract(sealed, expected_schema="trading.replay-risk-config/v1")
    return canonical_record_bytes(sealed)


def _feature_snapshot_record(snapshot_record: bytes, **features: Any) -> bytes:
    document = copy.deepcopy(parse_canonical_record(snapshot_record))
    document.pop("snapshot_id")
    feature_map = document["features"]
    assert isinstance(feature_map, dict)
    feature_map.update(features)
    sealed = seal_replay_content_id(document)
    require_valid_replay_contract(sealed, expected_schema="trading.feature-snapshot/v1")
    return canonical_record_bytes(sealed)


def _causal_digest(label: str) -> str:
    return hashlib.sha256(f"G2 RISK TEST STATE:{label}".encode("ascii")).hexdigest()


def _seal_portfolio_state(document: dict[str, Any]) -> bytes:
    sealed = seal_replay_content_id(document)
    require_valid_replay_contract(sealed, expected_schema="trading.portfolio-state/v1")
    return canonical_record_bytes(sealed)


def _flat_portfolio_state_record(
    vector: G2Vector,
    *,
    config_sha256: str | None = None,
    quote_available: str = "1000000",
    quote_reserved: str = "0",
    realized_pnl: str = "0",
    kill_reason_codes: tuple[str, ...] = (),
    open_intent_ids: tuple[str, ...] = (),
) -> bytes:
    quote_total = str(int(quote_available) + int(quote_reserved))
    return _seal_portfolio_state(
        {
            "schema": "trading.portfolio-state/v1",
            "state_sequence": "0",
            "as_of_ingest_sequence": "1",
            "equal_time_group": "1",
            "replay_clock_ns": "0",
            "previous_portfolio_state_id": None,
            "causation_schema": "RUN_INITIALIZATION",
            "causation_id": _causal_digest(f"flat:{quote_available}:{realized_pnl}:{','.join(kill_reason_codes)}"),
            "fixture_manifest_sha256": vector.run_input_bundle.fixture_manifest["fixture_manifest_sha256"],
            "config_admission_receipt_id": vector.run_input_bundle.config_admission_receipt[
                "config_admission_receipt_id"
            ],
            "validated_config_sha256": config_sha256 or vector.run_input_bundle.replay_risk_config["config_sha256"],
            "quote_mint": "synthetic-quote",
            "quote_decimals": 6,
            "balances": [
                {
                    "mint": "synthetic-quote",
                    "decimals": 6,
                    "available_atoms": quote_available,
                    "reserved_atoms": quote_reserved,
                    "total_atoms": quote_total,
                }
            ],
            "positions": [],
            "summary": {
                "realized_pnl_quote_atoms": realized_pnl,
                "unrealized_pnl_quote_atoms": "0",
                "cumulative_fees_quote_atoms": "0",
                "session_pnl_quote_atoms": realized_pnl,
                "equity_quote_atoms": quote_total,
                "peak_equity_quote_atoms": quote_total,
                "drawdown_bps": 0,
            },
            "kill_latched": bool(kill_reason_codes),
            "kill_reason_codes": list(kill_reason_codes),
            "open_intent_ids": sorted(open_intent_ids, key=lambda value: value.encode("utf-8")),
        }
    )


def _long_portfolio_state_record(
    vector: G2Vector,
    *,
    quantity_base_atoms: str,
    market_value_quote_atoms: str,
    cost_basis_quote_atoms: str,
    mark_price_q18: str,
    stop_price_q18: str,
    take_price_q18: str,
    quote_available: str = "900000",
    kill_reason_codes: tuple[str, ...] = (),
) -> bytes:
    unrealized = str(int(market_value_quote_atoms) - int(cost_basis_quote_atoms))
    equity = str(int(quote_available) + int(market_value_quote_atoms))
    return _seal_portfolio_state(
        {
            "schema": "trading.portfolio-state/v1",
            "state_sequence": "0",
            "as_of_ingest_sequence": "1",
            "equal_time_group": "1",
            "replay_clock_ns": "0",
            "previous_portfolio_state_id": None,
            "causation_schema": "RUN_INITIALIZATION",
            "causation_id": _causal_digest(f"long:{quantity_base_atoms}:{','.join(kill_reason_codes)}"),
            "fixture_manifest_sha256": vector.run_input_bundle.fixture_manifest["fixture_manifest_sha256"],
            "config_admission_receipt_id": vector.run_input_bundle.config_admission_receipt[
                "config_admission_receipt_id"
            ],
            "validated_config_sha256": vector.run_input_bundle.replay_risk_config["config_sha256"],
            "quote_mint": "synthetic-quote",
            "quote_decimals": 6,
            "balances": [
                {
                    "mint": "synthetic-base",
                    "decimals": 9,
                    "available_atoms": quantity_base_atoms,
                    "reserved_atoms": "0",
                    "total_atoms": quantity_base_atoms,
                },
                {
                    "mint": "synthetic-quote",
                    "decimals": 6,
                    "available_atoms": quote_available,
                    "reserved_atoms": "0",
                    "total_atoms": quote_available,
                },
            ],
            "positions": [
                {
                    "market_id": "synthetic-base/synthetic-quote:jupiter",
                    "base_mint": "synthetic-base",
                    "base_decimals": 9,
                    "quantity_base_atoms": quantity_base_atoms,
                    "reserved_base_atoms": "0",
                    "cost_basis_quote_atoms": cost_basis_quote_atoms,
                    "mark_price_q18": mark_price_q18,
                    "market_value_quote_atoms": market_value_quote_atoms,
                    "unrealized_pnl_quote_atoms": unrealized,
                    "stop_price_q18": stop_price_q18,
                    "take_price_q18": take_price_q18,
                }
            ],
            "summary": {
                "realized_pnl_quote_atoms": "0",
                "unrealized_pnl_quote_atoms": unrealized,
                "cumulative_fees_quote_atoms": "0",
                "session_pnl_quote_atoms": unrealized,
                "equity_quote_atoms": equity,
                "peak_equity_quote_atoms": equity,
                "drawdown_bps": 0,
            },
            "kill_latched": bool(kill_reason_codes),
            "kill_reason_codes": list(kill_reason_codes),
            "open_intent_ids": [],
        }
    )


def _documents(evaluation: Any) -> tuple[dict[str, Any], dict[str, Any] | None]:
    decision = parse_canonical_record(evaluation.risk_decision_record)
    require_valid_replay_contract(decision, expected_schema="trading.risk-decision/v1")
    assert verify_replay_content_id(decision)
    if evaluation.simulated_order_intent_record is None:
        return decision, None
    intent = parse_canonical_record(evaluation.simulated_order_intent_record)
    require_valid_replay_contract(intent, expected_schema="trading.simulated-order-intent/v1")
    assert verify_replay_content_id(intent)
    return decision, intent


def _reservation_id(
    *,
    fusion_decision_id: str,
    feature_snapshot_id: str,
    portfolio_state_id: str,
    config_sha256: str,
    action: str,
    approved_base_atoms: str,
    approved_notional_quote_atoms: str,
    reserved_quote_atoms: str,
    reserved_base_atoms: str,
) -> str:
    return hashlib.sha256(
        canonical_json_bytes(
            {
                "domain": "build-finance.g2.risk.reservation/v1",
                "accepted_fusion_decision_id": fusion_decision_id,
                "feature_snapshot_id": feature_snapshot_id,
                "portfolio_state_id": portfolio_state_id,
                "config_sha256": config_sha256,
                "decision_sequence": "2",
                "market_id": "synthetic-base/synthetic-quote:jupiter",
                "effective_action": action,
                "approved_base_atoms": approved_base_atoms,
                "approved_notional_quote_atoms": approved_notional_quote_atoms,
                "reserved_quote_atoms": reserved_quote_atoms,
                "reserved_base_atoms": reserved_base_atoms,
            }
        )
    ).hexdigest()


def test_flat_to_long_entry_sizes_gates_seals_identity_and_rejects_substituted_fusion() -> None:
    _RiskEvaluation, evaluate_risk = _risk_api()
    vector, group, snapshot_record, candidates, model_evidence, fusion_result, _config_record = _case()
    config_record = _risk_config_record(vector, session_end_replay_clock_ns="1000000001")
    config = parse_canonical_record(config_record)
    snapshot = parse_canonical_record(snapshot_record)
    fusion = parse_live_record(fusion_result.fusion_decision_record)
    portfolio_state_record = _flat_portfolio_state_record(vector, config_sha256=str(config["config_sha256"]))
    portfolio = parse_canonical_record(portfolio_state_record)

    evaluation = evaluate_risk(
        group,
        snapshot_record,
        candidates,
        model_evidence,
        fusion_result,
        portfolio_state_record,
        config_record,
    )
    repeated = evaluate_risk(
        group,
        snapshot_record,
        candidates,
        model_evidence,
        fusion_result,
        portfolio_state_record,
        config_record,
    )
    decision, intent = _documents(evaluation)
    assert evaluation == repeated
    assert evaluation.fusion_decision_id == fusion["fusion_decision_id"]
    assert intent is not None
    expected_reservation = _reservation_id(
        fusion_decision_id=str(fusion["fusion_decision_id"]),
        feature_snapshot_id=str(snapshot["snapshot_id"]),
        portfolio_state_id=str(portfolio["portfolio_state_id"]),
        config_sha256=str(config["config_sha256"]),
        action="ENTER_LONG",
        approved_base_atoms="99999800",
        approved_notional_quote_atoms="100000",
        reserved_quote_atoms="101000",
        reserved_base_atoms="0",
    )

    assert decision == {
        "schema": "trading.risk-decision/v1",
        "risk_decision_id": decision["risk_decision_id"],
        "decision_sequence": "2",
        "replay_clock_ns": "1000000000",
        "feature_snapshot_id": snapshot["snapshot_id"],
        "config_admission_receipt_id": portfolio["config_admission_receipt_id"],
        "validated_config_sha256": config["config_sha256"],
        "portfolio_state_before_id": portfolio["portfolio_state_id"],
        "model_signal_status": "ABSENT",
        "model_signal_id": None,
        "model_validation_receipt_id": None,
        "baseline_id": "MOMENTUM_V1",
        "baseline_action": "ENTER_LONG",
        "fused_action": "ENTER_LONG",
        "effective_action": "ENTER_LONG",
        "market_id": "synthetic-base/synthetic-quote:jupiter",
        "verdict": "APPROVE",
        "reason_codes": [],
        "reference_price_q18": "1000002000000000000",
        "requested_base_atoms": "99999800",
        "approved_base_atoms": "99999800",
        "requested_notional_quote_atoms": "100000",
        "approved_notional_quote_atoms": "100000",
        "measures": {
            "participation_bps": 100,
            "impact_bps": 25,
            "concentration_bps": 1000,
            "drawdown_bps": 0,
            "projected_market_value_quote_atoms": "100000",
            "projected_equity_quote_atoms": "1000000",
            "session_pnl_quote_atoms": "0",
            "stale_age_ns": "0",
        },
        "stop_price_q18": "950002000000000000",
        "take_price_q18": "1100003000000000000",
        "reservation_id": expected_reservation,
        "reserved_quote_atoms": "101000",
        "reserved_base_atoms": "0",
    }
    assert intent == {
        "schema": "trading.simulated-order-intent/v1",
        "intent_id": intent["intent_id"],
        "intent_sequence": "2",
        "decision_sequence": "2",
        "risk_decision_id": decision["risk_decision_id"],
        "config_admission_receipt_id": portfolio["config_admission_receipt_id"],
        "validated_config_sha256": config["config_sha256"],
        "portfolio_state_before_id": portfolio["portfolio_state_id"],
        "reservation_id": expected_reservation,
        "market_id": "synthetic-base/synthetic-quote:jupiter",
        "base_mint": "synthetic-base",
        "quote_mint": "synthetic-quote",
        "base_decimals": 9,
        "quote_decimals": 6,
        "action": "OPEN_LONG",
        "quantity_base_atoms": "99999800",
        "reference_price_q18": "1000002000000000000",
        "stop_price_q18": "950002000000000000",
        "take_price_q18": "1100003000000000000",
        "max_participation_bps": 500,
        "max_impact_bps": 50,
        "reserved_quote_atoms": "101000",
        "reserved_base_atoms": "0",
        "created_replay_clock_ns": "1000000000",
        "decision_ingest_sequence": "2",
        "decision_equal_time_group": "2",
        "fill_policy": "STRICT_NEXT_EVENT",
        "time_in_force": "ONE_EVENT_GROUP",
    }

    substitute_manifest = parse_live_record(fusion_result.decision_group_manifest_record)
    substitute_manifest.pop("decision_group_manifest_id")
    substitute_manifest["decision_group_key"] = "g2-group-2-self-addressed-substitute"
    substitute_result = replace(
        fusion_result,
        decision_group_manifest_record=live_canonical_record_bytes(seal_live_content_id(substitute_manifest)),
    )
    require_valid_live_contract(
        parse_live_record(substitute_result.decision_group_manifest_record),
        expected_schema="trading.decision-group-manifest/v1",
    )
    with pytest.raises(ValueError, match="fusion"):
        evaluate_risk(
            group,
            snapshot_record,
            candidates,
            model_evidence,
            substitute_result,
            portfolio_state_record,
            config_record,
        )


def test_overlapping_kill_session_stop_and_take_exit_reserves_full_base_with_precedence() -> None:
    _RiskEvaluation, evaluate_risk = _risk_api()
    vector, group, snapshot_record, candidates, model_evidence, fusion_result, config_record = _case()
    snapshot = parse_canonical_record(snapshot_record)
    fusion = parse_live_record(fusion_result.fusion_decision_record)
    portfolio_state_record = _long_portfolio_state_record(
        vector,
        quantity_base_atoms="99999800",
        market_value_quote_atoms="100000",
        cost_basis_quote_atoms="100000",
        mark_price_q18="1000002000000000000",
        stop_price_q18="1000002000000000000",
        take_price_q18="1000002000000000000",
        kill_reason_codes=("RISK_ARITHMETIC_RANGE",),
    )
    portfolio = parse_canonical_record(portfolio_state_record)
    config = parse_canonical_record(config_record)

    evaluation = evaluate_risk(
        group,
        snapshot_record,
        candidates,
        model_evidence,
        fusion_result,
        portfolio_state_record,
        config_record,
    )
    decision, intent = _documents(evaluation)
    assert intent is not None
    expected_reservation = _reservation_id(
        fusion_decision_id=str(fusion["fusion_decision_id"]),
        feature_snapshot_id=str(snapshot["snapshot_id"]),
        portfolio_state_id=str(portfolio["portfolio_state_id"]),
        config_sha256=str(config["config_sha256"]),
        action="EXIT_LONG",
        approved_base_atoms="99999800",
        approved_notional_quote_atoms="100000",
        reserved_quote_atoms="0",
        reserved_base_atoms="99999800",
    )

    assert decision["baseline_action"] == "HOLD"
    assert decision["fused_action"] == "HOLD"
    assert decision["effective_action"] == "EXIT_LONG"
    assert decision["verdict"] == "APPROVE"
    assert decision["reason_codes"] == ["RISK_KILL_EXIT", "RISK_STOP_TRIGGERED", "RISK_TAKE_TRIGGERED"]
    assert "RISK_RUN_END_EXIT" not in decision["reason_codes"]
    assert decision["model_signal_status"] == "ABSENT"
    assert decision["model_signal_id"] is None
    assert decision["model_validation_receipt_id"] is None
    assert decision["requested_base_atoms"] == decision["approved_base_atoms"] == "99999800"
    assert decision["requested_notional_quote_atoms"] == decision["approved_notional_quote_atoms"] == "100000"
    assert decision["reserved_quote_atoms"] == "0"
    assert decision["reserved_base_atoms"] == "99999800"
    assert decision["reference_price_q18"] == "1000002000000000000"
    assert decision["stop_price_q18"] == "1000002000000000000"
    assert decision["take_price_q18"] == "1000002000000000000"
    assert decision["reservation_id"] == expected_reservation
    assert decision["measures"] == {
        "participation_bps": 0,
        "impact_bps": 25,
        "concentration_bps": 1000,
        "drawdown_bps": 0,
        "projected_market_value_quote_atoms": "100000",
        "projected_equity_quote_atoms": "1000000",
        "session_pnl_quote_atoms": "0",
        "stale_age_ns": "0",
    }
    assert intent["action"] == "CLOSE_LONG"
    assert intent["quantity_base_atoms"] == "99999800"
    assert intent["reserved_quote_atoms"] == "0"
    assert intent["reserved_base_atoms"] == "99999800"
    assert intent["reservation_id"] == expected_reservation
    assert intent["intent_sequence"] == intent["decision_sequence"] == "2"


def test_stale_or_latched_flat_entries_emit_no_intent_and_overflow_fails_closed() -> None:
    _RiskEvaluation, evaluate_risk = _risk_api()
    vector, group, snapshot_record, candidates, model_evidence, fusion_result, config_record = _case()
    fusion = parse_live_record(fusion_result.fusion_decision_record)
    boundary_state_record = _flat_portfolio_state_record(vector)
    boundary_evaluation = evaluate_risk(
        group,
        snapshot_record,
        candidates,
        model_evidence,
        fusion_result,
        boundary_state_record,
        config_record,
    )
    boundary_decision, boundary_intent = _documents(boundary_evaluation)
    assert boundary_evaluation.fusion_decision_id == fusion["fusion_decision_id"]
    assert boundary_intent is None
    assert boundary_decision["verdict"] == "REJECT"
    assert boundary_decision["effective_action"] == "HOLD"
    assert boundary_decision["reason_codes"] == ["RISK_SESSION_CLOSED"]

    stale_config_record = _risk_config_record(
        vector,
        stale_after_ns="0",
        session_end_replay_clock_ns="1000000001",
    )
    stale_config = parse_canonical_record(stale_config_record)
    flat_state_record = _flat_portfolio_state_record(vector, config_sha256=str(stale_config["config_sha256"]))

    stale_evaluation = evaluate_risk(
        group,
        snapshot_record,
        candidates,
        model_evidence,
        fusion_result,
        flat_state_record,
        stale_config_record,
    )
    stale_decision, stale_intent = _documents(stale_evaluation)
    assert stale_intent is None
    assert stale_decision["verdict"] == "REJECT"
    assert stale_decision["effective_action"] == "HOLD"
    assert stale_decision["reason_codes"] == ["RISK_EVENT_STALE"]
    assert stale_decision["approved_base_atoms"] == stale_decision["approved_notional_quote_atoms"] == "0"
    assert stale_decision["reservation_id"] is None
    assert stale_decision["measures"]["stale_age_ns"] == "0"

    loss_latched_state_record = _flat_portfolio_state_record(
        vector,
        config_sha256=str(stale_config["config_sha256"]),
        realized_pnl="-100000",
        kill_reason_codes=("RISK_SESSION_LOSS",),
    )
    loss_evaluation = evaluate_risk(
        group,
        snapshot_record,
        candidates,
        model_evidence,
        fusion_result,
        loss_latched_state_record,
        stale_config_record,
    )
    loss_decision, loss_intent = _documents(loss_evaluation)
    assert loss_intent is None
    assert loss_decision["verdict"] == "KILL"
    assert loss_decision["effective_action"] == "HOLD"
    assert loss_decision["reason_codes"] == ["RISK_SESSION_LOSS"]
    assert loss_decision["approved_base_atoms"] == loss_decision["approved_notional_quote_atoms"] == "0"
    assert loss_decision["reserved_quote_atoms"] == loss_decision["reserved_base_atoms"] == "0"
    assert loss_decision["reservation_id"] is None
    assert loss_decision["model_signal_status"] == "ABSENT"
    assert loss_decision["baseline_action"] == "HOLD"
    assert loss_decision["fused_action"] == "HOLD"
    assert loss_decision["reference_price_q18"] == "1000002000000000000"
    assert loss_decision["measures"] == {
        "participation_bps": 0,
        "impact_bps": 25,
        "concentration_bps": 0,
        "drawdown_bps": 0,
        "projected_market_value_quote_atoms": "0",
        "projected_equity_quote_atoms": "1000000",
        "session_pnl_quote_atoms": "-100000",
        "stale_age_ns": "0",
    }

    overflow_config_record = _risk_config_record(
        vector,
        target_entry_notional_quote_atoms=str(_MAX_U64),
        max_notional_quote_atoms=str(_MAX_U64),
        session_end_replay_clock_ns="1000000001",
    )
    overflow_config = parse_canonical_record(overflow_config_record)
    overflow_state_record = _flat_portfolio_state_record(
        vector,
        config_sha256=str(overflow_config["config_sha256"]),
        quote_available=str(_MAX_U64),
    )
    overflow_evaluation = evaluate_risk(
        group,
        snapshot_record,
        candidates,
        model_evidence,
        fusion_result,
        overflow_state_record,
        overflow_config_record,
    )
    overflow_decision, overflow_intent = _documents(overflow_evaluation)
    assert overflow_evaluation.fusion_decision_id == fusion["fusion_decision_id"]
    assert overflow_intent is None
    assert overflow_decision["verdict"] == "KILL"
    assert overflow_decision["effective_action"] == "HOLD"
    assert overflow_decision["reason_codes"] == ["RISK_ARITHMETIC_RANGE"]
    assert overflow_decision["reference_price_q18"] is None
    assert overflow_decision["reservation_id"] is None
    assert overflow_decision["requested_base_atoms"] == overflow_decision["approved_base_atoms"] == "0"
    assert overflow_decision["requested_notional_quote_atoms"] == overflow_decision["approved_notional_quote_atoms"] == "0"
    assert overflow_decision["measures"] == {
        "participation_bps": None,
        "impact_bps": None,
        "concentration_bps": None,
        "drawdown_bps": None,
        "projected_market_value_quote_atoms": None,
        "projected_equity_quote_atoms": None,
        "session_pnl_quote_atoms": None,
        "stale_age_ns": None,
    }

    zero_quantity_snapshot_record = _feature_snapshot_record(
        snapshot_record,
        mid_price_q18="200000000000000000000000000",
    )
    zero_quantity_config_record = _risk_config_record(vector, session_end_replay_clock_ns="1000000001")
    zero_quantity_config = parse_canonical_record(zero_quantity_config_record)
    zero_quantity_state_record = _flat_portfolio_state_record(
        vector,
        config_sha256=str(zero_quantity_config["config_sha256"]),
    )
    zero_quantity_candidates = derive_algorithm_candidates(group, zero_quantity_snapshot_record)
    zero_quantity_model = validate_model_signal(None, None, None, zero_quantity_snapshot_record)
    zero_quantity_fusion = fuse_signal_evidence(
        group,
        zero_quantity_snapshot_record,
        zero_quantity_candidates,
        zero_quantity_model,
    )
    zero_quantity_fusion_doc = parse_live_record(zero_quantity_fusion.fusion_decision_record)
    zero_quantity_evaluation = evaluate_risk(
        group,
        zero_quantity_snapshot_record,
        zero_quantity_candidates,
        zero_quantity_model,
        zero_quantity_fusion,
        zero_quantity_state_record,
        zero_quantity_config_record,
    )
    zero_quantity_decision, zero_quantity_intent = _documents(zero_quantity_evaluation)
    assert zero_quantity_evaluation.fusion_decision_id == zero_quantity_fusion_doc["fusion_decision_id"]
    assert zero_quantity_intent is None
    assert zero_quantity_decision["verdict"] == "REJECT"
    assert zero_quantity_decision["effective_action"] == "HOLD"
    assert zero_quantity_decision["reason_codes"] == ["RISK_MIN_NOTIONAL"]
    assert zero_quantity_decision["requested_base_atoms"] == "0"
    assert zero_quantity_decision["approved_base_atoms"] == "0"
    assert zero_quantity_decision["approved_notional_quote_atoms"] == "0"
