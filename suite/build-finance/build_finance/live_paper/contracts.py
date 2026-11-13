"""Independent G2 live-paper contract definitions."""

from __future__ import annotations

from collections.abc import Mapping
from typing import Any

JsonObject = dict[str, Any]

LIVE_PAPER_SCHEMA_IDS = (
    "trading.algorithm-candidate/v1",
    "trading.fusion-decision/v1",
    "trading.normalization-receipt/v1",
    "trading.decision-group-manifest/v1",
)
SELF_ID_FIELDS = {
    "trading.algorithm-candidate/v1": "algorithm_candidate_id",
    "trading.fusion-decision/v1": "fusion_decision_id",
    "trading.normalization-receipt/v1": "normalization_receipt_id",
    "trading.decision-group-manifest/v1": "decision_group_manifest_id",
}
SCHEMA_FILENAMES = {
    "trading.algorithm-candidate/v1": "algorithm-candidate-v1.schema.json",
    "trading.fusion-decision/v1": "fusion-decision-v1.schema.json",
    "trading.normalization-receipt/v1": "normalization-receipt-v1.schema.json",
    "trading.decision-group-manifest/v1": "decision-group-manifest-v1.schema.json",
}

_DRAFT_SCHEME = "https"
_DRAFT_2020_12 = _DRAFT_SCHEME + "://json-schema.org/draft/2020-12/schema"
_SHA256_PATTERN = "^[0-9a-f]{64}$"
_U64_PATTERN = "^(0|[1-9][0-9]*)$"
_I128_PATTERN = "^(0|-?[1-9][0-9]*)$"
_BOUNDED_NAME = "^[A-Za-z0-9][A-Za-z0-9_.:/-]{0,127}$"


def schema_filename(schema_id: str) -> str:
    """Return the deterministic generated filename for a live-paper schema."""
    try:
        return SCHEMA_FILENAMES[schema_id]
    except KeyError as error:
        raise ValueError(f"unknown live-paper schema id: {schema_id!r}") from error


def json_schema_id(schema_id: str) -> str:
    """Return the local absolute JSON Schema URN for a live-paper contract."""
    if not schema_id.startswith("trading.") or not schema_id.endswith("/v1"):
        raise ValueError(f"invalid live-paper trading contract schema tag: {schema_id!r}")
    contract_name, version = schema_id.removeprefix("trading.").split("/", maxsplit=1)
    return f"urn:build-finance:live-paper:contract:{contract_name}:{version}"


def _closed_object(properties: Mapping[str, JsonObject]) -> JsonObject:
    copied = dict(properties)
    return {
        "type": "object",
        "required": list(copied),
        "properties": copied,
        "additionalProperties": False,
    }


def _array(items: JsonObject, *, unique: bool = False, min_items: int | None = None) -> JsonObject:
    result: JsonObject = {"type": "array", "items": items}
    if unique:
        result["uniqueItems"] = True
    if min_items is not None:
        result["minItems"] = min_items
    return result


def _ref(name: str) -> JsonObject:
    return {"$ref": f"#/$defs/{name}"}


def _nullable_ref(name: str) -> JsonObject:
    return {"anyOf": [_ref(name), {"type": "null"}]}


def _shared_scalar_definitions() -> JsonObject:
    return {
        "ContentID": {"type": "string", "pattern": _SHA256_PATTERN},
        "sha256": {"type": "string", "pattern": _SHA256_PATTERN},
        "u64s": {"type": "string", "pattern": _U64_PATTERN},
        "sq18s": {"type": "string", "pattern": _I128_PATTERN},
        "uq18s": {"type": "string", "pattern": _U64_PATTERN},
        "bounded_name": {"type": "string", "pattern": _BOUNDED_NAME},
    }


def _schema(schema_id: str, properties: Mapping[str, JsonObject]) -> JsonObject:
    document = _closed_object(properties)
    document.update(
        {
            "$schema": _DRAFT_2020_12,
            "$id": json_schema_id(schema_id),
            "x-contract-schema": schema_id,
            "$defs": _shared_scalar_definitions(),
        }
    )
    return document


