"""SYNTHETIC local Jupiter fixture writer for T03 admission tests only."""

from __future__ import annotations

import json
from collections.abc import Mapping
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from build_finance.crypto_replay.canonical import JsonObject, JsonValue, canonical_record_bytes, sha256_hex
from build_finance.crypto_replay.content_ids import seal_content_id

SYNTHETIC_SOURCE_ID = "SYNTHETIC_TEST_ONLY_INVALID"
SYNTHETIC_BASE_MINT = "SYNTHETIC_BASE_MINT_0OIl_INVALID"
SYNTHETIC_QUOTE_MINT = "SYNTHETIC_QUOTE_MINT_INVALID"
SYNTHETIC_EVENT_TIME = "2026-01-01T00:00:00.000000000Z"
SYNTHETIC_OBSERVED_AT = "2026-01-01T00:00:01.000000000Z"
SYNTHETIC_INGESTED_AT = "2026-01-01T00:00:02.000000000Z"
SYNTHETIC_TERMS_PREFIX = "SYNTHETIC TEST FIXTURE — NOT MARKET DATA"
SYNTHETIC_PAYLOAD_SOURCE_SENTINEL = b'"source_id":"SYNTHETIC_TEST_ONLY_INVALID"'
SYNTHETIC_PAYLOAD_BASE_SENTINEL = b'"base_mint":"SYNTHETIC_BASE_MINT_0OIl_INVALID"'


@dataclass(frozen=True, slots=True)
class SYNTHETICEventSpec:
    """SYNTHETIC event values used to write one local-only fixture payload."""

    admission_sequence: str = "1"
    availability_slot: str = "1"
    relative_path: str = "payloads/quote-0001.json"
    source_position_slot: str | None = None
    transaction_index: int = 0
    instruction_index: int = 0
    event_index: int = 0
    source_native_event_id: str = "synthetic-event-0001"
    source_subsequence: str = "0"
    source_revision: str = "synthetic-revision-v1"
    revision_kind: str = "ORIGINAL"
    revision_availability_slot: str | None = None
    revision_availability_admission_sequence: str | None = None
    supersedes_event_id: str | None = None
    retracts_event_id: str | None = None
    market_id: str = f"{SYNTHETIC_BASE_MINT}/{SYNTHETIC_QUOTE_MINT}:jupiter"
    base_mint: str = SYNTHETIC_BASE_MINT
    quote_mint: str = SYNTHETIC_QUOTE_MINT
    base_decimals: int = 6
    quote_decimals: int = 6
    event_time: str = SYNTHETIC_EVENT_TIME
    observed_at: str | None = SYNTHETIC_OBSERVED_AT
    ingested_at: str | None = SYNTHETIC_INGESTED_AT
    route_capacity_base_atoms: str = "1000000"
    liquidity_quote_atoms: str = "2000000"
    venue_fee_quote_atoms: str = "100"
    priority_fee_quote_atoms: str = "7"


@dataclass(frozen=True, slots=True)
class SYNTHETICLocalFixture:
    """SYNTHETIC fixture paths and exact bytes written below pytest tmp_path."""

    root: Path
    manifest_path: Path
    rights_manifest_path: Path
    payload_paths: tuple[Path, ...]
    terms_path: Path
    witness_paths: tuple[Path, ...]
    manifest: JsonObject
    rights_manifest: JsonObject
    payloads: tuple[bytes, ...]
    terms_payload: bytes


