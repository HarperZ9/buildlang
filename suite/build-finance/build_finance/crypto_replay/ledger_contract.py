"""Pure Task-9 ledger contract formulas over already-retained evidence.

This module classifies causal digests and evaluates immutable ledger shapes. It
has no append, storage, genesis-construction, replay, clock, or execution API.
"""

from __future__ import annotations

import re
import unicodedata
from collections.abc import Mapping
from typing import Any, cast

from build_finance.crypto_replay.canonical import JsonValue, canonical_json_bytes, canonical_record_bytes, sha256_hex
from build_finance.crypto_replay.content_ids import compute_content_id, verify_content_id
from build_finance.crypto_replay.contract_semantics import (
    _LEDGER_OBJECT_SCHEMA_BY_TYPE as LEDGER_OBJECT_SCHEMA_BY_TYPE,
)
from build_finance.crypto_replay.formats import parse_bounded_decimal_string

_MAX_U64 = 18_446_744_073_709_551_615
_MAX_U64_TEXT = str(_MAX_U64)
_DIGEST = re.compile(r"[0-9a-f]{64}\Z")
_EXECUTION_PHASES = frozenset(
    {
        "INITIAL_PREFIX",
        "RAW_GROUP_COMMIT",
        "FILL_OR_EXPIRY",
        "GROUP_MARK",
        "SNAPSHOT",
        "RISK_AND_STATE_EFFECT",
        "GROUP_GATE",
        "FINAL_GATE",
        "TERMINAL_KILL_BATCH",
        "RUN_END",
    }
)
_ARITHMETIC_OPERATIONS = frozenset({"GROUP_MARK_VALUE", "SUMMARY_EQUITY", "LEDGER_POSTING", "RESIDUAL_SERIALIZATION"})
_TYPED_KEY_FIELDS = {
    "trading.group-mark-key/v1": frozenset(
        {"schema", "run_receipt_id", "equal_time_group", "as_of_ingest_sequence", "portfolio_state_before_id"}
    ),
    "trading.risk-idempotency-key/v1": frozenset({"schema", "run_receipt_id", "equal_time_group", "market_id"}),
    "trading.fill-idempotency-key/v1": frozenset({"schema", "intent_id"}),
    "trading.model-validation-attempt-key/v1": frozenset(
        {
            "schema",
            "model_signal_manifest_sha256",
            "relative_path",
            "raw_signal_sha256",
            "feature_snapshot_id",
            "decision_sequence",
            "requested_producer_scope_key_sha256",
            "horizon_ns",
        }
    ),
    "trading.execution-transition-key/v1": frozenset(
        {"schema", "run_receipt_id", "equal_time_group", "phase", "market_id", "item_sequence"}
    ),
    "trading.reconciliation-arithmetic-range-key/v1": frozenset(
        {
            "schema",
            "run_receipt_id",
            "portfolio_state_before_id",
            "ingest_sequence",
            "equal_time_group",
            "replay_clock_ns",
            "arithmetic_operation",
            "arithmetic_operands_sha256",
        }
    ),
}


def _is_digest(value: object) -> bool:
    return isinstance(value, str) and _DIGEST.fullmatch(value) is not None


def _is_u64(value: object, *, positive: bool = False) -> bool:
    if not isinstance(value, str) or not value or len(value) > len(_MAX_U64_TEXT):
        return False
    if value != "0" and (value[0] not in "123456789" or not value.isascii() or not value.isdigit()):
        return False
    if len(value) == len(_MAX_U64_TEXT) and value > _MAX_U64_TEXT:
        return False
    try:
        parse_bounded_decimal_string(value, minimum=1 if positive else 0, maximum=_MAX_U64)
    except ValueError:
        return False
    return True


def _is_nonempty(value: object) -> bool:
    return isinstance(value, str) and bool(value)


def _is_bounded_registry(value: object) -> bool:
    if not _is_nonempty(value):
        return False
    assert isinstance(value, str)
    try:
        return len(value.encode("utf-8", errors="strict")) <= 128
    except UnicodeEncodeError:
        return False


def _is_safe_relative_path(value: object) -> bool:
    if not isinstance(value, str) or not value or unicodedata.normalize("NFC", value) != value:
        return False
    if value.startswith("/") or "\\" in value or ":" in value or "\x00" in value or value.endswith("/"):
        return False
    return all(segment not in ("", ".", "..") for segment in value.split("/"))


