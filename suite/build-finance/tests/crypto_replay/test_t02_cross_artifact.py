"""T02 RED contracts for retained run-input and cross-artifact identity."""

from __future__ import annotations

import importlib
from copy import deepcopy
from dataclasses import replace
from pathlib import Path
from typing import Any

import pytest

from build_finance.crypto_replay.canonical import canonical_json_bytes, parse_canonical_record, sha256_hex
from tests.crypto_replay.test_t02_supporting_contracts import (
    T02Vector,
    _digest,
    _reseal,
    build_t02_vector,
)

RESOURCE_ROOT = Path(__file__).resolve().parents[2] / "build_finance" / "crypto_replay" / "resources"


def _run_inputs_symbol(name: str, capability: str) -> Any:
    target = "build_finance.crypto_replay.run_inputs"
    missing_message: str | None = None
    try:
        module = importlib.import_module(target)
    except ModuleNotFoundError as error:
        if error.name == target:
            missing_message = f"T02 RED - missing {capability} in {target}"
        else:
            raise
    if missing_message is not None:
        pytest.fail(missing_message, pytrace=False)
    symbol = getattr(module, name, None)
    if symbol is None:
        pytest.fail(f"T02 RED - missing {capability}: {target}.{name}", pytrace=False)
    return symbol


def _make_bundle(vector: T02Vector, **changes: Any) -> Any:
    contract_code_preimage = _run_inputs_symbol("ContractCodePreimage", "closed code-preimage binding")
    run_input_bundle = _run_inputs_symbol("RunInputBundle", "closed run-input bundle")

    values: dict[str, Any] = {
        "fixture_manifest": vector.documents["trading.fixture-manifest/v1"],
        "config_admission_receipt": vector.documents["trading.config-admission-receipt/v1"],
        "replay_risk_config": vector.documents["trading.replay-risk-config/v1"],
        "raw_config_bytes": vector.raw_config_bytes,
        "source_admission_receipts": vector.source_receipts,
        "normalized_events": vector.raw_events,
        "run_closure_receipt": vector.documents["trading.run-closure-receipt/v1"],
        "availability_schedule": vector.attachments["trading.availability-schedule/v1"],
        "counter_capacity": vector.attachments["trading.counter-capacity/v1"],
        "source_tree": vector.attachments["trading.source-tree/v1"],
        "model_registry": None,
        "model_signal_manifest": None,
        "public_seed_bytes": vector.public_seed_bytes,
        "code_preimages": tuple(
            contract_code_preimage(field_name=name, payload=payload, assurance="CONTRACT_ONLY")
            for name, payload in vector.code_preimages.items()
        ),
    }
    values.update(changes)
    return run_input_bundle(**values)


def _verify(vector: T02Vector, *, run_receipt: dict[str, Any] | None = None, bundle: Any | None = None) -> Any:
    verify_contract_run_inputs = _run_inputs_symbol(
        "verify_contract_run_inputs",
        "contract-only run-input verification",
    )

    return verify_contract_run_inputs(
        vector.documents["trading.run-receipt/v1"] if run_receipt is None else run_receipt,
        _make_bundle(vector) if bundle is None else bundle,
    )


def _assert_rejected(
    vector: T02Vector, *, run_receipt: dict[str, Any] | None = None, bundle: Any | None = None
) -> None:
    from build_finance.crypto_replay.schema_model import ValidationIssue

    with pytest.raises(ValueError) as caught:
        _verify(vector, run_receipt=run_receipt, bundle=bundle)
    arguments: tuple[Any, ...]
    if len(caught.value.args) == 1 and isinstance(caught.value.args[0], (list, tuple)):
        arguments = tuple(caught.value.args[0])
    else:
        arguments = caught.value.args
    assert arguments and all(isinstance(issue, ValidationIssue) for issue in arguments)


