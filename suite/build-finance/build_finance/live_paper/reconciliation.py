"""Independent reconciliation receipts for G2 paper accounting transitions."""

from __future__ import annotations

import copy
from collections.abc import Mapping, Sequence
from typing import Any, Literal, cast

from build_finance.crypto_replay.canonical import (
    JsonValue,
    canonical_json_bytes,
    canonical_record_bytes,
    parse_canonical_record,
    sha256_hex,
)
from build_finance.crypto_replay.content_ids import seal_content_id, verify_content_id
from build_finance.crypto_replay.run_inputs import ContractVerifiedRunInputs
from build_finance.crypto_replay.schema_registry import require_valid_contract

_MAX_U64 = 18_446_744_073_709_551_615
_MIN_I128 = -170_141_183_460_469_231_731_687_303_715_884_105_728
_MAX_I128 = 170_141_183_460_469_231_731_687_303_715_884_105_727
_ZERO_FIELDS = {
    "realized_pnl_residual_quote_atoms": "0",
    "unrealized_pnl_residual_quote_atoms": "0",
    "fee_residual_quote_atoms": "0",
    "peak_equity_residual_quote_atoms": "0",
    "drawdown_residual_bps": "0",
    "equity_residual_quote_atoms": "0",
    "unmatched_reservation_count": "0",
}


class _ArithmeticRange(ValueError):
    """Raised when independent reconciliation arithmetic exceeds frozen bounds."""

    def __init__(
        self,
        message: str,
        *,
        operation: str = "SUMMARY_EQUITY",
        operands: Sequence[tuple[str, int]] = (),
        coordinates: Mapping[str, Any] | None = None,
    ) -> None:
        super().__init__(message)
        self.operation = operation
        self.operands = tuple(operands)
        self.coordinates = {} if coordinates is None else dict(coordinates)


class _ReconciliationMismatch(ValueError):
    """Raised for an independently observed transition mismatch."""


def _require_verified_run(verified: ContractVerifiedRunInputs) -> Mapping[str, object]:
    if not isinstance(verified, ContractVerifiedRunInputs) or verified.authority != "CONTRACT_ONLY":
        raise ValueError("reconciliation requires frozen contract-verified run inputs")
    run_id = verified.run_receipt.get("run_receipt_id")
    if not isinstance(run_id, str) or len(run_id) != 64:
        raise ValueError("verified run receipt identity is absent")
    return verified.run_receipt


def _parse_record(record: bytes, expected_schema: str) -> dict[str, Any]:
    document = cast(dict[str, Any], parse_canonical_record(bytes(record)))
    require_valid_contract(document, expected_schema=expected_schema)
    if not verify_content_id(document) or canonical_record_bytes(document) != bytes(record):
        raise ValueError(f"{expected_schema} self-ID does not match retained bytes")
    return document


def _best_effort_doc(record: bytes | None) -> dict[str, Any]:
    if record is None:
        return {}
    try:
        return cast(dict[str, Any], parse_canonical_record(bytes(record)))
    except ValueError:
        return {}


def _u64(value: object, *, field: str, positive: bool = False) -> int:
    if not isinstance(value, str) or not value.isascii() or not value.isdigit() or str(int(value)) != value:
        raise ValueError(f"{field} must be canonical u64 text")
    parsed = int(value)
    if parsed > _MAX_U64 or (positive and parsed == 0):
        raise ValueError(f"{field} is outside its u64 range")
    return parsed


def _i128(value: object, *, field: str) -> int:
    if not isinstance(value, str):
        raise ValueError(f"{field} must be canonical i128 text")
    if value == "0":
        parsed = 0
    elif value.startswith("-"):
        digits = value[1:]
        if not digits or digits.startswith("0") or not digits.isascii() or not digits.isdigit():
            raise ValueError(f"{field} must be canonical i128 text")
        parsed = -int(digits)
    elif value.isascii() and value.isdigit() and not value.startswith("0"):
        parsed = int(value)
    else:
        raise ValueError(f"{field} must be canonical i128 text")
    if parsed < _MIN_I128 or parsed > _MAX_I128:
        raise ValueError(f"{field} is outside its i128 range")
    return parsed


def _checked_u64(
    value: int,
    *,
    operation: str = "SUMMARY_EQUITY",
    operands: Sequence[tuple[str, int]] = (),
    coordinates: Mapping[str, Any] | None = None,
) -> int:
    if value < 0 or value > _MAX_U64:
        raise _ArithmeticRange(
            "u64 arithmetic range exceeded",
            operation=operation,
            operands=operands,
            coordinates=coordinates,
        )
    return value


def _checked_i128(
    value: int,
    *,
    operation: str = "SUMMARY_EQUITY",
    operands: Sequence[tuple[str, int]] = (),
    coordinates: Mapping[str, Any] | None = None,
) -> int:
    if value < _MIN_I128 or value > _MAX_I128:
        raise _ArithmeticRange(
            "i128 arithmetic range exceeded",
            operation=operation,
            operands=operands,
            coordinates=coordinates,
        )
    return value


def _ceil_div(numerator: int, denominator: int) -> int:
    if numerator < 0 or denominator <= 0:
        raise _ArithmeticRange(
            "ceiling division requires a non-negative numerator and positive denominator",
            operands=(("numerator", numerator), ("denominator", denominator)),
        )
    return (numerator + denominator - 1) // denominator


def _content_id(document: Mapping[str, Any]) -> str:
    schema = document.get("schema")
    field_by_schema = {
        "trading.portfolio-state/v1": "portfolio_state_id",
        "trading.simulated-order-intent/v1": "intent_id",
        "trading.simulated-fill-receipt/v1": "fill_receipt_id",
        "trading.ledger-record/v1": "ledger_record_id",
        "trading.reconciliation-receipt/v1": "reconciliation_receipt_id",
    }
    field = field_by_schema.get(str(schema))
    value = None if field is None else document.get(field)
    if isinstance(value, str):
        return value
    raise ValueError("record has no supported content identity")


def _seal_record(document: Mapping[str, JsonValue], expected_schema: str) -> bytes:
    sealed = seal_content_id(document)
    require_valid_contract(sealed, expected_schema=expected_schema)
    return canonical_record_bytes(sealed)


def _sorted_ids(values: Sequence[str]) -> list[str]:
    return sorted(dict.fromkeys(values), key=lambda value: value.encode("utf-8"))


def _zero_reconciliation(portfolio_state_id: str) -> dict[str, JsonValue]:
    return {
        "portfolio_state_id": portfolio_state_id,
        "asset_residuals": [],
        "equity_residual_quote_atoms": "0",
        "unmatched_reservation_count": "0",
        "account_residuals": [],
        "realized_pnl_residual_quote_atoms": "0",
        "unrealized_pnl_residual_quote_atoms": "0",
        "fee_residual_quote_atoms": "0",
        "peak_equity_residual_quote_atoms": "0",
        "drawdown_residual_bps": "0",
    }


def _balance_map(state: Mapping[str, Any]) -> dict[str, dict[str, Any]]:
    return {str(row["mint"]): copy.deepcopy(dict(row)) for row in cast(list[Mapping[str, Any]], state["balances"])}


def _position_map(state: Mapping[str, Any]) -> dict[str, dict[str, Any]]:
    return {str(row["market_id"]): copy.deepcopy(dict(row)) for row in cast(list[Mapping[str, Any]], state["positions"])}


def _sort_balances(balances: Mapping[str, Mapping[str, Any]], quote_mint: str) -> list[dict[str, JsonValue]]:
    rows: list[dict[str, JsonValue]] = []
    for mint, row in balances.items():
        total = _u64(row["total_atoms"], field=f"balance[{mint}].total_atoms")
        if mint != quote_mint and total == 0:
            continue
        rows.append(cast(dict[str, JsonValue], dict(row)))
    return sorted(rows, key=lambda row: str(row["mint"]).encode("utf-8"))


