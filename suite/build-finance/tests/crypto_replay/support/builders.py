"""Strict in-memory builders for synthetic crypto-replay contract vectors."""

from __future__ import annotations

from collections.abc import Mapping
from dataclasses import dataclass

from build_finance.crypto_replay.canonical import (
    JsonObject,
    JsonValue,
    canonical_record_bytes,
    parse_canonical_record,
    sha256_hex,
)
from build_finance.crypto_replay.content_ids import compute_content_id, seal_content_id, verify_content_id
from build_finance.crypto_replay.schema_definitions import CONTRACT_SPECS_BY_SCHEMA
from build_finance.crypto_replay.schema_model import Assurance, ContractFamily, ResolvedContent
from build_finance.crypto_replay.schema_registry import require_valid_contract


@dataclass(frozen=True, slots=True)
class _RetainedRecord:
    record: bytes
    assurance: Assurance


class InMemoryResolver:
    """Resolve self-addressed records and exact retained bytes in separate maps."""

    __slots__ = ("_bytes_by_sha256", "_objects_by_content_id")

    def __init__(self) -> None:
        self._objects_by_content_id: dict[str, _RetainedRecord] = {}
        self._bytes_by_sha256: dict[str, bytes] = {}

    @property
    def content_ids(self) -> tuple[str, ...]:
        return tuple(self._objects_by_content_id)

    @property
    def byte_sha256s(self) -> tuple[str, ...]:
        return tuple(self._bytes_by_sha256)

    @property
    def resolved_contents(self) -> tuple[ResolvedContent, ...]:
        return tuple(self._resolved_content(retained) for retained in self._objects_by_content_id.values())

    def resolve_object(self, content_id: str) -> ResolvedContent | None:
        retained = self._objects_by_content_id.get(content_id)
        return None if retained is None else self._resolved_content(retained)

    def resolve_bytes(self, sha256: str) -> bytes | None:
        return self._bytes_by_sha256.get(sha256)

    def retain_bytes(self, sha256: str, payload: bytes) -> None:
        """Retain exact bytes at their SHA-256 key, rejecting collisions and mismatches."""
        retained = bytes(payload)
        existing = self._bytes_by_sha256.get(sha256)
        if existing is not None and existing != retained:
            raise ValueError(f"duplicate retained-byte SHA-256 {sha256!r} has different bytes")
        if sha256_hex(retained) != sha256:
            raise ValueError("retained-byte SHA-256 does not match the supplied bytes")
        self._bytes_by_sha256[sha256] = retained

    def retain_payload(self, payload: bytes) -> str:
        """Retain exact bytes and return their independently keyed SHA-256."""
        digest = sha256_hex(payload)
        self.retain_bytes(digest, payload)
        return digest

    def retain_record(self, record: bytes, *, assurance: Assurance = "SCHEMA_VALID") -> JsonObject:
        """Validate and retain one primary record with internally derived assurance."""
        if assurance not in ("DIGEST_ONLY", "SCHEMA_VALID"):
            raise ValueError(f"invalid assurance: {assurance!r}")
        if assurance != "SCHEMA_VALID":
            raise ValueError("DIGEST_ONLY retention is restricted to the supporting builder")
        return self._retain_record(record, assurance="SCHEMA_VALID", expected_family="PRIMARY")

    def retain_supporting_record(self, record: bytes) -> JsonObject:
        """Validate and retain one implemented supporting contract record."""
        return self._retain_record(record, assurance="SCHEMA_VALID", expected_family="SUPPORTING")

    def retain_self_addressed_authority_record(self, record: bytes) -> JsonObject:
        """Validate and retain one implemented primary/supporting authority record."""
        document = parse_canonical_record(bytes(record))
        schema_id = document["schema"]
        if not isinstance(schema_id, str):
            raise ValueError("authority record requires a string schema tag")
        family = CONTRACT_SPECS_BY_SCHEMA[schema_id].family
        if family == "PRIMARY":
            return self.retain_record(record)
        if family == "SUPPORTING":
            return self.retain_supporting_record(record)
        raise ValueError(f"schema is not a self-addressed authority family: {schema_id!r}")

    @staticmethod
    def _resolved_content(retained: _RetainedRecord) -> ResolvedContent:
        return ResolvedContent(
            record=retained.record,
            document=parse_canonical_record(retained.record),
            assurance=retained.assurance,
        )

    def _retain_digest_only_supporting_record(self, record: bytes) -> JsonObject:
        return self._retain_record(record, assurance="DIGEST_ONLY", expected_family="SUPPORTING")

    def _retain_record(
        self,
        record: bytes,
        *,
        assurance: object,
        expected_family: ContractFamily,
    ) -> JsonObject:
        if assurance not in ("DIGEST_ONLY", "SCHEMA_VALID"):
            raise ValueError(f"invalid assurance: {assurance!r}")
        if (assurance, expected_family) not in {
            ("SCHEMA_VALID", "PRIMARY"),
            ("SCHEMA_VALID", "SUPPORTING"),
            ("DIGEST_ONLY", "SUPPORTING"),
        }:
            raise ValueError("assurance does not match the internally owned contract family")

        retained_record = bytes(record)
        document = parse_canonical_record(retained_record)
        if canonical_record_bytes(document) != retained_record:
            raise ValueError("content preimage is not the complete canonical LF record")
        if not verify_content_id(document):
            raise ValueError("content preimage has an invalid self-ID")
        schema_id = document["schema"]
        if not isinstance(schema_id, str):
            raise ValueError("content preimage requires a string schema tag")
        family = CONTRACT_SPECS_BY_SCHEMA[schema_id].family
        if family != expected_family:
            raise ValueError(f"record family is {family}, expected {expected_family}")
        if assurance == "SCHEMA_VALID":
            require_valid_contract(document, expected_schema=schema_id)

        content_id = compute_content_id(document)
        retained = _RetainedRecord(record=retained_record, assurance=assurance)
        record_sha256 = sha256_hex(retained_record)
        existing = self._objects_by_content_id.get(content_id)
        if existing is not None and existing.record != retained_record:
            raise ValueError(f"duplicate ContentID {content_id!r} has different bytes")
        if existing is not None and existing != retained:
            raise ValueError(f"duplicate ContentID {content_id!r} has different assurance or content")
        existing_bytes = self._bytes_by_sha256.get(record_sha256)
        if existing_bytes is not None and existing_bytes != retained_record:
            raise ValueError(f"duplicate retained-byte SHA-256 {record_sha256!r} has different bytes")

        self._bytes_by_sha256[record_sha256] = retained_record
        self._objects_by_content_id[content_id] = retained
        return parse_canonical_record(retained_record)


