"""G2 RED contract for the offline live-paper capital engine.

The production package intentionally does not exist at this RED stage.  These
tests freeze the smallest acceptable vertical behavior before Task 2 builds it.
"""

from __future__ import annotations

import copy
import hashlib
import json
import re
from collections.abc import Mapping
from typing import Any

import pytest

_HEX64 = re.compile(r"^[0-9a-f]{64}$")
_EXPECTED_PRICE_Q18 = "100000000000000000000"
_EXPECTED_INITIAL_QUOTE_ATOMS = "100000000"
_EXPECTED_ORDER_BASE_ATOMS = "100000"
_EXPECTED_GROSS_QUOTE_ATOMS = "10000000"
_EXPECTED_FEE_QUOTE_ATOMS = "10000"
_EXPECTED_RESERVED_QUOTE_ATOMS = "10010000"
_EXPECTED_CASH_AFTER_FILL_ATOMS = "89990000"
_EXPECTED_BASE_AFTER_FILL_ATOMS = "100000"
_U64_MAX_TEXT = "18446744073709551615"
_U64_OVERFLOW_TEXT = "18446744073709551616"
_DIGEST_1 = "1" * 64
_DIGEST_2 = "2" * 64
_DIGEST_3 = "3" * 64
_DIGEST_4 = "4" * 64
_DIGEST_5 = "5" * 64
_DIGEST_6 = "6" * 64
_DIGEST_7 = "7" * 64
_TASK2_SCHEMA_IDS = (
    "trading.algorithm-candidate/v1",
    "trading.fusion-decision/v1",
    "trading.normalization-receipt/v1",
    "trading.decision-group-manifest/v1",
)
_TASK2_SELF_ID_FIELDS = {
    "trading.algorithm-candidate/v1": "algorithm_candidate_id",
    "trading.fusion-decision/v1": "fusion_decision_id",
    "trading.normalization-receipt/v1": "normalization_receipt_id",
    "trading.decision-group-manifest/v1": "decision_group_manifest_id",
}
_TASK2_U64_FIELDS = (
    ("trading.normalization-receipt/v1", "input_count"),
    ("trading.normalization-receipt/v1", "normalized_event_count"),
    ("trading.algorithm-candidate/v1", "decision_sequence"),
    ("trading.algorithm-candidate/v1", "equal_time_group"),
    ("trading.fusion-decision/v1", "decision_sequence"),
    ("trading.fusion-decision/v1", "equal_time_group"),
    ("trading.decision-group-manifest/v1", "member_count"),
)


def _assert_schema_recursively_closed(schema: Mapping[str, Any]) -> None:
    """Fail if any object node can accept implicit or optional fields."""

    nodes: list[Mapping[str, Any]] = [schema]
    for node in nodes:
        for keyword in ("$defs", "properties"):
            children = node.get(keyword)
            if isinstance(children, Mapping):
                nodes.extend(child for child in children.values() if isinstance(child, Mapping))
        items = node.get("items")
        if isinstance(items, Mapping):
            nodes.append(items)
        for keyword in ("anyOf", "oneOf", "allOf"):
            branches = node.get(keyword)
            if isinstance(branches, list):
                nodes.extend(branch for branch in branches if isinstance(branch, Mapping))
        negated = node.get("not")
        if isinstance(negated, Mapping):
            nodes.append(negated)
        if node.get("type") == "object":
            assert node.get("additionalProperties") is False
            assert set(node.get("required", ())) == set(node.get("properties", ()))


def _parse_canonical_record(record: bytes) -> dict[str, Any]:
    assert record.endswith(b"\n") and not record.endswith(b"\n\n")
    parsed = json.loads(record[:-1].decode("utf-8"), parse_float=lambda value: (_ for _ in ()).throw(ValueError(value)))
    assert isinstance(parsed, dict)
    assert _canonical_json_bytes(parsed) + b"\n" == record
    return parsed


def _task2_structural_issues_for_field(schema_id: str, field: str, value: Any) -> tuple[Any, ...]:
    """Validate one generated schema property through the live-paper resolver."""

    from build_finance.live_paper import registry

    schema = registry.get_schema_document(schema_id)
    properties = schema["properties"]
    assert isinstance(properties, Mapping)
    field_schema = properties[field]
    assert isinstance(field_schema, Mapping)
    return registry._validate_instance(field_schema, value, (field,), schema)


def _normalization_body() -> dict[str, Any]:
    return {
        "schema": "trading.normalization-receipt/v1",
        "normalization_receipt_id": "0" * 64,
        "source_batch_id": "synthetic-g2-batch-001",
        "normalization_code_sha256": _DIGEST_1,
        "input_content_ids": [_DIGEST_2, _DIGEST_3],
        "input_count": "2",
        "normalized_event_ids": [_DIGEST_4, _DIGEST_5],
        "normalized_event_count": "2",
        "output_merkle_root_sha256": _DIGEST_6,
        "status": "PASS",
        "reason_codes": [],
        "total_evidence": "TOTAL_INPUT_CLOSURE",
    }


