"""T01 RED contract for the eight closed primary v1 schemas."""

from __future__ import annotations

import json
from collections.abc import Callable, Mapping, Sequence
from copy import deepcopy
from inspect import signature
from pathlib import Path
from typing import Any

import pytest

EXAMPLE_ROOT = Path(__file__).parent / "resources" / "primary-v1" / "examples"
EXAMPLES = {
    "trading.feature-snapshot/v1": ("feature-snapshot.json", "snapshot_id"),
    "trading.ledger-record/v1": ("ledger-record.json", "ledger_record_id"),
    "trading.model-signal/v1": ("model-signal.json", "signal_id"),
    "trading.portfolio-state/v1": ("portfolio-state.json", "portfolio_state_id"),
    "trading.raw-event/v1": ("raw-event.json", "event_id"),
    "trading.risk-decision/v1": ("risk-decision.json", "risk_decision_id"),
    "trading.simulated-fill-receipt/v1": ("simulated-fill-receipt.json", "fill_receipt_id"),
    "trading.simulated-order-intent/v1": ("simulated-order-intent.json", "intent_id"),
}

SUPPORTING_SELF_ID_FIELDS = {
    "trading.fixture-manifest/v1": "fixture_manifest_sha256",
    "trading.replay-risk-config/v1": "config_sha256",
    "trading.source-admission-receipt/v1": "source_admission_receipt_id",
    "trading.config-admission-receipt/v1": "config_admission_receipt_id",
    "trading.run-closure-receipt/v1": "run_closure_receipt_id",
    "trading.execution-quarantine-receipt/v1": "execution_quarantine_receipt_id",
    "trading.model-registry/v1": "model_registry_sha256",
    "trading.model-signal-manifest/v1": "model_signal_manifest_sha256",
    "trading.model-validation-receipt/v1": "model_validation_receipt_id",
    "trading.reconciliation-receipt/v1": "reconciliation_receipt_id",
    "trading.run-receipt/v1": "run_receipt_id",
    "trading.benchmark-measurement/v1": "benchmark_measurement_id",
    "trading.benchmark-receipt/v1": "benchmark_receipt_id",
}

ATTACHMENT_SCHEMA_IDS = (
    "trading.adverse-fill-draw-key/v1",
    "trading.availability-schedule/v1",
    "trading.benchmark-manifest/v1",
    "trading.benchmark-metrics/v1",
    "trading.benchmark-request/v1",
    "trading.counter-capacity/v1",
    "trading.execution-footprint-component/v1",
    "trading.execution-transition-footprint/v1",
    "trading.execution-transition-key/v1",
    "trading.fill-idempotency-key/v1",
    "trading.force-close-state-envelope/v1",
    "trading.group-mark-key/v1",
    "trading.hardware-profile/v1",
    "trading.model-attempt-footprint/v1",
    "trading.model-request-key/v1",
    "trading.model-validation-attempt-key/v1",
    "trading.normalized-event-set/v1",
    "trading.preregistered-thresholds/v1",
    "trading.reconciliation-arithmetic-operands/v1",
    "trading.reconciliation-arithmetic-range-key/v1",
    "trading.reservation-key/v1",
    "trading.risk-idempotency-key/v1",
    "trading.run-closure-fill-candidate-semantic/v1",
    "trading.run-closure-full-fill-proof-row/v1",
    "trading.run-closure-full-fill-proof-set/v1",
    "trading.run-closure-reference-set/v1",
    "trading.source-tree/v1",
)


def _example(schema_id: str) -> dict[str, Any]:
    filename, _ = EXAMPLES[schema_id]
    document = json.loads((EXAMPLE_ROOT / filename).read_text(encoding="utf-8"))
    assert document["schema"] == schema_id
    return document


def _changed(document: Mapping[str, Any], path: Sequence[str | int], value: Any) -> dict[str, Any]:
    result = deepcopy(document)
    target: Any = result
    for part in path[:-1]:
        target = target[part]
    target[path[-1]] = value
    return result


def _different(value: Any) -> Any:
    if value is None:
        return "mutation"
    if isinstance(value, bool):
        return not value
    if isinstance(value, int):
        return value + 1
    if isinstance(value, str):
        return value + "~"
    if isinstance(value, list):
        return [*value, "mutation"]
    if isinstance(value, dict):
        return {**value, "mutation": True}
    raise AssertionError(f"unhandled JSON value: {type(value)!r}")


def _assert_semantic_mutations_rejected(
    document: Mapping[str, Any],
    mutations: Sequence[tuple[Sequence[str | int], Any]],
    *,
    seal_content_id: Callable[[Mapping[str, Any]], dict[str, Any]],
    validate_contract: Callable[..., tuple[Any, ...]],
) -> None:
    _, self_id_field = EXAMPLES[str(document["schema"])]
    for path, value in mutations:
        mutation = _changed(document, path, value)
        mutation.pop(self_id_field)
        sealed = seal_content_id(mutation)
        issues = validate_contract(sealed, expected_schema=document["schema"])
        dotted_path = ".".join(str(part) for part in path)
        assert issues, f"semantic mutation unexpectedly accepted: {dotted_path}"


def test_unknown_property_rejected_for_all_primary_v1_schemas() -> None:
    from build_finance.crypto_replay.content_ids import seal_content_id
    from build_finance.crypto_replay.schema_registry import validate_contract

    for schema_id, (_, self_id_field) in EXAMPLES.items():
        mutation = _example(schema_id)
        mutation["unknown_property"] = True
        mutation.pop(self_id_field)
        mutation = seal_content_id(mutation)
        assert validate_contract(mutation, expected_schema=schema_id)


