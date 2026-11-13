"""Read-only feedback views for completed offline capital replays."""

from __future__ import annotations

from build_engine.capital.contracts import CapitalReplayResult, FeedbackObservation


def observations_from_result(result: CapitalReplayResult) -> tuple[FeedbackObservation, ...]:
    """Derive immutable observations without updating model or execution state."""
    return tuple(
        FeedbackObservation(
            kind=evidence.kind,
            status=evidence.status,
            reason_codes=evidence.reason_codes,
            evidence_ids=(evidence.evidence_id, result.projection_id),
        )
        for evidence in result.evidence
    )


__all__ = ["observations_from_result"]