def test_run_receipt_requires_matching_closure_receipt() -> None:
    vector = build_t02_vector()
    verified = _verify(vector)
    assert verified.authority == "CONTRACT_ONLY"
    run = vector.documents["trading.run-receipt/v1"]
    closure = vector.documents["trading.run-closure-receipt/v1"]
    shared_fields = (
        "fixture_manifest_sha256",
        "config_admission_receipt_id",
        "validated_config_sha256",
        "model_signal_mode",
        "model_registry_sha256",
        "model_signal_manifest_sha256",
        "schema_bundle_sha256",
        "admission_code_sha256",
        "normalization_code_sha256",
        "availability_grouping_code_sha256",
        "run_closure_code_sha256",
        "risk_code_sha256",
        "feature_code_sha256",
        "fill_code_sha256",
        "accounting_code_sha256",
        "source_admission_receipt_ids",
        "availability_schedule_sha256",
        "run_end_position_policy",
        "terminal_equal_time_group",
    )
    for field in shared_fields:
        assert closure[field] == run[field]
        mutation = deepcopy(closure)
        value = mutation[field]
        if value is None:
            mutation[field] = _digest(f"closure-{field}")
        elif isinstance(value, list):
            mutation[field] = [*value, _digest(f"closure-{field}")]
        elif value in ("DISABLED", "CACHED_FIXTURES"):
            mutation[field] = "CACHED_FIXTURES" if value == "DISABLED" else "DISABLED"
        elif value in ("LEAVE_MARKED_OPEN", "FORCE_CLOSE_NEXT_EVENT"):
            mutation[field] = "FORCE_CLOSE_NEXT_EVENT" if value == "LEAVE_MARKED_OPEN" else "LEAVE_MARKED_OPEN"
        elif field == "terminal_equal_time_group":
            mutation[field] = str(int(value) + 1)
        else:
            mutation[field] = _digest(f"closure-{field}")
        changed_closure = _reseal(mutation)
        coordinated_run = _reseal({**run, "run_closure_receipt_id": changed_closure["run_closure_receipt_id"]})
        mutated_bundle = replace(_make_bundle(vector), run_closure_receipt=changed_closure)
        _assert_rejected(vector, run_receipt=coordinated_run, bundle=mutated_bundle)
    failed = _reseal(
        {
            **closure,
            "status": "FAIL",
            "reason_codes": ["ADMISSION_RUN_END_UNCLOSED"],
            "terminal_equal_time_group": None,
        }
    )
    failed_run = _reseal({**run, "run_closure_receipt_id": failed["run_closure_receipt_id"]})
    _assert_rejected(
        vector,
        run_receipt=failed_run,
        bundle=replace(_make_bundle(vector), run_closure_receipt=failed),
    )


def test_known_good_graph_resolves_every_t02_authority() -> None:
    from build_finance.crypto_replay.content_ids import verify_content_id
    from build_finance.crypto_replay.schema_definitions import CONTRACT_SPECS_BY_SCHEMA
    from build_finance.crypto_replay.schema_registry import validate_contract
    from tests.crypto_replay.support.known_good_graph import (
        EXPECTED_PUBLIC_SEED_HEX,
        EXPECTED_SCHEDULE,
        build_known_good_graph,
    )

    lock = parse_canonical_record((RESOURCE_ROOT / "schema-lock.json").read_bytes())
    schema_bundle = parse_canonical_record((RESOURCE_ROOT / "schema-bundle.json").read_bytes())
    schema_rows = schema_bundle["schemas"]
    formula_rows = schema_bundle["binary_formulas"]
    assert isinstance(schema_rows, list)
    assert isinstance(formula_rows, list)
    primary_rows = [row for row in schema_rows if isinstance(row, dict) and row["family"] == "PRIMARY"]
    supporting_rows = [row for row in schema_rows if isinstance(row, dict) and row["family"] == "SUPPORTING"]
    attachment_rows = [row for row in schema_rows if isinstance(row, dict) and row["family"] == "ATTACHMENT"]
    assert len(primary_rows) == lock["primary_contract_count"] == 8
    assert len(supporting_rows) == lock["supporting_contract_count"] == 13
    assert len(attachment_rows) == lock["json_attachment_schema_count"] == 27
    assert len(formula_rows) == lock["binary_formula_count"] == 1
    assert len(schema_rows) == lock["generated_json_schema_count"] == 48
    assert len(schema_rows) + len(formula_rows) == lock["total_authority_contract_count"] == 49

    graph = build_known_good_graph()
    assert graph.schema_lock == lock
    assert graph.authority_counts == {
        "primary": 8,
        "supporting": 13,
        "attachment": 27,
        "binary_formula": 1,
        "generated_json_schema": 48,
        "total_authority": 49,
    }
    assert graph.public_seed_hex == EXPECTED_PUBLIC_SEED_HEX
    assert graph.run_input_bundle.availability_schedule["availability_groups"] == EXPECTED_SCHEDULE
    assert graph.run_input_bundle.counter_capacity["source_admission_count"] == "2"
    assert graph.run_input_bundle.counter_capacity["raw_event_count"] == "2"
    assert graph.run_input_bundle.counter_capacity["availability_group_count"] == "2"
    assert graph.run_input_bundle.counter_capacity["market_count"] == "1"
    assert graph.run_input_bundle.counter_capacity["model_candidate_count"] == "0"
    assert graph.run_input_bundle.counter_capacity["decision_sequence_next_upper_bound"] == "3"
    assert graph.run_input_bundle.counter_capacity["producer_sequence_next_upper_bound"] == "0"
    assert graph.run_input_bundle.counter_capacity["intent_sequence_next_upper_bound"] == "3"
    assert graph.run_input_bundle.counter_capacity["fill_receipt_sequence_next_upper_bound"] == "3"
    assert graph.run_input_bundle.counter_capacity["unmatched_reservation_count_upper_bound"] == "4"
    assert graph.run_input_bundle.counter_capacity["state_sequence_next_upper_bound"] == "13"
    assert graph.run_input_bundle.counter_capacity["ledger_sequence_next_upper_bound"] == "43"
    assert graph.run_input_bundle.run_closure_receipt["model_signal_mode"] == "DISABLED"
    assert graph.benchmark_authority == "CONTRACT_ONLY_INELIGIBLE"
    assert graph.execution_quarantine_receipt_id not in graph.ledger_member_content_ids

    for row in (*primary_rows, *supporting_rows, *attachment_rows):
        assert isinstance(row, dict)
        digest = row["schema_sha256"]
        assert isinstance(digest, str)
        payload = graph.resolver.resolve_bytes(digest)
        assert payload is not None
        assert sha256_hex(payload) == digest
    for row in formula_rows:
        assert isinstance(row, dict)
        digest = row["formula_sha256"]
        assert isinstance(digest, str)
        payload = graph.resolver.resolve_bytes(digest)
        assert payload is not None
        assert sha256_hex(payload) == digest

    assert len(graph.authority_content_ids) == 21
    for content_id in graph.authority_content_ids:
        resolved = graph.resolver.resolve_object(content_id)
        assert resolved is not None
        assert resolved.assurance == "SCHEMA_VALID"
        assert verify_content_id(resolved.document)
        schema_id = resolved.document["schema"]
        assert isinstance(schema_id, str)
        assert CONTRACT_SPECS_BY_SCHEMA[schema_id].family in {"PRIMARY", "SUPPORTING"}
        assert not validate_contract(resolved.document, expected_schema=schema_id)

    for resolved in graph.resolver.resolved_contents:
        schema_id = resolved.document["schema"]
        assert isinstance(schema_id, str)
        if CONTRACT_SPECS_BY_SCHEMA[schema_id].family == "SUPPORTING":
            assert resolved.assurance != "DIGEST_ONLY"

    verified = _run_inputs_symbol(
        "verify_contract_run_inputs",
        "contract-only known-good graph resolver",
    )(graph.run_receipt, graph.run_input_bundle)
    assert verified.authority == "CONTRACT_ONLY"