def _sort_positions(positions: Mapping[str, Mapping[str, Any]]) -> list[dict[str, JsonValue]]:
    rows = [cast(dict[str, JsonValue], dict(row)) for row in positions.values() if row["quantity_base_atoms"] != "0"]
    return sorted(rows, key=lambda row: str(row["market_id"]).encode("utf-8"))


def _set_balance(row: dict[str, Any], *, available: int, reserved: int) -> None:
    total = available + reserved
    operands = (
        ("available_atoms", available),
        ("reserved_atoms", reserved),
        ("total_atoms", total),
        ("decimals", int(row["decimals"])),
    )
    row["available_atoms"] = str(_checked_u64(available, operation="LEDGER_POSTING", operands=operands))
    row["reserved_atoms"] = str(_checked_u64(reserved, operation="LEDGER_POSTING", operands=operands))
    row["total_atoms"] = str(_checked_u64(total, operation="LEDGER_POSTING", operands=operands))


def _summary_from_state(
    *,
    before: Mapping[str, Any],
    quote_balance: Mapping[str, Any],
    positions: Mapping[str, Mapping[str, Any]],
    realized: int,
    cumulative_fees: int,
) -> dict[str, JsonValue]:
    market_value_total = 0
    unrealized_total = 0
    for position in positions.values():
        market_value_total += _u64(position["market_value_quote_atoms"], field="position.market_value_quote_atoms")
        unrealized_total += _i128(position["unrealized_pnl_quote_atoms"], field="position.unrealized_pnl_quote_atoms")
    quote_total = _u64(quote_balance["total_atoms"], field="quote_balance.total_atoms")
    equity = _checked_u64(quote_total + market_value_total)
    before_summary = cast(Mapping[str, Any], before["summary"])
    peak = max(_u64(before_summary["peak_equity_quote_atoms"], field="summary.peak_equity_quote_atoms"), equity)
    drawdown = 0 if peak == 0 else _ceil_div((peak - equity) * 10_000, peak)
    session = _checked_i128(realized + unrealized_total)
    return {
        "realized_pnl_quote_atoms": str(_checked_i128(realized)),
        "unrealized_pnl_quote_atoms": str(_checked_i128(unrealized_total)),
        "cumulative_fees_quote_atoms": str(_checked_u64(cumulative_fees)),
        "session_pnl_quote_atoms": str(session),
        "equity_quote_atoms": str(equity),
        "peak_equity_quote_atoms": str(_checked_u64(peak)),
        "drawdown_bps": drawdown,
    }


def _expected_intent_state(before: Mapping[str, Any], intent: Mapping[str, Any]) -> bytes:
    before_id = cast(str, before["portfolio_state_id"])
    quote_mint = cast(str, before["quote_mint"])
    balances = _balance_map(before)
    positions = _position_map(before)
    open_intents = list(cast(list[str], before["open_intent_ids"]))
    intent_id = cast(str, intent["intent_id"])
    if intent_id in open_intents:
        raise _ReconciliationMismatch("intent is already open")

    if intent["action"] == "OPEN_LONG":
        quote = balances[quote_mint]
        reserved = _u64(intent["reserved_quote_atoms"], field="intent.reserved_quote_atoms")
        _set_balance(
            quote,
            available=_u64(quote["available_atoms"], field="quote.available_atoms") - reserved,
            reserved=_u64(quote["reserved_atoms"], field="quote.reserved_atoms") + reserved,
        )
    else:
        base_mint = cast(str, intent["base_mint"])
        market_id = cast(str, intent["market_id"])
        reserved = _u64(intent["reserved_base_atoms"], field="intent.reserved_base_atoms")
        base = balances[base_mint]
        position = positions[market_id]
        _set_balance(
            base,
            available=_u64(base["available_atoms"], field="base.available_atoms") - reserved,
            reserved=_u64(base["reserved_atoms"], field="base.reserved_atoms") + reserved,
        )
        position["reserved_base_atoms"] = str(
            _checked_u64(_u64(position["reserved_base_atoms"], field="position.reserved_base_atoms") + reserved)
        )

    after = copy.deepcopy(dict(before))
    after.pop("portfolio_state_id", None)
    after.update(
        {
            "state_sequence": str(_checked_u64(_u64(before["state_sequence"], field="state_sequence") + 1)),
            "as_of_ingest_sequence": intent["decision_ingest_sequence"],
            "equal_time_group": intent["decision_equal_time_group"],
            "replay_clock_ns": intent["created_replay_clock_ns"],
            "previous_portfolio_state_id": before_id,
            "causation_schema": "trading.simulated-order-intent/v1",
            "causation_id": intent_id,
            "balances": _sort_balances(balances, quote_mint),
            "positions": _sort_positions(positions),
            "open_intent_ids": _sorted_ids((*open_intents, intent_id)),
        }
    )
    return _seal_record(cast(Mapping[str, JsonValue], after), "trading.portfolio-state/v1")


def _fees(fill: Mapping[str, Any]) -> int:
    return (
        _u64(fill["venue_fee_quote_atoms"], field="venue_fee_quote_atoms")
        + _u64(fill["priority_fee_quote_atoms"], field="priority_fee_quote_atoms")
        + _u64(fill["simulation_fee_quote_atoms"], field="simulation_fee_quote_atoms")
    )