def write_SYNTHETIC_local_fixture(
    tmp_path: Path,
    *,
    event_specs: tuple[SYNTHETICEventSpec, ...] | None = None,
    rights_overrides: Mapping[str, JsonValue] | None = None,
    manifest_overrides: Mapping[str, JsonValue] | None = None,
) -> SYNTHETICLocalFixture:
    """Write one SYNTHETIC local fixture tree under tmp_path and return its exact evidence."""
    root = tmp_path / "SYNTHETIC-local-fixture"
    root.mkdir(parents=True)
    specs = event_specs or (SYNTHETICEventSpec(),)

    terms_path = root / "terms" / "synthetic-terms.txt"
    terms_payload = (
        f"{SYNTHETIC_TERMS_PREFIX}\nThese bytes are synthetic contract evidence and are never observed market data.\n"
    ).encode()
    _write_bytes(terms_path, terms_payload)
    terms_sha256 = sha256_hex(terms_payload)

    rights_manifest = _rights_manifest(terms_sha256)
    if rights_overrides:
        rights_manifest.update(rights_overrides)
    rights_payload = canonical_record_bytes(rights_manifest)
    rights_manifest_path = root / "rights" / "rights-manifest.json"
    _write_bytes(rights_manifest_path, rights_payload)
    rights_manifest_sha256 = sha256_hex(rights_payload)

    payload_paths: list[Path] = []
    witness_paths: list[Path] = []
    payloads: list[bytes] = []
    file_rows: list[JsonObject] = []
    allowed_markets: dict[tuple[str, str, str], JsonObject] = {}

    for spec in specs:
        payload = _payload_bytes(spec)
        payloads.append(payload)
        payload_path = root / spec.relative_path
        _write_bytes(payload_path, payload)
        payload_paths.append(payload_path)
        payload_sha256 = sha256_hex(payload)

        witness = _witness_record(spec, payload_sha256, len(payload))
        witness_path = root / "witnesses" / f"witness-{int(spec.admission_sequence):04d}.json"
        _write_bytes(witness_path, canonical_record_bytes(witness))
        witness_paths.append(witness_path)

        file_rows.append(
            {
                "relative_path": spec.relative_path,
                "raw_payload_sha256": payload_sha256,
                "byte_length": str(len(payload)),
                "admission_sequence": spec.admission_sequence,
                "availability_slot": spec.availability_slot,
                "media_type": "application/json",
                "source_id": SYNTHETIC_SOURCE_ID,
                "source_kind": "solana-jupiter-quote",
                "source_revision": spec.source_revision,
                "market_id": spec.market_id,
            }
        )
        allowed_markets[(spec.market_id, spec.base_mint, spec.quote_mint)] = {
            "market_id": spec.market_id,
            "base_mint": spec.base_mint,
            "quote_mint": spec.quote_mint,
            "base_decimals": spec.base_decimals,
            "quote_decimals": spec.quote_decimals,
        }

    first = specs[0]
    manifest = {
        "schema": "trading.fixture-manifest/v1",
        "network": "solana-mainnet",
        "venue_profile": "solana-jupiter-fixture/v1",
        "quote_mint": first.quote_mint,
        "quote_decimals": first.quote_decimals,
        "initial_quote_atoms": "1000000000",
        "replay_tick_ns": "1000000000",
        "universe_policy": "FULL_DECLARED_SOURCE_UNIVERSE",
        "point_in_time_mode": "PROVEN_AVAILABILITY_SLOT",
        "run_end_position_policy": "LEAVE_MARKED_OPEN",
        "session_start_availability_slot": "1",
        "session_end_availability_slot": "99",
        "rights_manifest_sha256": rights_manifest_sha256,
        "selection_failures_retained": True,
        "no_route_observations_retained": True,
        "inactive_assets_retained": True,
        "gaps_retained": True,
        "allowed_markets": sorted(allowed_markets.values(), key=lambda row: str(row["market_id"]).encode("utf-8")),
        "files": sorted(file_rows, key=lambda row: str(row["relative_path"]).encode("utf-8")),
    }
    if manifest_overrides:
        manifest.update(manifest_overrides)
    manifest = seal_content_id(manifest)
    manifest_path = root / "manifest.json"
    _write_bytes(manifest_path, canonical_record_bytes(manifest))

    return SYNTHETICLocalFixture(
        root=root,
        manifest_path=manifest_path,
        rights_manifest_path=rights_manifest_path,
        payload_paths=tuple(payload_paths),
        terms_path=terms_path,
        witness_paths=tuple(witness_paths),
        manifest=manifest,
        rights_manifest=rights_manifest,
        payloads=tuple(payloads),
        terms_payload=terms_payload,
    )


def reseal_SYNTHETIC_manifest(fixture: SYNTHETICLocalFixture, manifest: Mapping[str, JsonValue]) -> None:
    """Rewrite the SYNTHETIC manifest as a canonical self-addressed LF record."""
    body = dict(manifest)
    body.pop("fixture_manifest_sha256", None)
    _write_bytes(fixture.manifest_path, canonical_record_bytes(seal_content_id(body)))


def rewrite_SYNTHETIC_rights(fixture: SYNTHETICLocalFixture, rights_manifest: Mapping[str, JsonValue]) -> None:
    """Rewrite the SYNTHETIC rights manifest as a canonical LF record."""
    _write_bytes(fixture.rights_manifest_path, canonical_record_bytes(dict(rights_manifest)))


