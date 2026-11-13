"""Hash-chained in-memory accounting for the offline G2 paper kernel."""

from __future__ import annotations

import copy
from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from typing import Any, cast

from build_finance.crypto_replay.canonical import JsonValue, canonical_record_bytes, parse_canonical_record
from build_finance.crypto_replay.content_ids import seal_content_id, verify_content_id
from build_finance.crypto_replay.run_inputs import ContractVerifiedRunInputs
from build_finance.crypto_replay.schema_registry import require_valid_contract
from build_finance.live_paper.event_store import InMemoryLedgerStore, append_ledger_record
from build_finance.live_paper.reconciliation import reconcile_transition

_MAX_U64 = 18_446_744_073_709_551_615
_MIN_I128 = -170_141_183_460_469_231_731_687_303_715_884_105_728
_MAX_I128 = 170_141_183_460_469_231_731_687_303_715_884_105_727


@dataclass(frozen=True, slots=True)
class AccountingTransition:
    store: InMemoryLedgerStore
    portfolio_state_record: bytes
    ledger_record: bytes | None
    reconciliation_receipt_record: bytes

    def __post_init__(self) -> None:
        object.__setattr__(self, "portfolio_state_record", bytes(self.portfolio_state_record))
        ledger = None if self.ledger_record is None else bytes(self.ledger_record)
        object.__setattr__(self, "ledger_record", ledger)
        object.__setattr__(self, "reconciliation_receipt_record", bytes(self.reconciliation_receipt_record))


class _ArithmeticRange(ValueError):
    """Raised when accounting arithmetic exceeds frozen bounds."""


def _require_verified_run(verified: ContractVerifiedRunInputs) -> Mapping[str, object]:
    if not isinstance(verified, ContractVerifiedRunInputs) or verified.authority != "CONTRACT_ONLY":
        raise ValueError("accounting requires frozen contract-verified run inputs")
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


def _seal_record(document: Mapping[str, JsonValue], expected_schema: str) -> bytes:
    sealed = seal_content_id(document)
    require_valid_contract(sealed, expected_schema=expected_schema)
    return canonical_record_bytes(sealed)


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


def _checked_u64(value: int) -> int:
    if value < 0 or value > _MAX_U64:
        raise _ArithmeticRange("u64 arithmetic range exceeded")
    return value


def _checked_i128(value: int) -> int:
    if value < _MIN_I128 or value > _MAX_I128:
        raise _ArithmeticRange("i128 arithmetic range exceeded")
    return value


def _ceil_div(numerator: int, denominator: int) -> int:
    if numerator < 0 or denominator <= 0:
        raise _ArithmeticRange("ceiling division requires a non-negative numerator and positive denominator")
    return (numerator + denominator - 1) // denominator


def _sorted_ids(values: Sequence[str]) -> list[str]:
    return sorted(dict.fromkeys(values), key=lambda value: value.encode("utf-8"))


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
    row["available_atoms"] = str(_checked_u64(available))
    row["reserved_atoms"] = str(_checked_u64(reserved))
    row["total_atoms"] = str(_checked_u64(available + reserved))


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
    return {
        "realized_pnl_quote_atoms": str(_checked_i128(realized)),
        "unrealized_pnl_quote_atoms": str(_checked_i128(unrealized_total)),
        "cumulative_fees_quote_atoms": str(_checked_u64(cumulative_fees)),
        "session_pnl_quote_atoms": str(_checked_i128(realized + unrealized_total)),
        "equity_quote_atoms": str(equity),
        "peak_equity_quote_atoms": str(_checked_u64(peak)),
        "drawdown_bps": drawdown,
    }


def _zero_ledger_reconciliation(portfolio_state_id: str) -> dict[str, JsonValue]:
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


