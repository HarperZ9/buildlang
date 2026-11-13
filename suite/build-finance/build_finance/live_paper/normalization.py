"""Deterministic normalization of rooted, admitted G2 source evidence."""

from __future__ import annotations

from collections.abc import Mapping
from copy import deepcopy
from dataclasses import dataclass
from typing import Any, cast

from build_finance.crypto_replay.admission import ParsedSourceCandidate
from build_finance.crypto_replay.canonical import (
    JsonObject,
    parse_canonical_json,
    parse_canonical_record,
)
from build_finance.crypto_replay.canonical import (
    canonical_record_bytes as replay_record_bytes,
)
from build_finance.crypto_replay.canonical import (
    sha256_hex as replay_sha256_hex,
)
from build_finance.crypto_replay.content_ids import seal_content_id as seal_replay_content_id
from build_finance.crypto_replay.content_ids import verify_content_id as verify_replay_content_id
from build_finance.crypto_replay.schema_registry import validate_contract as validate_replay_contract
from build_finance.live_paper.content_ids import canonical_json_bytes as live_json_bytes
from build_finance.live_paper.content_ids import canonical_record_bytes as live_record_bytes
from build_finance.live_paper.content_ids import seal_content_id as seal_live_content_id
from build_finance.live_paper.content_ids import sha256_hex as live_sha256_hex
from build_finance.live_paper.resolver import EvidenceResolver

_NORMALIZATION_CODE_SHA256 = "52d2a97d33ca73d5619f7422fe275e0a4a37bec6b9b025ac257cce36dca03c25"
_PAYLOAD_SCHEMA = "build-finance.live-paper.synthetic-normalization-input/v1"
_JUPITER_PAYLOAD_SCHEMA = "build-finance.live-paper.synthetic-jupiter-normalization-input/v1"
_PAYLOAD_FIELDS = (
    "equal_time_group",
    "event_kind",
    "event_time",
    "executable",
    "ingest_sequence",
    "market",
    "quality_flags",
    "replay_clock_ns",
    "revision",
    "schema",
    "source_position",
    "source_sequence",
)
_JUPITER_PAYLOAD_FIELDS = (
    "base_amount_atoms",
    "base_decimals",
    "base_mint",
    "equal_time_group",
    "event_kind",
    "event_time",
    "executable",
    "fees",
    "ingest_sequence",
    "liquidity",
    "market_id",
    "quality_flags",
    "quote_amount_atoms",
    "quote_decimals",
    "quote_mint",
    "replay_clock_ns",
    "revision",
    "route",
    "route_impact_bps",
    "schema",
    "source_id",
    "source_kind",
    "source_position",
    "source_revision",
    "source_sequence",
)
_MARKET_FIELDS = (
    "base_amount_atoms",
    "liquidity_quote_atoms",
    "priority_fee_quote_atoms",
    "quote_amount_atoms",
    "route_capacity_base_atoms",
    "route_impact_bps",
    "venue_fee_quote_atoms",
)
_JUPITER_ROUTE_FIELDS = ("route_capacity_base_atoms",)
_JUPITER_LIQUIDITY_FIELDS = ("liquidity_quote_atoms",)
_JUPITER_FEE_FIELDS = ("priority_fee_quote_atoms", "venue_fee_quote_atoms")
_JUPITER_POSITION_FIELDS = (
    "event_index",
    "instruction_index",
    "slot",
    "source_native_event_id",
    "source_subsequence",
    "transaction_index",
)
_JUPITER_REVISION_FIELDS = (
    "availability_admission_sequence",
    "availability_slot",
    "kind",
    "retracts_event_id",
    "supersedes_event_id",
)
_CANDIDATE_RECEIPT_FIELDS = (
    ("admission_sequence", "admission_sequence"),
    ("relative_path", "relative_path"),
    ("raw_payload_sha256", "raw_payload_sha256"),
    ("source_id", "source_id"),
    ("source_kind", "source_kind"),
    ("source_revision", "source_revision"),
    ("market_id", "market_id"),
    ("observed_at", "observed_at"),
    ("ingested_at", "ingested_at"),
)
_FILE_RECEIPT_FIELDS = (
    "admission_sequence",
    "availability_slot",
    "byte_length",
    "market_id",
    "media_type",
    "raw_payload_sha256",
    "relative_path",
    "source_id",
    "source_kind",
    "source_revision",
)


@dataclass(frozen=True, slots=True)
class NormalizedCandidate:
    """One sealed raw event and its total normalization receipt."""

    raw_event_record: bytes
    normalization_receipt_record: bytes

    def __post_init__(self) -> None:
        object.__setattr__(self, "raw_event_record", bytes(self.raw_event_record))
        object.__setattr__(self, "normalization_receipt_record", bytes(self.normalization_receipt_record))


