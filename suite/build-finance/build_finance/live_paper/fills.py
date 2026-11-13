"""No-lookahead terminal fill simulation for the offline G2 paper kernel."""

from __future__ import annotations

import hashlib
from collections.abc import Mapping
from dataclasses import dataclass
from typing import Any, NoReturn, cast

from build_finance.crypto_replay.canonical import (
    JsonObject,
    canonical_json_bytes,
    canonical_record_bytes,
    parse_canonical_record,
    sha256_hex,
)
from build_finance.crypto_replay.content_ids import seal_content_id, verify_content_id
from build_finance.crypto_replay.run_inputs import ContractVerifiedRunInputs
from build_finance.crypto_replay.schema_registry import require_valid_contract
from build_finance.live_paper.grouping import EventGroup
from build_finance.live_paper.resolver import EvidenceResolver

_MAX_U64 = 18_446_744_073_709_551_615
_MIN_I128 = -170_141_183_460_469_231_731_687_303_715_884_105_728
_MAX_I128 = 170_141_183_460_469_231_731_687_303_715_884_105_727
_MAX_BPS_MEASURE = 2_147_483_647
_Q18 = 1_000_000_000_000_000_000
_BPS = 10_000


@dataclass(frozen=True, slots=True)
class TerminalFillEvidence:
    fill_receipt_record: bytes
    adverse_fill_draw_key_bytes: bytes
    adverse_fill_draw_bytes: bytes

    def __post_init__(self) -> None:
        object.__setattr__(self, "fill_receipt_record", bytes(self.fill_receipt_record))
        object.__setattr__(self, "adverse_fill_draw_key_bytes", bytes(self.adverse_fill_draw_key_bytes))
        object.__setattr__(self, "adverse_fill_draw_bytes", bytes(self.adverse_fill_draw_bytes))


class _ArithmeticRange(ValueError):
    """Raised internally when integer authority would exceed frozen aliases."""


class _MarketMismatch(ValueError):
    """Raised internally when event identity cannot satisfy the frozen intent."""


def _fail(message: str) -> NoReturn:
    raise ValueError(message)


def _checked_u64(value: int) -> int:
    if value < 0 or value > _MAX_U64:
        raise _ArithmeticRange("u64 arithmetic range exceeded")
    return value


def _checked_i128(value: int) -> int:
    if value < _MIN_I128 or value > _MAX_I128:
        raise _ArithmeticRange("i128 arithmetic range exceeded")
    return value


def _checked_bps(value: int) -> int:
    if value < 0 or value > _MAX_BPS_MEASURE:
        raise _ArithmeticRange("basis-point arithmetic range exceeded")
    return value


def _ceil_div(numerator: int, denominator: int) -> int:
    if numerator < 0 or denominator <= 0:
        raise _ArithmeticRange("ceiling division requires non-negative numerator and positive denominator")
    return (numerator + denominator - 1) // denominator


def _u64_text(value: object, *, field: str, positive: bool = False) -> int:
    if not isinstance(value, str) or not value.isascii() or not value.isdigit() or str(int(value)) != value:
        _fail(f"{field} must be canonical unsigned numeric text")
    parsed = int(value)
    if parsed > _MAX_U64 or (positive and parsed == 0):
        _fail(f"{field} is outside its required unsigned range")
    return parsed


def _parse_replay_record(record: bytes, *, expected_schema: str) -> JsonObject:
    try:
        document = parse_canonical_record(bytes(record))
        require_valid_contract(document, expected_schema=expected_schema)
        if not verify_content_id(document):
            _fail(f"{expected_schema} self-ID does not match its retained record")
    except (KeyError, TypeError, ValueError) as error:
        raise ValueError(f"{expected_schema} record failed closed validation: {error}") from error
    return document


def _require_verified_run(verified: ContractVerifiedRunInputs) -> tuple[Mapping[str, object], bytes]:
    if not isinstance(verified, ContractVerifiedRunInputs) or verified.authority != "CONTRACT_ONLY":
        _fail("fill simulation requires frozen contract-verified run inputs")
    run_receipt = verified.run_receipt
    seed = bytes(verified.public_seed_bytes)
    public_seed_hex = run_receipt.get("public_seed_hex")
    public_seed_sha256 = run_receipt.get("public_seed_sha256")
    bundle_seed = bytes(verified.bundle.public_seed_bytes)
    if (
        len(seed) != 32
        or bundle_seed != seed
        or public_seed_hex != seed.hex()
        or public_seed_sha256 != hashlib.sha256(seed).hexdigest()
    ):
        _fail("public seed bytes do not match the verified run receipt")
    run_receipt_id = run_receipt.get("run_receipt_id")
    if not isinstance(run_receipt_id, str) or len(run_receipt_id) != 64:
        _fail("verified run receipt identity is absent")
    return run_receipt, seed


