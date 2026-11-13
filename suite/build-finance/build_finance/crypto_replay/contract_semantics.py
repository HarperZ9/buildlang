"""Pure cross-field semantics for primary and supporting replay contracts.

JSON Schema owns lexical shape.  These validators own only relationships
that the generated schema subset cannot express.  They perform no I/O and do
not resolve content identifiers.
"""

from __future__ import annotations

import re
import unicodedata
from collections.abc import Callable, Iterable, Mapping
from datetime import datetime
from itertools import pairwise
from typing import Any, Literal

from build_finance.crypto_replay.canonical import JsonValue, canonical_json_bytes, sha256_hex
from build_finance.crypto_replay.formats import parse_bounded_decimal_string
from build_finance.crypto_replay.schema_definitions import ATTACHMENT_SCHEMA_DOCUMENTS
from build_finance.crypto_replay.schema_model import ValidationIssue

_MAX_U64 = 18_446_744_073_709_551_615
_MIN_I128 = -170_141_183_460_469_231_731_687_303_715_884_105_728
_MAX_I128 = 170_141_183_460_469_231_731_687_303_715_884_105_727
_RFC3339_NS_UTC = re.compile(
    r"(?P<year>[0-9]{4})-(?P<month>[0-9]{2})-(?P<day>[0-9]{2})"
    r"T(?P<hour>[0-9]{2}):(?P<minute>[0-9]{2}):(?P<second>[0-9]{2})"
    r"\.(?P<nanosecond>[0-9]{9})Z\Z"
)
_RFC3339_FULL_DATE = re.compile(r"(?P<year>[0-9]{4})-(?P<month>[0-9]{2})-(?P<day>[0-9]{2})\Z")