def _verified_replay_record(record: bytes, expected_schema: str) -> JsonObject:
    document = parse_canonical_record(bytes(record))
    issues = validate_replay_contract(document, expected_schema=expected_schema)
    if issues:
        raise ValueError(issues)
    if not verify_replay_content_id(document):
        raise ValueError(f"{expected_schema} self-ID does not match its canonical body")
    return document


def _require_exact_keys(document: Mapping[str, Any], keys: tuple[str, ...], label: str) -> None:
    if set(document) != set(keys):
        raise ValueError(f"{label} fields do not match the closed G2 contract")


def _require_mapping(value: object, keys: tuple[str, ...], label: str) -> Mapping[str, Any]:
    if not isinstance(value, Mapping):
        raise ValueError(f"{label} must be a closed object")
    _require_exact_keys(value, keys, label)
    return value


def _require_text(value: object, label: str) -> str:
    if not isinstance(value, str):
        raise ValueError(f"{label} must be canonical text")
    return value


def _require_int(value: object, label: str) -> int:
    if isinstance(value, bool) or not isinstance(value, int):
        raise ValueError(f"{label} must be an integer")
    return value


def _candidate_receipt_binding(candidate: ParsedSourceCandidate, receipt: Mapping[str, Any]) -> None:
    for candidate_field, receipt_field in _CANDIDATE_RECEIPT_FIELDS:
        if getattr(candidate, candidate_field) != receipt[receipt_field]:
            raise ValueError(f"candidate {candidate_field} does not match its admitted source receipt")
    if not isinstance(candidate.source_position, Mapping) or not isinstance(candidate.revision, Mapping):
        raise ValueError("admitted candidate requires source position and revision evidence")
    if candidate.event_time is None:
        raise ValueError("the compact G2 normalization profile requires provider event time")


def _fixture_market(
    profile: Mapping[str, Any],
    receipt: Mapping[str, Any],
) -> Mapping[str, Any]:
    files_value = profile["files"]
    markets_value = profile["allowed_markets"]
    if not isinstance(files_value, list) or not isinstance(markets_value, list):
        raise ValueError("fixture normalization profile has invalid closed rows")

    files = [row for row in files_value if isinstance(row, Mapping)]
    matching_files = [row for row in files if row.get("admission_sequence") == receipt["admission_sequence"]]
    if len(matching_files) != 1:
        raise ValueError("source receipt does not select exactly one fixture file")
    fixture_file = matching_files[0]
    if any(fixture_file.get(field) != receipt[field] for field in _FILE_RECEIPT_FIELDS):
        raise ValueError("source receipt does not match its exact fixture file authority")

    matching_markets = [row for row in markets_value if isinstance(row, Mapping) and row.get("market_id") == receipt["market_id"]]
    if len(matching_markets) != 1:
        raise ValueError("source receipt does not select exactly one fixture market")
    return matching_markets[0]


def _candidate_payload_binding(candidate: ParsedSourceCandidate, payload: Mapping[str, Any]) -> None:
    if payload["source_position"] != candidate.source_position:
        raise ValueError("candidate source_position does not match exact payload authority")
    if payload["revision"] != candidate.revision:
        raise ValueError("candidate revision does not match exact payload authority")
    if payload["event_time"] != candidate.event_time:
        raise ValueError("candidate event_time does not match exact payload authority")

    if not isinstance(payload["source_position"], Mapping) or not isinstance(payload["revision"], Mapping):
        raise ValueError("normalization payload position and revision must be closed objects")
    if not isinstance(payload["event_time"], str) or type(payload["executable"]) is not bool:
        raise ValueError("normalization payload event time and executable disposition have invalid types")
    if not isinstance(payload["quality_flags"], list) or not all(
        isinstance(flag, str) for flag in payload["quality_flags"]
    ):
        raise ValueError("normalization payload quality flags must be an array of strings")
    for field in ("source_sequence", "ingest_sequence", "equal_time_group", "replay_clock_ns"):
        if not isinstance(payload[field], str):
            raise ValueError(f"normalization payload {field} must be canonical numeric text")


def _jupiter_identity_binding(
    candidate: ParsedSourceCandidate,
    market_profile: Mapping[str, Any],
    payload: Mapping[str, Any],
) -> None:
    for field in ("source_id", "source_kind", "source_revision", "market_id"):
        if payload[field] != getattr(candidate, field):
            raise ValueError(f"synthetic Jupiter payload {field} does not match admitted candidate")
    for field in ("base_mint", "quote_mint", "base_decimals", "quote_decimals"):
        if payload[field] != market_profile[field]:
            raise ValueError(f"synthetic Jupiter payload {field} does not match fixture market")