def _algorithm_body(normalization_receipt_id: str) -> dict[str, Any]:
    return {
        "schema": "trading.algorithm-candidate/v1",
        "algorithm_candidate_id": "0" * 64,
        "normalization_receipt_id": normalization_receipt_id,
        "feature_snapshot_id": _DIGEST_7,
        "algorithm_id": "G2_LONG_OR_FLAT_SYNTHETIC_V1",
        "algorithm_version": "2026-08-21",
        "decision_group_key": "synthetic-g2-decision-group-001",
        "decision_sequence": "1",
        "equal_time_group": "1",
        "input_content_ids": [normalization_receipt_id, _DIGEST_7],
        "candidate_action": "OPEN_LONG",
        "rationale_code": "OPEN_IF_FLAT",
        "confidence_q18": "1000000000000000000",
        "can_size": False,
        "can_execute": False,
    }


def _fusion_body(normalization_receipt_id: str, algorithm_candidate_id: str) -> dict[str, Any]:
    return {
        "schema": "trading.fusion-decision/v1",
        "fusion_decision_id": "0" * 64,
        "normalization_receipt_id": normalization_receipt_id,
        "algorithm_candidate_id": algorithm_candidate_id,
        "model_signal_id": None,
        "decision_sequence": "1",
        "equal_time_group": "1",
        "input_content_ids": [normalization_receipt_id, algorithm_candidate_id],
        "model_mode": "DISABLED_ABSTAIN",
        "model_action": "ABSTAIN",
        "model_score_q18": "0",
        "model_probability_abstain_q18": "1000000000000000000",
        "candidate_action": "OPEN_LONG",
        "fused_action": "OPEN_LONG",
        "fusion_policy": "MODEL_ABSTAINS_USE_ALGORITHM_CANDIDATE",
        "can_size": False,
        "can_execute": False,
    }


def _manifest_body(normalization_receipt_id: str, algorithm_candidate_id: str, fusion_decision_id: str) -> dict[str, Any]:
    return {
        "schema": "trading.decision-group-manifest/v1",
        "decision_group_manifest_id": "0" * 64,
        "decision_group_key": "synthetic-g2-decision-group-001",
        "normalization_receipt_id": normalization_receipt_id,
        "algorithm_candidate_ids": [algorithm_candidate_id],
        "fusion_decision_ids": [fusion_decision_id],
        "member_content_ids": sorted(
            [normalization_receipt_id, algorithm_candidate_id, fusion_decision_id],
            key=lambda value: value.encode("utf-8"),
        ),
        "member_count": "3",
        "sealed_by": "CONTENT_ID_REGISTRY_V1",
    }


def _sealed_task2_vector() -> tuple[dict[str, Any], dict[str, Any], dict[str, Any], dict[str, Any]]:
    from build_finance.live_paper.content_ids import seal_content_id

    normalization_body = _normalization_body()
    normalization_body.pop("normalization_receipt_id")
    normalization = seal_content_id(normalization_body)
    algorithm_body = _algorithm_body(str(normalization["normalization_receipt_id"]))
    algorithm_body.pop("algorithm_candidate_id")
    algorithm = seal_content_id(algorithm_body)
    fusion_body = _fusion_body(str(normalization["normalization_receipt_id"]), str(algorithm["algorithm_candidate_id"]))
    fusion_body.pop("fusion_decision_id")
    fusion = seal_content_id(fusion_body)
    manifest_body = _manifest_body(
        str(normalization["normalization_receipt_id"]),
        str(algorithm["algorithm_candidate_id"]),
        str(fusion["fusion_decision_id"]),
    )
    manifest_body.pop("decision_group_manifest_id")
    manifest = seal_content_id(manifest_body)
    return normalization, algorithm, fusion, manifest


def _synthetic_verified_inputs() -> dict[str, Any]:
    """Return the hand-derived synthetic fixture used by the G2 vertical contract."""

    return {
        "schema": "build-finance.live-paper.synthetic-verified-input/v1",
        "evidence_classification": "SYNTHETIC_CONTRACT_VECTOR",
        "synthetic_notice": "Synthetic fixture; never observed market data, credentials, or venue state.",
        "run_id": "g2-red-synthetic-long-flat-001",
        "market_id": "SYNTH_BASE_SYNTH_QUOTE_SPOT",
        "base_mint": "SYNTH_BASE_MINT",
        "quote_mint": "SYNTH_QUOTE_MINT",
        "base_decimals": 6,
        "quote_decimals": 6,
        "positioning": "LONG_OR_FLAT_SPOT",
        "execution_mode": "OFFLINE_PAPER_ONLY",
        "model_mode": "DISABLED_ABSTAIN",
        "initial_portfolio": {
            "base_atoms": "0",
            "quote_atoms": _EXPECTED_INITIAL_QUOTE_ATOMS,
        },
        "risk": {
            "deterministic_risk_only": True,
            "max_notional_quote_atoms": _EXPECTED_GROSS_QUOTE_ATOMS,
            "fee_bps": 10,
            "max_impact_bps": 0,
            "max_participation_bps": 10000,
        },
        "events": [
            {
                "source_event_id": "synthetic-event-0001",
                "ingest_sequence": "1",
                "equal_time_group": "1",
                "replay_clock_ns": "1000000000",
                "kind": "QUOTE",
                "price_q18": _EXPECTED_PRICE_Q18,
                "executable_base_atoms": "250000",
            },
            {
                "source_event_id": "synthetic-event-0002",
                "ingest_sequence": "2",
                "equal_time_group": "2",
                "replay_clock_ns": "2000000000",
                "kind": "QUOTE",
                "price_q18": _EXPECTED_PRICE_Q18,
                "executable_base_atoms": "250000",
            },
        ],
    }