def _typed_key_is_closed(value: object) -> bool:
    if not isinstance(value, Mapping):
        return False
    schema = value.get("schema")
    if not isinstance(schema, str) or schema not in _TYPED_KEY_FIELDS or set(value) != _TYPED_KEY_FIELDS[schema]:
        return False
    if schema == "trading.group-mark-key/v1":
        return (
            _is_digest(value["run_receipt_id"])
            and _is_u64(value["equal_time_group"], positive=True)
            and _is_u64(value["as_of_ingest_sequence"])
            and _is_digest(value["portfolio_state_before_id"])
        )
    if schema == "trading.risk-idempotency-key/v1":
        return (
            _is_digest(value["run_receipt_id"])
            and _is_u64(value["equal_time_group"], positive=True)
            and _is_bounded_registry(value["market_id"])
        )
    if schema == "trading.fill-idempotency-key/v1":
        return _is_digest(value["intent_id"])
    if schema == "trading.model-validation-attempt-key/v1":
        return (
            _is_digest(value["model_signal_manifest_sha256"])
            and _is_safe_relative_path(value["relative_path"])
            and _is_digest(value["raw_signal_sha256"])
            and _is_digest(value["feature_snapshot_id"])
            and _is_u64(value["decision_sequence"], positive=True)
            and _is_digest(value["requested_producer_scope_key_sha256"])
            and _is_u64(value["horizon_ns"])
        )
    if schema == "trading.execution-transition-key/v1":
        phase = value["phase"]
        market_id = value["market_id"]
        return (
            _is_digest(value["run_receipt_id"])
            and _is_u64(value["equal_time_group"])
            and isinstance(phase, str)
            and phase in _EXECUTION_PHASES
            and (market_id is None or _is_bounded_registry(market_id))
            and (value["item_sequence"] is None or _is_u64(value["item_sequence"]))
        )
    arithmetic_operation = value["arithmetic_operation"]
    return (
        _is_digest(value["run_receipt_id"])
        and _is_digest(value["portfolio_state_before_id"])
        and _is_u64(value["ingest_sequence"])
        and _is_u64(value["equal_time_group"])
        and _is_u64(value["replay_clock_ns"])
        and isinstance(arithmetic_operation, str)
        and arithmetic_operation in _ARITHMETIC_OPERATIONS
        and _is_digest(value["arithmetic_operands_sha256"])
    )


def _classify_causal_digest(
    *,
    digest: object,
    resolver: object,
    typed_key_preimages: object,
) -> dict[str, object]:
    if not _is_digest(digest):
        return {"accepted": False, "issue_codes": ("CAUSAL_DIGEST_MALFORMED",)}
    assert isinstance(digest, str)
    matches: list[str] = []
    resolution_mismatch = False
    try:
        resolved = resolver.resolve_object(digest)  # type: ignore[attr-defined]
    except (AttributeError, KeyError, TypeError, ValueError):
        resolved = None
    if resolved is not None:
        document = getattr(resolved, "document", None)
        record = getattr(resolved, "record", None)
        if isinstance(document, Mapping) and isinstance(record, bytes):
            canonical_document = cast(dict[str, JsonValue], dict(document))
            try:
                valid_content = (
                    verify_content_id(canonical_document)
                    and compute_content_id(canonical_document) == digest
                    and canonical_record_bytes(canonical_document) == record
                    and canonical_document.get("schema") is not None
                )
            except (KeyError, TypeError, ValueError):
                valid_content = False
            if valid_content:
                matches.append("CONTENT_ID")
            else:
                resolution_mismatch = True
        else:
            resolution_mismatch = True
    try:
        retained = resolver.resolve_bytes(digest)  # type: ignore[attr-defined]
    except (AttributeError, KeyError, TypeError, ValueError):
        retained = None
    if isinstance(retained, bytes) and sha256_hex(retained) == digest:
        matches.append("RETAINED_BYTES")
    elif retained is not None:
        resolution_mismatch = True

    preimage_mismatch = False
    if isinstance(typed_key_preimages, Mapping):
        for mapped_digest, preimage in typed_key_preimages.items():
            computed: str | None = None
            if _typed_key_is_closed(preimage):
                try:
                    computed = sha256_hex(canonical_json_bytes(preimage))
                except (TypeError, ValueError):
                    computed = None
            if mapped_digest == digest or computed == digest:
                if mapped_digest == digest and computed == digest:
                    matches.append("TYPED_KEY")
                else:
                    preimage_mismatch = True
    if preimage_mismatch:
        return {"accepted": False, "issue_codes": ("CAUSAL_DIGEST_PREIMAGE_MISMATCH",)}
    if resolution_mismatch:
        return {"accepted": False, "issue_codes": ("CAUSAL_DIGEST_RESOLUTION_MISMATCH",)}
    if len(matches) > 1:
        return {"accepted": False, "issue_codes": ("CAUSAL_DIGEST_AMBIGUOUS",)}
    if len(matches) == 1:
        return {"accepted": True, "issue_codes": (), "kind": matches[0]}
    return {"accepted": False, "issue_codes": ("CAUSAL_DIGEST_UNRESOLVED",)}


