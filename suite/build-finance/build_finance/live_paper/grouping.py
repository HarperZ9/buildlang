"""Deterministic grouping of already-verified offline market events."""

from __future__ import annotations

import hashlib
from collections.abc import Mapping
from dataclasses import dataclass
from typing import cast

from build_finance.crypto_replay.canonical import JsonObject, JsonValue, canonical_record_bytes
from build_finance.crypto_replay.run_inputs import ContractVerifiedRunInputs
from build_finance.live_paper.content_ids import canonical_record_bytes as live_record_bytes
from build_finance.live_paper.content_ids import seal_content_id as seal_live_content_id
from build_finance.live_paper.registry import require_valid_contract as require_valid_live_contract


@dataclass(frozen=True, slots=True)
class EventGroup:
    """One verified availability group with exact retained evidence records."""

    group_sequence: str
    event_ids: tuple[str, ...]
    availability_slot: str
    event_records: tuple[bytes, ...]
    normalization_receipt_record: bytes
    feature_code_sha256: str

    def __post_init__(self) -> None:
        object.__setattr__(self, "event_ids", tuple(self.event_ids))
        object.__setattr__(self, "event_records", tuple(bytes(record) for record in self.event_records))
        object.__setattr__(self, "normalization_receipt_record", bytes(self.normalization_receipt_record))


def _json_value(value: object) -> JsonValue:
    if isinstance(value, Mapping):
        return {str(key): _json_value(item) for key, item in value.items()}
    if isinstance(value, (list, tuple)):
        return [_json_value(item) for item in value]
    if value is None or type(value) in {bool, int, str}:
        return cast(JsonValue, value)
    raise ValueError(f"verified event contains unsupported canonical value: {type(value).__name__}")


def _event_document(value: Mapping[str, JsonValue]) -> JsonObject:
    document = _json_value(value)
    if not isinstance(document, dict):
        raise ValueError("verified event must be an object")
    return document


def _numeric_text(value: object, *, field: str) -> int:
    if not isinstance(value, str) or not value.isascii() or not value.isdigit() or str(int(value)) != value:
        raise ValueError(f"{field} must be canonical unsigned numeric text")
    return int(value)


def _merkle_root(event_ids: tuple[str, ...]) -> str:
    if not event_ids:
        raise ValueError("an event group cannot be empty")
    try:
        level = [hashlib.sha256(b"\x00" + bytes.fromhex(event_id)).digest() for event_id in event_ids]
    except ValueError as error:
        raise ValueError("event group contains a malformed event ID") from error
    if any(len(event_id) != 64 for event_id in event_ids):
        raise ValueError("event group contains a malformed event ID")
    while len(level) > 1:
        if len(level) % 2:
            level.append(level[-1])
        level = [
            hashlib.sha256(b"\x01" + level[index] + level[index + 1]).digest()
            for index in range(0, len(level), 2)
        ]
    return level[0].hex()


def _group_receipt(
    group_sequence: str,
    events: tuple[JsonObject, ...],
    normalization_code_sha256: str,
) -> bytes:
    event_ids = tuple(cast(str, event["event_id"]) for event in events)
    input_ids: list[str] = []
    for event in events:
        source_id = cast(str, event["source_admission_receipt_id"])
        if source_id not in input_ids:
            input_ids.append(source_id)
    receipt = seal_live_content_id(
        {
            "schema": "trading.normalization-receipt/v1",
            "source_batch_id": f"g2-decision-group-{group_sequence}",
            "normalization_code_sha256": normalization_code_sha256,
            "input_content_ids": input_ids,
            "input_count": str(len(input_ids)),
            "normalized_event_ids": list(event_ids),
            "normalized_event_count": str(len(event_ids)),
            "output_merkle_root_sha256": _merkle_root(event_ids),
            "status": "PASS",
            "reason_codes": [],
            "total_evidence": "TOTAL_INPUT_CLOSURE",
        }
    )
    require_valid_live_contract(receipt, expected_schema="trading.normalization-receipt/v1")
    return live_record_bytes(receipt)


def group_committed_events(verified: ContractVerifiedRunInputs) -> tuple[EventGroup, ...]:
    """Group verified events in the exact verified availability-schedule order."""
    if not isinstance(verified, ContractVerifiedRunInputs) or verified.authority != "CONTRACT_ONLY":
        raise ValueError("grouping requires frozen contract-verified run inputs")
    run = verified.run_receipt
    normalization_code = run.get("normalization_code_sha256")
    feature_code = run.get("feature_code_sha256")
    if not isinstance(normalization_code, str) or not isinstance(feature_code, str):
        raise ValueError("verified run is missing deterministic code identities")

    schedule = verified.bundle.availability_schedule.get("availability_groups")
    if not isinstance(schedule, (list, tuple)):
        raise ValueError("verified availability schedule has no groups")
    events = tuple(_event_document(event) for event in verified.bundle.normalized_events)
    grouped: list[EventGroup] = []
    consumed: set[str] = set()
    for row in schedule:
        if not isinstance(row, Mapping):
            raise ValueError("verified availability group must be an object")
        group_sequence = row.get("equal_time_group")
        availability_slot = row.get("availability_slot")
        _numeric_text(group_sequence, field="equal_time_group")
        _numeric_text(availability_slot, field="availability_slot")
        assert isinstance(group_sequence, str) and isinstance(availability_slot, str)
        members = tuple(
            sorted(
                (event for event in events if event.get("equal_time_group") == group_sequence),
                key=lambda event: _numeric_text(event.get("ingest_sequence"), field="ingest_sequence"),
            )
        )
        if not members:
            raise ValueError("verified availability group has no committed events")
        for event in members:
            revision = event.get("revision")
            if not isinstance(revision, Mapping) or revision.get("availability_slot") != availability_slot:
                raise ValueError("event availability does not match its verified group")
        event_ids = tuple(cast(str, event["event_id"]) for event in members)
        if consumed.intersection(event_ids):
            raise ValueError("a committed event appears in more than one availability group")
        consumed.update(event_ids)
        grouped.append(
            EventGroup(
                group_sequence=group_sequence,
                event_ids=event_ids,
                availability_slot=availability_slot,
                event_records=tuple(canonical_record_bytes(event) for event in members),
                normalization_receipt_record=_group_receipt(group_sequence, members, normalization_code),
                feature_code_sha256=feature_code,
            )
        )
    if consumed != {cast(str, event["event_id"]) for event in events}:
        raise ValueError("verified availability schedule does not cover every committed event")
    return tuple(grouped)