def test_primary_id_digest_excludes_only_self_id() -> None:
    from build_finance.crypto_replay.content_ids import compute_content_id

    schema_ids = tuple(EXAMPLES)
    for index, (schema_id, (_, self_id_field)) in enumerate(EXAMPLES.items()):
        document = _example(schema_id)
        expected = document[self_id_field]
        assert compute_content_id(document) == expected

        without_self_id = dict(document)
        without_self_id.pop(self_id_field)
        assert compute_content_id(without_self_id) == expected

        changed_self_id = dict(document)
        changed_self_id[self_id_field] = "0" * 64
        assert compute_content_id(changed_self_id) == expected

        for field, value in document.items():
            if field == self_id_field:
                continue
            mutation = dict(document)
            mutation[field] = schema_ids[(index + 1) % len(schema_ids)] if field == "schema" else _different(value)
            assert compute_content_id(mutation) != expected, f"digest ignored {schema_id}.{field}"

        changed_schema = dict(document)
        changed_schema["schema"] = "trading.unsupported/v1"
        with pytest.raises((KeyError, ValueError)):
            compute_content_id(changed_schema)


def test_content_id_registry_and_sealing_are_fail_closed() -> None:
    from build_finance.crypto_replay.content_ids import compute_content_id, seal_content_id, verify_content_id
    from build_finance.crypto_replay.schema_definitions import (
        PRIMARY_SELF_ID_FIELDS,
    )
    from build_finance.crypto_replay.schema_definitions import (
        SUPPORTING_SELF_ID_FIELDS as REGISTERED_SUPPORTING_SELF_ID_FIELDS,
    )

    assert dict(PRIMARY_SELF_ID_FIELDS) == {schema_id: self_id for schema_id, (_, self_id) in EXAMPLES.items()}
    assert dict(REGISTERED_SUPPORTING_SELF_ID_FIELDS) == SUPPORTING_SELF_ID_FIELDS
    for function in (compute_content_id, seal_content_id, verify_content_id):
        assert tuple(signature(function).parameters) == ("document",)

    for schema_id, (_, self_id_field) in EXAMPLES.items():
        document = _example(schema_id)
        without_self_id = dict(document)
        without_self_id.pop(self_id_field)

        sealed = seal_content_id(without_self_id)
        assert sealed is not without_self_id
        assert self_id_field not in without_self_id
        assert sealed == document
        assert verify_content_id(sealed)
        assert not verify_content_id(without_self_id)

        mismatch = dict(document)
        mismatch[self_id_field] = "0" * 64
        with pytest.raises(ValueError):
            seal_content_id(mismatch)

    for schema_id, self_id_field in SUPPORTING_SELF_ID_FIELDS.items():
        document = {"schema": schema_id, "payload": "synthetic-contract-vector"}
        sealed = seal_content_id(document)
        assert self_id_field in sealed
        assert verify_content_id(sealed)
        assert compute_content_id({**document, "payload": "mutated"}) != sealed[self_id_field]

    with pytest.raises(ValueError):
        compute_content_id({"schema": ATTACHMENT_SCHEMA_IDS[0]})


def test_primary_examples_validate_and_self_ids_recompute() -> None:
    from build_finance.crypto_replay.content_ids import compute_content_id, verify_content_id
    from build_finance.crypto_replay.schema_registry import require_valid_contract

    for schema_id, (_, self_id_field) in EXAMPLES.items():
        document = _example(schema_id)
        require_valid_contract(document, expected_schema=schema_id)
        assert compute_content_id(document) == document[self_id_field]
        assert verify_content_id(document)


def test_raw_event_example_validates() -> None:
    from build_finance.crypto_replay.schema_registry import require_valid_contract

    require_valid_contract(_example("trading.raw-event/v1"), expected_schema="trading.raw-event/v1")


def test_feature_snapshot_example_validates() -> None:
    from build_finance.crypto_replay.schema_registry import require_valid_contract

    require_valid_contract(_example("trading.feature-snapshot/v1"), expected_schema="trading.feature-snapshot/v1")


def test_model_signal_example_validates() -> None:
    from build_finance.crypto_replay.schema_registry import require_valid_contract

    require_valid_contract(_example("trading.model-signal/v1"), expected_schema="trading.model-signal/v1")


def test_risk_decision_example_validates() -> None:
    from build_finance.crypto_replay.schema_registry import require_valid_contract

    require_valid_contract(_example("trading.risk-decision/v1"), expected_schema="trading.risk-decision/v1")


def test_simulated_order_intent_example_validates() -> None:
    from build_finance.crypto_replay.schema_registry import require_valid_contract

    require_valid_contract(
        _example("trading.simulated-order-intent/v1"),
        expected_schema="trading.simulated-order-intent/v1",
    )


def test_simulated_fill_receipt_example_validates() -> None:
    from build_finance.crypto_replay.schema_registry import require_valid_contract

    require_valid_contract(
        _example("trading.simulated-fill-receipt/v1"),
        expected_schema="trading.simulated-fill-receipt/v1",
    )


def test_portfolio_state_example_validates() -> None:
    from build_finance.crypto_replay.schema_registry import require_valid_contract

    require_valid_contract(_example("trading.portfolio-state/v1"), expected_schema="trading.portfolio-state/v1")


def test_ledger_record_example_validates() -> None:
    from build_finance.crypto_replay.schema_registry import require_valid_contract

    require_valid_contract(_example("trading.ledger-record/v1"), expected_schema="trading.ledger-record/v1")


def _schema_nodes(node: Any) -> list[Mapping[str, Any]]:
    if not isinstance(node, Mapping):
        return []
    result = [node]
    for keyword in ("$defs", "properties"):
        children = node.get(keyword)
        if isinstance(children, Mapping):
            for child in children.values():
                result.extend(_schema_nodes(child))
    items = node.get("items")
    if isinstance(items, Mapping):
        result.extend(_schema_nodes(items))
    for keyword in ("anyOf", "oneOf", "allOf"):
        branches = node.get(keyword)
        if isinstance(branches, list):
            for branch in branches:
                result.extend(_schema_nodes(branch))
    negated = node.get("not")
    if isinstance(negated, Mapping):
        result.extend(_schema_nodes(negated))
    return result


def _object_paths(value: Any, path: tuple[str | int, ...] = ()) -> list[tuple[str | int, ...]]:
    paths: list[tuple[str | int, ...]] = []
    if isinstance(value, dict):
        paths.append(path)
        for key, child in value.items():
            paths.extend(_object_paths(child, (*path, key)))
    elif isinstance(value, list):
        for index, child in enumerate(value):
            paths.extend(_object_paths(child, (*path, index)))
    return paths


