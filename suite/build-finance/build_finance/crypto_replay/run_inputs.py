"""Pure contract-only run-input structures, derivations, and verification.

This module has no file, network, provider, model, replay, ledger, or execution
capability.  It derives and verifies only closed structural evidence from
already-retained in-memory bodies.
"""

from __future__ import annotations

from collections.abc import Mapping
from copy import deepcopy
from dataclasses import dataclass
from types import MappingProxyType
from typing import Any, Literal, cast

from build_finance.crypto_replay.canonical import (
    JsonValue,
    canonical_json_bytes,
    canonical_record_bytes,
    sha256_hex,
)
from build_finance.crypto_replay.content_ids import verify_content_id
from build_finance.crypto_replay.formats import parse_bounded_decimal_string
from build_finance.crypto_replay.schema_model import ValidationIssue
from build_finance.crypto_replay.schema_registry import validate_contract
from build_finance.crypto_replay.source_tree import SourceTreeError, reconstruct_source_tree

_MAX_U64 = 18_446_744_073_709_551_615

CodeFieldName = Literal[
    "admission_code_sha256",
    "normalization_code_sha256",
    "availability_grouping_code_sha256",
    "run_closure_code_sha256",
    "feature_code_sha256",
    "baseline_code_sha256",
    "risk_code_sha256",
    "fill_code_sha256",
    "accounting_code_sha256",
    "benchmark_code_sha256",
]
_EventOrderKey = tuple[Any, ...]
_EventRow = tuple[
    str,
    int,
    int,
    int,
    int,
    tuple[bytes, bytes, bytes],
    _EventOrderKey,
    _EventOrderKey,
]


@dataclass(frozen=True, slots=True)
class ContractCodePreimage:
    """One retained, explicitly contract-only code preimage."""

    field_name: CodeFieldName
    payload: bytes
    assurance: Literal["CONTRACT_ONLY"] = "CONTRACT_ONLY"


@dataclass(frozen=True, slots=True)
class RunInputBundle:
    """Closed in-memory input carrier; it is not runnable authority."""

    fixture_manifest: Mapping[str, JsonValue]
    config_admission_receipt: Mapping[str, JsonValue]
    replay_risk_config: Mapping[str, JsonValue] | None
    raw_config_bytes: bytes | None
    source_admission_receipts: tuple[Mapping[str, JsonValue], ...]
    normalized_events: tuple[Mapping[str, JsonValue], ...]
    run_closure_receipt: Mapping[str, JsonValue]
    availability_schedule: Mapping[str, JsonValue]
    counter_capacity: Mapping[str, JsonValue]
    source_tree: Mapping[str, JsonValue]
    model_registry: Mapping[str, JsonValue] | None
    model_signal_manifest: Mapping[str, JsonValue] | None
    public_seed_bytes: bytes
    code_preimages: tuple[ContractCodePreimage, ...]


@dataclass(frozen=True, slots=True)
class ContractVerifiedRunInputs:
    """Immutable contract-only snapshot; it grants no replay or execution authority."""

    authority: Literal["CONTRACT_ONLY"]
    run_receipt: Mapping[str, object]
    bundle: RunInputBundle
    public_seed_bytes: bytes


def _validation_issue(code: str, path: tuple[str | int, ...], message: str) -> ValidationIssue:
    return ValidationIssue(code=code, path=path, message=message)


def _freeze(value: object) -> object:
    if isinstance(value, Mapping):
        return MappingProxyType({str(key): _freeze(item) for key, item in value.items()})
    if isinstance(value, (list, tuple)):
        return tuple(_freeze(item) for item in value)
    if isinstance(value, bytes):
        return bytes(value)
    return value


def _snapshot_mapping(value: Mapping[str, JsonValue]) -> dict[str, JsonValue]:
    return deepcopy(dict(value))


def _snapshot_bundle(bundle: RunInputBundle) -> RunInputBundle:
    return RunInputBundle(
        fixture_manifest=_snapshot_mapping(bundle.fixture_manifest),
        config_admission_receipt=_snapshot_mapping(bundle.config_admission_receipt),
        replay_risk_config=(
            None if bundle.replay_risk_config is None else _snapshot_mapping(bundle.replay_risk_config)
        ),
        raw_config_bytes=None if bundle.raw_config_bytes is None else bytes(bundle.raw_config_bytes),
        source_admission_receipts=tuple(_snapshot_mapping(value) for value in bundle.source_admission_receipts),
        normalized_events=tuple(_snapshot_mapping(value) for value in bundle.normalized_events),
        run_closure_receipt=_snapshot_mapping(bundle.run_closure_receipt),
        availability_schedule=_snapshot_mapping(bundle.availability_schedule),
        counter_capacity=_snapshot_mapping(bundle.counter_capacity),
        source_tree=_snapshot_mapping(bundle.source_tree),
        model_registry=None if bundle.model_registry is None else _snapshot_mapping(bundle.model_registry),
        model_signal_manifest=(
            None if bundle.model_signal_manifest is None else _snapshot_mapping(bundle.model_signal_manifest)
        ),
        public_seed_bytes=bytes(bundle.public_seed_bytes),
        code_preimages=tuple(
            ContractCodePreimage(
                field_name=value.field_name,
                payload=bytes(value.payload),
                assurance=value.assurance,
            )
            for value in bundle.code_preimages
        ),
    )


def _freeze_bundle(bundle: RunInputBundle) -> RunInputBundle:
    def frozen_mapping(value: Mapping[str, JsonValue]) -> Mapping[str, JsonValue]:
        return cast(Mapping[str, JsonValue], _freeze(value))

    return RunInputBundle(
        fixture_manifest=frozen_mapping(bundle.fixture_manifest),
        config_admission_receipt=frozen_mapping(bundle.config_admission_receipt),
        replay_risk_config=(None if bundle.replay_risk_config is None else frozen_mapping(bundle.replay_risk_config)),
        raw_config_bytes=None if bundle.raw_config_bytes is None else bytes(bundle.raw_config_bytes),
        source_admission_receipts=tuple(frozen_mapping(value) for value in bundle.source_admission_receipts),
        normalized_events=tuple(frozen_mapping(value) for value in bundle.normalized_events),
        run_closure_receipt=frozen_mapping(bundle.run_closure_receipt),
        availability_schedule=frozen_mapping(bundle.availability_schedule),
        counter_capacity=frozen_mapping(bundle.counter_capacity),
        source_tree=frozen_mapping(bundle.source_tree),
        model_registry=None if bundle.model_registry is None else frozen_mapping(bundle.model_registry),
        model_signal_manifest=(
            None if bundle.model_signal_manifest is None else frozen_mapping(bundle.model_signal_manifest)
        ),
        public_seed_bytes=bytes(bundle.public_seed_bytes),
        code_preimages=tuple(
            ContractCodePreimage(
                field_name=value.field_name,
                payload=bytes(value.payload),
                assurance=value.assurance,
            )
            for value in bundle.code_preimages
        ),
    )