def _derive_record_contract(*, record_type: object, context: object) -> dict[str, object]:
    if not isinstance(record_type, str) or record_type not in LEDGER_OBJECT_SCHEMA_BY_TYPE:
        return {"accepted": False, "issue_codes": ("LEDGER_RECORD_TYPE_UNKNOWN",)}
    if not isinstance(context, Mapping):
        return {"accepted": False, "issue_codes": ("LEDGER_CONTEXT_INVALID",)}
    causes: list[str] = []
    invalid = False

    def required(field: str) -> None:
        nonlocal invalid
        value = context.get(field)
        if not _is_digest(value):
            invalid = True
        else:
            assert isinstance(value, str)
            causes.append(value)

    def optional(field: str) -> None:
        nonlocal invalid
        value = context.get(field)
        if value is None:
            return
        if not _is_digest(value):
            invalid = True
        else:
            assert isinstance(value, str)
            causes.append(value)

    def digest_list(field: str) -> None:
        nonlocal invalid
        value = context.get(field)
        if not isinstance(value, list) or any(not _is_digest(item) for item in value):
            invalid = True
            return
        assert all(isinstance(item, str) for item in value)
        if len(value) != len(set(value)):
            invalid = True
            return
        causes.extend(value)

    if record_type == "SOURCE_ADMISSION":
        required("fixture_manifest_sha256")
    elif record_type == "CONFIG_ADMISSION":
        optional("validated_config_sha256")
    elif record_type == "RUN_RECEIPT":
        required("fixture_manifest_sha256")
        required("config_admission_receipt_id")
        required("run_closure_receipt_id")
        digest_list("source_admission_receipt_ids")
        optional("validated_config_sha256")
        optional("model_registry_sha256")
        optional("model_signal_manifest_sha256")
    elif record_type == "RAW_ADMISSION":
        required("source_admission_receipt_id")
    elif record_type == "FEATURE_SNAPSHOT":
        digest_list("causal_raw_event_ids")
    elif record_type == "MODEL_VALIDATION":
        required("feature_snapshot_id")
        required("model_registry_sha256")
        required("model_signal_manifest_sha256")
    elif record_type == "MODEL_SIGNAL_ACCEPTED":
        required("model_validation_receipt_id")
    elif record_type == "RISK_DECISION":
        required("feature_snapshot_id")
        required("portfolio_state_before_id")
        required("config_admission_receipt_id")
        optional("validated_config_sha256")
        optional("model_validation_receipt_id")
        optional("model_signal_id")
    elif record_type == "SIMULATED_INTENT":
        required("risk_decision_id")
        required("portfolio_state_before_id")
    elif record_type == "SIMULATED_FILL":
        required("intent_id")
        required("portfolio_state_before_id")
        optional("fill_event_id")
        reason_values = context.get("fill_reason_codes")
        if not isinstance(reason_values, list) or any(not isinstance(value, str) for value in reason_values):
            invalid = True
        else:
            if len(reason_values) != len(set(reason_values)):
                invalid = True
            latching = context.get("enclosing_latching_risk_decision_id")
            requires_latching = "FILL_KILL_LATCHED" in reason_values
            if requires_latching and latching is None:
                return {"accepted": False, "issue_codes": ("LEDGER_LATCHING_RISK_CAUSE_REQUIRED",)}
            if not requires_latching and latching is not None:
                return {"accepted": False, "issue_codes": ("LEDGER_LATCHING_RISK_CAUSE_FORBIDDEN",)}
            if requires_latching:
                required("enclosing_latching_risk_decision_id")
    elif record_type in ("PORTFOLIO_STATE", "KILL_STATE"):
        optional("previous_portfolio_state_id")
        required("causation_id")
    elif record_type == "RECONCILIATION":
        required("portfolio_state_before_id")
        required("portfolio_state_after_id")
        digest_list("reconciliation_causation_ids")
    elif record_type == "RUN_END":
        required("final_reconciliation_receipt_id")

    if invalid:
        return {"accepted": False, "issue_codes": ("LEDGER_CONTEXT_INVALID",)}
    return {
        "accepted": True,
        "issue_codes": (),
        "object_schema": LEDGER_OBJECT_SCHEMA_BY_TYPE[record_type],
        "causation_ids": tuple(sorted(set(causes))),
    }


