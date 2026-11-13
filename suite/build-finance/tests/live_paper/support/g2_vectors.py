"""Readable G2 RED vectors built from the frozen crypto-replay run-input graph.

The helpers in this file intentionally do not import Task 3 production modules.
They adapt the existing T02 replay vector into the smaller G2 normalization
contract surface: exact LF self-addressed records, exact digest-addressed bytes,
and explicitly synthetic in-memory candidates.
"""

from __future__ import annotations

import copy
import json
from collections.abc import Mapping
from dataclasses import dataclass, replace
from typing import Any

from build_finance.crypto_replay.admission import ParsedSourceCandidate
from build_finance.crypto_replay.canonical import (
    canonical_json_bytes as replay_canonical_json_bytes,
)
from build_finance.crypto_replay.canonical import (
    canonical_record_bytes as replay_canonical_record_bytes,
)
from build_finance.crypto_replay.canonical import (
    parse_canonical_record as parse_replay_record,
)
from build_finance.crypto_replay.canonical import sha256_hex as replay_sha256_hex
from build_finance.crypto_replay.content_ids import compute_content_id as replay_content_id
from build_finance.crypto_replay.content_ids import seal_content_id as replay_seal_content_id
from build_finance.crypto_replay.run_inputs import ContractCodePreimage, RunInputBundle
from build_finance.crypto_replay.schema_definitions import SELF_ID_FIELDS as REPLAY_SELF_ID_FIELDS
from build_finance.crypto_replay.schema_registry import require_valid_contract as require_valid_replay_contract
from build_finance.live_paper.content_ids import canonical_json_bytes as live_canonical_json_bytes
from build_finance.live_paper.content_ids import canonical_record_bytes as live_canonical_record_bytes
from build_finance.live_paper.content_ids import seal_content_id as live_seal_content_id
from build_finance.live_paper.content_ids import sha256_hex as live_sha256_hex
from build_finance.live_paper.registry import require_valid_contract as require_valid_live_contract
from tests.crypto_replay.test_t02_supporting_contracts import T02Vector, build_t02_vector

PROFILE_FIELDS = (
    "normalization_profile_record",
    "feature_profile_record",
    "algorithm_profile_records",
    "model_validation_profile_record",
    "fusion_profile_record",
    "risk_config_record",
    "fill_profile_record",
)


@dataclass(frozen=True, slots=True)
class G2NormalizationCase:
    """One admitted source candidate and its expected total normalization output."""

    candidate: ParsedSourceCandidate
    source_receipt_record: bytes
    raw_payload: bytes
    raw_event_record: bytes
    normalization_receipt_record: bytes


@dataclass(frozen=True, slots=True)
class G2Vector:
    """Complete valid rooted G2 graph, retained only in memory."""

    replay_vector: T02Vector
    resolver: G2EvidenceResolver
    run_receipt_record: bytes
    source_receipt_records: tuple[bytes, ...]
    admitted_candidates: tuple[ParsedSourceCandidate, ...]
    normalization_cases: tuple[G2NormalizationCase, ...]
    run_input_bundle: RunInputBundle
    required_record_content_ids: tuple[str, ...]
    required_byte_sha256s: tuple[str, ...]
    profile_records: Mapping[str, object]

    @property
    def run_receipt(self) -> dict[str, Any]:
        return parse_replay_record(self.run_receipt_record)

    @property
    def fixture_manifest_record(self) -> bytes:
        fixture_id = str(self.run_receipt["fixture_manifest_sha256"])
        return self.resolver.record_bytes(fixture_id)

    @property
    def availability_schedule_sha256(self) -> str:
        return str(self.run_receipt["availability_schedule_sha256"])