def _validate_self_document(
    document: Mapping[str, JsonValue],
    *,
    expected_schema: str,
    path: tuple[str | int, ...],
) -> tuple[ValidationIssue, ...]:
    issues: list[ValidationIssue] = []
    if document.get("schema") != expected_schema:
        return (
            _validation_issue(
                "run_input_self_id",
                (*path, "schema"),
                f"expected self-addressed {expected_schema}",
            ),
        )
    try:
        self_id_valid = verify_content_id(document)
    except (KeyError, TypeError, ValueError) as error:
        self_id_valid = False
        detail = str(error)
    else:
        detail = "self-ID does not match canonical body"
    if not self_id_valid:
        return (_validation_issue("run_input_self_id", path, detail),)
    try:
        local_issues = validate_contract(document, expected_schema=expected_schema)
    except (KeyError, TypeError, ValueError) as error:
        return (_validation_issue("run_input_contract", path, str(error)),)
    issues.extend(
        ValidationIssue(code=issue.code, path=(*path, *issue.path), message=issue.message) for issue in local_issues
    )
    return tuple(issues)


_COUNTER_ORDER = (
    ("decision_attempt_upper_bound", "DECISION_ATTEMPT"),
    ("decision_sequence_next_upper_bound", "DECISION_SEQUENCE"),
    ("producer_sequence_next_upper_bound", "PRODUCER_SEQUENCE"),
    ("intent_sequence_next_upper_bound", "INTENT_SEQUENCE"),
    ("fill_receipt_sequence_next_upper_bound", "FILL_RECEIPT_SEQUENCE"),
    ("unmatched_reservation_count_upper_bound", "UNMATCHED_RESERVATION_COUNT"),
    ("state_sequence_next_upper_bound", "STATE_SEQUENCE"),
    ("ledger_sequence_next_upper_bound", "LEDGER_SEQUENCE"),
)


def _require_unbounded_count(value: object, *, field: str) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or value < 0:
        raise ValueError(f"{field} must be a non-negative unbounded integer")
    return value


def _totalize_counter_capacity_bounds(unbounded: Mapping[str, int]) -> dict[str, JsonValue]:
    """Convert one complete unbounded counter map to aliases exactly once."""
    if set(unbounded) != {field for field, _ in _COUNTER_ORDER}:
        raise ValueError("counter bound map is not the exact closed field set")
    values = {field: _require_unbounded_count(unbounded[field], field=field) for field, _ in _COUNTER_ORDER}
    first_exceeded: str | None = None
    totalized: dict[str, JsonValue] = {}
    for field, code in _COUNTER_ORDER:
        value = values[field]
        if value <= _MAX_U64:
            totalized[field] = str(value)
        else:
            totalized[field] = None
            if first_exceeded is None:
                first_exceeded = code
    totalized["status"] = "WITHIN_LIMIT" if first_exceeded is None else "EXCEEDED"
    totalized["first_exceeded_counter"] = first_exceeded
    return totalized


def _derive_counter_capacity_from_counts(
    *,
    source_admission_count: int,
    raw_event_count: int,
    availability_group_count: int,
    market_count: int,
    model_candidate_count: int,
    model_signal_mode: str,
) -> dict[str, JsonValue]:
    """Plan-authorized count-only boundary probe using unbounded arithmetic."""
    source_count = _require_unbounded_count(source_admission_count, field="source_admission_count")
    event_count = _require_unbounded_count(raw_event_count, field="raw_event_count")
    group_count = _require_unbounded_count(availability_group_count, field="availability_group_count")
    markets = _require_unbounded_count(market_count, field="market_count")
    candidates = _require_unbounded_count(model_candidate_count, field="model_candidate_count")

    if not 1 <= markets <= _MAX_U64:
        return {"admissible": False, "rejection_code": "MARKET_COUNT_RANGE"}
    if source_count > _MAX_U64 or event_count > _MAX_U64 or not 1 <= group_count <= _MAX_U64:
        raise ValueError("source/event/group counts must fit their pre-closure aliases")
    if model_signal_mode == "DISABLED":
        if candidates != 0:
            return {"admissible": False, "rejection_code": "MODEL_CANDIDATE_COUNT_RANGE"}
    elif model_signal_mode == "CACHED_FIXTURES":
        if not 1 <= candidates <= _MAX_U64:
            return {"admissible": False, "rejection_code": "MODEL_CANDIDATE_COUNT_RANGE"}
    else:
        raise ValueError("model_signal_mode is not a closed value")

    decision_count = markets * group_count
    unbounded = {
        "decision_attempt_upper_bound": decision_count,
        "decision_sequence_next_upper_bound": decision_count + 1,
        "producer_sequence_next_upper_bound": candidates,
        "intent_sequence_next_upper_bound": decision_count + 1,
        "fill_receipt_sequence_next_upper_bound": decision_count + 1,
        "unmatched_reservation_count_upper_bound": 2 * decision_count,
        "state_sequence_next_upper_bound": 3 * decision_count + 2 * group_count + 3,
        "ledger_sequence_next_upper_bound": (
            source_count + event_count + 10 * decision_count + 2 * candidates + 5 * group_count + 9
        ),
    }
    return {
        "admissible": True,
        "rejection_code": None,
        "model_signal_mode": model_signal_mode,
        "source_admission_count": str(source_count),
        "raw_event_count": str(event_count),
        "availability_group_count": str(group_count),
        "market_count": str(markets),
        "model_candidate_count": str(candidates),
        **_totalize_counter_capacity_bounds(unbounded),
    }


def _selected_model_candidate_count(bundle: RunInputBundle, mode: str) -> tuple[str | None, str | None, int]:
    if mode == "DISABLED":
        if bundle.model_registry is not None or bundle.model_signal_manifest is not None:
            raise ValueError("disabled mode requires absent model bodies")
        return None, None, 0
    if mode != "CACHED_FIXTURES":
        raise ValueError("unknown model signal mode")
    registry = bundle.model_registry
    manifest = bundle.model_signal_manifest
    if registry is None or manifest is None:
        raise ValueError("cached mode requires registry and manifest bodies")
    registry_issues = _validate_self_document(
        registry,
        expected_schema="trading.model-registry/v1",
        path=("model_registry",),
    )
    manifest_issues = _validate_self_document(
        manifest,
        expected_schema="trading.model-signal-manifest/v1",
        path=("model_signal_manifest",),
    )
    if registry_issues or manifest_issues:
        raise ValueError((*registry_issues, *manifest_issues))
    registry_id = registry.get("model_registry_sha256")
    manifest_id = manifest.get("model_signal_manifest_sha256")
    if not isinstance(registry_id, str) or not isinstance(manifest_id, str):
        raise ValueError("cached model identities are absent")
    if manifest.get("model_registry_sha256") != registry_id:
        raise ValueError("cached model manifest does not bind the actual registry")
    candidates = manifest.get("candidates")
    if not isinstance(candidates, list):
        raise ValueError("cached model candidate array is absent")
    return registry_id, manifest_id, len(candidates)


