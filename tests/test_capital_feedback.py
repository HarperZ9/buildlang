from __future__ import annotations

from pathlib import Path

from build_engine.capital.contracts import CapitalReplayResult, EvidenceSummary, ModelObservation
from build_engine.capital.feedback import observations_from_result


def test_feedback_is_read_only_and_derived_from_retained_evidence() -> None:
    result = CapitalReplayResult(
        fixture=Path("fixture"),
        projection_id="projection-1",
        closure_status="CLOSED",
        closure_reason_codes=(),
        receipt_ids=("run-1", "closure-1"),
        evidence=(
            EvidenceSummary("DECISION", "decision-1", "ABSTAIN", ("MODEL_DISABLED",)),
            EvidenceSummary("FILL", "fill-1", "NOT_APPLICABLE", ()),
            EvidenceSummary("PORTFOLIO", "portfolio-1", "CLOSED", ()),
            EvidenceSummary("CLOSURE", "closure-1", "CLOSED", ()),
        ),
        model_observations=(ModelObservation("1", "ABSTAIN", ("MODEL_DISABLED",)),),
    )

    observations = observations_from_result(result)

    assert tuple(observation.kind for observation in observations) == (
        "DECISION",
        "FILL",
        "PORTFOLIO",
        "CLOSURE",
    )
    assert observations[0].evidence_ids == ("decision-1", "projection-1")
    assert observations[-1].status == "CLOSED"
    assert result.evidence[0].status == "ABSTAIN"