class G2EvidenceResolver:
    """Task-3-shaped resolver with separated record and byte address spaces."""

    __slots__ = ("_bytes_by_sha256", "_record_hits", "_records_by_content_id", "_byte_hits")

    def __init__(
        self,
        records_by_content_id: Mapping[str, bytes],
        bytes_by_sha256: Mapping[str, bytes],
    ) -> None:
        self._records_by_content_id = {str(key): bytes(value) for key, value in records_by_content_id.items()}
        self._bytes_by_sha256 = {str(key): bytes(value) for key, value in bytes_by_sha256.items()}
        self._record_hits: list[str] = []
        self._byte_hits: list[str] = []

    @property
    def record_hits(self) -> tuple[str, ...]:
        return tuple(self._record_hits)

    @property
    def byte_hits(self) -> tuple[str, ...]:
        return tuple(self._byte_hits)

    def resolve_record(self, content_id: str) -> bytes:
        self._record_hits.append(content_id)
        return self.record_bytes(content_id)

    def resolve_bytes(self, sha256: str) -> bytes:
        self._byte_hits.append(sha256)
        try:
            return bytes(self._bytes_by_sha256[sha256])
        except KeyError as error:
            raise KeyError(f"missing digest-addressed bytes: {sha256}") from error

    def record_bytes(self, content_id: str) -> bytes:
        try:
            return bytes(self._records_by_content_id[content_id])
        except KeyError as error:
            raise KeyError(f"missing self-addressed record: {content_id}") from error

    def without_record(self, content_id: str) -> G2EvidenceResolver:
        records = dict(self._records_by_content_id)
        records.pop(content_id)
        return G2EvidenceResolver(records, self._bytes_by_sha256)

    def with_resolved_bytes(self, sha256: str, payload: bytes) -> G2EvidenceResolver:
        payloads = dict(self._bytes_by_sha256)
        payloads[sha256] = bytes(payload)
        return G2EvidenceResolver(self._records_by_content_id, payloads)


def build_g2_vector() -> G2Vector:
    """Build one complete G2-rooted graph from the frozen T02 vector."""

    vector = _with_g2_payloads(build_t02_vector())
    records = _record_map(vector)
    payloads = _byte_map(vector)
    resolver = G2EvidenceResolver(records, payloads)
    run = vector.documents["trading.run-receipt/v1"]
    run_receipt_record = replay_canonical_record_bytes(run)
    source_receipt_records = tuple(replay_canonical_record_bytes(receipt) for receipt in vector.source_receipts)
    cases = tuple(
        _normalization_case(vector, receipt, event, payload)
        for receipt, event, payload in zip(vector.source_receipts, vector.raw_events, vector.raw_payloads, strict=True)
    )
    bundle = _run_input_bundle(vector)
    required_records = _required_record_content_ids(vector)
    required_bytes = _required_byte_sha256s(vector)
    return G2Vector(
        replay_vector=vector,
        resolver=resolver,
        run_receipt_record=run_receipt_record,
        source_receipt_records=source_receipt_records,
        admitted_candidates=tuple(case.candidate for case in cases),
        normalization_cases=cases,
        run_input_bundle=bundle,
        required_record_content_ids=required_records,
        required_byte_sha256s=required_bytes,
        profile_records=_profile_records(records[str(run["fixture_manifest_sha256"])]),
    )


def _with_g2_payloads(vector: T02Vector) -> T02Vector:
    """Replace opaque T02 payloads and reseal only their dependent rooted graph."""

    payloads = _g2_payloads(vector)
    fixture = copy.deepcopy(vector.documents["trading.fixture-manifest/v1"])
    for row, payload in zip(fixture["files"], payloads, strict=True):
        row["raw_payload_sha256"] = replay_sha256_hex(payload)
        row["byte_length"] = str(len(payload))
    fixture = reseal_replay_document(fixture)

    source_receipts = []
    for receipt, payload in zip(vector.source_receipts, payloads, strict=True):
        changed = copy.deepcopy(receipt)
        changed["fixture_manifest_sha256"] = fixture["fixture_manifest_sha256"]
        changed["raw_payload_sha256"] = replay_sha256_hex(payload)
        changed["byte_length"] = str(len(payload))
        source_receipts.append(reseal_replay_document(changed))

    raw_events = []
    for event, receipt, payload in zip(vector.raw_events, source_receipts, payloads, strict=True):
        changed = copy.deepcopy(event)
        changed["fixture_manifest_sha256"] = fixture["fixture_manifest_sha256"]
        changed["raw_payload_sha256"] = replay_sha256_hex(payload)
        changed["source_admission_receipt_id"] = receipt["source_admission_receipt_id"]
        raw_events.append(reseal_replay_document(changed))
    source_ids = sorted(str(receipt["source_admission_receipt_id"]) for receipt in source_receipts)

    availability = copy.deepcopy(vector.attachments["trading.availability-schedule/v1"])
    availability["fixture_manifest_sha256"] = fixture["fixture_manifest_sha256"]
    availability["source_admission_receipt_ids"] = source_ids
    availability_payload = replay_canonical_json_bytes(availability)

    normalized_set = copy.deepcopy(vector.attachments["trading.normalized-event-set/v1"])
    normalized_set["fixture_manifest_sha256"] = fixture["fixture_manifest_sha256"]
    normalized_set["event_ids"] = [event["event_id"] for event in raw_events]
    normalized_payload = replay_canonical_json_bytes(normalized_set)

    counter = copy.deepcopy(vector.attachments["trading.counter-capacity/v1"])
    counter["normalized_event_set_sha256"] = replay_sha256_hex(normalized_payload)
    counter_payload = replay_canonical_json_bytes(counter)

    closure = copy.deepcopy(vector.documents["trading.run-closure-receipt/v1"])
    closure["fixture_manifest_sha256"] = fixture["fixture_manifest_sha256"]
    closure["source_admission_receipt_ids"] = source_ids
    closure["availability_schedule_sha256"] = replay_sha256_hex(availability_payload)
    closure["counter_capacity_sha256"] = replay_sha256_hex(counter_payload)
    closure = reseal_replay_document(closure)

    run = copy.deepcopy(vector.documents["trading.run-receipt/v1"])
    run["fixture_manifest_sha256"] = fixture["fixture_manifest_sha256"]
    run["source_admission_receipt_ids"] = source_ids
    run["availability_schedule_sha256"] = replay_sha256_hex(availability_payload)
    run["run_closure_receipt_id"] = closure["run_closure_receipt_id"]
    run = reseal_replay_document(run)

    documents = copy.deepcopy(vector.documents)
    documents["trading.fixture-manifest/v1"] = fixture
    documents["trading.run-closure-receipt/v1"] = closure
    documents["trading.run-receipt/v1"] = run
    attachments = copy.deepcopy(vector.attachments)
    attachments["trading.availability-schedule/v1"] = availability
    attachments["trading.normalized-event-set/v1"] = normalized_set
    attachments["trading.counter-capacity/v1"] = counter
    attachment_payloads = dict(vector.attachment_payloads)
    attachment_payloads["trading.availability-schedule/v1"] = availability_payload
    attachment_payloads["trading.normalized-event-set/v1"] = normalized_payload
    attachment_payloads["trading.counter-capacity/v1"] = counter_payload
    return replace(
        vector,
        documents=documents,
        attachments=attachments,
        attachment_payloads=attachment_payloads,
        raw_payloads=payloads,
        source_receipts=(source_receipts[0], source_receipts[1]),
        raw_events=(raw_events[0], raw_events[1]),
    )