def _expected_fill_state(before: Mapping[str, Any], intent: Mapping[str, Any], fill: Mapping[str, Any]) -> bytes:
    before_id = cast(str, before["portfolio_state_id"])
    quote_mint = cast(str, before["quote_mint"])
    balances = _balance_map(before)
    positions = _position_map(before)
    open_intents = [value for value in cast(list[str], before["open_intent_ids"]) if value != intent["intent_id"]]
    before_summary = cast(Mapping[str, Any], before["summary"])
    realized = _i128(before_summary["realized_pnl_quote_atoms"], field="summary.realized_pnl_quote_atoms")
    cumulative_fees = _u64(before_summary["cumulative_fees_quote_atoms"], field="summary.cumulative_fees_quote_atoms")
    filled = _u64(fill["filled_base_atoms"], field="fill.filled_base_atoms")
    gross = _u64(fill["gross_quote_atoms"], field="fill.gross_quote_atoms")
    fee_total = _fees(fill)
    cumulative_fees = _checked_u64(cumulative_fees + fee_total)
    status = fill["status"]

    if intent["action"] == "OPEN_LONG":
        quote = balances[quote_mint]
        reserved_quote = _u64(intent["reserved_quote_atoms"], field="intent.reserved_quote_atoms")
        released_quote = _u64(fill["released_quote_atoms"], field="fill.released_quote_atoms")
        _set_balance(
            quote,
            available=_u64(quote["available_atoms"], field="quote.available_atoms") + released_quote,
            reserved=_u64(quote["reserved_atoms"], field="quote.reserved_atoms") - reserved_quote,
        )
        if status in ("FILLED", "PARTIAL") and filled:
            base_mint = cast(str, intent["base_mint"])
            base = balances.setdefault(
                base_mint,
                {
                    "mint": base_mint,
                    "decimals": intent["base_decimals"],
                    "available_atoms": "0",
                    "reserved_atoms": "0",
                    "total_atoms": "0",
                },
            )
            _set_balance(
                base,
                available=_u64(base["available_atoms"], field="base.available_atoms") + filled,
                reserved=_u64(base["reserved_atoms"], field="base.reserved_atoms"),
            )
            market_id = cast(str, intent["market_id"])
            prior = positions.get(market_id)
            prior_quantity = 0 if prior is None else _u64(prior["quantity_base_atoms"], field="position.quantity")
            prior_cost = 0 if prior is None else _u64(prior["cost_basis_quote_atoms"], field="position.cost_basis")
            prior_market = 0 if prior is None else _u64(prior["market_value_quote_atoms"], field="position.market_value")
            quantity = _checked_u64(prior_quantity + filled)
            cost_basis = _checked_u64(prior_cost + gross + fee_total)
            market_value = _checked_u64(prior_market + gross)
            positions[market_id] = {
                "market_id": market_id,
                "base_mint": base_mint,
                "base_decimals": intent["base_decimals"],
                "quantity_base_atoms": str(quantity),
                "reserved_base_atoms": "0" if prior is None else prior["reserved_base_atoms"],
                "cost_basis_quote_atoms": str(cost_basis),
                "mark_price_q18": fill["execution_price_q18"],
                "market_value_quote_atoms": str(market_value),
                "unrealized_pnl_quote_atoms": str(_checked_i128(market_value - cost_basis)),
                "stop_price_q18": intent["stop_price_q18"],
                "take_price_q18": intent["take_price_q18"],
            }
    else:
        base_mint = cast(str, intent["base_mint"])
        market_id = cast(str, intent["market_id"])
        reserved_base = _u64(intent["reserved_base_atoms"], field="intent.reserved_base_atoms")
        released_base = _u64(fill["released_base_atoms"], field="fill.released_base_atoms")
        base = balances[base_mint]
        _set_balance(
            base,
            available=_u64(base["available_atoms"], field="base.available_atoms") + released_base,
            reserved=_u64(base["reserved_atoms"], field="base.reserved_atoms") - reserved_base,
        )
        quote = balances[quote_mint]
        cash_delta = _i128(fill["cash_delta_quote_atoms"], field="fill.cash_delta_quote_atoms")
        _set_balance(
            quote,
            available=_u64(quote["available_atoms"], field="quote.available_atoms") + cash_delta,
            reserved=_u64(quote["reserved_atoms"], field="quote.reserved_atoms"),
        )
        if status in ("FILLED", "PARTIAL") and filled:
            position = positions[market_id]
            quantity_before = _u64(position["quantity_base_atoms"], field="position.quantity_base_atoms")
            cost_before = _u64(position["cost_basis_quote_atoms"], field="position.cost_basis_quote_atoms")
            market_before = _u64(position["market_value_quote_atoms"], field="position.market_value_quote_atoms")
            if filled > quantity_before:
                raise _ReconciliationMismatch("fill exceeds open position quantity")
            allocated_cost = cost_before if filled == quantity_before else (cost_before * filled) // quantity_before
            realized = _checked_i128(realized + gross - fee_total - allocated_cost)
            quantity_after = quantity_before - filled
            cost_after = cost_before - allocated_cost
            if quantity_after == 0:
                positions.pop(market_id)
            else:
                market_after = (market_before * quantity_after) // quantity_before
                position["quantity_base_atoms"] = str(quantity_after)
                position["cost_basis_quote_atoms"] = str(cost_after)
                position["market_value_quote_atoms"] = str(market_after)
                position["reserved_base_atoms"] = str(
                    _checked_u64(_u64(position["reserved_base_atoms"], field="position.reserved_base_atoms") - reserved_base)
                )
                position["unrealized_pnl_quote_atoms"] = str(_checked_i128(market_after - cost_after))

    quote_balance = balances[quote_mint]
    after = copy.deepcopy(dict(before))
    after.pop("portfolio_state_id", None)
    after.update(
        {
            "state_sequence": str(_checked_u64(_u64(before["state_sequence"], field="state_sequence") + 1)),
            "as_of_ingest_sequence": fill["fill_ingest_sequence"] or fill["decision_ingest_sequence"],
            "equal_time_group": fill["fill_equal_time_group"] or fill["decision_equal_time_group"],
            "replay_clock_ns": fill["fill_replay_clock_ns"] or before["replay_clock_ns"],
            "previous_portfolio_state_id": before_id,
            "causation_schema": "trading.simulated-fill-receipt/v1",
            "causation_id": fill["fill_receipt_id"],
            "balances": _sort_balances(balances, quote_mint),
            "positions": _sort_positions(positions),
            "summary": _summary_from_state(
                before=before,
                quote_balance=quote_balance,
                positions=positions,
                realized=realized,
                cumulative_fees=cumulative_fees,
            ),
            "open_intent_ids": _sorted_ids(open_intents),
        }
    )
    return _seal_record(cast(Mapping[str, JsonValue], after), "trading.portfolio-state/v1")


def _intent_postings(intent: Mapping[str, Any]) -> list[dict[str, JsonValue]]:
    if intent["action"] == "OPEN_LONG":
        reserved = _u64(intent["reserved_quote_atoms"], field="intent.reserved_quote_atoms")
        return _posting_rows(
            [
                (cast(str, intent["quote_mint"]), "CASH_AVAILABLE", cast(int, intent["quote_decimals"]), -reserved),
                (cast(str, intent["quote_mint"]), "CASH_RESERVED", cast(int, intent["quote_decimals"]), reserved),
            ]
        )
    reserved = _u64(intent["reserved_base_atoms"], field="intent.reserved_base_atoms")
    return _posting_rows(
        [
            (cast(str, intent["base_mint"]), "POSITION_AVAILABLE", cast(int, intent["base_decimals"]), -reserved),
            (cast(str, intent["base_mint"]), "POSITION_RESERVED", cast(int, intent["base_decimals"]), reserved),
        ]
    )


def _posting_rows(rows: Sequence[tuple[str, str, int, int]]) -> list[dict[str, JsonValue]]:
    aggregate: dict[tuple[str, str], tuple[int, int]] = {}
    for mint, account, decimals, amount in rows:
        if amount == 0:
            continue
        key = (mint, account)
        prior = aggregate.get(key)
        if prior is not None and prior[0] != decimals:
            raise _ReconciliationMismatch("posting decimals disagree")
        prior_amount = 0 if prior is None else prior[1]
        aggregate[key] = (
            decimals,
            _checked_i128(
                prior_amount + amount,
                operation="LEDGER_POSTING",
                operands=(
                    ("prior_amount_atoms", prior_amount),
                    ("posting_amount_atoms", amount),
                    ("decimals", decimals),
                ),
            ),
        )
    return [
        {"account": account, "asset_mint": mint, "decimals": decimals, "amount_atoms": str(amount)}
        for (mint, account), (decimals, amount) in sorted(
            aggregate.items(),
            key=lambda item: (item[0][0].encode("utf-8"), item[0][1].encode("utf-8")),
        )
        if amount != 0
    ]


def _summary_delta(before: Mapping[str, Any], after: Mapping[str, Any], field: str) -> int:
    return _i128(cast(Mapping[str, Any], after["summary"])[field], field=f"after.{field}") - _i128(
        cast(Mapping[str, Any], before["summary"])[field],
        field=f"before.{field}",
    )


def _fill_postings(before: Mapping[str, Any], after: Mapping[str, Any], intent: Mapping[str, Any], fill: Mapping[str, Any]) -> list[dict[str, JsonValue]]:
    rows: list[tuple[str, str, int, int]] = []
    quote = cast(str, intent["quote_mint"])
    quote_decimals = cast(int, intent["quote_decimals"])
    base = cast(str, intent["base_mint"])
    base_decimals = cast(int, intent["base_decimals"])
    filled = _u64(fill["filled_base_atoms"], field="fill.filled_base_atoms")
    gross = _u64(fill["gross_quote_atoms"], field="fill.gross_quote_atoms")
    if intent["action"] == "OPEN_LONG":
        rows.extend(
            [
                (base, "POSITION_AVAILABLE", base_decimals, filled),
                (base, "TRADE_CLEARING", base_decimals, -filled),
                (quote, "CASH_AVAILABLE", quote_decimals, _u64(fill["released_quote_atoms"], field="released_quote")),
                (quote, "CASH_RESERVED", quote_decimals, -_u64(intent["reserved_quote_atoms"], field="reserved_quote")),
                (quote, "TRADE_CLEARING", quote_decimals, gross),
            ]
        )
    else:
        rows.extend(
            [
                (base, "POSITION_AVAILABLE", base_decimals, _u64(fill["released_base_atoms"], field="released_base")),
                (base, "POSITION_RESERVED", base_decimals, -_u64(intent["reserved_base_atoms"], field="reserved_base")),
                (base, "TRADE_CLEARING", base_decimals, filled),
                (quote, "CASH_AVAILABLE", quote_decimals, _i128(fill["cash_delta_quote_atoms"], field="cash_delta")),
                (quote, "TRADE_CLEARING", quote_decimals, -gross),
            ]
        )
    fee_delta = _summary_delta(before, after, "cumulative_fees_quote_atoms")
    realized_delta = _summary_delta(before, after, "realized_pnl_quote_atoms")
    unrealized_delta = _summary_delta(before, after, "unrealized_pnl_quote_atoms")
    rows.extend(
        [
            (quote, "FEE_EXPENSE", quote_decimals, fee_delta),
            (quote, "REALIZED_PNL", quote_decimals, realized_delta),
            (quote, "UNREALIZED_PNL", quote_decimals, unrealized_delta),
        ]
    )
    quote_sum = sum(amount for mint, _account, _decimals, amount in rows if mint == quote)
    rows.append((quote, "EQUITY_CONTROL", quote_decimals, -quote_sum))
    return _posting_rows(rows)


