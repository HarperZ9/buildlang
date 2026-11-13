"""Pure in-memory evidence resolution boundary for offline paper runs."""

from __future__ import annotations

from typing import Protocol


class EvidenceResolver(Protocol):
    """Resolve exact bytes already authorized by a verified evidence graph."""

    def resolve_record(self, content_id: str) -> bytes: ...

    def resolve_bytes(self, sha256: str) -> bytes: ...
