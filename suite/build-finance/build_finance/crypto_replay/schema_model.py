"""Types shared by the local crypto-replay schema infrastructure."""

from __future__ import annotations

from collections.abc import Mapping
from dataclasses import dataclass
from typing import Literal, Protocol

from build_finance.crypto_replay.canonical import JsonValue

ContractFamily = Literal["PRIMARY", "SUPPORTING", "ATTACHMENT"]
Assurance = Literal["DIGEST_ONLY", "SCHEMA_VALID"]
ValidationPath = tuple[str | int, ...]


@dataclass(frozen=True, slots=True)
class ValidationIssue:
    """One deterministic structural or semantic validation failure."""

    code: str
    path: ValidationPath
    message: str


@dataclass(frozen=True, slots=True)
class ResolvedContent:
    """Retained canonical bytes and their parsed document."""

    record: bytes
    document: Mapping[str, JsonValue]
    assurance: Assurance


@dataclass(frozen=True, slots=True)
class ContractSpec:
    """Registry metadata for one protocol contract schema."""

    schema_id: str
    self_id_field: str | None
    family: ContractFamily
    schema_filename: str


class EvidenceResolver(Protocol):
    """Read-only evidence lookup used by later graph validation."""

    def resolve_object(self, content_id: str) -> ResolvedContent | None:
        """Return retained object content, if present."""

    def resolve_bytes(self, sha256: str) -> bytes | None:
        """Return retained opaque bytes, if present."""