def _u64(value: object, *, field: str, positive: bool = False) -> int:
    minimum = 1 if positive else 0
    try:
        return parse_bounded_decimal_string(value, minimum=minimum, maximum=_MAX_U64)
    except ValueError as error:
        raise ValueError(f"{field} is not a canonical bounded u64s value") from error


def _event_order_keys(
    event: Mapping[str, JsonValue],
    *,
    index: int,
) -> tuple[tuple[bytes, bytes, bytes], _EventOrderKey, _EventOrderKey, int]:
    position = event.get("source_position")
    revision = event.get("revision")
    if not isinstance(position, Mapping) or not isinstance(revision, Mapping):
        raise ValueError(f"normalized_events[{index}] position/revision is absent")
    position_slot = _u64(position.get("slot"), field=f"normalized_events[{index}].source_position.slot")
    source_subsequence = _u64(
        position.get("source_subsequence"),
        field=f"normalized_events[{index}].source_position.source_subsequence",
    )
    revision_slot = _u64(
        revision.get("availability_slot"),
        field=f"normalized_events[{index}].revision.availability_slot",
        positive=True,
    )
    kind = revision.get("kind")
    kind_rank = {"ORIGINAL": 0, "CORRECTION": 1, "RETRACTION": 2}.get(str(kind))
    if kind_rank is None:
        raise ValueError(f"normalized_events[{index}].revision.kind is not closed")
    if kind == "CORRECTION":
        target = revision.get("supersedes_event_id")
    elif kind == "RETRACTION":
        target = revision.get("retracts_event_id")
    else:
        target = ""
    if not isinstance(target, str):
        raise ValueError(f"normalized_events[{index}] revision target is malformed")
    numeric_positions: list[int] = []
    for field in ("transaction_index", "instruction_index", "event_index"):
        value = position.get(field)
        if isinstance(value, bool) or not isinstance(value, int) or value < 0:
            raise ValueError(f"normalized_events[{index}].source_position.{field} is malformed")
        numeric_positions.append(value)
    source_id = str(event.get("source_id"))
    source_kind = str(event.get("source_kind"))
    market_id = str(event.get("market_id"))
    source_native_event_id = str(position.get("source_native_event_id"))
    raw_payload_sha256 = str(event.get("raw_payload_sha256"))
    position_suffix: _EventOrderKey = (
        position_slot,
        *numeric_positions,
        source_native_event_id.encode("utf-8"),
        source_subsequence,
        revision_slot,
        kind_rank,
        target.encode("utf-8"),
        raw_payload_sha256.encode("ascii"),
    )
    source_group = (source_id.encode("utf-8"), source_kind.encode("utf-8"), market_id.encode("utf-8"))
    global_key: _EventOrderKey = (
        revision_slot,
        market_id.encode("utf-8"),
        source_id.encode("utf-8"),
        source_kind.encode("utf-8"),
        *position_suffix[:6],
        *position_suffix[7:],
    )
    return source_group, position_suffix, global_key, revision_slot