def _posting_rows(rows: Sequence[tuple[str, str, int, int]]) -> list[dict[str, JsonValue]]:
    aggregate: dict[tuple[str, str], tuple[int, int]] = {}
    for mint, account, decimals, amount in rows:
        if amount == 0:
            continue
        key = (mint, account)
        prior = aggregate.get(key)
        if prior is not None and prior[0] != decimals:
            raise ValueError("posting decimals disagree")
        aggregate[key] = (decimals, _checked_i128((0 if prior is None else prior[1]) + amount))
    return [
        {"account": account, "asset_mint": mint, "decimals": decimals, "amount_atoms": str(amount)}
        for (mint, account), (decimals, amount) in sorted(
            aggregate.items(),
            key=lambda item: (item[0][0].encode("utf-8"), item[0][1].encode("utf-8")),
        )
        if amount != 0
    ]


def _ledger_head(store: InMemoryLedgerStore) -> str | None:
    if not store.ledger_records:
        return None
    return cast(str, _parse_record(store.ledger_records[-1], "trading.ledger-record/v1")["ledger_record_id"])


def _ledger_record(
    verified: ContractVerifiedRunInputs,
    store: InMemoryLedgerStore,
    *,
    record_type: str,
    object_document: Mapping[str, Any],
    portfolio_state_id: str,
    causation_ids: Sequence[str],
    entries: Sequence[Mapping[str, JsonValue]],
) -> bytes:
    run = _require_verified_run(verified)
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
    object_id = cast(str, object_document["intent_id"] if record_type == "SIMULATED_INTENT" else object_document["fill_receipt_id"])
    ledger = {
        "schema": "trading.ledger-record/v1",
        "ledger_sequence": str(len(store.ledger_records)),
        "previous_ledger_record_id": _ledger_head(store),
        "run_receipt_id": run["run_receipt_id"],
        "fixture_manifest_sha256": run["fixture_manifest_sha256"],
        "config_admission_receipt_id": run["config_admission_receipt_id"],
        "validated_config_sha256": run["validated_config_sha256"],
        "record_type": record_type,
        "object_schema": object_document["schema"],
        "object_id": object_id,
        "object_sha256": object_id,
        "decision_sequence": decision_sequence,
        "ingest_sequence": ingest_sequence,
        "equal_time_group": equal_time_group,
        "replay_clock_ns": replay_clock_ns,
        "causation_ids": _sorted_ids(causation_ids),
        "entries": [dict(row) for row in entries],
        "reconciliation": _zero_ledger_reconciliation(portfolio_state_id),
    }
    return _seal_record(cast(Mapping[str, JsonValue], ledger), "trading.ledger-record/v1")


def _intent_state_record(before: Mapping[str, Any], intent: Mapping[str, Any]) -> bytes:
    before_id = cast(str, before["portfolio_state_id"])
    quote_mint = cast(str, before["quote_mint"])
    balances = _balance_map(before)
    positions = _position_map(before)
    intent_id = cast(str, intent["intent_id"])
    open_intents = list(cast(list[str], before["open_intent_ids"]))
    if intent_id in open_intents:
        raise ValueError("intent is already open")

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
        base = balances[base_mint]
        position = positions[market_id]
        reserved = _u64(intent["reserved_base_atoms"], field="intent.reserved_base_atoms")
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


def _fees(fill: Mapping[str, Any]) -> int:
    return (
        _u64(fill["venue_fee_quote_atoms"], field="venue_fee_quote_atoms")
        + _u64(fill["priority_fee_quote_atoms"], field="priority_fee_quote_atoms")
        + _u64(fill["simulation_fee_quote_atoms"], field="simulation_fee_quote_atoms")
    )


def _fill_state_record(before: Mapping[str, Any], intent: Mapping[str, Any], fill: Mapping[str, Any]) -> bytes:
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
        base = balances[base_mint]
        reserved_base = _u64(intent["reserved_base_atoms"], field="intent.reserved_base_atoms")
        released_base = _u64(fill["released_base_atoms"], field="fill.released_base_atoms")
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
                raise ValueError("fill exceeds open position quantity")
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
    rows.extend(
        [
            (quote, "FEE_EXPENSE", quote_decimals, _summary_delta(before, after, "cumulative_fees_quote_atoms")),
            (quote, "REALIZED_PNL", quote_decimals, _summary_delta(before, after, "realized_pnl_quote_atoms")),
            (quote, "UNREALIZED_PNL", quote_decimals, _summary_delta(before, after, "unrealized_pnl_quote_atoms")),
        ]
    )
    quote_sum = sum(amount for mint, _account, _decimals, amount in rows if mint == quote)
    rows.append((quote, "EQUITY_CONTROL", quote_decimals, -quote_sum))
    return _posting_rows(rows)


