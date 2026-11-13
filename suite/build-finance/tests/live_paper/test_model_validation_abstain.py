"""Qualitative contract for the model-disabled G2 boundary."""

from __future__ import annotations

import copy
from dataclasses import FrozenInstanceError
from typing import Any

import pytest

from build_finance.crypto_replay.canonical import canonical_record_bytes, parse_canonical_record
from build_finance.live_paper.features import derive_feature_snapshot
from build_finance.live_paper.grouping import group_committed_events
from build_finance.live_paper.model_validation import validate_model_signal
from build_finance.live_paper.profiles import PaperKernelProfiles
from build_finance.live_paper.run_input_builder import build_verified_run_inputs
from tests.live_paper.support.g2_vectors import G2Vector, build_g2_vector


def _feature_snapshot_record(vector: G2Vector) -> bytes:
    verified = build_verified_run_inputs(
        vector.run_receipt_record,
        vector.admitted_candidates,
        vector.source_receipt_records,
        vector.resolver,
        PaperKernelProfiles(**vector.profile_records),  # type: ignore[arg-type]
    )
    return derive_feature_snapshot(group_committed_events(verified))


def test_none_model_inputs_return_exact_repeatable_immutable_abstain() -> None:
    snapshot_record = _feature_snapshot_record(build_g2_vector())
    snapshot_id = parse_canonical_record(snapshot_record)["snapshot_id"]

    first = validate_model_signal(None, None, None, snapshot_record)
    second = validate_model_signal(None, None, None, snapshot_record)

    assert first == second
    assert (
        first.accepted_signal_record,
        first.feature_snapshot_id,
        first.disposition,
        first.reason_code,
    ) == (None, snapshot_id, "ABSTAIN", "MODEL_DISABLED")
    with pytest.raises((FrozenInstanceError, TypeError)):
        first.disposition = "ABSTAIN"  # type: ignore[misc]
    with pytest.raises((AttributeError, TypeError)):
        first.undeclared = "ambient-state"  # type: ignore[attr-defined]


@pytest.mark.parametrize(
    ("signal_record", "signal_manifest_record", "model_registry_record", "stale_snapshot"),
    (
        (b"opaque-signal-body", None, None, False),
        (None, b"opaque-manifest-body", None, False),
        (None, None, b"opaque-registry-body", False),
        (None, None, None, True),
    ),
    ids=("signal", "manifest", "registry", "stale-feature-id"),
)
def test_supplied_model_bodies_or_stale_feature_identity_reject(
    signal_record: bytes | None,
    signal_manifest_record: bytes | None,
    model_registry_record: bytes | None,
    stale_snapshot: bool,
) -> None:
    snapshot_record = _feature_snapshot_record(build_g2_vector())
    if stale_snapshot:
        snapshot: dict[str, Any] = copy.deepcopy(parse_canonical_record(snapshot_record))
        snapshot["snapshot_id"] = "0" * 64
        snapshot_record = canonical_record_bytes(snapshot)

    with pytest.raises(ValueError, match="snapshot|disabled"):
        validate_model_signal(
            signal_record,
            signal_manifest_record,
            model_registry_record,
            snapshot_record,
        )