def _resolved_group_events(
    selected_event_group: EventGroup,
    resolver: EvidenceResolver,
) -> tuple[JsonObject, ...]:
    if not isinstance(selected_event_group, EventGroup):
        _fail("selected_event_group must be an EventGroup or None")
    if not selected_event_group.event_ids or len(selected_event_group.event_ids) != len(selected_event_group.event_records):
        _fail("selected event group must carry one retained record per event ID")

    events: list[JsonObject] = []
    for event_id, declared_record in zip(
        selected_event_group.event_ids,
        selected_event_group.event_records,
        strict=True,
    ):
        try:
            resolved_record = bytes(resolver.resolve_record(event_id))
        except (KeyError, TypeError, ValueError) as error:
            raise ValueError(f"resolved event evidence is unavailable for {event_id}") from error
        if resolved_record != declared_record:
            _fail("resolved event bytes do not exactly match the selected group evidence")
        event = _parse_replay_record(resolved_record, expected_schema="trading.raw-event/v1")
        if event.get("event_id") != event_id:
            _fail("event self-ID does not match the selected group event ID")
        if event.get("equal_time_group") != selected_event_group.group_sequence:
            _fail("event equal-time group does not match selected group")
        revision = event.get("revision")
        if not isinstance(revision, Mapping) or revision.get("availability_slot") != selected_event_group.availability_slot:
            _fail("event availability slot does not match selected group")
        events.append(event)

    ordered = tuple(sorted(events, key=lambda event: _u64_text(event["ingest_sequence"], field="ingest_sequence")))
    if ordered != tuple(events):
        _fail("selected event records must be retained in ingest order")
    return tuple(events)


