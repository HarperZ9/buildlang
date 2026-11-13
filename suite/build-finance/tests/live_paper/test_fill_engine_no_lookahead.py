"""Qualitative contract for causal terminal G2 simulated fills."""

from __future__ import annotations

import copy
import hashlib
from dataclasses import replace
from typing import Any

import pytest

from build_finance.crypto_replay.canonical import (
    canonical_record_bytes,
    parse_canonical_record,
)
from build_finance.crypto_replay.content_ids import seal_content_id as seal_replay_content_id
from build_finance.crypto_replay.content_ids import verify_content_id as verify_replay_content_id
from build_finance.crypto_replay.run_inputs import ContractVerifiedRunInputs
from build_finance.crypto_replay.schema_registry import require_valid_contract as require_valid_replay_contract
from build_finance.live_paper.accounting import apply_fill_receipt, initialize_accounting, reserve_intent
from build_finance.live_paper.fills import TerminalFillEvidence, simulate_terminal_fill
from build_finance.live_paper.grouping import EventGroup, group_committed_events
from build_finance.live_paper.profiles import PaperKernelProfiles
from build_finance.live_paper.run_input_builder import build_verified_run_inputs
from tests.live_paper.support.g2_vectors import G2Vector, build_g2_vector


class RecordingResolver:
    """In-memory resolver that exposes the exact content IDs consumed."""

    def __init__(self, records_by_content_id: dict[str, bytes]) -> None:
        self._records_by_content_id = {content_id: bytes(record) for content_id, record in records_by_content_id.items()}
        self.record_hits: list[str] = []
        self.byte_hits: list[str] = []

    def resolve_record(self, content_id: str) -> bytes:
        self.record_hits.append(content_id)
        return bytes(self._records_by_content_id[content_id])

    def resolve_bytes(self, sha256: str) -> bytes:
        self.byte_hits.append(sha256)
        raise AssertionError("fill simulation must not resolve digest-addressed or external bytes")


def _profiles(vector: G2Vector) -> PaperKernelProfiles:
    return PaperKernelProfiles(**vector.profile_records)  # type: ignore[arg-type]


def _verified_groups() -> tuple[G2Vector, ContractVerifiedRunInputs, tuple[EventGroup, ...]]:
    vector = build_g2_vector()
    verified = build_verified_run_inputs(
        vector.run_receipt_record,
        vector.admitted_candidates,
        vector.source_receipt_records,
        vector.resolver,
        _profiles(vector),
    )
    return vector, verified, group_committed_events(verified)


def _causal_digest(label: str) -> str:
    return hashlib.sha256(f"G2 FILL TEST STATE:{label}".encode("ascii")).hexdigest()


def _seal_portfolio_state(document: dict[str, Any]) -> bytes:
    sealed = seal_replay_content_id(document)
    require_valid_replay_contract(sealed, expected_schema="trading.portfolio-state/v1")
    return canonical_record_bytes(sealed)


def _flat_portfolio_state_record(
    vector: G2Vector,
    *,
    quote_available: str = "1000000",
    quote_reserved: str = "0",
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
            "causation_id": _causal_digest(f"flat:{quote_available}:{quote_reserved}"),
            "fixture_manifest_sha256": vector.run_input_bundle.fixture_manifest["fixture_manifest_sha256"],
            "config_admission_receipt_id": vector.run_input_bundle.config_admission_receipt[
                "config_admission_receipt_id"
            ],
            "validated_config_sha256": vector.run_input_bundle.replay_risk_config["config_sha256"],
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
                "realized_pnl_quote_atoms": "0",
                "unrealized_pnl_quote_atoms": "0",
                "cumulative_fees_quote_atoms": "0",
                "session_pnl_quote_atoms": "0",
                "equity_quote_atoms": quote_total,
                "peak_equity_quote_atoms": quote_total,
                "drawdown_bps": 0,
            },
            "kill_latched": False,
            "kill_reason_codes": [],
            "open_intent_ids": sorted(open_intent_ids, key=lambda value: value.encode("utf-8")),
        }
    )