def test_public_seed_bytes_resolve_from_closed_run_inputs() -> None:
    vector = build_t02_vector()
    verified = _verify(vector)
    run = vector.documents["trading.run-receipt/v1"]
    assert len(verified.public_seed_bytes) == 32
    assert verified.public_seed_bytes == bytes.fromhex(run["public_seed_hex"])
    assert sha256_hex(verified.public_seed_bytes) == run["public_seed_sha256"]
    assert vector.resolver.resolve_bytes(run["public_seed_sha256"]) == vector.public_seed_bytes


def test_public_seed_hash_mismatch_prevents_genesis() -> None:
    vector = build_t02_vector()
    changed_seed = bytes([vector.public_seed_bytes[0] ^ 1, *vector.public_seed_bytes[1:]])
    bundle = replace(_make_bundle(vector), public_seed_bytes=changed_seed)
    _assert_rejected(vector, bundle=bundle)
    forbidden_genesis_surfaces = (
        "create_genesis_state",
        "construct_genesis",
        "append_initial_ledger_prefix",
        "run_replay",
    )
    run_inputs = importlib.import_module("build_finance.crypto_replay.run_inputs")
    assert all(not hasattr(run_inputs, name) for name in forbidden_genesis_surfaces)


def test_run_receipt_has_no_output_cycle() -> None:
    from build_finance.crypto_replay.schema_definitions import SUPPORTING_SCHEMA_DOCUMENTS

    vector = build_t02_vector()
    _verify(vector)
    run = vector.documents["trading.run-receipt/v1"]
    forbidden = {
        "ledger_root_id",
        "output_sha256",
        "final_portfolio_state_id",
        "run_end_ledger_record_id",
        "benchmark_receipt_id",
    }
    assert not forbidden & run.keys()
    properties = set(SUPPORTING_SCHEMA_DOCUMENTS["trading.run-receipt/v1"]["properties"])
    assert not forbidden & properties
    for field in forbidden:
        mutation = _reseal({**run, field: _digest(field)})
        _assert_rejected(vector, run_receipt=mutation)