def derive_normalized_event_set(bundle: RunInputBundle) -> dict[str, JsonValue]:
    """Reconstruct a normalized-event-set only from a closed retained body graph."""
    preflight: list[ValidationIssue] = list(
        _validate_self_document(
            bundle.fixture_manifest,
            expected_schema="trading.fixture-manifest/v1",
            path=("fixture_manifest",),
        )
    )
    preflight.extend(
        _validate_self_document(
            bundle.config_admission_receipt,
            expected_schema="trading.config-admission-receipt/v1",
            path=("config_admission_receipt",),
        )
    )
    for index, receipt in enumerate(bundle.source_admission_receipts):
        preflight.extend(
            _validate_self_document(
                receipt,
                expected_schema="trading.source-admission-receipt/v1",
                path=("source_admission_receipts", index),
            )
        )
    for index, event in enumerate(bundle.normalized_events):
        preflight.extend(
            _validate_self_document(
                event,
                expected_schema="trading.raw-event/v1",
                path=("normalized_events", index),
            )
        )
    if preflight:
        raise ValueError(tuple(preflight))

    fixture_id = bundle.fixture_manifest.get("fixture_manifest_sha256")
    if not isinstance(fixture_id, str):
        raise ValueError("fixture manifest identity is absent")
    fixture_files_value = bundle.fixture_manifest.get("files")
    fixture_markets_value = bundle.fixture_manifest.get("allowed_markets")
    if not isinstance(fixture_files_value, list) or not isinstance(fixture_markets_value, list):
        raise ValueError("fixture file/market arrays are absent")
    fixture_files = {str(row["relative_path"]): row for row in fixture_files_value if isinstance(row, Mapping)}
    allowed_markets = {str(row["market_id"]): row for row in fixture_markets_value if isinstance(row, Mapping)}
    source_by_id: dict[str, Mapping[str, JsonValue]] = {}
    source_paths: set[str] = set()
    admission_sequences = [
        _u64(
            bundle.config_admission_receipt.get("admission_sequence"),
            field="config_admission_receipt.admission_sequence",
            positive=True,
        )
    ]
    for index, receipt in enumerate(bundle.source_admission_receipts):
        source_id = receipt.get("source_admission_receipt_id")
        relative_path = receipt.get("relative_path")
        if (
            not isinstance(source_id, str)
            or not isinstance(relative_path, str)
            or source_id in source_by_id
            or relative_path in source_paths
        ):
            raise ValueError(f"source_admission_receipts[{index}] identity is absent or duplicated")
        file_row = fixture_files.get(relative_path)
        if file_row is None or receipt.get("status") != "ADMITTED":
            raise ValueError(f"source_admission_receipts[{index}] is not an admitted declared fixture file")
        field_pairs = (
            ("fixture_manifest_sha256", "fixture_manifest_sha256"),
            ("raw_payload_sha256", "raw_payload_sha256"),
            ("byte_length", "byte_length"),
            ("admission_sequence", "admission_sequence"),
            ("availability_slot", "availability_slot"),
            ("media_type", "media_type"),
            ("source_id", "source_id"),
            ("source_kind", "source_kind"),
            ("source_revision", "source_revision"),
            ("market_id", "market_id"),
        )
        for receipt_field, fixture_field in field_pairs:
            expected = fixture_id if fixture_field == "fixture_manifest_sha256" else file_row.get(fixture_field)
            if receipt.get(receipt_field) != expected:
                raise ValueError(f"source_admission_receipts[{index}].{receipt_field} does not match fixture")
        source_by_id[source_id] = receipt
        source_paths.add(relative_path)
        admission_sequences.append(
            _u64(
                receipt.get("admission_sequence"),
                field=f"source_admission_receipts[{index}].admission_sequence",
                positive=True,
            )
        )
    if source_paths != set(fixture_files):
        raise ValueError("source-admission receipts must form an exact bijection with fixture files")
    if sorted(admission_sequences) != list(range(1, len(admission_sequences) + 1)):
        raise ValueError("config and source admission sequences must be unique and contiguous from one")

    event_rows: list[_EventRow] = []
    cited_source_ids: set[str] = set()
    event_ids: set[str] = set()
    for index, event in enumerate(bundle.normalized_events):
        ingest_sequence = _u64(event.get("ingest_sequence"), field=f"normalized_events[{index}].ingest_sequence")
        event_id = event.get("event_id")
        source_id = event.get("source_admission_receipt_id")
        if not isinstance(event_id, str) or event_id in event_ids:
            raise ValueError(f"normalized_events[{index}].event_id is absent or duplicated")
        if not isinstance(source_id, str) or source_id not in source_by_id:
            raise ValueError(f"normalized_events[{index}] does not cite an admitted source receipt")
        source = source_by_id[source_id]
        market_id = event.get("market_id")
        market = allowed_markets.get(str(market_id))
        revision = event.get("revision")
        if market is None or not isinstance(revision, Mapping):
            raise ValueError(f"normalized_events[{index}] does not bind a declared market/revision")
        exact_pairs = (
            (event.get("fixture_manifest_sha256"), fixture_id),
            (event.get("raw_payload_sha256"), source.get("raw_payload_sha256")),
            (event.get("source_id"), source.get("source_id")),
            (event.get("source_kind"), source.get("source_kind")),
            (event.get("source_revision"), source.get("source_revision")),
            (event.get("market_id"), source.get("market_id")),
            (event.get("admission_sequence"), source.get("admission_sequence")),
            (revision.get("availability_slot"), source.get("availability_slot")),
            (revision.get("availability_admission_sequence"), source.get("admission_sequence")),
            (event.get("base_mint"), market.get("base_mint")),
            (event.get("quote_mint"), market.get("quote_mint")),
            (event.get("base_decimals"), market.get("base_decimals")),
            (event.get("quote_decimals"), market.get("quote_decimals")),
        )
        if any(actual != expected for actual, expected in exact_pairs):
            raise ValueError(f"normalized_events[{index}] retained lineage does not match fixture/source bodies")
        event_ids.add(event_id)
        cited_source_ids.add(source_id)
        source_sequence = _u64(
            event.get("source_sequence"),
            field=f"normalized_events[{index}].source_sequence",
            positive=True,
        )
        equal_time_group = _u64(
            event.get("equal_time_group"),
            field=f"normalized_events[{index}].equal_time_group",
            positive=True,
        )
        replay_clock = _u64(
            event.get("replay_clock_ns"),
            field=f"normalized_events[{index}].replay_clock_ns",
        )
        source_group, source_key, global_key, _ = _event_order_keys(event, index=index)
        event_rows.append(
            (
                event_id,
                ingest_sequence,
                source_sequence,
                equal_time_group,
                replay_clock,
                source_group,
                source_key,
                global_key,
            )
        )
    if cited_source_ids != set(source_by_id):
        raise ValueError("normalized events must cover the exact admitted source-receipt set")
    global_sorted = sorted(event_rows, key=lambda row: row[7])
    if len({row[7] for row in global_sorted}) != len(global_sorted):
        raise ValueError("normalized events contain duplicate global ordering keys")
    for rank, row in enumerate(global_sorted, start=1):
        if row[1] != rank:
            raise ValueError("ingest_sequence must equal the exact frozen global-key rank")
    rows_by_source: dict[tuple[bytes, bytes, bytes], list[_EventRow]] = {}
    for row in event_rows:
        rows_by_source.setdefault(row[5], []).append(row)
    for rows in rows_by_source.values():
        source_sorted = sorted(rows, key=lambda row: row[6])
        if len({row[6] for row in source_sorted}) != len(source_sorted):
            raise ValueError("normalized events contain duplicate source ordering keys")
        for rank, row in enumerate(source_sorted, start=1):
            if row[2] != rank:
                raise ValueError("source_sequence must equal the exact frozen source-key rank")

    availability_slots = sorted({int(row[7][0]) for row in event_rows})
    group_by_slot = {slot: group for group, slot in enumerate(availability_slots, start=1)}
    replay_tick = _u64(
        bundle.fixture_manifest.get("replay_tick_ns"),
        field="fixture_manifest.replay_tick_ns",
        positive=True,
    )
    for row in event_rows:
        slot = int(row[7][0])
        expected_group = group_by_slot[slot]
        expected_clock = (expected_group - 1) * replay_tick
        if expected_clock > _MAX_U64:
            raise ValueError("normalized event replay clock overflows u64")
        if row[3] != expected_group or row[4] != expected_clock:
            raise ValueError("equal_time_group/replay_clock_ns do not match the frozen availability-slot rank")
    return {
        "schema": "trading.normalized-event-set/v1",
        "fixture_manifest_sha256": fixture_id,
        "raw_event_count": str(len(global_sorted)),
        "event_ids": [row[0] for row in global_sorted],
    }