def _require_mapping(value: Any, label: str) -> Mapping[str, Any]:
    assert isinstance(value, Mapping), f"{label} must be a mapping, got {type(value).__name__}"
    return value


def _require_hex_id(value: Any, label: str) -> str:
    assert isinstance(value, str), f"{label} must be a hex content id string"
    assert _HEX64.fullmatch(value), f"{label} must be a 64-byte lowercase hex digest"
    return value


def _require_list(value: Any, label: str, length: int) -> list[Any]:
    assert isinstance(value, list), f"{label} must be a list"
    assert len(value) == length, f"{label} must contain {length} item(s)"
    return value


def _canonical_json_bytes(value: Any) -> bytes:
    _reject_float(value)
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"), allow_nan=False).encode("utf-8")


def _reject_float(value: Any) -> None:
    if isinstance(value, float):
        raise TypeError("G2 contract IDs are derived from integer/string canonical JSON, never floats")
    if isinstance(value, Mapping):
        for key, item in value.items():
            if not isinstance(key, str):
                raise TypeError("G2 contract IDs are derived from string-keyed canonical JSON objects")
            _reject_float(item)
    elif isinstance(value, list):
        for item in value:
            _reject_float(item)


def _computed_content_id(document: Mapping[str, Any], self_id_field: str) -> str:
    body = dict(document)
    body.pop(self_id_field, None)
    return hashlib.sha256(_canonical_json_bytes(body)).hexdigest()


def _assert_self_content_id(document: Mapping[str, Any], self_id_field: str, label: str) -> str:
    supplied = _require_hex_id(document.get(self_id_field), label)
    expected = _computed_content_id(document, self_id_field)
    assert supplied == expected, f"{label} content id mismatch: expected {expected}, got {supplied}"
    return supplied


