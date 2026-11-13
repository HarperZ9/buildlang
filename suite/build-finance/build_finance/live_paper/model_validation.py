"""Model-disabled evidence boundary for the offline G2 paper kernel."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Literal, cast

from build_finance.crypto_replay.canonical import parse_canonical_record
from build_finance.crypto_replay.content_ids import verify_content_id
from build_finance.crypto_replay.schema_registry import require_valid_contract


@dataclass(frozen=True, slots=True)
class ValidatedModelEvidence:
    """One immutable model-disabled decision input."""

    accepted_signal_record: bytes | None
    feature_snapshot_id: str
    disposition: Literal["ABSTAIN"]
    reason_code: Literal["MODEL_DISABLED"]


def validate_model_signal(
    signal_record: bytes | None,
    signal_manifest_record: bytes | None,
    model_registry_record: bytes | None,
    feature_snapshot_record: bytes,
) -> ValidatedModelEvidence:
    """Bind G2's mandatory model abstention to one verified FeatureSnapshot."""
    snapshot = parse_canonical_record(feature_snapshot_record)
    require_valid_contract(snapshot, expected_schema="trading.feature-snapshot/v1")
    if not verify_content_id(snapshot):
        raise ValueError("feature snapshot ID does not match its retained record")
    if any(
        record is not None
        for record in (signal_record, signal_manifest_record, model_registry_record)
    ):
        raise ValueError("model evidence is disabled for G2")
    return ValidatedModelEvidence(
        accepted_signal_record=None,
        feature_snapshot_id=cast(str, snapshot["snapshot_id"]),
        disposition="ABSTAIN",
        reason_code="MODEL_DISABLED",
    )