def test_primary_schema_documents_are_exact_draft_valid_and_recursively_closed() -> None:
    from jsonschema import Draft202012Validator

    from build_finance.crypto_replay.schema_definitions import PRIMARY_SCHEMA_DOCUMENTS, PRIMARY_SCHEMA_IDS

    assert tuple(PRIMARY_SCHEMA_DOCUMENTS) == tuple(PRIMARY_SCHEMA_IDS)
    assert len(PRIMARY_SCHEMA_DOCUMENTS) == 8
    for schema_id, schema in PRIMARY_SCHEMA_DOCUMENTS.items():
        Draft202012Validator.check_schema(schema)
        assert schema["$schema"] == "https://json-schema.org/draft/2020-12/schema"
        assert schema["x-contract-schema"] == schema_id
        contract_name, version = schema_id.removeprefix("trading.").split("/", maxsplit=1)
        assert schema["$id"] == f"urn:build-finance:contract:{contract_name}:{version}"
        for node in _schema_nodes(schema):
            if node.get("type") == "object":
                assert node.get("additionalProperties") is False
                assert set(node.get("required", ())) == set(node.get("properties", ()))


def test_primary_shared_scalar_definitions_are_frozen() -> None:
    from build_finance.crypto_replay.schema_definitions import PRIMARY_SCHEMA_DOCUMENTS

    expected = {
        "sha256": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
        "ContentID": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
        "CausalDigest": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
        "u64s": {"type": "string", "pattern": "^(0|[1-9][0-9]*)$"},
        "uints": {"type": "string", "pattern": "^(0|[1-9][0-9]*)$"},
        "i128s": {"type": "string", "pattern": "^(0|-?[1-9][0-9]*)$"},
        "sq18s": {"type": "string", "pattern": "^(0|-?[1-9][0-9]*)$"},
        "uq18s": {"type": "string", "pattern": "^(0|[1-9][0-9]*)$"},
        "rfc3339_ns_utc": {
            "type": "string",
            "pattern": r"^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}\.[0-9]{9}Z$",
        },
        "rfc3339_full_date": {"type": "string", "pattern": r"^[0-9]{4}-[0-9]{2}-[0-9]{2}$"},
        "bounded_utf8_registry_string": {"type": "string", "minLength": 1, "maxLength": 128},
    }
    definitions = [schema["$defs"] for schema in PRIMARY_SCHEMA_DOCUMENTS.values()]
    assert all(definition == definitions[0] for definition in definitions)
    for name, value in expected.items():
        assert definitions[0][name] == value
    assert "safe_relative_path" in definitions[0]


def test_primary_local_and_independent_oracles_agree_on_basic_cases() -> None:
    from jsonschema import Draft202012Validator

    from build_finance.crypto_replay.schema_definitions import PRIMARY_SCHEMA_DOCUMENTS
    from build_finance.crypto_replay.schema_registry import SchemaRegistry

    local = SchemaRegistry.from_documents(PRIMARY_SCHEMA_DOCUMENTS.values())
    for schema_id, (_, self_id_field) in EXAMPLES.items():
        schema = PRIMARY_SCHEMA_DOCUMENTS[schema_id]
        oracle = Draft202012Validator(schema)
        positive = _example(schema_id)
        unknown = {**positive, "unknown_property": True}
        missing = dict(positive)
        missing.pop(self_id_field)
        wrong_type = {**positive, self_id_field: 0}
        vectors = (positive, unknown, missing, wrong_type)
        local_results = [not local.validate(document, expected_schema=schema_id) for document in vectors]
        oracle_results = [not tuple(oracle.iter_errors(document)) for document in vectors]
        assert local_results == oracle_results == [True, False, False, False]


def test_unknown_property_rejected_recursively_for_all_primary_schemas() -> None:
    from jsonschema import Draft202012Validator

    from build_finance.crypto_replay.schema_definitions import PRIMARY_SCHEMA_DOCUMENTS
    from build_finance.crypto_replay.schema_registry import SchemaRegistry

    local = SchemaRegistry.from_documents(PRIMARY_SCHEMA_DOCUMENTS.values())
    for schema_id, (_, self_id_field) in EXAMPLES.items():
        positive = _example(schema_id)
        oracle = Draft202012Validator(PRIMARY_SCHEMA_DOCUMENTS[schema_id])
        for path in _object_paths(positive):
            mutation = deepcopy(positive)
            target: Any = mutation
            for part in path:
                target = target[part]
            target["unknown_property"] = True
            assert local.validate(mutation, expected_schema=schema_id), (schema_id, path)
            assert tuple(oracle.iter_errors(mutation)), (schema_id, path)
        assert self_id_field in positive


def test_primary_semantic_dispatch_has_exactly_eight_validators() -> None:
    from build_finance.crypto_replay.contract_semantics import SEMANTIC_VALIDATORS

    assert set(SEMANTIC_VALIDATORS) == set(EXAMPLES)
    assert len(SEMANTIC_VALIDATORS) == 8
    assert {schema_id: validator.__name__ for schema_id, validator in SEMANTIC_VALIDATORS.items()} == {
        "trading.feature-snapshot/v1": "validate_feature_snapshot_semantics",
        "trading.ledger-record/v1": "validate_ledger_record_semantics",
        "trading.model-signal/v1": "validate_model_signal_semantics",
        "trading.portfolio-state/v1": "validate_portfolio_state_semantics",
        "trading.raw-event/v1": "validate_raw_event_semantics",
        "trading.risk-decision/v1": "validate_risk_decision_semantics",
        "trading.simulated-fill-receipt/v1": "validate_simulated_fill_receipt_semantics",
        "trading.simulated-order-intent/v1": "validate_simulated_order_intent_semantics",
    }