def derive_availability_schedule(bundle: RunInputBundle) -> dict[str, JsonValue]:
    """Derive max-cutoff availability groups from retained admission bodies."""
    fixture_id = bundle.fixture_manifest.get("fixture_manifest_sha256")
    config_id = bundle.config_admission_receipt.get("config_admission_receipt_id")
    if not isinstance(fixture_id, str) or not isinstance(config_id, str):
        raise ValueError("fixture/config admission identity is absent")
    config_sequence = _u64(
        bundle.config_admission_receipt.get("admission_sequence"),
        field="config_admission_receipt.admission_sequence",
        positive=True,
    )
    source_rows: list[tuple[int, int, str]] = []
    for index, receipt in enumerate(bundle.source_admission_receipts):
        source_id = receipt.get("source_admission_receipt_id")
        if not isinstance(source_id, str):
            raise ValueError(f"source_admission_receipts[{index}].source_admission_receipt_id is absent")
        slot = _u64(
            receipt.get("availability_slot"),
            field=f"source_admission_receipts[{index}].availability_slot",
            positive=True,
        )
        sequence = _u64(
            receipt.get("admission_sequence"),
            field=f"source_admission_receipts[{index}].admission_sequence",
            positive=True,
        )
        source_rows.append((slot, sequence, source_id))
    maximum_sequence_by_slot: dict[int, int] = {}
    for slot, sequence, _ in source_rows:
        maximum_sequence_by_slot[slot] = max(sequence, maximum_sequence_by_slot.get(slot, 0))
    groups: list[JsonValue] = []
    cutoff = config_sequence
    for group_index, (slot, slot_maximum) in enumerate(sorted(maximum_sequence_by_slot.items()), start=1):
        cutoff = max(cutoff, slot_maximum)
        groups.append(
            {
                "availability_slot": str(slot),
                "equal_time_group": str(group_index),
                "admission_cutoff": str(cutoff),
            }
        )
    return {
        "schema": "trading.availability-schedule/v1",
        "fixture_manifest_sha256": fixture_id,
        "config_admission_receipt_id": config_id,
        "source_admission_receipt_ids": sorted(source_id for _, _, source_id in source_rows),
        "availability_groups": groups,
    }


def derive_counter_capacity(bundle: RunInputBundle) -> dict[str, JsonValue]:
    """Derive counter evidence from retained bodies; supplied count fields are ignored."""
    fixture_markets = bundle.fixture_manifest.get("allowed_markets")
    groups = derive_availability_schedule(bundle).get("availability_groups")
    if not isinstance(fixture_markets, list) or not isinstance(groups, list):
        raise ValueError("retained market/group arrays are absent")
    normalized_digest = sha256_hex(canonical_json_bytes(derive_normalized_event_set(bundle)))
    mode_value = bundle.run_closure_receipt.get("model_signal_mode")
    if not isinstance(mode_value, str):
        raise ValueError("run closure model mode is absent")
    registry_id, manifest_id, candidate_count = _selected_model_candidate_count(bundle, mode_value)
    totalized = _derive_counter_capacity_from_counts(
        source_admission_count=len(bundle.source_admission_receipts),
        raw_event_count=len(bundle.normalized_events),
        availability_group_count=len(groups),
        market_count=len(fixture_markets),
        model_candidate_count=candidate_count,
        model_signal_mode=mode_value,
    )
    if totalized["admissible"] is not True:
        raise ValueError(f"counter-capacity preflight rejected: {totalized['rejection_code']}")
    return {
        "schema": "trading.counter-capacity/v1",
        "model_signal_mode": mode_value,
        "model_registry_sha256": registry_id,
        "model_signal_manifest_sha256": manifest_id,
        "normalized_event_set_sha256": normalized_digest,
        "source_admission_count": totalized["source_admission_count"],
        "raw_event_count": totalized["raw_event_count"],
        "availability_group_count": totalized["availability_group_count"],
        "market_count": totalized["market_count"],
        "model_candidate_count": totalized["model_candidate_count"],
        **{field: totalized[field] for field, _ in _COUNTER_ORDER},
        "status": totalized["status"],
        "first_exceeded_counter": totalized["first_exceeded_counter"],
    }


def _issues_from_error(
    error: Exception,
    *,
    code: str,
    path: tuple[str | int, ...],
) -> tuple[ValidationIssue, ...]:
    if len(error.args) == 1 and isinstance(error.args[0], (list, tuple)):
        nested = tuple(error.args[0])
        if nested and all(isinstance(issue, ValidationIssue) for issue in nested):
            return cast(tuple[ValidationIssue, ...], nested)
    return (_validation_issue(code, path, str(error)),)


def _preflight_run_inputs(
    run_receipt: Mapping[str, JsonValue],
    bundle: RunInputBundle,
) -> tuple[ValidationIssue, ...]:
    documents: list[tuple[Mapping[str, JsonValue], str, tuple[str | int, ...]]] = [
        (run_receipt, "trading.run-receipt/v1", ("run_receipt",)),
        (bundle.fixture_manifest, "trading.fixture-manifest/v1", ("fixture_manifest",)),
        (
            bundle.config_admission_receipt,
            "trading.config-admission-receipt/v1",
            ("config_admission_receipt",),
        ),
        (bundle.run_closure_receipt, "trading.run-closure-receipt/v1", ("run_closure_receipt",)),
    ]
    if bundle.replay_risk_config is not None:
        documents.append((bundle.replay_risk_config, "trading.replay-risk-config/v1", ("replay_risk_config",)))
    documents.extend(
        (receipt, "trading.source-admission-receipt/v1", ("source_admission_receipts", index))
        for index, receipt in enumerate(bundle.source_admission_receipts)
    )
    documents.extend(
        (event, "trading.raw-event/v1", ("normalized_events", index))
        for index, event in enumerate(bundle.normalized_events)
    )
    if bundle.model_registry is not None:
        documents.append((bundle.model_registry, "trading.model-registry/v1", ("model_registry",)))
    if bundle.model_signal_manifest is not None:
        documents.append(
            (
                bundle.model_signal_manifest,
                "trading.model-signal-manifest/v1",
                ("model_signal_manifest",),
            )
        )
    issues: list[ValidationIssue] = []
    for document, expected_schema, path in documents:
        issues.extend(_validate_self_document(document, expected_schema=expected_schema, path=path))
    return tuple(issues)


