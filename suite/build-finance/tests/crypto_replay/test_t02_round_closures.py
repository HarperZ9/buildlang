"""T02 RED contracts owned by replay closure rounds 9 through 18.

Every fixture is synthetic and local.  These tests close serialization,
retained-byte, and deterministic arithmetic contracts only; they never run a
replay, infer a signal, append a ledger, or expose an actuator.
"""

from __future__ import annotations

import hashlib
import importlib
import os
import stat
import subprocess
from collections.abc import Callable
from contextlib import contextmanager
from copy import deepcopy
from dataclasses import replace
from pathlib import Path
from typing import Any

import pytest

from build_finance.crypto_replay.canonical import (
    canonical_json_bytes,
    canonical_record_bytes,
    parse_canonical_record,
    sha256_hex,
)
from tests.crypto_replay.test_t02_supporting_contracts import (
    MARKET_ID,
    MAX_PROOF_ROWS,
    MAX_U64,
    _digest,
    _reseal,
    build_t02_vector,
)

RESOURCE_ROOT = Path(__file__).resolve().parents[2] / "build_finance" / "crypto_replay" / "resources"
EXPECTED_SCHEMA_BUNDLE_SHA256 = "2ae775112ca71cf95c233468529289af3ea04c284dda3da8d4452e9156cec1b9"


def _future_symbol(module_name: str, symbol_name: str, capability: str) -> Any:
    """Require one delayed T02 capability without hiding dependency errors."""
    missing_message: str | None = None
    try:
        module = importlib.import_module(module_name)
    except ModuleNotFoundError as error:
        if error.name == module_name:
            missing_message = f"T02 RED - missing {capability} in {module_name}"
        else:
            raise
    if missing_message is not None:
        pytest.fail(missing_message, pytrace=False)
    value = getattr(module, symbol_name, None)
    if value is None:
        pytest.fail(
            f"T02 RED - missing {capability}: {module_name}.{symbol_name}",
            pytrace=False,
        )
    return value


def _assert_contract_valid(document: dict[str, Any], capability: str) -> None:
    from build_finance.crypto_replay.schema_registry import validate_contract

    issues = validate_contract(document, expected_schema=str(document["schema"]))
    assert not issues, f"T02 RED - missing {capability}: {issues!r}"


def _assert_contract_invalid(document: dict[str, Any], capability: str) -> None:
    from build_finance.crypto_replay.schema_registry import validate_contract

    issues = validate_contract(document, expected_schema=str(document["schema"]))
    assert issues, f"T02 RED - {capability} mutation unexpectedly valid"


def _contract_issue_codes(document: dict[str, Any]) -> set[str]:
    from build_finance.crypto_replay.schema_registry import validate_contract

    return {issue.code for issue in validate_contract(document, expected_schema=str(document["schema"]))}


def _quarantine(
    *,
    reason_codes: list[str],
    slot_observations: list[dict[str, Any]],
    component_observations: list[dict[str, Any]],
    first_unsafe_ledger_sequence: str | None,
    anchored: bool,
) -> dict[str, Any]:
    vector = build_t02_vector()
    base = vector.documents["trading.execution-quarantine-receipt/v1"]
    anchor_head = _digest("last-verified-ledger-head") if anchored else None
    anchor_state = _digest("last-verified-portfolio-state") if anchored else None
    return _reseal(
        {
            **base,
            "reason_codes": reason_codes,
            "last_verified_ledger_head_id": anchor_head,
            "last_verified_portfolio_state_id": anchor_state,
            "first_unsafe_ledger_sequence": first_unsafe_ledger_sequence,
            "slot_observations": slot_observations,
            "component_observations": component_observations,
        }
    )


def _slot_observation(classification: str, sequence: str = "0") -> dict[str, Any]:
    expected = None if classification == "EXTRA" else _digest("expected-ledger-record")
    observed = None if classification == "MISSING" else _digest("observed-ledger-record")
    observed_bytes = None if classification == "MISSING" else _digest("observed-ledger-bytes")
    return {
        "ledger_sequence": sequence,
        "expected_ledger_record_id": expected,
        "observed_ledger_record_id": observed,
        "observed_byte_sha256": observed_bytes,
        "classification": classification,
    }


def _component_observation(component_kind: str, classification: str) -> dict[str, Any]:
    return {
        "component_kind": component_kind,
        "ordinal": None,
        "expected_component_sha256": (None if classification == "EXTRA" else _digest(f"expected-{component_kind}")),
        "observed_component_sha256": (None if classification == "MISSING" else _digest(f"observed-{component_kind}")),
        "classification": classification,
    }


def test_t02_schema_bundle_digest_is_pinned_independently() -> None:
    bundle_record = (RESOURCE_ROOT / "schema-bundle.json").read_bytes()
    digest_record = (RESOURCE_ROOT / "schema-bundle.sha256").read_text(encoding="ascii")
    lock = parse_canonical_record((RESOURCE_ROOT / "schema-lock.json").read_bytes())

    bundle = parse_canonical_record(bundle_record)
    observed_digest = sha256_hex(canonical_json_bytes(bundle))
    assert observed_digest == EXPECTED_SCHEMA_BUNDLE_SHA256
    assert digest_record == f"{EXPECTED_SCHEMA_BUNDLE_SHA256}\n"
    assert lock["schema_bundle_sha256"] == EXPECTED_SCHEMA_BUNDLE_SHA256
    assert lock["primary_contract_count"] == 8
    assert lock["supporting_contract_count"] == 13
    assert lock["json_attachment_schema_count"] == 27
    assert lock["generated_json_schema_count"] == 48
    assert lock["total_authority_contract_count"] == 49


def test_initial_prefix_missing_anchor_emits_out_of_band_quarantine() -> None:
    receipt = _quarantine(
        reason_codes=[
            "QUARANTINE_INITIAL_PREFIX_ANCHOR_MISSING",
            "QUARANTINE_LEDGER_SLOT_MISSING",
            "QUARANTINE_FOOTPRINT_MISMATCH",
        ],
        slot_observations=[_slot_observation("MISSING")],
        component_observations=[],
        first_unsafe_ledger_sequence="0",
        anchored=False,
    )
    _assert_contract_valid(receipt, "initial-prefix quarantine schema and semantics")
    assert receipt["last_verified_ledger_head_id"] is None
    assert receipt["last_verified_portfolio_state_id"] is None
    assert receipt["first_unsafe_ledger_sequence"] == "0"


def test_completed_footprint_wrong_slot_emits_out_of_band_quarantine() -> None:
    receipt = _quarantine(
        reason_codes=[
            "QUARANTINE_LEDGER_SLOT_WRONG",
            "QUARANTINE_APPEND_SLOT_OCCUPIED",
            "QUARANTINE_FOOTPRINT_MISMATCH",
        ],
        slot_observations=[_slot_observation("WRONG", "8")],
        component_observations=[],
        first_unsafe_ledger_sequence="8",
        anchored=True,
    )
    _assert_contract_valid(receipt, "occupied wrong-slot quarantine classification")
    mutation = deepcopy(receipt)
    mutation["slot_observations"][0]["observed_byte_sha256"] = None
    _assert_contract_invalid(_reseal(mutation), "wrong slot must retain occupied bytes")


def test_completed_footprint_extra_slot_emits_out_of_band_quarantine() -> None:
    receipt = _quarantine(
        reason_codes=[
            "QUARANTINE_LEDGER_SLOT_EXTRA",
            "QUARANTINE_APPEND_SLOT_OCCUPIED",
            "QUARANTINE_FOOTPRINT_MISMATCH",
        ],
        slot_observations=[_slot_observation("EXTRA", "9")],
        component_observations=[],
        first_unsafe_ledger_sequence="9",
        anchored=True,
    )
    _assert_contract_valid(receipt, "occupied extra-slot quarantine classification")
    mutation = deepcopy(receipt)
    mutation["slot_observations"][0]["expected_ledger_record_id"] = _digest("forbidden-expected")
    _assert_contract_invalid(_reseal(mutation), "extra slot forbids an expected ID")


def test_initial_prefix_sequence_zero_missing_wrong_extra_vectors_serialize() -> None:
    reason_suffixes = {
        "MISSING": ["QUARANTINE_LEDGER_SLOT_MISSING", "QUARANTINE_FOOTPRINT_MISMATCH"],
        "WRONG": [
            "QUARANTINE_LEDGER_SLOT_WRONG",
            "QUARANTINE_APPEND_SLOT_OCCUPIED",
            "QUARANTINE_FOOTPRINT_MISMATCH",
        ],
        "EXTRA": [
            "QUARANTINE_LEDGER_SLOT_EXTRA",
            "QUARANTINE_APPEND_SLOT_OCCUPIED",
            "QUARANTINE_FOOTPRINT_MISMATCH",
        ],
    }
    for classification, suffix in reason_suffixes.items():
        receipt = _quarantine(
            reason_codes=["QUARANTINE_INITIAL_PREFIX_ANCHOR_MISSING", *suffix],
            slot_observations=[_slot_observation(classification)],
            component_observations=[],
            first_unsafe_ledger_sequence="0",
            anchored=False,
        )
        assert parse_canonical_record(canonical_record_bytes(receipt)) == receipt
        _assert_contract_valid(receipt, f"sequence-zero {classification} quarantine vector")


def test_component_only_missing_claim_emits_quarantine() -> None:
    receipt = _quarantine(
        reason_codes=["QUARANTINE_COMPONENT_MISSING", "QUARANTINE_FOOTPRINT_MISMATCH"],
        slot_observations=[],
        component_observations=[_component_observation("CLAIMED_REQUEST_KEY", "MISSING")],
        first_unsafe_ledger_sequence=None,
        anchored=True,
    )
    _assert_contract_valid(receipt, "component-only missing request-claim quarantine")
    assert receipt["component_observations"][0]["observed_component_sha256"] is None


def test_component_only_wrong_counter_emits_quarantine() -> None:
    receipt = _quarantine(
        reason_codes=["QUARANTINE_COMPONENT_WRONG", "QUARANTINE_FOOTPRINT_MISMATCH"],
        slot_observations=[],
        component_observations=[_component_observation("NEXT_LEDGER_SEQUENCE", "WRONG")],
        first_unsafe_ledger_sequence=None,
        anchored=True,
    )
    _assert_contract_valid(receipt, "component-only wrong-counter quarantine")
    observation = receipt["component_observations"][0]
    assert observation["expected_component_sha256"] != observation["observed_component_sha256"]


def test_component_only_missing_resulting_state_emits_quarantine() -> None:
    receipt = _quarantine(
        reason_codes=["QUARANTINE_COMPONENT_MISSING", "QUARANTINE_FOOTPRINT_MISMATCH"],
        slot_observations=[],
        component_observations=[_component_observation("RESULTING_PORTFOLIO_STATE", "MISSING")],
        first_unsafe_ledger_sequence=None,
        anchored=True,
    )
    _assert_contract_valid(receipt, "component-only missing resulting-state quarantine")
    assert receipt["first_unsafe_ledger_sequence"] is None


def _capacity_from_counts(
    *,
    market_count: int,
    model_candidate_count: int,
    model_signal_mode: str,
    source_admission_count: int = 2,
    raw_event_count: int = 2,
    availability_group_count: int = 2,
) -> dict[str, Any]:
    """Call the plan-authorized private count-only arithmetic probe."""
    totalize = _future_symbol(
        "build_finance.crypto_replay.run_inputs",
        "_derive_counter_capacity_from_counts",
        "count-only counter-capacity arithmetic probe",
    )
    return dict(
        totalize(
            source_admission_count=source_admission_count,
            raw_event_count=raw_event_count,
            availability_group_count=availability_group_count,
            market_count=market_count,
            model_candidate_count=model_candidate_count,
            model_signal_mode=model_signal_mode,
        )
    )


def _expected_capacity_bounds(
    *,
    source_count: int,
    raw_count: int,
    group_count: int,
    market_count: int,
    candidate_count: int,
) -> dict[str, int | None]:
    decision_count = market_count * group_count
    values = {
        "decision_attempt_upper_bound": decision_count,
        "decision_sequence_next_upper_bound": decision_count + 1,
        "producer_sequence_next_upper_bound": candidate_count,
        "intent_sequence_next_upper_bound": decision_count + 1,
        "fill_receipt_sequence_next_upper_bound": decision_count + 1,
        "unmatched_reservation_count_upper_bound": 2 * decision_count,
        "state_sequence_next_upper_bound": 3 * decision_count + 2 * group_count + 3,
        "ledger_sequence_next_upper_bound": (
            source_count + raw_count + 10 * decision_count + 2 * candidate_count + 5 * group_count + 9
        ),
    }
    return {name: value if value <= MAX_U64 else None for name, value in values.items()}