def test_availability_cutoff_plateau_matches_max_formula() -> None:
    derive_availability_schedule = _run_inputs_symbol(
        "derive_availability_schedule",
        "availability-cutoff max/plateau derivation",
    )

    vector = build_t02_vector()
    fixture = deepcopy(vector.documents["trading.fixture-manifest/v1"])
    config_receipt = _reseal({**vector.documents["trading.config-admission-receipt/v1"], "admission_sequence": "5"})
    source_receipts = []
    for receipt, sequence in zip(vector.source_receipts, ("1", "4"), strict=True):
        source_receipts.append(_reseal({**receipt, "admission_sequence": sequence}))
    for file_row, sequence in zip(fixture["files"], ("1", "4"), strict=True):
        file_row["admission_sequence"] = sequence
    fixture = _reseal(fixture)
    bundle = _make_bundle(
        vector,
        fixture_manifest=fixture,
        config_admission_receipt=config_receipt,
        source_admission_receipts=tuple(source_receipts),
    )
    schedule = derive_availability_schedule(bundle)
    assert schedule["availability_groups"] == [
        {"availability_slot": "1", "equal_time_group": "1", "admission_cutoff": "5"},
        {"availability_slot": "2", "equal_time_group": "2", "admission_cutoff": "5"},
    ]
    decreasing = deepcopy(schedule)
    decreasing["availability_groups"][1]["admission_cutoff"] = "4"
    assert canonical_json_bytes(decreasing) != canonical_json_bytes(schedule)


def _build_cached_model_case(vector: T02Vector) -> tuple[dict[str, Any], Any]:
    registry = vector.documents["trading.model-registry/v1"]
    manifest = vector.documents["trading.model-signal-manifest/v1"]
    counter = vector.attachments["trading.counter-capacity/v1"]
    closure = vector.documents["trading.run-closure-receipt/v1"]
    run = vector.documents["trading.run-receipt/v1"]
    cached_counter = {
        **counter,
        "model_signal_mode": "CACHED_FIXTURES",
        "model_registry_sha256": registry["model_registry_sha256"],
        "model_signal_manifest_sha256": manifest["model_signal_manifest_sha256"],
        "model_candidate_count": "1",
        "producer_sequence_next_upper_bound": "1",
        "ledger_sequence_next_upper_bound": "45",
    }
    cached_closure = _reseal(
        {
            **closure,
            "model_signal_mode": "CACHED_FIXTURES",
            "model_registry_sha256": registry["model_registry_sha256"],
            "model_signal_manifest_sha256": manifest["model_signal_manifest_sha256"],
            "counter_capacity_sha256": sha256_hex(canonical_json_bytes(cached_counter)),
        }
    )
    candidate = manifest["candidates"][0]
    cached_run = _reseal(
        {
            **run,
            "run_closure_receipt_id": cached_closure["run_closure_receipt_id"],
            "model_signal_mode": "CACHED_FIXTURES",
            "model_registry_sha256": registry["model_registry_sha256"],
            "model_signal_manifest_sha256": manifest["model_signal_manifest_sha256"],
            "selected_model_scopes": [
                {
                    "market_id": vector.documents["trading.fixture-manifest/v1"]["allowed_markets"][0]["market_id"],
                    "decision_sequence": candidate["decision_sequence"],
                    "horizon_ns": candidate["horizon_ns"],
                    "producer_scope_key_sha256": candidate["requested_producer_scope_key_sha256"],
                }
            ],
            "model_decision_budget_ns": "2",
        }
    )
    return cached_run, _make_bundle(
        vector,
        run_closure_receipt=cached_closure,
        counter_capacity=cached_counter,
        model_registry=registry,
        model_signal_manifest=manifest,
    )