def _config_code_seed_issues(
    run_receipt: Mapping[str, JsonValue],
    bundle: RunInputBundle,
) -> tuple[ValidationIssue, ...]:
    issues: list[ValidationIssue] = []
    config_receipt = bundle.config_admission_receipt
    closure = bundle.run_closure_receipt
    status = config_receipt["status"]
    raw_bytes = bundle.raw_config_bytes
    config = bundle.replay_risk_config
    validated_id: object = None
    if status == "MISSING":
        if config is not None or raw_bytes is not None:
            issues.append(
                _validation_issue("run_input_config_binding", ("raw_config_bytes",), "MISSING has no body or bytes")
            )
    elif status == "INVALID":
        if config is not None or type(raw_bytes) is not bytes or not raw_bytes:
            issues.append(
                _validation_issue(
                    "run_input_config_binding",
                    ("raw_config_bytes",),
                    "INVALID requires exact nonempty bytes and no parsed config",
                )
            )
    elif status == "VALID":
        if config is None or type(raw_bytes) is not bytes:
            issues.append(
                _validation_issue(
                    "run_input_config_binding",
                    ("raw_config_bytes",),
                    "VALID requires a parsed config and exact canonical record bytes",
                )
            )
        else:
            expected_record = canonical_record_bytes(dict(config))
            if raw_bytes != expected_record:
                issues.append(
                    _validation_issue(
                        "run_input_config_binding",
                        ("raw_config_bytes",),
                        "VALID raw bytes must equal canonical_record_bytes(config)",
                    )
                )
            validated_id = config["config_sha256"]
    else:
        issues.append(_validation_issue("run_input_config_binding", ("status",), "unknown config status"))

    expected_raw_digest = None if raw_bytes is None else sha256_hex(raw_bytes)
    expected_raw_length = "0" if raw_bytes is None else str(len(raw_bytes))
    config_expectations = {
        "raw_config_sha256": expected_raw_digest,
        "raw_byte_length": expected_raw_length,
        "validated_config_sha256": validated_id,
    }
    for field, expected in config_expectations.items():
        if config_receipt[field] != expected:
            issues.append(
                _validation_issue(
                    "run_input_config_binding",
                    ("config_admission_receipt", field),
                    f"{field} does not bind exact retained config input",
                )
            )
    if run_receipt["config_admission_receipt_id"] != config_receipt["config_admission_receipt_id"]:
        issues.append(
            _validation_issue(
                "run_input_config_binding",
                ("run_receipt", "config_admission_receipt_id"),
                "run receipt does not bind config admission receipt",
            )
        )
    for owner_name, owner in (("run_receipt", run_receipt), ("run_closure_receipt", closure)):
        if owner["validated_config_sha256"] != validated_id:
            issues.append(
                _validation_issue(
                    "run_input_config_binding",
                    (owner_name, "validated_config_sha256"),
                    "validated config identity does not match config status",
                )
            )
    if config_receipt["schema_bundle_sha256"] != run_receipt["schema_bundle_sha256"]:
        issues.append(
            _validation_issue(
                "run_input_config_binding",
                ("config_admission_receipt", "schema_bundle_sha256"),
                "config validator and run must bind one schema bundle",
            )
        )
    expected_closure_status = "PASS" if status == "VALID" else "NOT_REQUIRED_ZERO_AUTHORITY"
    if closure["status"] != expected_closure_status:
        issues.append(
            _validation_issue(
                "run_input_config_binding",
                ("run_closure_receipt", "status"),
                "closure status does not match config authority",
            )
        )

    expected_code_fields = {
        "admission_code_sha256",
        "normalization_code_sha256",
        "availability_grouping_code_sha256",
        "run_closure_code_sha256",
        "feature_code_sha256",
        "baseline_code_sha256",
        "risk_code_sha256",
        "fill_code_sha256",
        "accounting_code_sha256",
        "benchmark_code_sha256",
    }
    observed_fields: set[str] = set()
    observed_payloads: set[bytes] = set()
    observed_digests: set[str] = set()
    for index, preimage in enumerate(bundle.code_preimages):
        field = preimage.field_name
        payload = preimage.payload
        if field not in expected_code_fields or field in observed_fields:
            issues.append(
                _validation_issue(
                    "run_input_code_preimage", ("code_preimages", index, "field_name"), "field is extra or duplicated"
                )
            )
            continue
        observed_fields.add(field)
        if preimage.assurance != "CONTRACT_ONLY" or type(payload) is not bytes or not payload:
            issues.append(
                _validation_issue(
                    "run_input_code_preimage",
                    ("code_preimages", index),
                    "preimage must be nonempty exact bytes with CONTRACT_ONLY assurance",
                )
            )
            continue
        digest = sha256_hex(payload)
        if payload in observed_payloads or digest in observed_digests:
            issues.append(
                _validation_issue(
                    "run_input_code_preimage", ("code_preimages", index), "payload/digest alias is forbidden"
                )
            )
        observed_payloads.add(payload)
        observed_digests.add(digest)
        if run_receipt[field] != digest:
            issues.append(
                _validation_issue(
                    "run_input_code_preimage",
                    ("run_receipt", field),
                    "code digest does not match retained contract-only preimage",
                )
            )
    if observed_fields != expected_code_fields:
        issues.append(
            _validation_issue(
                "run_input_code_preimage",
                ("code_preimages",),
                "code preimages must contain the exact ten-field set",
            )
        )

    seed = bundle.public_seed_bytes
    if type(seed) is not bytes or len(seed) != 32:
        issues.append(
            _validation_issue("run_input_public_seed", ("public_seed_bytes",), "public seed must be exactly 32 bytes")
        )
    elif run_receipt["public_seed_hex"] != seed.hex() or run_receipt["public_seed_sha256"] != sha256_hex(seed):
        issues.append(
            _validation_issue(
                "run_input_public_seed",
                ("public_seed_bytes",),
                "public seed bytes do not match canonical hex and digest",
            )
        )
    forbidden_outputs = {
        "ledger_root_id",
        "output_sha256",
        "final_portfolio_state_id",
        "run_end_ledger_record_id",
        "benchmark_receipt_id",
    }
    if forbidden_outputs.intersection(run_receipt):
        issues.append(
            _validation_issue(
                "run_input_output_cycle", ("run_receipt",), "pre-output receipt contains cyclic output fields"
            )
        )
    return tuple(issues)


