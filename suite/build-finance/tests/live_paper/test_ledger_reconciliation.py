"""Qualitative contract for hash-chained G2 paper accounting and reconciliation."""

from __future__ import annotations

import copy
import hashlib
from typing import Any

from build_finance.crypto_replay.canonical import (
    canonical_record_bytes,
    parse_canonical_record,
    sha256_hex,
)
from build_finance.crypto_replay.content_ids import seal_content_id as seal_replay_content_id
from build_finance.crypto_replay.content_ids import verify_content_id as verify_replay_content_id
from build_finance.crypto_replay.run_inputs import ContractVerifiedRunInputs
from build_finance.crypto_replay.schema_registry import require_valid_contract as require_valid_replay_contract
from build_finance.live_paper.accounting import apply_fill_receipt, initialize_accounting, reserve_intent
from build_finance.live_paper.event_store import InMemoryLedgerStore, append_ledger_record
from build_finance.live_paper.profiles import PaperKernelProfiles
from build_finance.live_paper.reconciliation import reconcile_transition
from build_finance.live_paper.run_input_builder import build_verified_run_inputs
from tests.live_paper.support.g2_vectors import G2Vector, build_g2_vector

_MAX_U64 = 18_446_744_073_709_551_615
_QUOTE = "synthetic-quote"
_BASE = "synthetic-base"
_MARKET = "synthetic-base/synthetic-quote:jupiter"


def _profiles(vector: G2Vector) -> PaperKernelProfiles:
    return PaperKernelProfiles(**vector.profile_records)  # type: ignore[arg-type]


def _verified_case() -> tuple[G2Vector, ContractVerifiedRunInputs]:
    vector = build_g2_vector()
    verified = build_verified_run_inputs(
        vector.run_receipt_record,
        vector.admitted_candidates,
        vector.source_receipt_records,
        vector.resolver,
        _profiles(vector),
    )
    return vector, verified


def _digest(label: str) -> str:
    return hashlib.sha256(f"G2 TASK 10:{label}".encode("ascii")).hexdigest()


def _doc(record: bytes) -> dict[str, Any]:
    return dict(parse_canonical_record(record))


def _self_id(record: bytes, field: str) -> str:
    value = _doc(record)[field]
    assert isinstance(value, str)
    return value


def _sealed_record(document: dict[str, Any], expected_schema: str) -> bytes:
    sealed = seal_replay_content_id(document)
    require_valid_replay_contract(sealed, expected_schema=expected_schema)
    assert verify_replay_content_id(sealed)
    return canonical_record_bytes(sealed)


def _intent_record(
    state_record: bytes,
    *,
    label: str,
    intent_sequence: str,
    decision_sequence: str,
    action: str,
    quantity_base_atoms: str,
    reserved_quote_atoms: str,
    reserved_base_atoms: str,
    decision_ingest_sequence: str,
    decision_equal_time_group: str,
    created_replay_clock_ns: str,
    reference_price_q18: str = "1000000000000000000",
) -> bytes:
    state = _doc(state_record)
    document = {
        "schema": "trading.simulated-order-intent/v1",
        "intent_sequence": intent_sequence,
        "decision_sequence": decision_sequence,
        "risk_decision_id": _digest(f"risk:{label}"),
        "config_admission_receipt_id": state["config_admission_receipt_id"],
        "validated_config_sha256": state["validated_config_sha256"],
        "portfolio_state_before_id": state["portfolio_state_id"],
        "reservation_id": _digest(f"reservation:{label}"),
        "market_id": _MARKET,
        "base_mint": _BASE,
        "quote_mint": _QUOTE,
        "base_decimals": 9,
        "quote_decimals": 6,
        "action": action,
        "quantity_base_atoms": quantity_base_atoms,
        "reference_price_q18": reference_price_q18,
        "stop_price_q18": "950000000000000000",
        "take_price_q18": "1100000000000000000",
        "max_participation_bps": 500,
        "max_impact_bps": 50,
        "reserved_quote_atoms": reserved_quote_atoms,
        "reserved_base_atoms": reserved_base_atoms,
        "created_replay_clock_ns": created_replay_clock_ns,
        "decision_ingest_sequence": decision_ingest_sequence,
        "decision_equal_time_group": decision_equal_time_group,
        "fill_policy": "STRICT_NEXT_EVENT",
        "time_in_force": "ONE_EVENT_GROUP",
    }
    return _sealed_record(document, "trading.simulated-order-intent/v1")