def test_raw_event_semantic_matrix() -> None:
    from build_finance.crypto_replay.content_ids import seal_content_id
    from build_finance.crypto_replay.schema_registry import validate_contract

    document = _example("trading.raw-event/v1")
    _assert_semantic_mutations_rejected(
        document,
        [
            (("base_mint",), document["quote_mint"]),
            (("revision", "availability_slot"), "2"),
            (("revision", "availability_admission_sequence"), "2"),
            (("revision", "kind"), "CORRECTION"),
            (("quality_flags",), ["NON_EXECUTABLE"]),
            (("ingested_at",), "2025-01-01T00:00:00.000000000Z"),
            (("event_time",), "2027-01-01T00:00:00.000000000Z"),
            (("event_time",), None),
            (("market", "route_capacity_base_atoms"), "0"),
            (("quality_flags",), ["STALE_SOURCE", "NON_EXECUTABLE"]),
        ],
        seal_content_id=seal_content_id,
        validate_contract=validate_contract,
    )


def test_feature_snapshot_semantic_matrix() -> None:
    from build_finance.crypto_replay.content_ids import seal_content_id
    from build_finance.crypto_replay.schema_registry import validate_contract

    document = _example("trading.feature-snapshot/v1")
    _assert_semantic_mutations_rejected(
        document,
        [
            (("features", "history_count"), 0),
            (("missing_features",), ["mid_price_q18"]),
            (("as_of_event_id",), None),
            (("input_merkle_root_sha256",), None),
            (("features", "mid_price_q18"), None),
            (("features", "rsi_14_q18"), "100000000000000000001"),
        ],
        seal_content_id=seal_content_id,
        validate_contract=validate_contract,
    )


def test_model_signal_semantic_matrix() -> None:
    from build_finance.crypto_replay.content_ids import seal_content_id
    from build_finance.crypto_replay.schema_registry import validate_contract

    document = _example("trading.model-signal/v1")
    _assert_semantic_mutations_rejected(
        document,
        [
            (("probability_long_bias_q18",), "1"),
            (("decision_close_replay_clock_ns",), "0"),
            (("expires_replay_clock_ns",), "4"),
            (("action",), "LONG_BIAS"),
            (("score_q18",), "1000000000000000001"),
        ],
        seal_content_id=seal_content_id,
        validate_contract=validate_contract,
    )


def test_risk_decision_semantic_matrix() -> None:
    from build_finance.crypto_replay.content_ids import seal_content_id
    from build_finance.crypto_replay.schema_registry import validate_contract

    document = _example("trading.risk-decision/v1")
    _assert_semantic_mutations_rejected(
        document,
        [
            (("model_signal_status",), "ACCEPTED"),
            (("approved_base_atoms",), "999"),
            (("reservation_id",), None),
            (("reason_codes",), ["RISK_DRAWDOWN", "RISK_SESSION_LOSS"]),
            (("validated_config_sha256",), None),
        ],
        seal_content_id=seal_content_id,
        validate_contract=validate_contract,
    )


def test_simulated_order_intent_semantic_matrix() -> None:
    from build_finance.crypto_replay.content_ids import seal_content_id
    from build_finance.crypto_replay.schema_registry import validate_contract

    document = _example("trading.simulated-order-intent/v1")
    _assert_semantic_mutations_rejected(
        document,
        [
            (("quantity_base_atoms",), "0"),
            (("reserved_quote_atoms",), "0"),
            (("stop_price_q18",), "1200000000000000000"),
            (("action",), "CLOSE_LONG"),
        ],
        seal_content_id=seal_content_id,
        validate_contract=validate_contract,
    )


def test_simulated_fill_receipt_semantic_matrix() -> None:
    from build_finance.crypto_replay.content_ids import seal_content_id
    from build_finance.crypto_replay.schema_registry import validate_contract

    document = _example("trading.simulated-fill-receipt/v1")
    _assert_semantic_mutations_rejected(
        document,
        [
            (("filled_base_atoms",), "999"),
            (("fill_equal_time_group",), "1"),
            (("cash_delta_quote_atoms",), "0"),
            (("reason_codes",), ["FILL_PARTIAL"]),
            (("fill_event_id",), None),
        ],
        seal_content_id=seal_content_id,
        validate_contract=validate_contract,
    )


def test_portfolio_state_semantic_matrix() -> None:
    from build_finance.crypto_replay.content_ids import seal_content_id
    from build_finance.crypto_replay.schema_registry import validate_contract

    document = _example("trading.portfolio-state/v1")
    duplicate_balance = [document["balances"][0], document["balances"][0]]
    _assert_semantic_mutations_rejected(
        document,
        [
            (("balances", 0, "total_atoms"), "0"),
            (("balances",), duplicate_balance),
            (("balances",), list(reversed(document["balances"]))),
            (("kill_latched",), True),
            (("summary", "session_pnl_quote_atoms"), "0"),
            (("summary", "equity_quote_atoms"), "0"),
        ],
        seal_content_id=seal_content_id,
        validate_contract=validate_contract,
    )


def test_ledger_record_semantic_matrix() -> None:
    from build_finance.crypto_replay.content_ids import seal_content_id
    from build_finance.crypto_replay.schema_registry import validate_contract

    document = _example("trading.ledger-record/v1")
    _assert_semantic_mutations_rejected(
        document,
        [
            (("object_schema",), "trading.raw-event/v1"),
            (("object_sha256",), "0" * 64),
            (("entries",), []),
            (("previous_ledger_record_id",), None),
            (("causation_ids",), list(reversed(document["causation_ids"]))),
            (("causation_ids",), [document["object_id"]]),
            (("entries",), list(reversed(document["entries"]))),
        ],
        seal_content_id=seal_content_id,
        validate_contract=validate_contract,
    )