def _cross_binding_issues(
    run_receipt: Mapping[str, JsonValue],
    bundle: RunInputBundle,
) -> tuple[ValidationIssue, ...]:
    issues: list[ValidationIssue] = []
    fixture = bundle.fixture_manifest
    closure = bundle.run_closure_receipt
    config_receipt = bundle.config_admission_receipt

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
        if run_receipt[field] != closure[field]:
            issues.append(
                _validation_issue(
                    "run_input_cross_binding",
                    ("run_closure_receipt", field),
                    f"run and closure must bind the same {field}",
                )
            )
    if run_receipt["run_closure_receipt_id"] != closure["run_closure_receipt_id"]:
        issues.append(
            _validation_issue(
                "run_input_cross_binding",
                ("run_receipt", "run_closure_receipt_id"),
                "run does not bind the actual closure receipt",
            )
        )
    if closure["status"] not in ("PASS", "NOT_REQUIRED_ZERO_AUTHORITY"):
        issues.append(
            _validation_issue("run_input_cross_binding", ("run_closure_receipt", "status"), "failed closure cannot run")
        )
    if closure["counter_capacity_status"] != "WITHIN_LIMIT":
        issues.append(
            _validation_issue(
                "run_input_counter_binding",
                ("run_closure_receipt", "counter_capacity_status"),
                "counter capacity must be within limit",
            )
        )

    fixture_id = fixture["fixture_manifest_sha256"]
    if run_receipt["fixture_manifest_sha256"] != fixture_id:
        issues.append(
            _validation_issue(
                "run_input_fixture_binding",
                ("run_receipt", "fixture_manifest_sha256"),
                "run does not bind the actual fixture manifest",
            )
        )
    for field in ("replay_tick_ns", "run_end_position_policy"):
        if run_receipt[field] != fixture[field]:
            issues.append(
                _validation_issue(
                    "run_input_fixture_binding",
                    ("run_receipt", field),
                    f"run {field} does not match fixture",
                )
            )

    try:
        normalized_set = derive_normalized_event_set(bundle)
    except (KeyError, TypeError, ValueError) as error:
        issues.extend(
            _issues_from_error(
                error,
                code="run_input_event_binding",
                path=("normalized_events",),
            )
        )
        return tuple(issues)
    actual_source_ids = sorted(
        cast(str, receipt["source_admission_receipt_id"]) for receipt in bundle.source_admission_receipts
    )
    if run_receipt["source_admission_receipt_ids"] != actual_source_ids:
        issues.append(
            _validation_issue(
                "run_input_source_binding",
                ("run_receipt", "source_admission_receipt_ids"),
                "run must bind the exact admitted source set",
            )
        )

    try:
        schedule = derive_availability_schedule(bundle)
        retained_schedule_bytes = canonical_json_bytes(bundle.availability_schedule)
        schedule_bytes = canonical_json_bytes(schedule)
    except (KeyError, TypeError, ValueError) as error:
        issues.extend(
            _issues_from_error(
                error,
                code="run_input_schedule_binding",
                path=("availability_schedule",),
            )
        )
        return tuple(issues)
    schedule_digest = sha256_hex(schedule_bytes)
    if retained_schedule_bytes != schedule_bytes:
        issues.append(
            _validation_issue(
                "run_input_schedule_binding",
                ("availability_schedule",),
                "retained schedule bytes do not equal independent derivation",
            )
        )
    for owner_name, owner in (("run_receipt", run_receipt), ("run_closure_receipt", closure)):
        if owner["availability_schedule_sha256"] != schedule_digest:
            issues.append(
                _validation_issue(
                    "run_input_schedule_binding",
                    (owner_name, "availability_schedule_sha256"),
                    "schedule digest does not bind independent canonical bytes",
                )
            )
    if run_receipt["availability_groups"] != schedule["availability_groups"]:
        issues.append(
            _validation_issue(
                "run_input_schedule_binding",
                ("run_receipt", "availability_groups"),
                "run availability groups do not equal independent schedule",
            )
        )
    schedule_groups = cast(list[JsonValue], schedule["availability_groups"])
    group_by_slot = {
        cast(Mapping[str, JsonValue], row)["availability_slot"]: cast(Mapping[str, JsonValue], row)["equal_time_group"]
        for row in schedule_groups
    }
    for field in ("session_start_availability_slot", "session_end_availability_slot"):
        if fixture[field] not in group_by_slot:
            issues.append(
                _validation_issue(
                    "run_input_schedule_binding",
                    ("fixture_manifest", field),
                    "fixture boundary slot is absent from derived schedule",
                )
            )
    expected_terminal = group_by_slot.get(fixture["session_end_availability_slot"])
    if expected_terminal is not None and run_receipt["terminal_equal_time_group"] != expected_terminal:
        issues.append(
            _validation_issue(
                "run_input_schedule_binding",
                ("run_receipt", "terminal_equal_time_group"),
                "terminal group must equal the fixture session-end slot group",
            )
        )

    mode = cast(str, run_receipt["model_signal_mode"])
    registry = bundle.model_registry
    manifest = bundle.model_signal_manifest
    expected_scopes: list[dict[str, JsonValue]] = []
    if mode == "DISABLED":
        if registry is not None or manifest is not None:
            issues.append(
                _validation_issue(
                    "run_input_model_binding",
                    ("model_signal_mode",),
                    "disabled mode cannot retain model bodies",
                )
            )
    elif mode == "CACHED_FIXTURES" and registry is not None and manifest is not None:
        registry_id = registry["model_registry_sha256"]
        manifest_id = manifest["model_signal_manifest_sha256"]
        if manifest["model_registry_sha256"] != registry_id:
            issues.append(
                _validation_issue(
                    "run_input_model_binding",
                    ("model_signal_manifest", "model_registry_sha256"),
                    "manifest does not bind actual registry",
                )
            )
        for owner_name, owner in (("run_receipt", run_receipt), ("run_closure_receipt", closure)):
            if owner["model_registry_sha256"] != registry_id or owner["model_signal_manifest_sha256"] != manifest_id:
                issues.append(
                    _validation_issue(
                        "run_input_model_binding",
                        (owner_name, "model_registry_sha256"),
                        "model IDs do not bind actual retained bodies",
                    )
                )
        promotions_value = registry["promotions"]
        candidates_value = manifest["candidates"]
        assert isinstance(promotions_value, list) and isinstance(candidates_value, list)
        promotions: dict[str, Mapping[str, JsonValue]] = {}
        for row in promotions_value:
            assert isinstance(row, Mapping)
            promotion_id = cast(str, row["producer_scope_key_sha256"])
            if promotion_id in promotions:
                issues.append(
                    _validation_issue(
                        "run_input_model_binding",
                        ("model_registry", "promotions"),
                        "promotion scope digests must be unique",
                    )
                )
            promotions[promotion_id] = row
        markets_value = fixture["allowed_markets"]
        assert isinstance(markets_value, list)
        market_ids = [cast(str, cast(Mapping[str, JsonValue], row)["market_id"]) for row in markets_value]
        market_index = {market_id: index for index, market_id in enumerate(market_ids)}
        tick = _u64(run_receipt["replay_tick_ns"], field="run_receipt.replay_tick_ns", positive=True)
        budget = _u64(
            run_receipt["model_decision_budget_ns"],
            field="run_receipt.model_decision_budget_ns",
            positive=True,
        )
        start_group = _u64(
            group_by_slot[fixture["session_start_availability_slot"]],
            field="availability_schedule.start_group",
            positive=True,
        )
        terminal_group = _u64(
            run_receipt["terminal_equal_time_group"],
            field="run_receipt.terminal_equal_time_group",
            positive=True,
        )
        scope_by_key: dict[tuple[int, str], dict[str, JsonValue]] = {}
        for index, candidate_value in enumerate(candidates_value):
            assert isinstance(candidate_value, Mapping)
            candidate = candidate_value
            promotion_id = cast(str, candidate["requested_producer_scope_key_sha256"])
            promotion = promotions.get(promotion_id)
            if promotion is None:
                issues.append(
                    _validation_issue(
                        "run_input_model_binding",
                        ("model_signal_manifest", "candidates", index),
                        "candidate promotion is absent",
                    )
                )
                continue
            market_id = cast(str, promotion["market_id"])
            decision = _u64(
                candidate["decision_sequence"],
                field=f"model_signal_manifest.candidates[{index}].decision_sequence",
                positive=True,
            )
            market_ordinal = market_index.get(market_id)
            if market_ordinal is None or (decision - 1) % len(market_ids) != market_ordinal:
                issues.append(
                    _validation_issue(
                        "run_input_model_binding",
                        ("model_signal_manifest", "candidates", index),
                        "candidate has no uniquely schedulable selected request",
                    )
                )
                continue
            group = start_group + (decision - 1) // len(market_ids)
            expected_close = (group - 1) * tick + budget
            if (
                group > terminal_group
                or expected_close > _MAX_U64
                or candidate["decision_close_replay_clock_ns"] != str(expected_close)
            ):
                issues.append(
                    _validation_issue(
                        "run_input_model_binding",
                        ("model_signal_manifest", "candidates", index, "decision_close_replay_clock_ns"),
                        "candidate decision-close clock does not match the sealed schedule",
                    )
                )
            scope = {
                "market_id": market_id,
                "decision_sequence": str(decision),
                "horizon_ns": candidate["horizon_ns"],
                "producer_scope_key_sha256": promotion_id,
            }
            key = (decision, market_id)
            prior = scope_by_key.get(key)
            if prior is not None and prior != scope:
                issues.append(
                    _validation_issue(
                        "run_input_model_binding",
                        ("model_signal_manifest", "candidates", index),
                        "one selected request cannot bind conflicting scopes",
                    )
                )
            scope_by_key[key] = scope
        expected_scopes = [
            scope_by_key[key] for key in sorted(scope_by_key, key=lambda item: (item[0], item[1].encode("utf-8")))
        ]
    else:
        issues.append(
            _validation_issue(
                "run_input_model_binding",
                ("model_signal_mode",),
                "cached mode requires both retained model bodies",
            )
        )
    if run_receipt["selected_model_scopes"] != expected_scopes:
        issues.append(
            _validation_issue(
                "run_input_model_binding",
                ("run_receipt", "selected_model_scopes"),
                "selected scopes do not equal manifest-owned requests",
            )
        )

    try:
        counter = derive_counter_capacity(bundle)
        counter_bytes = canonical_json_bytes(counter)
        retained_counter_bytes = canonical_json_bytes(bundle.counter_capacity)
    except (KeyError, TypeError, ValueError) as error:
        issues.extend(
            _issues_from_error(
                error,
                code="run_input_counter_binding",
                path=("counter_capacity",),
            )
        )
        return tuple(issues)
    if retained_counter_bytes != counter_bytes:
        issues.append(
            _validation_issue(
                "run_input_counter_binding",
                ("counter_capacity",),
                "retained counter capacity does not equal body-derived capacity",
            )
        )
    if (
        closure["counter_capacity_sha256"] != sha256_hex(counter_bytes)
        or closure["counter_capacity_status"] != counter["status"]
    ):
        issues.append(
            _validation_issue(
                "run_input_counter_binding",
                ("run_closure_receipt", "counter_capacity_sha256"),
                "closure does not bind independently derived counter capacity",
            )
        )
    normalized_digest = sha256_hex(canonical_json_bytes(normalized_set))
    if counter["normalized_event_set_sha256"] != normalized_digest:
        issues.append(
            _validation_issue(
                "run_input_counter_binding",
                ("counter_capacity", "normalized_event_set_sha256"),
                "counter does not bind independently derived normalized-event set",
            )
        )

    try:
        source_tree = reconstruct_source_tree(bundle.source_tree)
        source_tree_bytes = canonical_json_bytes(source_tree)
        retained_source_tree_bytes = canonical_json_bytes(bundle.source_tree)
    except (SourceTreeError, TypeError, ValueError) as error:
        issues.append(
            _validation_issue(
                "run_input_source_tree", ("source_tree",), f"source-tree evidence is not canonical: {error}"
            )
        )
    else:
        if retained_source_tree_bytes != source_tree_bytes:
            issues.append(
                _validation_issue(
                    "run_input_source_tree",
                    ("source_tree",),
                    "retained source-tree bytes do not equal independent reconstruction",
                )
            )
        source_tree_digest = sha256_hex(source_tree_bytes)
        if run_receipt["source_tree_sha256"] != source_tree_digest:
            issues.append(
                _validation_issue(
                    "run_input_source_tree",
                    ("run_receipt", "source_tree_sha256"),
                    "run does not bind supplied canonical source-tree evidence",
                )
            )
    if closure["config_admission_receipt_id"] != config_receipt["config_admission_receipt_id"]:
        issues.append(
            _validation_issue(
                "run_input_config_binding",
                ("run_closure_receipt", "config_admission_receipt_id"),
                "closure does not bind actual config admission receipt",
            )
        )
    return tuple(issues)