def _assert_complete_vertical_contract(result: Mapping[str, Any]) -> None:
    """Assert the full G2 RED vertical path from synthetic input to closure."""

    mode = _require_mapping(result.get("mode"), "mode")
    assert mode == {
        "data_boundary": "OFFLINE_VERIFIED_INPUT",
        "execution": "PAPER_ONLY",
        "model": "DISABLED_ABSTAIN",
        "positioning": "LONG_OR_FLAT_SPOT",
        "runtime_dependencies": "PYTHON_STDLIB_ONLY",
    }
    assert result.get("schema") == "build-finance.live-paper.g2-run-result/v1"
    assert result.get("evidence_classification") == "SYNTHETIC_CONTRACT_VECTOR"

    capability_receipt = _require_mapping(result.get("capability_receipt"), "capability_receipt")
    assert capability_receipt.get("external_io_attempts") == []
    assert capability_receipt.get("provider_sdk_touches") == []
    assert capability_receipt.get("credential_lookups") == []
    assert capability_receipt.get("broker_wallet_or_signer_touches") == []
    assert capability_receipt.get("venue_order_touches") == []

    normalized_events = _require_list(result.get("normalized_events"), "normalized_events", 2)
    first_event = _require_mapping(normalized_events[0], "normalized_events[0]")
    second_event = _require_mapping(normalized_events[1], "normalized_events[1]")
    first_event_id = _assert_self_content_id(first_event, "event_id", "first normalized event id")
    second_event_id = _assert_self_content_id(second_event, "event_id", "second normalized event id")
    assert first_event["source_event_id"] == "synthetic-event-0001"
    assert second_event["source_event_id"] == "synthetic-event-0002"
    assert first_event["price_q18"] == _EXPECTED_PRICE_Q18
    assert second_event["price_q18"] == _EXPECTED_PRICE_Q18
    assert int(second_event["replay_clock_ns"]) > int(first_event["replay_clock_ns"])

    feature = _require_mapping(result.get("feature_snapshot"), "feature_snapshot")
    feature_id = _assert_self_content_id(feature, "feature_snapshot_id", "feature snapshot id")
    assert feature["schema"] == "build-finance.live-paper.feature-snapshot/v1"
    assert feature["as_of_event_id"] == first_event_id
    assert feature["market_id"] == "SYNTH_BASE_SYNTH_QUOTE_SPOT"
    assert feature["close_price_q18"] == _EXPECTED_PRICE_Q18
    assert feature["position_base_atoms_before"] == "0"
    assert feature["cash_quote_atoms_before"] == _EXPECTED_INITIAL_QUOTE_ATOMS

    algorithm = _require_mapping(result.get("algorithm_evidence"), "algorithm_evidence")
    algorithm_id = _assert_self_content_id(algorithm, "algorithm_evidence_id", "algorithm evidence id")
    assert algorithm["schema"] == "build-finance.live-paper.algorithm-evidence/v1"
    assert algorithm["feature_snapshot_id"] == feature_id
    assert algorithm["rule_id"] == "G2_SYNTHETIC_LONG_OR_FLAT_OPEN_IF_FLAT"
    assert algorithm["candidate_action"] == "OPEN_LONG"
    assert "quantity_base_atoms" not in algorithm
    assert "venue_order" not in algorithm

    model = _require_mapping(result.get("model_signal"), "model_signal")
    model_id = _assert_self_content_id(model, "model_signal_id", "model signal id")
    assert model["schema"] == "build-finance.live-paper.model-signal/v1"
    assert model["feature_snapshot_id"] == feature_id
    assert model["mode"] == "DISABLED"
    assert model["action"] == "ABSTAIN"
    assert model["can_size"] is False
    assert model["can_execute"] is False

    fusion = _require_mapping(result.get("fusion_decision"), "fusion_decision")
    fusion_id = _assert_self_content_id(fusion, "fusion_decision_id", "fusion decision id")
    assert fusion["schema"] == "build-finance.live-paper.fusion-decision/v1"
    assert fusion["model_signal_id"] == model_id
    assert fusion["algorithm_evidence_id"] == algorithm_id
    assert fusion["model_action"] == "ABSTAIN"
    assert fusion["candidate_action"] == "OPEN_LONG"
    assert fusion["can_size"] is False
    assert fusion["can_execute"] is False
    assert "quantity_base_atoms" not in fusion

    risk = _require_mapping(result.get("risk_decision"), "risk_decision")
    risk_id = _assert_self_content_id(risk, "risk_decision_id", "risk decision id")
    assert risk["schema"] == "build-finance.live-paper.risk-decision/v1"
    assert risk["fusion_decision_id"] == fusion_id
    assert risk["authority"] == "DETERMINISTIC_RISK_ONLY"
    assert risk["decision"] == "MINT_PAPER_INTENT"
    assert risk["reference_price_q18"] == _EXPECTED_PRICE_Q18
    assert risk["quantity_base_atoms"] == _EXPECTED_ORDER_BASE_ATOMS
    assert risk["max_notional_quote_atoms"] == _EXPECTED_GROSS_QUOTE_ATOMS
    assert risk["reserved_quote_atoms"] == _EXPECTED_RESERVED_QUOTE_ATOMS
    assert risk["model_sized"] is False

    intents = _require_list(result.get("paper_intents"), "paper_intents", 1)
    intent = _require_mapping(intents[0], "paper_intents[0]")
    intent_id = _assert_self_content_id(intent, "intent_id", "paper intent id")
    assert intent["schema"] == "build-finance.live-paper.paper-intent/v1"
    assert intent["risk_decision_id"] == risk_id
    assert intent["created_by"] == "DETERMINISTIC_RISK_ONLY"
    assert intent["action"] == "OPEN_LONG"
    assert intent["paper_only"] is True
    assert intent["quantity_base_atoms"] == _EXPECTED_ORDER_BASE_ATOMS
    assert intent["reserved_quote_atoms"] == _EXPECTED_RESERVED_QUOTE_ATOMS
    assert intent["broker_order_id"] is None
    assert intent["wallet_signature"] is None

    fills = _require_list(result.get("paper_fills"), "paper_fills", 1)
    fill = _require_mapping(fills[0], "paper_fills[0]")
    fill_id = _assert_self_content_id(fill, "fill_id", "paper fill id")
    assert fill["schema"] == "build-finance.live-paper.paper-fill/v1"
    assert fill["intent_id"] == intent_id
    assert fill["decision_event_id"] == first_event_id
    assert fill["fill_event_id"] == second_event_id
    assert fill["fill_policy"] == "STRICT_NEXT_EVENT"
    assert fill["filled_base_atoms"] == _EXPECTED_ORDER_BASE_ATOMS
    assert fill["gross_quote_atoms"] == _EXPECTED_GROSS_QUOTE_ATOMS
    assert fill["simulation_fee_quote_atoms"] == _EXPECTED_FEE_QUOTE_ATOMS
    assert fill["cash_delta_quote_atoms"] == f"-{_EXPECTED_RESERVED_QUOTE_ATOMS}"

    ledger = _require_mapping(result.get("ledger"), "ledger")
    ledger_id = _assert_self_content_id(ledger, "ledger_record_id", "ledger record id")
    assert ledger["schema"] == "build-finance.live-paper.ledger-record/v1"
    assert ledger["fill_id"] == fill_id
    assert ledger["cash_quote_atoms_after"] == _EXPECTED_CASH_AFTER_FILL_ATOMS
    assert ledger["position_base_atoms_after"] == _EXPECTED_BASE_AFTER_FILL_ATOMS
    assert ledger["postings"] == [
        {
            "account": "CASH_AVAILABLE",
            "asset_mint": "SYNTH_QUOTE_MINT",
            "delta_atoms": f"-{_EXPECTED_RESERVED_QUOTE_ATOMS}",
        },
        {
            "account": "POSITION_AVAILABLE",
            "asset_mint": "SYNTH_BASE_MINT",
            "delta_atoms": _EXPECTED_ORDER_BASE_ATOMS,
        },
        {
            "account": "FEES_PAID",
            "asset_mint": "SYNTH_QUOTE_MINT",
            "delta_atoms": _EXPECTED_FEE_QUOTE_ATOMS,
        },
    ]

    reconciliation = _require_mapping(result.get("reconciliation"), "reconciliation")
    reconciliation_id = _assert_self_content_id(reconciliation, "reconciliation_id", "reconciliation id")
    assert reconciliation["schema"] == "build-finance.live-paper.reconciliation/v1"
    assert reconciliation["ledger_record_id"] == ledger_id
    assert reconciliation["status"] == "PASS"
    assert reconciliation["cash_residual_quote_atoms"] == "0"
    assert reconciliation["position_residual_base_atoms"] == "0"
    assert reconciliation["unmatched_paper_intent_count"] == 0
    assert reconciliation["external_position_source"] is None

    closure = _require_mapping(result.get("closure"), "closure")
    _assert_self_content_id(closure, "closure_id", "closure id")
    assert closure["schema"] == "build-finance.live-paper.run-closure/v1"
    assert closure["reconciliation_id"] == reconciliation_id
    assert closure["status"] == "CLOSED"
    assert closure["final_position_state"] == "LONG"
    assert closure["open_paper_intent_count"] == 0
    assert closure["open_venue_order_count"] == 0
    assert closure["final_cash_quote_atoms"] == _EXPECTED_CASH_AFTER_FILL_ATOMS
    assert closure["final_position_base_atoms"] == _EXPECTED_BASE_AFTER_FILL_ATOMS