def _intent_record(
    *,
    vector: G2Vector,
    portfolio_state_record: bytes,
    decision_group: EventGroup,
    quantity_base_atoms: str = "100000000",
    reserved_quote_atoms: str = "101000",
    max_participation_bps: int = 500,
) -> bytes:
    event = parse_canonical_record(decision_group.event_records[0])
    portfolio = parse_canonical_record(portfolio_state_record)
    document = {
        "schema": "trading.simulated-order-intent/v1",
        "intent_sequence": decision_group.group_sequence,
        "decision_sequence": decision_group.group_sequence,
        "risk_decision_id": _causal_digest(f"risk:{decision_group.group_sequence}:{quantity_base_atoms}"),
        "config_admission_receipt_id": portfolio["config_admission_receipt_id"],
        "validated_config_sha256": vector.run_input_bundle.replay_risk_config["config_sha256"],
        "portfolio_state_before_id": portfolio["portfolio_state_id"],
        "reservation_id": _causal_digest(f"reservation:{quantity_base_atoms}:{reserved_quote_atoms}"),
        "market_id": event["market_id"],
        "base_mint": event["base_mint"],
        "quote_mint": event["quote_mint"],
        "base_decimals": event["base_decimals"],
        "quote_decimals": event["quote_decimals"],
        "action": "OPEN_LONG",
        "quantity_base_atoms": quantity_base_atoms,
        "reference_price_q18": "1000001000000000000",
        "stop_price_q18": "950001000000000000",
        "take_price_q18": "1100002000000000000",
        "max_participation_bps": max_participation_bps,
        "max_impact_bps": 50,
        "reserved_quote_atoms": reserved_quote_atoms,
        "reserved_base_atoms": "0",
        "created_replay_clock_ns": event["replay_clock_ns"],
        "decision_ingest_sequence": event["ingest_sequence"],
        "decision_equal_time_group": event["equal_time_group"],
        "fill_policy": "STRICT_NEXT_EVENT",
        "time_in_force": "ONE_EVENT_GROUP",
    }
    sealed = seal_replay_content_id(document)
    require_valid_replay_contract(sealed, expected_schema="trading.simulated-order-intent/v1")
    return canonical_record_bytes(sealed)


def _resolver_for(*groups: EventGroup) -> RecordingResolver:
    return RecordingResolver(
        {
            event_id: record
            for group in groups
            for event_id, record in zip(group.event_ids, group.event_records, strict=True)
        }
    )


def _resealed_event_group(group: EventGroup, event: dict[str, Any]) -> EventGroup:
    body = copy.deepcopy(event)
    body.pop("event_id", None)
    record = canonical_record_bytes(seal_replay_content_id(body))
    resealed = parse_canonical_record(record)
    return replace(group, event_ids=(str(resealed["event_id"]),), event_records=(record,))


def _receipt(evidence: TerminalFillEvidence) -> dict[str, Any]:
    receipt = parse_canonical_record(evidence.fill_receipt_record)
    require_valid_replay_contract(receipt, expected_schema="trading.simulated-fill-receipt/v1")
    assert verify_replay_content_id(receipt)
    assert hashlib.sha256(evidence.adverse_fill_draw_key_bytes).hexdigest() == receipt[
        "adverse_fill_draw_key_sha256"
    ]
    assert hashlib.sha256(evidence.adverse_fill_draw_bytes).hexdigest() == receipt["adverse_fill_draw_sha256"]
    return receipt