def _fill_record(
    intent_record: bytes,
    portfolio_state_before_fill_record: bytes,
    *,
    label: str,
    receipt_sequence: str,
    fill_ingest_sequence: str,
    fill_equal_time_group: str,
    fill_replay_clock_ns: str,
    gross_quote_atoms: str,
    fee_quote_atoms: str,
    cash_delta_quote_atoms: str,
    execution_price_q18: str,
    released_quote_atoms: str,
    released_base_atoms: str,
    filled_base_atoms: str = "100000000",
    unfilled_base_atoms: str = "0",
) -> bytes:
    intent = _doc(intent_record)
    state = _doc(portfolio_state_before_fill_record)
    document = {
        "schema": "trading.simulated-fill-receipt/v1",
        "receipt_sequence": receipt_sequence,
        "decision_sequence": intent["decision_sequence"],
        "intent_id": intent["intent_id"],
        "portfolio_state_before_id": state["portfolio_state_id"],
        "status": "FILLED",
        "reason_codes": [],
        "fill_event_id": _digest(f"fill-event:{label}"),
        "decision_ingest_sequence": intent["decision_ingest_sequence"],
        "decision_equal_time_group": intent["decision_equal_time_group"],
        "fill_ingest_sequence": fill_ingest_sequence,
        "fill_equal_time_group": fill_equal_time_group,
        "fill_replay_clock_ns": fill_replay_clock_ns,
        "requested_base_atoms": intent["quantity_base_atoms"],
        "filled_base_atoms": filled_base_atoms,
        "unfilled_base_atoms": unfilled_base_atoms,
        "gross_quote_atoms": gross_quote_atoms,
        "venue_fee_quote_atoms": fee_quote_atoms,
        "priority_fee_quote_atoms": "0",
        "simulation_fee_quote_atoms": "0",
        "cash_delta_quote_atoms": cash_delta_quote_atoms,
        "execution_price_q18": execution_price_q18,
        "participation_bps": 200,
        "reference_deviation_bps": 0,
        "impact_bps": 0,
        "fee_bps": 25,
        "adverse_fill_bps": 0,
        "adverse_fill_draw_key_sha256": _digest(f"draw-key:{label}"),
        "adverse_fill_draw_sha256": _digest(f"draw:{label}"),
        "released_quote_atoms": released_quote_atoms,
        "released_base_atoms": released_base_atoms,
    }
    return _sealed_record(document, "trading.simulated-fill-receipt/v1")


def _zero_reconciliation(portfolio_state_id: str) -> dict[str, Any]:
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


def _assert_replay_record(record: bytes, expected_schema: str) -> dict[str, Any]:
    document = _doc(record)
    require_valid_replay_contract(document, expected_schema=expected_schema)
    assert verify_replay_content_id(document)
    return document


def _assert_reconciliation(record: bytes) -> dict[str, Any]:
    return _assert_replay_record(record, "trading.reconciliation-receipt/v1")


def _resealed_record(record: bytes, identity_field: str, expected_schema: str, **updates: Any) -> bytes:
    document = _doc(record)
    document.pop(identity_field)
    document.update(updates)
    return _sealed_record(document, expected_schema)


def _tampered_valid_ledger_record(ledger_record: bytes) -> bytes:
    ledger = _doc(ledger_record)
    ledger.pop("ledger_record_id")
    entries = copy.deepcopy(ledger["entries"])
    entries[0]["amount_atoms"] = "99999999"
    entries[1]["amount_atoms"] = "-99999999"
    ledger["entries"] = entries
    return _sealed_record(ledger, "trading.ledger-record/v1")


def _tampered_valid_state_record(state_record: bytes) -> bytes:
    state = _doc(state_record)
    state.pop("portfolio_state_id")
    balances = copy.deepcopy(state["balances"])
    quote = next(row for row in balances if row["mint"] == _QUOTE)
    quote["available_atoms"] = str(int(quote["available_atoms"]) + 1)
    quote["total_atoms"] = str(int(quote["total_atoms"]) + 1)
    state["balances"] = balances
    summary = copy.deepcopy(state["summary"])
    summary["equity_quote_atoms"] = str(int(summary["equity_quote_atoms"]) + 1)
    state["summary"] = summary
    return _sealed_record(state, "trading.portfolio-state/v1")


def _invalid_self_id_ledger_record(ledger_record: bytes) -> bytes:
    ledger = _doc(ledger_record)
    ledger["ledger_record_id"] = _digest("invalid retained duplicate ledger self id")
    return canonical_record_bytes(ledger)


def _assert_group_gate_invariant(receipt: dict[str, Any], expected_state_id: str) -> None:
    assert receipt["status"] == "KILLED"
    assert receipt["reconciliation_kind"] == "GROUP_GATE"
    assert receipt["reason_codes"] == [
        "RECONCILIATION_ABSOLUTE_STATE_INVARIANT",
        "RECONCILIATION_MISMATCH",
    ]
    assert receipt["portfolio_state_before_id"] == expected_state_id
    assert receipt["portfolio_state_after_id"] == expected_state_id