def _expected_ledger(
    *,
    verified: ContractVerifiedRunInputs,
    prior_ledgers: Sequence[Mapping[str, Any]],
    record_type: str,
    object_document: Mapping[str, Any],
    portfolio_state_id: str,
    causation_ids: Sequence[str],
    entries: Sequence[Mapping[str, JsonValue]],
) -> bytes:
    run = verified.run_receipt
    previous_ledger_record_id: str | None = None
    for expected_sequence, ledger in enumerate(prior_ledgers):
        if ledger["ledger_sequence"] != str(expected_sequence) or ledger["previous_ledger_record_id"] != previous_ledger_record_id:
            raise _ReconciliationMismatch("retained ledger chain coordinates are not independently consistent")
        previous_ledger_record_id = cast(str, ledger["ledger_record_id"])
    if record_type == "SIMULATED_INTENT":
        decision_sequence = object_document["decision_sequence"]
        ingest_sequence = object_document["decision_ingest_sequence"]
        equal_time_group = object_document["decision_equal_time_group"]
        replay_clock_ns = object_document["created_replay_clock_ns"]
    else:
        decision_sequence = object_document["decision_sequence"]
        ingest_sequence = object_document["fill_ingest_sequence"] or object_document["decision_ingest_sequence"]
        equal_time_group = object_document["fill_equal_time_group"] or object_document["decision_equal_time_group"]
        replay_clock_ns = object_document["fill_replay_clock_ns"]
    ledger = {
        "schema": "trading.ledger-record/v1",
        "ledger_sequence": str(len(prior_ledgers)),
        "previous_ledger_record_id": previous_ledger_record_id,
        "run_receipt_id": run["run_receipt_id"],
        "fixture_manifest_sha256": run["fixture_manifest_sha256"],
        "config_admission_receipt_id": run["config_admission_receipt_id"],
        "validated_config_sha256": run["validated_config_sha256"],
        "record_type": record_type,
        "object_schema": object_document["schema"],
        "object_id": _content_id(object_document),
        "object_sha256": _content_id(object_document),
        "decision_sequence": decision_sequence,
        "ingest_sequence": ingest_sequence,
        "equal_time_group": equal_time_group,
        "replay_clock_ns": replay_clock_ns,
        "causation_ids": _sorted_ids(causation_ids),
        "entries": [dict(row) for row in entries],
        "reconciliation": _zero_reconciliation(portfolio_state_id),
    }
    return _seal_record(cast(Mapping[str, JsonValue], ledger), "trading.ledger-record/v1")


def _receipt_record(
    *,
    status: Literal["PASS", "KILLED"],
    kind: str,
    decision_sequence: str | None,
    ingest_sequence: str,
    equal_time_group: str,
    replay_clock_ns: str,
    before_id: str,
    after_id: str,
    causation_ids: Sequence[str],
    reason_codes: Sequence[str] = (),
    asset_residuals: Sequence[Mapping[str, JsonValue]] = (),
    account_residuals: Sequence[Mapping[str, JsonValue]] = (),
    residuals: Mapping[str, str] | None = None,
    idempotency_conflict_scope: str | None = None,
    idempotency_key_sha256: str | None = None,
    original_object_id: str | None = None,
    conflicting_body_sha256: str | None = None,
    arithmetic_range_key_sha256: str | None = None,
    arithmetic_operands_sha256: str | None = None,
    arithmetic_operation: str | None = None,
) -> bytes:
    values = dict(_ZERO_FIELDS if residuals is None else residuals)
    receipt = {
        "schema": "trading.reconciliation-receipt/v1",
        "status": status,
        "reconciliation_kind": kind,
        "reason_codes": list(reason_codes),
        "decision_sequence": decision_sequence,
        "ingest_sequence": ingest_sequence,
        "equal_time_group": equal_time_group,
        "replay_clock_ns": replay_clock_ns,
        "portfolio_state_before_id": before_id,
        "portfolio_state_after_id": after_id,
        "causation_ids": _sorted_ids(causation_ids),
        "asset_residuals": [dict(row) for row in asset_residuals],
        "account_residuals": [dict(row) for row in account_residuals],
        **values,
        "idempotency_conflict_scope": idempotency_conflict_scope,
        "idempotency_key_sha256": idempotency_key_sha256,
        "original_object_id": original_object_id,
        "conflicting_body_sha256": conflicting_body_sha256,
        "integrity_validation_attempt_key_sha256": None,
        "integrity_transition_key_sha256": None,
        "integrity_expected_footprint_sha256": None,
        "integrity_observed_footprint_sha256": None,
        "integrity_ledger_head_before_check_id": None,
        "arithmetic_range_key_sha256": arithmetic_range_key_sha256,
        "arithmetic_operands_sha256": arithmetic_operands_sha256,
        "arithmetic_operation": arithmetic_operation,
        "kill_latched": status == "KILLED",
    }
    return _seal_record(cast(Mapping[str, JsonValue], receipt), "trading.reconciliation-receipt/v1")


def _safe_content_or_digest(record: bytes | None, fallback: bytes) -> str:
    document = _best_effort_doc(record)
    try:
        return _content_id(document)
    except ValueError:
        return sha256_hex(record or fallback)


def _invariant_receipt(
    verified: ContractVerifiedRunInputs,
    *,
    before_record: bytes | None,
    after_record: bytes,
    cause_record: bytes | None,
) -> bytes:
    run = _require_verified_run(verified)
    before_doc = _best_effort_doc(before_record)
    after_doc = _best_effort_doc(after_record)
    state_id = cast(str, before_doc.get("portfolio_state_id") or after_doc.get("portfolio_state_id") or run["run_receipt_id"])
    cause = _safe_content_or_digest(cause_record, after_record)
    return _receipt_record(
        status="KILLED",
        kind="GROUP_GATE",
        decision_sequence=cast(str | None, after_doc.get("state_sequence") or before_doc.get("state_sequence")),
        ingest_sequence=cast(str, after_doc.get("as_of_ingest_sequence") or before_doc.get("as_of_ingest_sequence") or "0"),
        equal_time_group=cast(str, after_doc.get("equal_time_group") or before_doc.get("equal_time_group") or "0"),
        replay_clock_ns=cast(str, after_doc.get("replay_clock_ns") or before_doc.get("replay_clock_ns") or "0"),
        before_id=state_id,
        after_id=state_id,
        causation_ids=[cause],
        reason_codes=["RECONCILIATION_ABSOLUTE_STATE_INVARIANT", "RECONCILIATION_MISMATCH"],
    )


def _balance_row(state: Mapping[str, Any], mint: str) -> Mapping[str, Any]:
    for row in cast(list[Mapping[str, Any]], state.get("balances", [])):
        if row.get("mint") == mint:
            return row
    return {"available_atoms": "0", "reserved_atoms": "0", "total_atoms": "0", "decimals": 0}


def _summary_row(state: Mapping[str, Any]) -> Mapping[str, Any]:
    return cast(Mapping[str, Any], state.get("summary", {}))


