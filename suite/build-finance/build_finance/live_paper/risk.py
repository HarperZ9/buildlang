"""Deterministic risk authority for the offline G2 paper kernel."""

from __future__ import annotations

import hashlib
from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from typing import Any, NoReturn, cast

from build_finance.crypto_replay.canonical import (
    JsonValue,
    canonical_json_bytes,
    canonical_record_bytes,
    parse_canonical_record,
)
from build_finance.crypto_replay.content_ids import seal_content_id as seal_replay_content_id
from build_finance.crypto_replay.content_ids import verify_content_id as verify_replay_content_id
from build_finance.crypto_replay.schema_registry import require_valid_contract as require_valid_replay_contract
from build_finance.live_paper.content_ids import verify_content_id as verify_live_content_id
from build_finance.live_paper.fusion import FusionResult, fuse_signal_evidence
from build_finance.live_paper.grouping import EventGroup
from build_finance.live_paper.model_validation import ValidatedModelEvidence
from build_finance.live_paper.registry import require_valid_contract as require_valid_live_contract

_MAX_U64 = 18_446_744_073_709_551_615
_MIN_I128 = -170_141_183_460_469_231_731_687_303_715_884_105_728
_MAX_I128 = 170_141_183_460_469_231_731_687_303_715_884_105_727
_MAX_BPS_MEASURE = 2_147_483_647
_Q18 = 1_000_000_000_000_000_000


class _ArithmeticRange(ValueError):
    """Raised internally when bounded integer authority would overflow."""


@dataclass(frozen=True, slots=True)
class RiskEvaluation:
    fusion_decision_id: str
    risk_decision_record: bytes
    simulated_order_intent_record: bytes | None

    def __post_init__(self) -> None:
        object.__setattr__(self, "risk_decision_record", bytes(self.risk_decision_record))
        intent = None if self.simulated_order_intent_record is None else bytes(self.simulated_order_intent_record)
        object.__setattr__(self, "simulated_order_intent_record", intent)


def _fail(message: str) -> NoReturn:
    raise ValueError(message)


def _parse_replay_record(record: bytes, *, expected_schema: str) -> dict[str, Any]:
    document = cast(dict[str, Any], parse_canonical_record(bytes(record)))
    require_valid_replay_contract(document, expected_schema=expected_schema)
    if not verify_replay_content_id(document):
        _fail(f"{expected_schema} self-ID does not match its retained record")
    return document


def _parse_live_record(record: bytes, *, expected_schema: str) -> dict[str, Any]:
    document = cast(dict[str, Any], parse_canonical_record(bytes(record)))
    require_valid_live_contract(document, expected_schema=expected_schema)
    if not verify_live_content_id(document):
        _fail(f"{expected_schema} self-ID does not match its retained record")
    return document


def _u64_text(value: object, *, field: str, positive: bool = False) -> int:
    if not isinstance(value, str) or not value.isascii() or not value.isdigit() or str(int(value)) != value:
        _fail(f"{field} must be canonical unsigned numeric text")
    parsed = int(value)
    if parsed > _MAX_U64 or (positive and parsed == 0):
        _fail(f"{field} is outside its required unsigned range")
    return parsed


def _i128_text(value: object, *, field: str, positive: bool = False) -> int:
    if not isinstance(value, str):
        _fail(f"{field} must be canonical signed numeric text")
    if value == "0":
        parsed = 0
    elif value.startswith("-"):
        digits = value[1:]
        if not digits or digits.startswith("0") or not digits.isascii() or not digits.isdigit():
            _fail(f"{field} must be canonical signed numeric text")
        parsed = -int(digits)
    elif value.isascii() and value.isdigit() and not value.startswith("0"):
        parsed = int(value)
    else:
        _fail(f"{field} must be canonical signed numeric text")
    if parsed < _MIN_I128 or parsed > _MAX_I128 or (positive and parsed <= 0):
        _fail(f"{field} is outside its required signed range")
    return parsed


def _checked_i128(value: int) -> int:
    if value < _MIN_I128 or value > _MAX_I128:
        raise _ArithmeticRange("integer arithmetic exceeded the signed-i128 authority range")
    return value


def _checked_u64(value: int) -> int:
    if value < 0 or value > _MAX_U64:
        raise _ArithmeticRange("integer arithmetic exceeded the u64 authority range")
    return value


def _checked_bps(value: int) -> int:
    if value < 0 or value > _MAX_BPS_MEASURE:
        raise _ArithmeticRange("basis-point measure exceeded the frozen integer range")
    return value


def _checked_mul(*values: int) -> int:
    result = 1
    for value in values:
        result = _checked_i128(result * value)
    return result


def _ceil_div(numerator: int, denominator: int) -> int:
    if denominator <= 0:
        raise _ArithmeticRange("integer division denominator must be positive")
    if numerator < 0:
        raise _ArithmeticRange("ceiling division requires a non-negative numerator")
    return (numerator + denominator - 1) // denominator


def _ceil_to_tick(value: int, tick: int) -> int:
    return _checked_i128(_ceil_div(value, tick) * tick)