def _overflow_close_case(
    vector: G2Vector,
    genesis_state_record: bytes,
) -> tuple[bytes, bytes, bytes]:
    genesis = _doc(genesis_state_record)
    intent_record = _intent_record(
        genesis_state_record,
        label="overflow-close",
        intent_sequence="9",
        decision_sequence="9",
        action="CLOSE_LONG",
        quantity_base_atoms="1",
        reserved_quote_atoms="0",
        reserved_base_atoms="1",
        decision_ingest_sequence="9",
        decision_equal_time_group="9",
        created_replay_clock_ns="9000000000",
    )
    intent = _doc(intent_record)
    reservation_state = {
        "schema": "trading.portfolio-state/v1",
        "state_sequence": "1",
        "as_of_ingest_sequence": "9",
        "equal_time_group": "9",
        "replay_clock_ns": "9000000000",
        "previous_portfolio_state_id": genesis["portfolio_state_id"],
        "causation_schema": "trading.simulated-order-intent/v1",
        "causation_id": intent["intent_id"],
        "fixture_manifest_sha256": vector.run_input_bundle.fixture_manifest["fixture_manifest_sha256"],
        "config_admission_receipt_id": vector.run_input_bundle.config_admission_receipt[
            "config_admission_receipt_id"
        ],
        "validated_config_sha256": vector.run_input_bundle.replay_risk_config["config_sha256"],
        "quote_mint": _QUOTE,
        "quote_decimals": 6,
        "balances": [
            {
                "mint": _BASE,
                "decimals": 9,
                "available_atoms": "0",
                "reserved_atoms": "1",
                "total_atoms": "1",
            },
            {
                "mint": _QUOTE,
                "decimals": 6,
                "available_atoms": str(_MAX_U64 - 100),
                "reserved_atoms": "0",
                "total_atoms": str(_MAX_U64 - 100),
            },
        ],
        "positions": [
            {
                "market_id": _MARKET,
                "base_mint": _BASE,
                "base_decimals": 9,
                "quantity_base_atoms": "1",
                "reserved_base_atoms": "1",
                "cost_basis_quote_atoms": "0",
                "mark_price_q18": "1000000000000000000",
                "market_value_quote_atoms": "100",
                "unrealized_pnl_quote_atoms": "100",
                "stop_price_q18": "1",
                "take_price_q18": "2",
            }
        ],
        "summary": {
            "realized_pnl_quote_atoms": "0",
            "unrealized_pnl_quote_atoms": "100",
            "cumulative_fees_quote_atoms": "0",
            "session_pnl_quote_atoms": "100",
            "equity_quote_atoms": str(_MAX_U64),
            "peak_equity_quote_atoms": str(_MAX_U64),
            "drawdown_bps": 0,
        },
        "kill_latched": False,
        "kill_reason_codes": [],
        "open_intent_ids": [intent["intent_id"]],
    }
    reservation_state_record = _sealed_record(reservation_state, "trading.portfolio-state/v1")
    overflow_fill_record = _fill_record(
        intent_record,
        reservation_state_record,
        label="overflow-close",
        receipt_sequence="9",
        fill_ingest_sequence="10",
        fill_equal_time_group="10",
        fill_replay_clock_ns="10000000000",
        gross_quote_atoms="200",
        fee_quote_atoms="0",
        cash_delta_quote_atoms="200",
        execution_price_q18="2000000000000000000",
        released_quote_atoms="0",
        released_base_atoms="0",
        filled_base_atoms="1",
    )
    return reservation_state_record, intent_record, overflow_fill_record