def _residual(expected: object, observed: object, *, field: str) -> int:
    return _i128(observed, field=f"observed.{field}") - _i128(expected, field=f"expected.{field}")


def _account_residuals_from_states(
    expected: Mapping[str, Any],
    observed: Mapping[str, Any],
) -> list[dict[str, JsonValue]]:
    quote_mint = cast(str, expected.get("quote_mint") or observed.get("quote_mint"))
    mints = sorted(
        {
            *[cast(str, row["mint"]) for row in cast(list[Mapping[str, Any]], expected.get("balances", []))],
            *[cast(str, row["mint"]) for row in cast(list[Mapping[str, Any]], observed.get("balances", []))],
        },
        key=lambda value: value.encode("utf-8"),
    )
    rows: list[dict[str, JsonValue]] = []
    for mint in mints:
        expected_row = _balance_row(expected, mint)
        observed_row = _balance_row(observed, mint)
        available = _residual(expected_row["available_atoms"], observed_row["available_atoms"], field=f"{mint}.available")
        reserved = _residual(expected_row["reserved_atoms"], observed_row["reserved_atoms"], field=f"{mint}.reserved")
        available_account = "CASH_AVAILABLE" if mint == quote_mint else "POSITION_AVAILABLE"
        reserved_account = "CASH_RESERVED" if mint == quote_mint else "POSITION_RESERVED"
        if available:
            rows.append({"asset_mint": mint, "account": available_account, "residual_atoms": str(available)})
        if reserved:
            rows.append({"asset_mint": mint, "account": reserved_account, "residual_atoms": str(reserved)})
    return sorted(rows, key=lambda row: (str(row["asset_mint"]).encode("utf-8"), str(row["account"]).encode("utf-8")))


def _asset_residuals_from_states(
    expected: Mapping[str, Any],
    observed: Mapping[str, Any],
) -> list[dict[str, JsonValue]]:
    mints = sorted(
        {
            *[cast(str, row["mint"]) for row in cast(list[Mapping[str, Any]], expected.get("balances", []))],
            *[cast(str, row["mint"]) for row in cast(list[Mapping[str, Any]], observed.get("balances", []))],
        },
        key=lambda value: value.encode("utf-8"),
    )
    rows: list[dict[str, JsonValue]] = []
    for mint in mints:
        difference = _residual(_balance_row(expected, mint)["total_atoms"], _balance_row(observed, mint)["total_atoms"], field=f"{mint}.total")
        if difference:
            rows.append({"asset_mint": mint, "residual_atoms": str(difference)})
    return rows


def _ledger_account_residuals(
    expected: Mapping[str, Any] | None,
    observed: Mapping[str, Any] | None,
) -> list[dict[str, JsonValue]]:
    if expected is None or observed is None:
        return []
    allowed = {"CASH_AVAILABLE", "CASH_RESERVED", "POSITION_AVAILABLE", "POSITION_RESERVED"}

    def aggregate(ledger: Mapping[str, Any]) -> dict[tuple[str, str], int]:
        values: dict[tuple[str, str], int] = {}
        for row in cast(list[Mapping[str, Any]], ledger["entries"]):
            account = cast(str, row["account"])
            if account not in allowed:
                continue
            key = (cast(str, row["asset_mint"]), account)
            values[key] = values.get(key, 0) + _i128(row["amount_atoms"], field=f"ledger.{account}")
        return values

    expected_values = aggregate(expected)
    observed_values = aggregate(observed)
    rows: list[dict[str, JsonValue]] = []
    for mint, account in sorted(
        set(expected_values) | set(observed_values),
        key=lambda item: (item[0].encode("utf-8"), item[1].encode("utf-8")),
    ):
        difference = observed_values.get((mint, account), 0) - expected_values.get((mint, account), 0)
        if difference:
            rows.append({"asset_mint": mint, "account": account, "residual_atoms": str(difference)})
    return rows


def _projection_mismatch_receipt(
    verified: ContractVerifiedRunInputs,
    *,
    kind: str,
    before_record: bytes,
    observed_after_record: bytes,
    expected_after_record: bytes | None,
    observed_ledger_record: bytes | None,
    expected_ledger_record: bytes | None,
    cause_record: bytes | None,
) -> bytes:
    before = _parse_record(before_record, "trading.portfolio-state/v1")
    observed_after = _parse_record(observed_after_record, "trading.portfolio-state/v1")
    expected_after = None
    if expected_after_record is not None:
        expected_after = _parse_record(expected_after_record, "trading.portfolio-state/v1")
    observed_ledger = None if observed_ledger_record is None else _parse_record(observed_ledger_record, "trading.ledger-record/v1")
    expected_ledger = None if expected_ledger_record is None else _parse_record(expected_ledger_record, "trading.ledger-record/v1")

    account_residuals = (
        _ledger_account_residuals(expected_ledger, observed_ledger)
        if expected_after is not None and observed_after_record == expected_after_record and expected_ledger_record != observed_ledger_record
        else _account_residuals_from_states(expected_after or before, observed_after)
    )
    asset_residuals = [] if expected_after is None else _asset_residuals_from_states(expected_after, observed_after)
    residuals = dict(_ZERO_FIELDS)
    reason_codes: list[str] = []

    if account_residuals:
        reason_codes.append("RECONCILIATION_ACCOUNT_RESIDUAL")
    if asset_residuals:
        reason_codes.append("RECONCILIATION_ASSET_RESIDUAL")
    if expected_after is not None:
        expected_summary = _summary_row(expected_after)
        observed_summary = _summary_row(observed_after)
        realized = _residual(
            expected_summary["realized_pnl_quote_atoms"],
            observed_summary["realized_pnl_quote_atoms"],
            field="summary.realized_pnl_quote_atoms",
        )
        unrealized = _residual(
            expected_summary["unrealized_pnl_quote_atoms"],
            observed_summary["unrealized_pnl_quote_atoms"],
            field="summary.unrealized_pnl_quote_atoms",
        )
        fee = _residual(
            expected_summary["cumulative_fees_quote_atoms"],
            observed_summary["cumulative_fees_quote_atoms"],
            field="summary.cumulative_fees_quote_atoms",
        )
        peak = _residual(
            expected_summary["peak_equity_quote_atoms"],
            observed_summary["peak_equity_quote_atoms"],
            field="summary.peak_equity_quote_atoms",
        )
        drawdown = int(observed_summary["drawdown_bps"]) - int(expected_summary["drawdown_bps"])
        equity = _residual(
            expected_summary["equity_quote_atoms"],
            observed_summary["equity_quote_atoms"],
            field="summary.equity_quote_atoms",
        )
        if realized or unrealized:
            reason_codes.append("RECONCILIATION_PNL_RESIDUAL")
            residuals["realized_pnl_residual_quote_atoms"] = str(realized)
            residuals["unrealized_pnl_residual_quote_atoms"] = str(unrealized)
        if fee:
            reason_codes.append("RECONCILIATION_FEE_RESIDUAL")
            residuals["fee_residual_quote_atoms"] = str(fee)
        if peak or drawdown or equity:
            reason_codes.append("RECONCILIATION_EQUITY_RESIDUAL")
            residuals["peak_equity_residual_quote_atoms"] = str(peak)
            residuals["drawdown_residual_bps"] = str(drawdown)
            residuals["equity_residual_quote_atoms"] = str(equity)
        expected_open = set(cast(list[str], expected_after.get("open_intent_ids", [])))
        observed_open = set(cast(list[str], observed_after.get("open_intent_ids", [])))
        unmatched = len(expected_open.symmetric_difference(observed_open))
        if unmatched:
            reason_codes.append("RECONCILIATION_RESERVATION_RESIDUAL")
            residuals["unmatched_reservation_count"] = str(unmatched)

    if not reason_codes:
        return _invariant_receipt(
            verified,
            before_record=before_record,
            after_record=observed_after_record,
            cause_record=cause_record or observed_ledger_record,
        )

    reason_codes.append("RECONCILIATION_MISMATCH")
    cause = _safe_content_or_digest(cause_record or observed_ledger_record, observed_after_record)
    return _receipt_record(
        status="KILLED",
        kind=kind,
        decision_sequence=cast(str | None, observed_after.get("state_sequence")),
        ingest_sequence=cast(str, observed_after["as_of_ingest_sequence"]),
        equal_time_group=cast(str, observed_after["equal_time_group"]),
        replay_clock_ns=cast(str, observed_after["replay_clock_ns"]),
        before_id=cast(str, before["portfolio_state_id"]),
        after_id=cast(str, observed_after["portfolio_state_id"]),
        causation_ids=[cause],
        reason_codes=reason_codes,
        asset_residuals=asset_residuals,
        account_residuals=account_residuals,
        residuals=residuals,
    )