def _sorted_reasons(reasons: Sequence[str]) -> list[str]:
    precedence = {
        code: rank
        for rank, code in enumerate(
            (
                "RISK_CONFIG_MISSING",
                "RISK_CONFIG_INVALID",
                "RISK_SEQUENCE_INVALID",
                "RISK_STATE_UNRECONCILED",
                "RISK_KILL_LATCHED",
                "RISK_SESSION_CLOSED",
                "RISK_EVENT_STALE",
                "RISK_DECIMALS_MISMATCH",
                "RISK_ARITHMETIC_RANGE",
                "RISK_NONPOSITIVE_EQUITY",
                "RISK_SESSION_LOSS",
                "RISK_DRAWDOWN",
                "RISK_STOP_MISSING",
                "RISK_INSUFFICIENT_BALANCE",
                "RISK_RESERVATION_CONFLICT",
                "RISK_INTENT_PENDING",
                "RISK_MIN_NOTIONAL",
                "RISK_MAX_NOTIONAL",
                "RISK_PARTICIPATION",
                "RISK_IMPACT",
                "RISK_CONCENTRATION",
                "RISK_NO_ACTION",
                "RISK_RUN_END_EXIT",
                "RISK_KILL_EXIT",
                "RISK_STOP_TRIGGERED",
                "RISK_TAKE_TRIGGERED",
            )
        )
    }
    unique = tuple(dict.fromkeys(reasons))
    return sorted(unique, key=precedence.__getitem__)


def _action(fusion_action: object) -> str:
    if fusion_action == "OPEN_LONG":
        return "ENTER_LONG"
    if fusion_action == "CLOSE_LONG":
        return "EXIT_LONG"
    return "HOLD"


def _summary_measures(
    *,
    participation_bps: int | None,
    impact_bps: int | None,
    concentration_bps: int | None,
    projected_market_value_quote_atoms: int | None,
    projected_equity_quote_atoms: int | None,
    session_pnl_quote_atoms: int | None,
    drawdown_bps: int | None,
    stale_age_ns: int | None,
) -> dict[str, JsonValue]:
    return {
        "participation_bps": None if participation_bps is None else _checked_bps(participation_bps),
        "impact_bps": None if impact_bps is None else _checked_bps(impact_bps),
        "concentration_bps": None if concentration_bps is None else _checked_bps(concentration_bps),
        "drawdown_bps": None if drawdown_bps is None else _checked_bps(drawdown_bps),
        "projected_market_value_quote_atoms": None
        if projected_market_value_quote_atoms is None
        else str(_checked_u64(projected_market_value_quote_atoms)),
        "projected_equity_quote_atoms": None
        if projected_equity_quote_atoms is None
        else str(_checked_u64(projected_equity_quote_atoms)),
        "session_pnl_quote_atoms": None
        if session_pnl_quote_atoms is None
        else str(_checked_i128(session_pnl_quote_atoms)),
        "stale_age_ns": None if stale_age_ns is None else str(_checked_u64(stale_age_ns)),
    }


def _zero_measures() -> dict[str, JsonValue]:
    return _summary_measures(
        participation_bps=None,
        impact_bps=None,
        concentration_bps=None,
        projected_market_value_quote_atoms=None,
        projected_equity_quote_atoms=None,
        session_pnl_quote_atoms=None,
        drawdown_bps=None,
        stale_age_ns=None,
    )


def _balance_by_mint(portfolio: Mapping[str, Any]) -> dict[str, Mapping[str, Any]]:
    balances = portfolio["balances"]
    assert isinstance(balances, list)
    return {cast(str, row["mint"]): cast(Mapping[str, Any], row) for row in balances}


def _state_numbers(portfolio: Mapping[str, Any]) -> tuple[int, int, int]:
    summary = cast(Mapping[str, Any], portfolio["summary"])
    equity = _u64_text(summary["equity_quote_atoms"], field="summary.equity_quote_atoms")
    session_pnl = _i128_text(summary["session_pnl_quote_atoms"], field="summary.session_pnl_quote_atoms")
    drawdown = cast(int, summary["drawdown_bps"])
    return equity, session_pnl, drawdown


def _reference_inputs(snapshot: Mapping[str, Any]) -> tuple[int | None, int | None, int | None, int | None]:
    features = cast(Mapping[str, Any], snapshot["features"])
    price = None if features["mid_price_q18"] is None else _i128_text(features["mid_price_q18"], field="mid_price_q18")
    liquidity = (
        None
        if features["liquidity_quote_atoms"] is None
        else _u64_text(features["liquidity_quote_atoms"], field="liquidity_quote_atoms")
    )
    stale_age = None if features["stale_age_ns"] is None else _u64_text(features["stale_age_ns"], field="stale_age_ns")
    impact = None if features["route_impact_bps"] is None else cast(int, features["route_impact_bps"])
    return price, liquidity, stale_age, impact