def _receipt_status(record: bytes) -> str:
    return cast(str, _parse_record(record, "trading.reconciliation-receipt/v1")["status"])


def _existing_fill_for_intent(store: InMemoryLedgerStore, intent_id: str) -> Mapping[str, Any] | None:
    for record in store.ledger_records:
        ledger = _parse_record(record, "trading.ledger-record/v1")
        if ledger["record_type"] == "SIMULATED_FILL" and intent_id in cast(list[str], ledger["causation_ids"]):
            return ledger
    return None


def initialize_accounting(
    verified: ContractVerifiedRunInputs,
    *,
    quote_mint: str,
    quote_decimals: int,
    starting_quote_atoms: int,
) -> AccountingTransition:
    run = _require_verified_run(verified)
    if type(starting_quote_atoms) is not int:
        raise ValueError("starting quote atoms must be an integer amount")
    starting = _checked_u64(starting_quote_atoms)
    state = {
        "schema": "trading.portfolio-state/v1",
        "state_sequence": "0",
        "as_of_ingest_sequence": "0",
        "equal_time_group": "0",
        "replay_clock_ns": "0",
        "previous_portfolio_state_id": None,
        "causation_schema": "RUN_INITIALIZATION",
        "causation_id": run["run_receipt_id"],
        "fixture_manifest_sha256": run["fixture_manifest_sha256"],
        "config_admission_receipt_id": run["config_admission_receipt_id"],
        "validated_config_sha256": run["validated_config_sha256"],
        "quote_mint": quote_mint,
        "quote_decimals": quote_decimals,
        "balances": [
            {
                "mint": quote_mint,
                "decimals": quote_decimals,
                "available_atoms": str(starting),
                "reserved_atoms": "0",
                "total_atoms": str(starting),
            }
        ],
        "positions": [],
        "summary": {
            "realized_pnl_quote_atoms": "0",
            "unrealized_pnl_quote_atoms": "0",
            "cumulative_fees_quote_atoms": "0",
            "session_pnl_quote_atoms": "0",
            "equity_quote_atoms": str(starting),
            "peak_equity_quote_atoms": str(starting),
            "drawdown_bps": 0,
        },
        "kill_latched": False,
        "kill_reason_codes": [],
        "open_intent_ids": [],
    }
    state_record = _seal_record(cast(Mapping[str, JsonValue], state), "trading.portfolio-state/v1")
    receipt_record = reconcile_transition(
        verified,
        kind="GENESIS",
        portfolio_state_before_record=None,
        portfolio_state_after_record=state_record,
        ledger_record=None,
        causation_records=(),
    )
    return AccountingTransition(
        store=InMemoryLedgerStore(()),
        portfolio_state_record=state_record,
        ledger_record=None,
        reconciliation_receipt_record=receipt_record,
    )


def reserve_intent(
    verified: ContractVerifiedRunInputs,
    store: InMemoryLedgerStore,
    portfolio_state_record: bytes,
    intent_record: bytes,
) -> AccountingTransition:
    before = _parse_record(bytes(portfolio_state_record), "trading.portfolio-state/v1")
    intent = _parse_record(bytes(intent_record), "trading.simulated-order-intent/v1")
    if intent["portfolio_state_before_id"] != before["portfolio_state_id"]:
        raise ValueError("intent does not bind the supplied portfolio state")
    after_record = _intent_state_record(before, intent)
    after = _parse_record(after_record, "trading.portfolio-state/v1")
    ledger_record = _ledger_record(
        verified,
        store,
        record_type="SIMULATED_INTENT",
        object_document=intent,
        portfolio_state_id=cast(str, after["portfolio_state_id"]),
        causation_ids=[cast(str, intent["risk_decision_id"]), cast(str, before["portfolio_state_id"])],
        entries=_intent_postings(intent),
    )
    receipt_record = reconcile_transition(
        verified,
        kind="INTENT_RESERVATION",
        portfolio_state_before_record=bytes(portfolio_state_record),
        portfolio_state_after_record=after_record,
        ledger_record=ledger_record,
        causation_records=(bytes(intent_record), *store.ledger_records),
    )
    if _receipt_status(receipt_record) != "PASS":
        return AccountingTransition(store=store, portfolio_state_record=bytes(portfolio_state_record), ledger_record=None, reconciliation_receipt_record=receipt_record)
    return AccountingTransition(
        store=append_ledger_record(store, ledger_record),
        portfolio_state_record=after_record,
        ledger_record=ledger_record,
        reconciliation_receipt_record=receipt_record,
    )