def _event_price_q18(event: Mapping[str, Any], *, intent: Mapping[str, Any]) -> int:
    market = event["market"]
    if not isinstance(market, Mapping):
        _fail("fill event market payload is absent")
    if (
        event["market_id"] != intent["market_id"]
        or event["base_mint"] != intent["base_mint"]
        or event["quote_mint"] != intent["quote_mint"]
        or event["base_decimals"] != intent["base_decimals"]
        or event["quote_decimals"] != intent["quote_decimals"]
    ):
        raise _MarketMismatch("event market identity or decimals do not match the intent")
    base_atoms = _u64_text(market["base_amount_atoms"], field="base_amount_atoms", positive=True)
    quote_atoms = _u64_text(market["quote_amount_atoms"], field="quote_amount_atoms", positive=True)
    base_decimals = cast(int, event["base_decimals"])
    quote_decimals = cast(int, event["quote_decimals"])
    return _checked_i128((quote_atoms * 10 ** (base_decimals + 18)) // (base_atoms * 10**quote_decimals))


def _reservation_release(intent: Mapping[str, Any]) -> tuple[int, int]:
    if intent["action"] == "OPEN_LONG":
        return _u64_text(intent["reserved_quote_atoms"], field="reserved_quote_atoms"), 0
    return 0, _u64_text(intent["reserved_base_atoms"], field="reserved_base_atoms")


def _require_portfolio_binding(intent: Mapping[str, Any], portfolio: Mapping[str, Any]) -> None:
    """Accept S0 or prove that S1 is the exact reservation descendant of S0."""
    if intent["portfolio_state_before_id"] == portfolio["portfolio_state_id"]:
        return
    intent_id = intent["intent_id"]
    if (
        portfolio["previous_portfolio_state_id"] != intent["portfolio_state_before_id"]
        or portfolio["causation_schema"] != "trading.simulated-order-intent/v1"
        or portfolio["causation_id"] != intent_id
        or portfolio["open_intent_ids"] != [intent_id]
        or portfolio["as_of_ingest_sequence"] != intent["decision_ingest_sequence"]
        or portfolio["equal_time_group"] != intent["decision_equal_time_group"]
        or portfolio["replay_clock_ns"] != intent["created_replay_clock_ns"]
        or portfolio["config_admission_receipt_id"] != intent["config_admission_receipt_id"]
        or portfolio["validated_config_sha256"] != intent["validated_config_sha256"]
        or portfolio["quote_mint"] != intent["quote_mint"]
        or portfolio["quote_decimals"] != intent["quote_decimals"]
        or _u64_text(portfolio["state_sequence"], field="portfolio.state_sequence") == 0
    ):
        _fail("portfolio state is not the intent's exact reservation descendant")

    balances = {
        row["mint"]: row
        for row in cast(list[Mapping[str, Any]], portfolio["balances"])
        if isinstance(row.get("mint"), str)
    }
    if intent["action"] == "OPEN_LONG":
        quote = balances.get(intent["quote_mint"])
        if quote is None or quote["reserved_atoms"] != intent["reserved_quote_atoms"]:
            _fail("reserved portfolio does not carry the intent's exact quote reservation")
        return

    base = balances.get(intent["base_mint"])
    position = next(
        (
            row
            for row in cast(list[Mapping[str, Any]], portfolio["positions"])
            if row.get("market_id") == intent["market_id"]
        ),
        None,
    )
    if (
        base is None
        or position is None
        or base["reserved_atoms"] != intent["reserved_base_atoms"]
        or position["reserved_base_atoms"] != intent["reserved_base_atoms"]
    ):
        _fail("reserved portfolio does not carry the intent's exact base reservation")


def _draw_evidence(
    *,
    run_receipt: Mapping[str, object],
    public_seed: bytes,
    intent: Mapping[str, Any],
    fill_event_id: str | None,
    fill_equal_time_group: str | None,
) -> tuple[bytes, bytes, str, str]:
    key = {
        "schema": "trading.adverse-fill-draw-key/v1",
        "run_receipt_id": cast(str, run_receipt["run_receipt_id"]),
        "intent_id": intent["intent_id"],
        "fill_event_id": fill_event_id,
        "decision_equal_time_group": intent["decision_equal_time_group"],
        "fill_equal_time_group": fill_equal_time_group,
        "requested_base_atoms": intent["quantity_base_atoms"],
        "max_adverse_fill_bps": 0,
    }
    require_valid_contract(key, expected_schema="trading.adverse-fill-draw-key/v1")
    key_bytes = canonical_json_bytes(key)
    key_sha256 = sha256_hex(key_bytes)
    draw_bytes = hashlib.sha256(
        b"trading.adverse-fill-draw/v1"
        + b"\x00"
        + public_seed
        + bytes.fromhex(key_sha256)
        + (0).to_bytes(8, byteorder="big")
    ).digest()
    return key_bytes, draw_bytes, key_sha256, sha256_hex(draw_bytes)


def _receipt_evidence(
    *,
    run_receipt: Mapping[str, object],
    public_seed: bytes,
    intent: Mapping[str, Any],
    portfolio: Mapping[str, Any],
    status: str,
    reason_codes: tuple[str, ...],
    fill_event: Mapping[str, Any] | None,
    requested_base_atoms: int,
    filled_base_atoms: int,
    unfilled_base_atoms: int,
    gross_quote_atoms: int,
    venue_fee_quote_atoms: int,
    priority_fee_quote_atoms: int,
    simulation_fee_quote_atoms: int,
    cash_delta_quote_atoms: int,
    execution_price_q18: int | None,
    participation_bps: int,
    reference_deviation_bps: int,
    impact_bps: int,
    fee_bps: int,
    adverse_fill_bps: int,
    released_quote_atoms: int,
    released_base_atoms: int,
) -> TerminalFillEvidence:
    fill_event_id = None if fill_event is None else cast(str, fill_event["event_id"])
    fill_equal_time_group = None if fill_event is None else cast(str, fill_event["equal_time_group"])
    key_bytes, draw_bytes, key_sha256, draw_sha256 = _draw_evidence(
        run_receipt=run_receipt,
        public_seed=public_seed,
        intent=intent,
        fill_event_id=fill_event_id,
        fill_equal_time_group=fill_equal_time_group,
    )
    receipt = seal_content_id(
        {
            "schema": "trading.simulated-fill-receipt/v1",
            "receipt_sequence": intent["intent_sequence"],
            "decision_sequence": intent["decision_sequence"],
            "intent_id": intent["intent_id"],
            "portfolio_state_before_id": portfolio["portfolio_state_id"],
            "status": status,
            "reason_codes": list(reason_codes),
            "fill_event_id": fill_event_id,
            "decision_ingest_sequence": intent["decision_ingest_sequence"],
            "decision_equal_time_group": intent["decision_equal_time_group"],
            "fill_ingest_sequence": None if fill_event is None else fill_event["ingest_sequence"],
            "fill_equal_time_group": fill_equal_time_group,
            "fill_replay_clock_ns": None if fill_event is None else fill_event["replay_clock_ns"],
            "requested_base_atoms": str(_checked_u64(requested_base_atoms)),
            "filled_base_atoms": str(_checked_u64(filled_base_atoms)),
            "unfilled_base_atoms": str(_checked_u64(unfilled_base_atoms)),
            "gross_quote_atoms": str(_checked_u64(gross_quote_atoms)),
            "venue_fee_quote_atoms": str(_checked_u64(venue_fee_quote_atoms)),
            "priority_fee_quote_atoms": str(_checked_u64(priority_fee_quote_atoms)),
            "simulation_fee_quote_atoms": str(_checked_u64(simulation_fee_quote_atoms)),
            "cash_delta_quote_atoms": str(_checked_i128(cash_delta_quote_atoms)),
            "execution_price_q18": None if execution_price_q18 is None else str(_checked_i128(execution_price_q18)),
            "participation_bps": _checked_bps(participation_bps),
            "reference_deviation_bps": _checked_bps(reference_deviation_bps),
            "impact_bps": _checked_bps(impact_bps),
            "fee_bps": _checked_bps(fee_bps),
            "adverse_fill_bps": _checked_bps(adverse_fill_bps),
            "adverse_fill_draw_key_sha256": key_sha256,
            "adverse_fill_draw_sha256": draw_sha256,
            "released_quote_atoms": str(_checked_u64(released_quote_atoms)),
            "released_base_atoms": str(_checked_u64(released_base_atoms)),
        }
    )
    require_valid_contract(receipt, expected_schema="trading.simulated-fill-receipt/v1")
    if not verify_content_id(receipt):
        _fail("simulated fill receipt self-ID does not match its canonical body")
    return TerminalFillEvidence(
        fill_receipt_record=canonical_record_bytes(receipt),
        adverse_fill_draw_key_bytes=key_bytes,
        adverse_fill_draw_bytes=draw_bytes,
    )


def _terminal_denial(
    *,
    run_receipt: Mapping[str, object],
    public_seed: bytes,
    intent: Mapping[str, Any],
    portfolio: Mapping[str, Any],
    status: str,
    reason: str,
    fill_event: Mapping[str, Any] | None,
) -> TerminalFillEvidence:
    requested = _u64_text(intent["quantity_base_atoms"], field="quantity_base_atoms", positive=True)
    released_quote, released_base = _reservation_release(intent)
    return _receipt_evidence(
        run_receipt=run_receipt,
        public_seed=public_seed,
        intent=intent,
        portfolio=portfolio,
        status=status,
        reason_codes=(reason,),
        fill_event=fill_event,
        requested_base_atoms=requested,
        filled_base_atoms=0,
        unfilled_base_atoms=requested,
        gross_quote_atoms=0,
        venue_fee_quote_atoms=0,
        priority_fee_quote_atoms=0,
        simulation_fee_quote_atoms=0,
        cash_delta_quote_atoms=0,
        execution_price_q18=None,
        participation_bps=0,
        reference_deviation_bps=0,
        impact_bps=0,
        fee_bps=0,
        adverse_fill_bps=0,
        released_quote_atoms=released_quote,
        released_base_atoms=released_base,
    )


def _terminal_fill(
    *,
    run_receipt: Mapping[str, object],
    public_seed: bytes,
    intent: Mapping[str, Any],
    portfolio: Mapping[str, Any],
    event: Mapping[str, Any],
) -> TerminalFillEvidence:
    if event["executable"] is not True:
        return _terminal_denial(
            run_receipt=run_receipt,
            public_seed=public_seed,
            intent=intent,
            portfolio=portfolio,
            status="REJECTED",
            reason="FILL_NEXT_EVENT_NON_EXECUTABLE",
            fill_event=event,
        )
    market = event["market"]
    assert isinstance(market, Mapping)
    try:
        requested = _u64_text(intent["quantity_base_atoms"], field="quantity_base_atoms", positive=True)
        price_q18 = _event_price_q18(event, intent=intent)
        route_capacity = _u64_text(market["route_capacity_base_atoms"], field="route_capacity_base_atoms")
        max_participation_bps = cast(int, intent["max_participation_bps"])
        capacity = min(route_capacity, (route_capacity * max_participation_bps) // _BPS)
        if capacity <= 0:
            return _terminal_denial(
                run_receipt=run_receipt,
                public_seed=public_seed,
                intent=intent,
                portfolio=portfolio,
                status="REJECTED",
                reason="FILL_ZERO_CAPACITY",
                fill_event=event,
            )
        impact_bps = cast(int, market["route_impact_bps"])
        if impact_bps > cast(int, intent["max_impact_bps"]):
            return _terminal_denial(
                run_receipt=run_receipt,
                public_seed=public_seed,
                intent=intent,
                portfolio=portfolio,
                status="REJECTED",
                reason="FILL_IMPACT",
                fill_event=event,
            )
        adverse_fill_bps = 0
        total_deviation_bps = impact_bps + adverse_fill_bps
        action = intent["action"]
        if action == "OPEN_LONG":
            execution_price = _ceil_div(price_q18 * (_BPS + total_deviation_bps), _BPS)
        else:
            execution_price = (price_q18 * (_BPS - total_deviation_bps)) // _BPS
        if execution_price <= 0:
            return _terminal_denial(
                run_receipt=run_receipt,
                public_seed=public_seed,
                intent=intent,
                portfolio=portfolio,
                status="REJECTED",
                reason="FILL_IMPACT",
                fill_event=event,
            )
        filled = min(requested, capacity)
        unfilled = requested - filled
        event_base_atoms = _u64_text(market["base_amount_atoms"], field="base_amount_atoms", positive=True)
        quote_scale = 10 ** cast(int, event["quote_decimals"])
        base_scale = 10 ** cast(int, event["base_decimals"])
        gross_numerator = filled * execution_price * quote_scale
        gross_denominator = _Q18 * base_scale
        gross = _ceil_div(gross_numerator, gross_denominator) if action == "OPEN_LONG" else gross_numerator // gross_denominator
        venue_fee = _ceil_div(
            _u64_text(market["venue_fee_quote_atoms"], field="venue_fee_quote_atoms") * filled,
            event_base_atoms,
        )
        priority_fee = _ceil_div(
            _u64_text(market["priority_fee_quote_atoms"], field="priority_fee_quote_atoms") * filled,
            event_base_atoms,
        )
        simulation_fee = 0
        fees = venue_fee + priority_fee + simulation_fee
        if action == "OPEN_LONG":
            spent = gross + fees
            reserved_quote = _u64_text(intent["reserved_quote_atoms"], field="reserved_quote_atoms")
            if spent > reserved_quote:
                return _terminal_denial(
                    run_receipt=run_receipt,
                    public_seed=public_seed,
                    intent=intent,
                    portfolio=portfolio,
                    status="REJECTED",
                    reason="FILL_INSUFFICIENT_RESERVATION",
                    fill_event=event,
                )
            cash_delta = -spent
            released_quote = reserved_quote - spent
            released_base = 0
        else:
            if fees > gross:
                return _terminal_denial(
                    run_receipt=run_receipt,
                    public_seed=public_seed,
                    intent=intent,
                    portfolio=portfolio,
                    status="REJECTED",
                    reason="FILL_FEE_EXCEEDS_PROCEEDS",
                    fill_event=event,
                )
            reserved_base = _u64_text(intent["reserved_base_atoms"], field="reserved_base_atoms")
            if filled > reserved_base:
                return _terminal_denial(
                    run_receipt=run_receipt,
                    public_seed=public_seed,
                    intent=intent,
                    portfolio=portfolio,
                    status="REJECTED",
                    reason="FILL_INSUFFICIENT_RESERVATION",
                    fill_event=event,
                )
            cash_delta = gross - fees
            released_quote = 0
            released_base = reserved_base - filled
        status = "FILLED" if filled == requested else "PARTIAL"
        reason_codes: tuple[str, ...] = () if status == "FILLED" else ("FILL_PARTIAL",)
        fee_bps = 0 if gross == 0 else _ceil_div(fees * _BPS, gross)
        participation_bps = _ceil_div(filled * _BPS, route_capacity) if route_capacity > 0 else 0
    except _MarketMismatch:
        return _terminal_denial(
            run_receipt=run_receipt,
            public_seed=public_seed,
            intent=intent,
            portfolio=portfolio,
            status="REJECTED",
            reason="FILL_DECIMALS_MISMATCH",
            fill_event=event,
        )
    except _ArithmeticRange:
        return _terminal_denial(
            run_receipt=run_receipt,
            public_seed=public_seed,
            intent=intent,
            portfolio=portfolio,
            status="REJECTED",
            reason="FILL_ARITHMETIC_RANGE",
            fill_event=event,
        )

    try:
        return _receipt_evidence(
            run_receipt=run_receipt,
            public_seed=public_seed,
            intent=intent,
            portfolio=portfolio,
            status=status,
            reason_codes=reason_codes,
            fill_event=event,
            requested_base_atoms=requested,
            filled_base_atoms=filled,
            unfilled_base_atoms=unfilled,
            gross_quote_atoms=gross,
            venue_fee_quote_atoms=venue_fee,
            priority_fee_quote_atoms=priority_fee,
            simulation_fee_quote_atoms=simulation_fee,
            cash_delta_quote_atoms=cash_delta,
            execution_price_q18=execution_price,
            participation_bps=participation_bps,
            reference_deviation_bps=total_deviation_bps,
            impact_bps=impact_bps,
            fee_bps=fee_bps,
            adverse_fill_bps=adverse_fill_bps,
            released_quote_atoms=released_quote,
            released_base_atoms=released_base,
        )
    except _ArithmeticRange:
        return _terminal_denial(
            run_receipt=run_receipt,
            public_seed=public_seed,
            intent=intent,
            portfolio=portfolio,
            status="REJECTED",
            reason="FILL_ARITHMETIC_RANGE",
            fill_event=event,
        )


def simulate_terminal_fill(
    verified: ContractVerifiedRunInputs,
    intent_record: bytes,
    portfolio_state_before_fill_record: bytes,
    selected_event_group: EventGroup | None,
    resolver: EvidenceResolver,
) -> TerminalFillEvidence:
    """Return one terminal fill receipt plus deterministic adverse-draw evidence."""
    run_receipt, public_seed = _require_verified_run(verified)
    intent = _parse_replay_record(bytes(intent_record), expected_schema="trading.simulated-order-intent/v1")
    portfolio = _parse_replay_record(
        bytes(portfolio_state_before_fill_record),
        expected_schema="trading.portfolio-state/v1",
    )
    _require_portfolio_binding(intent, portfolio)

    if selected_event_group is None:
        return _terminal_denial(
            run_receipt=run_receipt,
            public_seed=public_seed,
            intent=intent,
            portfolio=portfolio,
            status="EXPIRED",
            reason="FILL_NO_NEXT_EVENT",
            fill_event=None,
        )

    events = _resolved_group_events(selected_event_group, resolver)
    fill_event = events[0]
    decision_group = _u64_text(intent["decision_equal_time_group"], field="decision_equal_time_group", positive=True)
    decision_ingest = _u64_text(intent["decision_ingest_sequence"], field="decision_ingest_sequence", positive=True)
    fill_ingest = _u64_text(fill_event["ingest_sequence"], field="fill_event.ingest_sequence", positive=True)
    selected_group = _u64_text(selected_event_group.group_sequence, field="selected_event_group.group_sequence", positive=True)
    if selected_group <= decision_group:
        return _terminal_denial(
            run_receipt=run_receipt,
            public_seed=public_seed,
            intent=intent,
            portfolio=portfolio,
            status="REJECTED",
            reason="FILL_SAME_OR_EARLIER_EVENT",
            fill_event=fill_event,
        )
    if fill_ingest <= decision_ingest:
        return _terminal_denial(
            run_receipt=run_receipt,
            public_seed=public_seed,
            intent=intent,
            portfolio=portfolio,
            status="REJECTED",
            reason="FILL_SAME_OR_EARLIER_EVENT",
            fill_event=fill_event,
        )
    if selected_group != decision_group + 1:
        _fail("STRICT_NEXT_EVENT/ONE_EVENT_GROUP requires the immediate next event group")
    return _terminal_fill(
        run_receipt=run_receipt,
        public_seed=public_seed,
        intent=intent,
        portfolio=portfolio,
        event=fill_event,
    )