def _g2_payloads(vector: T02Vector) -> tuple[bytes, bytes]:
    return tuple(
        replay_canonical_json_bytes(
            {
                "schema": "build-finance.live-paper.synthetic-normalization-input/v1",
                "event_kind": event["event_kind"],
                "market": copy.deepcopy(event["market"]),
                "source_position": copy.deepcopy(event["source_position"]),
                "revision": copy.deepcopy(event["revision"]),
                "event_time": event["event_time"],
                "executable": event["executable"],
                "quality_flags": copy.deepcopy(event["quality_flags"]),
                "source_sequence": event["source_sequence"],
                "ingest_sequence": event["ingest_sequence"],
                "equal_time_group": event["equal_time_group"],
                "replay_clock_ns": event["replay_clock_ns"],
            }
        )
        for event in vector.raw_events
    )


def parse_live_record(record: bytes) -> dict[str, Any]:
    """Parse one live-paper LF canonical record while rejecting JSON floats."""

    if not record.endswith(b"\n") or record.endswith(b"\n\n"):
        raise ValueError("live-paper authority records must be exactly one LF-terminated JSON object")
    document = json.loads(record[:-1].decode("utf-8"), parse_float=_reject_json_float)
    if not isinstance(document, dict):
        raise ValueError("live-paper authority record must parse to a JSON object")
    if live_canonical_record_bytes(document) != record:
        raise ValueError("live-paper authority record is not canonical")
    return document


def reseal_replay_document(document: Mapping[str, Any]) -> dict[str, Any]:
    """Return a replay self-addressed document after dropping its old self-ID."""

    body = copy.deepcopy(dict(document))
    body.pop(REPLAY_SELF_ID_FIELDS[str(body["schema"])], None)
    return replay_seal_content_id(body)


def tamper_run_receipt_record(vector: G2Vector, field: str, value: object) -> bytes:
    """Return a canonical LF run receipt whose body no longer matches its self-ID."""

    run = vector.run_receipt
    run[field] = value
    return replay_canonical_record_bytes(run)


def wrong_normalization_profile_record(vector: G2Vector) -> bytes:
    """Return a schema-valid fixture profile that does not match the rooted fixture."""

    profile = parse_replay_record(vector.fixture_manifest_record)
    profile["initial_quote_atoms"] = str(int(str(profile["initial_quote_atoms"])) + 1)
    return replay_canonical_record_bytes(reseal_replay_document(profile))


def dropped_source_receipt_records(vector: G2Vector) -> tuple[bytes, ...]:
    """Return a candidate/source set with one admitted receipt missing."""

    return vector.source_receipt_records[:-1]