def _structural_schema_documents() -> tuple[dict[str, Any], dict[str, Any]]:
    draft = "https://json-schema.org/draft/2020-12/schema"
    leaf_schema = "trading.test-leaf/v1"
    leaf = {
        "$schema": draft,
        "$id": "urn:build-finance:contract:test-leaf:v1",
        "$defs": {
            "short_code": {
                "type": "string",
                "pattern": "^[A-Z]+$",
                "minLength": 1,
                "maxLength": 2,
            }
        },
        "x-contract-schema": leaf_schema,
        "type": "object",
        "required": ["schema", "kind", "mode", "count", "label", "tags", "code"],
        "properties": {
            "schema": {"const": leaf_schema},
            "kind": {"const": "leaf"},
            "mode": {"enum": ["A", "B"]},
            "count": {"type": "integer", "minimum": 1, "maximum": 3},
            "label": {"type": "string", "pattern": "^[a-z]+$", "minLength": 1, "maxLength": 4},
            "tags": {
                "type": "array",
                "items": {"type": "string"},
                "minItems": 1,
                "maxItems": 2,
                "uniqueItems": True,
            },
            "code": {"$ref": "#/$defs/short_code"},
        },
        "additionalProperties": False,
    }

    root_schema = "trading.test-root/v1"
    root = {
        "$schema": draft,
        "$id": "urn:build-finance:contract:test-root:v1",
        "x-contract-schema": root_schema,
        "type": "object",
        "required": ["schema", "leaf", "selector", "exclusive", "combined", "allowed", "nullable"],
        "properties": {
            "schema": {"const": root_schema},
            "leaf": {"$ref": leaf["$id"]},
            "selector": {"anyOf": [{"const": "x"}, {"const": "y"}]},
            "exclusive": {"oneOf": [{"const": "left"}, {"const": "right"}]},
            "combined": {
                "allOf": [
                    {"type": "string"},
                    {"minLength": 2},
                    {"maxLength": 3},
                ]
            },
            "allowed": {"not": {"const": "forbidden"}},
            "nullable": {"type": ["string", "null"]},
        },
        "additionalProperties": False,
    }
    return leaf, root


def _structural_vectors() -> tuple[tuple[str, dict[str, Any], tuple[dict[str, Any], ...]], ...]:
    leaf = {
        "schema": "trading.test-leaf/v1",
        "kind": "leaf",
        "mode": "A",
        "count": 2,
        "label": "good",
        "tags": ["one", "two"],
        "code": "OK",
    }
    leaf_negatives = (
        _changed(leaf, ("count",), True),
        _changed(leaf, ("count",), 0),
        _changed(leaf, ("count",), 4),
        _changed(leaf, ("mode",), "C"),
        _changed(leaf, ("label",), ""),
        _changed(leaf, ("label",), "TOO-LONG"),
        _changed(leaf, ("tags",), []),
        _changed(leaf, ("tags",), ["one", "two", "three"]),
        _changed(leaf, ("tags",), ["same", "same"]),
        _changed(leaf, ("code",), "bad"),
        {key: value for key, value in leaf.items() if key != "kind"},
        {**leaf, "extra": True},
    )

    root = {
        "schema": "trading.test-root/v1",
        "leaf": leaf,
        "selector": "x",
        "exclusive": "left",
        "combined": "abc",
        "allowed": "permitted",
        "nullable": None,
    }
    root_negatives = (
        _changed(root, ("leaf", "count"), True),
        _changed(root, ("selector",), "z"),
        _changed(root, ("exclusive",), "neither"),
        _changed(root, ("combined",), "a"),
        _changed(root, ("combined",), "abcd"),
        _changed(root, ("allowed",), "forbidden"),
        _changed(root, ("nullable",), 1),
        {**root, "extra": True},
    )
    return (
        ("trading.test-leaf/v1", leaf, leaf_negatives),
        ("trading.test-root/v1", root, root_negatives),
    )


def test_registry_rejects_unsupported_keywords_and_remote_refs() -> None:
    from build_finance.crypto_replay.schema_registry import SchemaRegistry

    base = {
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "urn:build-finance:contract:test-contract:v1",
        "x-contract-schema": "trading.test-contract/v1",
        "type": "object",
        "required": [],
        "properties": {},
        "additionalProperties": False,
    }
    metadata_only = {
        key: value
        for key, value in base.items()
        if key not in {"type", "required", "properties", "additionalProperties"}
    }
    rejected = (
        {**base, "format": "custom"},
        {**base, "x-unregistered-metadata": True},
        {**base, "additionalProperties": True},
        {
            **base,
            "required": [],
            "properties": {"declared_but_optional": {"type": "string"}},
        },
        {**base, "$ref": "https://example.invalid/remote.schema.json"},
        {**base, "$ref": "urn:build-finance:contract:not-preloaded:v1"},
        {**base, "$ref": "#/$defs/not-present"},
        {**base, "properties": {"nested": {"format": "custom"}}},
        {**metadata_only, "properties": {}},
        {**metadata_only, "required": []},
        {**metadata_only, "additionalProperties": False},
    )

    for schema in rejected:
        with pytest.raises(ValueError):
            SchemaRegistry.from_documents([schema])

    SchemaRegistry.from_documents([{**base, "type": ["object", "null"]}])


def test_registry_preserves_explicit_contract_specs() -> None:
    from build_finance.crypto_replay.schema_model import ContractSpec
    from build_finance.crypto_replay.schema_registry import SchemaRegistry

    schemas = _structural_schema_documents()
    specs = tuple(
        ContractSpec(
            schema_id=schema["x-contract-schema"],
            self_id_field=None,
            family="ATTACHMENT",
            schema_filename=f"test-{index}.schema.json",
        )
        for index, schema in enumerate(schemas)
    )
    registry = SchemaRegistry.from_documents(schemas, specs=(spec for spec in specs))
    assert registry.specs == specs

    extra = ContractSpec(
        schema_id="trading.test-extra/v1",
        self_id_field=None,
        family="ATTACHMENT",
        schema_filename="test-extra.schema.json",
    )
    with pytest.raises(ValueError, match="ContractSpec"):
        SchemaRegistry.from_documents(schemas, specs=(spec for spec in (*specs, extra)))


