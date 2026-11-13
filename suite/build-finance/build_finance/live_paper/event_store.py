"""Immutable in-memory ledger store for the offline G2 paper kernel."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, cast

from build_finance.crypto_replay.canonical import canonical_record_bytes, parse_canonical_record
from build_finance.crypto_replay.content_ids import verify_content_id
from build_finance.crypto_replay.schema_registry import require_valid_contract


@dataclass(frozen=True, slots=True)
class InMemoryLedgerStore:
    ledger_records: tuple[bytes, ...]

    def __post_init__(self) -> None:
        object.__setattr__(self, "ledger_records", tuple(bytes(record) for record in self.ledger_records))


def _ledger_document(record: bytes) -> dict[str, Any]:
    document = cast(dict[str, Any], parse_canonical_record(bytes(record)))
    require_valid_contract(document, expected_schema="trading.ledger-record/v1")
    if not verify_content_id(document) or canonical_record_bytes(document) != bytes(record):
        raise ValueError("ledger record self-ID does not match its retained canonical record")
    return document


def append_ledger_record(store: InMemoryLedgerStore, ledger_record: bytes) -> InMemoryLedgerStore:
    """Return a new store after validating one hash-chained frozen ledger record."""
    if not isinstance(store, InMemoryLedgerStore):
        raise ValueError("append_ledger_record requires an InMemoryLedgerStore")
    document = _ledger_document(bytes(ledger_record))
    sequence = int(str(document["ledger_sequence"]))
    if sequence != len(store.ledger_records):
        raise ValueError("ledger_sequence must equal the current in-memory ledger length")
    expected_previous = None if not store.ledger_records else _ledger_document(store.ledger_records[-1])["ledger_record_id"]
    if document["previous_ledger_record_id"] != expected_previous:
        raise ValueError("previous_ledger_record_id does not match the current ledger head")
    ledger_id = document["ledger_record_id"]
    for retained in store.ledger_records:
        if _ledger_document(retained)["ledger_record_id"] == ledger_id:
            raise ValueError("duplicate ledger_record_id cannot be appended")
    return InMemoryLedgerStore((*store.ledger_records, bytes(ledger_record)))