def test_counter_capacity_preflight_is_total() -> None:
    expected = _expected_capacity_bounds(
        source_count=2,
        raw_count=2,
        group_count=2,
        market_count=1,
        candidate_count=0,
    )
    derived = _capacity_from_counts(
        market_count=1,
        model_candidate_count=0,
        model_signal_mode="DISABLED",
    )
    assert {name: derived[name] for name in expected} == {
        name: None if value is None else str(value) for name, value in expected.items()
    }
    assert derived["status"] == "WITHIN_LIMIT"
    assert derived["first_exceeded_counter"] is None
    totalize_bounds = _future_symbol(
        "build_finance.crypto_replay.run_inputs",
        "_totalize_counter_capacity_bounds",
        "ordered first-exceeded counter totalizer",
    )
    counter_order = (
        ("decision_attempt_upper_bound", "DECISION_ATTEMPT"),
        ("decision_sequence_next_upper_bound", "DECISION_SEQUENCE"),
        ("producer_sequence_next_upper_bound", "PRODUCER_SEQUENCE"),
        ("intent_sequence_next_upper_bound", "INTENT_SEQUENCE"),
        ("fill_receipt_sequence_next_upper_bound", "FILL_RECEIPT_SEQUENCE"),
        ("unmatched_reservation_count_upper_bound", "UNMATCHED_RESERVATION_COUNT"),
        ("state_sequence_next_upper_bound", "STATE_SEQUENCE"),
        ("ledger_sequence_next_upper_bound", "LEDGER_SEQUENCE"),
    )
    exact_boundary_inputs = {field: MAX_U64 for field, _ in counter_order}
    exact_boundary = totalize_bounds(exact_boundary_inputs)
    assert exact_boundary["status"] == "WITHIN_LIMIT"
    assert exact_boundary["first_exceeded_counter"] is None
    assert all(exact_boundary[field] == str(MAX_U64) for field, _ in counter_order)
    for index, (_first_field, first_code) in enumerate(counter_order):
        unbounded = {
            field: MAX_U64 if position < index else MAX_U64 + position - index + 1
            for position, (field, _) in enumerate(counter_order)
        }
        totalized = totalize_bounds(unbounded)
        assert totalized["status"] == "EXCEEDED"
        assert totalized["first_exceeded_counter"] == first_code
        for position, (field, _) in enumerate(counter_order):
            assert totalized[field] == (str(MAX_U64) if position < index else None)

    from tests.crypto_replay.test_t02_cross_artifact import _make_bundle

    derive_from_retained = _future_symbol(
        "build_finance.crypto_replay.run_inputs",
        "derive_counter_capacity",
        "counter capacity derived from retained arrays and bodies",
    )
    vector = build_t02_vector()
    bundle = _make_bundle(vector)
    retained = derive_from_retained(bundle)
    assert retained == vector.attachments["trading.counter-capacity/v1"]

    caller_counts = {
        **vector.attachments["trading.counter-capacity/v1"],
        "source_admission_count": "999",
        "raw_event_count": "999",
        "availability_group_count": "999",
        "market_count": "999",
        "model_candidate_count": "999",
    }
    assert derive_from_retained(replace(bundle, counter_capacity=caller_counts)) == retained

    # Task 9 predecessor-test correction: retained graph derivation is fail-closed,
    # so dropping one admitted source's event cannot become a smaller valid run.
    with pytest.raises(ValueError):
        derive_from_retained(replace(bundle, normalized_events=(vector.raw_events[0],)))
    one_group_schedule = deepcopy(vector.attachments["trading.availability-schedule/v1"])
    one_group_schedule["availability_groups"] = one_group_schedule["availability_groups"][:1]
    one_group = derive_from_retained(replace(bundle, availability_schedule=one_group_schedule))
    assert one_group == retained

    totalize_counts = _future_symbol(
        "build_finance.crypto_replay.run_inputs",
        "_derive_counter_capacity_from_counts",
        "closed count-input validation",
    )
    common_counts = {
        "source_admission_count": 2,
        "raw_event_count": 2,
        "availability_group_count": 2,
        "market_count": 1,
        "model_candidate_count": 0,
        "model_signal_mode": "DISABLED",
    }
    one_event_group = totalize_counts(
        **{
            **common_counts,
            "raw_event_count": 1,
            "availability_group_count": 1,
        }
    )
    assert one_event_group["raw_event_count"] == "1"
    assert one_event_group["availability_group_count"] == "1"
    assert one_event_group["decision_attempt_upper_bound"] == "1"
    for field, wrong in (
        ("source_admission_count", True),
        ("raw_event_count", -1),
        ("availability_group_count", 0),
        ("source_admission_count", MAX_U64 + 1),
        ("raw_event_count", MAX_U64 + 1),
        ("availability_group_count", MAX_U64 + 1),
        ("market_count", 1.0),
        ("model_candidate_count", "0"),
        ("model_signal_mode", "UNKNOWN"),
    ):
        with pytest.raises(ValueError):
            totalize_counts(**{**common_counts, field: wrong})


def test_run_closure_failed_field_presence_matrix_is_exact() -> None:
    precedence = (
        "CLOSURE_H_MISSING",
        "CLOSURE_CANDIDATE_CARDINALITY",
        "CLOSURE_CANDIDATE_SCHEMA_INVALID",
        "CLOSURE_CANDIDATE_NON_EXECUTABLE",
        "CLOSURE_IDENTITY_MISMATCH",
        "CLOSURE_DECIMALS_MISMATCH",
        "CLOSURE_Q_CAP_RANGE",
        "CLOSURE_CAPACITY_INSUFFICIENT",
        "CLOSURE_REFERENCE_SET_INVALID",
        "CLOSURE_PROOF_ROW_COUNT_RANGE",
        "CLOSURE_STATE_ENVELOPE_RANGE",
        "CLOSURE_PROOF_PREDICATE_FAILED",
    )
    always_present = {"market_id", "failure_codes", "adverse_fill_extreme_count"}
    candidate_fields = {
        "fill_event_id",
        "capacity_base_atoms",
        "fill_candidate_semantic_sha256",
    }
    independent_fields = {
        "q_cap_base_atoms",
        "reference_price_count",
        "reference_set_root_sha256",
        "proof_row_count",
    }
    base = _force_market_row()
    matrix: list[tuple[str, dict[str, Any], set[str], str | None, str]] = []

    h_missing = {**base, "failure_codes": [precedence[0]]}
    for field in (
        "earliest_trigger_equal_time_group",
        *candidate_fields,
        "state_envelope_sha256",
    ):
        h_missing[field] = None
    matrix.append(
        (
            "H_MISSING",
            h_missing,
            always_present | independent_fields,
            "2",
            "WITHIN_LIMIT",
        )
    )
    for code in precedence[1:3]:
        row = {**base, "failure_codes": [code]}
        for field in candidate_fields:
            row[field] = None
        matrix.append(
            (
                code,
                row,
                always_present | independent_fields | {"earliest_trigger_equal_time_group", "state_envelope_sha256"},
                "2",
                "WITHIN_LIMIT",
            )
        )
    for code in precedence[3:6]:
        matrix.append(
            (
                code,
                {**base, "failure_codes": [code]},
                always_present
                | independent_fields
                | candidate_fields
                | {"earliest_trigger_equal_time_group", "state_envelope_sha256"},
                "2",
                "WITHIN_LIMIT",
            )
        )
    q_range = {
        **base,
        "failure_codes": [precedence[6], precedence[10]],
        "q_cap_base_atoms": None,
        "proof_row_count": None,
        "state_envelope_sha256": None,
    }
    matrix.append(
        (
            "Q_CAP_RANGE",
            q_range,
            always_present
            | candidate_fields
            | {
                "earliest_trigger_equal_time_group",
                "reference_price_count",
                "reference_set_root_sha256",
            },
            "0",
            "WITHIN_LIMIT",
        )
    )
    insufficient = {
        **_force_market_row(q_cap_base_atoms="2", proof_row_count="4"),
        "failure_codes": [precedence[7]],
        "capacity_base_atoms": "1",
    }
    matrix.append(
        (
            "CAPACITY_INSUFFICIENT",
            insufficient,
            always_present
            | independent_fields
            | candidate_fields
            | {"earliest_trigger_equal_time_group", "state_envelope_sha256"},
            "4",
            "WITHIN_LIMIT",
        )
    )
    bad_reference = {
        **base,
        "failure_codes": [precedence[8]],
        "reference_price_count": None,
        "reference_set_root_sha256": None,
        "proof_row_count": None,
    }
    matrix.append(
        (
            "REFERENCE_SET_INVALID",
            bad_reference,
            always_present
            | candidate_fields
            | {"earliest_trigger_equal_time_group", "q_cap_base_atoms", "state_envelope_sha256"},
            "0",
            "WITHIN_LIMIT",
        )
    )
    proof_count_range = {
        **_force_market_row(
            q_cap_base_atoms=str(MAX_U64),
            proof_row_count=None,
        ),
        "failure_codes": [precedence[9]],
    }
    matrix.append(
        (
            "PROOF_ROW_COUNT_RANGE",
            proof_count_range,
            always_present
            | candidate_fields
            | {
                "earliest_trigger_equal_time_group",
                "q_cap_base_atoms",
                "reference_price_count",
                "reference_set_root_sha256",
                "state_envelope_sha256",
            },
            None,
            "EXCEEDED",
        )
    )
    state_range = {**base, "failure_codes": [precedence[10]], "state_envelope_sha256": None}
    matrix.append(
        (
            "STATE_ENVELOPE_RANGE",
            state_range,
            always_present | independent_fields | candidate_fields | {"earliest_trigger_equal_time_group"},
            "2",
            "WITHIN_LIMIT",
        )
    )
    proof_failed = {
        **base,
        "failure_codes": [precedence[11]],
        "proof_domain": "ALL_RESIDUAL_REFERENCE_PAIRS_AT_FILL_EXTREMES_V1",
        "proof_root_sha256": None,
    }
    matrix.append(
        (
            "PROOF_PREDICATE_FAILED",
            proof_failed,
            always_present
            | independent_fields
            | candidate_fields
            | {"earliest_trigger_equal_time_group", "state_envelope_sha256", "proof_domain"},
            "2",
            "WITHIN_LIMIT",
        )
    )
    assert len(matrix) == 12
    nullable_fields = {
        "earliest_trigger_equal_time_group",
        "fill_event_id",
        "q_cap_base_atoms",
        "capacity_base_atoms",
        "fill_candidate_semantic_sha256",
        "reference_price_count",
        "state_envelope_sha256",
        "proof_domain",
        "reference_set_root_sha256",
        "proof_root_sha256",
        "proof_row_count",
    }
    fabricated_values = {
        "earliest_trigger_equal_time_group": "3",
        "fill_event_id": _digest("inverse-fill-event"),
        "q_cap_base_atoms": "1",
        "capacity_base_atoms": "1",
        "fill_candidate_semantic_sha256": _digest("inverse-candidate-semantic"),
        "reference_price_count": "1",
        "state_envelope_sha256": _digest("inverse-state-envelope"),
        "proof_domain": "ALL_RESIDUAL_REFERENCE_PAIRS_AT_FILL_EXTREMES_V1",
        "reference_set_root_sha256": _digest("inverse-reference-set"),
        "proof_root_sha256": _digest("inverse-proof-root"),
        "proof_row_count": "2",
    }
    for index, (case, row, expected_nonnull, total, budget_status) in enumerate(matrix):
        assert row["failure_codes"] == [code for code in precedence if code in row["failure_codes"]]
        actual_nonnull = {field for field in nullable_fields if row[field] is not None}
        assert actual_nonnull == expected_nonnull - always_present, case
        reasons = (
            ["ADMISSION_RUN_END_PROOF_BUDGET", "ADMISSION_RUN_END_UNCLOSED"]
            if budget_status == "EXCEEDED"
            else ["ADMISSION_RUN_END_UNCLOSED"]
        )
        closure = _force_closure(
            status="FAIL",
            reason_codes=reasons,
            proof_row_limit=str(MAX_PROOF_ROWS),
            proof_row_count_total=total,
            proof_budget_status=budget_status,
            market_proofs=[row],
        )
        _assert_contract_valid(closure, f"failed-field matrix row {case}")
        inverse_row = deepcopy(row)
        if index % 2 == 0:
            required_present = sorted(actual_nonnull)[0]
            inverse_row[required_present] = None
        else:
            required_null = sorted(nullable_fields - actual_nonnull)[0]
            inverse_row[required_null] = fabricated_values[required_null]
        inverse_closure = _force_closure(
            status="FAIL",
            reason_codes=reasons,
            proof_row_limit=str(MAX_PROOF_ROWS),
            proof_row_count_total=total,
            proof_budget_status=budget_status,
            market_proofs=[inverse_row],
        )
        _assert_contract_invalid(
            inverse_closure,
            f"failed-field inverse row {case}",
        )


def test_counter_capacity_binds_selected_model_manifest_count() -> None:
    from dataclasses import replace

    from tests.crypto_replay.test_t02_cross_artifact import (
        _assert_rejected,
        _build_cached_model_case,
    )

    derive_counter_capacity = _future_symbol(
        "build_finance.crypto_replay.run_inputs",
        "derive_counter_capacity",
        "retained-body counter-capacity derivation",
    )
    vector = build_t02_vector()
    cached_run, cached_bundle = _build_cached_model_case(vector)
    manifest = vector.documents["trading.model-signal-manifest/v1"]
    baseline = derive_counter_capacity(cached_bundle)
    assert baseline["model_signal_mode"] == "CACHED_FIXTURES"
    assert baseline["model_candidate_count"] == str(len(manifest["candidates"]))
    assert baseline["model_registry_sha256"] == vector.documents["trading.model-registry/v1"]["model_registry_sha256"]
    assert baseline["model_signal_manifest_sha256"] == manifest["model_signal_manifest_sha256"]

    disabled_closure = _reseal(
        {
            **cached_bundle.run_closure_receipt,
            "model_signal_mode": "DISABLED",
            "model_registry_sha256": None,
            "model_signal_manifest_sha256": None,
        }
    )
    disabled_run = _reseal(
        {
            **cached_run,
            "model_signal_mode": "DISABLED",
            "model_registry_sha256": None,
            "model_signal_manifest_sha256": None,
            "selected_model_scopes": [],
            "model_decision_budget_ns": "0",
            "run_closure_receipt_id": disabled_closure["run_closure_receipt_id"],
        }
    )
    _assert_rejected(
        vector,
        run_receipt=disabled_run,
        bundle=replace(
            cached_bundle,
            run_closure_receipt=disabled_closure,
            model_registry=None,
            model_signal_manifest=None,
        ),
    )

    stale_manifest = deepcopy(manifest)
    stale_manifest["candidates"][0]["raw_byte_length"] = str(
        int(stale_manifest["candidates"][0]["raw_byte_length"]) + 1
    )
    _assert_rejected(
        vector,
        run_receipt=cached_run,
        bundle=replace(cached_bundle, model_signal_manifest=stale_manifest),
    )
    wrong_self_id = deepcopy(manifest)
    wrong_self_id["model_signal_manifest_sha256"] = _digest("wrong-selected-manifest-id")
    _assert_rejected(
        vector,
        run_receipt=cached_run,
        bundle=replace(cached_bundle, model_signal_manifest=wrong_self_id),
    )

    expanded = deepcopy(manifest)
    second = deepcopy(expanded["candidates"][0])
    second.update(
        {
            "relative_path": "signals/decision-2.json",
            "raw_signal_sha256": _digest("second-candidate-bytes"),
            "raw_byte_length": "1",
            "declared_signal_id": None,
            "feature_snapshot_id": _digest("feature-snapshot-2"),
            "decision_sequence": "2",
            "producer_sequence": None,
            "producer_scope_key_sha256": None,
            "issued_replay_clock_ns": "1",
            "available_replay_clock_ns": "2",
            "decision_close_replay_clock_ns": "3",
        }
    )
    expanded["candidates"].append(second)
    expanded = _reseal(expanded)
    expanded_closure = _reseal(
        {
            **cached_bundle.run_closure_receipt,
            "model_signal_manifest_sha256": expanded["model_signal_manifest_sha256"],
        }
    )
    expanded_run = _reseal(
        {
            **cached_run,
            "model_signal_manifest_sha256": expanded["model_signal_manifest_sha256"],
            "selected_model_scopes": [
                *cached_run["selected_model_scopes"],
                {
                    "market_id": MARKET_ID,
                    "decision_sequence": "2",
                    "horizon_ns": second["horizon_ns"],
                    "producer_scope_key_sha256": second["requested_producer_scope_key_sha256"],
                },
            ],
            "run_closure_receipt_id": expanded_closure["run_closure_receipt_id"],
        }
    )
    expanded_bundle = replace(
        cached_bundle,
        run_closure_receipt=expanded_closure,
        model_signal_manifest=expanded,
    )
    expanded_capacity = derive_counter_capacity(expanded_bundle)
    assert expanded_capacity["model_candidate_count"] == "2"
    assert expanded_capacity["producer_sequence_next_upper_bound"] == "2"
    assert expanded_capacity["ledger_sequence_next_upper_bound"] == "47"
    _assert_rejected(
        vector,
        run_receipt=expanded_run,
        bundle=expanded_bundle,
    )