def test_genesis_and_intent_reservation_hash_chain_reservations_and_replay_are_byte_stable() -> None:
    _vector, verified = _verified_case()

    genesis = initialize_accounting(
        verified,
        quote_mint=_QUOTE,
        quote_decimals=6,
        starting_quote_atoms=1_000_000,
    )
    genesis_state = _assert_replay_record(genesis.portfolio_state_record, "trading.portfolio-state/v1")
    genesis_receipt = _assert_reconciliation(genesis.reconciliation_receipt_record)

    assert isinstance(genesis.store, InMemoryLedgerStore)
    assert genesis.store.ledger_records == ()
    assert genesis.ledger_record is None
    assert genesis_state["balances"] == [
        {
            "mint": _QUOTE,
            "decimals": 6,
            "available_atoms": "1000000",
            "reserved_atoms": "0",
            "total_atoms": "1000000",
        }
    ]
    assert genesis_state["positions"] == []
    assert genesis_state["summary"] == {
        "realized_pnl_quote_atoms": "0",
        "unrealized_pnl_quote_atoms": "0",
        "cumulative_fees_quote_atoms": "0",
        "session_pnl_quote_atoms": "0",
        "equity_quote_atoms": "1000000",
        "peak_equity_quote_atoms": "1000000",
        "drawdown_bps": 0,
    }
    assert genesis_receipt["status"] == "PASS"
    assert genesis_receipt["reconciliation_kind"] == "GENESIS"
    assert genesis_receipt["reason_codes"] == []
    assert genesis_receipt["portfolio_state_before_id"] == genesis_receipt["portfolio_state_after_id"] == genesis_state[
        "portfolio_state_id"
    ]

    intent_record = _intent_record(
        genesis.portfolio_state_record,
        label="open-reservation",
        intent_sequence="1",
        decision_sequence="1",
        action="OPEN_LONG",
        quantity_base_atoms="100000000",
        reserved_quote_atoms="101000",
        reserved_base_atoms="0",
        decision_ingest_sequence="1",
        decision_equal_time_group="1",
        created_replay_clock_ns="0",
    )
    intent = _assert_replay_record(intent_record, "trading.simulated-order-intent/v1")
    reserved = reserve_intent(verified, genesis.store, genesis.portfolio_state_record, intent_record)
    repeated = reserve_intent(verified, genesis.store, genesis.portfolio_state_record, intent_record)
    reserved_state = _assert_replay_record(reserved.portfolio_state_record, "trading.portfolio-state/v1")
    reserved_ledger = _assert_replay_record(reserved.ledger_record, "trading.ledger-record/v1")
    reserved_receipt = _assert_reconciliation(reserved.reconciliation_receipt_record)

    assert repeated == reserved
    assert genesis.store.ledger_records == ()
    assert reserved.store.ledger_records == (reserved.ledger_record,)
    assert append_ledger_record(genesis.store, reserved.ledger_record) == reserved.store
    assert reserved_state["state_sequence"] == "1"
    assert reserved_state["previous_portfolio_state_id"] == genesis_state["portfolio_state_id"]
    assert reserved_state["causation_id"] == intent["intent_id"]
    assert reserved_state["balances"] == [
        {
            "mint": _QUOTE,
            "decimals": 6,
            "available_atoms": "899000",
            "reserved_atoms": "101000",
            "total_atoms": "1000000",
        }
    ]
    assert reserved_state["positions"] == []
    assert reserved_state["open_intent_ids"] == [intent["intent_id"]]
    assert reserved_state["summary"] == genesis_state["summary"]
    assert reserved_ledger == {
        "schema": "trading.ledger-record/v1",
        "ledger_record_id": reserved_ledger["ledger_record_id"],
        "ledger_sequence": "0",
        "previous_ledger_record_id": None,
        "run_receipt_id": verified.run_receipt["run_receipt_id"],
        "fixture_manifest_sha256": genesis_state["fixture_manifest_sha256"],
        "config_admission_receipt_id": genesis_state["config_admission_receipt_id"],
        "validated_config_sha256": genesis_state["validated_config_sha256"],
        "record_type": "SIMULATED_INTENT",
        "object_schema": "trading.simulated-order-intent/v1",
        "object_id": intent["intent_id"],
        "object_sha256": intent["intent_id"],
        "decision_sequence": "1",
        "ingest_sequence": "1",
        "equal_time_group": "1",
        "replay_clock_ns": "0",
        "causation_ids": sorted([intent["risk_decision_id"], genesis_state["portfolio_state_id"]]),
        "entries": [
            {"account": "CASH_AVAILABLE", "asset_mint": _QUOTE, "decimals": 6, "amount_atoms": "-101000"},
            {"account": "CASH_RESERVED", "asset_mint": _QUOTE, "decimals": 6, "amount_atoms": "101000"},
        ],
        "reconciliation": _zero_reconciliation(reserved_state["portfolio_state_id"]),
    }
    assert reserved_receipt["status"] == "PASS"
    assert reserved_receipt["reconciliation_kind"] == "INTENT_RESERVATION"
    assert reserved_receipt["causation_ids"] == [reserved_ledger["ledger_record_id"]]

    reserved_as_genesis = _assert_reconciliation(
        reconcile_transition(
            verified,
            kind="GENESIS",
            portfolio_state_before_record=genesis.portfolio_state_record,
            portfolio_state_after_record=reserved.portfolio_state_record,
            ledger_record=reserved.ledger_record,
            causation_records=(intent_record,),
        )
    )
    _assert_group_gate_invariant(reserved_as_genesis, genesis_state["portfolio_state_id"])

    bad_intent_record = _resealed_record(
        intent_record,
        "intent_id",
        "trading.simulated-order-intent/v1",
        portfolio_state_before_id=_digest("foreign-before-state"),
    )
    bad_intent = _assert_replay_record(bad_intent_record, "trading.simulated-order-intent/v1")
    bad_reserved_state_record = _resealed_record(
        reserved.portfolio_state_record,
        "portfolio_state_id",
        "trading.portfolio-state/v1",
        causation_id=bad_intent["intent_id"],
        open_intent_ids=[bad_intent["intent_id"]],
    )
    bad_reserved_ledger_record = _resealed_record(
        reserved.ledger_record,
        "ledger_record_id",
        "trading.ledger-record/v1",
        object_id=bad_intent["intent_id"],
        object_sha256=bad_intent["intent_id"],
        causation_ids=sorted([bad_intent["risk_decision_id"], genesis_state["portfolio_state_id"]]),
    )
    bad_intent_binding = _assert_reconciliation(
        reconcile_transition(
            verified,
            kind="INTENT_RESERVATION",
            portfolio_state_before_record=genesis.portfolio_state_record,
            portfolio_state_after_record=bad_reserved_state_record,
            ledger_record=bad_reserved_ledger_record,
            causation_records=(bad_intent_record,),
        )
    )
    _assert_group_gate_invariant(bad_intent_binding, genesis_state["portfolio_state_id"])