def test_causal_fill_rejects_decision_group_uses_later_group_and_is_byte_stable() -> None:
    vector, verified, groups = _verified_groups()
    portfolio_record = _flat_portfolio_state_record(vector)
    intent_record = _intent_record(vector=vector, portfolio_state_record=portfolio_record, decision_group=groups[0])

    same_group_resolver = _resolver_for(groups[0])
    same_group = simulate_terminal_fill(verified, intent_record, portfolio_record, groups[0], same_group_resolver)
    same_group_receipt = _receipt(same_group)

    assert same_group_resolver.record_hits == list(groups[0].event_ids)
    assert same_group_resolver.byte_hits == []
    assert same_group_receipt["status"] == "REJECTED"
    assert same_group_receipt["reason_codes"] == ["FILL_SAME_OR_EARLIER_EVENT"]
    assert same_group_receipt["fill_event_id"] == groups[0].event_ids[0]
    assert same_group_receipt["filled_base_atoms"] == "0"
    assert same_group_receipt["released_quote_atoms"] == "101000"

    later_resolver = _resolver_for(groups[1])
    later = simulate_terminal_fill(verified, intent_record, portfolio_record, groups[1], later_resolver)
    repeated = simulate_terminal_fill(verified, intent_record, portfolio_record, groups[1], _resolver_for(groups[1]))
    later_receipt = _receipt(later)

    assert later == repeated
    assert later_resolver.record_hits == list(groups[1].event_ids)
    assert later_resolver.byte_hits == []
    assert later_receipt["status"] == "FILLED"
    assert later_receipt["reason_codes"] == []
    assert later_receipt["fill_event_id"] == groups[1].event_ids[0]
    assert later_receipt["fill_equal_time_group"] == "2"
    assert later_receipt["fill_ingest_sequence"] == "2"
    assert later_receipt["portfolio_state_before_id"] == parse_canonical_record(portfolio_record)["portfolio_state_id"]

    non_later_ingest_event = parse_canonical_record(groups[1].event_records[0])
    non_later_ingest_event["ingest_sequence"] = "1"
    non_later_ingest_group = _resealed_event_group(groups[1], non_later_ingest_event)
    non_later_ingest = _receipt(
        simulate_terminal_fill(
            verified,
            intent_record,
            portfolio_record,
            non_later_ingest_group,
            _resolver_for(non_later_ingest_group),
        )
    )
    assert non_later_ingest["status"] == "REJECTED"
    assert non_later_ingest["reason_codes"] == ["FILL_SAME_OR_EARLIER_EVENT"]
    assert non_later_ingest["fill_event_id"] == non_later_ingest_group.event_ids[0]
    assert non_later_ingest["fill_equal_time_group"] == "2"
    assert non_later_ingest["fill_ingest_sequence"] == "1"


def test_reserved_state_is_accepted_as_fill_and_accounting_before_state() -> None:
    """Breaks if the S0-bound intent cannot advance through reserved S1 to a terminal fill."""
    vector, verified, groups = _verified_groups()
    genesis = initialize_accounting(
        verified,
        quote_mint="synthetic-quote",
        quote_decimals=6,
        starting_quote_atoms=1_000_000,
    )
    intent_record = _intent_record(
        vector=vector,
        portfolio_state_record=genesis.portfolio_state_record,
        decision_group=groups[0],
    )
    reserved = reserve_intent(verified, genesis.store, genesis.portfolio_state_record, intent_record)

    fill = simulate_terminal_fill(
        verified,
        intent_record,
        reserved.portfolio_state_record,
        groups[1],
        _resolver_for(groups[1]),
    )
    fill_receipt = _receipt(fill)
    applied = apply_fill_receipt(
        verified,
        reserved.store,
        reserved.portfolio_state_record,
        intent_record,
        fill.fill_receipt_record,
    )
    reserved_state = parse_canonical_record(reserved.portfolio_state_record)
    final_state = parse_canonical_record(applied.portfolio_state_record)
    reconciliation = parse_canonical_record(applied.reconciliation_receipt_record)

    assert fill_receipt["portfolio_state_before_id"] == reserved_state["portfolio_state_id"]
    assert fill_receipt["status"] == "FILLED"
    assert reconciliation["status"] == "PASS"
    assert final_state["open_intent_ids"] == []
    assert len(applied.store.ledger_records) == 2