def test_preclosure_cardinality_caps_are_exact() -> None:
    zero_market = _capacity_from_counts(
        market_count=0,
        model_candidate_count=0,
        model_signal_mode="DISABLED",
    )
    over_market = _capacity_from_counts(
        market_count=MAX_U64 + 1,
        model_candidate_count=0,
        model_signal_mode="DISABLED",
    )
    zero_cached = _capacity_from_counts(
        market_count=1,
        model_candidate_count=0,
        model_signal_mode="CACHED_FIXTURES",
    )
    over_cached = _capacity_from_counts(
        market_count=1,
        model_candidate_count=MAX_U64 + 1,
        model_signal_mode="CACHED_FIXTURES",
    )
    assert zero_market["admissible"] is False
    assert over_market["admissible"] is False
    assert zero_cached["admissible"] is False
    assert over_cached["admissible"] is False


def _source_tree_api() -> tuple[Callable[..., Any], type[Exception], Any]:
    scan = _future_symbol(
        "build_finance.crypto_replay.source_tree",
        "scan_source_tree",
        "canonical no-follow source-tree scanner",
    )
    error_type = _future_symbol(
        "build_finance.crypto_replay.source_tree",
        "SourceTreeError",
        "typed source-tree rejection",
    )
    module = importlib.import_module("build_finance.crypto_replay.source_tree")
    return scan, error_type, module


def _assert_source_tree_run_input_rejected(vector: Any, observed_source_tree: dict[str, Any]) -> None:
    from tests.crypto_replay.test_t02_cross_artifact import _assert_rejected, _make_bundle

    run_receipt = vector.documents["trading.run-receipt/v1"]
    expected_digest = run_receipt["source_tree_sha256"]
    observed_digest = sha256_hex(canonical_json_bytes(observed_source_tree))
    assert expected_digest == sha256_hex(canonical_json_bytes(vector.attachments["trading.source-tree/v1"]))
    assert observed_digest != expected_digest
    _assert_rejected(vector, bundle=_make_bundle(vector, source_tree=observed_source_tree))


def _write_source_fixture(root: Path) -> None:
    source = root / "src"
    source.mkdir(parents=True)
    (source / "kernel.py").write_bytes(b"abc")
    (source / "risk.py").write_bytes(b"")


@contextmanager
def _block_real_file_reads(path: Path) -> Any:
    if os.name == "nt":
        import ctypes
        from ctypes import wintypes

        create_file = ctypes.windll.kernel32.CreateFileW
        create_file.argtypes = (
            wintypes.LPCWSTR,
            wintypes.DWORD,
            wintypes.DWORD,
            wintypes.LPVOID,
            wintypes.DWORD,
            wintypes.DWORD,
            wintypes.HANDLE,
        )
        create_file.restype = wintypes.HANDLE
        handle = create_file(str(path), 0x80000000, 0, None, 3, 0x80, None)
        invalid_handle = wintypes.HANDLE(-1).value
        assert handle not in (None, invalid_handle)
        try:
            yield
        finally:
            assert ctypes.windll.kernel32.CloseHandle(handle)
    else:
        mode = path.stat().st_mode
        path.chmod(0)
        try:
            yield
        finally:
            path.chmod(mode)


def test_source_tree_preimage_is_canonical(tmp_path: Path) -> None:
    scan, source_tree_error, module = _source_tree_api()
    _write_source_fixture(tmp_path)
    body = scan(tmp_path, root_label="repository-root")
    expected = build_t02_vector().attachments["trading.source-tree/v1"]
    assert canonical_json_bytes(body) == canonical_json_bytes(expected)
    assert sha256_hex(canonical_json_bytes(body)) == (
        "6e713ecd2fe3705e33044ebff33f8411cf3f15fab8d7bbcb198265f2d682fc54"
    )
    assert [row["relative_path"] for row in body["files"]] == [
        "src/kernel.py",
        "src/risk.py",
    ]
    _assert_contract_valid(body, "canonical source-tree attachment")
    invalid_bodies: list[dict[str, Any]] = []
    for unsafe_path in ("src\\kernel.py", "../kernel.py", ".git/config"):
        mutation = deepcopy(body)
        mutation["files"][0]["relative_path"] = unsafe_path
        invalid_bodies.append(mutation)
    reversed_rows = deepcopy(body)
    reversed_rows["files"].reverse()
    invalid_bodies.append(reversed_rows)
    for mutation in invalid_bodies:
        with pytest.raises(source_tree_error):
            module.reconstruct_source_tree(mutation)

    original_digest = sha256_hex(canonical_json_bytes(body))
    (tmp_path / "src" / "kernel.py").write_bytes(b"abd")
    changed = scan(tmp_path, root_label="repository-root")
    assert changed["files"][0]["file_sha256"] == sha256_hex(b"abd")
    assert sha256_hex(canonical_json_bytes(changed)) != original_digest