def _compact_payload_from_jupiter(
    candidate: ParsedSourceCandidate,
    receipt: Mapping[str, Any],
    profile: Mapping[str, Any],
    payload: Mapping[str, Any],
) -> JsonObject:
    _require_exact_keys(payload, _JUPITER_PAYLOAD_FIELDS, "synthetic Jupiter normalization payload")
    market_profile = _fixture_market(profile, receipt)
    _jupiter_identity_binding(candidate, market_profile, payload)
    route = _require_mapping(payload["route"], _JUPITER_ROUTE_FIELDS, "synthetic Jupiter route")
    liquidity = _require_mapping(payload["liquidity"], _JUPITER_LIQUIDITY_FIELDS, "synthetic Jupiter liquidity")
    fees = _require_mapping(payload["fees"], _JUPITER_FEE_FIELDS, "synthetic Jupiter fees")
    source_position = _require_mapping(
        payload["source_position"],
        _JUPITER_POSITION_FIELDS,
        "synthetic Jupiter source_position",
    )
    revision = _require_mapping(payload["revision"], _JUPITER_REVISION_FIELDS, "synthetic Jupiter revision")
    compact: JsonObject = {
        "schema": _PAYLOAD_SCHEMA,
        "event_kind": _require_text(payload["event_kind"], "synthetic Jupiter event_kind"),
        "market": {
            "base_amount_atoms": _require_text(payload["base_amount_atoms"], "synthetic Jupiter base_amount_atoms"),
            "quote_amount_atoms": _require_text(payload["quote_amount_atoms"], "synthetic Jupiter quote_amount_atoms"),
            "route_capacity_base_atoms": _require_text(
                route["route_capacity_base_atoms"],
                "synthetic Jupiter route_capacity_base_atoms",
            ),
            "liquidity_quote_atoms": _require_text(
                liquidity["liquidity_quote_atoms"],
                "synthetic Jupiter liquidity_quote_atoms",
            ),
            "venue_fee_quote_atoms": _require_text(
                fees["venue_fee_quote_atoms"],
                "synthetic Jupiter venue_fee_quote_atoms",
            ),
            "priority_fee_quote_atoms": _require_text(
                fees["priority_fee_quote_atoms"],
                "synthetic Jupiter priority_fee_quote_atoms",
            ),
            "route_impact_bps": _require_int(payload["route_impact_bps"], "synthetic Jupiter route_impact_bps"),
        },
        "source_position": deepcopy(dict(source_position)),
        "revision": deepcopy(dict(revision)),
        "event_time": _require_text(payload["event_time"], "synthetic Jupiter event_time"),
        "executable": payload["executable"],
        "quality_flags": deepcopy(payload["quality_flags"]),
        "source_sequence": _require_text(payload["source_sequence"], "synthetic Jupiter source_sequence"),
        "ingest_sequence": _require_text(payload["ingest_sequence"], "synthetic Jupiter ingest_sequence"),
        "equal_time_group": _require_text(payload["equal_time_group"], "synthetic Jupiter equal_time_group"),
        "replay_clock_ns": _require_text(payload["replay_clock_ns"], "synthetic Jupiter replay_clock_ns"),
    }
    _candidate_payload_binding(candidate, compact)
    return compact


def _normalization_payload(
    candidate: ParsedSourceCandidate,
    receipt: Mapping[str, Any],
    profile: Mapping[str, Any],
    payload: Mapping[str, Any],
) -> Mapping[str, Any]:
    if payload.get("schema") == _JUPITER_PAYLOAD_SCHEMA:
        return _compact_payload_from_jupiter(candidate, receipt, profile, payload)
    _require_exact_keys(payload, _PAYLOAD_FIELDS, "normalization payload")
    if payload["schema"] != _PAYLOAD_SCHEMA or not isinstance(payload["event_kind"], str):
        raise ValueError("normalization payload does not match the compact G2 profile")
    market = payload["market"]
    if not isinstance(market, Mapping):
        raise ValueError("normalization payload market must be a closed object")
    _require_exact_keys(market, _MARKET_FIELDS, "normalization market")
    _candidate_payload_binding(candidate, payload)
    return payload


