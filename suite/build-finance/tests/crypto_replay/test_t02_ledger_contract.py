"""T02 RED contracts for causal-digest, ledger-cause, and prefix formulas."""

from __future__ import annotations

import importlib
import json
from collections.abc import Mapping
from copy import deepcopy
from pathlib import Path
from typing import Any
from unittest.mock import patch

import pytest

from build_finance.crypto_replay.canonical import canonical_json_bytes, sha256_hex
from build_finance.crypto_replay.content_ids import seal_content_id
from tests.crypto_replay.test_t02_supporting_contracts import _digest, _reseal, build_t02_vector


def _ledger_contract(operation: str, **inputs: Any) -> Mapping[str, Any]:
    target = "build_finance.crypto_replay.ledger_contract"
    missing_message: str | None = None
    try:
        module = importlib.import_module(target)
    except ModuleNotFoundError as error:
        if error.name == target:
            missing_message = f"T02 RED - missing closed ledger-contract evaluator in {target}"
        else:
            raise
    if missing_message is not None:
        pytest.fail("T02 RED - missing closed ledger-contract evaluator", pytrace=False)
    evaluator = getattr(module, "_evaluate_t02_contract", None)
    if evaluator is None:
        pytest.fail(
            f"T02 RED - missing closed ledger-contract evaluator: {target}._evaluate_t02_contract",
            pytrace=False,
        )
    result = evaluator(operation=operation, **inputs)
    assert isinstance(result, Mapping)
    assert set(result) >= {"accepted", "issue_codes"}
    assert isinstance(result["accepted"], bool)
    assert isinstance(result["issue_codes"], tuple)
    return result


def test_causal_digest_kind_and_resolution_are_closed() -> None:
    vector = build_t02_vector()
    resolver = vector.resolver
    content_id = vector.documents["trading.fixture-manifest/v1"]["fixture_manifest_sha256"]
    retained_byte_digest = sha256_hex(vector.raw_payloads[0])
    typed_key = {
        "schema": "trading.group-mark-key/v1",
        "run_receipt_id": vector.documents["trading.run-receipt/v1"]["run_receipt_id"],
        "equal_time_group": "2",
        "as_of_ingest_sequence": "2",
        "portfolio_state_before_id": _digest("prefix-state"),
    }
    typed_key_digest = sha256_hex(canonical_json_bytes(typed_key))
    typed_preimages = {typed_key_digest: typed_key}
    for digest, expected_kind in (
        (content_id, "CONTENT_ID"),
        (retained_byte_digest, "RETAINED_BYTES"),
        (typed_key_digest, "TYPED_KEY"),
    ):
        result = _ledger_contract(
            "classify_causal_digest",
            digest=digest,
            resolver=resolver,
            typed_key_preimages=typed_preimages,
        )
        assert result == {"accepted": True, "issue_codes": (), "kind": expected_kind}

    wrong_preimage = {**typed_key, "equal_time_group": "3"}
    for mapped_preimages in (
        {typed_key_digest: wrong_preimage},
        {_digest("wrong-map-key"): typed_key},
    ):
        result = _ledger_contract(
            "classify_causal_digest",
            digest=typed_key_digest,
            resolver=resolver,
            typed_key_preimages=mapped_preimages,
        )
        assert result["accepted"] is False
        assert result["issue_codes"] == ("CAUSAL_DIGEST_PREIMAGE_MISMATCH",)
    for unknown in (_digest("unknown-cause"), "f" * 63, "G" * 64):
        result = _ledger_contract(
            "classify_causal_digest",
            digest=unknown,
            resolver=resolver,
            typed_key_preimages=typed_preimages,
        )
        assert result["accepted"] is False
        assert result["issue_codes"]

    class MisindexedResolver:
        def resolve_object(self, _digest_value: str) -> Any:
            return resolver.resolve_object(content_id)

        def resolve_bytes(self, _digest_value: str) -> None:
            return None

    misindexed = _ledger_contract(
        "classify_causal_digest",
        digest=_digest("misindexed-query"),
        resolver=MisindexedResolver(),
        typed_key_preimages=typed_preimages,
    )
    assert misindexed["accepted"] is False
    aliased = _ledger_contract(
        "classify_causal_digest",
        digest=typed_key_digest,
        resolver=resolver,
        typed_key_preimages={typed_key_digest: typed_key, _digest("typed-alias"): typed_key},
    )
    assert aliased["accepted"] is False
    assert aliased["issue_codes"] == ("CAUSAL_DIGEST_PREIMAGE_MISMATCH",)
    absolute_attempt = {
        "schema": "trading.model-validation-attempt-key/v1",
        "model_signal_manifest_sha256": _digest("absolute-manifest"),
        "relative_path": "/signals/absolute.json",
        "raw_signal_sha256": _digest("absolute-signal"),
        "feature_snapshot_id": _digest("absolute-snapshot"),
        "decision_sequence": "1",
        "requested_producer_scope_key_sha256": _digest("absolute-scope"),
        "horizon_ns": "1",
    }
    absolute_digest = sha256_hex(canonical_json_bytes(absolute_attempt))
    absolute = _ledger_contract(
        "classify_causal_digest",
        digest=absolute_digest,
        resolver=resolver,
        typed_key_preimages={absolute_digest: absolute_attempt},
    )
    assert absolute["accepted"] is False
    invalid_typed_keys = [
        {**absolute_attempt, "relative_path": "signals/\x00bad.json"},
        {
            "schema": "trading.risk-idempotency-key/v1",
            "run_receipt_id": vector.documents["trading.run-receipt/v1"]["run_receipt_id"],
            "equal_time_group": "1",
            "market_id": "x" * 129,
        },
        {
            "schema": "trading.execution-transition-key/v1",
            "run_receipt_id": vector.documents["trading.run-receipt/v1"]["run_receipt_id"],
            "equal_time_group": "1",
            "phase": [],
            "market_id": None,
            "item_sequence": None,
        },
        {
            "schema": "trading.reconciliation-arithmetic-range-key/v1",
            "run_receipt_id": vector.documents["trading.run-receipt/v1"]["run_receipt_id"],
            "portfolio_state_before_id": _digest("range-state"),
            "ingest_sequence": "1",
            "equal_time_group": "1",
            "replay_clock_ns": "0",
            "arithmetic_operation": [],
            "arithmetic_operands_sha256": _digest("range-operands"),
        },
    ]
    for invalid_key in invalid_typed_keys:
        invalid_digest = sha256_hex(canonical_json_bytes(invalid_key))
        result = _ledger_contract(
            "classify_causal_digest",
            digest=invalid_digest,
            resolver=resolver,
            typed_key_preimages={invalid_digest: invalid_key},
        )
        assert result["accepted"] is False

    class WrongResolution:
        def __init__(self, *, object_result: Any = None, byte_result: bytes | None = None) -> None:
            self.object_result = object_result
            self.byte_result = byte_result

        def resolve_object(self, _digest_value: str) -> Any:
            return self.object_result

        def resolve_bytes(self, _digest_value: str) -> bytes | None:
            return self.byte_result

    unrelated_object = resolver.resolve_object(content_id)
    for wrong_resolver in (
        WrongResolution(object_result=unrelated_object),
        WrongResolution(byte_result=b"wrong retained bytes"),
    ):
        result = _ledger_contract(
            "classify_causal_digest",
            digest=typed_key_digest,
            resolver=wrong_resolver,
            typed_key_preimages=typed_preimages,
        )
        assert result["accepted"] is False