def test_source_tree_rejects_missing_extra_nonregular_unreadable_and_reparse_entries(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    scan, source_tree_error, module = _source_tree_api()
    vector = build_t02_vector()

    baseline_root = tmp_path / "baseline"
    _write_source_fixture(baseline_root)
    baseline = scan(baseline_root, root_label="repository-root")
    assert canonical_json_bytes(baseline) == canonical_json_bytes(vector.attachments["trading.source-tree/v1"])
    from tests.crypto_replay.test_t02_cross_artifact import _make_bundle, _verify

    verified = _verify(vector, bundle=_make_bundle(vector, source_tree=baseline))
    assert verified.authority == "CONTRACT_ONLY"

    (baseline_root / "src" / "risk.py").unlink()
    missing = scan(baseline_root, root_label="repository-root")
    assert canonical_json_bytes(missing) != canonical_json_bytes(baseline)
    assert [row["relative_path"] for row in missing["files"]] == ["src/kernel.py"]
    _assert_source_tree_run_input_rejected(vector, missing)

    (baseline_root / "src" / "risk.py").write_bytes(b"")
    (baseline_root / "src" / "extra.py").write_bytes(b"extra")
    extra = scan(baseline_root, root_label="repository-root")
    assert canonical_json_bytes(extra) != canonical_json_bytes(baseline)
    assert "src/extra.py" in {row["relative_path"] for row in extra["files"]}
    _assert_source_tree_run_input_rejected(vector, extra)

    nonregular_root = tmp_path / "nonregular"
    _write_source_fixture(nonregular_root)
    nonregular = nonregular_root / "src" / "risk.py"
    if hasattr(os, "mkfifo"):
        nonregular.unlink()
        os.mkfifo(nonregular)
        with pytest.raises(source_tree_error, match="regular|type|kind|entry"):
            scan(nonregular_root, root_label="repository-root")
    else:
        original_lstat = module.os.lstat
        original_stat = module.os.stat
        original_scandir = module.os.scandir
        target_path = os.path.normcase(os.path.abspath(nonregular))

        def is_target(path: Any) -> bool:
            try:
                return os.path.normcase(os.path.abspath(os.fspath(path))) == target_path
            except TypeError:
                return False

        class NonregularMetadata:
            def __init__(self, metadata: Any) -> None:
                self._metadata = metadata
                self.st_mode = stat.S_IFIFO | stat.S_IRUSR | stat.S_IWUSR

            def __getattr__(self, name: str) -> Any:
                return getattr(self._metadata, name)

        def classify_nonregular(metadata: Any, path: Any) -> Any:
            return NonregularMetadata(metadata) if is_target(path) else metadata

        def controlled_lstat(path: Any, *args: Any, **kwargs: Any) -> Any:
            return classify_nonregular(original_lstat(path, *args, **kwargs), path)

        def controlled_stat(path: Any, *args: Any, **kwargs: Any) -> Any:
            return classify_nonregular(original_stat(path, *args, **kwargs), path)

        class ControlledDirEntry:
            def __init__(self, entry: Any) -> None:
                self._entry = entry

            def __getattr__(self, name: str) -> Any:
                return getattr(self._entry, name)

            def stat(self, *args: Any, **kwargs: Any) -> Any:
                return classify_nonregular(self._entry.stat(*args, **kwargs), self._entry.path)

            def is_file(self, *args: Any, **kwargs: Any) -> bool:
                return False if is_target(self._entry.path) else self._entry.is_file(*args, **kwargs)

            def is_dir(self, *args: Any, **kwargs: Any) -> bool:
                return False if is_target(self._entry.path) else self._entry.is_dir(*args, **kwargs)

        class ControlledScandir:
            def __init__(self, entries: Any) -> None:
                self._entries = entries

            def __enter__(self) -> ControlledScandir:
                self._entries.__enter__()
                return self

            def __exit__(self, *args: Any) -> Any:
                return self._entries.__exit__(*args)

            def __iter__(self) -> Any:
                return (ControlledDirEntry(entry) for entry in self._entries)

            def close(self) -> None:
                self._entries.close()

        def controlled_scandir(path: Any) -> ControlledScandir:
            return ControlledScandir(original_scandir(path))

        monkeypatch.setattr(module.os, "lstat", controlled_lstat)
        monkeypatch.setattr(module.os, "stat", controlled_stat)
        monkeypatch.setattr(module.os, "scandir", controlled_scandir)
        try:
            with pytest.raises(source_tree_error, match="regular|type|kind|entry"):
                scan(nonregular_root, root_label="repository-root")
        finally:
            monkeypatch.undo()

    unreadable_root = tmp_path / "unreadable"
    _write_source_fixture(unreadable_root)
    unreadable = unreadable_root / "src" / "risk.py"
    if os.name == "nt":
        with _block_real_file_reads(unreadable):
            with pytest.raises(source_tree_error, match="read|permission|sharing|stable"):
                scan(unreadable_root, root_label="repository-root")
    else:
        original_open = module.os.open

        def deny_open(path: Any, *args: Any, **kwargs: Any) -> Any:
            if os.fspath(path).endswith("risk.py"):
                raise PermissionError("synthetic unreadable source file")
            return original_open(path, *args, **kwargs)

        monkeypatch.setattr(module.os, "open", deny_open)
        with pytest.raises(source_tree_error, match="read|permission|stable"):
            scan(unreadable_root, root_label="repository-root")
        monkeypatch.undo()

    reparse_root = tmp_path / "reparse"
    _write_source_fixture(reparse_root)
    if os.name == "nt":
        target = reparse_root / "outside-dir"
        target.mkdir()
        (target / "outside.py").write_bytes(b"outside")
        link = reparse_root / "src" / "linked-dir"
        created = subprocess.run(
            ["cmd", "/d", "/c", "mklink", "/J", str(link), str(target)],
            check=False,
            capture_output=True,
            text=True,
        )
        assert created.returncode == 0, created.stderr
        assert os.lstat(link).st_file_attributes & 0x400
    else:
        target = reparse_root / "outside.py"
        target.write_bytes(b"outside")
        link = reparse_root / "src" / "linked.py"
        os.symlink(target, link)
    with pytest.raises(source_tree_error, match="symlink|reparse|no-follow"):
        scan(reparse_root, root_label="repository-root")


def test_source_tree_root_junction_is_rejected_without_resolution(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    scan, source_tree_error, module = _source_tree_api()
    real_root = tmp_path / "real-root"
    _write_source_fixture(real_root)
    linked_root = tmp_path / "linked-root"
    if os.name == "nt":
        created = subprocess.run(
            ["cmd", "/d", "/c", "mklink", "/J", str(linked_root), str(real_root)],
            check=False,
            capture_output=True,
            text=True,
        )
        assert created.returncode == 0, created.stderr
        attributes = os.lstat(linked_root).st_file_attributes
        assert attributes & 0x400
    else:
        os.symlink(real_root, linked_root, target_is_directory=True)
        assert linked_root.is_symlink()

    class TargetResolutionAttempt(AssertionError):
        pass

    attempts: list[str] = []
    linked_prefix = os.path.normcase(os.path.abspath(linked_root))
    real_prefix = os.path.normcase(os.path.abspath(real_root))

    def is_guarded(path: Any) -> bool:
        normalized = os.path.normcase(os.path.abspath(os.fspath(path)))
        return any(
            normalized == prefix or normalized.startswith(prefix + os.sep) for prefix in (linked_prefix, real_prefix)
        )

    original_scandir = module.os.scandir
    original_listdir = module.os.listdir
    original_open = module.os.open
    original_stat = module.os.stat
    original_realpath = module.os.path.realpath
    original_readlink = module.os.readlink
    original_path_open = Path.open

    def guarded_scandir(path: Any) -> Any:
        if is_guarded(path):
            attempts.append("scandir")
            raise TargetResolutionAttempt("junction root was descended")
        return original_scandir(path)

    def guarded_listdir(path: Any) -> Any:
        if is_guarded(path):
            attempts.append("listdir")
            raise TargetResolutionAttempt("junction root was listed")
        return original_listdir(path)

    def guarded_open(path: Any, flags: int, *args: Any, **kwargs: Any) -> Any:
        no_follow_flag = getattr(module.os, "O_NOFOLLOW", 0)
        if is_guarded(path) and not (no_follow_flag and flags & no_follow_flag):
            attempts.append("os.open-follow")
            raise TargetResolutionAttempt("junction root was opened without no-follow")
        return original_open(path, flags, *args, **kwargs)

    def guarded_path_open(path: Path, *args: Any, **kwargs: Any) -> Any:
        if is_guarded(path):
            attempts.append("Path.open-follow")
            raise TargetResolutionAttempt("junction root was opened through Path.open")
        return original_path_open(path, *args, **kwargs)

    def guarded_stat(path: Any, *args: Any, **kwargs: Any) -> Any:
        if is_guarded(path) and kwargs.get("follow_symlinks", True):
            attempts.append("stat-follow")
            raise TargetResolutionAttempt("junction root was followed by stat")
        return original_stat(path, *args, **kwargs)

    def guarded_realpath(path: Any, *args: Any, **kwargs: Any) -> Any:
        if is_guarded(path):
            attempts.append("realpath")
            raise TargetResolutionAttempt("junction root was resolved")
        return original_realpath(path, *args, **kwargs)

    def guarded_readlink(path: Any, *args: Any, **kwargs: Any) -> Any:
        if is_guarded(path):
            attempts.append("readlink")
            raise TargetResolutionAttempt("junction target was read")
        return original_readlink(path, *args, **kwargs)

    monkeypatch.setattr(module.os, "scandir", guarded_scandir)
    monkeypatch.setattr(module.os, "listdir", guarded_listdir)
    monkeypatch.setattr(module.os, "open", guarded_open)
    monkeypatch.setattr(module.os, "stat", guarded_stat)
    monkeypatch.setattr(module.os.path, "realpath", guarded_realpath)
    monkeypatch.setattr(module.os, "readlink", guarded_readlink)
    monkeypatch.setattr(Path, "open", guarded_path_open)
    with pytest.raises(source_tree_error, match="root|symlink|junction|reparse|no-follow"):
        scan(linked_root, root_label="repository-root")
    assert attempts == [], f"scanner touched junction target before rejection: {attempts}"


def test_source_tree_rejects_directory_junction_swap_before_recursive_listing(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    scan, source_tree_error, module = _source_tree_api()
    root = tmp_path / "root"
    src = root / "src"
    src.mkdir(parents=True)
    outside = tmp_path / "outside-root"
    outside.mkdir()
    (outside / "evil.py").write_bytes(b"outside")

    original_scandir = module.os.scandir
    replaced = False
    src_path = os.path.normcase(os.path.abspath(src))

    def swap_directory_before_listing(path: Any) -> Any:
        nonlocal replaced
        if not replaced and os.path.normcase(os.path.abspath(os.fspath(path))) == src_path:
            src.rmdir()
            if os.name == "nt":
                created = subprocess.run(
                    ["cmd", "/d", "/c", "mklink", "/J", str(src), str(outside)],
                    check=False,
                    capture_output=True,
                    text=True,
                )
                assert created.returncode == 0, created.stderr
            else:
                os.symlink(outside, src, target_is_directory=True)
            replaced = True
        return original_scandir(path)

    monkeypatch.setattr(module.os, "scandir", swap_directory_before_listing)
    with pytest.raises(source_tree_error, match="changed|identity|junction|reparse|stable|no-follow|outside"):
        scan(root, root_label="repository-root")
    assert replaced


def test_source_tree_rejects_parent_junction_swap_before_file_open_with_same_file_identity(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    scan, source_tree_error, module = _source_tree_api()
    root = tmp_path / "root"
    src = root / "src"
    src.mkdir(parents=True)
    safe = src / "safe.py"
    safe.write_bytes(b"safe")
    outside = tmp_path / "outside-root"
    outside.mkdir()
    outside_safe = outside / "safe.py"
    os.link(safe, outside_safe)
    assert safe.stat().st_ino == outside_safe.stat().st_ino

    original_open = module.os.open
    replaced = False
    safe_path = os.path.normcase(os.path.abspath(safe))

    def swap_parent_before_file_open(path: Any, *args: Any, **kwargs: Any) -> Any:
        nonlocal replaced
        if not replaced and os.path.normcase(os.path.abspath(os.fspath(path))) == safe_path:
            safe.unlink()
            src.rmdir()
            if os.name == "nt":
                created = subprocess.run(
                    ["cmd", "/d", "/c", "mklink", "/J", str(src), str(outside)],
                    check=False,
                    capture_output=True,
                    text=True,
                )
                assert created.returncode == 0, created.stderr
            else:
                os.symlink(outside, src, target_is_directory=True)
            replaced = True
        return original_open(path, *args, **kwargs)

    monkeypatch.setattr(module.os, "open", swap_parent_before_file_open)
    with pytest.raises(source_tree_error, match="changed|identity|junction|reparse|stable|no-follow|outside"):
        scan(root, root_label="repository-root")
    assert replaced


def test_source_tree_rejects_late_add_in_previous_sibling_directory_before_return(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    scan, source_tree_error, module = _source_tree_api()
    root = tmp_path / "root"
    previous = root / "a"
    later = root / "z"
    previous.mkdir(parents=True)
    later.mkdir()
    (previous / "file.py").write_bytes(b"previous")
    later_file = later / "file.py"
    later_file.write_bytes(b"later")

    original_open = module.os.open
    mutated = False
    later_file_path = os.path.normcase(os.path.abspath(later_file))

    def add_previous_sibling_entry_before_later_open(path: Any, *args: Any, **kwargs: Any) -> Any:
        nonlocal mutated
        if not mutated and os.path.normcase(os.path.abspath(os.fspath(path))) == later_file_path:
            (previous / "late.py").write_bytes(b"late")
            mutated = True
        return original_open(path, *args, **kwargs)

    monkeypatch.setattr(module.os, "open", add_previous_sibling_entry_before_later_open)
    with pytest.raises(source_tree_error, match="changed|identity|extra|missing|stable"):
        scan(root, root_label="repository-root")
    assert mutated


def test_source_tree_rejects_late_delete_in_previous_sibling_directory_before_return(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    scan, source_tree_error, module = _source_tree_api()
    root = tmp_path / "root"
    previous = root / "a"
    later = root / "z"
    previous.mkdir(parents=True)
    later.mkdir()
    previous_file = previous / "file.py"
    previous_file.write_bytes(b"previous")
    later_file = later / "file.py"
    later_file.write_bytes(b"later")

    original_open = module.os.open
    mutated = False
    later_file_path = os.path.normcase(os.path.abspath(later_file))

    def delete_previous_sibling_entry_before_later_open(path: Any, *args: Any, **kwargs: Any) -> Any:
        nonlocal mutated
        if not mutated and os.path.normcase(os.path.abspath(os.fspath(path))) == later_file_path:
            previous_file.unlink()
            mutated = True
        return original_open(path, *args, **kwargs)

    monkeypatch.setattr(module.os, "open", delete_previous_sibling_entry_before_later_open)
    with pytest.raises(source_tree_error, match="changed|identity|extra|missing|stable"):
        scan(root, root_label="repository-root")
    assert mutated


def test_source_tree_hashes_file_content_incrementally_without_full_payload_digest(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    scan, _, module = _source_tree_api()
    root = tmp_path / "root"
    source = root / "src"
    source.mkdir(parents=True)
    payload = b"abcdefghij"
    (source / "large.py").write_bytes(payload)
    expected_digest = sha256_hex(payload)
    monkeypatch.setattr(module, "_READ_CHUNK_SIZE", 3)

    observed_update_lengths: list[int] = []
    if hasattr(module, "hashlib"):
        real_sha256 = hashlib.sha256

        class RecordingHash:
            def __init__(self) -> None:
                self._inner = real_sha256()

            def update(self, chunk: bytes) -> None:
                observed_update_lengths.append(len(chunk))
                self._inner.update(chunk)

            def hexdigest(self) -> str:
                return self._inner.hexdigest()

        monkeypatch.setattr(module.hashlib, "sha256", RecordingHash)
    else:

        def forbid_full_payload_digest(_payload: bytes) -> str:
            raise AssertionError("scanner must stream file digest instead of hashing one joined payload")

        monkeypatch.setattr(module, "sha256_hex", forbid_full_payload_digest)

    body = scan(root, root_label="repository-root")

    assert body["files"] == [
        {
            "relative_path": "src/large.py",
            "byte_length": str(len(payload)),
            "file_sha256": expected_digest,
        }
    ]
    if observed_update_lengths:
        assert observed_update_lengths == [3, 3, 3, 1]


def test_source_tree_replacement_race_is_rejected(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    scan, source_tree_error, module = _source_tree_api()
    _write_source_fixture(tmp_path)
    target = tmp_path / "src" / "kernel.py"
    original_open = module.os.open
    replaced = False

    def racing_open(path: Any, *args: Any, **kwargs: Any) -> Any:
        nonlocal replaced
        if not replaced and os.fspath(path).endswith("kernel.py"):
            replacement = target.with_suffix(".replacement")
            replacement.write_bytes(b"xyz")
            replacement.replace(target)
            replaced = True
        return original_open(path, *args, **kwargs)

    monkeypatch.setattr(module.os, "open", racing_open)
    with pytest.raises(source_tree_error, match="changed|identity|race|stable"):
        scan(tmp_path, root_label="repository-root")
    assert replaced


def test_source_tree_git_control_file_is_canonically_excluded(tmp_path: Path) -> None:
    scan, _, _ = _source_tree_api()
    directory_root = tmp_path / "directory-root"
    _write_source_fixture(directory_root)
    git_dir = directory_root / ".git"
    git_dir.mkdir()
    (git_dir / "config").write_bytes(b"[core]\nrepositoryformatversion = 0\n")
    baseline = scan(directory_root, root_label="repository-root")
    (git_dir / "config").write_bytes(b"[core]\nrepositoryformatversion = 1\n")
    changed = scan(directory_root, root_label="repository-root")
    assert canonical_json_bytes(changed) == canonical_json_bytes(baseline)
    assert all(not row["relative_path"].startswith(".git/") for row in changed["files"])

    worktree_root = tmp_path / "worktree-root"
    _write_source_fixture(worktree_root)
    control_file = worktree_root / ".git"
    control_file.write_bytes(b"gitdir: ../git-metadata/worktrees/synthetic\n")
    worktree_baseline = scan(worktree_root, root_label="repository-root")
    control_file.write_bytes(b"gitdir: ../git-metadata/worktrees/changed\n")
    worktree_changed = scan(worktree_root, root_label="repository-root")
    assert canonical_json_bytes(worktree_changed) == canonical_json_bytes(worktree_baseline)
    assert ".git" not in {row["relative_path"] for row in worktree_changed["files"]}


def test_benchmark_preflight_failure_receipt_is_total() -> None:
    vector = build_t02_vector()
    request = vector.attachments["trading.benchmark-request/v1"]
    receipt = vector.documents["trading.benchmark-receipt/v1"]
    assert request["benchmark_manifest_raw_sha256"] is None
    assert request["benchmark_manifest_raw_byte_length"] == "0"
    assert receipt["status"] == "INELIGIBLE"
    assert receipt["reason_codes"] == [
        "BENCHMARK_FIXTURE_NOT_ADMITTED",
        "BENCHMARK_PREREGISTRATION_MISSING",
    ]
    _assert_contract_valid(request, "total benchmark-request preflight attachment")
    _assert_contract_valid(receipt, "total ineligible benchmark receipt")

    retained_empty = deepcopy(request)
    retained_empty["benchmark_manifest_raw_sha256"] = sha256_hex(b"")
    retained_empty["benchmark_manifest_raw_byte_length"] = "0"
    _assert_contract_valid(
        retained_empty,
        "present empty benchmark bytes distinct from missing input",
    )
    invalid_pairs = []
    for digest_field, length_field in (
        ("benchmark_manifest_raw_sha256", "benchmark_manifest_raw_byte_length"),
        ("preregistered_thresholds_raw_sha256", "preregistered_thresholds_raw_byte_length"),
        ("hardware_profile_raw_sha256", "hardware_profile_raw_byte_length"),
    ):
        missing_with_length = deepcopy(request)
        missing_with_length[digest_field] = None
        missing_with_length[length_field] = "1"
        invalid_pairs.append(missing_with_length)
    for mutation in invalid_pairs:
        _assert_contract_invalid(mutation, "benchmark raw digest/length totality")


def test_benchmark_hardware_profile_is_closed() -> None:
    profile = {
        "schema": "trading.hardware-profile/v1",
        "os_name": "synthetic-os",
        "os_version": "synthetic-os-version",
        "architecture": "synthetic-architecture",
        "cpu_vendor": "synthetic-vendor",
        "cpu_model": "synthetic-model",
        "runtime_isolation": "offline-test-process",
        "logical_cpu_count": "8",
        "physical_memory_bytes": "17179869184",
        "timer_resolution_ns": "100",
        "virtualization": "UNKNOWN_DECLARED",
        "power_profile": "UNKNOWN_DECLARED",
        "timer_source": "PERF_COUNTER",
    }
    _assert_contract_valid(profile, "closed benchmark hardware-profile attachment")
    mutations: list[dict[str, Any]] = []
    missing = deepcopy(profile)
    missing.pop("timer_source")
    mutations.append(missing)
    unknown_enum = deepcopy(profile)
    unknown_enum["virtualization"] = "UNDECLARED"
    mutations.append(unknown_enum)
    zero_count = deepcopy(profile)
    zero_count["logical_cpu_count"] = "0"
    mutations.append(zero_count)
    extra = deepcopy(profile)
    extra["hostname"] = "forbidden-local-identity"
    mutations.append(extra)
    for mutation in mutations:
        _assert_contract_invalid(mutation, "closed hardware profile")


def test_t02_review_attachment_numeric_aliases_reject_out_of_range_decimal_strings() -> None:
    benchmark_manifest = {
        "schema": "trading.benchmark-manifest/v1",
        "benchmark_manifest_version": "OFFLINE_REPLAY_CORE_V1",
        "requested_metric_set": "OFFLINE_REPLAY_CORE_V1",
        "case_count": "1",
        "repetition_count": "1",
        "cases": [
            {
                "case_id": "synthetic-case",
                "fixture_manifest_sha256": _digest("fixture"),
                "config_admission_receipt_id": _digest("config"),
                "run_closure_receipt_id": _digest("closure"),
                "warmup_event_count": "0",
                "measured_event_count": "1",
                "warmup_group_count": "0",
                "measured_group_count": "1",
            }
        ],
    }
    hardware_profile = {
        "schema": "trading.hardware-profile/v1",
        "os_name": "synthetic-os",
        "os_version": "synthetic-os-version",
        "architecture": "synthetic-architecture",
        "cpu_vendor": "synthetic-vendor",
        "cpu_model": "synthetic-model",
        "runtime_isolation": "offline-test-process",
        "logical_cpu_count": "8",
        "physical_memory_bytes": "17179869184",
        "timer_resolution_ns": "100",
        "virtualization": "UNKNOWN_DECLARED",
        "power_profile": "UNKNOWN_DECLARED",
        "timer_source": "PERF_COUNTER",
    }
    for document in (benchmark_manifest, hardware_profile):
        _assert_contract_valid(document, "review numeric alias baseline")

    manifest_overflow = deepcopy(benchmark_manifest)
    manifest_overflow["case_count"] = str(MAX_U64 + 1)
    profile_overflow = deepcopy(hardware_profile)
    profile_overflow["logical_cpu_count"] = str(MAX_U64 + 1)
    for mutation in (manifest_overflow, profile_overflow):
        _assert_contract_invalid(mutation, "review attachment numeric alias bounds")


def test_t02_review_attachment_count_fields_bind_to_array_lengths() -> None:
    benchmark_manifest = {
        "schema": "trading.benchmark-manifest/v1",
        "benchmark_manifest_version": "OFFLINE_REPLAY_CORE_V1",
        "requested_metric_set": "OFFLINE_REPLAY_CORE_V1",
        "case_count": "1",
        "repetition_count": "1",
        "cases": [
            {
                "case_id": "synthetic-case",
                "fixture_manifest_sha256": _digest("fixture"),
                "config_admission_receipt_id": _digest("config"),
                "run_closure_receipt_id": _digest("closure"),
                "warmup_event_count": "0",
                "measured_event_count": "1",
                "warmup_group_count": "0",
                "measured_group_count": "1",
            }
        ],
    }
    benchmark_metrics = {
        "schema": "trading.benchmark-metrics/v1",
        "benchmark_manifest_sha256": _digest("manifest"),
        "hardware_profile_sha256": _digest("hardware"),
        "measurement_count": "1",
        "metrics": [
            {
                "case_id": "synthetic-case",
                "phase": "end_to_end",
                "unit": "EQUAL_TIME_GROUP",
                "sample_count": "1",
                "p50_ns": "1",
                "p95_ns": "1",
                "p99_ns": "1",
                "max_ns": "1",
            }
        ],
    }
    normalized_event_set = {
        "schema": "trading.normalized-event-set/v1",
        "fixture_manifest_sha256": _digest("fixture"),
        "raw_event_count": "1",
        "event_ids": [_digest("event")],
    }
    for document in (benchmark_manifest, benchmark_metrics, normalized_event_set):
        _assert_contract_valid(document, "review count/array baseline")

    manifest_wrong_count = deepcopy(benchmark_manifest)
    manifest_wrong_count["case_count"] = "999"
    metrics_wrong_count = deepcopy(benchmark_metrics)
    metrics_wrong_count["measurement_count"] = "999"
    normalized_wrong_count = deepcopy(normalized_event_set)
    normalized_wrong_count["raw_event_count"] = "999"
    for mutation in (manifest_wrong_count, metrics_wrong_count, normalized_wrong_count):
        _assert_contract_invalid(mutation, "review count field must match array length")


def test_supporting_contract_count_is_thirteen() -> None:
    from build_finance.crypto_replay.schema_definitions import (
        SUPPORTING_CONTRACT_SPECS,
        SUPPORTING_SCHEMA_IDS,
        SUPPORTING_SELF_ID_FIELDS,
    )

    assert len(SUPPORTING_SCHEMA_IDS) == 13
    assert len(SUPPORTING_CONTRACT_SPECS) == 13
    assert len(SUPPORTING_SELF_ID_FIELDS) == 13
    assert set(SUPPORTING_SCHEMA_IDS) == set(SUPPORTING_SELF_ID_FIELDS)


def test_count_totalizer_market_0_rejects() -> None:
    result = _capacity_from_counts(
        market_count=0,
        model_candidate_count=0,
        model_signal_mode="DISABLED",
    )
    assert result["admissible"] is False
    assert result["rejection_code"] == "MARKET_COUNT_RANGE"


def test_count_totalizer_market_1_accepts() -> None:
    result = _capacity_from_counts(
        market_count=1,
        model_candidate_count=0,
        model_signal_mode="DISABLED",
    )
    assert result["admissible"] is True
    assert result["market_count"] == "1"
    assert result["decision_attempt_upper_bound"] == "2"


def test_count_totalizer_market_max_accepts() -> None:
    result = _capacity_from_counts(
        market_count=MAX_U64,
        model_candidate_count=0,
        model_signal_mode="DISABLED",
        availability_group_count=1,
    )
    assert result["admissible"] is True
    assert result["market_count"] == str(MAX_U64)
    assert result["decision_attempt_upper_bound"] == str(MAX_U64)
    assert result["status"] == "EXCEEDED"
    assert result["first_exceeded_counter"] == "DECISION_SEQUENCE"


def test_count_totalizer_market_max_plus_one_rejects() -> None:
    result = _capacity_from_counts(
        market_count=MAX_U64 + 1,
        model_candidate_count=0,
        model_signal_mode="DISABLED",
    )
    assert result["admissible"] is False
    assert result["rejection_code"] == "MARKET_COUNT_RANGE"


def test_count_totalizer_candidate_0_obeys_disabled_mode() -> None:
    disabled = _capacity_from_counts(
        market_count=1,
        model_candidate_count=0,
        model_signal_mode="DISABLED",
    )
    cached = _capacity_from_counts(
        market_count=1,
        model_candidate_count=0,
        model_signal_mode="CACHED_FIXTURES",
    )
    assert disabled["admissible"] is True
    assert disabled["producer_sequence_next_upper_bound"] == "0"
    assert cached["admissible"] is False
    assert cached["rejection_code"] == "MODEL_CANDIDATE_COUNT_RANGE"


def test_count_totalizer_candidate_1_accepts_cached_mode() -> None:
    result = _capacity_from_counts(
        market_count=1,
        model_candidate_count=1,
        model_signal_mode="CACHED_FIXTURES",
    )
    assert result["admissible"] is True
    assert result["model_candidate_count"] == "1"
    assert result["producer_sequence_next_upper_bound"] == "1"


def test_count_totalizer_candidate_max_accepts() -> None:
    result = _capacity_from_counts(
        market_count=1,
        model_candidate_count=MAX_U64,
        model_signal_mode="CACHED_FIXTURES",
        availability_group_count=1,
    )
    assert result["admissible"] is True
    assert result["model_candidate_count"] == str(MAX_U64)
    assert result["producer_sequence_next_upper_bound"] == str(MAX_U64)
    assert result["status"] == "EXCEEDED"
    assert result["first_exceeded_counter"] == "LEDGER_SEQUENCE"


def test_count_totalizer_candidate_max_plus_one_rejects() -> None:
    result = _capacity_from_counts(
        market_count=1,
        model_candidate_count=MAX_U64 + 1,
        model_signal_mode="CACHED_FIXTURES",
    )
    assert result["admissible"] is False
    assert result["rejection_code"] == "MODEL_CANDIDATE_COUNT_RANGE"


def _structured_run_rejection(call: Callable[[], Any]) -> bool:
    from build_finance.crypto_replay.schema_model import ValidationIssue

    try:
        call()
    except ValueError as error:
        flattened: tuple[Any, ...]
        if len(error.args) == 1 and isinstance(error.args[0], tuple):
            flattened = error.args[0]
        else:
            flattened = error.args
        assert flattened and all(isinstance(issue, ValidationIssue) for issue in flattened)
        return True
    return False


def _round18_failed_closure_case(vector: Any) -> tuple[dict[str, Any], Any]:
    from tests.crypto_replay.test_t02_cross_artifact import _make_bundle

    third_payload = b"SYNTHETIC T02 FIXTURE PAYLOAD - NOT MARKET DATA - GROUP 3\n"
    fixture = deepcopy(vector.documents["trading.fixture-manifest/v1"])
    fixture["files"].append(
        {
            **deepcopy(fixture["files"][1]),
            "relative_path": "quotes/group-3.json",
            "raw_payload_sha256": sha256_hex(third_payload),
            "byte_length": str(len(third_payload)),
            "admission_sequence": "4",
            "availability_slot": "3",
        }
    )
    fixture["run_end_position_policy"] = "FORCE_CLOSE_NEXT_EVENT"
    fixture = _reseal(fixture)

    source_receipts = [
        _reseal({**receipt, "fixture_manifest_sha256": fixture["fixture_manifest_sha256"]})
        for receipt in vector.source_receipts
    ]
    third_source = _reseal(
        {
            **source_receipts[1],
            "fixture_manifest_sha256": fixture["fixture_manifest_sha256"],
            "raw_payload_sha256": sha256_hex(third_payload),
            "relative_path": "quotes/group-3.json",
            "byte_length": str(len(third_payload)),
            "admission_sequence": "4",
            "availability_slot": "3",
            "observed_at": "2026-01-02T00:00:03.000000000Z",
            "ingested_at": "2026-01-02T00:00:03.000000001Z",
        }
    )
    source_receipts.append(third_source)

    events = [
        _reseal(
            {
                **event,
                "fixture_manifest_sha256": fixture["fixture_manifest_sha256"],
                "source_admission_receipt_id": receipt["source_admission_receipt_id"],
            }
        )
        for event, receipt in zip(vector.raw_events, source_receipts[:2], strict=True)
    ]
    third_event = deepcopy(events[1])
    third_event.update(
        {
            "fixture_manifest_sha256": fixture["fixture_manifest_sha256"],
            "raw_payload_sha256": sha256_hex(third_payload),
            "source_admission_receipt_id": third_source["source_admission_receipt_id"],
            "event_time": "2026-01-01T00:00:03.000000000Z",
            "observed_at": "2026-01-02T00:00:03.000000000Z",
            "ingested_at": "2026-01-02T00:00:03.000000001Z",
            "admission_sequence": "4",
            "source_sequence": "3",
            "ingest_sequence": "3",
            "equal_time_group": "3",
            "replay_clock_ns": "2000000000",
        }
    )
    third_event["source_position"] = {
        **third_event["source_position"],
        "slot": "3",
        "source_native_event_id": "synthetic-quote-request-3",
    }
    third_event["revision"] = {
        **third_event["revision"],
        "availability_slot": "3",
        "availability_admission_sequence": "4",
    }
    third_event = _reseal(third_event)
    events.append(third_event)

    source_ids = sorted(receipt["source_admission_receipt_id"] for receipt in source_receipts)
    schedule = {
        "schema": "trading.availability-schedule/v1",
        "fixture_manifest_sha256": fixture["fixture_manifest_sha256"],
        "config_admission_receipt_id": vector.documents["trading.config-admission-receipt/v1"][
            "config_admission_receipt_id"
        ],
        "source_admission_receipt_ids": source_ids,
        "availability_groups": [
            {"availability_slot": "1", "equal_time_group": "1", "admission_cutoff": "2"},
            {"availability_slot": "2", "equal_time_group": "2", "admission_cutoff": "3"},
            {"availability_slot": "3", "equal_time_group": "3", "admission_cutoff": "4"},
        ],
    }
    normalized = {
        "schema": "trading.normalized-event-set/v1",
        "fixture_manifest_sha256": fixture["fixture_manifest_sha256"],
        "raw_event_count": "3",
        "event_ids": [event["event_id"] for event in events],
    }
    counter = {
        **vector.attachments["trading.counter-capacity/v1"],
        "normalized_event_set_sha256": sha256_hex(canonical_json_bytes(normalized)),
        "source_admission_count": "3",
        "raw_event_count": "3",
        "availability_group_count": "3",
        "decision_attempt_upper_bound": "3",
        "decision_sequence_next_upper_bound": "4",
        "intent_sequence_next_upper_bound": "4",
        "fill_receipt_sequence_next_upper_bound": "4",
        "unmatched_reservation_count_upper_bound": "6",
        "state_sequence_next_upper_bound": "18",
        "ledger_sequence_next_upper_bound": "60",
    }
    candidate_semantic = {
        "schema": "trading.run-closure-fill-candidate-semantic/v1",
        "event_kind": third_event["event_kind"],
        "market_id": third_event["market_id"],
        "base_mint": third_event["base_mint"],
        "quote_mint": third_event["quote_mint"],
        "base_decimals": third_event["base_decimals"],
        "quote_decimals": third_event["quote_decimals"],
        **third_event["market"],
        "executable": third_event["executable"],
        "quality_flags": third_event["quality_flags"],
    }
    proof_row_count = str(1_000_002)
    market_row = {
        **_force_market_row(
            q_cap_base_atoms="500001",
            proof_row_count=proof_row_count,
        ),
        "earliest_trigger_equal_time_group": "3",
        "fill_event_id": third_event["event_id"],
        "capacity_base_atoms": third_event["market"]["route_capacity_base_atoms"],
        "fill_candidate_semantic_sha256": sha256_hex(canonical_json_bytes(candidate_semantic)),
    }
    closure = _reseal(
        {
            **vector.documents["trading.run-closure-receipt/v1"],
            "status": "FAIL",
            "reason_codes": ["ADMISSION_RUN_END_PROOF_BUDGET", "ADMISSION_RUN_END_UNCLOSED"],
            "fixture_manifest_sha256": fixture["fixture_manifest_sha256"],
            "source_admission_receipt_ids": source_ids,
            "availability_schedule_sha256": sha256_hex(canonical_json_bytes(schedule)),
            "counter_capacity_sha256": sha256_hex(canonical_json_bytes(counter)),
            "run_end_position_policy": "FORCE_CLOSE_NEXT_EVENT",
            "terminal_equal_time_group": None,
            "proof_row_limit": str(MAX_PROOF_ROWS),
            "proof_row_count_total": proof_row_count,
            "proof_budget_status": "EXCEEDED",
            "market_proofs": [market_row],
        }
    )
    run = _reseal(
        {
            **vector.documents["trading.run-receipt/v1"],
            "fixture_manifest_sha256": fixture["fixture_manifest_sha256"],
            "source_admission_receipt_ids": source_ids,
            "availability_schedule_sha256": sha256_hex(canonical_json_bytes(schedule)),
            "run_closure_receipt_id": closure["run_closure_receipt_id"],
            "run_end_position_policy": "FORCE_CLOSE_NEXT_EVENT",
            "availability_groups": deepcopy(schedule["availability_groups"]),
        }
    )
    _assert_contract_valid(closure, "independently recomputed Round18 failed closure")
    return run, _make_bundle(
        vector,
        fixture_manifest=fixture,
        source_admission_receipts=tuple(source_receipts),
        normalized_events=tuple(events),
        run_closure_receipt=closure,
        availability_schedule=schedule,
        counter_capacity=counter,
    )


def _build_semantic_round18_failed_graph(vector: Any) -> dict[str, Any]:
    """Build a second, independently structured three-group force-close graph."""
    third_payload = b"SYNTHETIC SEMANTIC T02 GROUP 3 - NOT MARKET DATA\n"
    fixture_body = deepcopy(vector.documents["trading.fixture-manifest/v1"])
    fixture_body["run_end_position_policy"] = "FORCE_CLOSE_NEXT_EVENT"
    fixture_body["files"] = [*fixture_body["files"]]
    fixture_body["files"].append(
        {
            **deepcopy(fixture_body["files"][-1]),
            "relative_path": "quotes/group-3.json",
            "raw_payload_sha256": sha256_hex(third_payload),
            "byte_length": str(len(third_payload)),
            "admission_sequence": "4",
            "availability_slot": "3",
        }
    )
    fixture = _reseal(fixture_body)

    sources = [
        _reseal({**receipt, "fixture_manifest_sha256": fixture["fixture_manifest_sha256"]})
        for receipt in vector.source_receipts
    ]
    sources.append(
        _reseal(
            {
                **sources[-1],
                "raw_payload_sha256": sha256_hex(third_payload),
                "relative_path": "quotes/group-3.json",
                "byte_length": str(len(third_payload)),
                "admission_sequence": "4",
                "availability_slot": "3",
                "observed_at": "2026-01-02T00:00:03.000000000Z",
                "ingested_at": "2026-01-02T00:00:03.000000001Z",
            }
        )
    )

    events = [
        _reseal(
            {
                **event,
                "fixture_manifest_sha256": fixture["fixture_manifest_sha256"],
                "source_admission_receipt_id": source["source_admission_receipt_id"],
            }
        )
        for event, source in zip(vector.raw_events, sources[:2], strict=True)
    ]
    third_event_body = deepcopy(events[-1])
    third_event_body.update(
        {
            "raw_payload_sha256": sha256_hex(third_payload),
            "source_admission_receipt_id": sources[-1]["source_admission_receipt_id"],
            "event_time": "2026-01-01T00:00:03.000000000Z",
            "observed_at": "2026-01-02T00:00:03.000000000Z",
            "ingested_at": "2026-01-02T00:00:03.000000001Z",
            "admission_sequence": "4",
            "source_sequence": "3",
            "ingest_sequence": "3",
            "equal_time_group": "3",
            "replay_clock_ns": "2000000000",
        }
    )
    third_event_body["source_position"] = {
        **third_event_body["source_position"],
        "slot": "3",
        "source_native_event_id": "semantic-quote-request-3",
    }
    third_event_body["revision"] = {
        **third_event_body["revision"],
        "availability_slot": "3",
        "availability_admission_sequence": "4",
    }
    events.append(_reseal(third_event_body))

    config_receipt = vector.documents["trading.config-admission-receipt/v1"]
    source_ids = sorted(source["source_admission_receipt_id"] for source in sources)
    group_rows: list[dict[str, str]] = []
    for group_number, slot in enumerate((1, 2, 3), start=1):
        cutoff = max(
            int(config_receipt["admission_sequence"]),
            *(int(source["admission_sequence"]) for source in sources if int(source["availability_slot"]) <= slot),
        )
        group_rows.append(
            {
                "availability_slot": str(slot),
                "equal_time_group": str(group_number),
                "admission_cutoff": str(cutoff),
            }
        )
    schedule: dict[str, Any] = {
        "schema": "trading.availability-schedule/v1",
        "fixture_manifest_sha256": fixture["fixture_manifest_sha256"],
        "config_admission_receipt_id": config_receipt["config_admission_receipt_id"],
        "source_admission_receipt_ids": source_ids,
        "availability_groups": group_rows,
    }
    normalized: dict[str, Any] = {
        "schema": "trading.normalized-event-set/v1",
        "fixture_manifest_sha256": fixture["fixture_manifest_sha256"],
        "raw_event_count": str(len(events)),
        "event_ids": [event["event_id"] for event in events],
    }
    source_count = len(sources)
    raw_count = len(events)
    group_count = len(group_rows)
    market_count = len(fixture["allowed_markets"])
    decision_count = group_count * market_count
    counter: dict[str, Any] = {
        **deepcopy(vector.attachments["trading.counter-capacity/v1"]),
        "normalized_event_set_sha256": sha256_hex(canonical_json_bytes(normalized)),
        "source_admission_count": str(source_count),
        "raw_event_count": str(raw_count),
        "availability_group_count": str(group_count),
        "market_count": str(market_count),
        "decision_attempt_upper_bound": str(decision_count),
        "decision_sequence_next_upper_bound": str(decision_count + 1),
        "producer_sequence_next_upper_bound": "0",
        "intent_sequence_next_upper_bound": str(decision_count + 1),
        "fill_receipt_sequence_next_upper_bound": str(decision_count + 1),
        "unmatched_reservation_count_upper_bound": str(2 * decision_count),
        "state_sequence_next_upper_bound": str(3 * decision_count + 2 * group_count + 3),
        "ledger_sequence_next_upper_bound": str(source_count + raw_count + 10 * decision_count + 5 * group_count + 9),
    }

    third_event = events[-1]
    candidate_semantic: dict[str, Any] = {
        "schema": "trading.run-closure-fill-candidate-semantic/v1",
        "event_kind": third_event["event_kind"],
        "market_id": third_event["market_id"],
        "base_mint": third_event["base_mint"],
        "quote_mint": third_event["quote_mint"],
        "base_decimals": third_event["base_decimals"],
        "quote_decimals": third_event["quote_decimals"],
        **third_event["market"],
        "executable": third_event["executable"],
        "quality_flags": third_event["quality_flags"],
    }
    reference_set = {
        "schema": "trading.run-closure-reference-set/v1",
        "market_id": third_event["market_id"],
        "reference_prices_q18": ["1000000000000000000"],
    }
    risk_config = vector.documents["trading.replay-risk-config/v1"]

    def ceil_div(numerator: int, denominator: int) -> int:
        return (numerator + denominator - 1) // denominator

    target_notional = int(risk_config["target_entry_notional_quote_atoms"])
    reference_price = int(reference_set["reference_prices_q18"][0])
    base_decimals = int(third_event["base_decimals"])
    quote_decimals = int(third_event["quote_decimals"])
    q_cap = (target_notional * 10**18 * 10**base_decimals) // (reference_price * 10**quote_decimals)
    entry_basis = ceil_div(
        target_notional * (10_000 + int(risk_config["max_impact_bps"])),
        10_000,
    ) + ceil_div(target_notional * int(risk_config["max_fee_bps"]), 10_000)
    single_fill_gross = max(
        int(event["market"]["quote_amount_atoms"]) * q_cap // int(event["market"]["base_amount_atoms"])
        for event in events
    )
    single_fill_fee = max(
        ceil_div(
            int(event["market"]["venue_fee_quote_atoms"]) * q_cap,
            int(event["market"]["base_amount_atoms"]),
        )
        + ceil_div(
            int(event["market"]["priority_fee_quote_atoms"]) * q_cap,
            int(event["market"]["base_amount_atoms"]),
        )
        for event in events
    )
    market_value = (reference_price * q_cap * 10**quote_decimals) // (10**18 * 10**base_decimals)
    initial_quote = int(fixture["initial_quote_atoms"])
    pre_quote = initial_quote + decision_count * single_fill_gross
    post_quote = initial_quote + (decision_count + 1) * single_fill_gross
    pre_realized = decision_count * (single_fill_gross + entry_basis)
    post_realized = (decision_count + 1) * (single_fill_gross + entry_basis)
    unrealized = market_value + entry_basis
    pre_session = pre_realized + unrealized
    post_session = post_realized + unrealized
    pre_fees = decision_count * single_fill_fee
    post_fees = (decision_count + 1) * single_fill_fee
    pre_equity = pre_quote + market_value
    post_equity = post_quote + market_value
    ledger_posting = max(
        q_cap,
        single_fill_gross,
        single_fill_fee,
        entry_basis,
        single_fill_gross + entry_basis,
        single_fill_gross + entry_basis + 2 * unrealized,
    )
    state_envelope: dict[str, Any] = {
        "schema": "trading.force-close-state-envelope/v1",
        "market_id": third_event["market_id"],
        "terminal_horizon_equal_time_group": "3",
        "decision_attempt_upper_bound": str(decision_count),
        "position_quantity_base_atoms_max": str(q_cap),
        "position_cost_basis_quote_atoms_max": str(entry_basis),
        "all_position_cost_basis_quote_atoms_max": str(entry_basis),
        "single_fill_gross_quote_atoms_max": str(single_fill_gross),
        "single_fill_fee_quote_atoms_max": str(single_fill_fee),
        "all_market_value_quote_atoms_max": str(market_value),
        "pre_quote_total_atoms_max": str(pre_quote),
        "post_quote_total_atoms_max": str(post_quote),
        "pre_realized_pnl_abs_max": str(pre_realized),
        "post_realized_pnl_abs_max": str(post_realized),
        "pre_unrealized_pnl_abs_max": str(unrealized),
        "post_unrealized_pnl_abs_max": str(unrealized),
        "pre_session_pnl_abs_max": str(pre_session),
        "post_session_pnl_abs_max": str(post_session),
        "pre_cumulative_fees_quote_atoms_max": str(pre_fees),
        "post_cumulative_fees_quote_atoms_max": str(post_fees),
        "pre_equity_quote_atoms_max": str(pre_equity),
        "post_equity_quote_atoms_max": str(post_equity),
        "pre_peak_equity_quote_atoms_max": str(pre_equity),
        "post_peak_equity_quote_atoms_max": str(max(pre_equity, post_equity)),
        "ledger_posting_abs_max": str(ledger_posting),
    }
    proof_count = str(q_cap * len(reference_set["reference_prices_q18"]) * 2)
    market_row = {
        **_force_market_row(
            q_cap_base_atoms=str(q_cap),
            proof_row_count=proof_count,
        ),
        "earliest_trigger_equal_time_group": "3",
        "fill_event_id": third_event["event_id"],
        "capacity_base_atoms": str(
            int(third_event["market"]["route_capacity_base_atoms"])
            * int(risk_config["max_participation_bps"])
            // 10_000
        ),
        "fill_candidate_semantic_sha256": sha256_hex(canonical_json_bytes(candidate_semantic)),
        "reference_set_root_sha256": sha256_hex(canonical_json_bytes(reference_set)),
        "state_envelope_sha256": sha256_hex(canonical_json_bytes(state_envelope)),
    }
    schedule_digest = sha256_hex(canonical_json_bytes(schedule))
    counter_digest = sha256_hex(canonical_json_bytes(counter))
    closure = _reseal(
        {
            **deepcopy(vector.documents["trading.run-closure-receipt/v1"]),
            "status": "FAIL",
            "reason_codes": [
                "ADMISSION_RUN_END_PROOF_BUDGET",
                "ADMISSION_RUN_END_UNCLOSED",
            ],
            "fixture_manifest_sha256": fixture["fixture_manifest_sha256"],
            "source_admission_receipt_ids": source_ids,
            "availability_schedule_sha256": schedule_digest,
            "counter_capacity_sha256": counter_digest,
            "run_end_position_policy": "FORCE_CLOSE_NEXT_EVENT",
            "terminal_equal_time_group": None,
            "proof_row_limit": str(MAX_PROOF_ROWS),
            "proof_row_count_total": proof_count,
            "proof_budget_status": "EXCEEDED",
            "market_proofs": [market_row],
        }
    )
    run = _reseal(
        {
            **deepcopy(vector.documents["trading.run-receipt/v1"]),
            "fixture_manifest_sha256": fixture["fixture_manifest_sha256"],
            "source_admission_receipt_ids": source_ids,
            "availability_schedule_sha256": schedule_digest,
            "run_closure_receipt_id": closure["run_closure_receipt_id"],
            "run_end_position_policy": "FORCE_CLOSE_NEXT_EVENT",
            "terminal_equal_time_group": "3",
            "availability_groups": deepcopy(group_rows),
        }
    )
    return {
        "fixture": fixture,
        "sources": tuple(sources),
        "events": tuple(events),
        "normalized": normalized,
        "schedule": schedule,
        "counter": counter,
        "candidate_semantic": candidate_semantic,
        "reference_set": reference_set,
        "state_envelope": state_envelope,
        "closure": closure,
        "run": run,
    }


def _derive_semantic_round18_run_decisions(vector: Any) -> dict[str, bool]:
    """Independently construct the semantic run/schedule decision cases."""
    fixture = vector.documents["trading.fixture-manifest/v1"]
    config_receipt = vector.documents["trading.config-admission-receipt/v1"]
    source_receipts = tuple(sorted(vector.source_receipts, key=lambda receipt: int(receipt["admission_sequence"])))
    source_ids = sorted(receipt["source_admission_receipt_id"] for receipt in source_receipts)
    slots = sorted({int(row["availability_slot"]) for row in fixture["files"]})
    schedule_rows: list[dict[str, str]] = []
    for group_number, slot in enumerate(slots, start=1):
        cutoff = max(
            int(config_receipt["admission_sequence"]),
            *(
                int(receipt["admission_sequence"])
                for receipt in source_receipts
                if int(receipt["availability_slot"]) <= slot
            ),
        )
        schedule_rows.append(
            {
                "availability_slot": str(slot),
                "equal_time_group": str(group_number),
                "admission_cutoff": str(cutoff),
            }
        )
    schedule: dict[str, Any] = {
        "schema": "trading.availability-schedule/v1",
        "fixture_manifest_sha256": fixture["fixture_manifest_sha256"],
        "config_admission_receipt_id": config_receipt["config_admission_receipt_id"],
        "source_admission_receipt_ids": source_ids,
        "availability_groups": schedule_rows,
    }
    schedule_digest = sha256_hex(canonical_json_bytes(schedule))
    closure = _reseal(
        {
            **deepcopy(vector.documents["trading.run-closure-receipt/v1"]),
            "fixture_manifest_sha256": fixture["fixture_manifest_sha256"],
            "config_admission_receipt_id": config_receipt["config_admission_receipt_id"],
            "source_admission_receipt_ids": source_ids,
            "availability_schedule_sha256": schedule_digest,
            "run_end_position_policy": fixture["run_end_position_policy"],
            "terminal_equal_time_group": schedule_rows[-1]["equal_time_group"],
        }
    )
    run = _reseal(
        {
            **deepcopy(vector.documents["trading.run-receipt/v1"]),
            "fixture_manifest_sha256": fixture["fixture_manifest_sha256"],
            "config_admission_receipt_id": config_receipt["config_admission_receipt_id"],
            "source_admission_receipt_ids": source_ids,
            "availability_schedule_sha256": schedule_digest,
            "run_closure_receipt_id": closure["run_closure_receipt_id"],
            "run_end_position_policy": fixture["run_end_position_policy"],
            "terminal_equal_time_group": schedule_rows[-1]["equal_time_group"],
            "availability_groups": deepcopy(schedule_rows),
        }
    )

    cutoff_drift = deepcopy(schedule)
    cutoff_drift["availability_groups"][0]["admission_cutoff"] = "1"

    failed_graph = _build_semantic_round18_failed_graph(vector)
    failed_fixture = failed_graph["fixture"]
    failed_sources = failed_graph["sources"]
    failed_events = failed_graph["events"]
    failed_normalized = failed_graph["normalized"]
    failed_schedule = failed_graph["schedule"]
    failed_counter = failed_graph["counter"]
    failed_candidate = failed_graph["candidate_semantic"]
    failed_reference_set = failed_graph["reference_set"]
    failed_state_envelope = failed_graph["state_envelope"]
    failed_closure = failed_graph["closure"]
    failed_run = failed_graph["run"]
    failed_market_row = failed_closure["market_proofs"][0]
    failed_source_ids = sorted(source["source_admission_receipt_id"] for source in failed_sources)
    failed_file_rows = {row["relative_path"]: row for row in failed_fixture["files"]}
    failed_graph_is_closed = (
        failed_fixture["run_end_position_policy"] == "FORCE_CLOSE_NEXT_EVENT"
        and failed_closure["run_end_position_policy"] == failed_fixture["run_end_position_policy"]
        and failed_run["run_end_position_policy"] == failed_fixture["run_end_position_policy"]
        and all(
            source["fixture_manifest_sha256"] == failed_fixture["fixture_manifest_sha256"] for source in failed_sources
        )
        and all(
            event["fixture_manifest_sha256"] == failed_fixture["fixture_manifest_sha256"] for event in failed_events
        )
        and failed_normalized["fixture_manifest_sha256"]
        == failed_schedule["fixture_manifest_sha256"]
        == failed_closure["fixture_manifest_sha256"]
        == failed_run["fixture_manifest_sha256"]
        == failed_fixture["fixture_manifest_sha256"]
        and {event["source_admission_receipt_id"] for event in failed_events} == set(failed_source_ids)
        and all(
            failed_file_rows[source["relative_path"]]["raw_payload_sha256"] == source["raw_payload_sha256"]
            for source in failed_sources
        )
        and len(failed_fixture["files"])
        == len(failed_sources)
        == len(failed_events)
        == int(failed_normalized["raw_event_count"])
        == int(failed_counter["source_admission_count"])
        == int(failed_counter["raw_event_count"])
        == 3
        and int(failed_counter["availability_group_count"]) == len(failed_schedule["availability_groups"]) == 3
        and failed_normalized["event_ids"] == [event["event_id"] for event in failed_events]
        and failed_schedule["source_admission_receipt_ids"] == failed_source_ids
        and failed_closure["source_admission_receipt_ids"] == failed_source_ids
        and failed_run["source_admission_receipt_ids"] == failed_source_ids
        and failed_counter["normalized_event_set_sha256"] == sha256_hex(canonical_json_bytes(failed_normalized))
        and failed_closure["availability_schedule_sha256"]
        == failed_run["availability_schedule_sha256"]
        == sha256_hex(canonical_json_bytes(failed_schedule))
        and failed_closure["counter_capacity_sha256"] == sha256_hex(canonical_json_bytes(failed_counter))
        and failed_market_row["fill_candidate_semantic_sha256"] == sha256_hex(canonical_json_bytes(failed_candidate))
        and failed_market_row["reference_set_root_sha256"] == sha256_hex(canonical_json_bytes(failed_reference_set))
        and failed_market_row["state_envelope_sha256"] == sha256_hex(canonical_json_bytes(failed_state_envelope))
        and failed_state_envelope["market_id"] == failed_market_row["market_id"]
        and failed_state_envelope["terminal_horizon_equal_time_group"]
        == failed_market_row["earliest_trigger_equal_time_group"]
        and failed_state_envelope["decision_attempt_upper_bound"] == failed_counter["decision_attempt_upper_bound"]
        and failed_state_envelope["position_quantity_base_atoms_max"] == failed_market_row["q_cap_base_atoms"]
        and int(failed_market_row["proof_row_count"])
        == int(failed_market_row["q_cap_base_atoms"])
        * int(failed_market_row["reference_price_count"])
        * int(failed_market_row["adverse_fill_extreme_count"])
        == int(failed_closure["proof_row_count_total"])
        > MAX_PROOF_ROWS
    )
    policy_drift_run = _reseal({**deepcopy(run), "run_end_position_policy": "FORCE_CLOSE_NEXT_EVENT"})

    return {
        "run_receipts_accept": (
            closure["status"] in {"PASS", "NOT_REQUIRED_ZERO_AUTHORITY"}
            and run["run_closure_receipt_id"] == closure["run_closure_receipt_id"]
            and run["availability_schedule_sha256"] == schedule_digest
            and closure["availability_schedule_sha256"] == schedule_digest
            and run["run_end_position_policy"] == fixture["run_end_position_policy"]
        ),
        "run_cutoff_drift_reject": (
            sha256_hex(canonical_json_bytes(cutoff_drift)) != run["availability_schedule_sha256"]
        ),
        "run_failed_closure_reject": (
            failed_graph_is_closed
            and failed_closure["status"] == "FAIL"
            and failed_run["run_closure_receipt_id"] == failed_closure["run_closure_receipt_id"]
        ),
        "run_policy_drift_reject": (
            policy_drift_run["run_end_position_policy"] != closure["run_end_position_policy"]
            and policy_drift_run["run_end_position_policy"] != fixture["run_end_position_policy"]
        ),
    }


def test_round18_run_and_source_decision_vector_is_derived(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    from tests.crypto_replay.test_t02_cross_artifact import _make_bundle, _verify

    vector = build_t02_vector()
    valid_bundle = _make_bundle(vector)
    run = vector.documents["trading.run-receipt/v1"]

    changed_schedule = deepcopy(vector.attachments["trading.availability-schedule/v1"])
    changed_schedule["availability_groups"][0]["admission_cutoff"] = "1"
    cutoff_bundle = replace(valid_bundle, availability_schedule=changed_schedule)

    failed_run, failed_bundle = _round18_failed_closure_case(vector)

    drift_run = _reseal({**run, "run_end_position_policy": "FORCE_CLOSE_NEXT_EVENT"})
    drift_bundle = valid_bundle

    scan, source_tree_error, source_module = _source_tree_api()
    _write_source_fixture(tmp_path)
    original_open = source_module.os.open
    source_mutated = False

    def mutate_at_open(path: Any, *args: Any, **kwargs: Any) -> Any:
        nonlocal source_mutated
        if not source_mutated and os.fspath(path).endswith("risk.py"):
            (tmp_path / "src" / "risk.py").write_bytes(b"late mutation")
            source_mutated = True
        return original_open(path, *args, **kwargs)

    def source_rejects_late_mutation() -> bool:
        monkeypatch.setattr(source_module.os, "open", mutate_at_open)
        try:
            scan(tmp_path, root_label="repository-root")
        except source_tree_error:
            rejected = True
        else:
            rejected = False
        finally:
            monkeypatch.undo()
        return rejected and source_mutated

    mechanical = {
        "run_receipts_accept": _verify(vector, bundle=valid_bundle).authority == "CONTRACT_ONLY",
        "run_cutoff_drift_reject": _structured_run_rejection(lambda: _verify(vector, bundle=cutoff_bundle)),
        "run_failed_closure_reject": _structured_run_rejection(
            lambda: _verify(vector, run_receipt=failed_run, bundle=failed_bundle)
        ),
        "run_policy_drift_reject": _structured_run_rejection(
            lambda: _verify(vector, run_receipt=drift_run, bundle=drift_bundle)
        ),
        "source_late_mutation_reject": source_rejects_late_mutation(),
    }
    semantic_root = tmp_path / "semantic-source-root"
    _write_source_fixture(semantic_root)
    semantic_target = semantic_root / "src" / "risk.py"
    before_stat = semantic_target.stat()
    before_digest = sha256_hex(semantic_target.read_bytes())
    semantic_target.write_bytes(b"late mutation")
    after_stat = semantic_target.stat()
    after_digest = sha256_hex(semantic_target.read_bytes())
    semantic_source_reject = (before_stat.st_size, before_stat.st_mtime_ns, before_digest) != (
        after_stat.st_size,
        after_stat.st_mtime_ns,
        after_digest,
    )
    semantic = {
        **_derive_semantic_round18_run_decisions(vector),
        "source_late_mutation_reject": semantic_source_reject,
    }
    assert mechanical == semantic
    assert all(mechanical.values())


def _force_market_row(
    *,
    market_id: str = MARKET_ID,
    failure_codes: list[str] | None = None,
    q_cap_base_atoms: str = "1",
    proof_row_count: str | None = "2",
    proof_domain: str | None = None,
    proof_root_sha256: str | None = None,
) -> dict[str, Any]:
    return {
        "market_id": market_id,
        "failure_codes": [] if failure_codes is None else failure_codes,
        "earliest_trigger_equal_time_group": "3",
        "fill_event_id": _digest(f"fill-event-{market_id}"),
        "q_cap_base_atoms": q_cap_base_atoms,
        "capacity_base_atoms": q_cap_base_atoms,
        "fill_candidate_semantic_sha256": _digest(f"candidate-semantic-{market_id}"),
        "reference_price_count": "1",
        "adverse_fill_extreme_count": "2",
        "proof_row_count": proof_row_count,
        "state_envelope_sha256": _digest(f"state-envelope-{market_id}"),
        "proof_domain": proof_domain,
        "reference_set_root_sha256": _digest(f"reference-set-{market_id}"),
        "proof_root_sha256": proof_root_sha256,
    }


def _force_closure(
    *,
    status: str,
    reason_codes: list[str],
    proof_row_limit: str,
    proof_row_count_total: str | None,
    proof_budget_status: str,
    market_proofs: list[dict[str, Any]],
) -> dict[str, Any]:
    closure = build_t02_vector().documents["trading.run-closure-receipt/v1"]
    return _reseal(
        {
            **closure,
            "status": status,
            "reason_codes": reason_codes,
            "run_end_position_policy": "FORCE_CLOSE_NEXT_EVENT",
            "terminal_equal_time_group": "3" if status == "PASS" else None,
            "proof_row_limit": proof_row_limit,
            "proof_row_count_total": proof_row_count_total,
            "proof_budget_status": proof_budget_status,
            "market_proofs": market_proofs,
        }
    )


def _proof_row(
    *,
    reference_price_q18: str = "1000000000000000000",
    residual_base_atoms: str = "1",
    adverse_fill_bps: int = 0,
) -> dict[str, Any]:
    return {
        "schema": "trading.run-closure-full-fill-proof-row/v1",
        "residual_base_atoms": residual_base_atoms,
        "reference_price_q18": reference_price_q18,
        "capacity_atoms": residual_base_atoms,
        "filled_base_atoms": residual_base_atoms,
        "unfilled_base_atoms": "0",
        "gross_quote_atoms": residual_base_atoms,
        "venue_fee_quote_atoms": "0",
        "priority_fee_quote_atoms": "0",
        "simulation_fee_quote_atoms": "0",
        "cash_delta_quote_atoms": residual_base_atoms,
        "execution_price_q18": reference_price_q18,
        "participation_bps": 10000,
        "reference_deviation_bps": adverse_fill_bps,
        "impact_bps": adverse_fill_bps,
        "fee_bps": 0,
        "adverse_fill_bps": adverse_fill_bps,
        "status": "FILLED",
        "reason_codes": [],
    }


def _candidate_semantic() -> dict[str, Any]:
    return {
        "schema": "trading.run-closure-fill-candidate-semantic/v1",
        "event_kind": "ROUTE_QUOTE",
        "market_id": MARKET_ID,
        "base_mint": "synthetic-base",
        "quote_mint": "synthetic-quote",
        "base_decimals": 9,
        "quote_decimals": 6,
        "base_amount_atoms": "1",
        "quote_amount_atoms": "1",
        "route_capacity_base_atoms": "1",
        "liquidity_quote_atoms": "1",
        "venue_fee_quote_atoms": "0",
        "priority_fee_quote_atoms": "0",
        "route_impact_bps": 0,
        "executable": True,
        "quality_flags": [],
    }


def _state_envelope() -> dict[str, Any]:
    return {
        "schema": "trading.force-close-state-envelope/v1",
        "market_id": MARKET_ID,
        "terminal_horizon_equal_time_group": "3",
        "decision_attempt_upper_bound": "2",
        "position_quantity_base_atoms_max": "1",
        "position_cost_basis_quote_atoms_max": "1",
        "all_position_cost_basis_quote_atoms_max": "1",
        "single_fill_gross_quote_atoms_max": "1",
        "single_fill_fee_quote_atoms_max": "1",
        "all_market_value_quote_atoms_max": "1",
        "pre_quote_total_atoms_max": "1",
        "post_quote_total_atoms_max": "2",
        "pre_realized_pnl_abs_max": "1",
        "post_realized_pnl_abs_max": "2",
        "pre_unrealized_pnl_abs_max": "1",
        "post_unrealized_pnl_abs_max": "1",
        "pre_session_pnl_abs_max": "2",
        "post_session_pnl_abs_max": "3",
        "pre_cumulative_fees_quote_atoms_max": "1",
        "post_cumulative_fees_quote_atoms_max": "2",
        "pre_equity_quote_atoms_max": "2",
        "post_equity_quote_atoms_max": "3",
        "pre_peak_equity_quote_atoms_max": "2",
        "post_peak_equity_quote_atoms_max": "3",
        "ledger_posting_abs_max": "3",
    }


def test_t02_run_closure_budget_failure_market_row_schema_is_total() -> None:
    row = _force_market_row()
    closure = _force_closure(
        status="FAIL",
        reason_codes=["ADMISSION_RUN_END_PROOF_BUDGET", "ADMISSION_RUN_END_UNCLOSED"],
        proof_row_limit="1",
        proof_row_count_total="2",
        proof_budget_status="EXCEEDED",
        market_proofs=[row],
    )
    _assert_contract_valid(closure, "budget-failure all-market row layout")
    assert row["proof_domain"] is None and row["proof_root_sha256"] is None
    partial = deepcopy(closure)
    partial["market_proofs"][0].pop("state_envelope_sha256")
    _assert_contract_invalid(_reseal(partial), "budget failure forbids partial rows")


def test_t02_run_closure_preproof_failure_market_row_schema_is_total() -> None:
    row = {
        **_force_market_row(
            failure_codes=["CLOSURE_CAPACITY_INSUFFICIENT"],
            q_cap_base_atoms="2",
            proof_row_count="4",
            proof_domain=None,
            proof_root_sha256=None,
        ),
        "capacity_base_atoms": "1",
    }
    closure = _force_closure(
        status="FAIL",
        reason_codes=["ADMISSION_RUN_END_UNCLOSED"],
        proof_row_limit="4",
        proof_row_count_total="4",
        proof_budget_status="WITHIN_LIMIT",
        market_proofs=[row],
    )
    assert int(row["capacity_base_atoms"]) < int(row["q_cap_base_atoms"])
    assert int(row["proof_row_count"]) == (
        int(row["q_cap_base_atoms"]) * int(row["reference_price_count"]) * int(row["adverse_fill_extreme_count"])
    )
    assert closure["proof_row_count_total"] == row["proof_row_count"]
    _assert_contract_valid(closure, "pre-proof failure total market row")
    fabricated = deepcopy(closure)
    fabricated["market_proofs"][0]["proof_domain"] = "ALL_RESIDUAL_REFERENCE_PAIRS_AT_FILL_EXTREMES_V1"
    _assert_contract_invalid(_reseal(fabricated), "pre-proof failure forbids proof domain")


def test_t02_run_closure_enumeration_failure_preserves_other_market_row_shape() -> None:
    failed = _force_market_row(
        market_id="a-market",
        failure_codes=["CLOSURE_PROOF_PREDICATE_FAILED"],
        proof_domain="ALL_RESIDUAL_REFERENCE_PAIRS_AT_FILL_EXTREMES_V1",
        proof_root_sha256=None,
    )
    completed = _force_market_row(
        market_id="b-market",
        proof_domain="ALL_RESIDUAL_REFERENCE_PAIRS_AT_FILL_EXTREMES_V1",
        proof_root_sha256=_digest("b-market-proof-root"),
    )
    closure = _force_closure(
        status="FAIL",
        reason_codes=["ADMISSION_RUN_END_UNCLOSED"],
        proof_row_limit="4",
        proof_row_count_total="4",
        proof_budget_status="WITHIN_LIMIT",
        market_proofs=[failed, completed],
    )
    _assert_contract_valid(closure, "enumeration failure preserves completed market roots")
    assert closure["market_proofs"][1]["proof_root_sha256"] == _digest("b-market-proof-root")
    # Pure receipt semantics cannot infer the external FixtureManifest market
    # cardinality.  They do own each retained row's complete local shape,
    # nullability, uniqueness, and deterministic order; Task 9/11 binds the
    # array cardinality to the resolved fixture market set.
    one_complete_row = deepcopy(closure)
    one_complete_row["market_proofs"] = [failed]
    one_complete_row["proof_row_count_total"] = failed["proof_row_count"]
    _assert_contract_valid(_reseal(one_complete_row), "one complete enumeration-failure row")
    partial = deepcopy(closure)
    partial["market_proofs"][1].pop("state_envelope_sha256")
    _assert_contract_invalid(_reseal(partial), "preserved nonfailing row cannot be partial")
    missing_completed_root = deepcopy(closure)
    missing_completed_root["market_proofs"][1]["proof_root_sha256"] = None
    _assert_contract_invalid(_reseal(missing_completed_root), "nonfailing enumerated row requires its root")
    reversed_rows = deepcopy(closure)
    reversed_rows["market_proofs"].reverse()
    _assert_contract_invalid(_reseal(reversed_rows), "enumeration rows are market-sorted")


def test_t02_force_close_sell_alias_attachment_schema_is_closed() -> None:
    envelope = _state_envelope()
    _assert_contract_valid(envelope, "force-close state-envelope alias schema")
    overflow = deepcopy(envelope)
    overflow["post_realized_pnl_abs_max"] = str(2**127)
    negative = deepcopy(envelope)
    negative["post_quote_total_atoms_max"] = "-1"
    extra = deepcopy(envelope)
    extra["unbounded_intermediate"] = "1"
    for mutation in (overflow, negative, extra):
        _assert_contract_invalid(mutation, "force-close state aliases")


def test_t02_review_force_close_unsigned_aliases_reject_u64_overflow() -> None:
    envelope = _state_envelope()
    boundary = deepcopy(envelope)
    boundary["post_quote_total_atoms_max"] = str(MAX_U64)
    _assert_contract_valid(boundary, "force-close unsigned alias u64 boundary")
    overflow = deepcopy(envelope)
    overflow["post_quote_total_atoms_max"] = str(MAX_U64 + 1)
    _assert_contract_invalid(overflow, "force-close unsigned alias u64 overflow")


def test_t02_force_close_fill_extreme_attachment_schema_is_closed() -> None:
    candidate = _candidate_semantic()
    favorable = _proof_row(adverse_fill_bps=0)
    adverse = _proof_row(adverse_fill_bps=5000)
    for document in (candidate, favorable, adverse):
        _assert_contract_valid(document, "force-close candidate/fill-extreme attachment")
    forbidden_provenance = deepcopy(candidate)
    forbidden_provenance["event_id"] = _digest("forbidden-event-provenance")
    bad_alias = deepcopy(favorable)
    bad_alias["execution_price_q18"] = str(2**127)
    wrong_extreme = deepcopy(adverse)
    wrong_extreme["status"] = "PARTIAL"
    for mutation in (forbidden_provenance, bad_alias, wrong_extreme):
        _assert_contract_invalid(mutation, "force-close fill-extreme closure")


def test_t02_run_closure_and_run_receipt_bind_declared_lineage_fields() -> None:
    from dataclasses import replace

    from tests.crypto_replay.test_t02_cross_artifact import _assert_rejected, _make_bundle

    vector = build_t02_vector()
    closure = vector.documents["trading.run-closure-receipt/v1"]
    run = vector.documents["trading.run-receipt/v1"]
    fields = (
        "schema_bundle_sha256",
        "admission_code_sha256",
        "normalization_code_sha256",
        "availability_grouping_code_sha256",
        "run_closure_code_sha256",
        "risk_code_sha256",
        "feature_code_sha256",
        "fill_code_sha256",
        "accounting_code_sha256",
    )
    for field in fields:
        changed_closure = _reseal({**closure, field: _digest(f"changed-{field}")})
        coordinated_run = _reseal({**run, "run_closure_receipt_id": changed_closure["run_closure_receipt_id"]})
        changed_bundle = replace(_make_bundle(vector), run_closure_receipt=changed_closure)
        _assert_rejected(vector, run_receipt=coordinated_run, bundle=changed_bundle)


def test_t02_force_proof_row_attachment_schema_and_sort_order_are_closed() -> None:
    reference_set = {
        "schema": "trading.run-closure-reference-set/v1",
        "market_id": MARKET_ID,
        "reference_prices_q18": ["1000000000000000000", "2000000000000000000"],
    }
    rows: list[dict[str, Any]] = [
        _proof_row(reference_price_q18=reference, residual_base_atoms=residual, adverse_fill_bps=adverse)
        for reference in ("1000000000000000000", "2000000000000000000")
        for residual in ("1", "2")
        for adverse in (0, 5000)
    ]
    proof_set: dict[str, Any] = {
        "schema": "trading.run-closure-full-fill-proof-set/v1",
        "market_id": MARKET_ID,
        "earliest_trigger_equal_time_group": "3",
        "fill_candidate_semantic_sha256": _digest("candidate-semantic"),
        "q_cap_base_atoms": "2",
        "reference_set_root_sha256": sha256_hex(canonical_json_bytes(reference_set)),
        "adverse_fill_extremes_bps": [0, 5000],
        "proof_domain": "ALL_RESIDUAL_REFERENCE_PAIRS_AT_FILL_EXTREMES_V1",
        "proof_row_count": str(len(rows)),
        "rows": rows,
    }
    _assert_contract_valid(reference_set, "force-close reference-set attachment")
    _assert_contract_valid(proof_set, "force-proof row set and numeric sort order")
    wrong_order = deepcopy(proof_set)
    wrong_order["rows"].reverse()
    duplicate = deepcopy(proof_set)
    duplicate["rows"][1] = deepcopy(duplicate["rows"][0])
    wrong_count = deepcopy(proof_set)
    wrong_count["proof_row_count"] = str(len(rows) - 1)
    for mutation in (wrong_order, duplicate, wrong_count):
        _assert_contract_invalid(mutation, "force-proof row set closure")


def test_t02_review_force_proof_rejects_duplicate_semantic_row_keys() -> None:
    rows = [
        _proof_row(reference_price_q18="1000000000000000000", residual_base_atoms="1", adverse_fill_bps=0),
        {
            **_proof_row(reference_price_q18="1000000000000000000", residual_base_atoms="1", adverse_fill_bps=0),
            "gross_quote_atoms": "2",
        },
    ]
    proof_set = {
        "schema": "trading.run-closure-full-fill-proof-set/v1",
        "market_id": MARKET_ID,
        "earliest_trigger_equal_time_group": "3",
        "fill_candidate_semantic_sha256": _digest("candidate-semantic"),
        "q_cap_base_atoms": "2",
        "reference_set_root_sha256": _digest("reference-set"),
        "adverse_fill_extremes_bps": [0, 5000],
        "proof_domain": "ALL_RESIDUAL_REFERENCE_PAIRS_AT_FILL_EXTREMES_V1",
        "proof_row_count": str(len(rows)),
        "rows": rows,
    }
    assert "semantic_proof_row_duplicate" in _contract_issue_codes(proof_set)


def test_t02_review_force_proof_set_declares_and_enforces_protocol_cap() -> None:
    from build_finance.crypto_replay.schema_registry import get_schema_document

    proof_schema = get_schema_document("trading.run-closure-full-fill-proof-set/v1")
    assert proof_schema["properties"]["rows"]["maxItems"] == MAX_PROOF_ROWS

    proof_set = {
        "schema": "trading.run-closure-full-fill-proof-set/v1",
        "market_id": MARKET_ID,
        "earliest_trigger_equal_time_group": "3",
        "fill_candidate_semantic_sha256": _digest("candidate-semantic"),
        "q_cap_base_atoms": "2",
        "reference_set_root_sha256": _digest("reference-set"),
        "adverse_fill_extremes_bps": [0, 5000],
        "proof_domain": "ALL_RESIDUAL_REFERENCE_PAIRS_AT_FILL_EXTREMES_V1",
        "proof_row_count": str(MAX_PROOF_ROWS + 1),
        "rows": [_proof_row()],
    }
    assert "semantic_proof_row_cap" in _contract_issue_codes(proof_set)


def test_t02_force_proof_budget_fields_accept_boundary_shaped_vectors() -> None:
    row = _force_market_row(
        q_cap_base_atoms="500000",
        proof_row_count=str(MAX_PROOF_ROWS),
        proof_domain="ALL_RESIDUAL_REFERENCE_PAIRS_AT_FILL_EXTREMES_V1",
        proof_root_sha256=_digest("protocol-boundary-proof-root"),
    )
    closure = _force_closure(
        status="PASS",
        reason_codes=[],
        proof_row_limit=str(MAX_PROOF_ROWS),
        proof_row_count_total=str(MAX_PROOF_ROWS),
        proof_budget_status="WITHIN_LIMIT",
        market_proofs=[row],
    )
    _assert_contract_valid(closure, "inclusive force-proof configured/protocol boundary")
    assert closure["proof_row_count_total"] == closure["proof_row_limit"]


def _protocol_cap_out_of_range_case() -> None:
    baseline_row = _force_market_row(
        q_cap_base_atoms="500000",
        proof_row_count=str(MAX_PROOF_ROWS),
        proof_domain="ALL_RESIDUAL_REFERENCE_PAIRS_AT_FILL_EXTREMES_V1",
        proof_root_sha256=_digest("protocol-cap-equality-proof-root"),
    )
    baseline = _force_closure(
        status="PASS",
        reason_codes=[],
        proof_row_limit=str(MAX_PROOF_ROWS),
        proof_row_count_total=str(MAX_PROOF_ROWS),
        proof_budget_status="WITHIN_LIMIT",
        market_proofs=[baseline_row],
    )
    _assert_contract_valid(baseline, "protocol proof-cap fields before out-of-range mutations")
    row = _force_market_row(proof_row_count=str(MAX_U64 + 1))
    closure = _force_closure(
        status="FAIL",
        reason_codes=["ADMISSION_RUN_END_PROOF_BUDGET", "ADMISSION_RUN_END_UNCLOSED"],
        proof_row_limit=str(MAX_PROOF_ROWS + 1),
        proof_row_count_total=str(MAX_U64 + 1),
        proof_budget_status="EXCEEDED",
        market_proofs=[row],
    )
    _assert_contract_invalid(closure, "protocol/config proof-cap out-of-range serialization")


_HYPHENATED_PROTOCOL_TEST_NAME = "test_t02_force_proof_protocol_cap_fields_reject_out_of-range_serialization"
_protocol_cap_out_of_range_case.__name__ = _HYPHENATED_PROTOCOL_TEST_NAME
globals()[_HYPHENATED_PROTOCOL_TEST_NAME] = _protocol_cap_out_of_range_case
del _protocol_cap_out_of_range_case

_INCORRECT_PROTOCOL_TEST_NAMES = frozenset(
    {
        "test_t02_force_proof_protocol_cap_fields_reject_out-of-range_serialization",
        "test_t02_force_proof_protocol_cap_fields_reject_out_of_range_serialization",
    }
)
_FORBIDDEN_T11_TEST_NAMES = frozenset(
    {
        "test_run_closure_budget_failure_has_total_market_row_layout",
        "test_run_closure_preproof_failure_has_total_market_row_layout",
        "test_run_closure_enumeration_failure_completes_all_other_market_roots",
        "test_force_close_proves_state_dependent_sell_transition_aliases",
        "test_force_close_proves_favorable_and_max_adverse_fill_aliases",
        "test_run_closure_and_run_receipt_cross_bind_all_proof_lineage",
        "test_force_proof_row_schema_order_and_known_root",
        "test_force_proof_budget_exact_limit_passes",
        "test_force_proof_budget_one_over_fails_before_enumeration",
        "test_run_closure_proof_rows_at_protocol_cap",
        "test_run_closure_proof_rows_protocol_cap_plus_one_fails_before_hashing",
    }
)
_COLLECTABLE_TEST_NAMES = {name for name, value in globals().items() if name.startswith("test_") and callable(value)}
assert _HYPHENATED_PROTOCOL_TEST_NAME in _COLLECTABLE_TEST_NAMES
assert not _INCORRECT_PROTOCOL_TEST_NAMES & _COLLECTABLE_TEST_NAMES
assert not _FORBIDDEN_T11_TEST_NAMES & _COLLECTABLE_TEST_NAMES