def test_task2_live_paper_registry_is_independent_closed_and_resource_locked() -> None:
    """Breaks if Task 2 schemas drift into crypto_replay or generated bytes stop being canonical."""

    from build_finance.crypto_replay.schema_definitions import CONTRACT_SPECS_BY_SCHEMA as REPLAY_SPECS
    from build_finance.live_paper import contracts, registry

    assert contracts.LIVE_PAPER_SCHEMA_IDS == _TASK2_SCHEMA_IDS
    assert dict(contracts.SELF_ID_FIELDS) == _TASK2_SELF_ID_FIELDS
    assert not set(_TASK2_SCHEMA_IDS).intersection(REPLAY_SPECS)

    expected_resources = registry.build_expected_resources()
    assert tuple(expected_resources) == (
        "schemas/algorithm-candidate-v1.schema.json",
        "schemas/fusion-decision-v1.schema.json",
        "schemas/normalization-receipt-v1.schema.json",
        "schemas/decision-group-manifest-v1.schema.json",
        "schema-bundle.json",
        "schema-bundle.sha256",
        "schema-lock.json",
    )

    bundle = _parse_canonical_record(expected_resources["schema-bundle.json"])
    assert bundle["schema"] == "build-finance.live-paper.schema-bundle/v1"
    assert bundle["jcs_profile"] == "RFC8785_INTEGER_AUTHORITY_V1"
    assert bundle["aliases"] == [
        {
            "name": "ContentID",
            "json_type": "string",
            "pattern": "^[0-9a-f]{64}$",
            "minimum": None,
            "maximum": None,
            "scale": None,
        },
        {
            "name": "u64s",
            "json_type": "string",
            "pattern": "^(0|[1-9][0-9]*)$",
            "minimum": "0",
            "maximum": "18446744073709551615",
            "scale": None,
        },
        {
            "name": "sq18s",
            "json_type": "string",
            "pattern": "^(0|-?[1-9][0-9]*)$",
            "minimum": "-170141183460469231731687303715884105728",
            "maximum": "170141183460469231731687303715884105727",
            "scale": "1000000000000000000",
        },
        {
            "name": "uq18s",
            "json_type": "string",
            "pattern": "^(0|[1-9][0-9]*)$",
            "minimum": "0",
            "maximum": "1000000000000000000",
            "scale": "1000000000000000000",
        },
    ]
    assert [row["contract_schema"] for row in bundle["schemas"]] == sorted(
        _TASK2_SCHEMA_IDS,
        key=lambda value: value.encode("utf-8"),
    )
    for row in bundle["schemas"]:
        schema_id = str(row["contract_schema"])
        assert row["family"] == "LIVE_PAPER"
        assert row["self_id_field"] == _TASK2_SELF_ID_FIELDS[schema_id]
        schema_record = expected_resources[f"schemas/{schema_id.removeprefix('trading.').replace('/', '-')}.schema.json"]
        schema = _parse_canonical_record(schema_record)
        assert schema["x-contract-schema"] == schema_id
        assert hashlib.sha256(schema_record[:-1]).hexdigest() == row["schema_sha256"]
        _assert_schema_recursively_closed(schema)

    assert expected_resources["schema-bundle.sha256"] == hashlib.sha256(
        expected_resources["schema-bundle.json"][:-1]
    ).hexdigest().encode("ascii") + b"\n"
    lock = _parse_canonical_record(expected_resources["schema-lock.json"])
    assert lock == {
        "schema": "build-finance.live-paper.schema-lock/v1",
        "contract_count": 4,
        "generated_json_schema_count": 4,
        "schema_bundle_sha256": hashlib.sha256(expected_resources["schema-bundle.json"][:-1]).hexdigest(),
    }
    registry.check_resources()


