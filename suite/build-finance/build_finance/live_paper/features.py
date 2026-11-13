"""Causal fixed-point feature snapshots for the offline G2 paper kernel."""

from __future__ import annotations

import hashlib
from collections.abc import Mapping, Sequence
from typing import Any, cast

from build_finance.crypto_replay.canonical import JsonObject, canonical_record_bytes, parse_canonical_record
from build_finance.crypto_replay.content_ids import seal_content_id, verify_content_id
from build_finance.crypto_replay.schema_registry import require_valid_contract
from build_finance.live_paper.grouping import EventGroup
from build_finance.live_paper.registry import require_valid_contract as require_valid_live_contract

_FEATURE_NAMES = (
    "atr_14_price_q18",
    "breakout_high_20_price_q18",
    "breakout_low_20_price_q18",
    "ema_fast_price_q18",
    "ema_slow_price_q18",
    "liquidity_quote_atoms",
    "mid_price_q18",
    "return_1_q18",
    "route_impact_bps",
    "rsi_14_q18",
    "stale_age_ns",
    "volume_20_base_atoms",
)
_HEX = "0123456789abcdef"
_Q18 = 1_000_000_000_000_000_000
_MAX_U64 = 18_446_744_073_709_551_615


def _u64(value: object, *, field: str, positive: bool = False) -> int:
    if not isinstance(value, str) or not value.isascii() or not value.isdigit() or str(int(value)) != value:
        raise ValueError(f"{field} must be canonical unsigned numeric text")
    parsed = int(value)
    if parsed > _MAX_U64 or (positive and parsed == 0):
        raise ValueError(f"{field} is outside its required unsigned range")
    return parsed


def _sha256(value: object, *, field: str) -> str:
    if not isinstance(value, str) or len(value) != 64 or any(character not in _HEX for character in value):
        raise ValueError(f"{field} must be a lowercase SHA-256 identity")
    return value


def _merkle_root(event_ids: Sequence[str]) -> str:
    if not event_ids:
        raise ValueError("feature history must contain at least one committed event")
    try:
        level = [hashlib.sha256(b"\x00" + bytes.fromhex(event_id)).digest() for event_id in event_ids]
    except ValueError as error:
        raise ValueError("feature history contains a malformed event ID") from error
    if any(len(event_id) != 64 for event_id in event_ids):
        raise ValueError("feature history contains a malformed event ID")
    while len(level) > 1:
        if len(level) % 2:
            level.append(level[-1])
        level = [
            hashlib.sha256(b"\x01" + level[index] + level[index + 1]).digest()
            for index in range(0, len(level), 2)
        ]
    return level[0].hex()


def _parse_event(record: bytes) -> JsonObject:
    document = parse_canonical_record(bytes(record))
    require_valid_contract(document, expected_schema="trading.raw-event/v1")
    if not verify_content_id(document):
        raise ValueError("raw-event self-ID does not match its retained record")
    return document


def _parse_group_receipt(record: bytes) -> dict[str, Any]:
    document = parse_canonical_record(bytes(record))
    require_valid_live_contract(document, expected_schema="trading.normalization-receipt/v1")
    return cast(dict[str, Any], document)


def _group_events(group: EventGroup) -> tuple[JsonObject, ...]:
    events = tuple(_parse_event(record) for record in group.event_records)
    event_ids = tuple(cast(str, event["event_id"]) for event in events)
    if event_ids != group.event_ids:
        raise ValueError("event-group IDs do not exactly match retained event records")
    if tuple(sorted(events, key=lambda event: _u64(event["ingest_sequence"], field="ingest_sequence"))) != events:
        raise ValueError("event-group records are not in verified ingest order")
    for event in events:
        revision = event["revision"]
        assert isinstance(revision, Mapping)
        if event["equal_time_group"] != group.group_sequence or revision["availability_slot"] != group.availability_slot:
            raise ValueError("event record does not belong to its declared group")

    receipt = _parse_group_receipt(group.normalization_receipt_record)
    source_ids: list[str] = []
    for event in events:
        source_id = cast(str, event["source_admission_receipt_id"])
        if source_id not in source_ids:
            source_ids.append(source_id)
    if receipt["input_content_ids"] != source_ids or receipt["normalized_event_ids"] != list(event_ids):
        raise ValueError("group normalization receipt does not exactly bind retained membership")
    if receipt["output_merkle_root_sha256"] != _merkle_root(event_ids):
        raise ValueError("group normalization receipt output root does not match retained membership")
    if receipt["status"] != "PASS" or receipt["reason_codes"] != []:
        raise ValueError("feature derivation requires a passing group-total normalization receipt")
    return events


def _truncating_division(numerator: int, denominator: int) -> int:
    if denominator <= 0:
        raise ValueError("fixed-point denominator must be positive")
    magnitude = abs(numerator) // denominator
    return -magnitude if numerator < 0 else magnitude


def _mid_price_q18(event: Mapping[str, Any]) -> int:
    market = event["market"]
    assert isinstance(market, Mapping)
    base_atoms = _u64(market["base_amount_atoms"], field="base_amount_atoms", positive=True)
    quote_atoms = _u64(market["quote_amount_atoms"], field="quote_amount_atoms", positive=True)
    base_decimals = event["base_decimals"]
    quote_decimals = event["quote_decimals"]
    if type(base_decimals) is not int or type(quote_decimals) is not int:
        raise ValueError("asset decimals must be exact integers")
    numerator = quote_atoms * 10 ** (base_decimals + 18)
    denominator = base_atoms * 10**quote_decimals
    return _truncating_division(numerator, denominator)


