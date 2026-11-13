"""Immutable, evidence-only contracts for offline capital replay."""

from __future__ import annotations

import json
from dataclasses import dataclass
from hashlib import sha256
from pathlib import Path
from typing import Any


def _canonical_json_bytes(value: object) -> bytes:
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"), sort_keys=True).encode("utf-8")


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

    def evaluator_receipt(self) -> dict[str, Any]:
        """Return a path-free, content-addressed evaluator evidence receipt."""
        unsigned: dict[str, Any] = {
            "schema": "build-engine.capital.evaluator-receipt/v1",
            "data_classification": "SYNTHETIC",
            "mode": "PAPER_ONLY",
            "execution": "SIMULATED",
            "model_inference": "DISABLED_ABSTAIN",
            "profitability_claim": False,
            "projection_id": self.projection_id,
            "closure_id": self.closure_id,
            "closure_status": self.closure_status,
            "closure_reason_codes": list(self.closure_reason_codes),
            "receipt_ids": list(self.receipt_ids),
            "evidence": [
                {
                    "kind": item.kind,
                    "evidence_id": item.evidence_id,
                    "status": item.status,
                    "reason_codes": list(item.reason_codes),
                }
                for item in self.evidence
            ],
            "model_evidence": [
                {
                    "decision_sequence": item.decision_sequence,
                    "status": item.status,
                    "reason_codes": list(item.reason_codes),
                }
                for item in self.model_observations
            ],
        }
        return {
            **unsigned,
            "evaluator_receipt_id": sha256(_canonical_json_bytes(unsigned)).hexdigest(),
        }

    def evaluator_receipt_bytes(self) -> bytes:
        """Serialize the evaluator receipt as stable canonical JSON plus LF."""
        return _canonical_json_bytes(self.evaluator_receipt()) + b"\n"


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