def rewrite_SYNTHETIC_witness(
    fixture: SYNTHETICLocalFixture,
    index: int,
    witness: Mapping[str, JsonValue],
    *,
    canonical: bool = True,
) -> None:
    """Rewrite one SYNTHETIC witness record, optionally as noncanonical bytes."""
    if canonical:
        payload = canonical_record_bytes(dict(witness))
    else:
        payload = json.dumps(dict(witness), indent=2, sort_keys=False).encode("utf-8") + b"\n"
    _write_bytes(fixture.witness_paths[index], payload)


def witness_for_SYNTHETIC_fixture(
    fixture: SYNTHETICLocalFixture,
    index: int,
    spec: SYNTHETICEventSpec,
) -> JsonObject:
    """Return the expected SYNTHETIC witness record for a fixture payload."""
    payload = fixture.payloads[index]
    return _witness_record(spec, sha256_hex(payload), len(payload))


def _rights_manifest(terms_sha256: str) -> JsonObject:
    return {
        "schema": "build-finance.synthetic-rights-manifest/v1",
        "rights_role": "offline_research_replay",
        "rights_effective_date": "2026-01-01",
        "rights_review_date": "2026-01-02",
        "retention_posture": "LOCAL_RESEARCH_RETENTION",
        "redistribution_posture": "NO_REDISTRIBUTION",
        "provenance_requirements": [
            "CITE_SOURCE_ID",
            "PRESERVE_RAW_SHA256",
            "PRESERVE_TERMS_SHA256",
        ],
        "terms": [{"relative_path": "terms/synthetic-terms.txt", "terms_sha256": terms_sha256}],
    }


def _witness_record(spec: SYNTHETICEventSpec, payload_sha256: str, byte_length: int) -> JsonObject:
    return {
        "schema": "build-finance.local-fixture-witness/v1",
        "admission_sequence": spec.admission_sequence,
        "relative_path": spec.relative_path,
        "raw_payload_sha256": payload_sha256,
        "byte_length": str(byte_length),
        "observed_at": spec.observed_at,
        "ingested_at": spec.ingested_at,
    }


def _payload_bytes(spec: SYNTHETICEventSpec) -> bytes:
    source_position_slot = spec.source_position_slot or spec.availability_slot
    revision_availability_slot = spec.revision_availability_slot or spec.availability_slot
    revision_availability_admission_sequence = spec.revision_availability_admission_sequence or spec.admission_sequence
    payload: dict[str, Any] = {
        "source_id": SYNTHETIC_SOURCE_ID,
        "source_kind": "solana-jupiter-quote",
        "source_revision": spec.source_revision,
        "market_id": spec.market_id,
        "base_mint": spec.base_mint,
        "quote_mint": spec.quote_mint,
        "base_decimals": spec.base_decimals,
        "quote_decimals": spec.quote_decimals,
        "source_position": {
            "slot": source_position_slot,
            "transaction_index": spec.transaction_index,
            "instruction_index": spec.instruction_index,
            "event_index": spec.event_index,
            "source_native_event_id": spec.source_native_event_id,
            "source_subsequence": spec.source_subsequence,
        },
        "revision": {
            "kind": spec.revision_kind,
            "supersedes_event_id": spec.supersedes_event_id,
            "retracts_event_id": spec.retracts_event_id,
            "availability_slot": revision_availability_slot,
            "availability_admission_sequence": revision_availability_admission_sequence,
        },
        "event_time": spec.event_time,
        "route": {"route_capacity_base_atoms": spec.route_capacity_base_atoms},
        "liquidity": {"liquidity_quote_atoms": spec.liquidity_quote_atoms},
        "fees": {
            "venue_fee_quote_atoms": spec.venue_fee_quote_atoms,
            "priority_fee_quote_atoms": spec.priority_fee_quote_atoms,
        },
    }
    result = json.dumps(payload, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
    if SYNTHETIC_PAYLOAD_SOURCE_SENTINEL not in result:
        raise AssertionError("SYNTHETIC source sentinel payload bytes were not preserved exactly")
    if spec.base_mint == SYNTHETIC_BASE_MINT and SYNTHETIC_PAYLOAD_BASE_SENTINEL not in result:
        raise AssertionError("SYNTHETIC sentinel payload bytes were not preserved exactly")
    return result


def _write_bytes(path: Path, payload: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(payload)