def _apply_revisions(events: Sequence[JsonObject]) -> tuple[JsonObject, ...]:
    seen: dict[str, JsonObject] = {}
    active: dict[str, JsonObject] = {}
    for event in events:
        event_id = cast(str, event["event_id"])
        if event_id in seen:
            raise ValueError("feature history contains a duplicate committed event")
        revision = event["revision"]
        assert isinstance(revision, Mapping)
        kind = revision["kind"]
        if kind == "ORIGINAL":
            active[event_id] = event
        else:
            target_field = "supersedes_event_id" if kind == "CORRECTION" else "retracts_event_id"
            target = revision[target_field]
            if not isinstance(target, str) or target not in active:
                raise ValueError(f"{kind} target must identify an active earlier event")
            if active[target]["market_id"] != event["market_id"]:
                raise ValueError(f"{kind} target must belong to the same market")
            active.pop(target)
            if kind == "CORRECTION":
                active[event_id] = event
        seen[event_id] = event
    return tuple(sorted(active.values(), key=lambda event: _u64(event["ingest_sequence"], field="ingest_sequence")))


def derive_feature_snapshot(committed_history: Sequence[EventGroup]) -> bytes:
    """Seal one frozen FeatureSnapshot from causal committed group history."""
    groups = tuple(committed_history)
    if not groups:
        raise ValueError("feature derivation requires at least one committed group")

    all_events: list[JsonObject] = []
    feature_code: str | None = None
    prior_slot = -1
    for index, group in enumerate(groups, start=1):
        sequence = _u64(group.group_sequence, field="group_sequence", positive=True)
        slot = _u64(group.availability_slot, field="availability_slot", positive=True)
        if sequence != index or slot <= prior_slot:
            raise ValueError("committed groups must be contiguous and monotone")
        prior_slot = slot
        code = _sha256(group.feature_code_sha256, field="feature_code_sha256")
        if feature_code is None:
            feature_code = code
        elif code != feature_code:
            raise ValueError("committed groups do not bind one feature-code identity")
        all_events.extend(_group_events(group))

    markets = {cast(str, event["market_id"]) for event in all_events}
    identities = {
        (
            event["market_id"],
            event["base_mint"],
            event["quote_mint"],
            event["base_decimals"],
            event["quote_decimals"],
        )
        for event in all_events
    }
    if len(markets) != 1 or len(identities) != 1:
        raise ValueError("G2 feature history requires one exact market identity")

    active = _apply_revisions(all_events)
    executable = tuple(event for event in active if event["executable"] is True)
    latest_committed = all_events[-1]
    features: dict[str, Any] = {
        "mid_price_q18": None,
        "return_1_q18": None,
        "ema_fast_price_q18": None,
        "ema_slow_price_q18": None,
        "rsi_14_q18": None,
        "atr_14_price_q18": None,
        "breakout_high_20_price_q18": None,
        "breakout_low_20_price_q18": None,
        "volume_20_base_atoms": None,
        "liquidity_quote_atoms": None,
        "stale_age_ns": None,
        "route_impact_bps": None,
        "history_count": len(executable),
    }
    if executable:
        latest_active = executable[-1]
        latest_mid = _mid_price_q18(latest_active)
        latest_market = latest_active["market"]
        assert isinstance(latest_market, Mapping)
        current_clock = _u64(latest_committed["replay_clock_ns"], field="replay_clock_ns")
        active_clock = _u64(latest_active["replay_clock_ns"], field="replay_clock_ns")
        if active_clock > current_clock:
            raise ValueError("active event cannot be newer than committed feature time")
        features["mid_price_q18"] = str(latest_mid)
        features["liquidity_quote_atoms"] = latest_market["liquidity_quote_atoms"]
        features["route_impact_bps"] = latest_market["route_impact_bps"]
        features["stale_age_ns"] = str(current_clock - active_clock)
        if len(executable) >= 2:
            previous_mid = _mid_price_q18(executable[-2])
            features["return_1_q18"] = str(_truncating_division((latest_mid - previous_mid) * _Q18, previous_mid))

    event_ids = tuple(cast(str, event["event_id"]) for event in all_events)
    identity = next(iter(identities))
    snapshot = seal_content_id(
        {
            "schema": "trading.feature-snapshot/v1",
            "feature_set_version": "solana-jupiter-deterministic-features/v1",
            "feature_code_sha256": cast(str, feature_code),
            "input_merkle_root_sha256": _merkle_root(event_ids),
            "market_id": identity[0],
            "base_mint": identity[1],
            "quote_mint": identity[2],
            "base_decimals": identity[3],
            "quote_decimals": identity[4],
            "decision_sequence": groups[-1].group_sequence,
            "as_of_event_id": latest_committed["event_id"],
            "as_of_admission_sequence": latest_committed["admission_sequence"],
            "as_of_ingest_sequence": latest_committed["ingest_sequence"],
            "equal_time_group": groups[-1].group_sequence,
            "replay_clock_ns": latest_committed["replay_clock_ns"],
            "event_time": latest_committed["event_time"],
            "observed_at": latest_committed["observed_at"],
            "ingested_at": latest_committed["ingested_at"],
            "features": features,
            "missing_features": [name for name in _FEATURE_NAMES if features[name] is None],
        }
    )
    require_valid_contract(snapshot, expected_schema="trading.feature-snapshot/v1")
    return canonical_record_bytes(snapshot)