def build_primary_document(
    document: Mapping[str, JsonValue],
    resolver: InMemoryResolver,
) -> JsonObject:
    """Seal, serialize, reparse, validate, and retain one primary record."""
    sealed = seal_content_id(document)
    return resolver.retain_record(canonical_record_bytes(sealed))


def build_digest_only_supporting_document(
    document: Mapping[str, JsonValue],
    resolver: InMemoryResolver,
) -> JsonObject:
    """Seal and retain a supporting preimage without asserting its T02 schema."""
    sealed = seal_content_id(document)
    return resolver._retain_digest_only_supporting_record(canonical_record_bytes(sealed))


def build_supporting_document(
    document: Mapping[str, JsonValue],
    resolver: InMemoryResolver,
) -> JsonObject:
    """Seal, serialize, reparse, validate, and retain one supporting record."""
    sealed = seal_content_id(document)
    return resolver.retain_supporting_record(canonical_record_bytes(sealed))


def _sealed_record(document: Mapping[str, JsonValue]) -> JsonObject:
    """Seal and reparse one complete canonical LF record."""
    sealed = seal_content_id(document)
    return parse_canonical_record(canonical_record_bytes(sealed))


def build_missing_config_admission_receipt(
    *,
    admission_sequence: str,
    validator_code_sha256: str,
    schema_bundle_sha256: str,
) -> JsonObject:
    """Build the exact absent-input config receipt without raw/config inputs."""
    return _sealed_record(
        {
            "schema": "trading.config-admission-receipt/v1",
            "status": "MISSING",
            "reason_codes": ["CONFIG_MISSING"],
            "raw_config_sha256": None,
            "validated_config_sha256": None,
            "raw_byte_length": "0",
            "admission_sequence": admission_sequence,
            "validator_code_sha256": validator_code_sha256,
            "schema_bundle_sha256": schema_bundle_sha256,
        }
    )