def test_terminal_fill_arithmetic_pins_full_partial_and_no_next_event() -> None:
    vector, verified, groups = _verified_groups()
    full_portfolio_record = _flat_portfolio_state_record(vector)
    full_intent_record = _intent_record(
        vector=vector,
        portfolio_state_record=full_portfolio_record,
        decision_group=groups[0],
        quantity_base_atoms="100000000",
        reserved_quote_atoms="101000",
    )
    partial_portfolio_record = _flat_portfolio_state_record(vector)
    partial_intent_record = _intent_record(
        vector=vector,
        portfolio_state_record=partial_portfolio_record,
        decision_group=groups[0],
        quantity_base_atoms="400000000",
        reserved_quote_atoms="500000",
    )

    full = _receipt(simulate_terminal_fill(verified, full_intent_record, full_portfolio_record, groups[1], _resolver_for(groups[1])))
    partial = _receipt(
        simulate_terminal_fill(verified, partial_intent_record, partial_portfolio_record, groups[1], _resolver_for(groups[1]))
    )
    expired = _receipt(simulate_terminal_fill(verified, full_intent_record, full_portfolio_record, None, _resolver_for()))

    assert full["status"] == "FILLED"
    assert full["execution_price_q18"] == "1002502005000000000"
    assert full["requested_base_atoms"] == "100000000"
    assert full["filled_base_atoms"] == "100000000"
    assert full["unfilled_base_atoms"] == "0"
    assert full["gross_quote_atoms"] == "100251"
    assert full["venue_fee_quote_atoms"] == "250"
    assert full["priority_fee_quote_atoms"] == "0"
    assert full["simulation_fee_quote_atoms"] == "0"
    assert full["cash_delta_quote_atoms"] == "-100501"
    assert full["released_quote_atoms"] == "499"
    assert full["released_base_atoms"] == "0"
    assert full["participation_bps"] == 200
    assert full["reference_deviation_bps"] == 25
    assert full["impact_bps"] == 25
    assert full["fee_bps"] == 25
    assert full["adverse_fill_bps"] == 0

    assert partial["status"] == "PARTIAL"
    assert partial["reason_codes"] == ["FILL_PARTIAL"]
    assert partial["execution_price_q18"] == "1002502005000000000"
    assert partial["requested_base_atoms"] == "400000000"
    assert partial["filled_base_atoms"] == "250000000"
    assert partial["unfilled_base_atoms"] == "150000000"
    assert partial["gross_quote_atoms"] == "250626"
    assert partial["venue_fee_quote_atoms"] == "625"
    assert partial["priority_fee_quote_atoms"] == "0"
    assert partial["simulation_fee_quote_atoms"] == "0"
    assert partial["cash_delta_quote_atoms"] == "-251251"
    assert partial["released_quote_atoms"] == "248749"
    assert partial["released_base_atoms"] == "0"
    assert partial["participation_bps"] == 500
    assert partial["reference_deviation_bps"] == 25
    assert partial["fee_bps"] == 25

    assert expired["status"] == "EXPIRED"
    assert expired["reason_codes"] == ["FILL_NO_NEXT_EVENT"]
    assert expired["fill_event_id"] is None
    assert expired["fill_ingest_sequence"] is None
    assert expired["fill_equal_time_group"] is None
    assert expired["fill_replay_clock_ns"] is None
    assert expired["filled_base_atoms"] == "0"
    assert expired["unfilled_base_atoms"] == "100000000"
    assert expired["gross_quote_atoms"] == "0"
    assert expired["venue_fee_quote_atoms"] == "0"
    assert expired["priority_fee_quote_atoms"] == "0"
    assert expired["simulation_fee_quote_atoms"] == "0"
    assert expired["cash_delta_quote_atoms"] == "0"
    assert expired["execution_price_q18"] is None
    assert expired["released_quote_atoms"] == "101000"
    assert expired["released_base_atoms"] == "0"

    extreme_event = parse_canonical_record(groups[1].event_records[0])
    extreme_event["base_decimals"] = 0
    extreme_event["quote_decimals"] = 18
    extreme_event["market"] = {
        **extreme_event["market"],
        "base_amount_atoms": "1",
        "quote_amount_atoms": "1000000000000000000",
        "route_capacity_base_atoms": "18446744073709551615",
        "liquidity_quote_atoms": "18446744073709551615",
        "venue_fee_quote_atoms": "0",
        "priority_fee_quote_atoms": "0",
        "route_impact_bps": 0,
    }
    extreme_group = _resealed_event_group(groups[1], extreme_event)
    overflow_intent = parse_canonical_record(full_intent_record)
    overflow_intent.pop("intent_id")
    overflow_intent.update(
        {
            "action": "CLOSE_LONG",
            "base_decimals": 0,
            "quote_decimals": 18,
            "quantity_base_atoms": "18446744073709551615",
            "reference_price_q18": "1000000000000000000",
            "stop_price_q18": "1",
            "take_price_q18": "2",
            "reserved_quote_atoms": "0",
            "reserved_base_atoms": "18446744073709551615",
            "max_participation_bps": 10000,
            "max_impact_bps": 0,
        }
    )
    overflow_intent_record = canonical_record_bytes(seal_replay_content_id(overflow_intent))
    overflow = _receipt(
        simulate_terminal_fill(
            verified,
            overflow_intent_record,
            full_portfolio_record,
            extreme_group,
            _resolver_for(extreme_group),
        )
    )
    assert overflow["status"] == "REJECTED"
    assert overflow["reason_codes"] == ["FILL_ARITHMETIC_RANGE"]
    assert overflow["fill_event_id"] == extreme_group.event_ids[0]
    assert overflow["filled_base_atoms"] == "0"
    assert overflow["gross_quote_atoms"] == "0"
    assert overflow["released_base_atoms"] == "18446744073709551615"