def test_task2_content_ids_resolve_only_live_paper_self_fields_and_reject_float_authority() -> None:
    """Breaks if the sealer hashes the wrong field, mutates input, accepts floats, or uses replay specs."""

    from build_finance.live_paper.content_ids import compute_content_id, seal_content_id, verify_content_id

    normalization_body = _normalization_body()
    normalization_body.pop("normalization_receipt_id")
    original_body = copy.deepcopy(normalization_body)
    sealed = seal_content_id(normalization_body)

    assert normalization_body == original_body
    assert sealed is not normalization_body
    assert set(sealed) == {*original_body, "normalization_receipt_id"}
    supplied_id = str(sealed["normalization_receipt_id"])
    assert _HEX64.fullmatch(supplied_id)
    assert compute_content_id(sealed) == supplied_id
    assert compute_content_id({**sealed, "normalization_receipt_id": "0" * 64}) == supplied_id
    assert verify_content_id(sealed)
    assert not verify_content_id(original_body)

    changed_payload = {**sealed, "output_merkle_root_sha256": _DIGEST_7}
    assert compute_content_id(changed_payload) != supplied_id

    with pytest.raises(ValueError, match="normalization_receipt_id"):
        seal_content_id({**sealed, "normalization_receipt_id": "0" * 64})

    with pytest.raises(TypeError, match="floating-point"):
        compute_content_id({**original_body, "input_count": 2.0})

    with pytest.raises(KeyError, match="unknown live-paper"):
        compute_content_id({"schema": "trading.raw-event/v1"})


def test_task2_contract_validation_enforces_authority_bounds_and_closed_membership() -> None:
    """Breaks if validation is only shape-checking and misses authority semantics."""

    from build_finance.live_paper.content_ids import seal_content_id
    from build_finance.live_paper.registry import validate_contract

    normalization, algorithm, fusion, manifest = _sealed_task2_vector()
    for document in (normalization, algorithm, fusion, manifest):
        assert validate_contract(document, expected_schema=str(document["schema"])) == ()

    stale_id = {**algorithm, "algorithm_candidate_id": "0" * 64}
    stale_id_issues = validate_contract(stale_id, expected_schema="trading.algorithm-candidate/v1")
    assert "content_id" in {issue.code for issue in stale_id_issues}

    oversized_confidence = dict(algorithm)
    oversized_confidence.pop("algorithm_candidate_id")
    oversized_confidence["confidence_q18"] = "1000000000000000001"
    oversized_confidence = seal_content_id(oversized_confidence)
    oversized_issues = validate_contract(oversized_confidence, expected_schema="trading.algorithm-candidate/v1")
    assert "fixed_point_range" in {issue.code for issue in oversized_issues}

    order_shaped_algorithm = dict(algorithm)
    order_shaped_algorithm.pop("algorithm_candidate_id")
    order_shaped_algorithm["quantity_base_atoms"] = "100"
    order_shaped_algorithm = seal_content_id(order_shaped_algorithm)
    order_shape_issues = validate_contract(order_shaped_algorithm, expected_schema="trading.algorithm-candidate/v1")
    assert "additionalProperties" in {issue.code for issue in order_shape_issues}

    count_mismatch = dict(normalization)
    count_mismatch.pop("normalization_receipt_id")
    count_mismatch["input_count"] = "1"
    count_mismatch = seal_content_id(count_mismatch)
    count_issues = validate_contract(count_mismatch, expected_schema="trading.normalization-receipt/v1")
    assert "normalization_total_evidence" in {issue.code for issue in count_issues}

    non_abstain_score = dict(fusion)
    non_abstain_score.pop("fusion_decision_id")
    non_abstain_score["model_score_q18"] = "1"
    non_abstain_score = seal_content_id(non_abstain_score)
    abstain_issues = validate_contract(non_abstain_score, expected_schema="trading.fusion-decision/v1")
    assert "disabled_model_abstain" in {issue.code for issue in abstain_issues}

    order_shaped_fusion = dict(fusion)
    order_shaped_fusion.pop("fusion_decision_id")
    order_shaped_fusion["venue_order_id"] = "paper-order-001"
    order_shaped_fusion = seal_content_id(order_shaped_fusion)
    fusion_shape_issues = validate_contract(order_shaped_fusion, expected_schema="trading.fusion-decision/v1")
    assert "additionalProperties" in {issue.code for issue in fusion_shape_issues}

    missing_member = dict(manifest)
    missing_member.pop("decision_group_manifest_id")
    missing_member["member_content_ids"] = missing_member["member_content_ids"][:-1]
    missing_member["member_count"] = "2"
    missing_member = seal_content_id(missing_member)
    member_issues = validate_contract(missing_member, expected_schema="trading.decision-group-manifest/v1")
    assert "decision_group_membership" in {issue.code for issue in member_issues}