def _mismatch_receipt(
    verified: ContractVerifiedRunInputs,
    *,
    kind: str,
    before_record: bytes | None,
    after_record: bytes,
    cause_record: bytes | None,
) -> bytes:
    return _invariant_receipt(
        verified,
        before_record=before_record,
        after_record=after_record,
        cause_record=cause_record,
    )


def _fill_idempotency_key(intent_id: str) -> str:
    key = {"schema": "trading.fill-idempotency-key/v1", "intent_id": intent_id}
    require_valid_contract(key, expected_schema="trading.fill-idempotency-key/v1")
    return sha256_hex(canonical_json_bytes(key))


def _duplicate_fill_receipt(
    verified: ContractVerifiedRunInputs,
    *,
    state_record: bytes,
    fill_record: bytes,
    existing_ledger: Mapping[str, Any],
) -> bytes:
    state = _parse_record(state_record, "trading.portfolio-state/v1")
    fill = _parse_record(fill_record, "trading.simulated-fill-receipt/v1")
    fill_id = cast(str, fill["fill_receipt_id"])
    conflicting = sha256_hex(fill_record)
    before_id = cast(str, state["portfolio_state_id"])
    return _receipt_record(
        status="KILLED",
        kind="IDEMPOTENCY_CONFLICT",
        decision_sequence=cast(str, fill["decision_sequence"]),
        ingest_sequence=cast(str, state["as_of_ingest_sequence"]),
        equal_time_group=cast(str, state["equal_time_group"]),
        replay_clock_ns=cast(str, state["replay_clock_ns"]),
        before_id=before_id,
        after_id=before_id,
        causation_ids=[before_id, fill_id, conflicting],
        reason_codes=["RECONCILIATION_IDEMPOTENCY_CONFLICT", "RECONCILIATION_MISMATCH"],
        idempotency_conflict_scope="FILL",
        idempotency_key_sha256=_fill_idempotency_key(cast(str, fill["intent_id"])),
        original_object_id=cast(str, existing_ledger["object_id"]),
        conflicting_body_sha256=conflicting,
    )


def _arithmetic_receipt(
    verified: ContractVerifiedRunInputs,
    *,
    before_record: bytes,
    coordinates: Mapping[str, Any],
    operation: str,
    operands: Sequence[tuple[str, int]],
) -> bytes:
    run = _require_verified_run(verified)
    before = _parse_record(before_record, "trading.portfolio-state/v1")
    operands_document = {
        "schema": "trading.reconciliation-arithmetic-operands/v1",
        "arithmetic_operation": operation,
        "operands": [{"operand_name": name, "operand_value": str(value)} for name, value in operands],
    }
    require_valid_contract(operands_document, expected_schema="trading.reconciliation-arithmetic-operands/v1")
    operands_digest = sha256_hex(canonical_json_bytes(operands_document))
    key = {
        "schema": "trading.reconciliation-arithmetic-range-key/v1",
        "run_receipt_id": run["run_receipt_id"],
        "portfolio_state_before_id": before["portfolio_state_id"],
        "ingest_sequence": coordinates.get("ingest_sequence") or before["as_of_ingest_sequence"],
        "equal_time_group": coordinates.get("equal_time_group") or before["equal_time_group"],
        "replay_clock_ns": coordinates.get("replay_clock_ns") or before["replay_clock_ns"],
        "arithmetic_operation": operation,
        "arithmetic_operands_sha256": operands_digest,
    }
    require_valid_contract(key, expected_schema="trading.reconciliation-arithmetic-range-key/v1")
    key_digest = sha256_hex(canonical_json_bytes(key))
    before_id = cast(str, before["portfolio_state_id"])
    return _receipt_record(
        status="KILLED",
        kind="ARITHMETIC_RANGE",
        decision_sequence=cast(str | None, coordinates.get("decision_sequence")),
        ingest_sequence=cast(str, key["ingest_sequence"]),
        equal_time_group=cast(str, key["equal_time_group"]),
        replay_clock_ns=cast(str, key["replay_clock_ns"]),
        before_id=before_id,
        after_id=before_id,
        causation_ids=[key_digest],
        reason_codes=["RECONCILIATION_ARITHMETIC_RANGE", "RECONCILIATION_MISMATCH"],
        arithmetic_range_key_sha256=key_digest,
        arithmetic_operands_sha256=operands_digest,
        arithmetic_operation=operation,
    )


def _records_with_schema(records: Sequence[bytes], schema: str) -> list[tuple[bytes, dict[str, Any]]]:
    found: list[tuple[bytes, dict[str, Any]]] = []
    for record in records:
        try:
            document = cast(dict[str, Any], parse_canonical_record(bytes(record)))
        except ValueError:
            continue
        if document.get("schema") == schema:
            try:
                found.append((bytes(record), _parse_record(bytes(record), schema)))
            except ValueError as error:
                raise _ReconciliationMismatch(f"{schema} retained evidence failed canonical validation") from error
    return found


def _existing_fill_for_intent(records: Sequence[bytes], intent_id: str) -> dict[str, Any] | None:
    for _record, ledger in _records_with_schema(records, "trading.ledger-record/v1"):
        if ledger.get("record_type") == "SIMULATED_FILL" and intent_id in cast(list[str], ledger.get("causation_ids", [])):
            return ledger
    return None


def _prior_ledgers(records: Sequence[bytes]) -> list[dict[str, Any]]:
    return [ledger for _record, ledger in _records_with_schema(records, "trading.ledger-record/v1")]


def _require_run_state_bindings(run: Mapping[str, object], state: Mapping[str, Any]) -> None:
    if state["fixture_manifest_sha256"] != run["fixture_manifest_sha256"]:
        raise _ReconciliationMismatch("portfolio state fixture provenance does not bind verified run")
    if state["config_admission_receipt_id"] != run["config_admission_receipt_id"]:
        raise _ReconciliationMismatch("portfolio state config provenance does not bind verified run")
    if state["validated_config_sha256"] != run["validated_config_sha256"]:
        raise _ReconciliationMismatch("portfolio state validated config does not bind verified run")


def _quote_balance(state: Mapping[str, Any]) -> Mapping[str, Any]:
    quote_mint = cast(str, state["quote_mint"])
    for row in cast(list[Mapping[str, Any]], state["balances"]):
        if row["mint"] == quote_mint:
            return row
    raise _ReconciliationMismatch("portfolio state has no quote balance")