def test_local_and_jsonschema_oracles_agree_on_structural_vectors() -> None:
    from jsonschema import Draft202012Validator
    from referencing import Registry, Resource

    from build_finance.crypto_replay.schema_registry import SchemaRegistry

    schemas = _structural_schema_documents()
    local_registry = SchemaRegistry.from_documents(schemas)
    retrievals: list[str] = []

    def reject_unknown_uri(uri: str) -> Resource[Any]:
        retrievals.append(uri)
        raise AssertionError(f"unexpected schema retrieval: {uri}")

    registry = Registry(retrieve=reject_unknown_uri).with_resources(
        (schema["$id"], Resource.from_contents(schema)) for schema in schemas
    )

    by_contract = {schema["x-contract-schema"]: schema for schema in schemas}
    for schema in schemas:
        Draft202012Validator.check_schema(schema)

    for schema_id, positive, negatives in _structural_vectors():
        schema = by_contract[schema_id]
        validator = Draft202012Validator(schema, registry=registry)
        for document in (positive, *negatives):
            local_valid = not local_registry.validate(document, expected_schema=schema_id)
            oracle_valid = not tuple(validator.iter_errors(document))
            assert local_valid == oracle_valid

    assert retrievals == []


def _oracle_bounded_decimal_string(
    value: object,
    *,
    minimum: int | None,
    maximum: int | None,
) -> bool:
    if not isinstance(value, str) or not value:
        return False
    if value == "0":
        negative = False
        digits = "0"
    else:
        negative = value.startswith("-")
        digits = value[1:] if negative else value
        if not digits or digits[0] == "0" or any(character not in "0123456789" for character in digits):
            return False

    def compare_to_integer(bound: int) -> int:
        bound_negative = bound < 0
        magnitude = -bound if bound_negative else bound
        reversed_digits: list[str] = []
        while magnitude:
            magnitude, digit = divmod(magnitude, 10)
            reversed_digits.append(chr(48 + digit))
        bound_digits = "".join(reversed(reversed_digits)) or "0"

        if negative != bound_negative:
            return -1 if negative else 1
        if len(digits) != len(bound_digits):
            magnitude_order = -1 if len(digits) < len(bound_digits) else 1
        elif digits == bound_digits:
            magnitude_order = 0
        else:
            magnitude_order = -1 if digits < bound_digits else 1
        return -magnitude_order if negative else magnitude_order

    return (minimum is None or compare_to_integer(minimum) >= 0) and (
        maximum is None or compare_to_integer(maximum) <= 0
    )


def _oracle_utf8_length_at_most(value: object, maximum_bytes: int) -> bool:
    if not isinstance(value, str):
        return False
    try:
        return len(value.encode("utf-8", errors="strict")) <= maximum_bytes
    except UnicodeEncodeError:
        return False


def test_numeric_string_ranges_have_independent_boundary_oracles() -> None:
    from build_finance.crypto_replay.formats import is_bounded_decimal_string

    i128_min = -170141183460469231731687303715884105728
    i128_max = 170141183460469231731687303715884105727
    u64_max = 18446744073709551615
    vectors = (
        ("0", 0, u64_max, True),
        (str(u64_max), 0, u64_max, True),
        (str(u64_max + 1), 0, u64_max, False),
        ("-1", 0, u64_max, False),
        (str(i128_min), i128_min, i128_max, True),
        (str(i128_min - 1), i128_min, i128_max, False),
        (str(i128_max), i128_min, i128_max, True),
        (str(i128_max + 1), i128_min, i128_max, False),
        ("01", 0, u64_max, False),
        ("-0", i128_min, i128_max, False),
        ("+1", 0, u64_max, False),
        (True, 0, u64_max, False),
    )
    for value, minimum, maximum, expected in vectors:
        assert _oracle_bounded_decimal_string(value, minimum=minimum, maximum=maximum) is expected
        assert is_bounded_decimal_string(value, minimum=minimum, maximum=maximum) is expected


def test_decimal_strings_are_limit_safe_beyond_python_digit_cap() -> None:
    from build_finance.crypto_replay.formats import (
        is_bounded_decimal_string,
        parse_bounded_decimal_string,
    )

    decimal = "1" + ("0" * 5000)
    integer = 10**5000

    assert _oracle_bounded_decimal_string(decimal, minimum=0, maximum=None)
    assert is_bounded_decimal_string(decimal, minimum=0, maximum=None)
    assert parse_bounded_decimal_string(decimal, minimum=0) == integer
    assert _oracle_bounded_decimal_string(decimal, minimum=0, maximum=integer)
    assert parse_bounded_decimal_string(decimal, minimum=0, maximum=integer) == integer

    for malformed in ("0" + decimal, "+" + decimal, decimal + "x"):
        assert not _oracle_bounded_decimal_string(malformed, minimum=0, maximum=None)
        assert not is_bounded_decimal_string(malformed, minimum=0, maximum=None)

    assert not _oracle_bounded_decimal_string(decimal, minimum=0, maximum=integer - 1)
    assert not is_bounded_decimal_string(decimal, minimum=0, maximum=integer - 1)
    assert not _oracle_bounded_decimal_string("-" + decimal, minimum=0, maximum=None)
    assert not is_bounded_decimal_string("-" + decimal, minimum=0, maximum=None)


def test_utf8_byte_length_has_independent_multibyte_boundaries() -> None:
    from build_finance.crypto_replay.formats import has_utf8_length_at_most

    vectors = (
        ("", 0, True),
        ("ascii", 5, True),
        ("ascii", 4, False),
        ("éé", 4, True),
        ("ééa", 4, False),
        ("😀", 4, True),
        ("😀a", 4, False),
        ("\ud800", 3, False),
        (b"text", 4, False),
    )
    for value, maximum_bytes, expected in vectors:
        assert _oracle_utf8_length_at_most(value, maximum_bytes) is expected
        assert has_utf8_length_at_most(value, maximum_bytes) is expected


