"""Frozen contract inventory and explicit in-repository schema definitions.

Task T01 adds primary schema dictionaries to ``PRIMARY_SCHEMA_DOCUMENTS``;
later T02 tasks populate the supporting and attachment dictionaries.  The
inventory is complete from the start so content-ID and code-generation scope
cannot drift while those definitions are added incrementally.
"""

from __future__ import annotations

from collections.abc import Mapping
from types import MappingProxyType

from build_finance.crypto_replay.canonical import JsonObject
from build_finance.crypto_replay.schema_model import ContractFamily, ContractSpec

_PRIMARY_SELF_ID_FIELDS = {
    "trading.raw-event/v1": "event_id",
    "trading.feature-snapshot/v1": "snapshot_id",
    "trading.model-signal/v1": "signal_id",
    "trading.risk-decision/v1": "risk_decision_id",
    "trading.simulated-order-intent/v1": "intent_id",
    "trading.simulated-fill-receipt/v1": "fill_receipt_id",
    "trading.portfolio-state/v1": "portfolio_state_id",
    "trading.ledger-record/v1": "ledger_record_id",
}
_SUPPORTING_SELF_ID_FIELDS = {
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

PRIMARY_SELF_ID_FIELDS = MappingProxyType(_PRIMARY_SELF_ID_FIELDS)
SUPPORTING_SELF_ID_FIELDS = MappingProxyType(_SUPPORTING_SELF_ID_FIELDS)
SELF_ID_FIELDS = MappingProxyType({**_PRIMARY_SELF_ID_FIELDS, **_SUPPORTING_SELF_ID_FIELDS})

PRIMARY_SCHEMA_IDS = tuple(_PRIMARY_SELF_ID_FIELDS)
SUPPORTING_SCHEMA_IDS = tuple(_SUPPORTING_SELF_ID_FIELDS)
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
ALL_JSON_SCHEMA_IDS = (*PRIMARY_SCHEMA_IDS, *SUPPORTING_SCHEMA_IDS, *ATTACHMENT_SCHEMA_IDS)


def schema_filename(schema_id: str) -> str:
    """Return the exact generated filename for a trading contract tag."""
    if not schema_id.startswith("trading.") or not schema_id.endswith("/v1"):
        raise ValueError(f"invalid trading contract schema tag: {schema_id!r}")
    return f"{schema_id.removeprefix('trading.').replace('/', '-')}.schema.json"


def json_schema_id(schema_id: str) -> str:
    """Return the local absolute JSON Schema URN for a contract tag."""
    if not schema_id.startswith("trading.") or not schema_id.endswith("/v1"):
        raise ValueError(f"invalid trading contract schema tag: {schema_id!r}")
    contract_name, version = schema_id.removeprefix("trading.").split("/", maxsplit=1)
    if not contract_name or not version:
        raise ValueError(f"invalid trading contract schema tag: {schema_id!r}")
    return f"urn:build-finance:contract:{contract_name}:{version}"


def _specs(
    schema_ids: tuple[str, ...],
    family: ContractFamily,
    self_id_fields: dict[str, str] | None = None,
) -> tuple[ContractSpec, ...]:
    return tuple(
        ContractSpec(
            schema_id=schema_id,
            self_id_field=None if self_id_fields is None else self_id_fields[schema_id],
            family=family,
            schema_filename=schema_filename(schema_id),
        )
        for schema_id in schema_ids
    )


PRIMARY_CONTRACT_SPECS = _specs(PRIMARY_SCHEMA_IDS, "PRIMARY", _PRIMARY_SELF_ID_FIELDS)
SUPPORTING_CONTRACT_SPECS = _specs(SUPPORTING_SCHEMA_IDS, "SUPPORTING", _SUPPORTING_SELF_ID_FIELDS)
ATTACHMENT_CONTRACT_SPECS = _specs(ATTACHMENT_SCHEMA_IDS, "ATTACHMENT")
CONTRACT_SPECS = (*PRIMARY_CONTRACT_SPECS, *SUPPORTING_CONTRACT_SPECS, *ATTACHMENT_CONTRACT_SPECS)
CONTRACT_SPECS_BY_SCHEMA = MappingProxyType({spec.schema_id: spec for spec in CONTRACT_SPECS})

_DRAFT_2020_12 = "https://json-schema.org/draft/2020-12/schema"
_SHA256_PATTERN = "^[0-9a-f]{64}$"
_U64_PATTERN = "^(0|[1-9][0-9]*)$"
_I128_PATTERN = "^(0|-?[1-9][0-9]*)$"
_RFC3339_NS_UTC_PATTERN = r"^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}\.[0-9]{9}Z$"
_RFC3339_FULL_DATE_PATTERN = r"^[0-9]{4}-[0-9]{2}-[0-9]{2}$"
_MAX_RUN_CLOSURE_PROOF_ROWS_V0 = 1_000_000
_SAFE_RELATIVE_PATH_PATTERN = (
    r"^(?![A-Za-z]:)(?!/)(?!\.{1,2}(?:/|$))(?!.*(?:/\.{1,2})(?:/|$))(?!.*//)[^/\\:\x00]+(?:/[^/\\:\x00]+)*$"
)


def _closed_object(properties: Mapping[str, JsonObject]) -> JsonObject:
    """Return one recursively closed object node with every property required."""
    copied = dict(properties)
    return {
        "type": "object",
        "required": list(copied),
        "properties": copied,
        "additionalProperties": False,
    }


def _array(items: JsonObject, *, unique: bool = False, maximum: int | None = None) -> JsonObject:
    schema: JsonObject = {"type": "array", "items": items}
    if unique:
        schema["uniqueItems"] = True
    if maximum is not None:
        schema["maxItems"] = maximum
    return schema


def _ref(name: str) -> JsonObject:
    return {"$ref": f"#/$defs/{name}"}


def _nullable_ref(name: str) -> JsonObject:
    return {"anyOf": [_ref(name), {"type": "null"}]}


def _shared_scalar_definitions() -> JsonObject:
    """Return the frozen scalar aliases embedded in every generated contract schema."""
    return {
        "sha256": {"type": "string", "pattern": _SHA256_PATTERN},
        "ContentID": {"type": "string", "pattern": _SHA256_PATTERN},
        "CausalDigest": {"type": "string", "pattern": _SHA256_PATTERN},
        "u64s": {"type": "string", "pattern": _U64_PATTERN},
        "uints": {"type": "string", "pattern": _U64_PATTERN},
        "i128s": {"type": "string", "pattern": _I128_PATTERN},
        "sq18s": {"type": "string", "pattern": _I128_PATTERN},
        "uq18s": {"type": "string", "pattern": _U64_PATTERN},
        "rfc3339_ns_utc": {"type": "string", "pattern": _RFC3339_NS_UTC_PATTERN},
        "rfc3339_full_date": {"type": "string", "pattern": _RFC3339_FULL_DATE_PATTERN},
        "safe_relative_path": {"type": "string", "pattern": _SAFE_RELATIVE_PATH_PATTERN},
        "bounded_utf8_registry_string": {"type": "string", "minLength": 1, "maxLength": 128},
        "bounded_utf8_registry_string_256": {"type": "string", "minLength": 1, "maxLength": 256},
    }


def _primary_schema(schema_id: str, properties: Mapping[str, JsonObject]) -> JsonObject:
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


def _attachment_schema(schema_id: str, properties: Mapping[str, JsonObject]) -> JsonObject:
    """Return a closed attachment JSON Schema without changing sealed primary schemas."""
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


_QUALITY_FLAGS = [
    "GAP_BEFORE",
    "MISSING_EVENT_TIME",
    "NON_EXECUTABLE",
    "PROVIDER_REVISION",
    "RETRACTED_SOURCE",
    "STALE_SOURCE",
]

_RISK_REASON_CODES = [
    "RISK_CONFIG_MISSING",
    "RISK_CONFIG_INVALID",
    "RISK_SEQUENCE_INVALID",
    "RISK_STATE_UNRECONCILED",
    "RISK_KILL_LATCHED",
    "RISK_SESSION_CLOSED",
    "RISK_EVENT_STALE",
    "RISK_DECIMALS_MISMATCH",
    "RISK_ARITHMETIC_RANGE",
    "RISK_NONPOSITIVE_EQUITY",
    "RISK_SESSION_LOSS",
    "RISK_DRAWDOWN",
    "RISK_STOP_MISSING",
    "RISK_INSUFFICIENT_BALANCE",
    "RISK_RESERVATION_CONFLICT",
    "RISK_INTENT_PENDING",
    "RISK_MIN_NOTIONAL",
    "RISK_MAX_NOTIONAL",
    "RISK_PARTICIPATION",
    "RISK_IMPACT",
    "RISK_CONCENTRATION",
    "RISK_NO_ACTION",
    "RISK_RUN_END_EXIT",
    "RISK_KILL_EXIT",
    "RISK_STOP_TRIGGERED",
    "RISK_TAKE_TRIGGERED",
]

_FILL_REASON_CODES = [
    "FILL_SAME_OR_EARLIER_EVENT",
    "FILL_KILL_LATCHED",
    "FILL_SESSION_END",
    "FILL_NO_NEXT_EVENT",
    "FILL_STALE_EVENT",
    "FILL_NEXT_EVENT_NON_EXECUTABLE",
    "FILL_DECIMALS_MISMATCH",
    "FILL_ZERO_CAPACITY",
    "FILL_PARTICIPATION",
    "FILL_ARITHMETIC_RANGE",
    "FILL_IMPACT",
    "FILL_FEE_CAP",
    "FILL_FEE_EXCEEDS_PROCEEDS",
    "FILL_INSUFFICIENT_RESERVATION",
    "FILL_PARTIAL",
]

_RECONCILIATION_REASON_CODES = [
    "RECONCILIATION_IDEMPOTENCY_CONFLICT",
    "RECONCILIATION_MODEL_ATTEMPT_INTEGRITY",
    "RECONCILIATION_EXECUTION_TRANSITION_INTEGRITY",
    "RECONCILIATION_ARITHMETIC_RANGE",
    "RECONCILIATION_ACCOUNT_RESIDUAL",
    "RECONCILIATION_ASSET_RESIDUAL",
    "RECONCILIATION_PNL_RESIDUAL",
    "RECONCILIATION_FEE_RESIDUAL",
    "RECONCILIATION_EQUITY_RESIDUAL",
    "RECONCILIATION_RESERVATION_RESIDUAL",
    "RECONCILIATION_INTENT_CARDINALITY",
    "RECONCILIATION_INTENT_RESERVATION_BIJECTION",
    "RECONCILIATION_ABSOLUTE_STATE_INVARIANT",
    "RECONCILIATION_RUN_END_UNCLOSED",
    "RECONCILIATION_MISMATCH",
]

_LATCHING_RISK_REASON_CODES = [
    "RISK_CONFIG_MISSING",
    "RISK_CONFIG_INVALID",
    "RISK_SEQUENCE_INVALID",
    "RISK_STATE_UNRECONCILED",
    "RISK_DECIMALS_MISMATCH",
    "RISK_ARITHMETIC_RANGE",
    "RISK_NONPOSITIVE_EQUITY",
    "RISK_SESSION_LOSS",
    "RISK_DRAWDOWN",
    "RISK_RESERVATION_CONFLICT",
]


def raw_event_schema() -> JsonObject:
    """Return the exhaustive closed ``trading.raw-event/v1`` JSON Schema."""
    source_position = _closed_object(
        {
            "slot": _ref("u64s"),
            "transaction_index": {"type": "integer", "minimum": 0, "maximum": 4_294_967_295},
            "instruction_index": {"type": "integer", "minimum": 0, "maximum": 4_294_967_295},
            "event_index": {"type": "integer", "minimum": 0, "maximum": 4_294_967_295},
            "source_native_event_id": _ref("bounded_utf8_registry_string_256"),
            "source_subsequence": _ref("u64s"),
        }
    )
    revision = _closed_object(
        {
            "kind": {"enum": ["ORIGINAL", "CORRECTION", "RETRACTION"]},
            "supersedes_event_id": _nullable_ref("ContentID"),
            "retracts_event_id": _nullable_ref("ContentID"),
            "availability_slot": _ref("u64s"),
            "availability_admission_sequence": _ref("u64s"),
        }
    )
    market = _closed_object(
        {
            "base_amount_atoms": _ref("u64s"),
            "quote_amount_atoms": _ref("u64s"),
            "route_capacity_base_atoms": _ref("u64s"),
            "liquidity_quote_atoms": _ref("u64s"),
            "venue_fee_quote_atoms": _ref("u64s"),
            "priority_fee_quote_atoms": _ref("u64s"),
            "route_impact_bps": {"type": "integer", "minimum": 0, "maximum": 1_000_000},
        }
    )
    schema_id = "trading.raw-event/v1"
    return _primary_schema(
        schema_id,
        {
            "schema": {"const": schema_id},
            "event_id": _ref("ContentID"),
            "fixture_manifest_sha256": _ref("ContentID"),
            "raw_payload_sha256": _ref("sha256"),
            "source_admission_receipt_id": _ref("ContentID"),
            "source_id": _ref("bounded_utf8_registry_string"),
            "source_kind": _ref("bounded_utf8_registry_string"),
            "source_revision": _ref("bounded_utf8_registry_string"),
            "network": {"const": "solana-mainnet"},
            "venue_profile": {"const": "solana-jupiter-fixture/v1"},
            "market_id": _ref("bounded_utf8_registry_string"),
            "base_mint": _ref("bounded_utf8_registry_string"),
            "quote_mint": _ref("bounded_utf8_registry_string"),
            "base_decimals": {"type": "integer", "minimum": 0, "maximum": 18},
            "quote_decimals": {"type": "integer", "minimum": 0, "maximum": 18},
            "event_kind": {"enum": ["ROUTE_QUOTE", "SWAP_OBSERVATION", "LIQUIDITY_SNAPSHOT", "SESSION_BOUNDARY"]},
            "source_position": source_position,
            "revision": revision,
            "event_time": _nullable_ref("rfc3339_ns_utc"),
            "observed_at": _ref("rfc3339_ns_utc"),
            "ingested_at": _ref("rfc3339_ns_utc"),
            "admission_sequence": _ref("u64s"),
            "source_sequence": _ref("u64s"),
            "ingest_sequence": _ref("u64s"),
            "equal_time_group": _ref("u64s"),
            "replay_clock_ns": _ref("u64s"),
            "executable": {"type": "boolean"},
            "market": market,
            "quality_flags": _array({"enum": _QUALITY_FLAGS}, unique=True, maximum=len(_QUALITY_FLAGS)),
        },
    )


_FEATURE_NAMES = [
    "atr_14_price_q18",
    "breakout_high_20_price_q18",
    "breakout_low_20_price_q18",
    "ema_fast_price_q18",
    "ema_slow_price_q18",
    "liquidity_quote_atoms",
    "mid_price_q18",
    "return_1_q18",
    "route_impact_bps",
    "rsi_14_q18",
    "stale_age_ns",
    "volume_20_base_atoms",
]


def feature_snapshot_schema() -> JsonObject:
    """Return the exhaustive closed ``trading.feature-snapshot/v1`` JSON Schema."""
    features = _closed_object(
        {
            "mid_price_q18": _nullable_ref("sq18s"),
            "return_1_q18": _nullable_ref("sq18s"),
            "ema_fast_price_q18": _nullable_ref("sq18s"),
            "ema_slow_price_q18": _nullable_ref("sq18s"),
            "rsi_14_q18": _nullable_ref("sq18s"),
            "atr_14_price_q18": _nullable_ref("sq18s"),
            "breakout_high_20_price_q18": _nullable_ref("sq18s"),
            "breakout_low_20_price_q18": _nullable_ref("sq18s"),
            "volume_20_base_atoms": _nullable_ref("u64s"),
            "liquidity_quote_atoms": _nullable_ref("u64s"),
            "stale_age_ns": _nullable_ref("u64s"),
            "route_impact_bps": {
                "anyOf": [
                    {"type": "integer", "minimum": 0, "maximum": 1_000_000},
                    {"type": "null"},
                ]
            },
            "history_count": {"type": "integer", "minimum": 0, "maximum": 4_294_967_295},
        }
    )
    schema_id = "trading.feature-snapshot/v1"
    return _primary_schema(
        schema_id,
        {
            "schema": {"const": schema_id},
            "snapshot_id": _ref("ContentID"),
            "feature_set_version": {"const": "solana-jupiter-deterministic-features/v1"},
            "feature_code_sha256": _ref("sha256"),
            "input_merkle_root_sha256": _nullable_ref("sha256"),
            "market_id": _ref("bounded_utf8_registry_string"),
            "base_mint": _ref("bounded_utf8_registry_string"),
            "quote_mint": _ref("bounded_utf8_registry_string"),
            "base_decimals": {"type": "integer", "minimum": 0, "maximum": 18},
            "quote_decimals": {"type": "integer", "minimum": 0, "maximum": 18},
            "decision_sequence": _ref("u64s"),
            "as_of_event_id": _nullable_ref("ContentID"),
            "as_of_admission_sequence": _ref("u64s"),
            "as_of_ingest_sequence": _ref("u64s"),
            "equal_time_group": _ref("u64s"),
            "replay_clock_ns": _ref("u64s"),
            "event_time": _nullable_ref("rfc3339_ns_utc"),
            "observed_at": _nullable_ref("rfc3339_ns_utc"),
            "ingested_at": _nullable_ref("rfc3339_ns_utc"),
            "features": features,
            "missing_features": _array(
                {"enum": _FEATURE_NAMES},
                unique=True,
                maximum=len(_FEATURE_NAMES),
            ),
        },
    )


def model_signal_schema() -> JsonObject:
    """Return the exhaustive closed ``trading.model-signal/v1`` JSON Schema."""
    schema_id = "trading.model-signal/v1"
    return _primary_schema(
        schema_id,
        {
            "schema": {"const": schema_id},
            "signal_id": _ref("ContentID"),
            "producer_id": _ref("bounded_utf8_registry_string"),
            "producer_sequence": _ref("u64s"),
            "model_id": _ref("bounded_utf8_registry_string"),
            "model_version": _ref("bounded_utf8_registry_string"),
            "runtime_profile_id": _ref("bounded_utf8_registry_string"),
            "calibration_version": _ref("bounded_utf8_registry_string"),
            "model_artifact_sha256": _ref("sha256"),
            "adapter_sha256": _nullable_ref("sha256"),
            "calibration_sha256": _ref("sha256"),
            "feature_snapshot_id": _ref("ContentID"),
            "feature_set_version": _ref("bounded_utf8_registry_string"),
            "market_id": _ref("bounded_utf8_registry_string"),
            "horizon_ns": _ref("u64s"),
            "decision_sequence": _ref("u64s"),
            "as_of_ingest_sequence": _ref("u64s"),
            "issued_replay_clock_ns": _ref("u64s"),
            "available_replay_clock_ns": _ref("u64s"),
            "decision_close_replay_clock_ns": _ref("u64s"),
            "ttl_ns": _ref("u64s"),
            "expires_replay_clock_ns": _ref("u64s"),
            "action": {"enum": ["ABSTAIN", "LONG_BIAS", "EXIT_BIAS"]},
            "score_q18": _ref("sq18s"),
            "probability_abstain_q18": _ref("uq18s"),
            "probability_long_bias_q18": _ref("uq18s"),
            "probability_exit_bias_q18": _ref("uq18s"),
            "uncertainty_q18": _ref("uq18s"),
            "ood_score_q18": _ref("uq18s"),
            "inference_duration_ns": _ref("u64s"),
        },
    )


def risk_decision_schema() -> JsonObject:
    """Return the exhaustive closed ``trading.risk-decision/v1`` JSON Schema."""
    nullable_bps: JsonObject = {
        "anyOf": [
            {"type": "integer", "minimum": 0, "maximum": 2_147_483_647},
            {"type": "null"},
        ]
    }
    measures = _closed_object(
        {
            "participation_bps": nullable_bps,
            "impact_bps": nullable_bps,
            "concentration_bps": nullable_bps,
            "drawdown_bps": nullable_bps,
            "projected_market_value_quote_atoms": _nullable_ref("u64s"),
            "projected_equity_quote_atoms": _nullable_ref("u64s"),
            "session_pnl_quote_atoms": _nullable_ref("i128s"),
            "stale_age_ns": _nullable_ref("u64s"),
        }
    )
    schema_id = "trading.risk-decision/v1"
    action: JsonObject = {"enum": ["HOLD", "ENTER_LONG", "EXIT_LONG"]}
    return _primary_schema(
        schema_id,
        {
            "schema": {"const": schema_id},
            "risk_decision_id": _ref("ContentID"),
            "decision_sequence": _ref("u64s"),
            "replay_clock_ns": _ref("u64s"),
            "feature_snapshot_id": _ref("ContentID"),
            "config_admission_receipt_id": _ref("ContentID"),
            "validated_config_sha256": _nullable_ref("ContentID"),
            "portfolio_state_before_id": _ref("ContentID"),
            "model_signal_status": {"enum": ["ABSENT", "ACCEPTED", "REJECTED", "EXPIRED", "DRIFT_DISABLED"]},
            "model_signal_id": _nullable_ref("ContentID"),
            "model_validation_receipt_id": _nullable_ref("ContentID"),
            "baseline_id": {
                "enum": [
                    "ALWAYS_HOLD_V1",
                    "MEAN_REVERSION_V1",
                    "BREAKOUT_V1",
                    "MOMENTUM_V1",
                    "TREND_V1",
                    None,
                ]
            },
            "baseline_action": action,
            "fused_action": action,
            "effective_action": action,
            "market_id": _ref("bounded_utf8_registry_string"),
            "verdict": {"enum": ["APPROVE", "REJECT", "KILL"]},
            "reason_codes": _array(
                {"enum": _RISK_REASON_CODES},
                unique=True,
                maximum=len(_RISK_REASON_CODES),
            ),
            "reference_price_q18": _nullable_ref("sq18s"),
            "requested_base_atoms": _ref("u64s"),
            "approved_base_atoms": _ref("u64s"),
            "requested_notional_quote_atoms": _ref("u64s"),
            "approved_notional_quote_atoms": _ref("u64s"),
            "measures": measures,
            "stop_price_q18": _nullable_ref("sq18s"),
            "take_price_q18": _nullable_ref("sq18s"),
            "reservation_id": _nullable_ref("ContentID"),
            "reserved_quote_atoms": _ref("u64s"),
            "reserved_base_atoms": _ref("u64s"),
        },
    )


def simulated_order_intent_schema() -> JsonObject:
    """Return the exhaustive closed simulated-order-intent v1 JSON Schema."""
    schema_id = "trading.simulated-order-intent/v1"
    return _primary_schema(
        schema_id,
        {
            "schema": {"const": schema_id},
            "intent_id": _ref("ContentID"),
            "intent_sequence": _ref("u64s"),
            "decision_sequence": _ref("u64s"),
            "risk_decision_id": _ref("ContentID"),
            "config_admission_receipt_id": _ref("ContentID"),
            "validated_config_sha256": _ref("ContentID"),
            "portfolio_state_before_id": _ref("ContentID"),
            "reservation_id": _ref("ContentID"),
            "market_id": _ref("bounded_utf8_registry_string"),
            "base_mint": _ref("bounded_utf8_registry_string"),
            "quote_mint": _ref("bounded_utf8_registry_string"),
            "base_decimals": {"type": "integer", "minimum": 0, "maximum": 18},
            "quote_decimals": {"type": "integer", "minimum": 0, "maximum": 18},
            "action": {"enum": ["OPEN_LONG", "CLOSE_LONG"]},
            "quantity_base_atoms": _ref("u64s"),
            "reference_price_q18": _ref("sq18s"),
            "stop_price_q18": _nullable_ref("sq18s"),
            "take_price_q18": _nullable_ref("sq18s"),
            "max_participation_bps": {"type": "integer", "minimum": 0, "maximum": 2_147_483_647},
            "max_impact_bps": {"type": "integer", "minimum": 0, "maximum": 2_147_483_647},
            "reserved_quote_atoms": _ref("u64s"),
            "reserved_base_atoms": _ref("u64s"),
            "created_replay_clock_ns": _ref("u64s"),
            "decision_ingest_sequence": _ref("u64s"),
            "decision_equal_time_group": _ref("u64s"),
            "fill_policy": {"const": "STRICT_NEXT_EVENT"},
            "time_in_force": {"const": "ONE_EVENT_GROUP"},
        },
    )


def simulated_fill_receipt_schema() -> JsonObject:
    """Return the exhaustive closed simulated-fill-receipt v1 JSON Schema."""
    schema_id = "trading.simulated-fill-receipt/v1"
    nonnegative_bps: JsonObject = {"type": "integer", "minimum": 0, "maximum": 2_147_483_647}
    return _primary_schema(
        schema_id,
        {
            "schema": {"const": schema_id},
            "fill_receipt_id": _ref("ContentID"),
            "receipt_sequence": _ref("u64s"),
            "decision_sequence": _ref("u64s"),
            "intent_id": _ref("ContentID"),
            "portfolio_state_before_id": _ref("ContentID"),
            "status": {"enum": ["FILLED", "PARTIAL", "REJECTED", "EXPIRED"]},
            "reason_codes": _array(
                {"enum": _FILL_REASON_CODES},
                unique=True,
                maximum=len(_FILL_REASON_CODES),
            ),
            "fill_event_id": _nullable_ref("ContentID"),
            "decision_ingest_sequence": _ref("u64s"),
            "decision_equal_time_group": _ref("u64s"),
            "fill_ingest_sequence": _nullable_ref("u64s"),
            "fill_equal_time_group": _nullable_ref("u64s"),
            "fill_replay_clock_ns": _nullable_ref("u64s"),
            "requested_base_atoms": _ref("u64s"),
            "filled_base_atoms": _ref("u64s"),
            "unfilled_base_atoms": _ref("u64s"),
            "gross_quote_atoms": _ref("u64s"),
            "venue_fee_quote_atoms": _ref("u64s"),
            "priority_fee_quote_atoms": _ref("u64s"),
            "simulation_fee_quote_atoms": _ref("u64s"),
            "cash_delta_quote_atoms": _ref("i128s"),
            "execution_price_q18": _nullable_ref("sq18s"),
            "participation_bps": nonnegative_bps,
            "reference_deviation_bps": nonnegative_bps,
            "impact_bps": nonnegative_bps,
            "fee_bps": nonnegative_bps,
            "adverse_fill_bps": nonnegative_bps,
            "adverse_fill_draw_key_sha256": _ref("sha256"),
            "adverse_fill_draw_sha256": _ref("sha256"),
            "released_quote_atoms": _ref("u64s"),
            "released_base_atoms": _ref("u64s"),
        },
    )


def portfolio_state_schema() -> JsonObject:
    """Return the exhaustive closed ``trading.portfolio-state/v1`` JSON Schema."""
    balance = _closed_object(
        {
            "mint": _ref("bounded_utf8_registry_string"),
            "decimals": {"type": "integer", "minimum": 0, "maximum": 18},
            "available_atoms": _ref("u64s"),
            "reserved_atoms": _ref("u64s"),
            "total_atoms": _ref("u64s"),
        }
    )
    position = _closed_object(
        {
            "market_id": _ref("bounded_utf8_registry_string"),
            "base_mint": _ref("bounded_utf8_registry_string"),
            "base_decimals": {"type": "integer", "minimum": 0, "maximum": 18},
            "quantity_base_atoms": _ref("u64s"),
            "reserved_base_atoms": _ref("u64s"),
            "cost_basis_quote_atoms": _ref("u64s"),
            "mark_price_q18": _ref("sq18s"),
            "market_value_quote_atoms": _ref("u64s"),
            "unrealized_pnl_quote_atoms": _ref("i128s"),
            "stop_price_q18": _ref("sq18s"),
            "take_price_q18": _ref("sq18s"),
        }
    )
    summary = _closed_object(
        {
            "realized_pnl_quote_atoms": _ref("i128s"),
            "unrealized_pnl_quote_atoms": _ref("i128s"),
            "cumulative_fees_quote_atoms": _ref("u64s"),
            "session_pnl_quote_atoms": _ref("i128s"),
            "equity_quote_atoms": _ref("u64s"),
            "peak_equity_quote_atoms": _ref("u64s"),
            "drawdown_bps": {"type": "integer", "minimum": 0, "maximum": 10_000},
        }
    )
    schema_id = "trading.portfolio-state/v1"
    kill_codes = [*_LATCHING_RISK_REASON_CODES, *_RECONCILIATION_REASON_CODES]
    return _primary_schema(
        schema_id,
        {
            "schema": {"const": schema_id},
            "portfolio_state_id": _ref("ContentID"),
            "state_sequence": _ref("u64s"),
            "as_of_ingest_sequence": _ref("u64s"),
            "equal_time_group": _ref("u64s"),
            "replay_clock_ns": _ref("u64s"),
            "previous_portfolio_state_id": _nullable_ref("ContentID"),
            "causation_schema": {
                "enum": [
                    "RUN_INITIALIZATION",
                    "GROUP_MARK_TO_MARKET",
                    "trading.simulated-order-intent/v1",
                    "trading.simulated-fill-receipt/v1",
                    "RISK_KILL",
                ]
            },
            "causation_id": _ref("CausalDigest"),
            "fixture_manifest_sha256": _ref("ContentID"),
            "config_admission_receipt_id": _ref("ContentID"),
            "validated_config_sha256": _nullable_ref("ContentID"),
            "quote_mint": _ref("bounded_utf8_registry_string"),
            "quote_decimals": {"type": "integer", "minimum": 0, "maximum": 18},
            "balances": _array(balance, unique=True),
            "positions": _array(position, unique=True),
            "summary": summary,
            "kill_latched": {"type": "boolean"},
            "kill_reason_codes": _array(
                {"enum": kill_codes},
                unique=True,
                maximum=len(kill_codes),
            ),
            "open_intent_ids": _array(_ref("ContentID"), unique=True),
        },
    )


_LEDGER_ACCOUNTS = [
    "CASH_AVAILABLE",
    "CASH_RESERVED",
    "POSITION_AVAILABLE",
    "POSITION_RESERVED",
    "TRADE_CLEARING",
    "FEE_EXPENSE",
    "REALIZED_PNL",
    "UNREALIZED_PNL",
    "EQUITY_CONTROL",
]

_STATE_BEARING_ACCOUNTS = [
    "CASH_AVAILABLE",
    "CASH_RESERVED",
    "POSITION_AVAILABLE",
    "POSITION_RESERVED",
]

_LEDGER_RECORD_TYPES = [
    "SOURCE_ADMISSION",
    "CONFIG_ADMISSION",
    "RAW_ADMISSION",
    "FEATURE_SNAPSHOT",
    "MODEL_VALIDATION",
    "MODEL_SIGNAL_ACCEPTED",
    "RISK_DECISION",
    "SIMULATED_INTENT",
    "SIMULATED_FILL",
    "PORTFOLIO_STATE",
    "RECONCILIATION",
    "KILL_STATE",
    "RUN_RECEIPT",
    "RUN_END",
]

_LEDGER_OBJECT_SCHEMAS = [
    "trading.source-admission-receipt/v1",
    "trading.config-admission-receipt/v1",
    "trading.raw-event/v1",
    "trading.feature-snapshot/v1",
    "trading.model-validation-receipt/v1",
    "trading.model-signal/v1",
    "trading.risk-decision/v1",
    "trading.simulated-order-intent/v1",
    "trading.simulated-fill-receipt/v1",
    "trading.portfolio-state/v1",
    "trading.reconciliation-receipt/v1",
    "trading.run-receipt/v1",
]


def ledger_record_schema() -> JsonObject:
    """Return the exhaustive closed ``trading.ledger-record/v1`` JSON Schema."""
    entry = _closed_object(
        {
            "account": {"enum": _LEDGER_ACCOUNTS},
            "asset_mint": _ref("bounded_utf8_registry_string"),
            "decimals": {"type": "integer", "minimum": 0, "maximum": 18},
            "amount_atoms": _ref("i128s"),
        }
    )
    asset_residual = _closed_object(
        {
            "asset_mint": _ref("bounded_utf8_registry_string"),
            "residual_atoms": _ref("i128s"),
        }
    )
    account_residual = _closed_object(
        {
            "asset_mint": _ref("bounded_utf8_registry_string"),
            "account": {"enum": _STATE_BEARING_ACCOUNTS},
            "residual_atoms": _ref("i128s"),
        }
    )
    reconciliation = _closed_object(
        {
            "portfolio_state_id": _ref("ContentID"),
            "asset_residuals": _array(asset_residual, unique=True),
            "equity_residual_quote_atoms": _ref("i128s"),
            "unmatched_reservation_count": _ref("u64s"),
            "account_residuals": _array(account_residual, unique=True),
            "realized_pnl_residual_quote_atoms": _ref("i128s"),
            "unrealized_pnl_residual_quote_atoms": _ref("i128s"),
            "fee_residual_quote_atoms": _ref("i128s"),
            "peak_equity_residual_quote_atoms": _ref("i128s"),
            "drawdown_residual_bps": _ref("i128s"),
        }
    )
    schema_id = "trading.ledger-record/v1"
    return _primary_schema(
        schema_id,
        {
            "schema": {"const": schema_id},
            "ledger_record_id": _ref("ContentID"),
            "ledger_sequence": _ref("u64s"),
            "previous_ledger_record_id": _nullable_ref("ContentID"),
            "run_receipt_id": _ref("ContentID"),
            "fixture_manifest_sha256": _ref("ContentID"),
            "config_admission_receipt_id": _ref("ContentID"),
            "validated_config_sha256": _nullable_ref("ContentID"),
            "record_type": {"enum": _LEDGER_RECORD_TYPES},
            "object_schema": {"enum": _LEDGER_OBJECT_SCHEMAS},
            "object_id": _ref("ContentID"),
            "object_sha256": _ref("sha256"),
            "decision_sequence": _nullable_ref("u64s"),
            "ingest_sequence": _nullable_ref("u64s"),
            "equal_time_group": _nullable_ref("u64s"),
            "replay_clock_ns": _nullable_ref("u64s"),
            "causation_ids": _array(_ref("CausalDigest"), unique=True),
            "entries": _array(entry, unique=True),
            "reconciliation": reconciliation,
        },
    )


_SOURCE_ADMISSION_REASON_CODES = [
    "ADMISSION_NOT_LOCAL",
    "ADMISSION_RIGHTS_MISSING",
    "ADMISSION_MANIFEST_MISMATCH",
    "ADMISSION_HASH_MISMATCH",
    "ADMISSION_POINT_IN_TIME_MISSING",
    "ADMISSION_LEAKAGE_FIELD",
    "ADMISSION_UNIVERSE_BIASED",
    "ADMISSION_PROFILE_MISMATCH",
    "ADMISSION_POSITION_CONFLICT",
    "ADMISSION_SEQUENCE_INVALID",
    "ADMISSION_REVISION_CAUSALITY",
    "ADMISSION_REVISION_FORK",
    "ADMISSION_SET_NOT_CLOSED",
]

_CONFIG_ADMISSION_REASON_CODES = [
    "CONFIG_MISSING",
    "CONFIG_BYTES_INVALID",
    "CONFIG_SCHEMA_INVALID",
    "CONFIG_ID_MISMATCH",
    "CONFIG_RANGE_INVALID",
]

_RUN_CLOSURE_REASON_CODES = [
    "ADMISSION_COUNTER_CAPACITY",
    "ADMISSION_RUN_END_PROOF_BUDGET",
    "ADMISSION_RUN_END_UNCLOSED",
]

_RUN_CLOSURE_MARKET_REASON_CODES = [
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
]

_QUARANTINE_REASON_CODES = [
    "QUARANTINE_INITIAL_PREFIX_ANCHOR_MISSING",
    "QUARANTINE_LEDGER_SLOT_MISSING",
    "QUARANTINE_LEDGER_SLOT_WRONG",
    "QUARANTINE_LEDGER_SLOT_EXTRA",
    "QUARANTINE_APPEND_SLOT_OCCUPIED",
    "QUARANTINE_COMPONENT_MISSING",
    "QUARANTINE_COMPONENT_WRONG",
    "QUARANTINE_COMPONENT_EXTRA",
    "QUARANTINE_FOOTPRINT_MISMATCH",
]

_QUARANTINE_COMPONENT_KINDS = [
    "JOURNAL_KEY",
    "CANONICAL_OBJECT",
    "LEDGER_SEQUENCE_START",
    "LEDGER_SEQUENCE_END",
    "CONSUMED_PRODUCER_SEQUENCE",
    "CLAIMED_REQUEST_KEY",
    "PRIOR_LEDGER_HEAD",
    "PRIOR_PORTFOLIO_STATE",
    "RESULTING_LEDGER_HEAD",
    "RESULTING_PORTFOLIO_STATE",
    "NEXT_LEDGER_SEQUENCE",
    "NEXT_STATE_SEQUENCE",
    "NEXT_DECISION_SEQUENCE",
    "NEXT_INTENT_SEQUENCE",
    "NEXT_FILL_RECEIPT_SEQUENCE",
]

_MODEL_VALIDATION_REASON_CODES = [
    "SIG_BYTES_INVALID",
    "SIG_SCHEMA_UNKNOWN",
    "SIG_ID_MISMATCH",
    "SIG_ORDER_SHAPED",
    "SIG_MODEL_UNPINNED",
    "SIG_RUNTIME_SUBSTITUTION",
    "SIG_SCOPE_MISMATCH",
    "SIG_FEATURE_MISMATCH",
    "SIG_PRODUCER_SEQUENCE_INVALID",
    "SIG_REPLAYED",
    "SIG_TIME_INVALID",
    "SIG_TTL_RANGE",
    "SIG_EXPIRED",
    "SIG_DEADLINE_MISS",
    "SIG_NUMERIC_INVALID",
    "SIG_PROBABILITY_INVALID",
    "SIG_ACTION_INCONSISTENT",
    "SIG_CALIBRATION_UNKNOWN",
    "SIG_UNCERTAIN_OR_OOD",
    "SIG_DRIFT_DISABLED",
]

_BASELINE_IDS = [
    "ALWAYS_HOLD_V1",
    "MEAN_REVERSION_V1",
    "BREAKOUT_V1",
    "MOMENTUM_V1",
    "TREND_V1",
]


def fixture_manifest_schema() -> JsonObject:
    """Return the exhaustive closed FixtureManifest schema."""
    allowed_market = _closed_object(
        {
            "market_id": _ref("bounded_utf8_registry_string"),
            "base_mint": _ref("bounded_utf8_registry_string"),
            "quote_mint": _ref("bounded_utf8_registry_string"),
            "base_decimals": {"type": "integer", "minimum": 0, "maximum": 18},
            "quote_decimals": {"type": "integer", "minimum": 0, "maximum": 18},
        }
    )
    source_file = _closed_object(
        {
            "relative_path": _ref("safe_relative_path"),
            "raw_payload_sha256": _ref("sha256"),
            "byte_length": _ref("u64s"),
            "admission_sequence": _ref("u64s"),
            "availability_slot": _ref("u64s"),
            "media_type": {"enum": ["application/json", "application/jsonl", "application/octet-stream"]},
            "source_id": _ref("bounded_utf8_registry_string"),
            "source_kind": _ref("bounded_utf8_registry_string"),
            "source_revision": _ref("bounded_utf8_registry_string"),
            "market_id": _ref("bounded_utf8_registry_string"),
        }
    )
    allowed_markets = _array(allowed_market, unique=True)
    allowed_markets["minItems"] = 1
    files = _array(source_file, unique=True)
    files["minItems"] = 1
    schema_id = "trading.fixture-manifest/v1"
    return _primary_schema(
        schema_id,
        {
            "schema": {"const": schema_id},
            "fixture_manifest_sha256": _ref("ContentID"),
            "network": {"const": "solana-mainnet"},
            "venue_profile": {"const": "solana-jupiter-fixture/v1"},
            "quote_mint": _ref("bounded_utf8_registry_string"),
            "quote_decimals": {"type": "integer", "minimum": 0, "maximum": 18},
            "initial_quote_atoms": _ref("u64s"),
            "replay_tick_ns": _ref("u64s"),
            "universe_policy": {"const": "FULL_DECLARED_SOURCE_UNIVERSE"},
            "point_in_time_mode": {"const": "PROVEN_AVAILABILITY_SLOT"},
            "run_end_position_policy": {"enum": ["FORCE_CLOSE_NEXT_EVENT", "LEAVE_MARKED_OPEN"]},
            "session_start_availability_slot": _ref("u64s"),
            "session_end_availability_slot": _ref("u64s"),
            "rights_manifest_sha256": _ref("sha256"),
            "selection_failures_retained": {"const": True},
            "no_route_observations_retained": {"const": True},
            "inactive_assets_retained": {"const": True},
            "gaps_retained": {"const": True},
            "allowed_markets": allowed_markets,
            "files": files,
        },
    )


def replay_risk_config_schema() -> JsonObject:
    """Return the exhaustive closed ReplayRiskConfig schema."""
    schema_id = "trading.replay-risk-config/v1"
    bps = {"type": "integer", "minimum": 0, "maximum": 10_000}
    return _primary_schema(
        schema_id,
        {
            "schema": {"const": schema_id},
            "config_sha256": _ref("ContentID"),
            "baseline_id": {"enum": _BASELINE_IDS},
            "target_entry_notional_quote_atoms": _ref("u64s"),
            "min_notional_quote_atoms": _ref("u64s"),
            "max_notional_quote_atoms": _ref("u64s"),
            "price_tick_q18": _ref("sq18s"),
            "max_participation_bps": bps,
            "max_impact_bps": {"type": "integer", "minimum": 0, "maximum": 1_000_000},
            "max_fee_bps": bps,
            "max_concentration_bps": bps,
            "max_session_loss_quote_atoms": _ref("u64s"),
            "max_drawdown_bps": bps,
            "stale_after_ns": _ref("u64s"),
            "stop_loss_bps": {"type": "integer", "minimum": 1, "maximum": 9_999},
            "take_profit_bps": {"type": "integer", "minimum": 1, "maximum": 1_000_000},
            "adverse_fill_bps_max": {"type": "integer", "minimum": 0, "maximum": 9_999},
            "max_run_closure_proof_rows": _ref("u64s"),
            "fee_model_version": {"const": "solana-jupiter-fixture-fee-proration/v1"},
            "liquidity_model_version": {"const": "solana-jupiter-route-capacity/v1"},
            "session_start_replay_clock_ns": _ref("u64s"),
            "session_end_replay_clock_ns": _ref("u64s"),
            "kill_exit_mode": {"enum": ["FREEZE_NO_NEW_INTENTS", "CLOSE_ON_NEXT_EVENT"]},
        },
    )


def source_admission_receipt_schema() -> JsonObject:
    """Return the exhaustive closed SourceAdmissionReceipt schema."""
    nullable_registry: JsonObject = {"anyOf": [_ref("bounded_utf8_registry_string"), {"type": "null"}]}
    schema_id = "trading.source-admission-receipt/v1"
    return _primary_schema(
        schema_id,
        {
            "schema": {"const": schema_id},
            "source_admission_receipt_id": _ref("ContentID"),
            "status": {"enum": ["ADMITTED", "REJECTED", "QUARANTINED"]},
            "reason_codes": _array({"enum": _SOURCE_ADMISSION_REASON_CODES}, unique=True),
            "fixture_manifest_sha256": _nullable_ref("ContentID"),
            "raw_payload_sha256": _nullable_ref("sha256"),
            "terms_sha256": _nullable_ref("sha256"),
            "parser_code_sha256": _ref("sha256"),
            "source_id": nullable_registry,
            "source_kind": nullable_registry,
            "source_revision": nullable_registry,
            "market_id": nullable_registry,
            "relative_path": {"anyOf": [_ref("safe_relative_path"), {"type": "null"}]},
            "media_type": {
                "anyOf": [
                    {"enum": ["application/json", "application/jsonl", "application/octet-stream"]},
                    {"type": "null"},
                ]
            },
            "byte_length": _nullable_ref("u64s"),
            "admission_sequence": _ref("u64s"),
            "availability_slot": _nullable_ref("u64s"),
            "observed_at": _nullable_ref("rfc3339_ns_utc"),
            "ingested_at": _nullable_ref("rfc3339_ns_utc"),
            "rights_role": {"anyOf": [{"const": "offline_research_replay"}, {"type": "null"}]},
            "rights_effective_date": _nullable_ref("rfc3339_full_date"),
            "rights_review_date": _nullable_ref("rfc3339_full_date"),
            "parser_version": {"const": "solana-jupiter-fixture-parser/v1"},
        },
    )


def config_admission_receipt_schema() -> JsonObject:
    """Return the exhaustive closed ConfigAdmissionReceipt schema."""
    schema_id = "trading.config-admission-receipt/v1"
    return _primary_schema(
        schema_id,
        {
            "schema": {"const": schema_id},
            "config_admission_receipt_id": _ref("ContentID"),
            "status": {"enum": ["MISSING", "INVALID", "VALID"]},
            "reason_codes": _array({"enum": _CONFIG_ADMISSION_REASON_CODES}, unique=True),
            "raw_config_sha256": _nullable_ref("sha256"),
            "validated_config_sha256": _nullable_ref("ContentID"),
            "raw_byte_length": _ref("u64s"),
            "admission_sequence": _ref("u64s"),
            "validator_code_sha256": _ref("sha256"),
            "schema_bundle_sha256": _ref("sha256"),
        },
    )


def run_closure_receipt_schema() -> JsonObject:
    """Return the exhaustive closed RunClosureReceipt schema."""
    schema_id = "trading.run-closure-receipt/v1"
    nullable_u64 = _nullable_ref("u64s")
    nullable_sha256 = _nullable_ref("sha256")
    market_proof = _closed_object(
        {
            "market_id": _ref("bounded_utf8_registry_string"),
            "failure_codes": _array({"enum": _RUN_CLOSURE_MARKET_REASON_CODES}, unique=True),
            "earliest_trigger_equal_time_group": nullable_u64,
            "fill_event_id": _nullable_ref("ContentID"),
            "q_cap_base_atoms": nullable_u64,
            "capacity_base_atoms": nullable_u64,
            "fill_candidate_semantic_sha256": nullable_sha256,
            "reference_price_count": nullable_u64,
            "adverse_fill_extreme_count": nullable_u64,
            "proof_row_count": nullable_u64,
            "state_envelope_sha256": nullable_sha256,
            "proof_domain": {
                "anyOf": [{"const": "ALL_RESIDUAL_REFERENCE_PAIRS_AT_FILL_EXTREMES_V1"}, {"type": "null"}]
            },
            "reference_set_root_sha256": nullable_sha256,
            "proof_root_sha256": nullable_sha256,
        }
    )
    source_ids = _array(_ref("ContentID"), unique=True)
    source_ids["minItems"] = 1
    return _primary_schema(
        schema_id,
        {
            "schema": {"const": schema_id},
            "run_closure_receipt_id": _ref("ContentID"),
            "status": {"enum": ["NOT_REQUIRED_ZERO_AUTHORITY", "PASS", "FAIL"]},
            "reason_codes": _array({"enum": _RUN_CLOSURE_REASON_CODES}, unique=True),
            "fixture_manifest_sha256": _ref("ContentID"),
            "config_admission_receipt_id": _ref("ContentID"),
            "validated_config_sha256": _nullable_ref("ContentID"),
            "model_signal_mode": {"enum": ["DISABLED", "CACHED_FIXTURES"]},
            "model_registry_sha256": _nullable_ref("ContentID"),
            "model_signal_manifest_sha256": _nullable_ref("ContentID"),
            "schema_bundle_sha256": _ref("sha256"),
            "admission_code_sha256": _ref("sha256"),
            "normalization_code_sha256": _ref("sha256"),
            "availability_grouping_code_sha256": _ref("sha256"),
            "run_closure_code_sha256": _ref("sha256"),
            "risk_code_sha256": _ref("sha256"),
            "feature_code_sha256": _ref("sha256"),
            "fill_code_sha256": _ref("sha256"),
            "accounting_code_sha256": _ref("sha256"),
            "source_admission_receipt_ids": source_ids,
            "availability_schedule_sha256": _ref("sha256"),
            "counter_capacity_sha256": _ref("sha256"),
            "counter_capacity_status": {"enum": ["WITHIN_LIMIT", "EXCEEDED"]},
            "run_end_position_policy": {"enum": ["FORCE_CLOSE_NEXT_EVENT", "LEAVE_MARKED_OPEN"]},
            "terminal_equal_time_group": nullable_u64,
            "proof_row_limit": nullable_u64,
            "proof_row_count_total": nullable_u64,
            "proof_budget_status": {"enum": ["NOT_REQUIRED", "WITHIN_LIMIT", "EXCEEDED"]},
            "market_proofs": _array(market_proof, unique=True),
        },
    )


def execution_quarantine_receipt_schema() -> JsonObject:
    """Return the exhaustive closed out-of-band quarantine receipt schema."""
    schema_id = "trading.execution-quarantine-receipt/v1"
    classification: JsonObject = {"enum": ["MISSING", "WRONG", "EXTRA"]}
    slot_observation = _closed_object(
        {
            "ledger_sequence": _ref("u64s"),
            "expected_ledger_record_id": _nullable_ref("ContentID"),
            "observed_ledger_record_id": _nullable_ref("ContentID"),
            "observed_byte_sha256": _nullable_ref("sha256"),
            "classification": classification,
        }
    )
    component_observation = _closed_object(
        {
            "component_kind": {"enum": _QUARANTINE_COMPONENT_KINDS},
            "ordinal": _nullable_ref("u64s"),
            "expected_component_sha256": _nullable_ref("sha256"),
            "observed_component_sha256": _nullable_ref("sha256"),
            "classification": classification,
        }
    )
    reasons = _array({"enum": _QUARANTINE_REASON_CODES}, unique=True)
    reasons["minItems"] = 1
    return _primary_schema(
        schema_id,
        {
            "schema": {"const": schema_id},
            "execution_quarantine_receipt_id": _ref("ContentID"),
            "status": {"const": "QUARANTINED"},
            "reason_codes": reasons,
            "run_receipt_id": _ref("ContentID"),
            "transition_kind": {"enum": ["MODEL_ATTEMPT", "EXECUTION_TRANSITION"]},
            "transition_key_sha256": _ref("sha256"),
            "expected_footprint_sha256": _ref("sha256"),
            "observed_footprint_sha256": _ref("sha256"),
            "last_verified_ledger_head_id": _nullable_ref("ContentID"),
            "last_verified_portfolio_state_id": _nullable_ref("ContentID"),
            "first_unsafe_ledger_sequence": _nullable_ref("u64s"),
            "slot_observations": _array(slot_observation, unique=True),
            "component_observations": _array(component_observation, unique=True),
        },
    )


def model_registry_schema() -> JsonObject:
    """Return the exhaustive closed immutable model registry schema."""
    schema_id = "trading.model-registry/v1"
    promotion = _closed_object(
        {
            "producer_scope_key_sha256": _ref("sha256"),
            "producer_id": _ref("bounded_utf8_registry_string"),
            "model_id": _ref("bounded_utf8_registry_string"),
            "model_version": _ref("bounded_utf8_registry_string"),
            "model_artifact_sha256": _ref("sha256"),
            "adapter_sha256": _nullable_ref("sha256"),
            "runtime_profile_id": _ref("bounded_utf8_registry_string"),
            "calibration_version": _ref("bounded_utf8_registry_string"),
            "calibration_sha256": _ref("sha256"),
            "feature_set_version": _ref("bounded_utf8_registry_string"),
            "market_id": _ref("bounded_utf8_registry_string"),
            "horizon_ns": _ref("u64s"),
            "max_ttl_ns": _ref("u64s"),
            "max_uncertainty_q18": _ref("uq18s"),
            "max_ood_score_q18": _ref("uq18s"),
            "status": {"enum": ["ENABLED", "DISABLED"]},
        }
    )
    promotions = _array(promotion, unique=True)
    return _primary_schema(
        schema_id,
        {
            "schema": {"const": schema_id},
            "model_registry_sha256": _ref("ContentID"),
            "promotions": promotions,
        },
    )


def model_signal_manifest_schema() -> JsonObject:
    """Return the exhaustive closed cached model-signal manifest schema."""
    schema_id = "trading.model-signal-manifest/v1"
    candidate = _closed_object(
        {
            "relative_path": _ref("safe_relative_path"),
            "media_type": {"const": "application/json"},
            "raw_signal_sha256": _ref("sha256"),
            "raw_byte_length": _ref("u64s"),
            "declared_signal_id": _nullable_ref("ContentID"),
            "feature_snapshot_id": _ref("ContentID"),
            "decision_sequence": _ref("u64s"),
            "producer_scope_key_sha256": _nullable_ref("sha256"),
            "producer_sequence": _nullable_ref("u64s"),
            "horizon_ns": _ref("u64s"),
            "issued_replay_clock_ns": _ref("u64s"),
            "available_replay_clock_ns": _ref("u64s"),
            "decision_close_replay_clock_ns": _ref("u64s"),
            "requested_producer_scope_key_sha256": _ref("sha256"),
        }
    )
    candidates = _array(candidate, unique=True)
    return _primary_schema(
        schema_id,
        {
            "schema": {"const": schema_id},
            "model_signal_manifest_sha256": _ref("ContentID"),
            "model_registry_sha256": _ref("ContentID"),
            "candidates": candidates,
        },
    )


def model_validation_receipt_schema() -> JsonObject:
    """Return the exhaustive closed model-validation evidence schema."""
    schema_id = "trading.model-validation-receipt/v1"
    return _primary_schema(
        schema_id,
        {
            "schema": {"const": schema_id},
            "model_validation_receipt_id": _ref("ContentID"),
            "validation_attempt_key_sha256": _ref("sha256"),
            "status": {"enum": ["ACCEPTED", "REJECTED", "EXPIRED", "DRIFT_DISABLED"]},
            "reason_codes": _array({"enum": _MODEL_VALIDATION_REASON_CODES}, unique=True),
            "raw_signal_sha256": _ref("sha256"),
            "declared_signal_id": _nullable_ref("ContentID"),
            "accepted_signal_id": _nullable_ref("ContentID"),
            "feature_snapshot_id": _ref("ContentID"),
            "decision_sequence": _ref("u64s"),
            "producer_sequence": _nullable_ref("u64s"),
            "producer_scope_key_sha256": _nullable_ref("sha256"),
            "issued_replay_clock_ns": _nullable_ref("u64s"),
            "available_replay_clock_ns": _ref("u64s"),
            "decision_close_replay_clock_ns": _ref("u64s"),
            "expires_replay_clock_ns": _nullable_ref("u64s"),
            "canonical_action": {"enum": ["ABSTAIN", "LONG_BIAS", "EXIT_BIAS"]},
            "canonical_score_q18": _ref("sq18s"),
            "canonical_probability_abstain_q18": _ref("uq18s"),
            "canonical_probability_long_bias_q18": _ref("uq18s"),
            "canonical_probability_exit_bias_q18": _ref("uq18s"),
            "canonical_uncertainty_q18": _ref("uq18s"),
            "canonical_ood_score_q18": _ref("uq18s"),
        },
    )


def reconciliation_receipt_schema() -> JsonObject:
    """Return the closed observational reconciliation receipt schema."""
    schema_id = "trading.reconciliation-receipt/v1"
    reason_codes = [
        "RECONCILIATION_IDEMPOTENCY_CONFLICT",
        "RECONCILIATION_MODEL_ATTEMPT_INTEGRITY",
        "RECONCILIATION_EXECUTION_TRANSITION_INTEGRITY",
        "RECONCILIATION_ARITHMETIC_RANGE",
        "RECONCILIATION_ACCOUNT_RESIDUAL",
        "RECONCILIATION_ASSET_RESIDUAL",
        "RECONCILIATION_PNL_RESIDUAL",
        "RECONCILIATION_FEE_RESIDUAL",
        "RECONCILIATION_EQUITY_RESIDUAL",
        "RECONCILIATION_RESERVATION_RESIDUAL",
        "RECONCILIATION_INTENT_CARDINALITY",
        "RECONCILIATION_INTENT_RESERVATION_BIJECTION",
        "RECONCILIATION_ABSOLUTE_STATE_INVARIANT",
        "RECONCILIATION_RUN_END_UNCLOSED",
        "RECONCILIATION_MISMATCH",
    ]
    asset_residual = _closed_object(
        {
            "asset_mint": _ref("bounded_utf8_registry_string"),
            "residual_atoms": _ref("i128s"),
        }
    )
    account_residual = _closed_object(
        {
            "asset_mint": _ref("bounded_utf8_registry_string"),
            "account": {"enum": _STATE_BEARING_ACCOUNTS},
            "residual_atoms": _ref("i128s"),
        }
    )
    return _primary_schema(
        schema_id,
        {
            "schema": {"const": schema_id},
            "reconciliation_receipt_id": _ref("ContentID"),
            "status": {"enum": ["PASS", "KILLED"]},
            "reconciliation_kind": {
                "enum": [
                    "GENESIS",
                    "INTENT_RESERVATION",
                    "FILL_TRANSITION",
                    "GROUP_MARK",
                    "RISK_KILL",
                    "GROUP_GATE",
                    "FINAL_GATE",
                    "IDEMPOTENCY_CONFLICT",
                    "MODEL_ATTEMPT_INTEGRITY",
                    "EXECUTION_TRANSITION_INTEGRITY",
                    "ARITHMETIC_RANGE",
                    "TERMINAL_KILL_PROMOTION",
                ]
            },
            "reason_codes": _array({"enum": reason_codes}, unique=True),
            "decision_sequence": _nullable_ref("u64s"),
            "ingest_sequence": _ref("u64s"),
            "equal_time_group": _ref("u64s"),
            "replay_clock_ns": _ref("u64s"),
            "portfolio_state_before_id": _ref("ContentID"),
            "portfolio_state_after_id": _ref("ContentID"),
            "causation_ids": _array(_ref("CausalDigest"), unique=True),
            "asset_residuals": _array(asset_residual, unique=True),
            "account_residuals": _array(account_residual, unique=True),
            "realized_pnl_residual_quote_atoms": _ref("i128s"),
            "unrealized_pnl_residual_quote_atoms": _ref("i128s"),
            "fee_residual_quote_atoms": _ref("i128s"),
            "peak_equity_residual_quote_atoms": _ref("i128s"),
            "drawdown_residual_bps": _ref("i128s"),
            "equity_residual_quote_atoms": _ref("i128s"),
            "unmatched_reservation_count": _ref("u64s"),
            "idempotency_conflict_scope": {"anyOf": [{"enum": ["RISK", "FILL"]}, {"type": "null"}]},
            "idempotency_key_sha256": _nullable_ref("sha256"),
            "original_object_id": _nullable_ref("ContentID"),
            "conflicting_body_sha256": _nullable_ref("sha256"),
            "integrity_validation_attempt_key_sha256": _nullable_ref("sha256"),
            "integrity_transition_key_sha256": _nullable_ref("sha256"),
            "integrity_expected_footprint_sha256": _nullable_ref("sha256"),
            "integrity_observed_footprint_sha256": _nullable_ref("sha256"),
            "integrity_ledger_head_before_check_id": _nullable_ref("ContentID"),
            "arithmetic_range_key_sha256": _nullable_ref("sha256"),
            "arithmetic_operands_sha256": _nullable_ref("sha256"),
            "arithmetic_operation": {
                "anyOf": [
                    {
                        "enum": [
                            "GROUP_MARK_VALUE",
                            "SUMMARY_EQUITY",
                            "LEDGER_POSTING",
                            "RESIDUAL_SERIALIZATION",
                        ]
                    },
                    {"type": "null"},
                ]
            },
            "kill_latched": {"type": "boolean"},
        },
    )


def run_receipt_schema() -> JsonObject:
    """Return the closed pre-output run-input receipt schema."""
    schema_id = "trading.run-receipt/v1"
    selected_scope = _closed_object(
        {
            "market_id": _ref("bounded_utf8_registry_string"),
            "decision_sequence": _ref("u64s"),
            "horizon_ns": _ref("u64s"),
            "producer_scope_key_sha256": _ref("sha256"),
        }
    )
    availability_group = _closed_object(
        {
            "availability_slot": _ref("u64s"),
            "equal_time_group": _ref("u64s"),
            "admission_cutoff": _ref("u64s"),
        }
    )
    tool_version = _closed_object(
        {
            "name": {"type": "string", "minLength": 1},
            "version": {"type": "string", "minLength": 1},
        }
    )
    source_ids = _array(_ref("ContentID"), unique=True)
    source_ids["minItems"] = 1
    availability_groups = _array(availability_group, unique=True)
    availability_groups["minItems"] = 1
    return _primary_schema(
        schema_id,
        {
            "schema": {"const": schema_id},
            "run_receipt_id": _ref("ContentID"),
            "fixture_manifest_sha256": _ref("ContentID"),
            "schema_bundle_sha256": _ref("sha256"),
            "admission_code_sha256": _ref("sha256"),
            "normalization_code_sha256": _ref("sha256"),
            "availability_grouping_code_sha256": _ref("sha256"),
            "run_closure_code_sha256": _ref("sha256"),
            "feature_code_sha256": _ref("sha256"),
            "baseline_code_sha256": _ref("sha256"),
            "risk_code_sha256": _ref("sha256"),
            "fill_code_sha256": _ref("sha256"),
            "accounting_code_sha256": _ref("sha256"),
            "benchmark_code_sha256": _ref("sha256"),
            "public_seed_sha256": _ref("sha256"),
            "source_tree_sha256": _ref("sha256"),
            "source_admission_receipt_ids": source_ids,
            "availability_schedule_sha256": _ref("sha256"),
            "config_admission_receipt_id": _ref("ContentID"),
            "validated_config_sha256": _nullable_ref("ContentID"),
            "run_closure_receipt_id": _ref("ContentID"),
            "model_registry_sha256": _nullable_ref("ContentID"),
            "model_signal_manifest_sha256": _nullable_ref("ContentID"),
            "selected_model_scopes": _array(selected_scope, unique=True),
            "replay_tick_ns": _ref("u64s"),
            "model_decision_budget_ns": _ref("u64s"),
            "model_signal_mode": {"enum": ["DISABLED", "CACHED_FIXTURES"]},
            "run_end_position_policy": {"enum": ["FORCE_CLOSE_NEXT_EVENT", "LEAVE_MARKED_OPEN"]},
            "terminal_equal_time_group": _ref("u64s"),
            "availability_groups": availability_groups,
            "os_name": {"type": "string", "minLength": 1},
            "architecture": {"type": "string", "minLength": 1},
            "runtime_name": {"type": "string", "minLength": 1},
            "runtime_version": {"type": "string", "minLength": 1},
            "decimal_version": {"type": "string", "minLength": 1},
            "jcs_implementation": {"type": "string", "minLength": 1},
            "jcs_version": {"type": "string", "minLength": 1},
            "tool_versions": _array(tool_version, unique=True),
            "public_seed_hex": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
        },
    )


_BENCHMARK_RANGE_FIELDS = [
    "MEASURED_WALL_DURATION",
    "PROCESS_CPU_TIME",
    "WALL_DURATION",
    "PEAK_RSS_BYTES",
    "ALLOCATION_COUNT",
    "INPUT_BYTES",
    "OUTPUT_BYTES",
]


def benchmark_measurement_schema() -> JsonObject:
    """Return the closed observational benchmark-measurement schema."""
    schema_id = "trading.benchmark-measurement/v1"
    raw_counter = _closed_object(
        {
            "counter": {"enum": _BENCHMARK_RANGE_FIELDS},
            "raw_value": {"anyOf": [_ref("uints"), {"type": "null"}]},
        }
    )
    raw_counters = _array(raw_counter, unique=True, maximum=len(_BENCHMARK_RANGE_FIELDS))
    raw_counters["minItems"] = len(_BENCHMARK_RANGE_FIELDS)
    phase_sample = _closed_object(
        {
            "phase": {"enum": ["admission", "feature", "risk", "fill", "accounting", "end_to_end"]},
            "unit": {
                "enum": [
                    "RAW_EVENT",
                    "FEATURE_SNAPSHOT",
                    "RISK_DECISION",
                    "SIMULATED_FILL_RECEIPT",
                    "RECONCILIATION_RECEIPT",
                    "EQUAL_TIME_GROUP",
                ]
            },
            "sample_count": _ref("u64s"),
            "samples_ns": _array(_ref("u64s")),
        }
    )
    phase_samples = _array(phase_sample, unique=True, maximum=6)
    phase_samples["minItems"] = 6
    return _primary_schema(
        schema_id,
        {
            "schema": {"const": schema_id},
            "benchmark_measurement_id": _ref("ContentID"),
            "measurement_status": {"enum": ["COMPLETE", "RANGE_FAILED"]},
            "first_range_failure": {"anyOf": [{"enum": _BENCHMARK_RANGE_FIELDS}, {"type": "null"}]},
            "range_failure_fields": _array(
                {"enum": _BENCHMARK_RANGE_FIELDS},
                unique=True,
                maximum=len(_BENCHMARK_RANGE_FIELDS),
            ),
            "raw_counters": raw_counters,
            "run_receipt_id": _ref("ContentID"),
            "case_id": {"type": "string", "minLength": 1},
            "repetition_index": _ref("u64s"),
            "benchmark_manifest_sha256": _ref("sha256"),
            "hardware_profile_sha256": _ref("sha256"),
            "measurement_tool_sha256": _ref("sha256"),
            "warmup_event_count": _ref("u64s"),
            "measured_event_count": _ref("u64s"),
            "warmup_group_count": _ref("u64s"),
            "measured_group_count": _ref("u64s"),
            "warmup_through_equal_time_group": _nullable_ref("u64s"),
            "phase_samples": phase_samples,
            "measured_wall_duration_ns": _nullable_ref("u64s"),
            "process_cpu_time_ns": _nullable_ref("u64s"),
            "wall_duration_ns": _nullable_ref("u64s"),
            "peak_rss_bytes": _nullable_ref("u64s"),
            "allocation_count": _nullable_ref("u64s"),
            "input_bytes": _nullable_ref("u64s"),
            "output_bytes": _nullable_ref("u64s"),
        },
    )


def benchmark_receipt_schema() -> JsonObject:
    """Return the closed benchmark admission/terminal-shape receipt schema."""
    schema_id = "trading.benchmark-receipt/v1"
    reason_codes = [
        "BENCHMARK_FIXTURE_NOT_ADMITTED",
        "BENCHMARK_CONFIG_INVALID",
        "BENCHMARK_RUN_CLOSURE_FAILED",
        "BENCHMARK_PREREGISTRATION_MISSING",
        "BENCHMARK_RUN_FAILED",
        "BENCHMARK_METRICS_INVALID",
        "BENCHMARK_REPRODUCIBILITY_FAILED",
        "BENCHMARK_GATE_FAILED",
    ]
    run_output = _closed_object(
        {
            "case_id": {"type": "string", "minLength": 1},
            "repetition_index": _ref("u64s"),
            "terminal_status": {"enum": ["RUN_END", "KILLED", "QUARANTINED", "PROCESS_FAILED"]},
            "run_receipt_id": _ref("ContentID"),
            "ledger_root_id": _nullable_ref("ContentID"),
            "failure_receipt_id": _nullable_ref("ContentID"),
            "execution_quarantine_receipt_id": _nullable_ref("ContentID"),
            "process_failure_code": {
                "anyOf": [
                    {"enum": ["LAUNCH_FAILED", "TIMEOUT", "NONZERO_EXIT", "SIGNALLED", "OUTPUT_INVALID"]},
                    {"type": "null"},
                ]
            },
            "process_exit_code": _nullable_ref("i128s"),
            "stdout_sha256": _nullable_ref("sha256"),
            "stderr_sha256": _nullable_ref("sha256"),
            "output_sha256": _ref("sha256"),
            "measurement_artifact_sha256": _ref("ContentID"),
        }
    )
    return _primary_schema(
        schema_id,
        {
            "schema": {"const": schema_id},
            "benchmark_receipt_id": _ref("ContentID"),
            "status": {"enum": ["PASS", "FAIL", "INELIGIBLE"]},
            "reason_codes": _array({"enum": reason_codes}, unique=True),
            "benchmark_request_sha256": _ref("sha256"),
            "benchmark_manifest_sha256": _nullable_ref("sha256"),
            "preregistered_thresholds_sha256": _nullable_ref("sha256"),
            "hardware_profile_sha256": _nullable_ref("sha256"),
            "metrics_artifact_sha256": _nullable_ref("sha256"),
            "percentile_method": {"const": "NEAREST_RANK_CEIL"},
            "sample_count": _ref("u64s"),
            "run_outputs": _array(run_output, unique=True),
        },
    )


_EXECUTION_PHASES = [
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
]
_FOOTPRINT_COMPONENT_KINDS = [
    "JOURNAL_KEY",
    "CANONICAL_OBJECT",
    "LEDGER_SEQUENCE_START",
    "LEDGER_SEQUENCE_END",
    "CONSUMED_PRODUCER_SEQUENCE",
    "CLAIMED_REQUEST_KEY",
    "PRIOR_LEDGER_HEAD",
    "PRIOR_PORTFOLIO_STATE",
    "RESULTING_LEDGER_HEAD",
    "RESULTING_PORTFOLIO_STATE",
    "NEXT_LEDGER_SEQUENCE",
    "NEXT_STATE_SEQUENCE",
    "NEXT_DECISION_SEQUENCE",
    "NEXT_INTENT_SEQUENCE",
    "NEXT_FILL_RECEIPT_SEQUENCE",
]
_COUNTER_CAPACITY_FIELDS = [
    "DECISION_ATTEMPT",
    "DECISION_SEQUENCE",
    "PRODUCER_SEQUENCE",
    "INTENT_SEQUENCE",
    "FILL_RECEIPT_SEQUENCE",
    "UNMATCHED_RESERVATION_COUNT",
    "STATE_SEQUENCE",
    "LEDGER_SEQUENCE",
]
_ARITHMETIC_OPERATIONS = [
    "GROUP_MARK_VALUE",
    "SUMMARY_EQUITY",
    "LEDGER_POSTING",
    "RESIDUAL_SERIALIZATION",
]


def adverse_fill_draw_key_schema() -> JsonObject:
    schema_id = "trading.adverse-fill-draw-key/v1"
    return _attachment_schema(
        schema_id,
        {
            "schema": {"const": schema_id},
            "run_receipt_id": _ref("ContentID"),
            "intent_id": _ref("ContentID"),
            "fill_event_id": _nullable_ref("ContentID"),
            "decision_equal_time_group": _ref("u64s"),
            "fill_equal_time_group": _nullable_ref("u64s"),
            "requested_base_atoms": _ref("u64s"),
            "max_adverse_fill_bps": {"type": "integer", "minimum": 0, "maximum": 9_999},
        },
    )


def availability_schedule_schema() -> JsonObject:
    schema_id = "trading.availability-schedule/v1"
    availability_group = _closed_object(
        {
            "availability_slot": _ref("u64s"),
            "equal_time_group": _ref("u64s"),
            "admission_cutoff": _ref("u64s"),
        }
    )
    groups = _array(availability_group, unique=True)
    groups["minItems"] = 1
    source_ids = _array(_ref("ContentID"), unique=True)
    source_ids["minItems"] = 1
    return _attachment_schema(
        schema_id,
        {
            "schema": {"const": schema_id},
            "fixture_manifest_sha256": _ref("ContentID"),
            "config_admission_receipt_id": _ref("ContentID"),
            "source_admission_receipt_ids": source_ids,
            "availability_groups": groups,
        },
    )


def benchmark_manifest_schema() -> JsonObject:
    schema_id = "trading.benchmark-manifest/v1"
    case = _closed_object(
        {
            "case_id": {"type": "string", "minLength": 1},
            "fixture_manifest_sha256": _ref("ContentID"),
            "config_admission_receipt_id": _ref("ContentID"),
            "run_closure_receipt_id": _ref("ContentID"),
            "warmup_event_count": _ref("u64s"),
            "measured_event_count": _ref("u64s"),
            "warmup_group_count": _ref("u64s"),
            "measured_group_count": _ref("u64s"),
        }
    )
    return _attachment_schema(
        schema_id,
        {
            "schema": {"const": schema_id},
            "benchmark_manifest_version": {"const": "OFFLINE_REPLAY_CORE_V1"},
            "requested_metric_set": {"const": "OFFLINE_REPLAY_CORE_V1"},
            "case_count": _ref("u64s"),
            "repetition_count": _ref("u64s"),
            "cases": _array(case, unique=True),
        },
    )


def benchmark_metrics_schema() -> JsonObject:
    schema_id = "trading.benchmark-metrics/v1"
    metric = _closed_object(
        {
            "case_id": {"type": "string", "minLength": 1},
            "phase": {"enum": ["admission", "feature", "risk", "fill", "accounting", "end_to_end"]},
            "unit": {
                "enum": [
                    "RAW_EVENT",
                    "FEATURE_SNAPSHOT",
                    "RISK_DECISION",
                    "SIMULATED_FILL_RECEIPT",
                    "RECONCILIATION_RECEIPT",
                    "EQUAL_TIME_GROUP",
                ]
            },
            "sample_count": _ref("u64s"),
            "p50_ns": _nullable_ref("u64s"),
            "p95_ns": _nullable_ref("u64s"),
            "p99_ns": _nullable_ref("u64s"),
            "max_ns": _nullable_ref("u64s"),
        }
    )
    return _attachment_schema(
        schema_id,
        {
            "schema": {"const": schema_id},
            "benchmark_manifest_sha256": _ref("sha256"),
            "hardware_profile_sha256": _ref("sha256"),
            "measurement_count": _ref("u64s"),
            "metrics": _array(metric, unique=True),
        },
    )


def benchmark_request_schema() -> JsonObject:
    schema_id = "trading.benchmark-request/v1"
    return _attachment_schema(
        schema_id,
        {
            "schema": {"const": schema_id},
            "request_code_sha256": _ref("sha256"),
            "fixture_manifest_sha256": _nullable_ref("ContentID"),
            "config_admission_receipt_id": _nullable_ref("ContentID"),
            "run_closure_receipt_id": _nullable_ref("ContentID"),
            "benchmark_manifest_raw_sha256": _nullable_ref("sha256"),
            "preregistered_thresholds_raw_sha256": _nullable_ref("sha256"),
            "hardware_profile_raw_sha256": _nullable_ref("sha256"),
            "benchmark_manifest_raw_byte_length": _ref("u64s"),
            "preregistered_thresholds_raw_byte_length": _ref("u64s"),
            "hardware_profile_raw_byte_length": _ref("u64s"),
            "requested_metric_set": {"const": "OFFLINE_REPLAY_CORE_V1"},
        },
    )


def counter_capacity_schema() -> JsonObject:
    schema_id = "trading.counter-capacity/v1"
    nullable_counter = _nullable_ref("u64s")
    return _attachment_schema(
        schema_id,
        {
            "schema": {"const": schema_id},
            "model_signal_mode": {"enum": ["DISABLED", "CACHED_FIXTURES"]},
            "model_registry_sha256": _nullable_ref("ContentID"),
            "model_signal_manifest_sha256": _nullable_ref("ContentID"),
            "normalized_event_set_sha256": _ref("sha256"),
            "source_admission_count": _ref("u64s"),
            "raw_event_count": _ref("u64s"),
            "availability_group_count": _ref("u64s"),
            "market_count": _ref("u64s"),
            "model_candidate_count": _ref("u64s"),
            "decision_attempt_upper_bound": nullable_counter,
            "decision_sequence_next_upper_bound": nullable_counter,
            "producer_sequence_next_upper_bound": nullable_counter,
            "intent_sequence_next_upper_bound": nullable_counter,
            "fill_receipt_sequence_next_upper_bound": nullable_counter,
            "unmatched_reservation_count_upper_bound": nullable_counter,
            "state_sequence_next_upper_bound": nullable_counter,
            "ledger_sequence_next_upper_bound": nullable_counter,
            "status": {"enum": ["WITHIN_LIMIT", "EXCEEDED"]},
            "first_exceeded_counter": {"anyOf": [{"enum": _COUNTER_CAPACITY_FIELDS}, {"type": "null"}]},
        },
    )


def execution_footprint_component_schema() -> JsonObject:
    schema_id = "trading.execution-footprint-component/v1"
    return _attachment_schema(
        schema_id,
        {
            "schema": {"const": schema_id},
            "component_kind": {"enum": _FOOTPRINT_COMPONENT_KINDS},
            "ordinal": _nullable_ref("u64s"),
            "component_sha256": _nullable_ref("sha256"),
            "object_id": _nullable_ref("ContentID"),
            "byte_sha256": _nullable_ref("sha256"),
            "u64_value": _nullable_ref("u64s"),
        },
    )


def execution_transition_footprint_schema() -> JsonObject:
    schema_id = "trading.execution-transition-footprint/v1"
    component = _closed_object(
        {
            "component_kind": {"enum": _FOOTPRINT_COMPONENT_KINDS},
            "ordinal": _nullable_ref("u64s"),
            "component_sha256": _nullable_ref("sha256"),
        }
    )
    return _attachment_schema(
        schema_id,
        {
            "schema": {"const": schema_id},
            "transition_key_sha256": _ref("sha256"),
            "ledger_head_before_id": _nullable_ref("ContentID"),
            "ledger_head_after_id": _nullable_ref("ContentID"),
            "portfolio_state_before_id": _nullable_ref("ContentID"),
            "portfolio_state_after_id": _nullable_ref("ContentID"),
            "components": _array(component, unique=True),
        },
    )


def execution_transition_key_schema() -> JsonObject:
    schema_id = "trading.execution-transition-key/v1"
    return _attachment_schema(
        schema_id,
        {
            "schema": {"const": schema_id},
            "run_receipt_id": _ref("ContentID"),
            "equal_time_group": _ref("u64s"),
            "phase": {"enum": _EXECUTION_PHASES},
            "market_id": _nullable_ref("bounded_utf8_registry_string"),
            "item_sequence": _nullable_ref("u64s"),
        },
    )


def fill_idempotency_key_schema() -> JsonObject:
    schema_id = "trading.fill-idempotency-key/v1"
    return _attachment_schema(schema_id, {"schema": {"const": schema_id}, "intent_id": _ref("ContentID")})


def force_close_state_envelope_schema() -> JsonObject:
    schema_id = "trading.force-close-state-envelope/v1"
    fields = (
        "decision_attempt_upper_bound",
        "position_quantity_base_atoms_max",
        "position_cost_basis_quote_atoms_max",
        "all_position_cost_basis_quote_atoms_max",
        "single_fill_gross_quote_atoms_max",
        "single_fill_fee_quote_atoms_max",
        "all_market_value_quote_atoms_max",
        "pre_quote_total_atoms_max",
        "post_quote_total_atoms_max",
        "pre_realized_pnl_abs_max",
        "post_realized_pnl_abs_max",
        "pre_unrealized_pnl_abs_max",
        "post_unrealized_pnl_abs_max",
        "pre_session_pnl_abs_max",
        "post_session_pnl_abs_max",
        "pre_cumulative_fees_quote_atoms_max",
        "post_cumulative_fees_quote_atoms_max",
        "pre_equity_quote_atoms_max",
        "post_equity_quote_atoms_max",
        "pre_peak_equity_quote_atoms_max",
        "post_peak_equity_quote_atoms_max",
        "ledger_posting_abs_max",
    )
    properties = {field: _ref("u64s") for field in fields}
    properties.update(
        {
            "schema": {"const": schema_id},
            "market_id": _ref("bounded_utf8_registry_string"),
            "terminal_horizon_equal_time_group": _ref("u64s"),
        }
    )
    return _attachment_schema(schema_id, properties)


def group_mark_key_schema() -> JsonObject:
    schema_id = "trading.group-mark-key/v1"
    return _attachment_schema(
        schema_id,
        {
            "schema": {"const": schema_id},
            "run_receipt_id": _ref("ContentID"),
            "equal_time_group": _ref("u64s"),
            "as_of_ingest_sequence": _ref("u64s"),
            "portfolio_state_before_id": _ref("ContentID"),
        },
    )


def hardware_profile_schema() -> JsonObject:
    schema_id = "trading.hardware-profile/v1"
    positive_count: JsonObject = {"type": "string", "pattern": "^[1-9][0-9]*$"}
    return _attachment_schema(
        schema_id,
        {
            "schema": {"const": schema_id},
            "os_name": {"type": "string", "minLength": 1},
            "os_version": {"type": "string", "minLength": 1},
            "architecture": {"type": "string", "minLength": 1},
            "cpu_vendor": {"type": "string", "minLength": 1},
            "cpu_model": {"type": "string", "minLength": 1},
            "runtime_isolation": {"type": "string", "minLength": 1},
            "logical_cpu_count": positive_count,
            "physical_memory_bytes": positive_count,
            "timer_resolution_ns": positive_count,
            "virtualization": {"enum": ["NONE_DECLARED", "UNKNOWN_DECLARED", "HYPERVISOR_DECLARED"]},
            "power_profile": {"enum": ["UNKNOWN_DECLARED", "BATTERY", "BALANCED", "HIGH_PERFORMANCE"]},
            "timer_source": {"enum": ["PERF_COUNTER", "MONOTONIC", "PROCESS_TIME"]},
        },
    )


def model_attempt_footprint_schema() -> JsonObject:
    schema_id = "trading.model-attempt-footprint/v1"
    return _attachment_schema(
        schema_id,
        {
            "schema": {"const": schema_id},
            "validation_attempt_key_sha256": _ref("sha256"),
            "model_signal_manifest_sha256": _ref("ContentID"),
            "raw_signal_sha256": _ref("sha256"),
            "declared_signal_id": _nullable_ref("ContentID"),
            "accepted_signal_id": _nullable_ref("ContentID"),
            "feature_snapshot_id": _ref("ContentID"),
            "decision_sequence": _ref("u64s"),
            "producer_sequence": _nullable_ref("u64s"),
            "producer_scope_key_sha256": _nullable_ref("sha256"),
        },
    )


def model_request_key_schema() -> JsonObject:
    schema_id = "trading.model-request-key/v1"
    return _attachment_schema(
        schema_id,
        {
            "schema": {"const": schema_id},
            "run_receipt_id": _ref("ContentID"),
            "market_id": _ref("bounded_utf8_registry_string"),
            "decision_sequence": _ref("u64s"),
            "horizon_ns": _ref("u64s"),
            "producer_scope_key_sha256": _ref("sha256"),
        },
    )


def model_validation_attempt_key_schema() -> JsonObject:
    schema_id = "trading.model-validation-attempt-key/v1"
    return _attachment_schema(
        schema_id,
        {
            "schema": {"const": schema_id},
            "model_signal_manifest_sha256": _ref("ContentID"),
            "relative_path": _ref("safe_relative_path"),
            "raw_signal_sha256": _ref("sha256"),
            "feature_snapshot_id": _ref("ContentID"),
            "decision_sequence": _ref("u64s"),
            "requested_producer_scope_key_sha256": _ref("sha256"),
            "horizon_ns": _ref("u64s"),
        },
    )


def normalized_event_set_schema() -> JsonObject:
    schema_id = "trading.normalized-event-set/v1"
    event_ids = _array(_ref("ContentID"), unique=True)
    event_ids["minItems"] = 1
    return _attachment_schema(
        schema_id,
        {
            "schema": {"const": schema_id},
            "fixture_manifest_sha256": _ref("ContentID"),
            "raw_event_count": _ref("u64s"),
            "event_ids": event_ids,
        },
    )


def preregistered_thresholds_schema() -> JsonObject:
    schema_id = "trading.preregistered-thresholds/v1"
    threshold = _closed_object(
        {
            "metric_name": {"type": "string", "minLength": 1},
            "maximum_value": _nullable_ref("u64s"),
            "minimum_value": _nullable_ref("u64s"),
            "unit": {"enum": ["ns", "bytes", "count", "bps", "ratio_q18"]},
        }
    )
    return _attachment_schema(
        schema_id,
        {
            "schema": {"const": schema_id},
            "requested_metric_set": {"const": "OFFLINE_REPLAY_CORE_V1"},
            "percentile_method": {"const": "NEAREST_RANK_CEIL"},
            "thresholds": _array(threshold, unique=True),
        },
    )


def reconciliation_arithmetic_operands_schema() -> JsonObject:
    schema_id = "trading.reconciliation-arithmetic-operands/v1"
    operand = _closed_object(
        {
            "operand_name": {"type": "string", "minLength": 1},
            "operand_value": _ref("i128s"),
        }
    )
    return _attachment_schema(
        schema_id,
        {
            "schema": {"const": schema_id},
            "arithmetic_operation": {"enum": _ARITHMETIC_OPERATIONS},
            "operands": _array(operand),
        },
    )


def reconciliation_arithmetic_range_key_schema() -> JsonObject:
    schema_id = "trading.reconciliation-arithmetic-range-key/v1"
    return _attachment_schema(
        schema_id,
        {
            "schema": {"const": schema_id},
            "run_receipt_id": _ref("ContentID"),
            "portfolio_state_before_id": _ref("ContentID"),
            "ingest_sequence": _ref("u64s"),
            "equal_time_group": _ref("u64s"),
            "replay_clock_ns": _ref("u64s"),
            "arithmetic_operation": {"enum": _ARITHMETIC_OPERATIONS},
            "arithmetic_operands_sha256": _ref("sha256"),
        },
    )


def reservation_key_schema() -> JsonObject:
    schema_id = "trading.reservation-key/v1"
    return _attachment_schema(
        schema_id,
        {
            "schema": {"const": schema_id},
            "run_receipt_id": _ref("ContentID"),
            "market_id": _ref("bounded_utf8_registry_string"),
            "intent_id": _nullable_ref("ContentID"),
            "risk_decision_id": _nullable_ref("ContentID"),
            "equal_time_group": _ref("u64s"),
            "reservation_scope": {"enum": ["QUOTE_FOR_BUY", "BASE_FOR_CLOSE_LONG"]},
        },
    )


def risk_idempotency_key_schema() -> JsonObject:
    schema_id = "trading.risk-idempotency-key/v1"
    return _attachment_schema(
        schema_id,
        {
            "schema": {"const": schema_id},
            "run_receipt_id": _ref("ContentID"),
            "equal_time_group": _ref("u64s"),
            "market_id": _ref("bounded_utf8_registry_string"),
        },
    )


def run_closure_fill_candidate_semantic_schema() -> JsonObject:
    schema_id = "trading.run-closure-fill-candidate-semantic/v1"
    nonnegative_bps = {"type": "integer", "minimum": 0, "maximum": 2_147_483_647}
    return _attachment_schema(
        schema_id,
        {
            "schema": {"const": schema_id},
            "event_kind": {"const": "ROUTE_QUOTE"},
            "market_id": _ref("bounded_utf8_registry_string"),
            "base_mint": _ref("bounded_utf8_registry_string"),
            "quote_mint": _ref("bounded_utf8_registry_string"),
            "base_decimals": {"type": "integer", "minimum": 0, "maximum": 18},
            "quote_decimals": {"type": "integer", "minimum": 0, "maximum": 18},
            "base_amount_atoms": _ref("u64s"),
            "quote_amount_atoms": _ref("u64s"),
            "route_capacity_base_atoms": _ref("u64s"),
            "liquidity_quote_atoms": _ref("u64s"),
            "venue_fee_quote_atoms": _ref("u64s"),
            "priority_fee_quote_atoms": _ref("u64s"),
            "route_impact_bps": nonnegative_bps,
            "executable": {"type": "boolean"},
            "quality_flags": _array({"enum": _QUALITY_FLAGS}, unique=True),
        },
    )


def run_closure_full_fill_proof_row_schema() -> JsonObject:
    schema_id = "trading.run-closure-full-fill-proof-row/v1"
    nonnegative_bps = {"type": "integer", "minimum": 0, "maximum": 2_147_483_647}
    return _attachment_schema(
        schema_id,
        {
            "schema": {"const": schema_id},
            "residual_base_atoms": _ref("u64s"),
            "reference_price_q18": _ref("sq18s"),
            "capacity_atoms": _ref("u64s"),
            "filled_base_atoms": _ref("u64s"),
            "unfilled_base_atoms": _ref("u64s"),
            "gross_quote_atoms": _ref("u64s"),
            "venue_fee_quote_atoms": _ref("u64s"),
            "priority_fee_quote_atoms": _ref("u64s"),
            "simulation_fee_quote_atoms": _ref("u64s"),
            "cash_delta_quote_atoms": _ref("i128s"),
            "execution_price_q18": _ref("sq18s"),
            "participation_bps": nonnegative_bps,
            "reference_deviation_bps": nonnegative_bps,
            "impact_bps": nonnegative_bps,
            "fee_bps": nonnegative_bps,
            "adverse_fill_bps": nonnegative_bps,
            "status": {"const": "FILLED"},
            "reason_codes": _array({"enum": _FILL_REASON_CODES}, unique=True),
        },
    )


def run_closure_full_fill_proof_set_schema() -> JsonObject:
    schema_id = "trading.run-closure-full-fill-proof-set/v1"
    extremes = _array({"type": "integer", "minimum": 0, "maximum": 9_999}, unique=True)
    extremes["minItems"] = 2
    rows = _array(
        {"$ref": json_schema_id("trading.run-closure-full-fill-proof-row/v1")},
        maximum=_MAX_RUN_CLOSURE_PROOF_ROWS_V0,
    )
    rows["minItems"] = 1
    return _attachment_schema(
        schema_id,
        {
            "schema": {"const": schema_id},
            "market_id": _ref("bounded_utf8_registry_string"),
            "earliest_trigger_equal_time_group": _ref("u64s"),
            "fill_candidate_semantic_sha256": _ref("sha256"),
            "q_cap_base_atoms": _ref("u64s"),
            "reference_set_root_sha256": _ref("sha256"),
            "adverse_fill_extremes_bps": extremes,
            "proof_domain": {"const": "ALL_RESIDUAL_REFERENCE_PAIRS_AT_FILL_EXTREMES_V1"},
            "proof_row_count": _ref("u64s"),
            "rows": rows,
        },
    )


def run_closure_reference_set_schema() -> JsonObject:
    schema_id = "trading.run-closure-reference-set/v1"
    prices = _array(_ref("sq18s"), unique=True)
    prices["minItems"] = 1
    return _attachment_schema(
        schema_id,
        {
            "schema": {"const": schema_id},
            "market_id": _ref("bounded_utf8_registry_string"),
            "reference_prices_q18": prices,
        },
    )


def source_tree_schema() -> JsonObject:
    schema_id = "trading.source-tree/v1"
    exclusion_prefix: JsonObject = {
        "type": "string",
        "pattern": (
            r"^(?![A-Za-z]:)(?!/)(?!\.{1,2}/)(?!.*(?:/\.{1,2})(?:/|$))(?!.*//)"
            r"[^/\\:\x00]+(?:/[^/\\:\x00]+)*/$"
        ),
    }
    file_row = _closed_object(
        {
            "relative_path": _ref("safe_relative_path"),
            "byte_length": _ref("u64s"),
            "file_sha256": _ref("sha256"),
        }
    )
    return _attachment_schema(
        schema_id,
        {
            "schema": {"const": schema_id},
            "root_label": {"type": "string", "minLength": 1},
            "path_policy": {"const": "UTF8_NFC_POSIX_RELATIVE_NO_SYMLINK_V1"},
            "exclusion_prefixes": _array(exclusion_prefix, unique=True),
            "files": _array(file_row, unique=True),
        },
    )


PRIMARY_SCHEMA_DOCUMENTS: dict[str, JsonObject] = {
    "trading.raw-event/v1": raw_event_schema(),
    "trading.feature-snapshot/v1": feature_snapshot_schema(),
    "trading.model-signal/v1": model_signal_schema(),
    "trading.risk-decision/v1": risk_decision_schema(),
    "trading.simulated-order-intent/v1": simulated_order_intent_schema(),
    "trading.simulated-fill-receipt/v1": simulated_fill_receipt_schema(),
    "trading.portfolio-state/v1": portfolio_state_schema(),
    "trading.ledger-record/v1": ledger_record_schema(),
}
SUPPORTING_SCHEMA_DOCUMENTS: dict[str, JsonObject] = {
    "trading.fixture-manifest/v1": fixture_manifest_schema(),
    "trading.replay-risk-config/v1": replay_risk_config_schema(),
    "trading.source-admission-receipt/v1": source_admission_receipt_schema(),
    "trading.config-admission-receipt/v1": config_admission_receipt_schema(),
    "trading.run-closure-receipt/v1": run_closure_receipt_schema(),
    "trading.execution-quarantine-receipt/v1": execution_quarantine_receipt_schema(),
    "trading.model-registry/v1": model_registry_schema(),
    "trading.model-signal-manifest/v1": model_signal_manifest_schema(),
    "trading.model-validation-receipt/v1": model_validation_receipt_schema(),
    "trading.reconciliation-receipt/v1": reconciliation_receipt_schema(),
    "trading.run-receipt/v1": run_receipt_schema(),
    "trading.benchmark-measurement/v1": benchmark_measurement_schema(),
    "trading.benchmark-receipt/v1": benchmark_receipt_schema(),
}
ATTACHMENT_SCHEMA_DOCUMENTS: dict[str, JsonObject] = {
    "trading.adverse-fill-draw-key/v1": adverse_fill_draw_key_schema(),
    "trading.availability-schedule/v1": availability_schedule_schema(),
    "trading.benchmark-manifest/v1": benchmark_manifest_schema(),
    "trading.benchmark-metrics/v1": benchmark_metrics_schema(),
    "trading.benchmark-request/v1": benchmark_request_schema(),
    "trading.counter-capacity/v1": counter_capacity_schema(),
    "trading.execution-footprint-component/v1": execution_footprint_component_schema(),
    "trading.execution-transition-footprint/v1": execution_transition_footprint_schema(),
    "trading.execution-transition-key/v1": execution_transition_key_schema(),
    "trading.fill-idempotency-key/v1": fill_idempotency_key_schema(),
    "trading.force-close-state-envelope/v1": force_close_state_envelope_schema(),
    "trading.group-mark-key/v1": group_mark_key_schema(),
    "trading.hardware-profile/v1": hardware_profile_schema(),
    "trading.model-attempt-footprint/v1": model_attempt_footprint_schema(),
    "trading.model-request-key/v1": model_request_key_schema(),
    "trading.model-validation-attempt-key/v1": model_validation_attempt_key_schema(),
    "trading.normalized-event-set/v1": normalized_event_set_schema(),
    "trading.preregistered-thresholds/v1": preregistered_thresholds_schema(),
    "trading.reconciliation-arithmetic-operands/v1": reconciliation_arithmetic_operands_schema(),
    "trading.reconciliation-arithmetic-range-key/v1": reconciliation_arithmetic_range_key_schema(),
    "trading.reservation-key/v1": reservation_key_schema(),
    "trading.risk-idempotency-key/v1": risk_idempotency_key_schema(),
    "trading.run-closure-fill-candidate-semantic/v1": run_closure_fill_candidate_semantic_schema(),
    "trading.run-closure-full-fill-proof-row/v1": run_closure_full_fill_proof_row_schema(),
    "trading.run-closure-full-fill-proof-set/v1": run_closure_full_fill_proof_set_schema(),
    "trading.run-closure-reference-set/v1": run_closure_reference_set_schema(),
    "trading.source-tree/v1": source_tree_schema(),
}


def get_defined_schema_documents() -> dict[str, JsonObject]:
    """Return a fresh combined mapping after checking inventory ownership."""
    combined: dict[str, JsonObject] = {}
    families = (
        (PRIMARY_SCHEMA_DOCUMENTS, frozenset(PRIMARY_SCHEMA_IDS), "PRIMARY"),
        (SUPPORTING_SCHEMA_DOCUMENTS, frozenset(SUPPORTING_SCHEMA_IDS), "SUPPORTING"),
        (ATTACHMENT_SCHEMA_DOCUMENTS, frozenset(ATTACHMENT_SCHEMA_IDS), "ATTACHMENT"),
    )
    for documents, allowed, family in families:
        unknown = set(documents).difference(allowed)
        if unknown:
            raise ValueError(f"{family} schema definitions contain unknown contracts: {sorted(unknown)!r}")
        overlap = set(documents).intersection(combined)
        if overlap:
            raise ValueError(f"duplicate schema definitions: {sorted(overlap)!r}")
        combined.update(documents)
    return combined