def test_task2_u64s_boundaries_are_enforced_after_ref_resolution_for_every_declared_field() -> None:
    """Breaks if u64s validation checks only the digit pattern and ignores the declared maximum."""

    from build_finance.live_paper.content_ids import seal_content_id
    from build_finance.live_paper.registry import validate_contract

    for schema_id, field in _TASK2_U64_FIELDS:
        max_issues = _task2_structural_issues_for_field(schema_id, field, _U64_MAX_TEXT)
        overflow_issues = _task2_structural_issues_for_field(schema_id, field, _U64_OVERFLOW_TEXT)

        assert max_issues == (), f"{schema_id}.{field} rejected hand-derived 2**64 - 1"
        assert "maximum" in {issue.code for issue in overflow_issues}, (
            f"{schema_id}.{field} accepted hand-derived 2**64"
        )

    normalization, algorithm, _fusion, _manifest = _sealed_task2_vector()
    assert _task2_structural_issues_for_field(
        "trading.normalization-receipt/v1",
        "input_count",
        normalization["input_count"],
    ) == ()

    max_algorithm = dict(algorithm)
    max_algorithm.pop("algorithm_candidate_id")
    max_algorithm["decision_sequence"] = _U64_MAX_TEXT
    max_algorithm = seal_content_id(max_algorithm)
    assert validate_contract(max_algorithm, expected_schema="trading.algorithm-candidate/v1") == ()

    overflow_algorithm = dict(algorithm)
    overflow_algorithm.pop("algorithm_candidate_id")
    overflow_algorithm["decision_sequence"] = _U64_OVERFLOW_TEXT
    overflow_algorithm = seal_content_id(overflow_algorithm)
    overflow_algorithm_issues = validate_contract(
        overflow_algorithm,
        expected_schema="trading.algorithm-candidate/v1",
    )
    assert "maximum" in {issue.code for issue in overflow_algorithm_issues}


def test_vertical_contract_assertions_reject_placeholders() -> None:
    """The contract helper must fail empty or top-level-only placeholders."""

    with pytest.raises(AssertionError, match="mode"):
        _assert_complete_vertical_contract({})

    placeholder = {
        "schema": "build-finance.live-paper.g2-run-result/v1",
        "evidence_classification": "SYNTHETIC_CONTRACT_VECTOR",
        "mode": {
            "data_boundary": "OFFLINE_VERIFIED_INPUT",
            "execution": "PAPER_ONLY",
            "model": "DISABLED_ABSTAIN",
            "positioning": "LONG_OR_FLAT_SPOT",
            "runtime_dependencies": "PYTHON_STDLIB_ONLY",
        },
        "capability_receipt": {
            "external_io_attempts": [],
            "provider_sdk_touches": [],
            "credential_lookups": [],
            "broker_wallet_or_signer_touches": [],
            "venue_order_touches": [],
        },
    }
    with pytest.raises(AssertionError, match="normalized_events"):
        _assert_complete_vertical_contract(placeholder)