def test_run_gate_recomputes_model_capacity_and_fixture_tick() -> None:
    vector = build_t02_vector()
    _verify(vector)
    run = vector.documents["trading.run-receipt/v1"]
    _assert_rejected(vector, run_receipt=_reseal({**run, "replay_tick_ns": "1000000001"}))

    closure = vector.documents["trading.run-closure-receipt/v1"]
    for field, value in (
        ("model_signal_mode", "CACHED_FIXTURES"),
        ("model_registry_sha256", _digest("unexpected-model-registry")),
        ("model_signal_manifest_sha256", _digest("unexpected-model-manifest")),
    ):
        changed_closure = _reseal({**closure, field: value})
        changed_run = _reseal(
            {
                **run,
                field: value,
                "run_closure_receipt_id": changed_closure["run_closure_receipt_id"],
            }
        )
        _assert_rejected(
            vector,
            run_receipt=changed_run,
            bundle=replace(_make_bundle(vector), run_closure_receipt=changed_closure),
        )
    counter = vector.attachments["trading.counter-capacity/v1"]
    assert counter["model_candidate_count"] == "0"
    assert counter["producer_sequence_next_upper_bound"] == "0"

    registry = vector.documents["trading.model-registry/v1"]
    manifest = vector.documents["trading.model-signal-manifest/v1"]
    cached_counter = {
        **counter,
        "model_signal_mode": "CACHED_FIXTURES",
        "model_registry_sha256": registry["model_registry_sha256"],
        "model_signal_manifest_sha256": manifest["model_signal_manifest_sha256"],
        "model_candidate_count": "1",
        "producer_sequence_next_upper_bound": "1",
        "ledger_sequence_next_upper_bound": "45",
    }
    cached_counter_digest = sha256_hex(canonical_json_bytes(cached_counter))
    cached_closure = _reseal(
        {
            **closure,
            "model_signal_mode": "CACHED_FIXTURES",
            "model_registry_sha256": registry["model_registry_sha256"],
            "model_signal_manifest_sha256": manifest["model_signal_manifest_sha256"],
            "counter_capacity_sha256": cached_counter_digest,
        }
    )
    candidate = manifest["candidates"][0]
    cached_run = _reseal(
        {
            **run,
            "run_closure_receipt_id": cached_closure["run_closure_receipt_id"],
            "model_signal_mode": "CACHED_FIXTURES",
            "model_registry_sha256": registry["model_registry_sha256"],
            "model_signal_manifest_sha256": manifest["model_signal_manifest_sha256"],
            "selected_model_scopes": [
                {
                    "market_id": vector.documents["trading.fixture-manifest/v1"]["allowed_markets"][0]["market_id"],
                    "decision_sequence": candidate["decision_sequence"],
                    "horizon_ns": candidate["horizon_ns"],
                    "producer_scope_key_sha256": candidate["requested_producer_scope_key_sha256"],
                }
            ],
            "model_decision_budget_ns": "2",
        }
    )
    cached_bundle = _make_bundle(
        vector,
        run_closure_receipt=cached_closure,
        counter_capacity=cached_counter,
        model_registry=registry,
        model_signal_manifest=manifest,
    )
    assert _verify(vector, run_receipt=cached_run, bundle=cached_bundle).authority == "CONTRACT_ONLY"

    stale_body = deepcopy(manifest)
    stale_body["candidates"][0]["raw_byte_length"] = str(int(stale_body["candidates"][0]["raw_byte_length"]) + 1)
    _assert_rejected(
        vector,
        run_receipt=cached_run,
        bundle=replace(cached_bundle, model_signal_manifest=stale_body),
    )
    wrong_self_id = deepcopy(manifest)
    wrong_self_id["model_signal_manifest_sha256"] = _digest("wrong-manifest-self-id")
    _assert_rejected(
        vector,
        run_receipt=cached_run,
        bundle=replace(cached_bundle, model_signal_manifest=wrong_self_id),
    )
    cardinality_body = deepcopy(manifest)
    second_candidate = deepcopy(cardinality_body["candidates"][0])
    second_candidate.update(
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
    cardinality_body["candidates"].append(second_candidate)
    changed_manifest = _reseal(cardinality_body)
    changed_closure = _reseal(
        {
            **cached_closure,
            "model_signal_manifest_sha256": changed_manifest["model_signal_manifest_sha256"],
        }
    )
    changed_run = _reseal(
        {
            **cached_run,
            "model_signal_manifest_sha256": changed_manifest["model_signal_manifest_sha256"],
            "run_closure_receipt_id": changed_closure["run_closure_receipt_id"],
        }
    )
    _assert_rejected(
        vector,
        run_receipt=changed_run,
        bundle=replace(
            cached_bundle,
            run_closure_receipt=changed_closure,
            model_signal_manifest=changed_manifest,
        ),
    )


def test_run_input_gate_recomputes_normalized_event_set_from_bodies() -> None:
    derive_normalized_event_set = _run_inputs_symbol(
        "derive_normalized_event_set",
        "normalized-event retained-body reconstruction",
    )

    vector = build_t02_vector()
    bundle = _make_bundle(vector)
    reconstructed = derive_normalized_event_set(bundle)
    assert canonical_json_bytes(reconstructed) == canonical_json_bytes(
        vector.attachments["trading.normalized-event-set/v1"]
    )
    stale_event = deepcopy(vector.raw_events[0])
    stale_event["market"]["quote_amount_atoms"] = str(int(stale_event["market"]["quote_amount_atoms"]) + 1)
    _assert_rejected(
        vector,
        bundle=replace(bundle, normalized_events=(stale_event, vector.raw_events[1])),
    )
    stale_fixture = deepcopy(vector.documents["trading.fixture-manifest/v1"])
    stale_fixture["rights_manifest_sha256"] = _digest("mutated-rights-manifest")
    _assert_rejected(vector, bundle=replace(bundle, fixture_manifest=stale_fixture))
    rehashed_fixture = _reseal(stale_fixture)
    _assert_rejected(vector, bundle=replace(bundle, fixture_manifest=rehashed_fixture))

    mutated_event = deepcopy(vector.raw_events[0])
    mutated_event["market"]["quote_amount_atoms"] = str(int(mutated_event["market"]["quote_amount_atoms"]) + 1)
    mutated_event = _reseal(mutated_event)
    changed_bundle = replace(bundle, normalized_events=(mutated_event, vector.raw_events[1]))
    changed_root = derive_normalized_event_set(changed_bundle)
    assert canonical_json_bytes(changed_root) != canonical_json_bytes(reconstructed)
    _assert_rejected(vector, bundle=changed_bundle)

    invalid_event_sets: list[tuple[dict[str, Any], dict[str, Any]]] = []
    duplicate_ingest = _reseal({**vector.raw_events[1], "ingest_sequence": "1"})
    invalid_event_sets.append((vector.raw_events[0], duplicate_ingest))
    gapped_ingest = _reseal({**vector.raw_events[1], "ingest_sequence": "3"})
    invalid_event_sets.append((vector.raw_events[0], gapped_ingest))
    swapped_ingest_first = _reseal({**vector.raw_events[0], "ingest_sequence": "2"})
    swapped_ingest_second = _reseal({**vector.raw_events[1], "ingest_sequence": "1"})
    invalid_event_sets.append((swapped_ingest_first, swapped_ingest_second))
    swapped_source_first = _reseal({**vector.raw_events[0], "source_sequence": "2"})
    swapped_source_second = _reseal({**vector.raw_events[1], "source_sequence": "1"})
    invalid_event_sets.append((swapped_source_first, swapped_source_second))
    wrong_fixture_event = _reseal({**vector.raw_events[0], "fixture_manifest_sha256": _digest("wrong-event-fixture")})
    invalid_event_sets.append((wrong_fixture_event, vector.raw_events[1]))
    wrong_source_event = _reseal({**vector.raw_events[0], "source_admission_receipt_id": _digest("wrong-event-source")})
    invalid_event_sets.append((wrong_source_event, vector.raw_events[1]))
    wrong_payload_event = _reseal({**vector.raw_events[0], "raw_payload_sha256": _digest("wrong-event-payload")})
    invalid_event_sets.append((wrong_payload_event, vector.raw_events[1]))
    wrong_decimals_event = _reseal({**vector.raw_events[0], "base_decimals": 8})
    invalid_event_sets.append((wrong_decimals_event, vector.raw_events[1]))
    wrong_market_event = _reseal({**vector.raw_events[0], "market_id": "undeclared/market:jupiter"})
    invalid_event_sets.append((wrong_market_event, vector.raw_events[1]))
    wrong_group_event = _reseal({**vector.raw_events[1], "equal_time_group": "1"})
    invalid_event_sets.append((vector.raw_events[0], wrong_group_event))
    wrong_clock_event = _reseal({**vector.raw_events[1], "replay_clock_ns": "0"})
    invalid_event_sets.append((vector.raw_events[0], wrong_clock_event))
    for events in invalid_event_sets:
        with pytest.raises(ValueError):
            derive_normalized_event_set(replace(bundle, normalized_events=events))

    with pytest.raises(ValueError):
        derive_normalized_event_set(
            replace(
                bundle,
                source_admission_receipts=(vector.source_receipts[0],),
                normalized_events=(vector.raw_events[0],),
            )
        )

    dropped_bundle = replace(bundle, normalized_events=(vector.raw_events[0],))
    with pytest.raises(ValueError):
        derive_normalized_event_set(dropped_bundle)
    _assert_rejected(vector, bundle=dropped_bundle)

    dropped_root = {
        "schema": "trading.normalized-event-set/v1",
        "fixture_manifest_sha256": vector.documents["trading.fixture-manifest/v1"]["fixture_manifest_sha256"],
        "raw_event_count": "1",
        "event_ids": [vector.raw_events[0]["event_id"]],
    }
    dropped_digest = sha256_hex(canonical_json_bytes(dropped_root))
    changed_counter = {
        **vector.attachments["trading.counter-capacity/v1"],
        "normalized_event_set_sha256": dropped_digest,
        "raw_event_count": "1",
        "ledger_sequence_next_upper_bound": "42",
    }
    changed_counter_digest = sha256_hex(canonical_json_bytes(changed_counter))
    changed_closure = _reseal(
        {
            **vector.documents["trading.run-closure-receipt/v1"],
            "counter_capacity_sha256": changed_counter_digest,
        }
    )
    changed_run = _reseal(
        {
            **vector.documents["trading.run-receipt/v1"],
            "run_closure_receipt_id": changed_closure["run_closure_receipt_id"],
        }
    )
    _assert_rejected(
        vector,
        run_receipt=changed_run,
        bundle=replace(
            dropped_bundle,
            counter_capacity=changed_counter,
            run_closure_receipt=changed_closure,
        ),
    )


def _config_mode_case(
    vector: T02Vector,
    *,
    status: str,
    raw_bytes: bytes | None,
) -> tuple[dict[str, Any], Any]:
    if status == "VALID":
        assert raw_bytes == vector.raw_config_bytes
        return vector.documents["trading.run-receipt/v1"], _make_bundle(vector)
    if status == "MISSING":
        reason_codes = ["CONFIG_MISSING"]
        assert raw_bytes is None
    elif status == "INVALID":
        reason_codes = ["CONFIG_BYTES_INVALID"]
        assert raw_bytes is not None
    else:
        raise AssertionError(f"unsupported synthetic config status: {status}")
    config_receipt = _reseal(
        {
            **vector.documents["trading.config-admission-receipt/v1"],
            "status": status,
            "reason_codes": reason_codes,
            "raw_config_sha256": None if raw_bytes is None else sha256_hex(raw_bytes),
            "validated_config_sha256": None,
            "raw_byte_length": "0" if raw_bytes is None else str(len(raw_bytes)),
        }
    )
    availability_schedule = {
        **deepcopy(vector.attachments["trading.availability-schedule/v1"]),
        "config_admission_receipt_id": config_receipt["config_admission_receipt_id"],
    }
    availability_digest = sha256_hex(canonical_json_bytes(availability_schedule))
    closure = _reseal(
        {
            **vector.documents["trading.run-closure-receipt/v1"],
            "status": "NOT_REQUIRED_ZERO_AUTHORITY",
            "reason_codes": [],
            "config_admission_receipt_id": config_receipt["config_admission_receipt_id"],
            "validated_config_sha256": None,
            "availability_schedule_sha256": availability_digest,
            "terminal_equal_time_group": "2",
            "proof_row_limit": None,
            "proof_row_count_total": "0",
            "proof_budget_status": "NOT_REQUIRED",
            "market_proofs": [],
        }
    )
    run = _reseal(
        {
            **vector.documents["trading.run-receipt/v1"],
            "config_admission_receipt_id": config_receipt["config_admission_receipt_id"],
            "validated_config_sha256": None,
            "availability_schedule_sha256": availability_digest,
            "run_closure_receipt_id": closure["run_closure_receipt_id"],
        }
    )
    return run, _make_bundle(
        vector,
        config_admission_receipt=config_receipt,
        replay_risk_config=None,
        raw_config_bytes=raw_bytes,
        run_closure_receipt=closure,
        availability_schedule=availability_schedule,
    )


def _coordinate_config_receipt(
    vector: T02Vector,
    run: dict[str, Any],
    bundle: Any,
    config_receipt: dict[str, Any],
) -> tuple[dict[str, Any], Any]:
    closure = _reseal(
        {
            **bundle.run_closure_receipt,
            "config_admission_receipt_id": config_receipt["config_admission_receipt_id"],
        }
    )
    changed_run = _reseal(
        {
            **run,
            "config_admission_receipt_id": config_receipt["config_admission_receipt_id"],
            "run_closure_receipt_id": closure["run_closure_receipt_id"],
        }
    )
    return changed_run, replace(
        bundle,
        config_admission_receipt=config_receipt,
        run_closure_receipt=closure,
    )


def test_run_input_oracle_binds_canonical_availability_schedule() -> None:
    derive_availability_schedule = _run_inputs_symbol(
        "derive_availability_schedule",
        "independent canonical availability-schedule oracle",
    )

    vector = build_t02_vector()
    bundle = _make_bundle(vector)
    reconstructed = derive_availability_schedule(bundle)
    retained = vector.attachments["trading.availability-schedule/v1"]
    assert canonical_json_bytes(reconstructed) == canonical_json_bytes(retained)
    changed = deepcopy(retained)
    changed["availability_groups"][0]["admission_cutoff"] = "1"
    changed_digest = sha256_hex(canonical_json_bytes(changed))
    closure = _reseal(
        {
            **vector.documents["trading.run-closure-receipt/v1"],
            "availability_schedule_sha256": changed_digest,
        }
    )
    run = _reseal(
        {
            **vector.documents["trading.run-receipt/v1"],
            "availability_schedule_sha256": changed_digest,
            "availability_groups": deepcopy(changed["availability_groups"]),
            "run_closure_receipt_id": closure["run_closure_receipt_id"],
        }
    )
    changed_bundle = replace(bundle, run_closure_receipt=closure, availability_schedule=changed)
    _assert_rejected(vector, run_receipt=run, bundle=changed_bundle)

    config_cases = (
        ("MISSING", None),
        ("INVALID", b"SYNTHETIC INVALID CONFIG BYTES\n"),
        ("VALID", vector.raw_config_bytes),
    )
    built_cases: dict[tuple[str, bytes | None], tuple[dict[str, Any], Any]] = {}
    for status, raw_bytes in config_cases:
        case_run, case_bundle = _config_mode_case(vector, status=status, raw_bytes=raw_bytes)
        assert _verify(vector, run_receipt=case_run, bundle=case_bundle).authority == "CONTRACT_ONLY"
        built_cases[(status, raw_bytes)] = (case_run, case_bundle)

    valid_run, valid_bundle = built_cases[("VALID", vector.raw_config_bytes)]
    _assert_rejected(
        vector,
        run_receipt=valid_run,
        bundle=replace(valid_bundle, raw_config_bytes=vector.raw_config_bytes[:-1]),
    )
    _assert_rejected(
        vector,
        run_receipt=valid_run,
        bundle=replace(valid_bundle, raw_config_bytes=vector.raw_config_bytes + b"\n"),
    )
    missing_run, missing_bundle = built_cases[("MISSING", None)]
    _assert_rejected(
        vector,
        run_receipt=missing_run,
        bundle=replace(missing_bundle, raw_config_bytes=b""),
    )
    empty_invalid_run, empty_invalid_bundle = _config_mode_case(vector, status="INVALID", raw_bytes=b"")
    _assert_rejected(vector, run_receipt=empty_invalid_run, bundle=empty_invalid_bundle)
    invalid_bytes = b"SYNTHETIC INVALID CONFIG BYTES\n"
    invalid_run, invalid_bundle = built_cases[("INVALID", invalid_bytes)]
    _assert_rejected(
        vector,
        run_receipt=invalid_run,
        bundle=replace(invalid_bundle, raw_config_bytes=None),
    )
    for field, value in (
        ("raw_byte_length", "1"),
        ("raw_config_sha256", _digest("wrong-empty-config-digest")),
    ):
        changed_receipt = _reseal({**invalid_bundle.config_admission_receipt, field: value})
        changed_run, changed_config_bundle = _coordinate_config_receipt(
            vector,
            invalid_run,
            invalid_bundle,
            changed_receipt,
        )
        _assert_rejected(vector, run_receipt=changed_run, bundle=changed_config_bundle)

    contract_code_preimage = _run_inputs_symbol(
        "ContractCodePreimage",
        "closed code-preimage mutation matrix",
    )
    preimages = valid_bundle.code_preimages
    _assert_rejected(vector, run_receipt=valid_run, bundle=replace(valid_bundle, code_preimages=preimages[:-1]))
    extra = contract_code_preimage(
        field_name="extra_code_sha256",
        payload=b"SYNTHETIC EXTRA CONTRACT-ONLY PREIMAGE\n",
        assurance="CONTRACT_ONLY",
    )
    _assert_rejected(
        vector,
        run_receipt=valid_run,
        bundle=replace(valid_bundle, code_preimages=(*preimages, extra)),
    )
    _assert_rejected(
        vector,
        run_receipt=valid_run,
        bundle=replace(valid_bundle, code_preimages=(*preimages, preimages[0])),
    )

    first, second = preimages[:2]
    aliased_second = contract_code_preimage(
        field_name=second.field_name,
        payload=first.payload,
        assurance="CONTRACT_ONLY",
    )
    aliased_closure = _reseal(
        {
            **valid_bundle.run_closure_receipt,
            second.field_name: sha256_hex(first.payload),
        }
    )
    aliased_run = _reseal(
        {
            **valid_run,
            second.field_name: sha256_hex(first.payload),
            "run_closure_receipt_id": aliased_closure["run_closure_receipt_id"],
        }
    )
    _assert_rejected(
        vector,
        run_receipt=aliased_run,
        bundle=replace(
            valid_bundle,
            run_closure_receipt=aliased_closure,
            code_preimages=(first, aliased_second, *preimages[2:]),
        ),
    )

    lineage_payload = b"SYNTHETIC COORDINATED LINEAGE MUTATION\n"
    changed_preimage = contract_code_preimage(
        field_name="normalization_code_sha256",
        payload=lineage_payload,
        assurance="CONTRACT_ONLY",
    )
    changed_preimages = tuple(
        changed_preimage if item.field_name == "normalization_code_sha256" else item for item in preimages
    )
    _assert_rejected(
        vector,
        run_receipt=valid_run,
        bundle=replace(valid_bundle, code_preimages=changed_preimages),
    )
    new_hash = sha256_hex(lineage_payload)
    lineage_closure = _reseal({**valid_bundle.run_closure_receipt, "normalization_code_sha256": new_hash})
    lineage_run = _reseal(
        {
            **valid_run,
            "normalization_code_sha256": new_hash,
            "run_closure_receipt_id": lineage_closure["run_closure_receipt_id"],
        }
    )
    assert (
        _verify(
            vector,
            run_receipt=lineage_run,
            bundle=replace(
                valid_bundle,
                run_closure_receipt=lineage_closure,
                code_preimages=changed_preimages,
            ),
        ).authority
        == "CONTRACT_ONLY"
    )