SOURCE_ADMISSION_REASON_PRECEDENCE = (
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
)
SOURCE_ADMISSION_QUARANTINE_REASONS = frozenset(SOURCE_ADMISSION_REASON_PRECEDENCE[8:])
CONFIG_ADMISSION_REASON_PRECEDENCE = (
    "CONFIG_MISSING",
    "CONFIG_BYTES_INVALID",
    "CONFIG_SCHEMA_INVALID",
    "CONFIG_ID_MISMATCH",
    "CONFIG_RANGE_INVALID",
)
RUN_CLOSURE_REASON_PRECEDENCE = (
    "ADMISSION_COUNTER_CAPACITY",
    "ADMISSION_RUN_END_PROOF_BUDGET",
    "ADMISSION_RUN_END_UNCLOSED",
)
RUN_CLOSURE_MARKET_REASON_PRECEDENCE = (
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
QUARANTINE_REASON_PRECEDENCE = (
    "QUARANTINE_INITIAL_PREFIX_ANCHOR_MISSING",
    "QUARANTINE_LEDGER_SLOT_MISSING",
    "QUARANTINE_LEDGER_SLOT_WRONG",
    "QUARANTINE_LEDGER_SLOT_EXTRA",
    "QUARANTINE_APPEND_SLOT_OCCUPIED",
    "QUARANTINE_COMPONENT_MISSING",
    "QUARANTINE_COMPONENT_WRONG",
    "QUARANTINE_COMPONENT_EXTRA",
    "QUARANTINE_FOOTPRINT_MISMATCH",
)
QUARANTINE_COMPONENT_KIND_PRECEDENCE = (
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
)
MODEL_VALIDATION_REASON_PRECEDENCE = (
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
)
RECONCILIATION_REASON_PRECEDENCE = (
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
)
BENCHMARK_REASON_PRECEDENCE = (
    "BENCHMARK_FIXTURE_NOT_ADMITTED",
    "BENCHMARK_CONFIG_INVALID",
    "BENCHMARK_RUN_CLOSURE_FAILED",
    "BENCHMARK_PREREGISTRATION_MISSING",
    "BENCHMARK_RUN_FAILED",
    "BENCHMARK_METRICS_INVALID",
    "BENCHMARK_REPRODUCIBILITY_FAILED",
    "BENCHMARK_GATE_FAILED",
)
BENCHMARK_RANGE_FIELD_PRECEDENCE = (
    "MEASURED_WALL_DURATION",
    "PROCESS_CPU_TIME",
    "WALL_DURATION",
    "PEAK_RSS_BYTES",
    "ALLOCATION_COUNT",
    "INPUT_BYTES",
    "OUTPUT_BYTES",
)
_MAX_RUN_CLOSURE_PROOF_ROWS_V0 = 1_000_000
_Q18_UNIT = 1_000_000_000_000_000_000

SemanticValidator = Callable[[Mapping[str, JsonValue]], tuple[ValidationIssue, ...]]


def _issue(code: str, path: tuple[str | int, ...], message: str) -> ValidationIssue:
    return ValidationIssue(code=code, path=path, message=message)


def _u64(value: object) -> int | None:
    try:
        return parse_bounded_decimal_string(value, minimum=0, maximum=_MAX_U64)
    except ValueError:
        return None


def _uint(value: object) -> int | None:
    if not isinstance(value, str) or not value or (value != "0" and value.startswith("0")) or not value.isascii():
        return None
    if any(character < "0" or character > "9" for character in value):
        return None
    if len(value) > len(str(_MAX_U64)):
        return _MAX_U64 + 1
    return int(value)


def _i128(value: object) -> int | None:
    try:
        return parse_bounded_decimal_string(value, minimum=_MIN_I128, maximum=_MAX_I128)
    except ValueError:
        return None


def _valid_utf8_registry(value: object, maximum_bytes: int) -> bool:
    if not isinstance(value, str) or not value:
        return False
    try:
        return len(value.encode("utf-8", errors="strict")) <= maximum_bytes
    except UnicodeEncodeError:
        return False


def _instant_key(value: object) -> tuple[int, ...] | None:
    if not isinstance(value, str):
        return None
    match = _RFC3339_NS_UTC.fullmatch(value)
    if match is None:
        return None
    parts = tuple(int(match.group(name)) for name in ("year", "month", "day", "hour", "minute", "second"))
    try:
        datetime(parts[0], parts[1], parts[2], parts[3], parts[4], parts[5])
    except ValueError:
        return None
    return (*parts, int(match.group("nanosecond")))


def _full_date_key(value: object) -> tuple[int, int, int] | None:
    if not isinstance(value, str):
        return None
    match = _RFC3339_FULL_DATE.fullmatch(value)
    if match is None:
        return None
    parts = tuple(int(match.group(name)) for name in ("year", "month", "day"))
    try:
        datetime(parts[0], parts[1], parts[2])
    except ValueError:
        return None
    return (parts[0], parts[1], parts[2])


def _sorted_unique(values: object, *, key: Callable[[Any], Any]) -> bool:
    return isinstance(values, list) and len(values) == len(set(values)) and values == sorted(values, key=key)


def _normalize_reason_codes(
    reason_codes: Iterable[str],
    precedence: tuple[str, ...],
) -> tuple[str, ...]:
    codes = tuple(reason_codes)
    if any(not isinstance(code, str) for code in codes):
        raise ValueError("reason code is not a string")
    if len(codes) != len(set(codes)):
        raise ValueError("duplicate reason code")
    unknown = tuple(code for code in codes if code not in precedence)
    if unknown:
        raise ValueError(f"unknown reason code: {unknown[0]}")
    order = {code: index for index, code in enumerate(precedence)}
    return tuple(sorted(codes, key=order.__getitem__))


def normalize_source_admission_reason_codes(reason_codes: Iterable[str]) -> tuple[str, ...]:
    """Return the unique closed source reason set in normative precedence."""
    return _normalize_reason_codes(reason_codes, SOURCE_ADMISSION_REASON_PRECEDENCE)


def derive_source_admission_status(
    reason_codes: Iterable[str],
) -> Literal["ADMITTED", "REJECTED", "QUARANTINED"]:
    """Derive source status from only the complete closed reason set."""
    normalized = normalize_source_admission_reason_codes(reason_codes)
    if not normalized:
        return "ADMITTED"
    if SOURCE_ADMISSION_QUARANTINE_REASONS.intersection(normalized):
        return "QUARANTINED"
    return "REJECTED"


def normalize_config_admission_reason_codes(reason_codes: Iterable[str]) -> tuple[str, ...]:
    """Return the unique closed config reason set in normative precedence."""
    return _normalize_reason_codes(reason_codes, CONFIG_ADMISSION_REASON_PRECEDENCE)


def validate_raw_event_semantics(document: Mapping[str, JsonValue]) -> tuple[ValidationIssue, ...]:
    """Validate RawEvent revision, availability, quality, and time semantics."""
    issues: list[ValidationIssue] = []
    registry_paths = (
        ("source_id",),
        ("source_kind",),
        ("source_revision",),
        ("market_id",),
        ("base_mint",),
        ("quote_mint",),
    )
    for registry_path in registry_paths:
        if not _valid_utf8_registry(document[registry_path[0]], 128):
            issues.append(_issue("semantic_registry", registry_path, "registry string exceeds 128 UTF-8 bytes"))

    source_position = document["source_position"]
    revision = document["revision"]
    market = document["market"]
    assert isinstance(source_position, Mapping) and isinstance(revision, Mapping) and isinstance(market, Mapping)
    if not _valid_utf8_registry(source_position["source_native_event_id"], 256):
        issues.append(
            _issue(
                "semantic_registry",
                ("source_position", "source_native_event_id"),
                "native event ID exceeds 256 UTF-8 bytes",
            )
        )

    u64_paths: tuple[tuple[str, ...], ...] = (
        ("source_position", "slot"),
        ("source_position", "source_subsequence"),
        ("revision", "availability_slot"),
        ("revision", "availability_admission_sequence"),
        ("admission_sequence",),
        ("source_sequence",),
        ("ingest_sequence",),
        ("equal_time_group",),
        ("replay_clock_ns",),
        ("market", "base_amount_atoms"),
        ("market", "quote_amount_atoms"),
        ("market", "route_capacity_base_atoms"),
        ("market", "liquidity_quote_atoms"),
        ("market", "venue_fee_quote_atoms"),
        ("market", "priority_fee_quote_atoms"),
    )
    parsed_u64: dict[tuple[str, ...], int] = {}
    for u64_path in u64_paths:
        value: object = document[u64_path[0]]
        if len(u64_path) == 2:
            assert isinstance(value, Mapping)
            value = value[u64_path[1]]
        parsed = _u64(value)
        if parsed is None:
            issues.append(_issue("semantic_u64", u64_path, "value exceeds the u64 authority range"))
        else:
            parsed_u64[u64_path] = parsed

    if document["base_mint"] == document["quote_mint"]:
        issues.append(_issue("semantic_mints", ("base_mint",), "base and quote mints must differ"))

    kind = revision["kind"]
    supersedes = revision["supersedes_event_id"]
    retracts = revision["retracts_event_id"]
    flags = document["quality_flags"]
    assert isinstance(flags, list)
    if kind == "ORIGINAL":
        if supersedes is not None or retracts is not None:
            issues.append(_issue("semantic_revision", ("revision",), "ORIGINAL requires both target IDs null"))
        availability = parsed_u64.get(("revision", "availability_slot"))
        slot = parsed_u64.get(("source_position", "slot"))
        if availability is not None and slot is not None and availability != slot:
            issues.append(
                _issue(
                    "semantic_availability",
                    ("revision", "availability_slot"),
                    "ORIGINAL availability slot must equal its source slot",
                )
            )
    elif kind == "CORRECTION":
        if supersedes is None or retracts is not None:
            issues.append(_issue("semantic_revision", ("revision",), "CORRECTION requires only supersedes_event_id"))
        if "PROVIDER_REVISION" not in flags:
            issues.append(_issue("semantic_revision", ("quality_flags",), "CORRECTION requires PROVIDER_REVISION"))
    elif kind == "RETRACTION":
        if retracts is None or supersedes is not None:
            issues.append(_issue("semantic_revision", ("revision",), "RETRACTION requires only retracts_event_id"))
        if "RETRACTED_SOURCE" not in flags or document["executable"] is not False:
            issues.append(
                _issue(
                    "semantic_revision",
                    ("quality_flags",),
                    "RETRACTION requires RETRACTED_SOURCE and non-executable status",
                )
            )

    availability_admission = parsed_u64.get(("revision", "availability_admission_sequence"))
    admission = parsed_u64.get(("admission_sequence",))
    if availability_admission is not None and admission is not None and availability_admission != admission:
        issues.append(
            _issue(
                "semantic_availability",
                ("revision", "availability_admission_sequence"),
                "revision availability admission sequence must equal admission_sequence",
            )
        )

    if not _sorted_unique(flags, key=lambda value: value.encode("utf-8")):
        issues.append(_issue("semantic_order", ("quality_flags",), "quality flags must be UTF-8 sorted and unique"))
    missing_time = "MISSING_EVENT_TIME" in flags
    if (document["event_time"] is None) != missing_time:
        issues.append(
            _issue(
                "semantic_event_time",
                ("event_time",),
                "event_time nullability must match MISSING_EVENT_TIME",
            )
        )

    event_key = None if document["event_time"] is None else _instant_key(document["event_time"])
    observed_key = _instant_key(document["observed_at"])
    ingested_key = _instant_key(document["ingested_at"])
    if observed_key is None:
        issues.append(_issue("semantic_time", ("observed_at",), "invalid RFC 3339 nanosecond UTC instant"))
    if ingested_key is None:
        issues.append(_issue("semantic_time", ("ingested_at",), "invalid RFC 3339 nanosecond UTC instant"))
    if document["event_time"] is not None and event_key is None:
        issues.append(_issue("semantic_time", ("event_time",), "invalid RFC 3339 nanosecond UTC instant"))
    if event_key is not None and observed_key is not None and event_key > observed_key:
        issues.append(_issue("semantic_time", ("event_time",), "event_time must not follow observed_at"))
    if observed_key is not None and ingested_key is not None and observed_key > ingested_key:
        issues.append(_issue("semantic_time", ("ingested_at",), "ingested_at must not precede observed_at"))

    blocking_flags = {"NON_EXECUTABLE", "RETRACTED_SOURCE", "STALE_SOURCE"}
    if blocking_flags.intersection(flags) and document["executable"] is not False:
        issues.append(_issue("semantic_executable", ("executable",), "blocking quality flags forbid execution"))
    if document["executable"] is True:
        positive_fields = (
            "base_amount_atoms",
            "quote_amount_atoms",
            "route_capacity_base_atoms",
            "liquidity_quote_atoms",
        )
        if document["event_kind"] not in {"ROUTE_QUOTE", "SWAP_OBSERVATION"} or kind == "RETRACTION":
            issues.append(_issue("semantic_executable", ("event_kind",), "event kind/revision cannot be executable"))
        for field in positive_fields:
            value = parsed_u64.get(("market", field))
            if value is not None and value <= 0:
                issues.append(
                    _issue("semantic_executable", ("market", field), "executable event value must be positive")
                )
    return tuple(issues)


_FEATURE_NAMES = (
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
)
_SIGNED_FEATURE_NAMES = (
    "mid_price_q18",
    "return_1_q18",
    "ema_fast_price_q18",
    "ema_slow_price_q18",
    "rsi_14_q18",
    "atr_14_price_q18",
    "breakout_high_20_price_q18",
    "breakout_low_20_price_q18",
)
_U64_FEATURE_NAMES = ("volume_20_base_atoms", "liquidity_quote_atoms", "stale_age_ns")


def validate_feature_snapshot_semantics(document: Mapping[str, JsonValue]) -> tuple[ValidationIssue, ...]:
    """Validate FeatureSnapshot zero-history and missing-feature alignment."""
    issues: list[ValidationIssue] = []
    for field in ("market_id", "base_mint", "quote_mint"):
        if not _valid_utf8_registry(document[field], 128):
            issues.append(_issue("semantic_registry", (field,), "registry string exceeds 128 UTF-8 bytes"))
    if document["base_mint"] == document["quote_mint"]:
        issues.append(_issue("semantic_mints", ("base_mint",), "base and quote mints must differ"))

    for field in (
        "decision_sequence",
        "as_of_admission_sequence",
        "as_of_ingest_sequence",
        "equal_time_group",
        "replay_clock_ns",
    ):
        if _u64(document[field]) is None:
            issues.append(_issue("semantic_u64", (field,), "value exceeds the u64 authority range"))

    features = document["features"]
    assert isinstance(features, Mapping)
    for field in _SIGNED_FEATURE_NAMES:
        value = features[field]
        if value is not None and _i128(value) is None:
            issues.append(_issue("semantic_i128", ("features", field), "feature exceeds signed i128 range"))
    for field in _U64_FEATURE_NAMES:
        value = features[field]
        if value is not None and _u64(value) is None:
            issues.append(_issue("semantic_u64", ("features", field), "feature exceeds the u64 range"))

    missing = document["missing_features"]
    expected_missing = [name for name in _FEATURE_NAMES if features[name] is None]
    if missing != expected_missing:
        issues.append(
            _issue(
                "semantic_missing_features",
                ("missing_features",),
                "missing_features must exactly name every null nullable feature in UTF-8 order",
            )
        )

    history_count = features["history_count"]
    assert isinstance(history_count, int)
    if history_count == 0 and any(features[name] is not None for name in _FEATURE_NAMES):
        issues.append(
            _issue(
                "semantic_zero_history",
                ("features", "history_count"),
                "zero history requires every nullable feature to be null",
            )
        )

    as_of_event_id = document["as_of_event_id"]
    merkle_root = document["input_merkle_root_sha256"]
    if (as_of_event_id is None) != (merkle_root is None):
        issues.append(
            _issue(
                "semantic_zero_history",
                ("as_of_event_id",),
                "as_of_event_id and input Merkle root must be null together",
            )
        )
    observed_at = document["observed_at"]
    ingested_at = document["ingested_at"]
    if (observed_at is None) != (ingested_at is None):
        issues.append(_issue("semantic_time", ("observed_at",), "observed_at and ingested_at must be null together"))
    if as_of_event_id is not None and (observed_at is None or ingested_at is None):
        issues.append(
            _issue(
                "semantic_time",
                ("observed_at",),
                "populated event evidence requires observed_at and ingested_at",
            )
        )
    if as_of_event_id is None:
        if any(document[field] is not None for field in ("event_time", "observed_at", "ingested_at")):
            issues.append(
                _issue(
                    "semantic_zero_history",
                    ("event_time",),
                    "a true zero-event snapshot requires all descriptive times null",
                )
            )
        if history_count != 0:
            issues.append(
                _issue(
                    "semantic_zero_history",
                    ("features", "history_count"),
                    "a true zero-event snapshot requires history_count zero",
                )
            )

    event_key = None if document["event_time"] is None else _instant_key(document["event_time"])
    observed_key = None if observed_at is None else _instant_key(observed_at)
    ingested_key = None if ingested_at is None else _instant_key(ingested_at)
    for field, value, key in (
        ("event_time", document["event_time"], event_key),
        ("observed_at", observed_at, observed_key),
        ("ingested_at", ingested_at, ingested_key),
    ):
        if value is not None and key is None:
            issues.append(_issue("semantic_time", (field,), "invalid RFC 3339 nanosecond UTC instant"))
    if event_key is not None and observed_key is not None and event_key > observed_key:
        issues.append(_issue("semantic_time", ("event_time",), "event_time must not follow observed_at"))
    if observed_key is not None and ingested_key is not None and observed_key > ingested_key:
        issues.append(_issue("semantic_time", ("ingested_at",), "ingested_at must not precede observed_at"))

    rsi = features["rsi_14_q18"]
    parsed_rsi = None if rsi is None else _i128(rsi)
    if parsed_rsi is not None and not 0 <= parsed_rsi <= 100_000_000_000_000_000_000:
        issues.append(_issue("semantic_rsi", ("features", "rsi_14_q18"), "RSI must be in [0,100*10^18]"))
    return tuple(issues)


def validate_model_signal_semantics(document: Mapping[str, JsonValue]) -> tuple[ValidationIssue, ...]:
    """Validate the flat directional signal's clocks and numeric tuple."""
    issues: list[ValidationIssue] = []
    for field in (
        "producer_id",
        "model_id",
        "model_version",
        "runtime_profile_id",
        "calibration_version",
        "feature_set_version",
        "market_id",
    ):
        if not _valid_utf8_registry(document[field], 128):
            issues.append(_issue("semantic_registry", (field,), "registry string exceeds 128 UTF-8 bytes"))

    u64_fields = (
        "producer_sequence",
        "horizon_ns",
        "decision_sequence",
        "as_of_ingest_sequence",
        "issued_replay_clock_ns",
        "available_replay_clock_ns",
        "decision_close_replay_clock_ns",
        "ttl_ns",
        "expires_replay_clock_ns",
        "inference_duration_ns",
    )
    parsed: dict[str, int] = {}
    for field in u64_fields:
        value = _u64(document[field])
        if value is None:
            issues.append(_issue("semantic_u64", (field,), "value exceeds the u64 authority range"))
        else:
            parsed[field] = value

    clock_fields = (
        "issued_replay_clock_ns",
        "available_replay_clock_ns",
        "decision_close_replay_clock_ns",
        "expires_replay_clock_ns",
    )
    if all(field in parsed for field in clock_fields):
        issued, available, close, expires = (parsed[field] for field in clock_fields)
        if not issued <= available <= close < expires:
            issues.append(
                _issue(
                    "semantic_clock_order",
                    ("issued_replay_clock_ns",),
                    "required clock order is issued <= available <= close < expires",
                )
            )
        ttl = parsed.get("ttl_ns")
        if ttl is not None and (issued + ttl > _MAX_U64 or issued + ttl != expires):
            issues.append(
                _issue(
                    "semantic_expiry",
                    ("expires_replay_clock_ns",),
                    "expires must equal issued + ttl without overflow",
                )
            )

    score = _i128(document["score_q18"])
    if score is None or not -1_000_000_000_000_000_000 <= score <= 1_000_000_000_000_000_000:
        issues.append(_issue("semantic_score", ("score_q18",), "score must be in [-10^18,10^18]"))

    probability_fields = (
        "probability_abstain_q18",
        "probability_long_bias_q18",
        "probability_exit_bias_q18",
    )
    probabilities: dict[str, int] = {}
    for field in (*probability_fields, "uncertainty_q18", "ood_score_q18"):
        value = _u64(document[field])
        if value is None or value > 1_000_000_000_000_000_000:
            issues.append(_issue("semantic_uq18", (field,), "value must be in [0,10^18]"))
        elif field in probability_fields:
            probabilities[field] = value
    if len(probabilities) == len(probability_fields):
        if sum(probabilities.values()) != 1_000_000_000_000_000_000:
            issues.append(_issue("semantic_probability", probability_fields, "probabilities must sum exactly to 10^18"))
        abstain = probabilities["probability_abstain_q18"]
        long_bias = probabilities["probability_long_bias_q18"]
        exit_bias = probabilities["probability_exit_bias_q18"]
        if long_bias > exit_bias and long_bias > abstain:
            probability_action = "LONG_BIAS"
        elif exit_bias > long_bias and exit_bias > abstain:
            probability_action = "EXIT_BIAS"
        else:
            probability_action = "ABSTAIN"
        action = document["action"]
        if action != probability_action:
            issues.append(
                _issue("semantic_action", ("action",), "action must equal the deterministic probability action")
            )
        if score is not None and ((action == "LONG_BIAS" and score <= 0) or (action == "EXIT_BIAS" and score >= 0)):
            issues.append(_issue("semantic_action", ("score_q18",), "directional action has the wrong score sign"))
    return tuple(issues)


_RISK_REASON_PRECEDENCE = (
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
)
_RISK_REASON_RANK = {code: rank for rank, code in enumerate(_RISK_REASON_PRECEDENCE)}
_RISK_CONFIG_REASON_FORMS = {
    ("RISK_CONFIG_MISSING",),
    ("RISK_CONFIG_INVALID",),
}
_RISK_FATAL_REASONS = frozenset(
    {
        "RISK_SEQUENCE_INVALID",
        "RISK_STATE_UNRECONCILED",
        "RISK_DECIMALS_MISMATCH",
        "RISK_ARITHMETIC_RANGE",
        "RISK_NONPOSITIVE_EQUITY",
        "RISK_RESERVATION_CONFLICT",
    }
)
_RISK_LATCH_PREFIXES = (
    ("RISK_SESSION_LOSS",),
    ("RISK_DRAWDOWN",),
    ("RISK_SESSION_LOSS", "RISK_DRAWDOWN"),
)
_RISK_EXIT_SUFFIXES = (
    (),
    ("RISK_STOP_TRIGGERED",),
    ("RISK_TAKE_TRIGGERED",),
    ("RISK_STOP_TRIGGERED", "RISK_TAKE_TRIGGERED"),
)
_RISK_EXIT_BOUNDARIES = (("RISK_RUN_END_EXIT",), ("RISK_KILL_EXIT",))
_RISK_EXIT_REASON_FORMS = (
    frozenset(_RISK_EXIT_SUFFIXES)
    | frozenset((*boundary, *suffix) for boundary in _RISK_EXIT_BOUNDARIES for suffix in _RISK_EXIT_SUFFIXES)
    | frozenset(
        (*latch, *boundary, *suffix)
        for latch in _RISK_LATCH_PREFIXES
        for boundary in _RISK_EXIT_BOUNDARIES
        for suffix in _RISK_EXIT_SUFFIXES
    )
)
_RISK_SUPPRESSED_EXIT_REASON_FORMS = _RISK_EXIT_REASON_FORMS - {()}
_RISK_KILL_LOSS_FORMS = frozenset(
    (*latch, *suffix) for latch in _RISK_LATCH_PREFIXES for suffix in ((), ("RISK_INTENT_PENDING",))
)
_RISK_REJECT_SINGLETONS = {
    ("RISK_SESSION_CLOSED",),
    ("RISK_INTENT_PENDING",),
}
_RISK_FORCE_DENIAL_REASONS = frozenset({"RISK_EVENT_STALE", "RISK_INSUFFICIENT_BALANCE", "RISK_NO_ACTION"})
_RISK_ORDINARY_DENIAL_REASONS = frozenset(
    {
        "RISK_EVENT_STALE",
        "RISK_STOP_MISSING",
        "RISK_INSUFFICIENT_BALANCE",
        "RISK_MIN_NOTIONAL",
        "RISK_MAX_NOTIONAL",
        "RISK_PARTICIPATION",
        "RISK_IMPACT",
        "RISK_CONCENTRATION",
    }
)


def _risk_reason_family_is_valid(
    *,
    verdict: object,
    action: object,
    reasons: list[JsonValue],
    validated_config: object,
) -> bool:
    reason_tuple = tuple(reasons)
    if verdict == "APPROVE":
        if action == "ENTER_LONG":
            return reason_tuple == ()
        if action == "EXIT_LONG":
            return reason_tuple in _RISK_EXIT_REASON_FORMS
        return False
    if verdict == "REJECT":
        if action != "HOLD":
            return False
        if reason_tuple in _RISK_REJECT_SINGLETONS:
            return True
        if "RISK_NO_ACTION" in reason_tuple:
            return bool(reason_tuple) and all(reason in _RISK_FORCE_DENIAL_REASONS for reason in reason_tuple)
        return bool(reason_tuple) and all(reason in _RISK_ORDINARY_DENIAL_REASONS for reason in reason_tuple)
    if verdict != "KILL" or action != "HOLD":
        return False
    if reason_tuple in _RISK_CONFIG_REASON_FORMS:
        return validated_config is None
    if validated_config is None:
        return False
    if reason_tuple == ("RISK_KILL_LATCHED",) or reason_tuple in _RISK_KILL_LOSS_FORMS:
        return True
    if "RISK_STATE_UNRECONCILED" in reason_tuple:
        return all(reason in {"RISK_SEQUENCE_INVALID", "RISK_STATE_UNRECONCILED"} for reason in reason_tuple)
    return bool(reason_tuple) and all(reason in _RISK_FATAL_REASONS for reason in reason_tuple)


def validate_risk_decision_semantics(document: Mapping[str, JsonValue]) -> tuple[ValidationIssue, ...]:
    """Validate RiskDecision evidence nullability, ordering, and authority tuple."""
    issues: list[ValidationIssue] = []
    if not _valid_utf8_registry(document["market_id"], 128):
        issues.append(_issue("semantic_registry", ("market_id",), "registry string exceeds 128 UTF-8 bytes"))
    for field in ("decision_sequence", "replay_clock_ns"):
        if _u64(document[field]) is None:
            issues.append(_issue("semantic_u64", (field,), "value exceeds the u64 authority range"))

    status = document["model_signal_status"]
    signal_id = document["model_signal_id"]
    validation_id = document["model_validation_receipt_id"]
    evidence_valid = (
        (status == "ABSENT" and signal_id is None and validation_id is None)
        or (status == "ACCEPTED" and signal_id is not None and validation_id is not None)
        or (status in {"REJECTED", "EXPIRED", "DRIFT_DISABLED"} and signal_id is None and validation_id is not None)
    )
    if not evidence_valid:
        issues.append(
            _issue(
                "semantic_model_evidence",
                ("model_signal_status",),
                "model status and nullable evidence IDs form an invalid tuple",
            )
        )

    reasons = document["reason_codes"]
    assert isinstance(reasons, list)
    if reasons != sorted(reasons, key=_RISK_REASON_RANK.__getitem__) or len(reasons) != len(set(reasons)):
        issues.append(
            _issue("semantic_reason_order", ("reason_codes",), "risk reasons must be unique and precedence sorted")
        )

    validated_config = document["validated_config_sha256"]
    baseline_id = document["baseline_id"]
    if (validated_config is None) != (baseline_id is None):
        issues.append(
            _issue(
                "semantic_config_tuple",
                ("validated_config_sha256",),
                "validated config and baseline ID must be null together",
            )
        )
    amount_fields = (
        "requested_base_atoms",
        "approved_base_atoms",
        "requested_notional_quote_atoms",
        "approved_notional_quote_atoms",
        "reserved_quote_atoms",
        "reserved_base_atoms",
    )
    amounts: dict[str, int] = {}
    for field in amount_fields:
        parsed_amount = _u64(document[field])
        if parsed_amount is None:
            issues.append(_issue("semantic_u64", (field,), "value exceeds the u64 authority range"))
        else:
            amounts[field] = parsed_amount

    for field in ("reference_price_q18", "stop_price_q18", "take_price_q18"):
        price_value = document[field]
        if price_value is not None and _i128(price_value) is None:
            issues.append(_issue("semantic_i128", (field,), "price exceeds the signed i128 range"))
    measures = document["measures"]
    assert isinstance(measures, Mapping)
    for field in (
        "projected_market_value_quote_atoms",
        "projected_equity_quote_atoms",
        "stale_age_ns",
    ):
        measure_value = measures[field]
        if measure_value is not None and _u64(measure_value) is None:
            issues.append(_issue("semantic_u64", ("measures", field), "measure exceeds the u64 range"))
    session_pnl = measures["session_pnl_quote_atoms"]
    if session_pnl is not None and _i128(session_pnl) is None:
        issues.append(_issue("semantic_i128", ("measures", "session_pnl_quote_atoms"), "measure exceeds i128 range"))

    verdict = document["verdict"]
    action = document["effective_action"]
    if not _risk_reason_family_is_valid(
        verdict=verdict,
        action=action,
        reasons=reasons,
        validated_config=validated_config,
    ):
        issues.append(
            _issue(
                "semantic_reason_family",
                ("reason_codes",),
                "verdict, effective action, and reasons do not match a closed risk family",
            )
        )

    suppressed_evidence = (
        status == "ABSENT"
        and signal_id is None
        and validation_id is None
        and document["baseline_action"] == "HOLD"
        and document["fused_action"] == "HOLD"
    )
    if (
        verdict == "APPROVE"
        and action == "EXIT_LONG"
        and tuple(reasons) in _RISK_SUPPRESSED_EXIT_REASON_FORMS
        and not suppressed_evidence
    ):
        issues.append(
            _issue(
                "semantic_suppressed_evidence",
                ("reason_codes",),
                "nonempty approved exit reasons require the exact suppressed-evidence tuple",
            )
        )
    zero_amount_authority = len(amounts) == len(amount_fields) and all(amounts[field] == 0 for field in amount_fields)
    zero_levels_and_reservation = (
        document["stop_price_q18"] is None and document["take_price_q18"] is None and document["reservation_id"] is None
    )
    zero_measures = all(measures[field] is None for field in measures)
    canonical_zero_authority = (
        document["reference_price_q18"] is None
        and zero_amount_authority
        and zero_levels_and_reservation
        and zero_measures
    )
    if validated_config is None:
        exact_config_tuple = (
            tuple(reasons) in _RISK_CONFIG_REASON_FORMS
            and baseline_id is None
            and suppressed_evidence
            and action == "HOLD"
            and verdict == "KILL"
            and canonical_zero_authority
        )
        if not exact_config_tuple:
            issues.append(
                _issue(
                    "semantic_config_tuple",
                    ("validated_config_sha256",),
                    "null config requires the complete missing/invalid zero-authority tuple",
                )
            )

    if len(amounts) == len(amount_fields):
        base_authority_valid = amounts["approved_base_atoms"] == amounts["requested_base_atoms"] > 0
        notional_authority_valid = amounts["approved_notional_quote_atoms"] == amounts[
            "requested_notional_quote_atoms"
        ] and (action == "EXIT_LONG" or amounts["approved_notional_quote_atoms"] > 0)
        approved_tuple = (
            base_authority_valid
            and notional_authority_valid
            and document["reservation_id"] is not None
            and action in {"ENTER_LONG", "EXIT_LONG"}
        )
        if verdict == "APPROVE":
            if not approved_tuple:
                issues.append(
                    _issue("semantic_approval", ("verdict",), "approved decision must preserve requested authority")
                )
            if action == "ENTER_LONG":
                reservation_valid = amounts["reserved_quote_atoms"] > 0 and amounts["reserved_base_atoms"] == 0
                reservation_path = ("reserved_quote_atoms",)
            elif action == "EXIT_LONG":
                reservation_valid = (
                    amounts["reserved_base_atoms"] == amounts["approved_base_atoms"] > 0
                    and amounts["reserved_quote_atoms"] == 0
                )
                reservation_path = ("reserved_base_atoms",)
            else:
                reservation_valid = False
                reservation_path = ("reserved_quote_atoms",)
            if not reservation_valid:
                issues.append(_issue("semantic_reservation", reservation_path, "reservation does not match action"))
        elif not (
            amounts["approved_base_atoms"] == 0
            and amounts["approved_notional_quote_atoms"] == 0
            and amounts["reserved_quote_atoms"] == 0
            and amounts["reserved_base_atoms"] == 0
            and document["reservation_id"] is None
            and action == "HOLD"
        ):
            issues.append(_issue("semantic_approval", ("verdict",), "non-approved decision retains authority"))

    reference = None if document["reference_price_q18"] is None else _i128(document["reference_price_q18"])
    stop = None if document["stop_price_q18"] is None else _i128(document["stop_price_q18"])
    take = None if document["take_price_q18"] is None else _i128(document["take_price_q18"])
    if verdict == "APPROVE":
        if reference is None or reference <= 0 or stop is None or stop <= 0 or take is None or take <= 0:
            issues.append(_issue("semantic_prices", ("reference_price_q18",), "approved prices must be positive"))
        elif action == "ENTER_LONG" and not stop < reference < take:
            issues.append(_issue("semantic_prices", ("stop_price_q18",), "entry requires stop < reference < take"))

    nonpositive_equity = "RISK_NONPOSITIVE_EQUITY" in reasons
    fatal_without_nonpositive = (
        verdict == "KILL"
        and bool(reasons)
        and all(reason in _RISK_FATAL_REASONS for reason in reasons)
        and not nonpositive_equity
    )
    latching_loss = verdict == "KILL" and action == "HOLD" and tuple(reasons) in _RISK_KILL_LOSS_FORMS
    if nonpositive_equity:
        levels_are_flat_or_positive_pair = (stop is None and take is None) or (
            stop is not None and stop > 0 and take is not None and take > 0
        )
        nonpositive_tuple = (
            validated_config is not None
            and baseline_id is not None
            and suppressed_evidence
            and verdict == "KILL"
            and action == "HOLD"
            and zero_amount_authority
            and document["reservation_id"] is None
            and (reference is None or reference > 0)
            and measures["participation_bps"] == 0
            and measures["concentration_bps"] is None
            and measures["drawdown_bps"] is not None
            and _u64(measures["projected_market_value_quote_atoms"]) is not None
            and _u64(measures["projected_equity_quote_atoms"]) == 0
            and _i128(measures["session_pnl_quote_atoms"]) is not None
            and levels_are_flat_or_positive_pair
        )
        if not nonpositive_tuple:
            issues.append(
                _issue(
                    "semantic_zero_authority",
                    ("reference_price_q18",),
                    "nonpositive equity requires its exact reason-owned authority tuple",
                )
            )
    elif fatal_without_nonpositive:
        exact_fatal_zero_tuple = (
            validated_config is not None
            and baseline_id is not None
            and suppressed_evidence
            and action == "HOLD"
            and canonical_zero_authority
        )
        if not exact_fatal_zero_tuple:
            issues.append(
                _issue(
                    "semantic_zero_authority",
                    ("reference_price_q18",),
                    "fatal zero reasons require the canonical valid-config zero tuple",
                )
            )
    elif latching_loss:
        levels_are_flat_or_positive_pair = (stop is None and take is None) or (
            stop is not None and stop > 0 and take is not None and take > 0
        )
        impact = measures["impact_bps"]
        stale_age = measures["stale_age_ns"]
        projected_market_value = _u64(measures["projected_market_value_quote_atoms"])
        projected_equity = _u64(measures["projected_equity_quote_atoms"])
        exact_latching_loss_tuple = (
            validated_config is not None
            and baseline_id is not None
            and suppressed_evidence
            and zero_amount_authority
            and document["reservation_id"] is None
            and (reference is None or reference > 0)
            and measures["participation_bps"] == 0
            and (impact is None) == (stale_age is None)
            and measures["concentration_bps"] is not None
            and measures["drawdown_bps"] is not None
            and projected_market_value is not None
            and projected_equity is not None
            and projected_equity > 0
            and _i128(measures["session_pnl_quote_atoms"]) is not None
            and levels_are_flat_or_positive_pair
        )
        if not exact_latching_loss_tuple:
            issues.append(
                _issue(
                    "semantic_loss_authority",
                    ("reason_codes",),
                    "latching loss reasons require suppressed zero authority and retained state measures",
                )
            )
    elif document["reference_price_q18"] is None and validated_config is not None:
        zero_history_levels = (stop is None and take is None) or (
            stop is not None and stop > 0 and take is not None and take > 0
        )
        zero_history_tuple = (
            validated_config is not None
            and baseline_id is not None
            and suppressed_evidence
            and action == "HOLD"
            and zero_amount_authority
            and document["reservation_id"] is None
            and zero_measures
            and zero_history_levels
        )
        if not zero_history_tuple:
            issues.append(
                _issue(
                    "semantic_zero_authority",
                    ("reference_price_q18",),
                    "this null-reference reason family requires the suppressed zero-history tuple",
                )
            )
    return tuple(issues)


def validate_simulated_order_intent_semantics(
    document: Mapping[str, JsonValue],
) -> tuple[ValidationIssue, ...]:
    """Validate intent action, quantity, reservation, and protective levels."""
    issues: list[ValidationIssue] = []
    for field in ("market_id", "base_mint", "quote_mint"):
        if not _valid_utf8_registry(document[field], 128):
            issues.append(_issue("semantic_registry", (field,), "registry string exceeds 128 UTF-8 bytes"))
    if document["base_mint"] == document["quote_mint"]:
        issues.append(_issue("semantic_mints", ("base_mint",), "base and quote mints must differ"))

    u64_fields = (
        "intent_sequence",
        "decision_sequence",
        "quantity_base_atoms",
        "reserved_quote_atoms",
        "reserved_base_atoms",
        "created_replay_clock_ns",
        "decision_ingest_sequence",
        "decision_equal_time_group",
    )
    parsed: dict[str, int] = {}
    for field in u64_fields:
        parsed_authority = _u64(document[field])
        if parsed_authority is None:
            issues.append(_issue("semantic_u64", (field,), "value exceeds the u64 authority range"))
        else:
            parsed[field] = parsed_authority
    if parsed.get("intent_sequence") == 0:
        issues.append(_issue("semantic_sequence", ("intent_sequence",), "intent sequence starts at one"))
    if parsed.get("quantity_base_atoms") == 0:
        issues.append(_issue("semantic_quantity", ("quantity_base_atoms",), "intent quantity must be positive"))

    reference = _i128(document["reference_price_q18"])
    stop = None if document["stop_price_q18"] is None else _i128(document["stop_price_q18"])
    take = None if document["take_price_q18"] is None else _i128(document["take_price_q18"])
    if reference is None or reference <= 0:
        issues.append(_issue("semantic_prices", ("reference_price_q18",), "reference price must be positive"))
    if stop is None or stop <= 0 or take is None or take <= 0:
        issues.append(_issue("semantic_prices", ("stop_price_q18",), "both protective levels must be positive"))

    action = document["action"]
    reserved_quote = parsed.get("reserved_quote_atoms")
    reserved_base = parsed.get("reserved_base_atoms")
    quantity = parsed.get("quantity_base_atoms")
    if action == "OPEN_LONG":
        if reserved_quote is not None and reserved_base is not None and not (reserved_quote > 0 and reserved_base == 0):
            issues.append(
                _issue("semantic_reservation", ("reserved_quote_atoms",), "OPEN_LONG requires quote reservation only")
            )
        if reference is not None and stop is not None and take is not None and not stop < reference < take:
            issues.append(_issue("semantic_prices", ("stop_price_q18",), "OPEN_LONG requires stop < reference < take"))
    elif (
        reserved_quote is not None
        and reserved_base is not None
        and quantity is not None
        and not (reserved_quote == 0 and reserved_base == quantity > 0)
    ):
        issues.append(
            _issue(
                "semantic_reservation",
                ("reserved_base_atoms",),
                "CLOSE_LONG requires full base-quantity reservation only",
            )
        )
    return tuple(issues)


_FILL_REASON_PRECEDENCE = (
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
)
_EXPIRY_FILL_REASONS = {"FILL_KILL_LATCHED", "FILL_SESSION_END", "FILL_NO_NEXT_EVENT"}


def validate_simulated_fill_receipt_semantics(
    document: Mapping[str, JsonValue],
) -> tuple[ValidationIssue, ...]:
    """Validate fill trigger, terminal status, quantity, fee, and release tuples."""
    issues: list[ValidationIssue] = []
    u64_fields = (
        "receipt_sequence",
        "decision_sequence",
        "decision_ingest_sequence",
        "decision_equal_time_group",
        "requested_base_atoms",
        "filled_base_atoms",
        "unfilled_base_atoms",
        "gross_quote_atoms",
        "venue_fee_quote_atoms",
        "priority_fee_quote_atoms",
        "simulation_fee_quote_atoms",
        "released_quote_atoms",
        "released_base_atoms",
    )
    parsed: dict[str, int] = {}
    for field in u64_fields:
        value = _u64(document[field])
        if value is None:
            issues.append(_issue("semantic_u64", (field,), "value exceeds the u64 authority range"))
        else:
            parsed[field] = value
    released_quote = parsed.get("released_quote_atoms")
    released_base = parsed.get("released_base_atoms")
    if released_quote is not None and released_base is not None and released_quote > 0 and released_base > 0:
        issues.append(
            _issue(
                "semantic_fill_release",
                ("released_quote_atoms",),
                "a terminal fill may release at most one reservation asset",
            )
        )
    if parsed.get("receipt_sequence") == 0:
        issues.append(_issue("semantic_sequence", ("receipt_sequence",), "receipt sequence starts at one"))

    fill_coordinate_fields = ("fill_ingest_sequence", "fill_equal_time_group", "fill_replay_clock_ns")
    fill_coordinates: dict[str, int] = {}
    for field in fill_coordinate_fields:
        coordinate_value = document[field]
        if coordinate_value is not None:
            parsed_value = _u64(coordinate_value)
            if parsed_value is None:
                issues.append(_issue("semantic_u64", (field,), "fill coordinate exceeds u64 range"))
            else:
                fill_coordinates[field] = parsed_value
    has_event = document["fill_event_id"] is not None
    coordinates_populated = len(fill_coordinates) == len(fill_coordinate_fields)
    coordinates_null = all(document[field] is None for field in fill_coordinate_fields)
    if not ((has_event and coordinates_populated) or (not has_event and coordinates_null)):
        issues.append(
            _issue(
                "semantic_fill_trigger",
                ("fill_event_id",),
                "fill event and all three fill coordinates must be populated or null together",
            )
        )

    reasons = document["reason_codes"]
    assert isinstance(reasons, list)
    status = document["status"]
    if status == "FILLED":
        expected_reason_tuple = reasons == []
    elif status == "PARTIAL":
        expected_reason_tuple = reasons == ["FILL_PARTIAL"]
    elif status == "EXPIRED":
        expected_reason_tuple = len(reasons) == 1 and reasons[0] in _EXPIRY_FILL_REASONS
    else:
        expected_reason_tuple = (
            len(reasons) == 1 and reasons[0] not in _EXPIRY_FILL_REASONS and reasons[0] != "FILL_PARTIAL"
        )
    if not expected_reason_tuple:
        issues.append(_issue("semantic_fill_status", ("reason_codes",), "status has the wrong exact reason tuple"))
    if not has_event and not (status == "EXPIRED" and set(reasons).issubset(_EXPIRY_FILL_REASONS)):
        issues.append(
            _issue("semantic_fill_trigger", ("fill_event_id",), "null fill event is reserved for exact expiry reasons")
        )
    if has_event and reasons and reasons[0] in _EXPIRY_FILL_REASONS:
        issues.append(_issue("semantic_fill_trigger", ("fill_event_id",), "expiry reason requires a null fill event"))

    decision_group = parsed.get("decision_equal_time_group")
    decision_ingest = parsed.get("decision_ingest_sequence")
    fill_group = fill_coordinates.get("fill_equal_time_group")
    fill_ingest = fill_coordinates.get("fill_ingest_sequence")
    same_or_earlier = reasons == ["FILL_SAME_OR_EARLIER_EVENT"]
    if has_event and same_or_earlier:
        if (
            fill_group is not None
            and decision_group is not None
            and fill_ingest is not None
            and decision_ingest is not None
            and fill_group > decision_group
            and fill_ingest > decision_ingest
        ):
            issues.append(
                _issue(
                    "semantic_fill_trigger",
                    ("fill_equal_time_group",),
                    "same-or-earlier reason requires a non-later group or ingest sequence",
                )
            )
    elif has_event and fill_group is not None and decision_group is not None:
        if fill_group <= decision_group:
            issues.append(
                _issue(
                    "semantic_fill_trigger",
                    ("fill_equal_time_group",),
                    "ordinary event fill group must strictly follow the decision group",
                )
            )
    if has_event and not same_or_earlier and fill_ingest is not None and decision_ingest is not None:
        if fill_ingest <= decision_ingest:
            issues.append(
                _issue(
                    "semantic_fill_trigger",
                    ("fill_ingest_sequence",),
                    "ordinary fill ingest sequence must strictly follow the decision cutoff",
                )
            )

    if len(parsed) == len(u64_fields):
        requested = parsed["requested_base_atoms"]
        filled = parsed["filled_base_atoms"]
        unfilled = parsed["unfilled_base_atoms"]
        if requested <= 0 or filled + unfilled != requested:
            issues.append(
                _issue(
                    "semantic_fill_quantity", ("filled_base_atoms",), "filled + unfilled must equal positive requested"
                )
            )
        if status == "FILLED" and not (filled == requested and unfilled == 0):
            issues.append(_issue("semantic_fill_quantity", ("status",), "FILLED requires a complete fill"))
        if status == "PARTIAL" and not (0 < filled < requested and unfilled > 0):
            issues.append(
                _issue("semantic_fill_quantity", ("status",), "PARTIAL requires positive filled and unfilled")
            )

        fees = (
            parsed["venue_fee_quote_atoms"] + parsed["priority_fee_quote_atoms"] + parsed["simulation_fee_quote_atoms"]
        )
        cash_delta = _i128(document["cash_delta_quote_atoms"])
        if cash_delta is None:
            issues.append(_issue("semantic_i128", ("cash_delta_quote_atoms",), "cash delta exceeds i128 range"))
        if status in {"REJECTED", "EXPIRED"}:
            zero_fields = (
                "filled_base_atoms",
                "gross_quote_atoms",
                "venue_fee_quote_atoms",
                "priority_fee_quote_atoms",
                "simulation_fee_quote_atoms",
            )
            diagnostics_zero = all(
                document[field] == 0
                for field in (
                    "participation_bps",
                    "reference_deviation_bps",
                    "impact_bps",
                    "fee_bps",
                    "adverse_fill_bps",
                )
            )
            release_is_full_side = (parsed["released_quote_atoms"] > 0) != (parsed["released_base_atoms"] > 0)
            if not (
                all(parsed[field] == 0 for field in zero_fields)
                and unfilled == requested
                and cash_delta == 0
                and document["execution_price_q18"] is None
                and diagnostics_zero
                and release_is_full_side
            ):
                issues.append(
                    _issue("semantic_fill_denial", ("status",), "denial/expiry must use the exact zero/release tuple")
                )
        else:
            execution_price = _i128(document["execution_price_q18"])
            gross = parsed["gross_quote_atoms"]
            if filled <= 0 or gross <= 0 or execution_price is None or execution_price <= 0:
                issues.append(
                    _issue(
                        "semantic_fill_execution", ("execution_price_q18",), "non-zero fill requires positive execution"
                    )
                )
            if cash_delta is not None:
                buy_cash = -(gross + fees)
                sell_cash = gross - fees
                if cash_delta != buy_cash and not (sell_cash >= 0 and cash_delta == sell_cash):
                    issues.append(
                        _issue(
                            "semantic_fill_fees", ("cash_delta_quote_atoms",), "cash delta does not bind gross and fees"
                        )
                    )
    return tuple(issues)


_LATCHING_RISK_CODES = (
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
)
_RECONCILIATION_CODES = (
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
)
_KILL_REASON_PRECEDENCE = (*_LATCHING_RISK_CODES, *_RECONCILIATION_CODES)
_KILL_REASON_RANK = {code: rank for rank, code in enumerate(_KILL_REASON_PRECEDENCE)}


def validate_portfolio_state_semantics(document: Mapping[str, JsonValue]) -> tuple[ValidationIssue, ...]:
    """Validate portfolio ordering, reservation bijection, and exact summary fields."""
    issues: list[ValidationIssue] = []
    if not _valid_utf8_registry(document["quote_mint"], 128):
        issues.append(_issue("semantic_registry", ("quote_mint",), "registry string exceeds 128 UTF-8 bytes"))
    top_u64_fields = ("state_sequence", "as_of_ingest_sequence", "equal_time_group", "replay_clock_ns")
    top_values: dict[str, int] = {}
    for field in top_u64_fields:
        value = _u64(document[field])
        if value is None:
            issues.append(_issue("semantic_u64", (field,), "value exceeds the u64 authority range"))
        else:
            top_values[field] = value
    state_sequence = top_values.get("state_sequence")
    if state_sequence is not None:
        genesis_tuple = (
            state_sequence == 0
            and document["previous_portfolio_state_id"] is None
            and document["causation_schema"] == "RUN_INITIALIZATION"
        )
        non_genesis_tuple = (
            state_sequence > 0
            and document["previous_portfolio_state_id"] is not None
            and document["causation_schema"] != "RUN_INITIALIZATION"
        )
        if not (genesis_tuple or non_genesis_tuple):
            issues.append(
                _issue("semantic_state_lineage", ("state_sequence",), "state sequence and predecessor tuple is invalid")
            )

    balances = document["balances"]
    positions = document["positions"]
    open_intents = document["open_intent_ids"]
    kill_reasons = document["kill_reason_codes"]
    assert isinstance(balances, list) and isinstance(positions, list)
    assert isinstance(open_intents, list) and isinstance(kill_reasons, list)
    balance_mints = [row["mint"] for row in balances]
    position_markets = [row["market_id"] for row in positions]
    position_mints = [row["base_mint"] for row in positions]
    if balance_mints != sorted(balance_mints, key=lambda value: value.encode("utf-8")) or len(balance_mints) != len(
        set(balance_mints)
    ):
        issues.append(_issue("semantic_order", ("balances",), "balances must be sorted and unique by mint"))
    if position_markets != sorted(position_markets, key=lambda value: value.encode("utf-8")) or len(
        position_markets
    ) != len(set(position_markets)):
        issues.append(_issue("semantic_order", ("positions",), "positions must be sorted and unique by market"))
    if len(position_mints) != len(set(position_mints)):
        issues.append(_issue("semantic_positions", ("positions",), "position base mints must be unique"))
    if not _sorted_unique(open_intents, key=lambda value: value.encode("utf-8")):
        issues.append(_issue("semantic_order", ("open_intent_ids",), "open intent IDs must be sorted and unique"))
    if kill_reasons != sorted(kill_reasons, key=_KILL_REASON_RANK.__getitem__) or len(kill_reasons) != len(
        set(kill_reasons)
    ):
        issues.append(
            _issue("semantic_reason_order", ("kill_reason_codes",), "kill reasons must use global precedence")
        )
    if bool(kill_reasons) != bool(document["kill_latched"]):
        issues.append(
            _issue("semantic_kill_latch", ("kill_latched",), "kill latch is true iff kill reasons are non-empty")
        )

    parsed_balances: dict[str, tuple[int, int, int, int]] = {}
    for index, row in enumerate(balances):
        assert isinstance(row, Mapping)
        mint = row["mint"]
        assert isinstance(mint, str)
        if not _valid_utf8_registry(mint, 128):
            issues.append(_issue("semantic_registry", ("balances", index, "mint"), "mint exceeds 128 UTF-8 bytes"))
        balance_values = tuple(_u64(row[field]) for field in ("available_atoms", "reserved_atoms", "total_atoms"))
        if any(value is None for value in balance_values):
            issues.append(_issue("semantic_u64", ("balances", index), "balance exceeds the u64 authority range"))
            continue
        available, reserved, total = balance_values
        assert available is not None and reserved is not None and total is not None
        if available + reserved != total:
            issues.append(
                _issue("semantic_balance", ("balances", index, "total_atoms"), "total must equal available + reserved")
            )
        if mint != document["quote_mint"] and total == 0:
            issues.append(_issue("semantic_balance", ("balances",), "zero non-quote balances must be omitted"))
        parsed_balances[mint] = (available, reserved, total, row["decimals"])

    quote_mint = document["quote_mint"]
    quote_balance = parsed_balances.get(quote_mint) if isinstance(quote_mint, str) else None
    if quote_balance is None or quote_balance[3] != document["quote_decimals"]:
        issues.append(
            _issue("semantic_quote_balance", ("quote_mint",), "quote balance must exist with matching decimals")
        )

    position_unrealized = 0
    market_value_total = 0
    for index, row in enumerate(positions):
        assert isinstance(row, Mapping)
        for field in ("market_id", "base_mint"):
            if not _valid_utf8_registry(row[field], 128):
                issues.append(
                    _issue("semantic_registry", ("positions", index, field), "registry string exceeds 128 UTF-8 bytes")
                )
        u64_fields = (
            "quantity_base_atoms",
            "reserved_base_atoms",
            "cost_basis_quote_atoms",
            "market_value_quote_atoms",
        )
        position_values = {field: _u64(row[field]) for field in u64_fields}
        if any(value is None for value in position_values.values()):
            issues.append(_issue("semantic_u64", ("positions", index), "position exceeds the u64 authority range"))
            continue
        quantity = position_values["quantity_base_atoms"]
        reserved = position_values["reserved_base_atoms"]
        cost_basis = position_values["cost_basis_quote_atoms"]
        market_value = position_values["market_value_quote_atoms"]
        assert quantity is not None and reserved is not None and cost_basis is not None and market_value is not None
        if quantity <= 0:
            issues.append(
                _issue(
                    "semantic_position", ("positions", index, "quantity_base_atoms"), "open position must be positive"
                )
            )
        balance = parsed_balances.get(row["base_mint"])
        if balance is None or balance[2] != quantity or balance[1] != reserved or balance[3] != row["base_decimals"]:
            issues.append(
                _issue("semantic_position", ("positions", index), "position must exactly match its base balance")
            )
        unrealized = _i128(row["unrealized_pnl_quote_atoms"])
        if unrealized is None or unrealized != market_value - cost_basis:
            issues.append(
                _issue(
                    "semantic_position", ("positions", index, "unrealized_pnl_quote_atoms"), "unrealized P&L mismatch"
                )
            )
        else:
            position_unrealized += unrealized
        for field in ("mark_price_q18", "stop_price_q18", "take_price_q18"):
            price = _i128(row[field])
            if price is None or price <= 0:
                issues.append(_issue("semantic_prices", ("positions", index, field), "position price must be positive"))
        market_value_total += market_value

    non_quote_balance_mints = {mint for mint, values in parsed_balances.items() if mint != quote_mint and values[2] > 0}
    if non_quote_balance_mints != set(position_mints):
        issues.append(
            _issue("semantic_positions", ("balances",), "positive non-quote balances and positions must be bijective")
        )
    has_reservation = any(values[1] > 0 for values in parsed_balances.values())
    if has_reservation != bool(open_intents):
        issues.append(
            _issue("semantic_reservation", ("open_intent_ids",), "open intents and active reservations must coexist")
        )

    summary = document["summary"]
    assert isinstance(summary, Mapping)
    realized = _i128(summary["realized_pnl_quote_atoms"])
    unrealized = _i128(summary["unrealized_pnl_quote_atoms"])
    session = _i128(summary["session_pnl_quote_atoms"])
    for field in ("cumulative_fees_quote_atoms", "equity_quote_atoms", "peak_equity_quote_atoms"):
        if _u64(summary[field]) is None:
            issues.append(_issue("semantic_u64", ("summary", field), "summary value exceeds u64 range"))
    equity = _u64(summary["equity_quote_atoms"])
    peak = _u64(summary["peak_equity_quote_atoms"])
    if unrealized is None or unrealized != position_unrealized:
        issues.append(
            _issue("semantic_summary", ("summary", "unrealized_pnl_quote_atoms"), "summary unrealized P&L mismatch")
        )
    if realized is None or unrealized is None or session is None or session != realized + unrealized:
        issues.append(
            _issue(
                "semantic_summary",
                ("summary", "session_pnl_quote_atoms"),
                "session P&L must equal realized + unrealized",
            )
        )
    if equity is not None and quote_balance is not None and equity != quote_balance[2] + market_value_total:
        issues.append(_issue("semantic_summary", ("summary", "equity_quote_atoms"), "equity total mismatch"))
    if equity is not None and peak is not None:
        if peak < equity:
            issues.append(_issue("semantic_summary", ("summary", "peak_equity_quote_atoms"), "peak is below equity"))
        expected_drawdown = 0 if peak == 0 else ((peak - equity) * 10_000 + peak - 1) // peak
        if summary["drawdown_bps"] != expected_drawdown:
            issues.append(_issue("semantic_summary", ("summary", "drawdown_bps"), "drawdown ceiling mismatch"))
    return tuple(issues)


_LEDGER_OBJECT_SCHEMA_BY_TYPE = {
    "SOURCE_ADMISSION": "trading.source-admission-receipt/v1",
    "CONFIG_ADMISSION": "trading.config-admission-receipt/v1",
    "RAW_ADMISSION": "trading.raw-event/v1",
    "FEATURE_SNAPSHOT": "trading.feature-snapshot/v1",
    "MODEL_VALIDATION": "trading.model-validation-receipt/v1",
    "MODEL_SIGNAL_ACCEPTED": "trading.model-signal/v1",
    "RISK_DECISION": "trading.risk-decision/v1",
    "SIMULATED_INTENT": "trading.simulated-order-intent/v1",
    "SIMULATED_FILL": "trading.simulated-fill-receipt/v1",
    "PORTFOLIO_STATE": "trading.portfolio-state/v1",
    "RECONCILIATION": "trading.reconciliation-receipt/v1",
    "KILL_STATE": "trading.portfolio-state/v1",
    "RUN_RECEIPT": "trading.run-receipt/v1",
    "RUN_END": "trading.portfolio-state/v1",
}
_LEDGER_POSTING_REQUIRED_TYPES = {"SIMULATED_INTENT", "SIMULATED_FILL"}
_LEDGER_POSTING_PERMITTED_TYPES = {*_LEDGER_POSTING_REQUIRED_TYPES, "PORTFOLIO_STATE"}


def validate_ledger_record_semantics(document: Mapping[str, JsonValue]) -> tuple[ValidationIssue, ...]:
    """Validate ledger mapping, ordering, prior-head, balance, and self-cause rules."""
    issues: list[ValidationIssue] = []
    record_type = document["record_type"]
    assert isinstance(record_type, str)
    if document["object_schema"] != _LEDGER_OBJECT_SCHEMA_BY_TYPE[record_type]:
        issues.append(
            _issue(
                "semantic_object_schema",
                ("object_schema",),
                "object schema does not match the closed record-type mapping",
            )
        )
    if document["object_sha256"] != document["object_id"]:
        issues.append(_issue("semantic_object_digest", ("object_sha256",), "object digest must equal object ID"))

    ledger_sequence = _u64(document["ledger_sequence"])
    previous_ledger_record_id = document["previous_ledger_record_id"]
    if ledger_sequence is None:
        issues.append(_issue("semantic_u64", ("ledger_sequence",), "ledger sequence exceeds u64 range"))
    elif (ledger_sequence == 0) != (previous_ledger_record_id is None):
        issues.append(
            _issue(
                "semantic_ledger_head",
                ("previous_ledger_record_id",),
                "previous ledger ID is null exactly at sequence zero",
            )
        )
    if previous_ledger_record_id == document["ledger_record_id"]:
        issues.append(
            _issue(
                "semantic_ledger_head",
                ("previous_ledger_record_id",),
                "previous ledger ID must differ from the current record ID",
            )
        )
    for field in ("decision_sequence", "ingest_sequence", "equal_time_group", "replay_clock_ns"):
        value = document[field]
        if value is not None and _u64(value) is None:
            issues.append(_issue("semantic_u64", (field,), "causal coordinate exceeds u64 range"))

    causes = document["causation_ids"]
    assert isinstance(causes, list)
    if not _sorted_unique(causes, key=lambda value: value.encode("utf-8")):
        issues.append(_issue("semantic_order", ("causation_ids",), "causes must be digest-byte sorted and unique"))
    if (
        document["ledger_record_id"] in causes
        or document["object_id"] in causes
        or (previous_ledger_record_id is not None and previous_ledger_record_id in causes)
    ):
        issues.append(_issue("semantic_self_cause", ("causation_ids",), "record/object self-cause is forbidden"))

    entries = document["entries"]
    assert isinstance(entries, list)
    entry_keys = [(row["asset_mint"], row["account"]) for row in entries]
    sorted_entry_keys = sorted(entry_keys, key=lambda pair: (pair[0].encode("utf-8"), pair[1].encode("utf-8")))
    if entry_keys != sorted_entry_keys or len(entry_keys) != len(set(entry_keys)):
        issues.append(
            _issue("semantic_order", ("entries",), "entries must be sorted and unique by (asset_mint, account)")
        )
    if record_type in _LEDGER_POSTING_REQUIRED_TYPES and not entries:
        issues.append(_issue("semantic_entries", ("entries",), "reservation/fill ledger record requires postings"))
    elif record_type not in _LEDGER_POSTING_PERMITTED_TYPES and entries:
        issues.append(_issue("semantic_entries", ("entries",), "evidence-only ledger record forbids postings"))
    sums: dict[str, int] = {}
    decimals_by_asset: dict[str, int] = {}
    for index, row in enumerate(entries):
        assert isinstance(row, Mapping)
        asset = row["asset_mint"]
        assert isinstance(asset, str)
        if not _valid_utf8_registry(asset, 128):
            issues.append(
                _issue("semantic_registry", ("entries", index, "asset_mint"), "asset mint exceeds 128 UTF-8 bytes")
            )
        amount = _i128(row["amount_atoms"])
        if amount is None:
            issues.append(_issue("semantic_i128", ("entries", index, "amount_atoms"), "entry exceeds i128 range"))
            continue
        if amount == 0:
            issues.append(_issue("semantic_entries", ("entries", index, "amount_atoms"), "zero entry must be omitted"))
        sums[asset] = sums.get(asset, 0) + amount
        decimals = row["decimals"]
        assert isinstance(decimals, int)
        if asset in decimals_by_asset and decimals_by_asset[asset] != decimals:
            issues.append(_issue("semantic_entries", ("entries", index, "decimals"), "asset decimals disagree"))
        decimals_by_asset[asset] = decimals
    for asset, total in sums.items():
        if total != 0:
            issues.append(_issue("semantic_entries", ("entries",), f"entries for {asset!r} do not balance"))

    reconciliation = document["reconciliation"]
    assert isinstance(reconciliation, Mapping)
    asset_residuals = reconciliation["asset_residuals"]
    account_residuals = reconciliation["account_residuals"]
    assert isinstance(asset_residuals, list) and isinstance(account_residuals, list)
    asset_keys = [row["asset_mint"] for row in asset_residuals]
    if asset_keys != sorted(asset_keys, key=lambda value: value.encode("utf-8")) or len(asset_keys) != len(
        set(asset_keys)
    ):
        issues.append(
            _issue("semantic_order", ("reconciliation", "asset_residuals"), "asset residuals are not sorted unique")
        )
    account_keys = [(row["asset_mint"], row["account"]) for row in account_residuals]
    sorted_account_keys = sorted(
        account_keys,
        key=lambda pair: (pair[0].encode("utf-8"), pair[1].encode("utf-8")),
    )
    if account_keys != sorted_account_keys or len(account_keys) != len(set(account_keys)):
        issues.append(
            _issue(
                "semantic_order",
                ("reconciliation", "account_residuals"),
                "account residuals are not sorted unique",
            )
        )
    for collection_name, rows in (("asset_residuals", asset_residuals), ("account_residuals", account_residuals)):
        for index, row in enumerate(rows):
            if not _valid_utf8_registry(row["asset_mint"], 128):
                issues.append(
                    _issue(
                        "semantic_registry",
                        ("reconciliation", collection_name, index, "asset_mint"),
                        "asset mint exceeds 128 UTF-8 bytes",
                    )
                )
            if _i128(row["residual_atoms"]) != 0:
                issues.append(
                    _issue(
                        "semantic_residual",
                        ("reconciliation", collection_name, index, "residual_atoms"),
                        "persisted ledger residual must be zero",
                    )
                )
    for field in (
        "equity_residual_quote_atoms",
        "realized_pnl_residual_quote_atoms",
        "unrealized_pnl_residual_quote_atoms",
        "fee_residual_quote_atoms",
        "peak_equity_residual_quote_atoms",
        "drawdown_residual_bps",
    ):
        if _i128(reconciliation[field]) != 0:
            issues.append(
                _issue("semantic_residual", ("reconciliation", field), "persisted ledger residual must be zero")
            )
    unmatched_reservation_count = _u64(reconciliation["unmatched_reservation_count"])
    if unmatched_reservation_count is None:
        issues.append(
            _issue(
                "semantic_u64",
                ("reconciliation", "unmatched_reservation_count"),
                "unmatched reservation count exceeds u64 range",
            )
        )
    elif unmatched_reservation_count != 0:
        issues.append(
            _issue(
                "semantic_residual",
                ("reconciliation", "unmatched_reservation_count"),
                "persisted unmatched reservation count must be zero",
            )
        )
    return tuple(issues)


def validate_fixture_manifest_semantics(document: Mapping[str, JsonValue]) -> tuple[ValidationIssue, ...]:
    """Validate fixture keyed sets, identities, paths, and scalar authority."""
    issues: list[ValidationIssue] = []
    parsed_root: dict[str, int] = {}
    for field in (
        "initial_quote_atoms",
        "replay_tick_ns",
        "session_start_availability_slot",
        "session_end_availability_slot",
    ):
        parsed = _u64(document[field])
        if parsed is None:
            issues.append(_issue("semantic_u64", (field,), "value exceeds the u64 authority range"))
        else:
            parsed_root[field] = parsed
    for field in ("initial_quote_atoms", "replay_tick_ns", "session_start_availability_slot"):
        if parsed_root.get(field) == 0:
            issues.append(_issue("semantic_positive", (field,), "value must be positive"))
    start_slot = parsed_root.get("session_start_availability_slot")
    end_slot = parsed_root.get("session_end_availability_slot")
    if end_slot == 0:
        issues.append(_issue("semantic_positive", ("session_end_availability_slot",), "value must be positive"))
    if start_slot is not None and end_slot is not None and start_slot >= end_slot:
        issues.append(
            _issue(
                "semantic_session_order",
                ("session_end_availability_slot",),
                "session start must be strictly before session end",
            )
        )

    if not _valid_utf8_registry(document["quote_mint"], 128):
        issues.append(_issue("semantic_registry", ("quote_mint",), "registry string exceeds 128 UTF-8 bytes"))

    allowed_markets = document["allowed_markets"]
    files = document["files"]
    assert isinstance(allowed_markets, list) and isinstance(files, list)
    if len(allowed_markets) > _MAX_U64:
        issues.append(_issue("semantic_count", ("allowed_markets",), "market count exceeds MAX_U64_V0"))

    market_ids: list[str] = []
    base_mints: list[str] = []
    for index, row_value in enumerate(allowed_markets):
        assert isinstance(row_value, Mapping)
        row = row_value
        for field in ("market_id", "base_mint", "quote_mint"):
            if not _valid_utf8_registry(row[field], 128):
                issues.append(
                    _issue(
                        "semantic_registry",
                        ("allowed_markets", index, field),
                        "registry string exceeds 128 UTF-8 bytes",
                    )
                )
        market_id = row["market_id"]
        base_mint = row["base_mint"]
        assert isinstance(market_id, str) and isinstance(base_mint, str)
        market_ids.append(market_id)
        base_mints.append(base_mint)
        if base_mint == document["quote_mint"]:
            issues.append(
                _issue("semantic_mints", ("allowed_markets", index, "base_mint"), "base and quote mints must differ")
            )
        if row["quote_mint"] != document["quote_mint"]:
            issues.append(
                _issue(
                    "semantic_quote_identity",
                    ("allowed_markets", index, "quote_mint"),
                    "market quote mint must match the manifest",
                )
            )
        if row["quote_decimals"] != document["quote_decimals"]:
            issues.append(
                _issue(
                    "semantic_quote_decimals",
                    ("allowed_markets", index, "quote_decimals"),
                    "market quote decimals must match the manifest",
                )
            )
    if len(market_ids) != len(set(market_ids)):
        issues.append(_issue("semantic_market_set", ("allowed_markets",), "market_id values must be unique"))
    if len(base_mints) != len(set(base_mints)):
        issues.append(_issue("semantic_market_set", ("allowed_markets",), "base_mint values must be unique"))
    try:
        sorted_market_ids = sorted(market_ids, key=lambda value: value.encode("utf-8", errors="strict"))
    except UnicodeEncodeError:
        sorted_market_ids = market_ids
    if market_ids != sorted_market_ids:
        issues.append(
            _issue("semantic_market_order", ("allowed_markets",), "markets must be sorted by unsigned UTF-8 market_id")
        )

    file_paths: list[str] = []
    admission_sequences: list[int] = []
    allowed_market_ids = set(market_ids)
    for index, row_value in enumerate(files):
        assert isinstance(row_value, Mapping)
        row = row_value
        for field in ("source_id", "source_kind", "source_revision", "market_id"):
            if not _valid_utf8_registry(row[field], 128):
                issues.append(
                    _issue(
                        "semantic_registry",
                        ("files", index, field),
                        "registry string exceeds 128 UTF-8 bytes",
                    )
                )
        path = row["relative_path"]
        assert isinstance(path, str)
        file_paths.append(path)
        if unicodedata.normalize("NFC", path) != path:
            issues.append(_issue("semantic_path", ("files", index, "relative_path"), "path must be NFC-normalized"))
        if row["market_id"] not in allowed_market_ids:
            issues.append(
                _issue("semantic_file_market", ("files", index, "market_id"), "file market is not allowlisted")
            )
        for field in ("byte_length", "admission_sequence", "availability_slot"):
            parsed = _u64(row[field])
            if parsed is None:
                issues.append(_issue("semantic_u64", ("files", index, field), "value exceeds the u64 authority range"))
            elif field == "admission_sequence":
                if parsed == 0:
                    issues.append(
                        _issue(
                            "semantic_admission_sequence",
                            ("files", index, field),
                            "admission sequence must be one-based",
                        )
                    )
                admission_sequences.append(parsed)
    if len(file_paths) != len(set(file_paths)):
        issues.append(_issue("semantic_file_set", ("files",), "relative_path values must be unique"))
    try:
        sorted_paths = sorted(file_paths, key=lambda value: value.encode("utf-8", errors="strict"))
    except UnicodeEncodeError:
        sorted_paths = file_paths
    if file_paths != sorted_paths:
        issues.append(_issue("semantic_file_order", ("files",), "files must be sorted by unsigned UTF-8 path"))
    if len(admission_sequences) != len(set(admission_sequences)):
        issues.append(_issue("semantic_admission_sequence", ("files",), "file admission sequences must be unique"))
    return tuple(issues)


def validate_replay_risk_config_semantics(document: Mapping[str, JsonValue]) -> tuple[ValidationIssue, ...]:
    """Validate all string-authority bounds and config cross-field relations."""
    issues: list[ValidationIssue] = []
    parsed: dict[str, int] = {}
    u64_fields = (
        "target_entry_notional_quote_atoms",
        "min_notional_quote_atoms",
        "max_notional_quote_atoms",
        "max_session_loss_quote_atoms",
        "stale_after_ns",
        "max_run_closure_proof_rows",
        "session_start_replay_clock_ns",
        "session_end_replay_clock_ns",
    )
    for field in u64_fields:
        value = _u64(document[field])
        if value is None:
            issues.append(_issue("semantic_u64", (field,), "value exceeds the u64 authority range"))
        else:
            parsed[field] = value
    for field in (
        "target_entry_notional_quote_atoms",
        "min_notional_quote_atoms",
        "max_notional_quote_atoms",
    ):
        if parsed.get(field) == 0:
            issues.append(_issue("semantic_positive", (field,), "notional must be positive"))
    minimum = parsed.get("min_notional_quote_atoms")
    target = parsed.get("target_entry_notional_quote_atoms")
    maximum = parsed.get("max_notional_quote_atoms")
    if minimum is not None and target is not None and maximum is not None and not minimum <= target <= maximum:
        issues.append(
            _issue(
                "semantic_notional_order",
                ("target_entry_notional_quote_atoms",),
                "min_notional must not exceed target, which must not exceed max_notional",
            )
        )
    tick = _i128(document["price_tick_q18"])
    if tick is None:
        issues.append(_issue("semantic_i128", ("price_tick_q18",), "price tick exceeds signed-i128 range"))
    elif tick <= 0:
        issues.append(_issue("semantic_positive", ("price_tick_q18",), "price tick must be positive"))
    proof_rows = parsed.get("max_run_closure_proof_rows")
    if proof_rows is not None and not 1 <= proof_rows <= _MAX_RUN_CLOSURE_PROOF_ROWS_V0:
        issues.append(
            _issue(
                "semantic_proof_cap",
                ("max_run_closure_proof_rows",),
                "proof-row limit must be within the v0 protocol cap",
            )
        )
    start = parsed.get("session_start_replay_clock_ns")
    end = parsed.get("session_end_replay_clock_ns")
    if start is not None and end is not None and start >= end:
        issues.append(
            _issue(
                "semantic_session_order",
                ("session_end_replay_clock_ns",),
                "session start must be strictly before session end",
            )
        )
    return tuple(issues)


def validate_source_admission_receipt_semantics(
    document: Mapping[str, JsonValue],
) -> tuple[ValidationIssue, ...]:
    """Validate pure source status, preservation-compatible nullability, and time rules."""
    issues: list[ValidationIssue] = []
    reason_values = document["reason_codes"]
    assert isinstance(reason_values, list)
    reason_codes = tuple(reason_values)
    try:
        normalized = normalize_source_admission_reason_codes(reason_codes)
    except ValueError as error:
        issues.append(_issue("semantic_reason_codes", ("reason_codes",), str(error)))
        normalized = reason_codes
    if reason_codes != normalized:
        issues.append(
            _issue("semantic_reason_order", ("reason_codes",), "reason codes are not in normative precedence")
        )
    try:
        derived_status = derive_source_admission_status(reason_codes)
    except ValueError:
        derived_status = None
    if document["status"] != derived_status:
        issues.append(_issue("semantic_status", ("status",), "status does not match the complete reason set"))

    reason_set = set(reason_codes)
    for field in ("terms_sha256", "rights_role", "rights_effective_date", "rights_review_date"):
        if document[field] is None and "ADMISSION_RIGHTS_MISSING" not in reason_set:
            issues.append(
                _issue(
                    "semantic_reason_completeness",
                    (field,),
                    "missing rights evidence requires ADMISSION_RIGHTS_MISSING",
                )
            )

    def require_null_owner(field: str, owner_codes: frozenset[str], message: str) -> None:
        if document[field] is None and reason_set.isdisjoint(owner_codes):
            issues.append(
                _issue(
                    "semantic_reason_completeness",
                    (field,),
                    message,
                )
            )

    manifest_owner = frozenset({"ADMISSION_MANIFEST_MISMATCH"})
    local_owner = frozenset({"ADMISSION_NOT_LOCAL"})
    profile_owner = frozenset({"ADMISSION_PROFILE_MISMATCH"})
    revision_owners = frozenset({"ADMISSION_PROFILE_MISMATCH", "ADMISSION_POINT_IN_TIME_MISSING"})
    point_in_time_owner = frozenset({"ADMISSION_POINT_IN_TIME_MISSING"})

    require_null_owner(
        "fixture_manifest_sha256",
        manifest_owner,
        "missing manifest identity requires ADMISSION_MANIFEST_MISMATCH",
    )
    manifest_blocked = document["fixture_manifest_sha256"] is None and not reason_set.isdisjoint(manifest_owner)

    local_fields = ("raw_payload_sha256", "relative_path", "byte_length")
    if not manifest_blocked:
        for field in local_fields:
            require_null_owner(field, local_owner, "missing local evidence requires ADMISSION_NOT_LOCAL")
    local_blocked = manifest_blocked or (
        any(document[field] is None for field in local_fields) and not reason_set.isdisjoint(local_owner)
    )

    profile_field_owners = {
        "source_id": profile_owner,
        "source_kind": profile_owner,
        "source_revision": revision_owners,
        "market_id": profile_owner,
        "media_type": profile_owner,
    }
    if not local_blocked:
        for field, owner_codes in profile_field_owners.items():
            require_null_owner(
                field, owner_codes, "missing parsed source identity requires a profile/point-in-time code"
            )
        require_null_owner(
            "availability_slot",
            point_in_time_owner,
            "missing availability evidence requires ADMISSION_POINT_IN_TIME_MISSING",
        )
    for field in ("observed_at", "ingested_at"):
        require_null_owner(
            field,
            point_in_time_owner,
            "missing witness evidence requires ADMISSION_POINT_IN_TIME_MISSING",
        )

    for field in ("source_id", "source_kind", "source_revision", "market_id"):
        value = document[field]
        if value is not None and not _valid_utf8_registry(value, 128):
            issues.append(_issue("semantic_registry", (field,), "registry string exceeds 128 UTF-8 bytes"))
    relative_path = document["relative_path"]
    if isinstance(relative_path, str) and unicodedata.normalize("NFC", relative_path) != relative_path:
        issues.append(_issue("semantic_path", ("relative_path",), "path must be NFC-normalized"))

    parsed_admission = _u64(document["admission_sequence"])
    if parsed_admission is None:
        issues.append(_issue("semantic_u64", ("admission_sequence",), "value exceeds the u64 authority range"))
    elif parsed_admission == 0:
        issues.append(
            _issue("semantic_admission_sequence", ("admission_sequence",), "admission sequence must be one-based")
        )
    for field in ("byte_length", "availability_slot"):
        value = document[field]
        if value is not None and _u64(value) is None:
            issues.append(_issue("semantic_u64", (field,), "value exceeds the u64 authority range"))

    observed = document["observed_at"]
    ingested = document["ingested_at"]
    observed_key = None if observed is None else _instant_key(observed)
    ingested_key = None if ingested is None else _instant_key(ingested)
    if observed is not None and observed_key is None:
        issues.append(_issue("semantic_timestamp", ("observed_at",), "timestamp is not a real UTC instant"))
    if ingested is not None and ingested_key is None:
        issues.append(_issue("semantic_timestamp", ("ingested_at",), "timestamp is not a real UTC instant"))
    if observed_key is not None and ingested_key is not None and ingested_key < observed_key:
        issues.append(_issue("semantic_time_order", ("ingested_at",), "ingested_at precedes observed_at"))

    effective = document["rights_effective_date"]
    review = document["rights_review_date"]
    effective_key = None if effective is None else _full_date_key(effective)
    review_key = None if review is None else _full_date_key(review)
    if effective is not None and effective_key is None:
        issues.append(_issue("semantic_date", ("rights_effective_date",), "date is not a real calendar date"))
    if review is not None and review_key is None:
        issues.append(_issue("semantic_date", ("rights_review_date",), "date is not a real calendar date"))
    if effective_key is not None and review_key is not None and review_key < effective_key:
        issues.append(_issue("semantic_rights_order", ("rights_review_date",), "rights review precedes effective date"))

    if document["status"] == "ADMITTED":
        admitted_required = (
            "fixture_manifest_sha256",
            "raw_payload_sha256",
            "terms_sha256",
            "source_id",
            "source_kind",
            "source_revision",
            "market_id",
            "relative_path",
            "media_type",
            "byte_length",
            "availability_slot",
            "observed_at",
            "ingested_at",
            "rights_role",
            "rights_effective_date",
            "rights_review_date",
        )
        for field in admitted_required:
            if document[field] is None:
                issues.append(
                    _issue("semantic_admitted_totality", (field,), "admitted receipt requires this witnessed field")
                )
    return tuple(issues)


def validate_config_admission_receipt_semantics(
    document: Mapping[str, JsonValue],
) -> tuple[ValidationIssue, ...]:
    """Validate the pure config status/reason/nullability tuple."""
    issues: list[ValidationIssue] = []
    reason_values = document["reason_codes"]
    assert isinstance(reason_values, list)
    reason_codes = tuple(reason_values)
    try:
        normalized = normalize_config_admission_reason_codes(reason_codes)
    except ValueError as error:
        issues.append(_issue("semantic_reason_codes", ("reason_codes",), str(error)))
        normalized = reason_codes
    if reason_codes != normalized:
        issues.append(
            _issue("semantic_reason_order", ("reason_codes",), "reason codes are not in normative precedence")
        )

    raw_length = _u64(document["raw_byte_length"])
    if raw_length is None:
        issues.append(_issue("semantic_u64", ("raw_byte_length",), "value exceeds the u64 authority range"))
    admission_sequence = _u64(document["admission_sequence"])
    if admission_sequence is None:
        issues.append(_issue("semantic_u64", ("admission_sequence",), "value exceeds the u64 authority range"))
    elif admission_sequence == 0:
        issues.append(
            _issue("semantic_admission_sequence", ("admission_sequence",), "admission sequence must be one-based")
        )

    status = document["status"]
    raw_digest = document["raw_config_sha256"]
    validated_digest = document["validated_config_sha256"]
    if status == "MISSING":
        if reason_codes != ("CONFIG_MISSING",):
            issues.append(_issue("semantic_config_tuple", ("reason_codes",), "MISSING requires exactly CONFIG_MISSING"))
        if raw_digest is not None or validated_digest is not None or raw_length != 0:
            issues.append(_issue("semantic_config_tuple", ("status",), "MISSING requires null digests and zero length"))
    elif status == "INVALID":
        if not reason_codes or "CONFIG_MISSING" in reason_codes:
            issues.append(
                _issue(
                    "semantic_config_tuple",
                    ("reason_codes",),
                    "INVALID requires non-missing rejection reasons",
                )
            )
        if raw_digest is None or validated_digest is not None:
            issues.append(
                _issue("semantic_config_tuple", ("status",), "INVALID requires raw digest and null validated digest")
            )
    else:
        if reason_codes:
            issues.append(_issue("semantic_config_tuple", ("reason_codes",), "VALID requires empty reasons"))
        if raw_digest is None or validated_digest is None:
            issues.append(
                _issue("semantic_config_tuple", ("status",), "VALID requires raw and validated config digests")
            )
        if raw_length == 0:
            issues.append(
                _issue("semantic_config_tuple", ("raw_byte_length",), "VALID canonical record cannot be empty")
            )
    return tuple(issues)


def _check_reason_order(
    values: object,
    precedence: tuple[str, ...],
    *,
    path: tuple[str | int, ...],
) -> tuple[tuple[str, ...], list[ValidationIssue]]:
    assert isinstance(values, list)
    codes = tuple(values)
    issues: list[ValidationIssue] = []
    try:
        normalized = _normalize_reason_codes(codes, precedence)
    except ValueError as error:
        issues.append(_issue("semantic_reason_codes", path, str(error)))
        normalized = codes
    if codes != normalized:
        issues.append(_issue("semantic_reason_order", path, "reason codes are not in normative precedence"))
    return codes, issues


def _utf8_sort_key(value: str) -> bytes:
    return value.encode("utf-8", errors="strict")


def validate_run_closure_receipt_semantics(
    document: Mapping[str, JsonValue],
) -> tuple[ValidationIssue, ...]:
    """Validate Task-8 closure status, reason, budget, and row-presence shape only."""
    issues: list[ValidationIssue] = []
    reason_codes, reason_issues = _check_reason_order(
        document["reason_codes"], RUN_CLOSURE_REASON_PRECEDENCE, path=("reason_codes",)
    )
    issues.extend(reason_issues)

    source_ids = document["source_admission_receipt_ids"]
    assert isinstance(source_ids, list)
    if source_ids != sorted(source_ids, key=_utf8_sort_key) or len(source_ids) != len(set(source_ids)):
        issues.append(
            _issue(
                "semantic_source_set",
                ("source_admission_receipt_ids",),
                "source receipt IDs must be unique and sorted by unsigned UTF-8 bytes",
            )
        )

    mode = document["model_signal_mode"]
    registry_id = document["model_registry_sha256"]
    manifest_id = document["model_signal_manifest_sha256"]
    if mode == "DISABLED":
        if registry_id is not None or manifest_id is not None:
            issues.append(
                _issue("semantic_model_mode", ("model_signal_mode",), "disabled mode requires both model IDs null")
            )
    elif registry_id is None or manifest_id is None:
        issues.append(_issue("semantic_model_mode", ("model_signal_mode",), "cached mode requires both model IDs"))

    parsed_scalars: dict[str, int | None] = {}
    for field in ("terminal_equal_time_group", "proof_row_limit", "proof_row_count_total"):
        value = document[field]
        parsed = None if value is None else _u64(value)
        parsed_scalars[field] = parsed
        if value is not None and parsed is None:
            issues.append(_issue("semantic_u64", (field,), "value exceeds the u64 authority range"))
    proof_limit = parsed_scalars["proof_row_limit"]
    if proof_limit is not None and not 1 <= proof_limit <= _MAX_RUN_CLOSURE_PROOF_ROWS_V0:
        issues.append(
            _issue(
                "semantic_proof_cap",
                ("proof_row_limit",),
                "proof-row limit exceeds the effective v0 protocol cap",
            )
        )

    market_values = document["market_proofs"]
    assert isinstance(market_values, list)
    market_rows = [row for row in market_values if isinstance(row, Mapping)]
    market_ids = [str(row["market_id"]) for row in market_rows]
    if market_ids != sorted(market_ids, key=_utf8_sort_key) or len(market_ids) != len(set(market_ids)):
        issues.append(
            _issue(
                "semantic_market_order",
                ("market_proofs",),
                "market proof rows must be unique and sorted by unsigned UTF-8 market_id",
            )
        )

    row_codes: list[tuple[str, ...]] = []
    for index, row in enumerate(market_rows):
        if not _valid_utf8_registry(row["market_id"], 128):
            issues.append(
                _issue(
                    "semantic_registry",
                    ("market_proofs", index, "market_id"),
                    "market ID must be nonempty and at most 128 UTF-8 bytes",
                )
            )
        codes, code_issues = _check_reason_order(
            row["failure_codes"],
            RUN_CLOSURE_MARKET_REASON_PRECEDENCE,
            path=("market_proofs", index, "failure_codes"),
        )
        issues.extend(code_issues)
        row_codes.append(codes)
        code_set = frozenset(codes)
        candidate_codes = frozenset(
            {
                "CLOSURE_CANDIDATE_CARDINALITY",
                "CLOSURE_CANDIDATE_SCHEMA_INVALID",
                "CLOSURE_CANDIDATE_NON_EXECUTABLE",
                "CLOSURE_IDENTITY_MISMATCH",
                "CLOSURE_DECIMALS_MISMATCH",
                "CLOSURE_CAPACITY_INSUFFICIENT",
            }
        )
        if "CLOSURE_H_MISSING" in code_set and candidate_codes.intersection(code_set):
            issues.append(
                _issue(
                    "semantic_failure_phase",
                    ("market_proofs", index, "failure_codes"),
                    "a missing trigger horizon skips every candidate-dependent predicate",
                )
            )
        if "CLOSURE_PROOF_PREDICATE_FAILED" in code_set and code_set != {"CLOSURE_PROOF_PREDICATE_FAILED"}:
            issues.append(
                _issue(
                    "semantic_failure_phase",
                    ("market_proofs", index, "failure_codes"),
                    "proof-predicate failure is reachable only after every pre-proof code passes",
                )
            )
        if {
            "CLOSURE_CANDIDATE_CARDINALITY",
            "CLOSURE_CANDIDATE_SCHEMA_INVALID",
        }.issubset(code_set):
            issues.append(
                _issue(
                    "semantic_failure_phase",
                    ("market_proofs", index, "failure_codes"),
                    "candidate cardinality and candidate schema failure are mutually exclusive",
                )
            )
        candidate_later_codes = frozenset(
            {
                "CLOSURE_CANDIDATE_NON_EXECUTABLE",
                "CLOSURE_IDENTITY_MISMATCH",
                "CLOSURE_DECIMALS_MISMATCH",
                "CLOSURE_CAPACITY_INSUFFICIENT",
            }
        )
        if "CLOSURE_CANDIDATE_CARDINALITY" in code_set and candidate_later_codes.intersection(code_set):
            issues.append(
                _issue(
                    "semantic_failure_phase",
                    ("market_proofs", index, "failure_codes"),
                    "candidate cardinality stops all later candidate predicates",
                )
            )
        if "CLOSURE_CANDIDATE_SCHEMA_INVALID" in code_set and candidate_later_codes.intersection(code_set):
            issues.append(
                _issue(
                    "semantic_failure_phase",
                    ("market_proofs", index, "failure_codes"),
                    "candidate schema failure stops all later candidate predicates",
                )
            )
        if {
            "CLOSURE_Q_CAP_RANGE",
            "CLOSURE_CAPACITY_INSUFFICIENT",
        }.issubset(code_set):
            issues.append(
                _issue(
                    "semantic_failure_phase",
                    ("market_proofs", index, "failure_codes"),
                    "candidate capacity cannot be compared without a representable Q-cap",
                )
            )
        if "CLOSURE_PROOF_ROW_COUNT_RANGE" in code_set and {
            "CLOSURE_Q_CAP_RANGE",
            "CLOSURE_REFERENCE_SET_INVALID",
        }.intersection(code_set):
            issues.append(
                _issue(
                    "semantic_failure_phase",
                    ("market_proofs", index, "failure_codes"),
                    "proof-row count runs only after Q-cap and reference-count derivation succeed",
                )
            )
        for field in (
            "earliest_trigger_equal_time_group",
            "q_cap_base_atoms",
            "capacity_base_atoms",
            "reference_price_count",
            "adverse_fill_extreme_count",
            "proof_row_count",
        ):
            value = row[field]
            if value is not None and _u64(value) is None:
                issues.append(
                    _issue(
                        "semantic_u64",
                        ("market_proofs", index, field),
                        "value exceeds the u64 authority range",
                    )
                )
        extreme_count = row["adverse_fill_extreme_count"]
        if extreme_count is not None and _u64(extreme_count) not in (1, 2):
            issues.append(
                _issue(
                    "semantic_extreme_count",
                    ("market_proofs", index, "adverse_fill_extreme_count"),
                    "valid-force structural rows require one or two fill extremes",
                )
            )

    any_h_missing = any("CLOSURE_H_MISSING" in codes for codes in row_codes)
    preproof_codes = frozenset(RUN_CLOSURE_MARKET_REASON_PRECEDENCE[:-1])
    any_preproof_failure = any(preproof_codes.intersection(codes) for codes in row_codes)
    global_proof_gate_open = document["proof_budget_status"] == "WITHIN_LIMIT" and not any_preproof_failure
    if any_h_missing and any("CLOSURE_STATE_ENVELOPE_RANGE" in codes for codes in row_codes):
        issues.append(
            _issue(
                "semantic_failure_phase",
                ("market_proofs",),
                "state-envelope predicates are globally unreachable when any trigger horizon is absent",
            )
        )
    if not global_proof_gate_open and any("CLOSURE_PROOF_PREDICATE_FAILED" in codes for codes in row_codes):
        issues.append(
            _issue(
                "semantic_failure_phase",
                ("market_proofs",),
                "proof-predicate failure is globally unreachable before pre-proof and budget gates pass",
            )
        )
    candidate_blockers = frozenset(
        {"CLOSURE_H_MISSING", "CLOSURE_CANDIDATE_CARDINALITY", "CLOSURE_CANDIDATE_SCHEMA_INVALID"}
    )

    def require_presence(index: int, row: Mapping[str, JsonValue], field: str, expected: bool) -> None:
        actual = row[field] is not None
        if actual != expected:
            issues.append(
                _issue(
                    "semantic_field_presence",
                    ("market_proofs", index, field),
                    f"field must be {'present' if expected else 'null'} under the closed failure matrix",
                )
            )

    for index, (row, codes) in enumerate(zip(market_rows, row_codes, strict=True)):
        code_set = frozenset(codes)
        h_present = "CLOSURE_H_MISSING" not in code_set
        candidate_present = h_present and code_set.isdisjoint(candidate_blockers)
        q_cap_present = "CLOSURE_Q_CAP_RANGE" not in code_set
        reference_present = "CLOSURE_REFERENCE_SET_INVALID" not in code_set
        proof_count_present = q_cap_present and reference_present and "CLOSURE_PROOF_ROW_COUNT_RANGE" not in code_set
        state_present = not any_h_missing and "CLOSURE_STATE_ENVELOPE_RANGE" not in code_set
        require_presence(index, row, "earliest_trigger_equal_time_group", h_present)
        for field in ("fill_event_id", "capacity_base_atoms", "fill_candidate_semantic_sha256"):
            require_presence(index, row, field, candidate_present)
        require_presence(index, row, "q_cap_base_atoms", q_cap_present)
        require_presence(index, row, "reference_price_count", reference_present)
        require_presence(index, row, "reference_set_root_sha256", reference_present)
        require_presence(index, row, "proof_row_count", proof_count_present)
        require_presence(index, row, "state_envelope_sha256", state_present)
        require_presence(index, row, "adverse_fill_extreme_count", True)
        require_presence(index, row, "proof_domain", global_proof_gate_open)
        require_presence(
            index,
            row,
            "proof_root_sha256",
            global_proof_gate_open and "CLOSURE_PROOF_PREDICATE_FAILED" not in code_set,
        )

    validated_config = document["validated_config_sha256"] is not None
    status = document["status"]
    counter_status = document["counter_capacity_status"]
    policy = document["run_end_position_policy"]
    budget_status = document["proof_budget_status"]
    proof_total = parsed_scalars["proof_row_count_total"]
    terminal_present = document["terminal_equal_time_group"] is not None

    if validated_config and policy == "FORCE_CLOSE_NEXT_EVENT":
        count_overflow = any("CLOSURE_PROOF_ROW_COUNT_RANGE" in codes for codes in row_codes)
        if count_overflow:
            expected_proof_total: int | None = None
        else:
            unbounded_total = sum(
                0
                if row["proof_row_count"] is None or _u64(row["proof_row_count"]) is None
                else _u64(row["proof_row_count"]) or 0
                for row in market_rows
            )
            expected_proof_total = unbounded_total if unbounded_total <= _MAX_U64 else None
        if proof_total != expected_proof_total:
            issues.append(
                _issue(
                    "semantic_proof_total",
                    ("proof_row_count_total",),
                    "proof-row total must equal the checked sum of all retained market-row counts",
                )
            )
        expected_budget = (
            "EXCEEDED"
            if expected_proof_total is None or (proof_limit is not None and expected_proof_total > proof_limit)
            else "WITHIN_LIMIT"
        )
        if budget_status != expected_budget:
            issues.append(
                _issue(
                    "semantic_proof_budget",
                    ("proof_budget_status",),
                    "proof budget status does not match the checked total and effective limit",
                )
            )

    if status == "FAIL" and terminal_present:
        issues.append(_issue("semantic_closure_tuple", ("terminal_equal_time_group",), "FAIL requires null terminal"))
    if status != "FAIL" and not terminal_present:
        issues.append(
            _issue("semantic_closure_tuple", ("terminal_equal_time_group",), "passing closure requires terminal group")
        )

    if not validated_config:
        if proof_limit is not None or proof_total != 0 or budget_status != "NOT_REQUIRED" or market_rows:
            issues.append(
                _issue(
                    "semantic_closure_layout",
                    ("proof_row_limit",),
                    "missing/invalid config always retains the zero-authority proof layout",
                )
            )
    elif policy == "LEAVE_MARKED_OPEN":
        if proof_limit is None or proof_total != 0 or budget_status != "NOT_REQUIRED" or market_rows:
            issues.append(
                _issue(
                    "semantic_closure_layout",
                    ("market_proofs",),
                    "valid leave-open always retains limit/zero/NOT_REQUIRED/empty proof layout",
                )
            )
    else:
        if not market_rows or proof_limit is None or budget_status == "NOT_REQUIRED":
            issues.append(
                _issue(
                    "semantic_closure_layout",
                    ("market_proofs",),
                    "valid force-close always retains complete market rows and a budget result",
                )
            )
        if budget_status == "EXCEEDED":
            if proof_total is not None and proof_limit is not None and proof_total <= proof_limit:
                issues.append(
                    _issue(
                        "semantic_proof_budget",
                        ("proof_row_count_total",),
                        "EXCEEDED requires total above limit or null",
                    )
                )
        elif budget_status == "WITHIN_LIMIT":
            if proof_total is None or (proof_limit is not None and proof_total > proof_limit):
                issues.append(
                    _issue("semantic_proof_budget", ("proof_row_count_total",), "WITHIN_LIMIT requires bounded total")
                )

    if counter_status == "EXCEEDED":
        if status != "FAIL" or reason_codes != (
            "ADMISSION_COUNTER_CAPACITY",
            "ADMISSION_RUN_END_UNCLOSED",
        ):
            issues.append(
                _issue("semantic_closure_tuple", ("reason_codes",), "counter exhaustion owns the exact failure tuple")
            )
    elif not validated_config:
        if status != "NOT_REQUIRED_ZERO_AUTHORITY" or reason_codes:
            issues.append(
                _issue(
                    "semantic_closure_tuple",
                    ("status",),
                    "missing/invalid config with capacity requires the exact zero-authority status tuple",
                )
            )
    elif policy == "LEAVE_MARKED_OPEN":
        if status != "PASS" or reason_codes:
            issues.append(_issue("semantic_closure_tuple", ("status",), "valid leave-open with capacity requires PASS"))
    elif budget_status == "EXCEEDED":
        if status != "FAIL" or reason_codes != (
            "ADMISSION_RUN_END_PROOF_BUDGET",
            "ADMISSION_RUN_END_UNCLOSED",
        ):
            issues.append(
                _issue("semantic_closure_tuple", ("reason_codes",), "proof budget exhaustion owns its exact tuple")
            )
    elif budget_status == "WITHIN_LIMIT":
        has_failure = any(row_codes)
        expected_status = "FAIL" if has_failure else "PASS"
        expected_reasons: tuple[str, ...] = ("ADMISSION_RUN_END_UNCLOSED",) if has_failure else ()
        if status != expected_status or reason_codes != expected_reasons:
            issues.append(
                _issue("semantic_closure_tuple", ("status",), "force-close status does not match row results")
            )
    return tuple(issues)


def validate_execution_quarantine_receipt_semantics(
    document: Mapping[str, JsonValue],
) -> tuple[ValidationIssue, ...]:
    """Validate exact out-of-band slot/component classifiers and reason totality."""
    issues: list[ValidationIssue] = []
    reason_codes, reason_issues = _check_reason_order(
        document["reason_codes"], QUARANTINE_REASON_PRECEDENCE, path=("reason_codes",)
    )
    issues.extend(reason_issues)
    slot_values = document["slot_observations"]
    component_values = document["component_observations"]
    assert isinstance(slot_values, list) and isinstance(component_values, list)
    if not slot_values and not component_values:
        issues.append(
            _issue("semantic_quarantine_totality", ("slot_observations",), "at least one observation is required")
        )

    head = document["last_verified_ledger_head_id"]
    state = document["last_verified_portfolio_state_id"]
    if (head is None) != (state is None):
        issues.append(_issue("semantic_anchor", ("last_verified_ledger_head_id",), "anchors have mixed nullability"))
    initial_prefix = head is None and state is None

    expected_reasons: set[str] = {"QUARANTINE_FOOTPRINT_MISMATCH"}
    if initial_prefix:
        expected_reasons.add("QUARANTINE_INITIAL_PREFIX_ANCHOR_MISSING")
    if document["expected_footprint_sha256"] == document["observed_footprint_sha256"]:
        issues.append(
            _issue(
                "semantic_footprint_mismatch",
                ("observed_footprint_sha256",),
                "mismatch observations require unequal expected and observed footprint digests",
            )
        )
    slot_keys: list[int] = []
    for index, value in enumerate(slot_values):
        assert isinstance(value, Mapping)
        row = value
        sequence = _u64(row["ledger_sequence"])
        if sequence is None:
            issues.append(
                _issue("semantic_u64", ("slot_observations", index, "ledger_sequence"), "slot exceeds u64 range")
            )
        else:
            slot_keys.append(sequence)
        classification = row["classification"]
        expected_id = row["expected_ledger_record_id"]
        observed_id = row["observed_ledger_record_id"]
        observed_bytes = row["observed_byte_sha256"]
        if classification == "MISSING":
            valid = expected_id is not None and observed_id is None and observed_bytes is None
            expected_reasons.add("QUARANTINE_LEDGER_SLOT_MISSING")
        elif classification == "WRONG":
            valid = expected_id is not None and observed_bytes is not None
            expected_reasons.update({"QUARANTINE_LEDGER_SLOT_WRONG", "QUARANTINE_APPEND_SLOT_OCCUPIED"})
        else:
            valid = expected_id is None and observed_bytes is not None
            expected_reasons.update({"QUARANTINE_LEDGER_SLOT_EXTRA", "QUARANTINE_APPEND_SLOT_OCCUPIED"})
        if not valid:
            issues.append(
                _issue(
                    "semantic_slot_classification",
                    ("slot_observations", index),
                    "slot observation violates classification nullability",
                )
            )
    if slot_keys != sorted(slot_keys) or len(slot_keys) != len(set(slot_keys)):
        issues.append(
            _issue("semantic_slot_order", ("slot_observations",), "slot observations must be unique numeric ascending")
        )
    elif any(right != left + 1 for left, right in pairwise(slot_keys)):
        issues.append(
            _issue(
                "semantic_slot_window",
                ("slot_observations",),
                "slot observations must form the exact contiguous unsafe window",
            )
        )
    first_unsafe = document["first_unsafe_ledger_sequence"]
    if slot_values:
        if _u64(first_unsafe) != (min(slot_keys) if slot_keys else None):
            issues.append(
                _issue("semantic_first_unsafe", ("first_unsafe_ledger_sequence",), "first unsafe slot must be minimal")
            )
    elif first_unsafe is not None:
        issues.append(
            _issue(
                "semantic_first_unsafe", ("first_unsafe_ledger_sequence",), "component-only receipt requires null slot"
            )
        )

    rank = {name: index for index, name in enumerate(QUARANTINE_COMPONENT_KIND_PRECEDENCE)}
    component_keys: list[tuple[int, int]] = []
    for index, value in enumerate(component_values):
        assert isinstance(value, Mapping)
        row = value
        kind = str(row["component_kind"])
        ordinal = row["ordinal"]
        parsed_ordinal = None if ordinal is None else _u64(ordinal)
        if kind == "CANONICAL_OBJECT":
            if parsed_ordinal is None:
                issues.append(
                    _issue(
                        "semantic_component_ordinal",
                        ("component_observations", index, "ordinal"),
                        "canonical objects require a u64 ordinal",
                    )
                )
            key_ordinal = -1 if parsed_ordinal is None else parsed_ordinal
        else:
            if ordinal is not None:
                issues.append(
                    _issue(
                        "semantic_component_ordinal",
                        ("component_observations", index, "ordinal"),
                        "scalar components require null ordinal",
                    )
                )
            key_ordinal = -1
        component_keys.append((rank[kind], key_ordinal))
        classification = row["classification"]
        expected_digest = row["expected_component_sha256"]
        observed_digest = row["observed_component_sha256"]
        if classification == "MISSING":
            valid = expected_digest is not None and observed_digest is None
            expected_reasons.add("QUARANTINE_COMPONENT_MISSING")
        elif classification == "WRONG":
            valid = expected_digest is not None and observed_digest is not None and expected_digest != observed_digest
            expected_reasons.add("QUARANTINE_COMPONENT_WRONG")
        else:
            valid = expected_digest is None and observed_digest is not None
            expected_reasons.add("QUARANTINE_COMPONENT_EXTRA")
        if not valid:
            issues.append(
                _issue(
                    "semantic_component_classification",
                    ("component_observations", index),
                    "component observation violates classification nullability",
                )
            )
    if component_keys != sorted(component_keys) or len(component_keys) != len(set(component_keys)):
        issues.append(
            _issue(
                "semantic_component_order",
                ("component_observations",),
                "component observations must be unique in normative rank/ordinal order",
            )
        )
    expected_ordered = tuple(code for code in QUARANTINE_REASON_PRECEDENCE if code in expected_reasons)
    if reason_codes != expected_ordered:
        issues.append(
            _issue("semantic_quarantine_reasons", ("reason_codes",), "reason set is not exhaustive for observations")
        )
    return tuple(issues)


_PRODUCER_SCOPE_FIELDS = (
    "adapter_sha256",
    "calibration_sha256",
    "calibration_version",
    "feature_set_version",
    "horizon_ns",
    "market_id",
    "model_artifact_sha256",
    "model_id",
    "model_version",
    "producer_id",
    "runtime_profile_id",
)


def validate_model_registry_semantics(document: Mapping[str, JsonValue]) -> tuple[ValidationIssue, ...]:
    """Validate immutable promotion scope keys, order, and scalar bounds."""
    issues: list[ValidationIssue] = []
    values = document["promotions"]
    assert isinstance(values, list)
    keys: list[str] = []
    for index, value in enumerate(values):
        assert isinstance(value, Mapping)
        row = value
        key = str(row["producer_scope_key_sha256"])
        keys.append(key)
        expected = sha256_hex(canonical_json_bytes({field: row[field] for field in _PRODUCER_SCOPE_FIELDS}))
        if key != expected:
            issues.append(
                _issue(
                    "semantic_scope_key",
                    ("promotions", index, "producer_scope_key_sha256"),
                    "scope key does not hash the exact immutable scope tuple",
                )
            )
        for field in ("horizon_ns", "max_ttl_ns"):
            parsed = _u64(row[field])
            if parsed is None or parsed == 0:
                issues.append(
                    _issue("semantic_positive_u64", ("promotions", index, field), "value must be a positive u64")
                )
        for field in ("max_uncertainty_q18", "max_ood_score_q18"):
            parsed = _u64(row[field])
            if parsed is None or parsed > _Q18_UNIT:
                issues.append(_issue("semantic_uq18", ("promotions", index, field), "value exceeds the uq18 domain"))
        for field in (
            "producer_id",
            "model_id",
            "model_version",
            "runtime_profile_id",
            "calibration_version",
            "feature_set_version",
            "market_id",
        ):
            if not _valid_utf8_registry(row[field], 128):
                issues.append(
                    _issue("semantic_registry", ("promotions", index, field), "registry string exceeds 128 UTF-8 bytes")
                )
    if keys != sorted(keys, key=_utf8_sort_key) or len(keys) != len(set(keys)):
        issues.append(
            _issue("semantic_promotion_set", ("promotions",), "promotion keys must be unique and UTF-8 sorted")
        )
    return tuple(issues)


def validate_model_signal_manifest_semantics(
    document: Mapping[str, JsonValue],
) -> tuple[ValidationIssue, ...]:
    """Validate the closed candidate envelope's local set and sort relations."""
    issues: list[ValidationIssue] = []
    values = document["candidates"]
    assert isinstance(values, list)
    paths: list[str] = []
    request_tuples: list[tuple[object, object, object]] = []
    sort_keys: list[tuple[bytes, int, int, int, bytes, int, bytes, bytes]] = []
    for index, value in enumerate(values):
        assert isinstance(value, Mapping)
        row = value
        path = str(row["relative_path"])
        paths.append(path)
        if unicodedata.normalize("NFC", path) != path:
            issues.append(
                _issue("semantic_path", ("candidates", index, "relative_path"), "path must be NFC-normalized")
            )
        parsed: dict[str, int] = {}
        for field in (
            "raw_byte_length",
            "decision_sequence",
            "horizon_ns",
            "issued_replay_clock_ns",
            "available_replay_clock_ns",
            "decision_close_replay_clock_ns",
        ):
            number = _u64(row[field])
            if number is None:
                issues.append(_issue("semantic_u64", ("candidates", index, field), "value exceeds u64"))
            else:
                parsed[field] = number
        producer_value = row["producer_sequence"]
        producer = None if producer_value is None else _u64(producer_value)
        if producer_value is not None and producer is None:
            issues.append(_issue("semantic_u64", ("candidates", index, "producer_sequence"), "value exceeds u64"))
        raw_length = parsed.get("raw_byte_length")
        if raw_length is not None and (raw_length == 0 or raw_length > 16_384):
            for field in ("declared_signal_id", "producer_sequence", "producer_scope_key_sha256"):
                if row[field] is not None:
                    issues.append(
                        _issue(
                            "semantic_hint_gate",
                            ("candidates", index, field),
                            "empty or oversized candidate bytes require all parsed hints null",
                        )
                    )
        request_tuples.append((row["requested_producer_scope_key_sha256"], producer_value, row["raw_signal_sha256"]))
        sort_keys.append(
            (
                _utf8_sort_key(str(row["requested_producer_scope_key_sha256"])),
                1 if producer is None else 0,
                0 if producer is None else producer,
                parsed.get("decision_sequence", -1),
                _utf8_sort_key(str(row["feature_snapshot_id"])),
                parsed.get("horizon_ns", -1),
                _utf8_sort_key(str(row["raw_signal_sha256"])),
                _utf8_sort_key(path),
            )
        )
    if len(paths) != len(set(paths)):
        issues.append(_issue("semantic_candidate_set", ("candidates",), "candidate paths must be unique"))
    if len(request_tuples) != len(set(request_tuples)):
        issues.append(
            _issue(
                "semantic_candidate_set", ("candidates",), "requested scope/sequence/raw digest tuples must be unique"
            )
        )
    if sort_keys != sorted(sort_keys):
        issues.append(
            _issue("semantic_candidate_order", ("candidates",), "candidate rows are not in canonical storage order")
        )
    return tuple(issues)


def validate_model_validation_receipt_semantics(
    document: Mapping[str, JsonValue],
) -> tuple[ValidationIssue, ...]:
    """Validate status derivation and the exact non-accepted fallback tuple."""
    issues: list[ValidationIssue] = []
    reason_codes, reason_issues = _check_reason_order(
        document["reason_codes"], MODEL_VALIDATION_REASON_PRECEDENCE, path=("reason_codes",)
    )
    issues.extend(reason_issues)
    reason_set = frozenset(reason_codes)
    identity_codes = frozenset(
        {
            "SIG_MODEL_UNPINNED",
            "SIG_RUNTIME_SUBSTITUTION",
            "SIG_SCOPE_MISMATCH",
            "SIG_FEATURE_MISMATCH",
            "SIG_CALIBRATION_UNKNOWN",
        }
    )
    sequence_codes = frozenset({"SIG_PRODUCER_SEQUENCE_INVALID", "SIG_REPLAYED"})
    drift_code = "SIG_DRIFT_DISABLED"

    def stage_issue(message: str) -> None:
        issues.append(_issue("semantic_validation_stage", ("reason_codes",), message))

    if "SIG_BYTES_INVALID" in reason_set and reason_set != {"SIG_BYTES_INVALID"}:
        stage_issue("byte-gate failure is the exact sole code and stops all later predicates")
    if "SIG_ORDER_SHAPED" in reason_set and "SIG_SCHEMA_UNKNOWN" not in reason_set:
        stage_issue("order-shaped evidence implies structural schema failure")
    if "SIG_SCHEMA_UNKNOWN" in reason_set and not reason_set <= {
        "SIG_SCHEMA_UNKNOWN",
        "SIG_ORDER_SHAPED",
    }:
        stage_issue("structural failure stops after the optional order-shaped predicate")
    if "SIG_ID_MISMATCH" in reason_set and reason_set != {"SIG_ID_MISMATCH"}:
        stage_issue("content-ID failure is the exact sole code and stops all later predicates")
    if identity_codes.intersection(reason_set) and not reason_set <= identity_codes | {drift_code}:
        stage_issue("identity/calibration failure stops before sequence and later predicates")
    reached_sequence_codes = sequence_codes.intersection(reason_set)
    if reached_sequence_codes and (
        len(reached_sequence_codes) != 1 or not reason_set <= reached_sequence_codes | {drift_code}
    ):
        stage_issue("sequence and replay failures are exclusive and stop later predicates")
    if "SIG_TIME_INVALID" in reason_set and {
        "SIG_EXPIRED",
        "SIG_DEADLINE_MISS",
    }.intersection(reason_set):
        stage_issue("invalid clock structure skips expiry and deadline predicates")

    if not reason_codes:
        expected_status = "ACCEPTED"
    elif reason_set == {"SIG_DRIFT_DISABLED"}:
        expected_status = "DRIFT_DISABLED"
    elif reason_set.issubset({"SIG_EXPIRED", "SIG_DEADLINE_MISS"}) and "SIG_EXPIRED" in reason_set:
        expected_status = "EXPIRED"
    else:
        expected_status = "REJECTED"
    if document["status"] != expected_status:
        issues.append(_issue("semantic_status", ("status",), "status does not match the complete reason set"))
    if "SIG_BYTES_INVALID" in reason_set:
        for field in (
            "declared_signal_id",
            "producer_sequence",
            "producer_scope_key_sha256",
            "issued_replay_clock_ns",
            "expires_replay_clock_ns",
        ):
            if document[field] is not None:
                issues.append(
                    _issue(
                        "semantic_byte_gate_evidence",
                        (field,),
                        "byte-invalid receipt cannot fabricate parsed candidate evidence",
                    )
                )
    elif "SIG_SCHEMA_UNKNOWN" not in reason_set:
        for field in (
            "declared_signal_id",
            "producer_sequence",
            "producer_scope_key_sha256",
            "issued_replay_clock_ns",
            "expires_replay_clock_ns",
        ):
            if document[field] is None:
                issues.append(
                    _issue(
                        "semantic_stage_evidence",
                        (field,),
                        "exact-schema and deeper stages require preserved typed candidate evidence",
                    )
                )

    for field in (
        "decision_sequence",
        "producer_sequence",
        "issued_replay_clock_ns",
        "available_replay_clock_ns",
        "decision_close_replay_clock_ns",
        "expires_replay_clock_ns",
    ):
        value = document[field]
        if value is not None and _u64(value) is None:
            issues.append(_issue("semantic_u64", (field,), "value exceeds the u64 authority range"))
    score = _i128(document["canonical_score_q18"])
    if score is None:
        issues.append(_issue("semantic_i128", ("canonical_score_q18",), "score exceeds signed-i128 range"))
    probability_fields = (
        "canonical_probability_abstain_q18",
        "canonical_probability_long_bias_q18",
        "canonical_probability_exit_bias_q18",
    )
    probabilities = [_u64(document[field]) for field in probability_fields]
    for field, value in zip(probability_fields, probabilities, strict=True):
        if value is None or value > _Q18_UNIT:
            issues.append(_issue("semantic_uq18", (field,), "probability exceeds the uq18 domain"))
    if (
        all(value is not None and value <= _Q18_UNIT for value in probabilities)
        and sum(value for value in probabilities if value is not None) != _Q18_UNIT
    ):
        issues.append(_issue("semantic_probability", probability_fields, "probabilities must sum exactly to unit"))
    for field in ("canonical_uncertainty_q18", "canonical_ood_score_q18"):
        value = _u64(document[field])
        if value is None or value > _Q18_UNIT:
            issues.append(_issue("semantic_uq18", (field,), "value exceeds the uq18 domain"))

    if expected_status == "ACCEPTED":
        if document["accepted_signal_id"] is None:
            issues.append(
                _issue("semantic_signal_tuple", ("accepted_signal_id",), "accepted status requires signal ID")
            )
    else:
        fallback = {
            "accepted_signal_id": None,
            "canonical_action": "ABSTAIN",
            "canonical_score_q18": "0",
            "canonical_probability_abstain_q18": str(_Q18_UNIT),
            "canonical_probability_long_bias_q18": "0",
            "canonical_probability_exit_bias_q18": "0",
            "canonical_uncertainty_q18": str(_Q18_UNIT),
            "canonical_ood_score_q18": str(_Q18_UNIT),
        }
        for field, expected in fallback.items():
            if document[field] != expected:
                issues.append(
                    _issue("semantic_fallback", (field,), "non-accepted receipt requires the exact fallback tuple")
                )
    return tuple(issues)


def validate_reconciliation_receipt_semantics(
    document: Mapping[str, JsonValue],
) -> tuple[ValidationIssue, ...]:
    """Validate reconciliation status, residual, ordering, and evidence-group shapes."""
    issues: list[ValidationIssue] = []
    reason_codes, reason_issues = _check_reason_order(
        document["reason_codes"],
        RECONCILIATION_REASON_PRECEDENCE,
        path=("reason_codes",),
    )
    issues.extend(reason_issues)
    status = document["status"]
    kind = document["reconciliation_kind"]
    if status == "PASS":
        if reason_codes:
            issues.append(_issue("semantic_reconciliation_status", ("reason_codes",), "PASS requires no reasons"))
    else:
        if len(reason_codes) < 2 or reason_codes[-1] != "RECONCILIATION_MISMATCH":
            issues.append(
                _issue(
                    "semantic_reconciliation_status",
                    ("reason_codes",),
                    "KILLED requires a nonempty reason tuple ending in RECONCILIATION_MISMATCH",
                )
            )
        if document["kill_latched"] is not True:
            issues.append(_issue("semantic_kill_latch", ("kill_latched",), "KILLED must latch the kill state"))
    if kind in ("GENESIS", "TERMINAL_KILL_PROMOTION") and status != "PASS":
        issues.append(_issue("semantic_reconciliation_kind", ("status",), f"{kind} must pass"))
    if kind in ("RISK_KILL", "TERMINAL_KILL_PROMOTION") and document["kill_latched"] is not True:
        issues.append(_issue("semantic_kill_latch", ("kill_latched",), f"{kind} must retain the kill latch"))
    special_reasons = {
        "IDEMPOTENCY_CONFLICT": (
            "RECONCILIATION_IDEMPOTENCY_CONFLICT",
            "RECONCILIATION_MISMATCH",
        ),
        "MODEL_ATTEMPT_INTEGRITY": (
            "RECONCILIATION_MODEL_ATTEMPT_INTEGRITY",
            "RECONCILIATION_MISMATCH",
        ),
        "EXECUTION_TRANSITION_INTEGRITY": (
            "RECONCILIATION_EXECUTION_TRANSITION_INTEGRITY",
            "RECONCILIATION_MISMATCH",
        ),
        "ARITHMETIC_RANGE": (
            "RECONCILIATION_ARITHMETIC_RANGE",
            "RECONCILIATION_MISMATCH",
        ),
    }
    expected_special = special_reasons.get(str(kind))
    if expected_special is not None and (status != "KILLED" or reason_codes != expected_special):
        issues.append(
            _issue(
                "semantic_reconciliation_kind",
                ("reason_codes",),
                f"{kind} requires its exact terminal reason tuple",
            )
        )
    if (
        kind == "FINAL_GATE"
        and status == "KILLED"
        and reason_codes
        != (
            "RECONCILIATION_RUN_END_UNCLOSED",
            "RECONCILIATION_MISMATCH",
        )
    ):
        issues.append(
            _issue("semantic_reconciliation_kind", ("reason_codes",), "killed FINAL_GATE has one exact reason tuple")
        )
    owner_by_reason = {
        "RECONCILIATION_IDEMPOTENCY_CONFLICT": "IDEMPOTENCY_CONFLICT",
        "RECONCILIATION_MODEL_ATTEMPT_INTEGRITY": "MODEL_ATTEMPT_INTEGRITY",
        "RECONCILIATION_EXECUTION_TRANSITION_INTEGRITY": "EXECUTION_TRANSITION_INTEGRITY",
        "RECONCILIATION_ARITHMETIC_RANGE": "ARITHMETIC_RANGE",
        "RECONCILIATION_RUN_END_UNCLOSED": "FINAL_GATE",
        "RECONCILIATION_INTENT_CARDINALITY": "GROUP_GATE",
        "RECONCILIATION_INTENT_RESERVATION_BIJECTION": "GROUP_GATE",
        "RECONCILIATION_ABSOLUTE_STATE_INVARIANT": "GROUP_GATE",
    }
    for code, owning_kind in owner_by_reason.items():
        if code in reason_codes and kind != owning_kind:
            issues.append(_issue("semantic_reason_owner", ("reason_codes",), f"{code} is owned only by {owning_kind}"))

    for field in ("decision_sequence", "ingest_sequence", "equal_time_group", "replay_clock_ns"):
        value = document[field]
        if value is not None and _u64(value) is None:
            issues.append(_issue("semantic_u64", (field,), "value exceeds the u64 authority range"))
    if kind == "GENESIS" and (
        document["decision_sequence"] is not None
        or document["ingest_sequence"] != "0"
        or document["equal_time_group"] != "0"
        or document["replay_clock_ns"] != "0"
    ):
        issues.append(
            _issue(
                "semantic_genesis_coordinates",
                ("reconciliation_kind",),
                "GENESIS has null decision and zero boundary coordinates",
            )
        )
    scalar_residual_fields = (
        "realized_pnl_residual_quote_atoms",
        "unrealized_pnl_residual_quote_atoms",
        "fee_residual_quote_atoms",
        "peak_equity_residual_quote_atoms",
        "drawdown_residual_bps",
        "equity_residual_quote_atoms",
    )
    scalar_residuals: dict[str, int | None] = {}
    for field in scalar_residual_fields:
        parsed = _i128(document[field])
        scalar_residuals[field] = parsed
        if parsed is None:
            issues.append(_issue("semantic_i128", (field,), "residual exceeds the signed-i128 range"))
        elif status == "PASS" and parsed != 0:
            issues.append(_issue("semantic_pass_residual", (field,), "PASS requires a zero residual"))
    unmatched = _u64(document["unmatched_reservation_count"])
    if unmatched is None:
        issues.append(_issue("semantic_u64", ("unmatched_reservation_count",), "value exceeds the u64 authority range"))
    elif status == "PASS" and unmatched != 0:
        issues.append(
            _issue(
                "semantic_pass_residual",
                ("unmatched_reservation_count",),
                "PASS requires no unmatched reservation",
            )
        )

    causes = document["causation_ids"]
    assert isinstance(causes, list)
    if causes != sorted(causes, key=_utf8_sort_key):
        issues.append(_issue("semantic_causation_order", ("causation_ids",), "causes must be digest-byte sorted"))
    expected_cause_count = (
        3
        if kind in ("FINAL_GATE", "IDEMPOTENCY_CONFLICT")
        else 4
        if kind
        in (
            "MODEL_ATTEMPT_INTEGRITY",
            "EXECUTION_TRANSITION_INTEGRITY",
        )
        else 1
    )
    if len(causes) != expected_cause_count:
        issues.append(_issue("semantic_causation_count", ("causation_ids",), "cause cardinality does not match kind"))

    asset_values = document["asset_residuals"]
    account_values = document["account_residuals"]
    assert isinstance(asset_values, list) and isinstance(account_values, list)
    asset_rows = [row for row in asset_values if isinstance(row, Mapping)]
    account_rows = [row for row in account_values if isinstance(row, Mapping)]
    if asset_rows != sorted(asset_rows, key=lambda row: _utf8_sort_key(str(row["asset_mint"]))):
        issues.append(_issue("semantic_asset_order", ("asset_residuals",), "asset residuals must be mint sorted"))
    asset_keys = [str(row["asset_mint"]) for row in asset_rows]
    if len(asset_keys) != len(set(asset_keys)):
        issues.append(_issue("semantic_asset_key", ("asset_residuals",), "asset mints must be unique"))
    if account_rows != sorted(
        account_rows,
        key=lambda row: (_utf8_sort_key(str(row["asset_mint"])), _utf8_sort_key(str(row["account"]))),
    ):
        issues.append(_issue("semantic_account_order", ("account_residuals",), "account residuals must be key sorted"))
    account_keys = [(str(row["asset_mint"]), str(row["account"])) for row in account_rows]
    if len(account_keys) != len(set(account_keys)):
        issues.append(_issue("semantic_account_key", ("account_residuals",), "asset/account keys must be unique"))
    for field, rows in (("asset_residuals", asset_rows), ("account_residuals", account_rows)):
        for index, row in enumerate(rows):
            residual = _i128(row["residual_atoms"])
            if residual is None:
                issues.append(
                    _issue("semantic_i128", (field, index, "residual_atoms"), "residual exceeds signed-i128 range")
                )
            elif status == "PASS" and residual != 0:
                issues.append(
                    _issue("semantic_pass_residual", (field, index, "residual_atoms"), "PASS requires zero residuals")
                )

    derived_residual_codes: set[str] = set()
    if any(_i128(row["residual_atoms"]) not in (None, 0) for row in account_rows):
        derived_residual_codes.add("RECONCILIATION_ACCOUNT_RESIDUAL")
    if any(_i128(row["residual_atoms"]) not in (None, 0) for row in asset_rows):
        derived_residual_codes.add("RECONCILIATION_ASSET_RESIDUAL")
    if scalar_residuals["realized_pnl_residual_quote_atoms"] not in (None, 0) or scalar_residuals[
        "unrealized_pnl_residual_quote_atoms"
    ] not in (None, 0):
        derived_residual_codes.add("RECONCILIATION_PNL_RESIDUAL")
    if scalar_residuals["fee_residual_quote_atoms"] not in (None, 0):
        derived_residual_codes.add("RECONCILIATION_FEE_RESIDUAL")
    if any(
        scalar_residuals[field] not in (None, 0)
        for field in ("peak_equity_residual_quote_atoms", "drawdown_residual_bps", "equity_residual_quote_atoms")
    ):
        derived_residual_codes.add("RECONCILIATION_EQUITY_RESIDUAL")
    if unmatched not in (None, 0):
        derived_residual_codes.add("RECONCILIATION_RESERVATION_RESIDUAL")
    owned_reason_by_kind = {
        "FINAL_GATE": {"RECONCILIATION_RUN_END_UNCLOSED"},
        "IDEMPOTENCY_CONFLICT": {"RECONCILIATION_IDEMPOTENCY_CONFLICT"},
        "MODEL_ATTEMPT_INTEGRITY": {"RECONCILIATION_MODEL_ATTEMPT_INTEGRITY"},
        "EXECUTION_TRANSITION_INTEGRITY": {"RECONCILIATION_EXECUTION_TRANSITION_INTEGRITY"},
        "ARITHMETIC_RANGE": {"RECONCILIATION_ARITHMETIC_RANGE"},
    }
    owned_reasons = set(owned_reason_by_kind.get(str(kind), set()))
    if kind == "GROUP_GATE":
        owned_reasons.update(
            code
            for code in (
                "RECONCILIATION_INTENT_CARDINALITY",
                "RECONCILIATION_INTENT_RESERVATION_BIJECTION",
                "RECONCILIATION_ABSOLUTE_STATE_INVARIANT",
            )
            if code in reason_codes
        )
    if status == "KILLED":
        applicable = derived_residual_codes | owned_reasons
        expected_reasons = tuple(
            code for code in RECONCILIATION_REASON_PRECEDENCE if code in applicable or code == "RECONCILIATION_MISMATCH"
        )
        if reason_codes != expected_reasons:
            issues.append(
                _issue(
                    "semantic_residual_reasons",
                    ("reason_codes",),
                    "reason codes must exactly match locally observable residuals and kind-owned invariants",
                )
            )

    grouped_fields = {
        "IDEMPOTENCY_CONFLICT": (
            "idempotency_conflict_scope",
            "idempotency_key_sha256",
            "original_object_id",
            "conflicting_body_sha256",
        ),
        "MODEL_ATTEMPT_INTEGRITY": ("integrity_validation_attempt_key_sha256",),
        "EXECUTION_TRANSITION_INTEGRITY": ("integrity_transition_key_sha256",),
        "ARITHMETIC_RANGE": (
            "arithmetic_range_key_sha256",
            "arithmetic_operands_sha256",
            "arithmetic_operation",
        ),
    }
    integrity_fields = (
        "integrity_expected_footprint_sha256",
        "integrity_observed_footprint_sha256",
        "integrity_ledger_head_before_check_id",
    )
    for owning_kind, fields in grouped_fields.items():
        for field in fields:
            if (kind == owning_kind) != (document[field] is not None):
                issues.append(
                    _issue("semantic_evidence_group", (field,), f"{field} is non-null exactly for {owning_kind}")
                )
    integrity_kind = kind in ("MODEL_ATTEMPT_INTEGRITY", "EXECUTION_TRANSITION_INTEGRITY")
    for field in integrity_fields:
        if integrity_kind != (document[field] is not None):
            issues.append(_issue("semantic_evidence_group", (field,), "integrity footprint fields are all-or-none"))
    if (
        integrity_kind
        and document["integrity_expected_footprint_sha256"] == document["integrity_observed_footprint_sha256"]
    ):
        issues.append(
            _issue(
                "semantic_integrity_difference",
                ("integrity_observed_footprint_sha256",),
                "integrity failure requires different expected and observed footprints",
            )
        )
    no_promotion_kinds = {
        "GENESIS",
        "GROUP_GATE",
        "FINAL_GATE",
        "IDEMPOTENCY_CONFLICT",
        "MODEL_ATTEMPT_INTEGRITY",
        "EXECUTION_TRANSITION_INTEGRITY",
        "ARITHMETIC_RANGE",
    }
    if kind in no_promotion_kinds and document["portfolio_state_before_id"] != document["portfolio_state_after_id"]:
        issues.append(_issue("semantic_state_identity", ("portfolio_state_after_id",), f"{kind} requires before=after"))
    if (
        kind == "TERMINAL_KILL_PROMOTION"
        and document["portfolio_state_before_id"] == document["portfolio_state_after_id"]
    ):
        issues.append(
            _issue(
                "semantic_state_identity",
                ("portfolio_state_after_id",),
                "TERMINAL_KILL_PROMOTION must promote a distinct protective state",
            )
        )
    exact_zero_kinds = {
        "GENESIS",
        "IDEMPOTENCY_CONFLICT",
        "MODEL_ATTEMPT_INTEGRITY",
        "EXECUTION_TRANSITION_INTEGRITY",
        "ARITHMETIC_RANGE",
        "TERMINAL_KILL_PROMOTION",
    }
    if kind in exact_zero_kinds and (
        asset_rows or account_rows or unmatched != 0 or any(value != 0 for value in scalar_residuals.values())
    ):
        issues.append(
            _issue(
                "semantic_zero_shape",
                ("reconciliation_kind",),
                f"{kind} requires empty touched rows and exact zero scalar residuals",
            )
        )
    return tuple(issues)


def validate_run_receipt_semantics(document: Mapping[str, JsonValue]) -> tuple[ValidationIssue, ...]:
    """Validate local run-input ordering, mode, schedule, and public-seed shape."""
    issues: list[ValidationIssue] = []
    parsed: dict[str, int] = {}
    for field in ("replay_tick_ns", "model_decision_budget_ns", "terminal_equal_time_group"):
        value = _u64(document[field])
        if value is None:
            issues.append(_issue("semantic_u64", (field,), "value exceeds the u64 authority range"))
        else:
            parsed[field] = value
    if parsed.get("replay_tick_ns") == 0:
        issues.append(_issue("semantic_replay_tick", ("replay_tick_ns",), "replay tick must be positive"))
    if parsed.get("terminal_equal_time_group") == 0:
        issues.append(
            _issue("semantic_terminal_group", ("terminal_equal_time_group",), "terminal group must be one-based")
        )

    source_values = document["source_admission_receipt_ids"]
    assert isinstance(source_values, list)
    if source_values != sorted(source_values, key=_utf8_sort_key):
        issues.append(
            _issue("semantic_source_order", ("source_admission_receipt_ids",), "source IDs must be digest-byte sorted")
        )

    scope_values = document["selected_model_scopes"]
    assert isinstance(scope_values, list)
    scopes = [row for row in scope_values if isinstance(row, Mapping)]
    scope_keys: list[tuple[int, bytes]] = []
    for index, row in enumerate(scopes):
        decision = _u64(row["decision_sequence"])
        horizon = _u64(row["horizon_ns"])
        if decision is None or decision == 0:
            issues.append(
                _issue(
                    "semantic_model_scope",
                    ("selected_model_scopes", index, "decision_sequence"),
                    "selected decision must be positive u64",
                )
            )
        if horizon is None:
            issues.append(
                _issue(
                    "semantic_u64",
                    ("selected_model_scopes", index, "horizon_ns"),
                    "horizon exceeds the u64 authority range",
                )
            )
        if decision is not None:
            scope_keys.append((decision, _utf8_sort_key(str(row["market_id"]))))
    if scope_keys != sorted(scope_keys):
        issues.append(
            _issue("semantic_model_scope_order", ("selected_model_scopes",), "scopes must be numeric-decision sorted")
        )
    if len(scope_keys) != len(set(scope_keys)):
        issues.append(
            _issue(
                "semantic_model_scope_key",
                ("selected_model_scopes",),
                "each decision/market scope key must be unique",
            )
        )

    group_values = document["availability_groups"]
    assert isinstance(group_values, list)
    groups = [row for row in group_values if isinstance(row, Mapping)]
    previous_slot = 0
    previous_cutoff = -1
    for index, row in enumerate(groups, start=1):
        slot = _u64(row["availability_slot"])
        group = _u64(row["equal_time_group"])
        cutoff = _u64(row["admission_cutoff"])
        if slot is None or group is None or cutoff is None:
            issues.append(_issue("semantic_u64", ("availability_groups", index - 1), "schedule values must fit u64"))
            continue
        if slot <= previous_slot or group != index or cutoff < previous_cutoff:
            issues.append(
                _issue(
                    "semantic_availability_schedule",
                    ("availability_groups", index - 1),
                    "slots increase, groups are contiguous, and cutoffs never decrease",
                )
            )
        previous_slot = slot
        previous_cutoff = cutoff

    tool_values = document["tool_versions"]
    assert isinstance(tool_values, list)
    tool_rows = [row for row in tool_values if isinstance(row, Mapping)]
    tool_names = [str(row["name"]) for row in tool_rows]
    if len(tool_names) != len(set(tool_names)) or tool_names != sorted(tool_names, key=_utf8_sort_key):
        issues.append(_issue("semantic_tool_order", ("tool_versions",), "tool names must be unique and UTF-8 sorted"))

    mode = document["model_signal_mode"]
    registry_id = document["model_registry_sha256"]
    manifest_id = document["model_signal_manifest_sha256"]
    budget = parsed.get("model_decision_budget_ns")
    tick = parsed.get("replay_tick_ns")
    if mode == "DISABLED":
        if registry_id is not None or manifest_id is not None or scopes or budget != 0:
            issues.append(
                _issue(
                    "semantic_model_mode",
                    ("model_signal_mode",),
                    "disabled mode has null IDs, no scopes, and zero budget",
                )
            )
    elif (
        registry_id is None
        or manifest_id is None
        or not scopes
        or budget is None
        or tick is None
        or not 0 < budget < tick
    ):
        issues.append(
            _issue(
                "semantic_model_mode",
                ("model_signal_mode",),
                "cached mode requires both IDs, scopes, and a positive sub-tick budget",
            )
        )

    seed_hex = document["public_seed_hex"]
    assert isinstance(seed_hex, str)
    try:
        seed_bytes = bytes.fromhex(seed_hex)
    except ValueError:
        seed_bytes = b""
    if (
        len(seed_bytes) != 32
        or seed_bytes.hex() != seed_hex
        or sha256_hex(seed_bytes) != document["public_seed_sha256"]
    ):
        issues.append(
            _issue("semantic_public_seed", ("public_seed_hex",), "seed hex must decode to and hash the exact 32 bytes")
        )
    return tuple(issues)


def validate_benchmark_measurement_semantics(
    document: Mapping[str, JsonValue],
) -> tuple[ValidationIssue, ...]:
    """Validate retained sample arrays and the total raw-counter range mapping."""
    issues: list[ValidationIssue] = []
    phase_order = ("admission", "feature", "risk", "fill", "accounting", "end_to_end")
    phase_units = {
        "admission": "RAW_EVENT",
        "feature": "FEATURE_SNAPSHOT",
        "risk": "RISK_DECISION",
        "fill": "SIMULATED_FILL_RECEIPT",
        "accounting": "RECONCILIATION_RECEIPT",
        "end_to_end": "EQUAL_TIME_GROUP",
    }
    phase_values = document["phase_samples"]
    assert isinstance(phase_values, list)
    phase_rows = [row for row in phase_values if isinstance(row, Mapping)]
    if tuple(row.get("phase") for row in phase_rows) != phase_order:
        issues.append(_issue("semantic_phase_order", ("phase_samples",), "phase rows must use the fixed order"))
    for index, row in enumerate(phase_rows):
        phase = row.get("phase")
        if isinstance(phase, str) and row.get("unit") != phase_units.get(phase):
            issues.append(
                _issue("semantic_phase_unit", ("phase_samples", index, "unit"), "phase work unit is not canonical")
            )
        samples = row.get("samples_ns")
        assert isinstance(samples, list)
        sample_count = _u64(row.get("sample_count"))
        if sample_count != len(samples):
            issues.append(
                _issue(
                    "semantic_sample_count",
                    ("phase_samples", index, "sample_count"),
                    "sample_count must equal the retained sample-array length",
                )
            )
        for sample_index, sample in enumerate(samples):
            if _u64(sample) is None:
                issues.append(
                    _issue(
                        "semantic_u64",
                        ("phase_samples", index, "samples_ns", sample_index),
                        "individual timer sample exceeds the u64 authority range",
                    )
                )

    if len(phase_rows) == len(phase_order):
        admission_count = _u64(phase_rows[0].get("sample_count"))
        end_to_end_count = _u64(phase_rows[-1].get("sample_count"))
        if admission_count != _u64(document["measured_event_count"]):
            issues.append(
                _issue(
                    "semantic_measurement_census",
                    ("phase_samples", 0, "sample_count"),
                    "admission samples must equal measured_event_count",
                )
            )
        if end_to_end_count != _u64(document["measured_group_count"]):
            issues.append(
                _issue(
                    "semantic_measurement_census",
                    ("phase_samples", 5, "sample_count"),
                    "end-to-end samples must equal measured_group_count",
                )
            )

    raw_values = document["raw_counters"]
    assert isinstance(raw_values, list)
    raw_rows = [row for row in raw_values if isinstance(row, Mapping)]
    if tuple(row.get("counter") for row in raw_rows) != BENCHMARK_RANGE_FIELD_PRECEDENCE:
        issues.append(_issue("semantic_counter_order", ("raw_counters",), "raw counters must use fixed precedence"))
    raw_by_counter = {str(row.get("counter")): row.get("raw_value") for row in raw_rows}

    if len(phase_rows) == len(phase_order):
        end_samples = phase_rows[-1].get("samples_ns")
        assert isinstance(end_samples, list)
        parsed_end_samples = tuple(_u64(sample) for sample in end_samples)
        if all(sample is not None for sample in parsed_end_samples):
            measured_sum = sum(sample for sample in parsed_end_samples if sample is not None)
            if raw_by_counter.get("MEASURED_WALL_DURATION") != str(measured_sum):
                issues.append(
                    _issue(
                        "semantic_measured_wall",
                        ("raw_counters", 0, "raw_value"),
                        "measured wall raw value must equal the unbounded end-to-end sample sum",
                    )
                )

    converted_fields = {
        "MEASURED_WALL_DURATION": "measured_wall_duration_ns",
        "PROCESS_CPU_TIME": "process_cpu_time_ns",
        "WALL_DURATION": "wall_duration_ns",
        "PEAK_RSS_BYTES": "peak_rss_bytes",
        "ALLOCATION_COUNT": "allocation_count",
        "INPUT_BYTES": "input_bytes",
        "OUTPUT_BYTES": "output_bytes",
    }
    failed_fields: list[str] = []
    for counter in BENCHMARK_RANGE_FIELD_PRECEDENCE:
        raw = raw_by_counter.get(counter)
        field = converted_fields[counter]
        if raw is None:
            if counter != "ALLOCATION_COUNT" or document[field] is not None:
                issues.append(
                    _issue("semantic_raw_counter", (field,), "only unsupported allocation count may map null to null")
                )
            continue
        parsed = _uint(raw)
        if parsed is None:
            continue
        if parsed > _MAX_U64:
            failed_fields.append(counter)
            if document[field] is not None:
                issues.append(_issue("semantic_range_mapping", (field,), "out-of-range counter must map to null"))
        elif document[field] != raw:
            issues.append(_issue("semantic_range_mapping", (field,), "fitting counter must preserve its exact value"))

    declared_failures = document["range_failure_fields"]
    assert isinstance(declared_failures, list)
    if tuple(declared_failures) != tuple(failed_fields):
        issues.append(
            _issue(
                "semantic_range_failures",
                ("range_failure_fields",),
                "range failures must be the complete precedence-ordered conversion failures",
            )
        )
    expected_status = "RANGE_FAILED" if failed_fields else "COMPLETE"
    if document["measurement_status"] != expected_status:
        issues.append(
            _issue("semantic_measurement_status", ("measurement_status",), "status must derive from range failures")
        )
    expected_first = failed_fields[0] if failed_fields else None
    if document["first_range_failure"] != expected_first:
        issues.append(
            _issue("semantic_first_range_failure", ("first_range_failure",), "first failure must follow precedence")
        )

    warmup_events = _u64(document["warmup_event_count"])
    warmup_groups = _u64(document["warmup_group_count"])
    boundary = document["warmup_through_equal_time_group"]
    if warmup_events == 0 and warmup_groups == 0:
        if boundary is not None:
            issues.append(
                _issue("semantic_warmup_boundary", ("warmup_through_equal_time_group",), "zero warmup has no boundary")
            )
    elif boundary is None:
        issues.append(
            _issue("semantic_warmup_boundary", ("warmup_through_equal_time_group",), "warmup requires a boundary")
        )
    return tuple(issues)


def validate_benchmark_receipt_semantics(document: Mapping[str, JsonValue]) -> tuple[ValidationIssue, ...]:
    """Validate benchmark admission/terminal shape without executing or aggregating a benchmark."""
    issues: list[ValidationIssue] = []
    reason_codes, reason_issues = _check_reason_order(
        document["reason_codes"],
        BENCHMARK_REASON_PRECEDENCE,
        path=("reason_codes",),
    )
    issues.extend(reason_issues)
    status = document["status"]
    sample_count = _u64(document["sample_count"])
    if sample_count is None:
        issues.append(_issue("semantic_u64", ("sample_count",), "sample count exceeds the u64 authority range"))
    output_values = document["run_outputs"]
    assert isinstance(output_values, list)
    outputs = [row for row in output_values if isinstance(row, Mapping)]
    preflight_reasons = frozenset(BENCHMARK_REASON_PRECEDENCE[:4])
    scheduled_reasons = frozenset(BENCHMARK_REASON_PRECEDENCE[4:])
    if status == "INELIGIBLE":
        if (
            not reason_codes
            or any(code not in preflight_reasons for code in reason_codes)
            or sample_count != 0
            or outputs
            or document["metrics_artifact_sha256"] is not None
        ):
            issues.append(
                _issue(
                    "semantic_benchmark_status",
                    ("status",),
                    "ineligible receipt has reasons, zero samples, no outputs, and no metrics",
                )
            )
    else:
        attachment_fields = (
            "benchmark_manifest_sha256",
            "preregistered_thresholds_sha256",
            "hardware_profile_sha256",
            "metrics_artifact_sha256",
        )
        if any(document[field] is None for field in attachment_fields) or not outputs:
            issues.append(
                _issue(
                    "semantic_benchmark_status",
                    ("status",),
                    "scheduled receipt requires all validated hashes, metrics, and outputs",
                )
            )
        if status == "PASS" and (reason_codes or sample_count in (None, 0)):
            issues.append(_issue("semantic_benchmark_status", ("status",), "PASS requires no reasons and samples"))
        if status == "FAIL" and (not reason_codes or any(code not in scheduled_reasons for code in reason_codes)):
            issues.append(
                _issue("semantic_benchmark_status", ("reason_codes",), "FAIL requires scheduled-only reasons")
            )

    output_keys: list[tuple[bytes, bytes, int]] = []
    repetitions: dict[tuple[str, str], list[int]] = {}
    has_failed_run = False
    for index, row in enumerate(outputs):
        repetition = _u64(row["repetition_index"])
        if repetition is None:
            issues.append(
                _issue(
                    "semantic_u64",
                    ("run_outputs", index, "repetition_index"),
                    "repetition exceeds the u64 authority range",
                )
            )
            continue
        case_id = str(row["case_id"])
        run_id = str(row["run_receipt_id"])
        output_keys.append((_utf8_sort_key(case_id), _utf8_sort_key(run_id), repetition))
        repetitions.setdefault((case_id, run_id), []).append(repetition)
        terminal = row["terminal_status"]
        has_failed_run = has_failed_run or terminal != "RUN_END"
        expected_nonnull: set[str]
        if terminal == "RUN_END":
            expected_nonnull = {"ledger_root_id"}
        elif terminal == "KILLED":
            expected_nonnull = {"failure_receipt_id"}
        elif terminal == "QUARANTINED":
            expected_nonnull = {"execution_quarantine_receipt_id"}
        else:
            expected_nonnull = {"process_failure_code", "stdout_sha256", "stderr_sha256"}
        terminal_fields = {
            "ledger_root_id",
            "failure_receipt_id",
            "execution_quarantine_receipt_id",
            "process_failure_code",
            "stdout_sha256",
            "stderr_sha256",
        }
        actual_nonnull = {field for field in terminal_fields if row[field] is not None}
        if actual_nonnull != expected_nonnull:
            issues.append(
                _issue(
                    "semantic_terminal_shape",
                    ("run_outputs", index),
                    "terminal-specific fields do not match terminal_status",
                )
            )
        if terminal != "PROCESS_FAILED" and row["process_exit_code"] is not None:
            issues.append(
                _issue(
                    "semantic_terminal_shape",
                    ("run_outputs", index, "process_exit_code"),
                    "exit code is process-failure-only",
                )
            )
        if row["process_exit_code"] is not None and _i128(row["process_exit_code"]) is None:
            issues.append(
                _issue(
                    "semantic_i128",
                    ("run_outputs", index, "process_exit_code"),
                    "process exit code exceeds signed-i128 range",
                )
            )
        if terminal == "PROCESS_FAILED":
            failure_code = row["process_failure_code"]
            exit_code = None if row["process_exit_code"] is None else _i128(row["process_exit_code"])
            if failure_code == "LAUNCH_FAILED" and (
                row["process_exit_code"] is not None or row["output_sha256"] != sha256_hex(b"")
            ):
                issues.append(
                    _issue(
                        "semantic_process_failure",
                        ("run_outputs", index),
                        "LAUNCH_FAILED has no exit status and hashes empty functional output",
                    )
                )
            if failure_code == "NONZERO_EXIT" and (exit_code is None or exit_code == 0):
                issues.append(
                    _issue(
                        "semantic_process_failure",
                        ("run_outputs", index, "process_exit_code"),
                        "NONZERO_EXIT requires an exact nonzero signed exit status",
                    )
                )
    if output_keys != sorted(output_keys):
        issues.append(_issue("semantic_run_output_order", ("run_outputs",), "run outputs are not canonically sorted"))
    if any(values != list(range(len(values))) for values in repetitions.values()):
        issues.append(
            _issue("semantic_repetition_sequence", ("run_outputs",), "repetitions must be contiguous from zero")
        )
    has_reason = "BENCHMARK_RUN_FAILED" in reason_codes
    if has_reason != has_failed_run:
        issues.append(
            _issue(
                "semantic_benchmark_run_failure",
                ("reason_codes",),
                "BENCHMARK_RUN_FAILED must exactly match non-RUN_END outcomes",
            )
        )
    return tuple(issues)


SEMANTIC_VALIDATORS: dict[str, SemanticValidator] = {
    "trading.raw-event/v1": validate_raw_event_semantics,
    "trading.feature-snapshot/v1": validate_feature_snapshot_semantics,
    "trading.model-signal/v1": validate_model_signal_semantics,
    "trading.risk-decision/v1": validate_risk_decision_semantics,
    "trading.simulated-order-intent/v1": validate_simulated_order_intent_semantics,
    "trading.simulated-fill-receipt/v1": validate_simulated_fill_receipt_semantics,
    "trading.portfolio-state/v1": validate_portfolio_state_semantics,
    "trading.ledger-record/v1": validate_ledger_record_semantics,
}

SUPPORTING_SEMANTIC_VALIDATORS: dict[str, SemanticValidator] = {
    "trading.fixture-manifest/v1": validate_fixture_manifest_semantics,
    "trading.replay-risk-config/v1": validate_replay_risk_config_semantics,
    "trading.source-admission-receipt/v1": validate_source_admission_receipt_semantics,
    "trading.config-admission-receipt/v1": validate_config_admission_receipt_semantics,
    "trading.run-closure-receipt/v1": validate_run_closure_receipt_semantics,
    "trading.execution-quarantine-receipt/v1": validate_execution_quarantine_receipt_semantics,
    "trading.model-registry/v1": validate_model_registry_semantics,
    "trading.model-signal-manifest/v1": validate_model_signal_manifest_semantics,
    "trading.model-validation-receipt/v1": validate_model_validation_receipt_semantics,
    "trading.reconciliation-receipt/v1": validate_reconciliation_receipt_semantics,
    "trading.run-receipt/v1": validate_run_receipt_semantics,
    "trading.benchmark-measurement/v1": validate_benchmark_measurement_semantics,
    "trading.benchmark-receipt/v1": validate_benchmark_receipt_semantics,
}


_POSITIVE_U64_DECIMAL_PATTERN = "^[1-9][0-9]*$"
_ATTACHMENT_COUNT_BINDINGS = {
    "trading.benchmark-manifest/v1": (("case_count", "cases"),),
    "trading.benchmark-metrics/v1": (("measurement_count", "metrics"),),
    "trading.normalized-event-set/v1": (("raw_event_count", "event_ids"),),
}


def _json_type_matches(value: object, expected: str) -> bool:
    if expected == "null":
        return value is None
    if expected == "boolean":
        return isinstance(value, bool)
    if expected == "object":
        return isinstance(value, Mapping)
    if expected == "array":
        return isinstance(value, list)
    if expected == "integer":
        return not isinstance(value, bool) and isinstance(value, int)
    if expected == "number":
        return not isinstance(value, bool) and isinstance(value, (int, float))
    if expected == "string":
        return isinstance(value, str)
    return False


def _resolve_attachment_schema_reference(
    reference: str,
    root: Mapping[str, Any],
) -> tuple[Mapping[str, Any], Mapping[str, Any]] | None:
    if reference.startswith("#/$defs/"):
        target: object = root
        for part in reference[2:].split("/"):
            if not isinstance(target, Mapping) or part not in target:
                return None
            target = target[part]
        if isinstance(target, Mapping):
            return target, root
        return None
    for document in ATTACHMENT_SCHEMA_DOCUMENTS.values():
        if document.get("$id") == reference:
            return document, document
    return None


def _schema_declared_type_matches(schema: Mapping[str, Any], value: object, root: Mapping[str, Any]) -> bool:
    reference = schema.get("$ref")
    if isinstance(reference, str):
        resolved = _resolve_attachment_schema_reference(reference, root)
        if resolved is None:
            return True
        target, target_root = resolved
        return _schema_declared_type_matches(target, value, target_root)

    declared_type = schema.get("type")
    if isinstance(declared_type, str):
        return _json_type_matches(value, declared_type)
    if isinstance(declared_type, list) and all(isinstance(item, str) for item in declared_type):
        return any(_json_type_matches(value, item) for item in declared_type)
    return True


def _attachment_alias_issue(
    alias: str,
    value: object,
    path: tuple[str | int, ...],
) -> ValidationIssue | None:
    if alias == "u64s":
        if _u64(value) is None:
            return _issue("semantic_attachment_u64", path, "attachment value exceeds the u64 authority range")
    elif alias in {"i128s", "sq18s"}:
        if _i128(value) is None:
            return _issue("semantic_attachment_i128", path, "attachment value exceeds the signed-i128 range")
    elif alias == "uq18s":
        parsed = _u64(value)
        if parsed is None or parsed > _Q18_UNIT:
            return _issue("semantic_attachment_uq18", path, "attachment value exceeds the uq18 range")
    elif alias == "uints" and _uint(value) is None:
        return _issue("semantic_attachment_uint", path, "attachment value is not an unsigned decimal string")
    return None


def _positive_u64_issue(value: object, path: tuple[str | int, ...]) -> ValidationIssue | None:
    try:
        parse_bounded_decimal_string(value, minimum=1, maximum=_MAX_U64)
    except ValueError:
        return _issue("semantic_attachment_positive_u64", path, "attachment value must be in [1, u64::MAX]")
    return None


def _validate_attachment_alias_ranges_at(
    schema: Mapping[str, Any],
    value: object,
    path: tuple[str | int, ...],
    root: Mapping[str, Any],
    active_refs: frozenset[tuple[int, int]],
) -> tuple[ValidationIssue, ...]:
    issues: list[ValidationIssue] = []

    for keyword in ("anyOf", "oneOf"):
        branches = schema.get(keyword)
        if isinstance(branches, list):
            for branch in branches:
                if isinstance(branch, Mapping) and _schema_declared_type_matches(branch, value, root):
                    issues.extend(_validate_attachment_alias_ranges_at(branch, value, path, root, active_refs))
            return tuple(issues)

    branches = schema.get("allOf")
    if isinstance(branches, list):
        for branch in branches:
            if isinstance(branch, Mapping):
                issues.extend(_validate_attachment_alias_ranges_at(branch, value, path, root, active_refs))

    reference = schema.get("$ref")
    if isinstance(reference, str):
        alias = reference.removeprefix("#/$defs/") if reference.startswith("#/$defs/") else None
        if alias is not None:
            issue = _attachment_alias_issue(alias, value, path)
            return () if issue is None else (issue,)
        resolved = _resolve_attachment_schema_reference(reference, root)
        if resolved is None:
            return tuple(issues)
        target, target_root = resolved
        marker = (id(target), id(value))
        if marker in active_refs:
            return tuple(issues)
        issues.extend(_validate_attachment_alias_ranges_at(target, value, path, target_root, active_refs | {marker}))
        return tuple(issues)

    if not _schema_declared_type_matches(schema, value, root):
        return tuple(issues)

    if schema.get("type") == "string" and schema.get("pattern") == _POSITIVE_U64_DECIMAL_PATTERN:
        issue = _positive_u64_issue(value, path)
        if issue is not None:
            issues.append(issue)

    if isinstance(value, Mapping):
        properties = schema.get("properties")
        if isinstance(properties, Mapping):
            for name, property_schema in properties.items():
                if isinstance(name, str) and name in value and isinstance(property_schema, Mapping):
                    issues.extend(
                        _validate_attachment_alias_ranges_at(
                            property_schema,
                            value[name],
                            (*path, name),
                            root,
                            active_refs,
                        )
                    )

    if isinstance(value, list):
        items = schema.get("items")
        if isinstance(items, Mapping):
            for index, item in enumerate(value):
                issues.extend(_validate_attachment_alias_ranges_at(items, item, (*path, index), root, active_refs))

    return tuple(issues)


def validate_attachment_alias_ranges(document: Mapping[str, JsonValue]) -> tuple[ValidationIssue, ...]:
    schema_id = document.get("schema")
    if not isinstance(schema_id, str):
        return ()
    schema = ATTACHMENT_SCHEMA_DOCUMENTS.get(schema_id)
    if schema is None:
        return ()
    return _validate_attachment_alias_ranges_at(schema, document, (), schema, frozenset())


def validate_attachment_count_semantics(document: Mapping[str, JsonValue]) -> tuple[ValidationIssue, ...]:
    schema_id = document.get("schema")
    if not isinstance(schema_id, str):
        return ()
    issues: list[ValidationIssue] = []
    for count_field, array_field in _ATTACHMENT_COUNT_BINDINGS.get(schema_id, ()):
        count = _u64(document[count_field])
        array_value = document[array_field]
        if count is not None and isinstance(array_value, list) and count != len(array_value):
            issues.append(
                _issue(
                    "semantic_attachment_count",
                    (count_field,),
                    f"{count_field} must equal {array_field} length",
                )
            )
    return tuple(issues)


def validate_common_attachment_semantics(document: Mapping[str, JsonValue]) -> tuple[ValidationIssue, ...]:
    return (*validate_attachment_alias_ranges(document), *validate_attachment_count_semantics(document))


def validate_benchmark_request_attachment_semantics(document: Mapping[str, JsonValue]) -> tuple[ValidationIssue, ...]:
    """Validate raw benchmark attachment digest/length totality."""
    issues: list[ValidationIssue] = []
    for digest_field, length_field in (
        ("benchmark_manifest_raw_sha256", "benchmark_manifest_raw_byte_length"),
        ("preregistered_thresholds_raw_sha256", "preregistered_thresholds_raw_byte_length"),
        ("hardware_profile_raw_sha256", "hardware_profile_raw_byte_length"),
    ):
        length = _u64(document[length_field])
        if length is None:
            issues.append(_issue("semantic_u64", (length_field,), "raw byte length exceeds the u64 authority range"))
            continue
        if document[digest_field] is None and length != 0:
            issues.append(
                _issue(
                    "semantic_raw_totality",
                    (length_field,),
                    "absent retained bytes require zero byte length",
                )
            )
    return tuple(issues)


def validate_force_close_state_envelope_semantics(document: Mapping[str, JsonValue]) -> tuple[ValidationIssue, ...]:
    """Validate unsigned u64 force-close state aliases."""
    issues: list[ValidationIssue] = []
    numeric_fields = set(document) - {"schema", "market_id"}
    for field in numeric_fields:
        value = _u64(document[field])
        if value is None:
            issues.append(
                _issue(
                    "semantic_force_close_state_range",
                    (field,),
                    "force-close state alias must be a u64 value",
                )
            )
    return tuple(issues)


def validate_run_closure_full_fill_proof_row_semantics(
    document: Mapping[str, JsonValue],
) -> tuple[ValidationIssue, ...]:
    """Validate force-close proof row bounded integer aliases."""
    issues: list[ValidationIssue] = []
    for field in ("reference_price_q18", "cash_delta_quote_atoms", "execution_price_q18"):
        if _i128(document[field]) is None:
            issues.append(
                _issue("semantic_force_proof_row_range", (field,), "proof row signed alias exceeds i128 range")
            )
    for field in (
        "residual_base_atoms",
        "capacity_atoms",
        "filled_base_atoms",
        "unfilled_base_atoms",
        "gross_quote_atoms",
        "venue_fee_quote_atoms",
        "priority_fee_quote_atoms",
        "simulation_fee_quote_atoms",
    ):
        if _u64(document[field]) is None:
            issues.append(_issue("semantic_u64", (field,), "proof row unsigned alias exceeds u64 range"))
    return tuple(issues)


def validate_run_closure_full_fill_proof_set_semantics(
    document: Mapping[str, JsonValue],
) -> tuple[ValidationIssue, ...]:
    """Validate force-close proof row cap, cardinality, uniqueness, and order."""
    issues: list[ValidationIssue] = []
    rows_value = document["rows"]
    assert isinstance(rows_value, list)
    rows = [row for row in rows_value if isinstance(row, Mapping)]
    proof_row_count = _u64(document["proof_row_count"])
    if proof_row_count is not None and proof_row_count > _MAX_RUN_CLOSURE_PROOF_ROWS_V0:
        issues.append(
            _issue(
                "semantic_proof_row_cap",
                ("proof_row_count",),
                "proof row count exceeds the protocol maximum",
            )
        )
        return tuple(issues)
    if proof_row_count != len(rows):
        issues.append(_issue("semantic_proof_row_count", ("proof_row_count",), "proof row count must equal rows"))

    def row_key(row: Mapping[str, JsonValue]) -> tuple[int, int, int] | None:
        reference = _i128(row["reference_price_q18"])
        residual = _u64(row["residual_base_atoms"])
        adverse = row["adverse_fill_bps"]
        if reference is None or residual is None or not isinstance(adverse, int):
            return None
        return (reference, residual, adverse)

    row_keys: list[tuple[int, int, int]] = []
    seen_keys: set[tuple[int, int, int]] = set()
    for index, row in enumerate(rows):
        key = row_key(row)
        if key is None:
            continue
        if key in seen_keys:
            issues.append(
                _issue("semantic_proof_row_duplicate", ("rows", index), "proof row semantic key is duplicated")
            )
            continue
        seen_keys.add(key)
        row_keys.append(key)

    if row_keys != sorted(row_keys):
        issues.append(_issue("semantic_proof_row_order", ("rows",), "proof rows are not deterministically sorted"))
    return tuple(issues)


ATTACHMENT_SEMANTIC_VALIDATORS: dict[str, SemanticValidator] = {
    "trading.benchmark-request/v1": validate_benchmark_request_attachment_semantics,
    "trading.force-close-state-envelope/v1": validate_force_close_state_envelope_semantics,
    "trading.run-closure-full-fill-proof-row/v1": validate_run_closure_full_fill_proof_row_semantics,
    "trading.run-closure-full-fill-proof-set/v1": validate_run_closure_full_fill_proof_set_semantics,
}


def validate_contract_semantics(document: Mapping[str, JsonValue]) -> tuple[ValidationIssue, ...]:
    """Dispatch one structurally valid document to its pure semantic validator."""
    schema_id = document.get("schema")
    if not isinstance(schema_id, str):
        return ()
    issues: list[ValidationIssue] = []
    if schema_id in ATTACHMENT_SCHEMA_DOCUMENTS:
        issues.extend(validate_common_attachment_semantics(document))
    validator = SEMANTIC_VALIDATORS.get(schema_id)
    if validator is None:
        validator = SUPPORTING_SEMANTIC_VALIDATORS.get(schema_id)
    if validator is None:
        validator = ATTACHMENT_SEMANTIC_VALIDATORS.get(schema_id)
    if validator is not None:
        issues.extend(validator(document))
    return tuple(issues)