def _raw_event_document(
    candidate: ParsedSourceCandidate,
    receipt: Mapping[str, Any],
    profile: Mapping[str, Any],
    payload: Mapping[str, Any],
) -> JsonObject:
    market_profile = _fixture_market(profile, receipt)
    market = payload["market"]
    assert isinstance(market, Mapping)
    document: JsonObject = {
        "schema": "trading.raw-event/v1",
        "fixture_manifest_sha256": cast(str, profile["fixture_manifest_sha256"]),
        "source_admission_receipt_id": cast(str, receipt["source_admission_receipt_id"]),
        "raw_payload_sha256": candidate.raw_payload_sha256,
        "source_id": cast(str, candidate.source_id),
        "source_kind": cast(str, candidate.source_kind),
        "source_revision": cast(str, candidate.source_revision),
        "market_id": cast(str, candidate.market_id),
        "network": cast(str, profile["network"]),
        "venue_profile": cast(str, profile["venue_profile"]),
        "base_mint": cast(str, market_profile["base_mint"]),
        "quote_mint": cast(str, market_profile["quote_mint"]),
        "base_decimals": cast(int, market_profile["base_decimals"]),
        "quote_decimals": cast(int, market_profile["quote_decimals"]),
        "admission_sequence": candidate.admission_sequence,
        "source_sequence": cast(str, payload["source_sequence"]),
        "ingest_sequence": cast(str, payload["ingest_sequence"]),
        "equal_time_group": cast(str, payload["equal_time_group"]),
        "replay_clock_ns": cast(str, payload["replay_clock_ns"]),
        "event_time": cast(str, payload["event_time"]),
        "observed_at": candidate.observed_at,
        "ingested_at": candidate.ingested_at,
        "source_position": deepcopy(dict(cast(Mapping[str, Any], payload["source_position"]))),
        "revision": deepcopy(dict(cast(Mapping[str, Any], payload["revision"]))),
        "event_kind": cast(str, payload["event_kind"]),
        "executable": cast(bool, payload["executable"]),
        "quality_flags": deepcopy(payload["quality_flags"]),
        "market": deepcopy(dict(market)),
    }
    sealed = seal_replay_content_id(document)
    issues = validate_replay_contract(sealed, expected_schema="trading.raw-event/v1")
    if issues:
        raise ValueError(issues)
    return sealed


def _normalization_receipt_document(source_receipt_id: str, admission_sequence: str, event_id: str) -> dict[str, Any]:
    from build_finance.live_paper.registry import validate_contract as validate_live_contract

    output_root = {
        "schema": "build-finance.live-paper.single-normalized-output-root/v1",
        "event_ids": [event_id],
    }
    sealed = seal_live_content_id(
        {
            "schema": "trading.normalization-receipt/v1",
            "source_batch_id": f"g2-normalization-source-{admission_sequence}",
            "normalization_code_sha256": _NORMALIZATION_CODE_SHA256,
            "input_content_ids": [source_receipt_id],
            "input_count": "1",
            "normalized_event_ids": [event_id],
            "normalized_event_count": "1",
            "output_merkle_root_sha256": live_sha256_hex(live_json_bytes(output_root)),
            "status": "PASS",
            "reason_codes": [],
            "total_evidence": "TOTAL_INPUT_CLOSURE",
        }
    )
    issues = validate_live_contract(sealed, expected_schema="trading.normalization-receipt/v1")
    if issues:
        raise ValueError(issues)
    return sealed


def normalize_admitted_candidate(
    candidate: ParsedSourceCandidate,
    source_receipt_record: bytes,
    resolver: EvidenceResolver,
    profile_record: bytes,
) -> NormalizedCandidate:
    """Normalize one exact admitted candidate without ambient authority or I/O."""
    if not isinstance(candidate, ParsedSourceCandidate):
        raise ValueError("normalization requires a ParsedSourceCandidate")
    receipt = _verified_replay_record(source_receipt_record, "trading.source-admission-receipt/v1")
    if receipt["status"] != "ADMITTED" or receipt["reason_codes"] != []:
        raise ValueError("only an exact ADMITTED source receipt may normalize")
    profile = _verified_replay_record(profile_record, "trading.fixture-manifest/v1")
    if profile["fixture_manifest_sha256"] != receipt["fixture_manifest_sha256"]:
        raise ValueError("normalization profile is not the fixture manifest rooted by the source receipt")
    _candidate_receipt_binding(candidate, receipt)

    normalization_code = bytes(resolver.resolve_bytes(_NORMALIZATION_CODE_SHA256))
    if replay_sha256_hex(normalization_code) != _NORMALIZATION_CODE_SHA256:
        raise ValueError("normalization code preimage digest mismatch")
    raw_payload = bytes(resolver.resolve_bytes(candidate.raw_payload_sha256))
    if replay_sha256_hex(raw_payload) != candidate.raw_payload_sha256:
        raise ValueError("raw payload digest mismatch")
    payload = _normalization_payload(candidate, receipt, profile, parse_canonical_json(raw_payload))

    raw_event = _raw_event_document(candidate, receipt, profile, payload)
    event_id = cast(str, raw_event["event_id"])
    normalization_receipt = _normalization_receipt_document(
        cast(str, receipt["source_admission_receipt_id"]),
        candidate.admission_sequence,
        event_id,
    )
    return NormalizedCandidate(
        raw_event_record=replay_record_bytes(raw_event),
        normalization_receipt_record=live_record_bytes(normalization_receipt),
    )