def test_codegen_scopes_expose_the_exact_frozen_inventory() -> None:
    from build_finance.crypto_replay.schema_codegen import get_scope_plan
    from build_finance.crypto_replay.schema_definitions import (
        ATTACHMENT_SCHEMA_IDS as REGISTERED_ATTACHMENT_SCHEMA_IDS,
    )
    from build_finance.crypto_replay.schema_definitions import (
        PRIMARY_SCHEMA_IDS,
        SUPPORTING_SCHEMA_IDS,
    )

    assert tuple(REGISTERED_ATTACHMENT_SCHEMA_IDS) == ATTACHMENT_SCHEMA_IDS
    assert len(PRIMARY_SCHEMA_IDS) == 8
    assert len(SUPPORTING_SCHEMA_IDS) == 13
    assert len(REGISTERED_ATTACHMENT_SCHEMA_IDS) == 27

    primary = get_scope_plan("primary")
    supporting = get_scope_plan("supporting")
    full = get_scope_plan("full")
    assert primary.checked_schema_ids == tuple(PRIMARY_SCHEMA_IDS)
    assert primary.writable_schema_ids == tuple(PRIMARY_SCHEMA_IDS)
    assert primary.preserved_schema_ids == ()
    assert primary.auxiliary_paths == ("primary-schema-bundle.json", "primary-schema-bundle.sha256")
    assert supporting.checked_schema_ids == tuple(SUPPORTING_SCHEMA_IDS)
    assert supporting.writable_schema_ids == tuple(SUPPORTING_SCHEMA_IDS)
    assert supporting.preserved_schema_ids == ()
    assert supporting.auxiliary_paths == ()
    assert full.checked_schema_ids == (
        *PRIMARY_SCHEMA_IDS,
        *SUPPORTING_SCHEMA_IDS,
        *REGISTERED_ATTACHMENT_SCHEMA_IDS,
    )
    assert full.writable_schema_ids == (*SUPPORTING_SCHEMA_IDS, *REGISTERED_ATTACHMENT_SCHEMA_IDS)
    assert full.preserved_schema_ids == tuple(PRIMARY_SCHEMA_IDS)
    assert full.auxiliary_paths == (
        "formulas/adverse-fill-draw-v1.txt",
        "schema-bundle.json",
        "schema-bundle.sha256",
        "schema-lock.json",
    )

    with pytest.raises(ValueError):
        get_scope_plan("unknown")