def test_open_and_close_fills_project_portfolio_fees_pnl_and_hash_chain() -> None:
    _vector, verified = _verified_case()
    genesis = initialize_accounting(verified, quote_mint=_QUOTE, quote_decimals=6, starting_quote_atoms=1_000_000)
    open_intent_record = _intent_record(
        genesis.portfolio_state_record,
        label="open-fill",
        intent_sequence="1",
        decision_sequence="1",
        action="OPEN_LONG",
        quantity_base_atoms="100000000",
        reserved_quote_atoms="101000",
        reserved_base_atoms="0",
        decision_ingest_sequence="1",
        decision_equal_time_group="1",
        created_replay_clock_ns="0",
    )
    open_reserved = reserve_intent(verified, genesis.store, genesis.portfolio_state_record, open_intent_record)
    open_fill_record = _fill_record(
        open_intent_record,
        open_reserved.portfolio_state_record,
        label="open-fill",
        receipt_sequence="1",
        fill_ingest_sequence="2",
        fill_equal_time_group="2",
        fill_replay_clock_ns="1000000000",
        gross_quote_atoms="100000",
        fee_quote_atoms="250",
        cash_delta_quote_atoms="-100250",
        execution_price_q18="1000000000000000000",
        released_quote_atoms="750",
        released_base_atoms="0",
    )
    opened = apply_fill_receipt(
        verified,
        open_reserved.store,
        open_reserved.portfolio_state_record,
        open_intent_record,
        open_fill_record,
    )
    open_state = _assert_replay_record(opened.portfolio_state_record, "trading.portfolio-state/v1")
    open_ledger = _assert_replay_record(opened.ledger_record, "trading.ledger-record/v1")

    assert open_state["balances"] == [
        {"mint": _BASE, "decimals": 9, "available_atoms": "100000000", "reserved_atoms": "0", "total_atoms": "100000000"},
        {"mint": _QUOTE, "decimals": 6, "available_atoms": "899750", "reserved_atoms": "0", "total_atoms": "899750"},
    ]
    assert open_state["positions"] == [
        {
            "market_id": _MARKET,
            "base_mint": _BASE,
            "base_decimals": 9,
            "quantity_base_atoms": "100000000",
            "reserved_base_atoms": "0",
            "cost_basis_quote_atoms": "100250",
            "mark_price_q18": "1000000000000000000",
            "market_value_quote_atoms": "100000",
            "unrealized_pnl_quote_atoms": "-250",
            "stop_price_q18": "950000000000000000",
            "take_price_q18": "1100000000000000000",
        }
    ]
    assert open_state["summary"] == {
        "realized_pnl_quote_atoms": "0",
        "unrealized_pnl_quote_atoms": "-250",
        "cumulative_fees_quote_atoms": "250",
        "session_pnl_quote_atoms": "-250",
        "equity_quote_atoms": "999750",
        "peak_equity_quote_atoms": "1000000",
        "drawdown_bps": 3,
    }
    assert open_state["open_intent_ids"] == []
    assert open_ledger["ledger_sequence"] == "1"
    assert open_ledger["previous_ledger_record_id"] == _self_id(open_reserved.ledger_record, "ledger_record_id")
    assert open_ledger["entries"] == [
        {"account": "POSITION_AVAILABLE", "asset_mint": _BASE, "decimals": 9, "amount_atoms": "100000000"},
        {"account": "TRADE_CLEARING", "asset_mint": _BASE, "decimals": 9, "amount_atoms": "-100000000"},
        {"account": "CASH_AVAILABLE", "asset_mint": _QUOTE, "decimals": 6, "amount_atoms": "750"},
        {"account": "CASH_RESERVED", "asset_mint": _QUOTE, "decimals": 6, "amount_atoms": "-101000"},
        {"account": "EQUITY_CONTROL", "asset_mint": _QUOTE, "decimals": 6, "amount_atoms": "250"},
        {"account": "FEE_EXPENSE", "asset_mint": _QUOTE, "decimals": 6, "amount_atoms": "250"},
        {"account": "TRADE_CLEARING", "asset_mint": _QUOTE, "decimals": 6, "amount_atoms": "100000"},
        {"account": "UNREALIZED_PNL", "asset_mint": _QUOTE, "decimals": 6, "amount_atoms": "-250"},
    ]

    close_intent_record = _intent_record(
        opened.portfolio_state_record,
        label="close-fill",
        intent_sequence="2",
        decision_sequence="2",
        action="CLOSE_LONG",
        quantity_base_atoms="100000000",
        reserved_quote_atoms="0",
        reserved_base_atoms="100000000",
        decision_ingest_sequence="3",
        decision_equal_time_group="3",
        created_replay_clock_ns="2000000000",
        reference_price_q18="1200000000000000000",
    )
    close_reserved = reserve_intent(verified, opened.store, opened.portfolio_state_record, close_intent_record)
    close_reserve_ledger = _assert_replay_record(close_reserved.ledger_record, "trading.ledger-record/v1")
    assert close_reserve_ledger["ledger_sequence"] == "2"
    assert close_reserve_ledger["previous_ledger_record_id"] == open_ledger["ledger_record_id"]
    assert close_reserve_ledger["entries"] == [
        {"account": "POSITION_AVAILABLE", "asset_mint": _BASE, "decimals": 9, "amount_atoms": "-100000000"},
        {"account": "POSITION_RESERVED", "asset_mint": _BASE, "decimals": 9, "amount_atoms": "100000000"},
    ]

    close_fill_record = _fill_record(
        close_intent_record,
        close_reserved.portfolio_state_record,
        label="close-fill",
        receipt_sequence="2",
        fill_ingest_sequence="4",
        fill_equal_time_group="4",
        fill_replay_clock_ns="3000000000",
        gross_quote_atoms="120000",
        fee_quote_atoms="300",
        cash_delta_quote_atoms="119700",
        execution_price_q18="1200000000000000000",
        released_quote_atoms="0",
        released_base_atoms="0",
    )
    closed = apply_fill_receipt(
        verified,
        close_reserved.store,
        close_reserved.portfolio_state_record,
        close_intent_record,
        close_fill_record,
    )
    closed_state = _assert_replay_record(closed.portfolio_state_record, "trading.portfolio-state/v1")
    close_ledger = _assert_replay_record(closed.ledger_record, "trading.ledger-record/v1")
    close_receipt = _assert_reconciliation(closed.reconciliation_receipt_record)

    assert closed_state["balances"] == [
        {"mint": _QUOTE, "decimals": 6, "available_atoms": "1019450", "reserved_atoms": "0", "total_atoms": "1019450"}
    ]
    assert closed_state["positions"] == []
    assert closed_state["summary"] == {
        "realized_pnl_quote_atoms": "19450",
        "unrealized_pnl_quote_atoms": "0",
        "cumulative_fees_quote_atoms": "550",
        "session_pnl_quote_atoms": "19450",
        "equity_quote_atoms": "1019450",
        "peak_equity_quote_atoms": "1019450",
        "drawdown_bps": 0,
    }
    assert close_ledger["ledger_sequence"] == "3"
    assert close_ledger["previous_ledger_record_id"] == close_reserve_ledger["ledger_record_id"]
    assert close_ledger["entries"] == [
        {"account": "POSITION_RESERVED", "asset_mint": _BASE, "decimals": 9, "amount_atoms": "-100000000"},
        {"account": "TRADE_CLEARING", "asset_mint": _BASE, "decimals": 9, "amount_atoms": "100000000"},
        {"account": "CASH_AVAILABLE", "asset_mint": _QUOTE, "decimals": 6, "amount_atoms": "119700"},
        {"account": "EQUITY_CONTROL", "asset_mint": _QUOTE, "decimals": 6, "amount_atoms": "-19700"},
        {"account": "FEE_EXPENSE", "asset_mint": _QUOTE, "decimals": 6, "amount_atoms": "300"},
        {"account": "REALIZED_PNL", "asset_mint": _QUOTE, "decimals": 6, "amount_atoms": "19450"},
        {"account": "TRADE_CLEARING", "asset_mint": _QUOTE, "decimals": 6, "amount_atoms": "-120000"},
        {"account": "UNREALIZED_PNL", "asset_mint": _QUOTE, "decimals": 6, "amount_atoms": "250"},
    ]
    assert [parse_canonical_record(record)["ledger_sequence"] for record in closed.store.ledger_records] == [
        "0",
        "1",
        "2",
        "3",
    ]
    assert closed.store.ledger_records[-1] == closed.ledger_record
    assert close_receipt["status"] == "PASS"
    assert close_receipt["reconciliation_kind"] == "FILL_TRANSITION"
    assert close_receipt["causation_ids"] == [close_ledger["ledger_record_id"]]