def _complete_placeholder_result(content_id: str = "0" * 64) -> dict[str, Any]:
    return {
        "schema": "build-finance.live-paper.g2-run-result/v1",
        "evidence_classification": "SYNTHETIC_CONTRACT_VECTOR",
        "mode": {
            "data_boundary": "OFFLINE_VERIFIED_INPUT",
            "execution": "PAPER_ONLY",
            "model": "DISABLED_ABSTAIN",
            "positioning": "LONG_OR_FLAT_SPOT",
            "runtime_dependencies": "PYTHON_STDLIB_ONLY",
        },
        "capability_receipt": {
            "external_io_attempts": [],
            "provider_sdk_touches": [],
            "credential_lookups": [],
            "broker_wallet_or_signer_touches": [],
            "venue_order_touches": [],
        },
        "normalized_events": [
            {
                "event_id": content_id,
                "source_event_id": "synthetic-event-0001",
                "price_q18": _EXPECTED_PRICE_Q18,
                "replay_clock_ns": "1000000000",
            },
            {
                "event_id": content_id,
                "source_event_id": "synthetic-event-0002",
                "price_q18": _EXPECTED_PRICE_Q18,
                "replay_clock_ns": "2000000000",
            },
        ],
        "feature_snapshot": {
            "schema": "build-finance.live-paper.feature-snapshot/v1",
            "feature_snapshot_id": content_id,
            "as_of_event_id": content_id,
            "market_id": "SYNTH_BASE_SYNTH_QUOTE_SPOT",
            "close_price_q18": _EXPECTED_PRICE_Q18,
            "position_base_atoms_before": "0",
            "cash_quote_atoms_before": _EXPECTED_INITIAL_QUOTE_ATOMS,
        },
        "algorithm_evidence": {
            "schema": "build-finance.live-paper.algorithm-evidence/v1",
            "algorithm_evidence_id": content_id,
            "feature_snapshot_id": content_id,
            "rule_id": "G2_SYNTHETIC_LONG_OR_FLAT_OPEN_IF_FLAT",
            "candidate_action": "OPEN_LONG",
        },
        "model_signal": {
            "schema": "build-finance.live-paper.model-signal/v1",
            "model_signal_id": content_id,
            "feature_snapshot_id": content_id,
            "mode": "DISABLED",
            "action": "ABSTAIN",
            "can_size": False,
            "can_execute": False,
        },
        "fusion_decision": {
            "schema": "build-finance.live-paper.fusion-decision/v1",
            "fusion_decision_id": content_id,
            "model_signal_id": content_id,
            "algorithm_evidence_id": content_id,
            "model_action": "ABSTAIN",
            "candidate_action": "OPEN_LONG",
            "can_size": False,
            "can_execute": False,
        },
        "risk_decision": {
            "schema": "build-finance.live-paper.risk-decision/v1",
            "risk_decision_id": content_id,
            "fusion_decision_id": content_id,
            "authority": "DETERMINISTIC_RISK_ONLY",
            "decision": "MINT_PAPER_INTENT",
            "reference_price_q18": _EXPECTED_PRICE_Q18,
            "quantity_base_atoms": _EXPECTED_ORDER_BASE_ATOMS,
            "max_notional_quote_atoms": _EXPECTED_GROSS_QUOTE_ATOMS,
            "reserved_quote_atoms": _EXPECTED_RESERVED_QUOTE_ATOMS,
            "model_sized": False,
        },
        "paper_intents": [
            {
                "schema": "build-finance.live-paper.paper-intent/v1",
                "intent_id": content_id,
                "risk_decision_id": content_id,
                "created_by": "DETERMINISTIC_RISK_ONLY",
                "action": "OPEN_LONG",
                "paper_only": True,
                "quantity_base_atoms": _EXPECTED_ORDER_BASE_ATOMS,
                "reserved_quote_atoms": _EXPECTED_RESERVED_QUOTE_ATOMS,
                "broker_order_id": None,
                "wallet_signature": None,
            }
        ],
        "paper_fills": [
            {
                "schema": "build-finance.live-paper.paper-fill/v1",
                "fill_id": content_id,
                "intent_id": content_id,
                "decision_event_id": content_id,
                "fill_event_id": content_id,
                "fill_policy": "STRICT_NEXT_EVENT",
                "filled_base_atoms": _EXPECTED_ORDER_BASE_ATOMS,
                "gross_quote_atoms": _EXPECTED_GROSS_QUOTE_ATOMS,
                "simulation_fee_quote_atoms": _EXPECTED_FEE_QUOTE_ATOMS,
                "cash_delta_quote_atoms": f"-{_EXPECTED_RESERVED_QUOTE_ATOMS}",
            }
        ],
        "ledger": {
            "schema": "build-finance.live-paper.ledger-record/v1",
            "ledger_record_id": content_id,
            "fill_id": content_id,
            "cash_quote_atoms_after": _EXPECTED_CASH_AFTER_FILL_ATOMS,
            "position_base_atoms_after": _EXPECTED_BASE_AFTER_FILL_ATOMS,
            "postings": [
                {
                    "account": "CASH_AVAILABLE",
                    "asset_mint": "SYNTH_QUOTE_MINT",
                    "delta_atoms": f"-{_EXPECTED_RESERVED_QUOTE_ATOMS}",
                },
                {
                    "account": "POSITION_AVAILABLE",
                    "asset_mint": "SYNTH_BASE_MINT",
                    "delta_atoms": _EXPECTED_ORDER_BASE_ATOMS,
                },
                {
                    "account": "FEES_PAID",
                    "asset_mint": "SYNTH_QUOTE_MINT",
                    "delta_atoms": _EXPECTED_FEE_QUOTE_ATOMS,
                },
            ],
        },
        "reconciliation": {
            "schema": "build-finance.live-paper.reconciliation/v1",
            "reconciliation_id": content_id,
            "ledger_record_id": content_id,
            "status": "PASS",
            "cash_residual_quote_atoms": "0",
            "position_residual_base_atoms": "0",
            "unmatched_paper_intent_count": 0,
            "external_position_source": None,
        },
        "closure": {
            "schema": "build-finance.live-paper.run-closure/v1",
            "closure_id": content_id,
            "reconciliation_id": content_id,
            "status": "CLOSED",
            "final_position_state": "LONG",
            "open_paper_intent_count": 0,
            "open_venue_order_count": 0,
            "final_cash_quote_atoms": _EXPECTED_CASH_AFTER_FILL_ATOMS,
            "final_position_base_atoms": _EXPECTED_BASE_AFTER_FILL_ATOMS,
        },
    }


def test_vertical_contract_assertions_reject_consistent_placeholder_content_ids() -> None:
    """Consistent 64-hex placeholders must not satisfy self-addressed authority IDs."""

    with pytest.raises(AssertionError, match="content id"):
        _assert_complete_vertical_contract(_complete_placeholder_result())


def test_reviewed_g2_run_reaches_reconciled_offline_kernel_closure() -> None:
    """The rooted G2 vector must use the reviewed verified-input kernel boundary."""

    from build_finance.crypto_replay.canonical import canonical_record_bytes
    from build_finance.live_paper.kernel import run_offline_paper_kernel
    from build_finance.live_paper.profiles import PaperKernelProfiles
    from build_finance.live_paper.run_input_builder import build_verified_run_inputs
    from tests.live_paper.support.g2_vectors import build_g2_vector

    vector = build_g2_vector()
    profiles = PaperKernelProfiles(
        **{
            **vector.profile_records,
            "risk_config_record": canonical_record_bytes(vector.run_input_bundle.replay_risk_config),
        }
    )  # type: ignore[arg-type]
    verified = build_verified_run_inputs(
        vector.run_receipt_record,
        vector.admitted_candidates,
        vector.source_receipt_records,
        vector.resolver,
        profiles,
    )
    result = run_offline_paper_kernel(verified, vector.resolver, profiles)

    assert result.closure.status == "CLOSED"
    assert result.closure.reason_codes == ()
    assert len(result.feature_snapshot_records) == 2
    assert len(result.risk_decision_records) == 2
    assert len(result.reconciliation_receipt_records) == 1