def test_every_record_type_causation_set_is_exact() -> None:
    ids = {
        name: _digest(name)
        for name in (
            "fixture",
            "config",
            "closure",
            "source-1",
            "source-2",
            "registry",
            "manifest",
            "snapshot",
            "state-before",
            "validation",
            "signal",
            "risk",
            "intent",
            "fill-event",
            "fill",
            "state-after",
            "reconciliation",
            "group-mark-key",
            "raw-cause",
            "latching-risk",
            "validated-config",
        )
    }
    context = {
        "fixture_manifest_sha256": ids["fixture"],
        "config_admission_receipt_id": ids["config"],
        "run_closure_receipt_id": ids["closure"],
        "source_admission_receipt_ids": [ids["source-1"], ids["source-2"]],
        "validated_config_sha256": None,
        "model_registry_sha256": ids["registry"],
        "model_signal_manifest_sha256": ids["manifest"],
        "feature_snapshot_id": ids["snapshot"],
        "portfolio_state_before_id": ids["state-before"],
        "model_validation_receipt_id": ids["validation"],
        "model_signal_id": ids["signal"],
        "risk_decision_id": ids["risk"],
        "intent_id": ids["intent"],
        "fill_event_id": ids["fill-event"],
        "fill_receipt_id": ids["fill"],
        "previous_portfolio_state_id": ids["state-before"],
        "causation_id": ids["fill"],
        "portfolio_state_after_id": ids["state-after"],
        "reconciliation_causation_ids": [ids["fill"]],
        "final_reconciliation_receipt_id": ids["reconciliation"],
        "group_mark_key_sha256": ids["group-mark-key"],
        "causal_raw_event_ids": [ids["raw-cause"]],
        "enclosing_latching_risk_decision_id": None,
        "fill_reason_codes": [],
    }
    expected = {
        "SOURCE_ADMISSION": {ids["fixture"]},
        "CONFIG_ADMISSION": set(),
        "RUN_RECEIPT": {
            ids["fixture"],
            ids["config"],
            ids["closure"],
            ids["source-1"],
            ids["source-2"],
            ids["registry"],
            ids["manifest"],
        },
        "RAW_ADMISSION": {ids["source-1"]},
        "FEATURE_SNAPSHOT": {ids["raw-cause"]},
        "MODEL_VALIDATION": {ids["snapshot"], ids["registry"], ids["manifest"]},
        "MODEL_SIGNAL_ACCEPTED": {ids["validation"]},
        "RISK_DECISION": {ids["snapshot"], ids["state-before"], ids["config"], ids["validation"], ids["signal"]},
        "SIMULATED_INTENT": {ids["risk"], ids["state-before"]},
        "SIMULATED_FILL": {ids["intent"], ids["state-before"], ids["fill-event"]},
        "PORTFOLIO_STATE": {ids["state-before"], ids["fill"]},
        "KILL_STATE": {ids["state-before"], ids["fill"]},
        "RECONCILIATION": {ids["state-before"], ids["state-after"], ids["fill"]},
        "RUN_END": {ids["reconciliation"]},
    }
    object_schemas = {
        "SOURCE_ADMISSION": "trading.source-admission-receipt/v1",
        "CONFIG_ADMISSION": "trading.config-admission-receipt/v1",
        "RUN_RECEIPT": "trading.run-receipt/v1",
        "RAW_ADMISSION": "trading.raw-event/v1",
        "FEATURE_SNAPSHOT": "trading.feature-snapshot/v1",
        "MODEL_VALIDATION": "trading.model-validation-receipt/v1",
        "MODEL_SIGNAL_ACCEPTED": "trading.model-signal/v1",
        "RISK_DECISION": "trading.risk-decision/v1",
        "SIMULATED_INTENT": "trading.simulated-order-intent/v1",
        "SIMULATED_FILL": "trading.simulated-fill-receipt/v1",
        "PORTFOLIO_STATE": "trading.portfolio-state/v1",
        "KILL_STATE": "trading.portfolio-state/v1",
        "RECONCILIATION": "trading.reconciliation-receipt/v1",
        "RUN_END": "trading.portfolio-state/v1",
    }
    context["source_admission_receipt_id"] = ids["source-1"]
    for record_type, causes in expected.items():
        result = _ledger_contract("derive_record_contract", record_type=record_type, context=context)
        assert result == {
            "accepted": True,
            "issue_codes": (),
            "object_schema": object_schemas[record_type],
            "causation_ids": tuple(sorted(causes)),
        }

    valid_context = {
        **context,
        "validated_config_sha256": ids["validated-config"],
    }
    for record_type in ("CONFIG_ADMISSION", "RUN_RECEIPT", "RISK_DECISION"):
        result = _ledger_contract(
            "derive_record_contract",
            record_type=record_type,
            context=valid_context,
        )
        assert result == {
            "accepted": True,
            "issue_codes": (),
            "object_schema": object_schemas[record_type],
            "causation_ids": tuple(sorted({*expected[record_type], ids["validated-config"]})),
        }

    latching_context = {
        **context,
        "fill_reason_codes": ["FILL_KILL_LATCHED"],
        "enclosing_latching_risk_decision_id": ids["latching-risk"],
    }
    latching = _ledger_contract(
        "derive_record_contract",
        record_type="SIMULATED_FILL",
        context=latching_context,
    )
    assert latching == {
        "accepted": True,
        "issue_codes": (),
        "object_schema": object_schemas["SIMULATED_FILL"],
        "causation_ids": tuple(sorted({*expected["SIMULATED_FILL"], ids["latching-risk"]})),
    }
    missing_latching_risk = _ledger_contract(
        "derive_record_contract",
        record_type="SIMULATED_FILL",
        context={
            **context,
            "fill_reason_codes": ["FILL_KILL_LATCHED"],
            "enclosing_latching_risk_decision_id": None,
        },
    )
    assert missing_latching_risk["accepted"] is False
    assert missing_latching_risk["issue_codes"] == ("LEDGER_LATCHING_RISK_CAUSE_REQUIRED",)
    forbidden_extra = _ledger_contract(
        "derive_record_contract",
        record_type="SIMULATED_FILL",
        context={
            **context,
            "enclosing_latching_risk_decision_id": ids["latching-risk"],
        },
    )
    assert forbidden_extra["accepted"] is False
    assert forbidden_extra["issue_codes"] == ("LEDGER_LATCHING_RISK_CAUSE_FORBIDDEN",)
    for unknown in ("BENCHMARK_RECEIPT", "SESSION_BOUNDARY", "UNDECLARED"):
        result = _ledger_contract("derive_record_contract", record_type=unknown, context=context)
        assert result["accepted"] is False
        assert result["issue_codes"] == ("LEDGER_RECORD_TYPE_UNKNOWN",)
    malformed_feature = _ledger_contract(
        "derive_record_contract",
        record_type="FEATURE_SNAPSHOT",
        context={**context, "causal_raw_event_ids": [[]]},
    )
    malformed_fill = _ledger_contract(
        "derive_record_contract",
        record_type="SIMULATED_FILL",
        context={**context, "fill_reason_codes": [[]]},
    )
    assert malformed_feature["accepted"] is False
    assert malformed_fill["accepted"] is False
    same_state_reconciliation = _ledger_contract(
        "derive_record_contract",
        record_type="RECONCILIATION",
        context={
            **context,
            "portfolio_state_after_id": ids["state-before"],
        },
    )
    assert same_state_reconciliation == {
        "accepted": True,
        "issue_codes": (),
        "object_schema": object_schemas["RECONCILIATION"],
        "causation_ids": tuple(sorted({ids["state-before"], ids["fill"]})),
    }