def _require_genesis_invariants(
    run: Mapping[str, object],
    *,
    before_record: bytes | None,
    ledger_record: bytes | None,
    causation_records: Sequence[bytes],
    after: Mapping[str, Any],
) -> None:
    if before_record is not None or ledger_record is not None or causation_records:
        raise _ReconciliationMismatch("genesis reconciliation has forbidden before, ledger, or causation evidence")
    _require_run_state_bindings(run, after)
    for field in ("state_sequence", "as_of_ingest_sequence", "equal_time_group", "replay_clock_ns"):
        if after[field] != "0":
            raise _ReconciliationMismatch("genesis state coordinates are not all zero")
    if after["previous_portfolio_state_id"] is not None:
        raise _ReconciliationMismatch("genesis state must not have a previous state")
    if after["causation_schema"] != "RUN_INITIALIZATION" or after["causation_id"] != run["run_receipt_id"]:
        raise _ReconciliationMismatch("genesis state does not bind RUN_INITIALIZATION to verified run")
    balances = cast(list[Mapping[str, Any]], after["balances"])
    if len(balances) != 1:
        raise _ReconciliationMismatch("genesis state must have exactly one quote balance")
    quote_balance = balances[0]
    if quote_balance["mint"] != after["quote_mint"] or quote_balance["decimals"] != after["quote_decimals"]:
        raise _ReconciliationMismatch("genesis quote balance does not match quote coordinates")
    if quote_balance["reserved_atoms"] != "0" or quote_balance["available_atoms"] != quote_balance["total_atoms"]:
        raise _ReconciliationMismatch("genesis quote balance is not internally available")
    if after["positions"] or after["open_intent_ids"]:
        raise _ReconciliationMismatch("genesis state cannot contain positions or open intents")
    if after["kill_latched"] is not False or after["kill_reason_codes"]:
        raise _ReconciliationMismatch("genesis state cannot start killed")
    summary = cast(Mapping[str, Any], after["summary"])
    if (
        summary["realized_pnl_quote_atoms"] != "0"
        or summary["unrealized_pnl_quote_atoms"] != "0"
        or summary["cumulative_fees_quote_atoms"] != "0"
        or summary["session_pnl_quote_atoms"] != "0"
        or summary["drawdown_bps"] != 0
    ):
        raise _ReconciliationMismatch("genesis summary must have zero pnl, fees, and drawdown")
    if summary["equity_quote_atoms"] != quote_balance["total_atoms"] or summary["peak_equity_quote_atoms"] != quote_balance["total_atoms"]:
        raise _ReconciliationMismatch("genesis equity and peak must equal starting quote balance")


def _require_intent_bindings(
    run: Mapping[str, object],
    before: Mapping[str, Any],
    intent: Mapping[str, Any],
    *,
    reservation_transition: bool,
) -> None:
    _require_run_state_bindings(run, before)
    before_id = cast(str, before["portfolio_state_id"])
    if reservation_transition and intent["portfolio_state_before_id"] != before_id:
        raise _ReconciliationMismatch("intent does not bind the supplied before-state")
    if intent["config_admission_receipt_id"] != run["config_admission_receipt_id"]:
        raise _ReconciliationMismatch("intent config provenance does not bind verified run")
    if intent["validated_config_sha256"] != run["validated_config_sha256"]:
        raise _ReconciliationMismatch("intent validated config does not bind verified run")
    if intent["quote_mint"] != before["quote_mint"] or intent["quote_decimals"] != before["quote_decimals"]:
        raise _ReconciliationMismatch("intent quote coordinates do not bind before-state quote coordinates")
    quote = _quote_balance(before)
    if quote["decimals"] != intent["quote_decimals"]:
        raise _ReconciliationMismatch("intent quote decimals do not bind quote balance")
    if reservation_transition and intent["intent_id"] in cast(list[str], before["open_intent_ids"]):
        raise _ReconciliationMismatch("intent is already open")
    if intent["action"] == "OPEN_LONG":
        reserved_quote = _u64(intent["reserved_quote_atoms"], field="intent.reserved_quote_atoms", positive=True)
        if intent["reserved_base_atoms"] != "0":
            raise _ReconciliationMismatch("open intent must reserve quote only")
        if reservation_transition and _u64(quote["available_atoms"], field="quote.available_atoms") < reserved_quote:
            raise _ReconciliationMismatch("open intent reserves more quote than available")
        return
    base_mint = cast(str, intent["base_mint"])
    market_id = cast(str, intent["market_id"])
    base_balance = _balance_map(before).get(base_mint)
    position = _position_map(before).get(market_id)
    if base_balance is None or position is None:
        raise _ReconciliationMismatch("close intent does not bind an open base balance and position")
    reserved_base = _u64(intent["reserved_base_atoms"], field="intent.reserved_base_atoms", positive=True)
    if intent["reserved_quote_atoms"] != "0" or intent["quantity_base_atoms"] != intent["reserved_base_atoms"]:
        raise _ReconciliationMismatch("close intent must reserve exactly the requested base quantity")
    if base_balance["decimals"] != intent["base_decimals"] or position["base_decimals"] != intent["base_decimals"]:
        raise _ReconciliationMismatch("close intent base decimals do not bind state coordinates")
    if position["base_mint"] != base_mint:
        raise _ReconciliationMismatch("close intent base mint does not bind position")
    if reservation_transition and _u64(base_balance["available_atoms"], field="base.available_atoms") < reserved_base:
        raise _ReconciliationMismatch("close intent reserves more base than available")
    if _u64(position["quantity_base_atoms"], field="position.quantity_base_atoms") < reserved_base:
        raise _ReconciliationMismatch("close intent reserves more base than position quantity")


def _require_fill_bindings(
    before: Mapping[str, Any],
    intent: Mapping[str, Any],
    fill: Mapping[str, Any],
) -> None:
    intent_id = cast(str, intent["intent_id"])
    if fill["intent_id"] != intent_id:
        raise _ReconciliationMismatch("fill does not bind the supplied intent")
    if fill["portfolio_state_before_id"] != before["portfolio_state_id"]:
        raise _ReconciliationMismatch("fill does not bind the supplied before-state")
    if intent_id not in cast(list[str], before["open_intent_ids"]):
        raise _ReconciliationMismatch("fill intent is not open in before-state")
    for field in ("decision_sequence", "decision_ingest_sequence", "decision_equal_time_group"):
        if fill[field] != intent[field]:
            raise _ReconciliationMismatch(f"fill {field} does not bind intent")
    if fill["requested_base_atoms"] != intent["quantity_base_atoms"]:
        raise _ReconciliationMismatch("fill requested base does not bind intent quantity")
    filled = _u64(fill["filled_base_atoms"], field="fill.filled_base_atoms")
    requested = _u64(intent["quantity_base_atoms"], field="intent.quantity_base_atoms")
    if filled > requested:
        raise _ReconciliationMismatch("fill exceeds requested intent quantity")
    if intent["action"] == "OPEN_LONG":
        if fill["released_base_atoms"] != "0":
            raise _ReconciliationMismatch("open fill cannot release base reservation")
        if _u64(fill["released_quote_atoms"], field="fill.released_quote_atoms") > _u64(
            intent["reserved_quote_atoms"], field="intent.reserved_quote_atoms"
        ):
            raise _ReconciliationMismatch("open fill releases more quote than reserved")
        return
    if fill["released_quote_atoms"] != "0":
        raise _ReconciliationMismatch("close fill cannot release quote reservation")
    if _u64(fill["released_base_atoms"], field="fill.released_base_atoms") > _u64(
        intent["reserved_base_atoms"], field="intent.reserved_base_atoms"
    ):
        raise _ReconciliationMismatch("close fill releases more base than reserved")