def test_authority_binding_failures_are_closed_and_in_memory_only() -> None:
    vector, verified, groups = _verified_groups()
    portfolio_record = _flat_portfolio_state_record(vector)
    intent_record = _intent_record(vector=vector, portfolio_state_record=portfolio_record, decision_group=groups[0])
    event = parse_canonical_record(groups[1].event_records[0])

    tampered_intent = parse_canonical_record(intent_record)
    tampered_intent["quantity_base_atoms"] = "1"
    wrong_schema_intent = parse_canonical_record(intent_record)
    wrong_schema_intent["schema"] = "trading.raw-event/v1"
    wrong_portfolio_record = _flat_portfolio_state_record(vector, quote_available="2000000")

    changed_event = copy.deepcopy(event)
    changed_event["market"] = {**changed_event["market"], "quote_amount_atoms": "1009999"}
    tampered_event_record = canonical_record_bytes(changed_event)
    tampered_event_group = replace(groups[1], event_records=(tampered_event_record,))
    mismatched_event = copy.deepcopy(changed_event)
    mismatched_event.pop("event_id")
    mismatched_event_group = replace(
        groups[1],
        event_records=(canonical_record_bytes(seal_replay_content_id(mismatched_event)),),
    )
    bad_seed_verified = ContractVerifiedRunInputs(
        authority="CONTRACT_ONLY",
        run_receipt=verified.run_receipt,
        bundle=verified.bundle,
        public_seed_bytes=b"\x00" * 32,
    )

    cases = (
        ("intent self-ID", verified, canonical_record_bytes(tampered_intent), portfolio_record, groups[1], _resolver_for(groups[1])),
        ("intent schema", verified, canonical_record_bytes(wrong_schema_intent), portfolio_record, groups[1], _resolver_for(groups[1])),
        ("portfolio binding", verified, intent_record, wrong_portfolio_record, groups[1], _resolver_for(groups[1])),
        ("event bytes", verified, intent_record, portfolio_record, mismatched_event_group, _resolver_for(groups[1])),
        (
            "event self-ID",
            verified,
            intent_record,
            portfolio_record,
            tampered_event_group,
            RecordingResolver({groups[1].event_ids[0]: tampered_event_record}),
        ),
        ("public seed binding", bad_seed_verified, intent_record, portfolio_record, groups[1], _resolver_for(groups[1])),
    )

    for label, run_inputs, candidate_intent, candidate_portfolio, candidate_group, resolver in cases:
        with pytest.raises(ValueError, match="self-ID|schema|portfolio|event|seed|resolved"):
            simulate_terminal_fill(
                run_inputs,
                candidate_intent,
                candidate_portfolio,
                candidate_group,
                resolver,
            ), label
        assert resolver.byte_hits == []