def verify_contract_run_inputs(
    run_receipt: Mapping[str, JsonValue],
    bundle: RunInputBundle,
) -> ContractVerifiedRunInputs:
    """Fail closed until every retained input is verified as contract-only evidence."""
    if not isinstance(run_receipt, Mapping) or not isinstance(bundle, RunInputBundle):
        raise ValueError(
            (
                _validation_issue(
                    "run_input_type",
                    (),
                    "run receipt and RunInputBundle are required",
                ),
            )
        )
    try:
        run_snapshot = _snapshot_mapping(run_receipt)
        bundle_snapshot = _snapshot_bundle(bundle)
    except (AttributeError, TypeError, ValueError) as error:
        raise ValueError(
            (_validation_issue("run_input_snapshot", (), f"unable to snapshot retained inputs: {error}"),)
        ) from None
    issues = [*_preflight_run_inputs(run_snapshot, bundle_snapshot)]
    if not issues:
        issues.extend(_config_code_seed_issues(run_snapshot, bundle_snapshot))
    if not issues:
        issues.extend(_cross_binding_issues(run_snapshot, bundle_snapshot))
    if issues:
        raise ValueError(tuple(issues))
    frozen_bundle = _freeze_bundle(bundle_snapshot)
    frozen_run = cast(Mapping[str, object], _freeze(run_snapshot))
    return ContractVerifiedRunInputs(
        authority="CONTRACT_ONLY",
        run_receipt=frozen_run,
        bundle=frozen_bundle,
        public_seed_bytes=bytes(bundle_snapshot.public_seed_bytes),
    )
