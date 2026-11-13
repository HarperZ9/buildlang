"""Task 11 vertical determinism tests for the G2 offline paper kernel."""

from __future__ import annotations

from dataclasses import replace

from build_finance.crypto_replay.canonical import canonical_record_bytes, parse_canonical_record
from build_finance.crypto_replay.schema_registry import require_valid_contract as require_valid_replay_contract
from build_finance.live_paper.profiles import PaperKernelProfiles
from build_finance.live_paper.run_input_builder import build_verified_run_inputs
from tests.live_paper.support.g2_vectors import G2Vector, build_g2_vector


def _profiles(vector: G2Vector) -> PaperKernelProfiles:
    return PaperKernelProfiles(
        **{
            **vector.profile_records,
            "risk_config_record": canonical_record_bytes(vector.run_input_bundle.replay_risk_config),
        }
    )  # type: ignore[arg-type]


def _verified_case() -> tuple[G2Vector, object, PaperKernelProfiles]:
    vector = build_g2_vector()
    profiles = _profiles(vector)
    verified = build_verified_run_inputs(
        vector.run_receipt_record,
        vector.admitted_candidates,
        vector.source_receipt_records,
        vector.resolver,
        profiles,
    )
    return vector, verified, profiles


def test_kernel_replay_is_byte_stable_and_retains_vertical_records() -> None:
    """Breaks if kernel output uses ambient state or omits a vertical record family."""

    from build_finance.live_paper.kernel import run_offline_paper_kernel
    from build_finance.live_paper.projections import kernel_result_canonical_bytes

    vector, verified, profiles = _verified_case()
    first = run_offline_paper_kernel(verified, vector.resolver, profiles)
    second = run_offline_paper_kernel(verified, vector.resolver, profiles)

    assert first == second
    assert kernel_result_canonical_bytes(first) == kernel_result_canonical_bytes(second)
    assert first.closure.status == "CLOSED"
    assert first.closure.reason_codes == ()
    assert first.closure.failure_boundary is None
    assert first.closure.ledger_head_id is not None
    assert first.ledger_records == first.store.ledger_records

    assert len(first.event_groups) == 2
    assert len(first.feature_snapshot_records) == 2
    assert len(first.algorithm_candidate_records) == 2
    assert len(first.fusion_decision_records) == 2
    assert len(first.decision_group_manifest_records) == 2
    assert len(first.risk_decision_records) == 2
    assert len(first.simulated_order_intent_records) == 0
    assert len(first.simulated_fill_receipt_records) == 0
    assert len(first.reconciliation_receipt_records) == 1
    assert len(first.portfolio_state_records) == 1
    assert len(first.ledger_records) == 0

    final_state = parse_canonical_record(first.final_portfolio_state_record)
    require_valid_replay_contract(final_state, expected_schema="trading.portfolio-state/v1")
    assert first.projection.cash_quote_atoms == final_state["balances"][0]["total_atoms"]
    assert first.projection.equity_quote_atoms == final_state["summary"]["equity_quote_atoms"]
    assert first.projection.receipt_ids.ledger_head_id == first.closure.ledger_head_id
    assert first.projection.fills == ()
    assert tuple(row.disposition for row in first.projection.model_validation) == ("ABSTAIN", "ABSTAIN")
    assert tuple(decision.decision_sequence for decision in first.projection.decisions) == ("1", "2")


def test_kernel_replay_is_independent_of_resolver_hit_history() -> None:
    """Breaks if resolver access counters or object identity leak into output bytes."""

    from build_finance.live_paper.kernel import run_offline_paper_kernel
    from build_finance.live_paper.projections import kernel_result_canonical_bytes

    vector, verified, profiles = _verified_case()
    first = run_offline_paper_kernel(verified, vector.resolver, profiles)
    resolver_with_history = vector.resolver.with_resolved_bytes("0" * 64, b"unread bytes")
    second = run_offline_paper_kernel(verified, resolver_with_history, profiles)

    assert kernel_result_canonical_bytes(first) == kernel_result_canonical_bytes(second)


def test_kernel_result_is_immutable_and_snapshots_profile_bytes() -> None:
    """Breaks if retained kernel evidence can be mutated after the run returns."""

    from build_finance.live_paper.kernel import run_offline_paper_kernel

    vector, verified, profiles = _verified_case()
    result = run_offline_paper_kernel(verified, vector.resolver, profiles)

    try:
        result.portfolio_state_records += (b"extra\n",)  # type: ignore[misc]
    except Exception as error:
        assert type(error).__name__ in {"FrozenInstanceError", "AttributeError"}
    else:  # pragma: no cover - this branch is the failure signal.
        raise AssertionError("PaperKernelResult accepted mutation")

    mutated_profiles = replace(profiles, risk_config_record=bytearray(profiles.risk_config_record))
    replayed = run_offline_paper_kernel(verified, vector.resolver, mutated_profiles)
    assert replayed.profile_records.risk_config_record == profiles.risk_config_record