def mutable_profile_kwargs(vector: G2Vector) -> dict[str, object]:
    """Return mutable byte-like inputs for PaperKernelProfiles snapshot tests."""

    return {
        "normalization_profile_record": bytearray(vector.profile_records["normalization_profile_record"]),
        "feature_profile_record": bytearray(vector.profile_records["feature_profile_record"]),
        "algorithm_profile_records": (
            bytearray(vector.profile_records["algorithm_profile_records"][0]),  # type: ignore[index]
        ),
        "model_validation_profile_record": bytearray(vector.profile_records["model_validation_profile_record"]),
        "fusion_profile_record": bytearray(vector.profile_records["fusion_profile_record"]),
        "risk_config_record": bytearray(vector.profile_records["risk_config_record"]),
        "fill_profile_record": bytearray(vector.profile_records["fill_profile_record"]),
    }


def assert_g2_vector_self_checks(vector: G2Vector) -> None:
    """Assert support vectors are valid without importing Task 3 production code."""

    from build_finance.crypto_replay.run_inputs import verify_contract_run_inputs

    assert verify_contract_run_inputs(vector.run_receipt, vector.run_input_bundle).authority == "CONTRACT_ONLY"
    wrong_profile = parse_replay_record(wrong_normalization_profile_record(vector))
    require_valid_replay_contract(wrong_profile, expected_schema="trading.fixture-manifest/v1")
    assert wrong_profile["fixture_manifest_sha256"] != vector.run_receipt["fixture_manifest_sha256"]
    assert len(vector.normalization_cases) == len(vector.admitted_candidates) == len(vector.source_receipt_records) == 2
    for content_id in vector.required_record_content_ids:
        record = vector.resolver.resolve_record(content_id)
        parsed = parse_replay_record(record)
        assert replay_content_id(parsed) == content_id
        assert record.endswith(b"\n")
    for sha256 in vector.required_byte_sha256s:
        payload = vector.resolver.resolve_bytes(sha256)
        assert replay_sha256_hex(payload) == sha256
    for case in vector.normalization_cases:
        raw_event = parse_replay_record(case.raw_event_record)
        normalization = parse_live_record(case.normalization_receipt_record)
        require_valid_replay_contract(raw_event, expected_schema="trading.raw-event/v1")
        require_valid_live_contract(normalization, expected_schema="trading.normalization-receipt/v1")
        assert raw_event["raw_payload_sha256"] == replay_sha256_hex(case.raw_payload)
        assert normalization["normalized_event_ids"] == [raw_event["event_id"]]


def _reject_json_float(value: str) -> None:
    raise ValueError(f"floating-point authority is forbidden: {value}")


def _record_map(vector: T02Vector) -> dict[str, bytes]:
    records: dict[str, bytes] = {}
    for document in (*vector.documents.values(), *vector.source_receipts, *vector.raw_events):
        content_id = replay_content_id(document)
        records[content_id] = replay_canonical_record_bytes(document)
    return records


def _byte_map(vector: T02Vector) -> dict[str, bytes]:
    payloads = (
        *vector.attachment_payloads.values(),
        *vector.raw_payloads,
        vector.raw_config_bytes,
        vector.public_seed_bytes,
        *vector.code_preimages.values(),
    )
    return {replay_sha256_hex(payload): bytes(payload) for payload in payloads}


def _run_input_bundle(vector: T02Vector) -> RunInputBundle:
    return RunInputBundle(
        fixture_manifest=copy.deepcopy(vector.documents["trading.fixture-manifest/v1"]),
        config_admission_receipt=copy.deepcopy(vector.documents["trading.config-admission-receipt/v1"]),
        replay_risk_config=copy.deepcopy(vector.documents["trading.replay-risk-config/v1"]),
        raw_config_bytes=bytes(vector.raw_config_bytes),
        source_admission_receipts=tuple(copy.deepcopy(receipt) for receipt in vector.source_receipts),
        normalized_events=tuple(copy.deepcopy(event) for event in vector.raw_events),
        run_closure_receipt=copy.deepcopy(vector.documents["trading.run-closure-receipt/v1"]),
        availability_schedule=copy.deepcopy(vector.attachments["trading.availability-schedule/v1"]),
        counter_capacity=copy.deepcopy(vector.attachments["trading.counter-capacity/v1"]),
        source_tree=copy.deepcopy(vector.attachments["trading.source-tree/v1"]),
        model_registry=None,
        model_signal_manifest=None,
        public_seed_bytes=bytes(vector.public_seed_bytes),
        code_preimages=tuple(
            ContractCodePreimage(field_name=name, payload=bytes(payload), assurance="CONTRACT_ONLY")
            for name, payload in vector.code_preimages.items()
        ),
    )