def test_initial_ledger_prefix_order_is_exact() -> None:
    vector = build_t02_vector()
    source_rows = sorted(
        (
            {
                "record_type": "SOURCE_ADMISSION",
                "object_id": receipt["source_admission_receipt_id"],
                "admission_sequence": receipt["admission_sequence"],
            }
            for receipt in vector.source_receipts
        ),
        key=lambda row: int(row["admission_sequence"]),
    )
    config = vector.documents["trading.config-admission-receipt/v1"]
    rows = [
        source_rows[0],
        {
            "record_type": "CONFIG_ADMISSION",
            "object_id": config["config_admission_receipt_id"],
            "admission_sequence": config["admission_sequence"],
        },
        source_rows[1],
        {
            "record_type": "RUN_RECEIPT",
            "object_id": vector.documents["trading.run-receipt/v1"]["run_receipt_id"],
            "admission_sequence": None,
        },
        {"record_type": "PORTFOLIO_STATE", "object_id": _digest("genesis-state"), "admission_sequence": None},
        {
            "record_type": "RECONCILIATION",
            "object_id": _digest("genesis-reconciliation"),
            "admission_sequence": None,
        },
    ]
    result = _ledger_contract("validate_initial_prefix", rows=rows)
    assert result == {"accepted": True, "issue_codes": ()}
    variants: list[list[dict[str, Any]]] = []
    missing = deepcopy(rows)
    missing.pop(0)
    variants.append(missing)
    reordered = deepcopy(rows)
    reordered[0], reordered[1] = reordered[1], reordered[0]
    variants.append(reordered)
    duplicate = deepcopy(rows)
    duplicate.insert(1, deepcopy(duplicate[0]))
    variants.append(duplicate)
    wrong_suffix = deepcopy(rows)
    wrong_suffix[-2], wrong_suffix[-1] = wrong_suffix[-1], wrong_suffix[-2]
    variants.append(wrong_suffix)
    for variant in variants:
        result = _ledger_contract("validate_initial_prefix", rows=variant)
        assert result["accepted"] is False
        assert result["issue_codes"]

    oversized_sequence = deepcopy(rows)
    oversized_sequence[0]["admission_sequence"] = "9" * 100_000
    with patch(
        "build_finance.crypto_replay.ledger_contract.parse_bounded_decimal_string",
        side_effect=AssertionError("oversized u64 reached integer parsing"),
    ):
        result = _ledger_contract("validate_initial_prefix", rows=oversized_sequence)
    assert result == {
        "accepted": False,
        "issue_codes": ("LEDGER_INITIAL_PREFIX_SEQUENCE",),
    }


def test_fill_ledger_record_cannot_self_cause() -> None:
    """T01 already rejects object/record/prior-head self-causation."""
    from build_finance.crypto_replay.schema_registry import validate_contract

    example_path = Path(__file__).parent / "resources" / "primary-v1" / "examples" / "ledger-record.json"
    document = json.loads(example_path.read_text(encoding="utf-8"))
    assert document["record_type"] == "SIMULATED_FILL"
    assert not validate_contract(document, expected_schema="trading.ledger-record/v1")
    mutation = deepcopy(document)
    mutation["causation_ids"] = sorted({*mutation["causation_ids"], mutation["object_id"]})
    mutation.pop("ledger_record_id")
    mutation = seal_content_id(mutation)
    issues = validate_contract(mutation, expected_schema="trading.ledger-record/v1")
    assert any(issue.code == "semantic_self_cause" and issue.path == ("causation_ids",) for issue in issues)
    changed = _reseal({**document, "causation_ids": sorted({*document["causation_ids"], document["object_id"]})})
    assert changed["ledger_record_id"] != document["ledger_record_id"]