def test_reconciliation_halts_duplicate_tampered_evidence_and_arithmetic_range() -> None:
    vector, verified = _verified_case()
    genesis = initialize_accounting(verified, quote_mint=_QUOTE, quote_decimals=6, starting_quote_atoms=1_000_000)
    open_intent_record = _intent_record(
        genesis.portfolio_state_record,
        label="halt-open",
        intent_sequence="1",
        decision_sequence="1",
        action="OPEN_LONG",
        quantity_base_atoms="100000000",
        reserved_quote_atoms="101000",
        reserved_base_atoms="0",
        decision_ingest_sequence="1",
        decision_equal_time_group="1",
        created_replay_clock_ns="0",
    )
    open_reserved = reserve_intent(verified, genesis.store, genesis.portfolio_state_record, open_intent_record)
    open_fill_record = _fill_record(
        open_intent_record,
        open_reserved.portfolio_state_record,
        label="halt-open",
        receipt_sequence="1",
        fill_ingest_sequence="2",
        fill_equal_time_group="2",
        fill_replay_clock_ns="1000000000",
        gross_quote_atoms="100000",
        fee_quote_atoms="250",
        cash_delta_quote_atoms="-100250",
        execution_price_q18="1000000000000000000",
        released_quote_atoms="750",
        released_base_atoms="0",
    )
    opened = apply_fill_receipt(
        verified,
        open_reserved.store,
        open_reserved.portfolio_state_record,
        open_intent_record,
        open_fill_record,
    )
    duplicate = apply_fill_receipt(
        verified,
        opened.store,
        opened.portfolio_state_record,
        open_intent_record,
        open_fill_record,
    )
    duplicate_receipt = _assert_reconciliation(duplicate.reconciliation_receipt_record)

    assert duplicate.store == opened.store
    assert duplicate.portfolio_state_record == opened.portfolio_state_record
    assert duplicate.ledger_record is None
    assert duplicate_receipt["status"] == "KILLED"
    assert duplicate_receipt["reconciliation_kind"] == "IDEMPOTENCY_CONFLICT"
    assert duplicate_receipt["reason_codes"] == [
        "RECONCILIATION_IDEMPOTENCY_CONFLICT",
        "RECONCILIATION_MISMATCH",
    ]
    assert duplicate_receipt["idempotency_conflict_scope"] == "FILL"
    assert duplicate_receipt["original_object_id"] == _self_id(open_fill_record, "fill_receipt_id")
    assert duplicate_receipt["conflicting_body_sha256"] == sha256_hex(open_fill_record)

    invalid_retained_duplicate = _assert_reconciliation(
        reconcile_transition(
            verified,
            kind="FILL_TRANSITION",
            portfolio_state_before_record=opened.portfolio_state_record,
            portfolio_state_after_record=opened.portfolio_state_record,
            ledger_record=None,
            causation_records=(open_intent_record, open_fill_record, _invalid_self_id_ledger_record(opened.ledger_record)),
        )
    )
    _assert_group_gate_invariant(invalid_retained_duplicate, _self_id(opened.portfolio_state_record, "portfolio_state_id"))

    tampered_ledger_receipt = _assert_reconciliation(
        reconcile_transition(
            verified,
            kind="FILL_TRANSITION",
            portfolio_state_before_record=open_reserved.portfolio_state_record,
            portfolio_state_after_record=opened.portfolio_state_record,
            ledger_record=_tampered_valid_ledger_record(opened.ledger_record),
            causation_records=(open_intent_record, open_fill_record),
        )
    )
    tampered_state_receipt = _assert_reconciliation(
        reconcile_transition(
            verified,
            kind="FILL_TRANSITION",
            portfolio_state_before_record=open_reserved.portfolio_state_record,
            portfolio_state_after_record=_tampered_valid_state_record(opened.portfolio_state_record),
            ledger_record=opened.ledger_record,
            causation_records=(open_intent_record, open_fill_record),
        )
    )
    tampered_fill_record = _fill_record(
        open_intent_record,
        open_reserved.portfolio_state_record,
        label="halt-open-tampered-fill",
        receipt_sequence="1",
        fill_ingest_sequence="2",
        fill_equal_time_group="2",
        fill_replay_clock_ns="1000000000",
        gross_quote_atoms="100001",
        fee_quote_atoms="250",
        cash_delta_quote_atoms="-100251",
        execution_price_q18="1000000000000000000",
        released_quote_atoms="749",
        released_base_atoms="0",
    )
    tampered_fill_receipt = _assert_reconciliation(
        reconcile_transition(
            verified,
            kind="FILL_TRANSITION",
            portfolio_state_before_record=open_reserved.portfolio_state_record,
            portfolio_state_after_record=opened.portfolio_state_record,
            ledger_record=opened.ledger_record,
            causation_records=(open_intent_record, tampered_fill_record),
        )
    )
    assert tampered_ledger_receipt["status"] == "KILLED"
    assert tampered_ledger_receipt["reconciliation_kind"] == "FILL_TRANSITION"
    assert tampered_ledger_receipt["reason_codes"] == [
        "RECONCILIATION_ACCOUNT_RESIDUAL",
        "RECONCILIATION_MISMATCH",
    ]
    assert tampered_ledger_receipt["account_residuals"] == [
        {"asset_mint": _BASE, "account": "POSITION_AVAILABLE", "residual_atoms": "-1"}
    ]
    assert tampered_ledger_receipt["asset_residuals"] == []

    assert tampered_state_receipt["status"] == "KILLED"
    assert tampered_state_receipt["reconciliation_kind"] == "FILL_TRANSITION"
    assert tampered_state_receipt["reason_codes"] == [
        "RECONCILIATION_ACCOUNT_RESIDUAL",
        "RECONCILIATION_ASSET_RESIDUAL",
        "RECONCILIATION_EQUITY_RESIDUAL",
        "RECONCILIATION_MISMATCH",
    ]
    assert tampered_state_receipt["account_residuals"] == [
        {"asset_mint": _QUOTE, "account": "CASH_AVAILABLE", "residual_atoms": "1"}
    ]
    assert tampered_state_receipt["asset_residuals"] == [{"asset_mint": _QUOTE, "residual_atoms": "1"}]
    assert tampered_state_receipt["equity_residual_quote_atoms"] == "1"

    assert tampered_fill_receipt["status"] == "KILLED"
    assert tampered_fill_receipt["reconciliation_kind"] == "FILL_TRANSITION"
    assert tampered_fill_receipt["reason_codes"] == [
        "RECONCILIATION_ACCOUNT_RESIDUAL",
        "RECONCILIATION_ASSET_RESIDUAL",
        "RECONCILIATION_MISMATCH",
    ]
    assert tampered_fill_receipt["account_residuals"] == [
        {"asset_mint": _QUOTE, "account": "CASH_AVAILABLE", "residual_atoms": "1"}
    ]
    assert tampered_fill_receipt["asset_residuals"] == [{"asset_mint": _QUOTE, "residual_atoms": "1"}]

    chain_tampered_ledger_receipt = _assert_reconciliation(
        reconcile_transition(
            verified,
            kind="FILL_TRANSITION",
            portfolio_state_before_record=open_reserved.portfolio_state_record,
            portfolio_state_after_record=opened.portfolio_state_record,
            ledger_record=_resealed_record(
                opened.ledger_record,
                "ledger_record_id",
                "trading.ledger-record/v1",
                ledger_sequence="99",
            ),
            causation_records=(open_intent_record, open_fill_record, *open_reserved.store.ledger_records),
        )
    )
    _assert_group_gate_invariant(
        chain_tampered_ledger_receipt,
        _self_id(open_reserved.portfolio_state_record, "portfolio_state_id"),
    )

    bad_fill_record = _resealed_record(
        open_fill_record,
        "fill_receipt_id",
        "trading.simulated-fill-receipt/v1",
        intent_id=_digest("foreign-fill-intent"),
    )
    bad_fill = _assert_replay_record(bad_fill_record, "trading.simulated-fill-receipt/v1")
    bad_fill_state_record = _resealed_record(
        opened.portfolio_state_record,
        "portfolio_state_id",
        "trading.portfolio-state/v1",
        causation_id=bad_fill["fill_receipt_id"],
    )
    bad_fill_ledger_record = _resealed_record(
        opened.ledger_record,
        "ledger_record_id",
        "trading.ledger-record/v1",
        object_id=bad_fill["fill_receipt_id"],
        object_sha256=bad_fill["fill_receipt_id"],
    )
    bad_fill_binding = _assert_reconciliation(
        reconcile_transition(
            verified,
            kind="FILL_TRANSITION",
            portfolio_state_before_record=open_reserved.portfolio_state_record,
            portfolio_state_after_record=bad_fill_state_record,
            ledger_record=bad_fill_ledger_record,
            causation_records=(open_intent_record, bad_fill_record),
        )
    )
    _assert_group_gate_invariant(
        bad_fill_binding,
        _self_id(open_reserved.portfolio_state_record, "portfolio_state_id"),
    )

    overflow_state_record, overflow_intent_record, overflow_fill_record = _overflow_close_case(
        vector,
        genesis.portfolio_state_record,
    )
    overflow = apply_fill_receipt(
        verified,
        opened.store,
        overflow_state_record,
        overflow_intent_record,
        overflow_fill_record,
    )
    overflow_receipt = _assert_reconciliation(overflow.reconciliation_receipt_record)
    assert overflow.store == opened.store
    assert overflow.portfolio_state_record == overflow_state_record
    assert overflow.ledger_record is None
    assert overflow_receipt["status"] == "KILLED"
    assert overflow_receipt["reconciliation_kind"] == "ARITHMETIC_RANGE"
    assert overflow_receipt["reason_codes"] == ["RECONCILIATION_ARITHMETIC_RANGE", "RECONCILIATION_MISMATCH"]
    assert overflow_receipt["arithmetic_operation"] == "LEDGER_POSTING"
    assert overflow_receipt["arithmetic_operands_sha256"] is not None
    assert overflow_receipt["arithmetic_range_key_sha256"] is not None