def normalization_receipt_schema() -> JsonObject:
    """Return the closed total-normalization receipt contract."""
    schema_id = "trading.normalization-receipt/v1"
    return _schema(
        schema_id,
        {
            "schema": {"const": schema_id},
            "normalization_receipt_id": _ref("ContentID"),
            "source_batch_id": _ref("bounded_name"),
            "normalization_code_sha256": _ref("sha256"),
            "input_content_ids": _array(_ref("ContentID"), unique=True, min_items=1),
            "input_count": _ref("u64s"),
            "normalized_event_ids": _array(_ref("ContentID"), unique=True, min_items=1),
            "normalized_event_count": _ref("u64s"),
            "output_merkle_root_sha256": _ref("sha256"),
            "status": {"enum": ["PASS", "REJECTED"]},
            "reason_codes": _array({"enum": ["INPUT_COUNT_MISMATCH", "EVENT_COUNT_MISMATCH", "INCOMPLETE_INPUT"]}),
            "total_evidence": {"const": "TOTAL_INPUT_CLOSURE"},
        },
    )


def algorithm_candidate_schema() -> JsonObject:
    """Return the closed algorithm-candidate contract with no sizing authority."""
    schema_id = "trading.algorithm-candidate/v1"
    return _schema(
        schema_id,
        {
            "schema": {"const": schema_id},
            "algorithm_candidate_id": _ref("ContentID"),
            "normalization_receipt_id": _ref("ContentID"),
            "feature_snapshot_id": _ref("ContentID"),
            "algorithm_id": _ref("bounded_name"),
            "algorithm_version": _ref("bounded_name"),
            "decision_group_key": _ref("bounded_name"),
            "decision_sequence": _ref("u64s"),
            "equal_time_group": _ref("u64s"),
            "input_content_ids": _array(_ref("ContentID"), unique=True, min_items=1),
            "candidate_action": {"enum": ["ABSTAIN", "OPEN_LONG", "CLOSE_LONG", "HOLD"]},
            "rationale_code": {"enum": ["OPEN_IF_FLAT", "CLOSE_IF_LONG", "NO_ACTION"]},
            "confidence_q18": _ref("uq18s"),
            "can_size": {"const": False},
            "can_execute": {"const": False},
        },
    )


def fusion_decision_schema() -> JsonObject:
    """Return the closed fusion contract with disabled-model ABSTAIN representation."""
    schema_id = "trading.fusion-decision/v1"
    return _schema(
        schema_id,
        {
            "schema": {"const": schema_id},
            "fusion_decision_id": _ref("ContentID"),
            "normalization_receipt_id": _ref("ContentID"),
            "algorithm_candidate_id": _ref("ContentID"),
            "model_signal_id": _nullable_ref("ContentID"),
            "decision_sequence": _ref("u64s"),
            "equal_time_group": _ref("u64s"),
            "input_content_ids": _array(_ref("ContentID"), unique=True, min_items=1),
            "model_mode": {"const": "DISABLED_ABSTAIN"},
            "model_action": {"const": "ABSTAIN"},
            "model_score_q18": _ref("sq18s"),
            "model_probability_abstain_q18": _ref("uq18s"),
            "candidate_action": {"enum": ["ABSTAIN", "OPEN_LONG", "CLOSE_LONG", "HOLD"]},
            "fused_action": {"enum": ["ABSTAIN", "OPEN_LONG", "CLOSE_LONG", "HOLD"]},
            "fusion_policy": {"const": "MODEL_ABSTAINS_USE_ALGORITHM_CANDIDATE"},
            "can_size": {"const": False},
            "can_execute": {"const": False},
        },
    )


def decision_group_manifest_schema() -> JsonObject:
    """Return the closed manifest binding a sealed decision group membership set."""
    schema_id = "trading.decision-group-manifest/v1"
    return _schema(
        schema_id,
        {
            "schema": {"const": schema_id},
            "decision_group_manifest_id": _ref("ContentID"),
            "decision_group_key": _ref("bounded_name"),
            "normalization_receipt_id": _ref("ContentID"),
            "algorithm_candidate_ids": _array(_ref("ContentID"), unique=True, min_items=1),
            "fusion_decision_ids": _array(_ref("ContentID"), unique=True, min_items=1),
            "member_content_ids": _array(_ref("ContentID"), unique=True, min_items=1),
            "member_count": _ref("u64s"),
            "sealed_by": {"const": "CONTENT_ID_REGISTRY_V1"},
        },
    )


def get_schema_documents() -> dict[str, JsonObject]:
    """Return fresh schema documents in registry inventory order."""
    return {
        "trading.algorithm-candidate/v1": algorithm_candidate_schema(),
        "trading.fusion-decision/v1": fusion_decision_schema(),
        "trading.normalization-receipt/v1": normalization_receipt_schema(),
        "trading.decision-group-manifest/v1": decision_group_manifest_schema(),
    }