def reconcile_transition(
    verified: ContractVerifiedRunInputs,
    *,
    kind: Literal["GENESIS", "INTENT_RESERVATION", "FILL_TRANSITION"],
    portfolio_state_before_record: bytes | None,
    portfolio_state_after_record: bytes,
    ledger_record: bytes | None,
    causation_records: Sequence[bytes],
) -> bytes:
    """Recompute one accounting transition from retained records and emit a frozen receipt."""
    run = _require_verified_run(verified)
    cause_records = tuple(bytes(record) for record in causation_records)
    after_record = bytes(portfolio_state_after_record)
    try:
        after = _parse_record(after_record, "trading.portfolio-state/v1")
    except ValueError:
        return _mismatch_receipt(
            verified,
            kind=kind,
            before_record=portfolio_state_before_record,
            after_record=after_record,
            cause_record=ledger_record,
        )

    if kind == "GENESIS":
        try:
            _require_genesis_invariants(
                run,
                before_record=portfolio_state_before_record,
                ledger_record=ledger_record,
                causation_records=cause_records,
                after=after,
            )
        except (ValueError, _ReconciliationMismatch):
            return _mismatch_receipt(
                verified,
                kind=kind,
                before_record=portfolio_state_before_record,
                after_record=after_record,
                cause_record=ledger_record or (cause_records[-1] if cause_records else None),
            )
        state_id = cast(str, after["portfolio_state_id"])
        return _receipt_record(
            status="PASS",
            kind="GENESIS",
            decision_sequence=None,
            ingest_sequence="0",
            equal_time_group="0",
            replay_clock_ns="0",
            before_id=state_id,
            after_id=state_id,
            causation_ids=[cast(str, run["run_receipt_id"])],
        )

    if portfolio_state_before_record is None:
        return _mismatch_receipt(verified, kind=kind, before_record=None, after_record=after_record, cause_record=ledger_record)
    before_record = bytes(portfolio_state_before_record)
    try:
        before = _parse_record(before_record, "trading.portfolio-state/v1")
    except ValueError:
        return _mismatch_receipt(
            verified,
            kind=kind,
            before_record=before_record,
            after_record=after_record,
            cause_record=ledger_record,
        )

    try:
        prior_ledgers = _prior_ledgers(cause_records)
        if kind == "INTENT_RESERVATION":
            intent_records = _records_with_schema(cause_records, "trading.simulated-order-intent/v1")
            if len(intent_records) != 1 or ledger_record is None:
                raise _ReconciliationMismatch("intent reservation requires one intent and one ledger record")
            intent_record, intent = intent_records[0]
            _require_intent_bindings(run, before, intent, reservation_transition=True)
            _parse_record(bytes(ledger_record), "trading.ledger-record/v1")
            expected_after = _expected_intent_state(before, intent)
            if expected_after != after_record:
                return _projection_mismatch_receipt(
                    verified,
                    kind="INTENT_RESERVATION",
                    before_record=before_record,
                    observed_after_record=after_record,
                    expected_after_record=expected_after,
                    observed_ledger_record=ledger_record,
                    expected_ledger_record=None,
                    cause_record=intent_record,
                )
            expected_after_doc = _parse_record(expected_after, "trading.portfolio-state/v1")
            expected_ledger = _expected_ledger(
                verified=verified,
                prior_ledgers=prior_ledgers,
                record_type="SIMULATED_INTENT",
                object_document=intent,
                portfolio_state_id=cast(str, expected_after_doc["portfolio_state_id"]),
                causation_ids=[cast(str, intent["risk_decision_id"]), cast(str, before["portfolio_state_id"])],
                entries=_intent_postings(intent),
            )
            if expected_ledger != bytes(ledger_record):
                return _projection_mismatch_receipt(
                    verified,
                    kind="INTENT_RESERVATION",
                    before_record=before_record,
                    observed_after_record=after_record,
                    expected_after_record=expected_after,
                    observed_ledger_record=ledger_record,
                    expected_ledger_record=expected_ledger,
                    cause_record=bytes(ledger_record),
                )
            ledger = _parse_record(expected_ledger, "trading.ledger-record/v1")
            return _receipt_record(
                status="PASS",
                kind="INTENT_RESERVATION",
                decision_sequence=cast(str, intent["decision_sequence"]),
                ingest_sequence=cast(str, intent["decision_ingest_sequence"]),
                equal_time_group=cast(str, intent["decision_equal_time_group"]),
                replay_clock_ns=cast(str, intent["created_replay_clock_ns"]),
                before_id=cast(str, before["portfolio_state_id"]),
                after_id=cast(str, after["portfolio_state_id"]),
                causation_ids=[cast(str, ledger["ledger_record_id"])],
            )

        intent_records = _records_with_schema(cause_records, "trading.simulated-order-intent/v1")
        fill_records = _records_with_schema(cause_records, "trading.simulated-fill-receipt/v1")
        if len(intent_records) != 1 or len(fill_records) != 1:
            raise _ReconciliationMismatch("fill reconciliation requires one intent and one fill")
        fill_record, fill = fill_records[0]
        existing = _existing_fill_for_intent(cause_records, cast(str, fill["intent_id"]))
        if existing is not None and ledger_record is None:
            return _duplicate_fill_receipt(verified, state_record=after_record, fill_record=fill_record, existing_ledger=existing)
        intent_record, intent = intent_records[0]
        _require_intent_bindings(run, before, intent, reservation_transition=False)
        _require_fill_bindings(before, intent, fill)
        expected_after = _expected_fill_state(before, intent, fill)
        if expected_after != after_record:
            return _projection_mismatch_receipt(
                verified,
                kind="FILL_TRANSITION",
                before_record=before_record,
                observed_after_record=after_record,
                expected_after_record=expected_after,
                observed_ledger_record=ledger_record,
                expected_ledger_record=None,
                cause_record=fill_record,
            )
        if ledger_record is None:
            raise _ReconciliationMismatch("successful fill reconciliation requires a ledger record")
        _parse_record(bytes(ledger_record), "trading.ledger-record/v1")
        expected_after_doc = _parse_record(expected_after, "trading.portfolio-state/v1")
        expected_ledger = _expected_ledger(
            verified=verified,
            prior_ledgers=prior_ledgers,
            record_type="SIMULATED_FILL",
            object_document=fill,
            portfolio_state_id=cast(str, expected_after_doc["portfolio_state_id"]),
            causation_ids=[
                *([] if fill["fill_event_id"] is None else [cast(str, fill["fill_event_id"])]),
                cast(str, intent["intent_id"]),
                cast(str, before["portfolio_state_id"]),
            ],
            entries=_fill_postings(before, expected_after_doc, intent, fill),
        )
        if expected_ledger != bytes(ledger_record):
            return _projection_mismatch_receipt(
                verified,
                kind="FILL_TRANSITION",
                before_record=before_record,
                observed_after_record=after_record,
                expected_after_record=expected_after,
                observed_ledger_record=ledger_record,
                expected_ledger_record=expected_ledger,
                cause_record=bytes(ledger_record),
            )
        ledger = _parse_record(expected_ledger, "trading.ledger-record/v1")
        return _receipt_record(
            status="PASS",
            kind="FILL_TRANSITION",
            decision_sequence=cast(str, fill["decision_sequence"]),
            ingest_sequence=cast(str, fill["fill_ingest_sequence"] or fill["decision_ingest_sequence"]),
            equal_time_group=cast(str, fill["fill_equal_time_group"] or fill["decision_equal_time_group"]),
            replay_clock_ns=cast(str, fill["fill_replay_clock_ns"] or before["replay_clock_ns"]),
            before_id=cast(str, before["portfolio_state_id"]),
            after_id=cast(str, after["portfolio_state_id"]),
            causation_ids=[cast(str, ledger["ledger_record_id"])],
        )
    except _ArithmeticRange as error:
        coordinates = {
            "decision_sequence": error.coordinates.get("decision_sequence") or after.get("state_sequence"),
            "ingest_sequence": after.get("as_of_ingest_sequence") or before.get("as_of_ingest_sequence"),
            "equal_time_group": after.get("equal_time_group") or before.get("equal_time_group"),
            "replay_clock_ns": after.get("replay_clock_ns") or before.get("replay_clock_ns"),
        }
        coordinates.update(error.coordinates)
        operands = error.operands or (
            (
                "before_quote_total",
                _u64(cast(Mapping[str, Any], _balance_map(before)[before["quote_mint"]])["total_atoms"], field="quote.total"),
            ),
            ("after_market_count", len(cast(list[Any], after["positions"]))),
        )
        return _arithmetic_receipt(
            verified,
            before_record=before_record,
            coordinates=coordinates,
            operation=error.operation,
            operands=operands,
        )
    except (StopIteration, ValueError, _ReconciliationMismatch):
        return _mismatch_receipt(
            verified,
            kind=kind,
            before_record=before_record,
            after_record=after_record,
            cause_record=ledger_record or (cause_records[-1] if cause_records else None),
        )