def build_invalid_config_admission_receipt(
    raw_config_bytes: bytes,
    *,
    reason_codes: tuple[str, ...],
    admission_sequence: str,
    validator_code_sha256: str,
    schema_bundle_sha256: str,
) -> JsonObject:
    """Build an invalid-input receipt from exact supplied bytes and closed reasons."""
    from build_finance.crypto_replay.contract_semantics import (
        normalize_config_admission_reason_codes,
    )

    normalized = normalize_config_admission_reason_codes(reason_codes)
    if not normalized or "CONFIG_MISSING" in normalized:
        raise ValueError("INVALID requires non-missing config rejection reasons")
    retained = bytes(raw_config_bytes)
    return _sealed_record(
        {
            "schema": "trading.config-admission-receipt/v1",
            "status": "INVALID",
            "reason_codes": list(normalized),
            "raw_config_sha256": sha256_hex(retained),
            "validated_config_sha256": None,
            "raw_byte_length": str(len(retained)),
            "admission_sequence": admission_sequence,
            "validator_code_sha256": validator_code_sha256,
            "schema_bundle_sha256": schema_bundle_sha256,
        }
    )


def build_valid_config_admission_receipt(
    raw_config_bytes: bytes,
    validated_config: Mapping[str, JsonValue],
    *,
    admission_sequence: str,
    validator_code_sha256: str,
    schema_bundle_sha256: str,
) -> JsonObject:
    """Build a valid receipt only from exact canonical bytes and verified config."""
    retained = bytes(raw_config_bytes)
    config = dict(validated_config)
    if config.get("schema") != "trading.replay-risk-config/v1":
        raise ValueError("validated config has the wrong schema")
    if retained != canonical_record_bytes(config):
        raise ValueError("VALID raw bytes must be the canonical LF config record")
    if not verify_content_id(config):
        raise ValueError("VALID requires a correctly self-addressed config")
    require_valid_contract(config, expected_schema="trading.replay-risk-config/v1")
    return _sealed_record(
        {
            "schema": "trading.config-admission-receipt/v1",
            "status": "VALID",
            "reason_codes": [],
            "raw_config_sha256": sha256_hex(retained),
            "validated_config_sha256": compute_content_id(config),
            "raw_byte_length": str(len(retained)),
            "admission_sequence": admission_sequence,
            "validator_code_sha256": validator_code_sha256,
            "schema_bundle_sha256": schema_bundle_sha256,
        }
    )


def build_source_admission_receipt(
    *,
    reason_codes: tuple[str, ...],
    fixture_manifest_sha256: str | None,
    raw_payload_sha256: str | None,
    terms_sha256: str | None,
    parser_code_sha256: str,
    source_id: str | None,
    source_kind: str | None,
    source_revision: str | None,
    market_id: str | None,
    relative_path: str | None,
    media_type: str | None,
    byte_length: str | None,
    admission_sequence: str,
    availability_slot: str | None,
    observed_at: str | None,
    ingested_at: str | None,
    rights_role: str | None,
    rights_effective_date: str | None,
    rights_review_date: str | None,
    parser_version: str,
) -> JsonObject:
    """Derive source status/ID while copying every evidence input independently."""
    from build_finance.crypto_replay.contract_semantics import (
        derive_source_admission_status,
        normalize_source_admission_reason_codes,
    )

    normalized = normalize_source_admission_reason_codes(reason_codes)
    return _sealed_record(
        {
            "schema": "trading.source-admission-receipt/v1",
            "status": derive_source_admission_status(normalized),
            "reason_codes": list(normalized),
            "fixture_manifest_sha256": fixture_manifest_sha256,
            "raw_payload_sha256": raw_payload_sha256,
            "terms_sha256": terms_sha256,
            "parser_code_sha256": parser_code_sha256,
            "source_id": source_id,
            "source_kind": source_kind,
            "source_revision": source_revision,
            "market_id": market_id,
            "relative_path": relative_path,
            "media_type": media_type,
            "byte_length": byte_length,
            "admission_sequence": admission_sequence,
            "availability_slot": availability_slot,
            "observed_at": observed_at,
            "ingested_at": ingested_at,
            "rights_role": rights_role,
            "rights_effective_date": rights_effective_date,
            "rights_review_date": rights_review_date,
            "parser_version": parser_version,
        }
    )


def synthetic_payload(label: str) -> bytes:
    """Return clearly synthetic retained bytes for a contract-only digest field."""
    if not label or "\n" in label or "\r" in label:
        raise ValueError("synthetic payload label must be one non-empty line")
    return f"SYNTHETIC_T01_CONTRACT_VECTOR\n{label}\n".encode("ascii")
