"""Immutable, evidence-only contracts for offline capital replay."""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True, slots=True)
class EvidenceSummary:
    """One retained receipt or projection outcome."""

    kind: str
    evidence_id: str
    status: str
    reason_codes: tuple[str, ...]


@dataclass(frozen=True, slots=True)
class ModelObservation:
    """Read-only model-validation disposition retained by the paper kernel."""

    decision_sequence: str
    status: str
    reason_codes: tuple[str, ...]


@dataclass(frozen=True, slots=True)
class CapitalReplayResult:
    """Evidence identifiers and outcome summaries from one offline replay."""

    fixture: Path
    projection_id: str
    closure_status: str
    closure_reason_codes: tuple[str, ...]
    receipt_ids: tuple[str, ...]
    evidence: tuple[EvidenceSummary, ...]
    model_observations: tuple[ModelObservation, ...]
    closure_id: str | None = None


@dataclass(frozen=True, slots=True)
class FeedbackObservation:
    """Read-only feedback view derived exclusively from retained evidence."""

    kind: str
    status: str
    reason_codes: tuple[str, ...]
    evidence_ids: tuple[str, ...]


__all__ = [
    "CapitalReplayResult",
    "EvidenceSummary",
    "FeedbackObservation",
    "ModelObservation",
]