def _validate_initial_prefix(*, rows: object) -> dict[str, object]:
    if not isinstance(rows, list) or len(rows) < 5:
        return {"accepted": False, "issue_codes": ("LEDGER_INITIAL_PREFIX_SHAPE",)}
    normalized: list[tuple[str, str, object]] = []
    for row in rows:
        if not isinstance(row, Mapping) or set(row) != {"record_type", "object_id", "admission_sequence"}:
            return {"accepted": False, "issue_codes": ("LEDGER_INITIAL_PREFIX_ROW",)}
        record_type = row.get("record_type")
        object_id = row.get("object_id")
        if not isinstance(record_type, str) or not _is_digest(object_id):
            return {"accepted": False, "issue_codes": ("LEDGER_INITIAL_PREFIX_ROW",)}
        assert isinstance(object_id, str)
        normalized.append((record_type, object_id, row.get("admission_sequence")))
    object_ids = [row[1] for row in normalized]
    if len(object_ids) != len(set(object_ids)):
        return {"accepted": False, "issue_codes": ("LEDGER_INITIAL_PREFIX_DUPLICATE",)}

    admissions = normalized[:-3]
    suffix = normalized[-3:]
    if [row[0] for row in suffix] != ["RUN_RECEIPT", "PORTFOLIO_STATE", "RECONCILIATION"]:
        return {"accepted": False, "issue_codes": ("LEDGER_INITIAL_PREFIX_SUFFIX",)}
    if any(row[2] is not None for row in suffix):
        return {"accepted": False, "issue_codes": ("LEDGER_INITIAL_PREFIX_SUFFIX",)}
    admission_types = [row[0] for row in admissions]
    if (
        not admissions
        or admission_types.count("SOURCE_ADMISSION") < 1
        or admission_types.count("CONFIG_ADMISSION") != 1
        or any(record_type not in ("SOURCE_ADMISSION", "CONFIG_ADMISSION") for record_type in admission_types)
    ):
        return {"accepted": False, "issue_codes": ("LEDGER_INITIAL_PREFIX_ADMISSIONS",)}
    sequences: list[int] = []
    for _, _, sequence in admissions:
        if not _is_u64(sequence, positive=True):
            return {"accepted": False, "issue_codes": ("LEDGER_INITIAL_PREFIX_SEQUENCE",)}
        assert isinstance(sequence, str)
        sequences.append(int(sequence))
    if sequences != list(range(1, len(sequences) + 1)):
        return {"accepted": False, "issue_codes": ("LEDGER_INITIAL_PREFIX_SEQUENCE",)}
    return {"accepted": True, "issue_codes": ()}


def _evaluate_t02_contract(*, operation: object, **inputs: Any) -> Mapping[str, Any]:
    """Evaluate one closed pure contract operation and return a total result mapping."""
    if operation == "classify_causal_digest":
        return _classify_causal_digest(
            digest=inputs.get("digest"),
            resolver=inputs.get("resolver"),
            typed_key_preimages=inputs.get("typed_key_preimages"),
        )
    if operation == "derive_record_contract":
        return _derive_record_contract(
            record_type=inputs.get("record_type"),
            context=inputs.get("context"),
        )
    if operation == "validate_initial_prefix":
        return _validate_initial_prefix(rows=inputs.get("rows"))
    return {"accepted": False, "issue_codes": ("LEDGER_OPERATION_UNKNOWN",)}