def _normalization_case(
    vector: T02Vector,
    source_receipt: Mapping[str, Any],
    raw_event: Mapping[str, Any],
    raw_payload: bytes,
) -> G2NormalizationCase:
    candidate = ParsedSourceCandidate(
        admission_sequence=str(source_receipt["admission_sequence"]),
        relative_path=str(source_receipt["relative_path"]),
        raw_payload_sha256=str(source_receipt["raw_payload_sha256"]),
        source_id=str(source_receipt["source_id"]),
        source_kind=str(source_receipt["source_kind"]),
        source_revision=str(source_receipt["source_revision"]),
        market_id=str(source_receipt["market_id"]),
        source_position=copy.deepcopy(raw_event["source_position"]),
        revision=copy.deepcopy(raw_event["revision"]),
        event_time=str(raw_event["event_time"]),
        observed_at=str(source_receipt["observed_at"]),
        ingested_at=str(source_receipt["ingested_at"]),
    )
    return G2NormalizationCase(
        candidate=candidate,
        source_receipt_record=replay_canonical_record_bytes(source_receipt),
        raw_payload=bytes(raw_payload),
        raw_event_record=replay_canonical_record_bytes(raw_event),
        normalization_receipt_record=_normalization_receipt_record(vector, source_receipt, raw_event),
    )


def _normalization_receipt_record(
    vector: T02Vector,
    source_receipt: Mapping[str, Any],
    raw_event: Mapping[str, Any],
) -> bytes:
    event_id = str(raw_event["event_id"])
    output_root_payload = {
        "schema": "build-finance.live-paper.single-normalized-output-root/v1",
        "event_ids": [event_id],
    }
    document = live_seal_content_id(
        {
            "schema": "trading.normalization-receipt/v1",
            "source_batch_id": f"g2-normalization-source-{source_receipt['admission_sequence']}",
            "normalization_code_sha256": replay_sha256_hex(vector.code_preimages["normalization_code_sha256"]),
            "input_content_ids": [str(source_receipt["source_admission_receipt_id"])],
            "input_count": "1",
            "normalized_event_ids": [event_id],
            "normalized_event_count": "1",
            "output_merkle_root_sha256": live_sha256_hex(live_canonical_json_bytes(output_root_payload)),
            "status": "PASS",
            "reason_codes": [],
            "total_evidence": "TOTAL_INPUT_CLOSURE",
        }
    )
    return live_canonical_record_bytes(document)


def _required_record_content_ids(vector: T02Vector) -> tuple[str, ...]:
    run = vector.documents["trading.run-receipt/v1"]
    return (
        str(run["fixture_manifest_sha256"]),
        str(run["config_admission_receipt_id"]),
        str(run["validated_config_sha256"]),
        str(run["run_closure_receipt_id"]),
        *(str(receipt["source_admission_receipt_id"]) for receipt in vector.source_receipts),
    )


def _required_byte_sha256s(vector: T02Vector) -> tuple[str, ...]:
    run = vector.documents["trading.run-receipt/v1"]
    closure = vector.documents["trading.run-closure-receipt/v1"]
    source_payloads = tuple(str(receipt["raw_payload_sha256"]) for receipt in vector.source_receipts)
    return (
        str(vector.documents["trading.config-admission-receipt/v1"]["raw_config_sha256"]),
        str(run["availability_schedule_sha256"]),
        str(closure["counter_capacity_sha256"]),
        str(run["source_tree_sha256"]),
        str(run["public_seed_sha256"]),
        *source_payloads,
        *(replay_sha256_hex(payload) for payload in vector.code_preimages.values()),
    )


def _profile_records(fixture_manifest_record: bytes) -> Mapping[str, object]:
    return {
        "normalization_profile_record": bytes(fixture_manifest_record),
        "feature_profile_record": b"SYNTHETIC G2 PROFILE RECORD: feature\n",
        "algorithm_profile_records": (b"SYNTHETIC G2 PROFILE RECORD: algorithm-0\n",),
        "model_validation_profile_record": b"SYNTHETIC G2 PROFILE RECORD: model-validation\n",
        "fusion_profile_record": b"SYNTHETIC G2 PROFILE RECORD: fusion\n",
        "risk_config_record": b"SYNTHETIC G2 PROFILE RECORD: risk\n",
        "fill_profile_record": b"SYNTHETIC G2 PROFILE RECORD: fill\n",
    }