def test_primary_codegen_check_rejects_missing_extra_and_byte_different_resources(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    import build_finance.crypto_replay.schema_codegen as schema_codegen
    from build_finance.crypto_replay.schema_codegen import check_resources, write_resources
    from build_finance.crypto_replay.schema_definitions import (
        CONTRACT_SPECS_BY_SCHEMA,
        PRIMARY_SCHEMA_IDS,
        SUPPORTING_SCHEMA_IDS,
    )

    write_resources("primary", resources_root=tmp_path)
    check_resources("primary", resources_root=tmp_path)

    schema_path = tmp_path / "schemas" / "raw-event-v1.schema.json"
    schema_record = schema_path.read_bytes()
    schema_path.unlink()
    with pytest.raises(ValueError, match="generated resource is missing"):
        check_resources("primary", resources_root=tmp_path)

    write_resources("primary", resources_root=tmp_path)
    schema_path.write_bytes(schema_record + b" ")
    with pytest.raises(ValueError, match="generated resource differs"):
        check_resources("primary", resources_root=tmp_path)

    write_resources("primary", resources_root=tmp_path)
    (tmp_path / "schemas" / "rogue.schema.json").write_bytes(b"{}\n")
    with pytest.raises(ValueError, match="unknown managed schema resource"):
        check_resources("primary", resources_root=tmp_path)

    def tree_snapshot(root: Path) -> tuple[tuple[str, bytes | None], ...]:
        paths = sorted(root.rglob("*"), key=lambda path: path.relative_to(root).as_posix())
        return tuple(
            (path.relative_to(root).as_posix(), None if path.is_dir() else path.read_bytes()) for path in paths
        )

    primary_owned = f"schemas/{CONTRACT_SPECS_BY_SCHEMA[PRIMARY_SCHEMA_IDS[0]].schema_filename}"
    supporting_owned = f"schemas/{CONTRACT_SPECS_BY_SCHEMA[SUPPORTING_SCHEMA_IDS[0]].schema_filename}"
    rogue_write_cases = (
        ("root-json", "primary", primary_owned, "rogue.json", "unknown managed root resource"),
        ("root-digest", "primary", primary_owned, "rogue.sha256", "unknown managed root resource"),
        ("schema-json", "primary", primary_owned, "schemas/rogue.schema.json", "unknown managed schema resource"),
        ("formula", "supporting", supporting_owned, "formulas/rogue.txt", "unknown managed formula resource"),
    )
    for case_name, scope, owned_relative, rogue_relative, message in rogue_write_cases:
        resources_root = tmp_path / case_name
        owned_path = resources_root / owned_relative
        owned_path.parent.mkdir(parents=True)
        owned_path.write_bytes(b"OWNED SENTINEL\n")
        rogue_path = resources_root / rogue_relative
        rogue_path.parent.mkdir(parents=True, exist_ok=True)
        rogue_path.write_bytes(b"ROGUE SENTINEL\n")
        before = tree_snapshot(resources_root)

        with pytest.raises(ValueError, match=message):
            write_resources(scope, resources_root=resources_root)

        assert tree_snapshot(resources_root) == before
        assert rogue_path.read_bytes() == b"ROGUE SENTINEL\n"

    race_root = tmp_path / "build-race"
    race_owned_path = race_root / primary_owned
    race_owned_path.parent.mkdir(parents=True)
    race_owned_path.write_bytes(b"OWNED SENTINEL\n")
    race_before = tree_snapshot(race_root)
    original_build_expected_resources = schema_codegen.build_expected_resources

    def injecting_build_expected_resources(
        scope: Any,
        *,
        resources_root: Path,
    ) -> tuple[dict[str, bytes], frozenset[str]]:
        result = original_build_expected_resources(scope, resources_root=resources_root)
        (resources_root / "rogue.json").write_bytes(b"INJECTED ROGUE\n")
        return result

    with monkeypatch.context() as race_patch:
        race_patch.setattr(schema_codegen, "build_expected_resources", injecting_build_expected_resources)
        with pytest.raises(ValueError, match="unknown managed root resource"):
            write_resources("primary", resources_root=race_root)

    expected_race_after = tuple(sorted((*race_before, ("rogue.json", b"INJECTED ROGUE\n"))))
    assert tree_snapshot(race_root) == expected_race_after
    assert race_owned_path.read_bytes() == b"OWNED SENTINEL\n"

    undefined_root = tmp_path / "known-undefined"
    with monkeypatch.context() as staged_patch:
        staged_ids = _write_staged_supporting_resources(staged_patch, undefined_root, count=4)
        undefined_owned_path = undefined_root / "schemas" / CONTRACT_SPECS_BY_SCHEMA[staged_ids[0]].schema_filename
        undefined_owned_path.write_bytes(b"OWNED SENTINEL\n")
        undefined_id = SUPPORTING_SCHEMA_IDS[len(staged_ids)]
        undefined_path = undefined_root / "schemas" / CONTRACT_SPECS_BY_SCHEMA[undefined_id].schema_filename
        undefined_path.write_bytes(b"KNOWN UNDEFINED\n")
        undefined_before = tree_snapshot(undefined_root)

        with pytest.raises(ValueError, match="extra generated schema resource"):
            write_resources("supporting", resources_root=undefined_root)

        assert tree_snapshot(undefined_root) == undefined_before
        assert undefined_owned_path.read_bytes() == b"OWNED SENTINEL\n"
        assert undefined_path.read_bytes() == b"KNOWN UNDEFINED\n"


def _staged_supporting_schema(schema_id: str) -> dict[str, Any]:
    from build_finance.crypto_replay.schema_definitions import json_schema_id

    return {
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": json_schema_id(schema_id),
        "x-contract-schema": schema_id,
        "type": "object",
        "required": ["schema"],
        "properties": {"schema": {"const": schema_id}},
        "additionalProperties": False,
    }


def _write_staged_supporting_resources(
    monkeypatch: pytest.MonkeyPatch,
    resources_root: Path,
    *,
    count: int = 4,
) -> tuple[str, ...]:
    import build_finance.crypto_replay.schema_definitions as definitions
    from build_finance.crypto_replay.schema_codegen import write_resources
    from build_finance.crypto_replay.schema_definitions import SUPPORTING_SCHEMA_IDS

    staged_ids = SUPPORTING_SCHEMA_IDS[:count]
    monkeypatch.setattr(definitions, "PRIMARY_SCHEMA_DOCUMENTS", {})
    monkeypatch.setattr(
        definitions,
        "SUPPORTING_SCHEMA_DOCUMENTS",
        {schema_id: _staged_supporting_schema(schema_id) for schema_id in staged_ids},
    )
    monkeypatch.setattr(definitions, "ATTACHMENT_SCHEMA_DOCUMENTS", {})
    write_resources("supporting", resources_root=resources_root)
    return staged_ids


@pytest.mark.parametrize("count", (4, 9, 13))
def test_supporting_codegen_accepts_each_staged_definition_count(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    count: int,
) -> None:
    from build_finance.crypto_replay.schema_codegen import check_resources

    _write_staged_supporting_resources(monkeypatch, tmp_path, count=count)
    check_resources("supporting", resources_root=tmp_path)


@pytest.mark.parametrize("relative_path", ("rogue.json", "rogue.sha256"))
def test_codegen_rejects_unknown_managed_root_resources(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    relative_path: str,
) -> None:
    from build_finance.crypto_replay.schema_codegen import check_resources

    _write_staged_supporting_resources(monkeypatch, tmp_path)
    (tmp_path / relative_path).write_bytes(b"rogue\n")

    with pytest.raises(ValueError, match="unknown managed root resource"):
        check_resources("supporting", resources_root=tmp_path)


def test_codegen_rejects_unknown_managed_schema_resource(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    from build_finance.crypto_replay.schema_codegen import check_resources

    _write_staged_supporting_resources(monkeypatch, tmp_path)
    (tmp_path / "schemas" / "rogue.schema.json").write_bytes(b"rogue\n")

    with pytest.raises(ValueError, match="unknown managed schema resource"):
        check_resources("supporting", resources_root=tmp_path)


def test_codegen_rejects_unknown_managed_formula_resource(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    from build_finance.crypto_replay.schema_codegen import check_resources

    _write_staged_supporting_resources(monkeypatch, tmp_path)
    formula = tmp_path / "formulas" / "rogue.txt"
    formula.parent.mkdir()
    formula.write_bytes(b"rogue\n")

    with pytest.raises(ValueError, match="unknown managed formula resource"):
        check_resources("supporting", resources_root=tmp_path)


def test_codegen_allows_known_nonowned_resources_but_rejects_undefined_owned_schema(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    from build_finance.crypto_replay.schema_codegen import check_resources
    from build_finance.crypto_replay.schema_definitions import (
        ATTACHMENT_SCHEMA_IDS as REGISTERED_ATTACHMENT_SCHEMA_IDS,
    )
    from build_finance.crypto_replay.schema_definitions import (
        CONTRACT_SPECS_BY_SCHEMA,
        PRIMARY_SCHEMA_IDS,
        SUPPORTING_SCHEMA_IDS,
    )

    staged_ids = _write_staged_supporting_resources(monkeypatch, tmp_path)
    for root_name in (
        "primary-schema-bundle.json",
        "primary-schema-bundle.sha256",
        "schema-bundle.json",
        "schema-bundle.sha256",
        "schema-lock.json",
    ):
        (tmp_path / root_name).write_bytes(b"known non-owned resource\n")

    for schema_id in (PRIMARY_SCHEMA_IDS[0], REGISTERED_ATTACHMENT_SCHEMA_IDS[0]):
        filename = CONTRACT_SPECS_BY_SCHEMA[schema_id].schema_filename
        (tmp_path / "schemas" / filename).write_bytes(b"known non-owned resource\n")

    formula = tmp_path / "formulas" / "adverse-fill-draw-v1.txt"
    formula.parent.mkdir()
    formula.write_bytes(b"known non-owned resource\n")
    check_resources("supporting", resources_root=tmp_path)

    absent_owned_id = SUPPORTING_SCHEMA_IDS[len(staged_ids)]
    absent_owned_filename = CONTRACT_SPECS_BY_SCHEMA[absent_owned_id].schema_filename
    (tmp_path / "schemas" / absent_owned_filename).write_bytes(b"recognized but undefined\n")
    with pytest.raises(ValueError, match="extra generated schema resource"):
        check_resources("supporting", resources_root=tmp_path)