def _quantity_for_entry(*, target_notional: int, price_q18: int, base_decimals: int, quote_decimals: int) -> int:
    numerator = _checked_mul(target_notional, _Q18, 10**base_decimals)
    denominator = _checked_mul(price_q18, 10**quote_decimals)
    return _checked_u64(numerator // denominator)


def _actual_notional(*, price_q18: int, quantity: int, base_decimals: int, quote_decimals: int) -> int:
    numerator = _checked_mul(price_q18, quantity, 10**quote_decimals)
    denominator = _checked_mul(_Q18, 10**base_decimals)
    return _checked_u64(_ceil_div(numerator, denominator))


def _entry_levels(*, price_q18: int, config: Mapping[str, Any]) -> tuple[int, int]:
    tick = _i128_text(config["price_tick_q18"], field="price_tick_q18", positive=True)
    stop_loss_bps = cast(int, config["stop_loss_bps"])
    take_profit_bps = cast(int, config["take_profit_bps"])
    raw_stop = _checked_mul(price_q18, 10_000 - stop_loss_bps) // 10_000
    raw_take = _ceil_div(_checked_mul(price_q18, 10_000 + take_profit_bps), 10_000)
    stop = _ceil_to_tick(raw_stop, tick)
    take = _ceil_to_tick(raw_take, tick)
    if not 0 < stop < price_q18 < take:
        raise _ArithmeticRange("protective levels are outside the approved entry order")
    return stop, take


def _reserved_quote(target_notional: int, *, max_impact_bps: int, max_fee_bps: int) -> int:
    impact_reserved = _ceil_div(_checked_mul(target_notional, 10_000 + max_impact_bps), 10_000)
    fee_reserved = _ceil_div(_checked_mul(target_notional, max_fee_bps), 10_000)
    return _checked_u64(impact_reserved + fee_reserved)


def _reservation_id(
    *,
    fusion_decision_id: str,
    feature_snapshot_id: str,
    portfolio_state_id: str,
    config_sha256: str,
    decision_sequence: str,
    market_id: str,
    effective_action: str,
    approved_base_atoms: str,
    approved_notional_quote_atoms: str,
    reserved_quote_atoms: str,
    reserved_base_atoms: str,
) -> str:
    payload = {
        "domain": "build-finance.g2.risk.reservation/v1",
        "accepted_fusion_decision_id": fusion_decision_id,
        "feature_snapshot_id": feature_snapshot_id,
        "portfolio_state_id": portfolio_state_id,
        "config_sha256": config_sha256,
        "decision_sequence": decision_sequence,
        "market_id": market_id,
        "effective_action": effective_action,
        "approved_base_atoms": approved_base_atoms,
        "approved_notional_quote_atoms": approved_notional_quote_atoms,
        "reserved_quote_atoms": reserved_quote_atoms,
        "reserved_base_atoms": reserved_base_atoms,
    }
    return hashlib.sha256(canonical_json_bytes(payload)).hexdigest()


def _risk_decision_record(
    *,
    snapshot: Mapping[str, Any],
    portfolio: Mapping[str, Any],
    config: Mapping[str, Any],
    baseline_action: str,
    fused_action: str,
    effective_action: str,
    verdict: str,
    reason_codes: Sequence[str],
    reference_price_q18: int | None,
    requested_base_atoms: int,
    approved_base_atoms: int,
    requested_notional_quote_atoms: int,
    approved_notional_quote_atoms: int,
    measures: Mapping[str, JsonValue],
    stop_price_q18: int | None,
    take_price_q18: int | None,
    reservation_id: str | None,
    reserved_quote_atoms: int,
    reserved_base_atoms: int,
) -> bytes:
    document = {
        "schema": "trading.risk-decision/v1",
        "decision_sequence": snapshot["decision_sequence"],
        "replay_clock_ns": snapshot["replay_clock_ns"],
        "feature_snapshot_id": snapshot["snapshot_id"],
        "config_admission_receipt_id": portfolio["config_admission_receipt_id"],
        "validated_config_sha256": config["config_sha256"],
        "portfolio_state_before_id": portfolio["portfolio_state_id"],
        "model_signal_status": "ABSENT",
        "model_signal_id": None,
        "model_validation_receipt_id": None,
        "baseline_id": config["baseline_id"],
        "baseline_action": baseline_action,
        "fused_action": fused_action,
        "effective_action": effective_action,
        "market_id": snapshot["market_id"],
        "verdict": verdict,
        "reason_codes": _sorted_reasons(reason_codes),
        "reference_price_q18": None if reference_price_q18 is None else str(_checked_i128(reference_price_q18)),
        "requested_base_atoms": str(_checked_u64(requested_base_atoms)),
        "approved_base_atoms": str(_checked_u64(approved_base_atoms)),
        "requested_notional_quote_atoms": str(_checked_u64(requested_notional_quote_atoms)),
        "approved_notional_quote_atoms": str(_checked_u64(approved_notional_quote_atoms)),
        "measures": dict(measures),
        "stop_price_q18": None if stop_price_q18 is None else str(_checked_i128(stop_price_q18)),
        "take_price_q18": None if take_price_q18 is None else str(_checked_i128(take_price_q18)),
        "reservation_id": reservation_id,
        "reserved_quote_atoms": str(_checked_u64(reserved_quote_atoms)),
        "reserved_base_atoms": str(_checked_u64(reserved_base_atoms)),
    }
    sealed = seal_replay_content_id(document)
    require_valid_replay_contract(sealed, expected_schema="trading.risk-decision/v1")
    return canonical_record_bytes(sealed)


def _intent_record(
    *,
    snapshot: Mapping[str, Any],
    portfolio: Mapping[str, Any],
    config: Mapping[str, Any],
    risk_decision: Mapping[str, Any],
    action: str,
    quantity_base_atoms: int,
    reference_price_q18: int,
    stop_price_q18: int,
    take_price_q18: int,
    reservation_id: str,
    reserved_quote_atoms: int,
    reserved_base_atoms: int,
) -> bytes:
    decision_sequence = cast(str, snapshot["decision_sequence"])
    document = {
        "schema": "trading.simulated-order-intent/v1",
        # G2 has no separate authoritative run-global intent counter yet; this is decision-group deterministic only.
        "intent_sequence": decision_sequence,
        "decision_sequence": decision_sequence,
        "risk_decision_id": risk_decision["risk_decision_id"],
        "config_admission_receipt_id": portfolio["config_admission_receipt_id"],
        "validated_config_sha256": config["config_sha256"],
        "portfolio_state_before_id": portfolio["portfolio_state_id"],
        "reservation_id": reservation_id,
        "market_id": snapshot["market_id"],
        "base_mint": snapshot["base_mint"],
        "quote_mint": snapshot["quote_mint"],
        "base_decimals": snapshot["base_decimals"],
        "quote_decimals": snapshot["quote_decimals"],
        "action": action,
        "quantity_base_atoms": str(_checked_u64(quantity_base_atoms)),
        "reference_price_q18": str(_checked_i128(reference_price_q18)),
        "stop_price_q18": str(_checked_i128(stop_price_q18)),
        "take_price_q18": str(_checked_i128(take_price_q18)),
        "max_participation_bps": config["max_participation_bps"],
        "max_impact_bps": config["max_impact_bps"],
        "reserved_quote_atoms": str(_checked_u64(reserved_quote_atoms)),
        "reserved_base_atoms": str(_checked_u64(reserved_base_atoms)),
        "created_replay_clock_ns": snapshot["replay_clock_ns"],
        "decision_ingest_sequence": snapshot["as_of_ingest_sequence"],
        "decision_equal_time_group": snapshot["equal_time_group"],
        "fill_policy": "STRICT_NEXT_EVENT",
        "time_in_force": "ONE_EVENT_GROUP",
    }
    sealed = seal_replay_content_id(document)
    require_valid_replay_contract(sealed, expected_schema="trading.simulated-order-intent/v1")
    return canonical_record_bytes(sealed)


def _zero_authority(
    *,
    fusion_decision_id: str,
    snapshot: Mapping[str, Any],
    portfolio: Mapping[str, Any],
    config: Mapping[str, Any],
    reason: str,
) -> RiskEvaluation:
    record = _risk_decision_record(
        snapshot=snapshot,
        portfolio=portfolio,
        config=config,
        baseline_action="HOLD",
        fused_action="HOLD",
        effective_action="HOLD",
        verdict="KILL",
        reason_codes=(reason,),
        reference_price_q18=None,
        requested_base_atoms=0,
        approved_base_atoms=0,
        requested_notional_quote_atoms=0,
        approved_notional_quote_atoms=0,
        measures=_zero_measures(),
        stop_price_q18=None,
        take_price_q18=None,
        reservation_id=None,
        reserved_quote_atoms=0,
        reserved_base_atoms=0,
    )
    return RiskEvaluation(
        fusion_decision_id=fusion_decision_id,
        risk_decision_record=record,
        simulated_order_intent_record=None,
    )


def _flat_kill_reasons(portfolio: Mapping[str, Any], *, session_loss: bool, drawdown: bool) -> tuple[str, ...]:
    kill_codes = tuple(cast(list[str], portfolio["kill_reason_codes"]))
    reasons: list[str] = []
    if "RISK_SESSION_LOSS" in kill_codes or session_loss:
        reasons.append("RISK_SESSION_LOSS")
    if "RISK_DRAWDOWN" in kill_codes or drawdown:
        reasons.append("RISK_DRAWDOWN")
    if reasons:
        if cast(list[Any], portfolio["open_intent_ids"]):
            reasons.append("RISK_INTENT_PENDING")
        return tuple(reasons)
    if portfolio["kill_latched"]:
        return ("RISK_KILL_LATCHED",)
    return ()


def _entry_evaluation(
    *,
    fusion_decision_id: str,
    mapped_action: str,
    snapshot: Mapping[str, Any],
    portfolio: Mapping[str, Any],
    config: Mapping[str, Any],
    price: int,
    liquidity: int,
    stale_age: int,
    impact: int,
    equity: int,
    session_pnl: int,
    drawdown: int,
) -> RiskEvaluation:
    try:
        target_notional = _u64_text(
            config["target_entry_notional_quote_atoms"], field="target_entry_notional_quote_atoms", positive=True
        )
        quantity = _quantity_for_entry(
            target_notional=target_notional,
            price_q18=price,
            base_decimals=cast(int, snapshot["base_decimals"]),
            quote_decimals=cast(int, snapshot["quote_decimals"]),
        )
        actual_notional = _actual_notional(
            price_q18=price,
            quantity=quantity,
            base_decimals=cast(int, snapshot["base_decimals"]),
            quote_decimals=cast(int, snapshot["quote_decimals"]),
        )
        zero_quantity_or_notional = quantity == 0 or actual_notional == 0
        participation = _checked_bps(_ceil_div(_checked_mul(target_notional, 10_000), liquidity))
        reserved_quote = _reserved_quote(
            target_notional,
            max_impact_bps=cast(int, config["max_impact_bps"]),
            max_fee_bps=cast(int, config["max_fee_bps"]),
        )
        concentration = _checked_bps(_ceil_div(_checked_mul(actual_notional, 10_000), equity))
        stop, take = _entry_levels(price_q18=price, config=config)
    except _ArithmeticRange:
        return _zero_authority(
            fusion_decision_id=fusion_decision_id,
            snapshot=snapshot,
            portfolio=portfolio,
            config=config,
            reason="RISK_ARITHMETIC_RANGE",
        )

    quote_balance = _balance_by_mint(portfolio).get(cast(str, snapshot["quote_mint"]))
    available_quote = 0 if quote_balance is None else _u64_text(quote_balance["available_atoms"], field="available_quote")
    reasons: list[str] = []
    if stale_age >= _u64_text(config["stale_after_ns"], field="stale_after_ns"):
        reasons.append("RISK_EVENT_STALE")
    if reserved_quote > available_quote:
        reasons.append("RISK_INSUFFICIENT_BALANCE")
    if target_notional < _u64_text(config["min_notional_quote_atoms"], field="min_notional_quote_atoms"):
        reasons.append("RISK_MIN_NOTIONAL")
    if zero_quantity_or_notional and "RISK_MIN_NOTIONAL" not in reasons:
        reasons.append("RISK_MIN_NOTIONAL")
    if target_notional > _u64_text(config["max_notional_quote_atoms"], field="max_notional_quote_atoms"):
        reasons.append("RISK_MAX_NOTIONAL")
    if participation > cast(int, config["max_participation_bps"]):
        reasons.append("RISK_PARTICIPATION")
    if impact > cast(int, config["max_impact_bps"]):
        reasons.append("RISK_IMPACT")
    if concentration > cast(int, config["max_concentration_bps"]):
        reasons.append("RISK_CONCENTRATION")

    measures = _summary_measures(
        participation_bps=participation,
        impact_bps=impact,
        concentration_bps=concentration,
        projected_market_value_quote_atoms=actual_notional,
        projected_equity_quote_atoms=equity,
        session_pnl_quote_atoms=session_pnl,
        drawdown_bps=drawdown,
        stale_age_ns=stale_age,
    )
    if reasons:
        record = _risk_decision_record(
            snapshot=snapshot,
            portfolio=portfolio,
            config=config,
            baseline_action=mapped_action,
            fused_action=mapped_action,
            effective_action="HOLD",
            verdict="REJECT",
            reason_codes=reasons,
            reference_price_q18=price,
            requested_base_atoms=quantity,
            approved_base_atoms=0,
            requested_notional_quote_atoms=target_notional,
            approved_notional_quote_atoms=0,
            measures=measures,
            stop_price_q18=stop,
            take_price_q18=take,
            reservation_id=None,
            reserved_quote_atoms=0,
            reserved_base_atoms=0,
        )
        return RiskEvaluation(fusion_decision_id=fusion_decision_id, risk_decision_record=record, simulated_order_intent_record=None)

    reservation = _reservation_id(
        fusion_decision_id=fusion_decision_id,
        feature_snapshot_id=cast(str, snapshot["snapshot_id"]),
        portfolio_state_id=cast(str, portfolio["portfolio_state_id"]),
        config_sha256=cast(str, config["config_sha256"]),
        decision_sequence=cast(str, snapshot["decision_sequence"]),
        market_id=cast(str, snapshot["market_id"]),
        effective_action="ENTER_LONG",
        approved_base_atoms=str(quantity),
        approved_notional_quote_atoms=str(target_notional),
        reserved_quote_atoms=str(reserved_quote),
        reserved_base_atoms="0",
    )
    decision_record = _risk_decision_record(
        snapshot=snapshot,
        portfolio=portfolio,
        config=config,
        baseline_action="ENTER_LONG",
        fused_action="ENTER_LONG",
        effective_action="ENTER_LONG",
        verdict="APPROVE",
        reason_codes=(),
        reference_price_q18=price,
        requested_base_atoms=quantity,
        approved_base_atoms=quantity,
        requested_notional_quote_atoms=target_notional,
        approved_notional_quote_atoms=target_notional,
        measures=measures,
        stop_price_q18=stop,
        take_price_q18=take,
        reservation_id=reservation,
        reserved_quote_atoms=reserved_quote,
        reserved_base_atoms=0,
    )
    decision = parse_canonical_record(decision_record)
    intent_record = _intent_record(
        snapshot=snapshot,
        portfolio=portfolio,
        config=config,
        risk_decision=decision,
        action="OPEN_LONG",
        quantity_base_atoms=quantity,
        reference_price_q18=price,
        stop_price_q18=stop,
        take_price_q18=take,
        reservation_id=reservation,
        reserved_quote_atoms=reserved_quote,
        reserved_base_atoms=0,
    )
    return RiskEvaluation(fusion_decision_id=fusion_decision_id, risk_decision_record=decision_record, simulated_order_intent_record=intent_record)


def _held_position(portfolio: Mapping[str, Any], snapshot: Mapping[str, Any]) -> Mapping[str, Any] | None:
    positions = cast(list[Mapping[str, Any]], portfolio["positions"])
    if not positions:
        return None
    if len(positions) != 1:
        _fail("G2 risk supports only one long-or-flat market position")
    position = positions[0]
    if (
        position["market_id"] != snapshot["market_id"]
        or position["base_mint"] != snapshot["base_mint"]
        or position["base_decimals"] != snapshot["base_decimals"]
    ):
        _fail("portfolio open-position identity does not match the feature snapshot")
    return position


def _exit_evaluation(
    *,
    fusion_decision_id: str,
    mapped_action: str,
    snapshot: Mapping[str, Any],
    portfolio: Mapping[str, Any],
    config: Mapping[str, Any],
    position: Mapping[str, Any],
    price: int,
    stale_age: int,
    impact: int,
    equity: int,
    session_pnl: int,
    drawdown: int,
    session_loss: bool,
    drawdown_latched: bool,
) -> RiskEvaluation:
    quantity = _u64_text(position["quantity_base_atoms"], field="position.quantity_base_atoms", positive=True)
    reserved = _u64_text(position["reserved_base_atoms"], field="position.reserved_base_atoms")
    available_quantity = quantity - reserved
    if available_quantity <= 0:
        reason = "RISK_INTENT_PENDING" if cast(list[Any], portfolio["open_intent_ids"]) else "RISK_RESERVATION_CONFLICT"
        record = _risk_decision_record(
            snapshot=snapshot,
            portfolio=portfolio,
            config=config,
            baseline_action="HOLD",
            fused_action="HOLD",
            effective_action="HOLD",
            verdict="REJECT" if reason == "RISK_INTENT_PENDING" else "KILL",
            reason_codes=(reason,),
            reference_price_q18=price,
            requested_base_atoms=0,
            approved_base_atoms=0,
            requested_notional_quote_atoms=0,
            approved_notional_quote_atoms=0,
            measures=_summary_measures(
                participation_bps=0,
                impact_bps=impact,
                concentration_bps=0,
                projected_market_value_quote_atoms=0,
                projected_equity_quote_atoms=equity,
                session_pnl_quote_atoms=session_pnl,
                drawdown_bps=drawdown,
                stale_age_ns=stale_age,
            ),
            stop_price_q18=None,
            take_price_q18=None,
            reservation_id=None,
            reserved_quote_atoms=0,
            reserved_base_atoms=0,
        )
        return RiskEvaluation(fusion_decision_id=fusion_decision_id, risk_decision_record=record, simulated_order_intent_record=None)

    stop = _i128_text(position["stop_price_q18"], field="position.stop_price_q18", positive=True)
    take = _i128_text(position["take_price_q18"], field="position.take_price_q18", positive=True)
    market_value = _u64_text(position["market_value_quote_atoms"], field="position.market_value_quote_atoms")
    reasons: list[str] = []
    if session_loss:
        reasons.append("RISK_SESSION_LOSS")
    if drawdown_latched:
        reasons.append("RISK_DRAWDOWN")
    if portfolio["kill_latched"]:
        reasons.append("RISK_KILL_EXIT")
    elif _u64_text(snapshot["replay_clock_ns"], field="replay_clock_ns") >= _u64_text(
        config["session_end_replay_clock_ns"], field="session_end_replay_clock_ns"
    ):
        reasons.append("RISK_RUN_END_EXIT")
    if price <= stop:
        reasons.append("RISK_STOP_TRIGGERED")
    if price >= take:
        reasons.append("RISK_TAKE_TRIGGERED")
    if not reasons and mapped_action != "EXIT_LONG":
        record = _risk_decision_record(
            snapshot=snapshot,
            portfolio=portfolio,
            config=config,
            baseline_action=mapped_action,
            fused_action=mapped_action,
            effective_action="HOLD",
            verdict="REJECT",
            reason_codes=("RISK_NO_ACTION",),
            reference_price_q18=price,
            requested_base_atoms=0,
            approved_base_atoms=0,
            requested_notional_quote_atoms=0,
            approved_notional_quote_atoms=0,
            measures=_summary_measures(
                participation_bps=0,
                impact_bps=impact,
                concentration_bps=_ceil_div(_checked_mul(market_value, 10_000), equity),
                projected_market_value_quote_atoms=market_value,
                projected_equity_quote_atoms=equity,
                session_pnl_quote_atoms=session_pnl,
                drawdown_bps=drawdown,
                stale_age_ns=stale_age,
            ),
            stop_price_q18=stop,
            take_price_q18=take,
            reservation_id=None,
            reserved_quote_atoms=0,
            reserved_base_atoms=0,
        )
        return RiskEvaluation(fusion_decision_id=fusion_decision_id, risk_decision_record=record, simulated_order_intent_record=None)

    concentration = _ceil_div(_checked_mul(market_value, 10_000), equity)
    reservation = _reservation_id(
        fusion_decision_id=fusion_decision_id,
        feature_snapshot_id=cast(str, snapshot["snapshot_id"]),
        portfolio_state_id=cast(str, portfolio["portfolio_state_id"]),
        config_sha256=cast(str, config["config_sha256"]),
        decision_sequence=cast(str, snapshot["decision_sequence"]),
        market_id=cast(str, snapshot["market_id"]),
        effective_action="EXIT_LONG",
        approved_base_atoms=str(available_quantity),
        approved_notional_quote_atoms=str(market_value),
        reserved_quote_atoms="0",
        reserved_base_atoms=str(available_quantity),
    )
    suppressed = bool(reasons)
    decision_record = _risk_decision_record(
        snapshot=snapshot,
        portfolio=portfolio,
        config=config,
        baseline_action="HOLD" if suppressed else "EXIT_LONG",
        fused_action="HOLD" if suppressed else "EXIT_LONG",
        effective_action="EXIT_LONG",
        verdict="APPROVE",
        reason_codes=reasons,
        reference_price_q18=price,
        requested_base_atoms=available_quantity,
        approved_base_atoms=available_quantity,
        requested_notional_quote_atoms=market_value,
        approved_notional_quote_atoms=market_value,
        measures=_summary_measures(
            participation_bps=0,
            impact_bps=impact,
            concentration_bps=concentration,
            projected_market_value_quote_atoms=market_value,
            projected_equity_quote_atoms=equity,
            session_pnl_quote_atoms=session_pnl,
            drawdown_bps=drawdown,
            stale_age_ns=stale_age,
        ),
        stop_price_q18=stop,
        take_price_q18=take,
        reservation_id=reservation,
        reserved_quote_atoms=0,
        reserved_base_atoms=available_quantity,
    )
    decision = parse_canonical_record(decision_record)
    intent_record = _intent_record(
        snapshot=snapshot,
        portfolio=portfolio,
        config=config,
        risk_decision=decision,
        action="CLOSE_LONG",
        quantity_base_atoms=available_quantity,
        reference_price_q18=price,
        stop_price_q18=stop,
        take_price_q18=take,
        reservation_id=reservation,
        reserved_quote_atoms=0,
        reserved_base_atoms=available_quantity,
    )
    return RiskEvaluation(fusion_decision_id=fusion_decision_id, risk_decision_record=decision_record, simulated_order_intent_record=intent_record)


def _validate_bindings(
    *,
    event_group: EventGroup,
    snapshot: Mapping[str, Any],
    portfolio: Mapping[str, Any],
    config: Mapping[str, Any],
) -> None:
    if snapshot["decision_sequence"] != event_group.group_sequence or snapshot["equal_time_group"] != event_group.group_sequence:
        _fail("feature snapshot does not bind the event group coordinates")
    if snapshot["as_of_event_id"] not in event_group.event_ids:
        _fail("feature snapshot as-of event is outside the retained event group")
    if snapshot["feature_code_sha256"] != event_group.feature_code_sha256:
        _fail("feature snapshot code identity does not match the event group")
    if portfolio["validated_config_sha256"] != config["config_sha256"]:
        _fail("portfolio state does not bind the supplied risk config")
    if portfolio["config_admission_receipt_id"] is None:
        _fail("portfolio state is missing config admission authority")
    if portfolio["quote_mint"] != snapshot["quote_mint"] or portfolio["quote_decimals"] != snapshot["quote_decimals"]:
        _fail("portfolio quote identity does not match the feature snapshot")
    state_ingest = _u64_text(portfolio["as_of_ingest_sequence"], field="portfolio.as_of_ingest_sequence")
    snapshot_ingest = _u64_text(snapshot["as_of_ingest_sequence"], field="snapshot.as_of_ingest_sequence")
    state_group = _u64_text(portfolio["equal_time_group"], field="portfolio.equal_time_group")
    snapshot_group = _u64_text(snapshot["equal_time_group"], field="snapshot.equal_time_group")
    state_clock = _u64_text(portfolio["replay_clock_ns"], field="portfolio.replay_clock_ns")
    snapshot_clock = _u64_text(snapshot["replay_clock_ns"], field="snapshot.replay_clock_ns")
    if state_ingest > snapshot_ingest or state_group > snapshot_group or state_clock > snapshot_clock:
        _fail("portfolio state coordinates cannot be later than the decision group")


def evaluate_risk(
    event_group: EventGroup,
    feature_snapshot_record: bytes,
    algorithm_candidate_records: Sequence[bytes],
    model_evidence: ValidatedModelEvidence,
    fusion_result: FusionResult,
    portfolio_state_record: bytes,
    risk_config_record: bytes,
) -> RiskEvaluation:
    expected_fusion = fuse_signal_evidence(
        event_group,
        feature_snapshot_record,
        algorithm_candidate_records,
        model_evidence,
    )
    if type(fusion_result) is not FusionResult or fusion_result != expected_fusion:
        _fail("fusion result does not exactly match the closed recomputed evidence")

    snapshot = _parse_replay_record(feature_snapshot_record, expected_schema="trading.feature-snapshot/v1")
    portfolio = _parse_replay_record(portfolio_state_record, expected_schema="trading.portfolio-state/v1")
    config = _parse_replay_record(risk_config_record, expected_schema="trading.replay-risk-config/v1")
    fusion = _parse_live_record(fusion_result.fusion_decision_record, expected_schema="trading.fusion-decision/v1")
    _parse_live_record(
        fusion_result.decision_group_manifest_record,
        expected_schema="trading.decision-group-manifest/v1",
    )
    _validate_bindings(event_group=event_group, snapshot=snapshot, portfolio=portfolio, config=config)

    price, liquidity, stale_age, impact = _reference_inputs(snapshot)
    mapped_action = _action(fusion["fused_action"])
    position = _held_position(portfolio, snapshot)
    equity, session_pnl, drawdown = _state_numbers(portfolio)
    session_loss = session_pnl <= -_u64_text(config["max_session_loss_quote_atoms"], field="max_session_loss")
    drawdown_latched = drawdown >= cast(int, config["max_drawdown_bps"])

    if price is None or price <= 0 or liquidity is None or liquidity <= 0 or stale_age is None or impact is None:
        record = _risk_decision_record(
            snapshot=snapshot,
            portfolio=portfolio,
            config=config,
            baseline_action="HOLD",
            fused_action="HOLD",
            effective_action="HOLD",
            verdict="REJECT",
            reason_codes=("RISK_NO_ACTION",),
            reference_price_q18=None,
            requested_base_atoms=0,
            approved_base_atoms=0,
            requested_notional_quote_atoms=0,
            approved_notional_quote_atoms=0,
            measures=_zero_measures(),
            stop_price_q18=None,
            take_price_q18=None,
            reservation_id=None,
            reserved_quote_atoms=0,
            reserved_base_atoms=0,
        )
        return RiskEvaluation(
            fusion_decision_id=cast(str, fusion["fusion_decision_id"]),
            risk_decision_record=record,
            simulated_order_intent_record=None,
        )

    if equity <= 0:
        measures = _summary_measures(
            participation_bps=0,
            impact_bps=None,
            concentration_bps=None,
            projected_market_value_quote_atoms=0,
            projected_equity_quote_atoms=0,
            session_pnl_quote_atoms=session_pnl,
            drawdown_bps=drawdown,
            stale_age_ns=None,
        )
        record = _risk_decision_record(
            snapshot=snapshot,
            portfolio=portfolio,
            config=config,
            baseline_action="HOLD",
            fused_action="HOLD",
            effective_action="HOLD",
            verdict="KILL",
            reason_codes=("RISK_NONPOSITIVE_EQUITY",),
            reference_price_q18=price,
            requested_base_atoms=0,
            approved_base_atoms=0,
            requested_notional_quote_atoms=0,
            approved_notional_quote_atoms=0,
            measures=measures,
            stop_price_q18=None,
            take_price_q18=None,
            reservation_id=None,
            reserved_quote_atoms=0,
            reserved_base_atoms=0,
        )
        return RiskEvaluation(cast(str, fusion["fusion_decision_id"]), record, None)

    if position is not None:
        if cast(list[Any], portfolio["open_intent_ids"]):
            record = _risk_decision_record(
                snapshot=snapshot,
                portfolio=portfolio,
                config=config,
                baseline_action="HOLD",
                fused_action="HOLD",
                effective_action="HOLD",
                verdict="REJECT",
                reason_codes=("RISK_INTENT_PENDING",),
                reference_price_q18=price,
                requested_base_atoms=0,
                approved_base_atoms=0,
                requested_notional_quote_atoms=0,
                approved_notional_quote_atoms=0,
                measures=_summary_measures(
                    participation_bps=0,
                    impact_bps=impact,
                    concentration_bps=0,
                    projected_market_value_quote_atoms=0,
                    projected_equity_quote_atoms=equity,
                    session_pnl_quote_atoms=session_pnl,
                    drawdown_bps=drawdown,
                    stale_age_ns=stale_age,
                ),
                stop_price_q18=None,
                take_price_q18=None,
                reservation_id=None,
                reserved_quote_atoms=0,
                reserved_base_atoms=0,
            )
            return RiskEvaluation(cast(str, fusion["fusion_decision_id"]), record, None)
        return _exit_evaluation(
            fusion_decision_id=cast(str, fusion["fusion_decision_id"]),
            mapped_action=mapped_action,
            snapshot=snapshot,
            portfolio=portfolio,
            config=config,
            position=position,
            price=price,
            stale_age=stale_age,
            impact=impact,
            equity=equity,
            session_pnl=session_pnl,
            drawdown=drawdown,
            session_loss=session_loss,
            drawdown_latched=drawdown_latched,
        )

    flat_kill = _flat_kill_reasons(portfolio, session_loss=session_loss, drawdown=drawdown_latched)
    if flat_kill:
        measures = _summary_measures(
            participation_bps=0,
            impact_bps=impact,
            concentration_bps=0,
            projected_market_value_quote_atoms=0,
            projected_equity_quote_atoms=equity,
            session_pnl_quote_atoms=session_pnl,
            drawdown_bps=drawdown,
            stale_age_ns=stale_age,
        )
        record = _risk_decision_record(
            snapshot=snapshot,
            portfolio=portfolio,
            config=config,
            baseline_action="HOLD",
            fused_action="HOLD",
            effective_action="HOLD",
            verdict="KILL",
            reason_codes=flat_kill,
            reference_price_q18=price,
            requested_base_atoms=0,
            approved_base_atoms=0,
            requested_notional_quote_atoms=0,
            approved_notional_quote_atoms=0,
            measures=measures,
            stop_price_q18=None,
            take_price_q18=None,
            reservation_id=None,
            reserved_quote_atoms=0,
            reserved_base_atoms=0,
        )
        return RiskEvaluation(cast(str, fusion["fusion_decision_id"]), record, None)

    if cast(list[Any], portfolio["open_intent_ids"]):
        record = _risk_decision_record(
            snapshot=snapshot,
            portfolio=portfolio,
            config=config,
            baseline_action="HOLD",
            fused_action="HOLD",
            effective_action="HOLD",
            verdict="REJECT",
            reason_codes=("RISK_INTENT_PENDING",),
            reference_price_q18=price,
            requested_base_atoms=0,
            approved_base_atoms=0,
            requested_notional_quote_atoms=0,
            approved_notional_quote_atoms=0,
            measures=_summary_measures(
                participation_bps=0,
                impact_bps=impact,
                concentration_bps=0,
                projected_market_value_quote_atoms=0,
                projected_equity_quote_atoms=equity,
                session_pnl_quote_atoms=session_pnl,
                drawdown_bps=drawdown,
                stale_age_ns=stale_age,
            ),
            stop_price_q18=None,
            take_price_q18=None,
            reservation_id=None,
            reserved_quote_atoms=0,
            reserved_base_atoms=0,
        )
        return RiskEvaluation(cast(str, fusion["fusion_decision_id"]), record, None)

    if _u64_text(snapshot["replay_clock_ns"], field="replay_clock_ns") >= _u64_text(
        config["session_end_replay_clock_ns"], field="session_end_replay_clock_ns"
    ):
        record = _risk_decision_record(
            snapshot=snapshot,
            portfolio=portfolio,
            config=config,
            baseline_action="HOLD",
            fused_action="HOLD",
            effective_action="HOLD",
            verdict="REJECT",
            reason_codes=("RISK_SESSION_CLOSED",),
            reference_price_q18=price,
            requested_base_atoms=0,
            approved_base_atoms=0,
            requested_notional_quote_atoms=0,
            approved_notional_quote_atoms=0,
            measures=_summary_measures(
                participation_bps=0,
                impact_bps=impact,
                concentration_bps=0,
                projected_market_value_quote_atoms=0,
                projected_equity_quote_atoms=equity,
                session_pnl_quote_atoms=session_pnl,
                drawdown_bps=drawdown,
                stale_age_ns=stale_age,
            ),
            stop_price_q18=None,
            take_price_q18=None,
            reservation_id=None,
            reserved_quote_atoms=0,
            reserved_base_atoms=0,
        )
        return RiskEvaluation(cast(str, fusion["fusion_decision_id"]), record, None)

    if mapped_action != "ENTER_LONG":
        record = _risk_decision_record(
            snapshot=snapshot,
            portfolio=portfolio,
            config=config,
            baseline_action=mapped_action,
            fused_action=mapped_action,
            effective_action="HOLD",
            verdict="REJECT",
            reason_codes=("RISK_NO_ACTION",),
            reference_price_q18=price,
            requested_base_atoms=0,
            approved_base_atoms=0,
            requested_notional_quote_atoms=0,
            approved_notional_quote_atoms=0,
            measures=_summary_measures(
                participation_bps=0,
                impact_bps=impact,
                concentration_bps=0,
                projected_market_value_quote_atoms=0,
                projected_equity_quote_atoms=equity,
                session_pnl_quote_atoms=session_pnl,
                drawdown_bps=drawdown,
                stale_age_ns=stale_age,
            ),
            stop_price_q18=None,
            take_price_q18=None,
            reservation_id=None,
            reserved_quote_atoms=0,
            reserved_base_atoms=0,
        )
        return RiskEvaluation(cast(str, fusion["fusion_decision_id"]), record, None)

    return _entry_evaluation(
        fusion_decision_id=cast(str, fusion["fusion_decision_id"]),
        mapped_action=mapped_action,
        snapshot=snapshot,
        portfolio=portfolio,
        config=config,
        price=price,
        liquidity=liquidity,
        stale_age=stale_age,
        impact=impact,
        equity=equity,
        session_pnl=session_pnl,
        drawdown=drawdown,
    )