def apply_fill_receipt(
    verified: ContractVerifiedRunInputs,
    store: InMemoryLedgerStore,
    portfolio_state_record: bytes,
    intent_record: bytes,
    fill_receipt_record: bytes,
) -> AccountingTransition:
    before_record = bytes(portfolio_state_record)
    before = _parse_record(before_record, "trading.portfolio-state/v1")
    intent = _parse_record(bytes(intent_record), "trading.simulated-order-intent/v1")
    fill = _parse_record(bytes(fill_receipt_record), "trading.simulated-fill-receipt/v1")
    if fill["intent_id"] != intent["intent_id"]:
        raise ValueError("fill receipt does not bind the supplied intent")
    existing = _existing_fill_for_intent(store, cast(str, intent["intent_id"]))
    if existing is not None:
        receipt_record = reconcile_transition(
            verified,
            kind="FILL_TRANSITION",
            portfolio_state_before_record=before_record,
            portfolio_state_after_record=before_record,
            ledger_record=None,
            causation_records=(bytes(intent_record), bytes(fill_receipt_record), *store.ledger_records),
        )
        return AccountingTransition(
            store=store,
            portfolio_state_record=before_record,
            ledger_record=None,
            reconciliation_receipt_record=receipt_record,
        )
    if fill["portfolio_state_before_id"] != before["portfolio_state_id"]:
        raise ValueError("fill receipt does not bind the supplied before-fill portfolio state")
    if intent["intent_id"] not in cast(list[str], before["open_intent_ids"]):
        raise ValueError("fill intent is not open in the supplied portfolio state")

    try:
        after_record = _fill_state_record(before, intent, fill)
    except _ArithmeticRange:
        receipt_record = reconcile_transition(
            verified,
            kind="FILL_TRANSITION",
            portfolio_state_before_record=before_record,
            portfolio_state_after_record=before_record,
            ledger_record=None,
            causation_records=(bytes(intent_record), bytes(fill_receipt_record), *store.ledger_records),
        )
        return AccountingTransition(
            store=store,
            portfolio_state_record=before_record,
            ledger_record=None,
            reconciliation_receipt_record=receipt_record,
        )
    after = _parse_record(after_record, "trading.portfolio-state/v1")
    ledger_record = _ledger_record(
        verified,
        store,
        record_type="SIMULATED_FILL",
        object_document=fill,
        portfolio_state_id=cast(str, after["portfolio_state_id"]),
        causation_ids=[
            *([] if fill["fill_event_id"] is None else [cast(str, fill["fill_event_id"])]),
            cast(str, intent["intent_id"]),
            cast(str, before["portfolio_state_id"]),
        ],
        entries=_fill_postings(before, after, intent, fill),
    )
    receipt_record = reconcile_transition(
        verified,
        kind="FILL_TRANSITION",
        portfolio_state_before_record=before_record,
        portfolio_state_after_record=after_record,
        ledger_record=ledger_record,
        causation_records=(bytes(intent_record), bytes(fill_receipt_record), *store.ledger_records),
    )
    if _receipt_status(receipt_record) != "PASS":
        return AccountingTransition(store=store, portfolio_state_record=before_record, ledger_record=None, reconciliation_receipt_record=receipt_record)
    return AccountingTransition(
        store=append_ledger_record(store, ledger_record),
        portfolio_state_record=after_record,
        ledger_record=ledger_record,
        reconciliation_receipt_record=receipt_record,
    )
