"""Rooted construction of frozen contract-verified offline run inputs."""

from __future__ import annotations

from collections.abc import Mapping, Sequence
from typing import cast

from build_finance.crypto_replay.admission import ParsedSourceCandidate
from build_finance.crypto_replay.canonical import (
    JsonObject,
    JsonValue,
    parse_canonical_json,
    parse_canonical_record,
    sha256_hex,
)
from build_finance.crypto_replay.content_ids import compute_content_id, verify_content_id
from build_finance.crypto_replay.run_inputs import (
    CodeFieldName,
    ContractCodePreimage,
    ContractVerifiedRunInputs,
    RunInputBundle,
    verify_contract_run_inputs,
)
from build_finance.crypto_replay.schema_registry import validate_contract
from build_finance.live_paper.normalization import _NORMALIZATION_CODE_SHA256, normalize_admitted_candidate
from build_finance.live_paper.profiles import PaperKernelProfiles
from build_finance.live_paper.resolver import EvidenceResolver

_CODE_FIELDS: tuple[CodeFieldName, ...] = (
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
)


def _verified_record(
    record: bytes,
    expected_schema: str,
) -> JsonObject:
    document = parse_canonical_record(bytes(record))
    issues = validate_contract(document, expected_schema=expected_schema)
    if issues:
        raise ValueError(issues)
    if not verify_content_id(document):
        raise ValueError(f"{expected_schema} self-ID does not match its canonical body")
    return document


def _resolve_record(
    resolver: EvidenceResolver,
    content_id: str,
    expected_schema: str,
) -> tuple[bytes, JsonObject]:
    record = bytes(resolver.resolve_record(content_id))
    document = _verified_record(record, expected_schema)
    if compute_content_id(document) != content_id:
        raise ValueError(f"resolved {expected_schema} does not match its rooted ContentID")
    return record, document


def _resolve_bytes(resolver: EvidenceResolver, digest: str) -> bytes:
    payload = bytes(resolver.resolve_bytes(digest))
    if sha256_hex(payload) != digest:
        raise ValueError("resolved digest-addressed bytes do not match their rooted SHA-256")
    return payload


def _resolve_attachment(
    resolver: EvidenceResolver,
    digest: str,
    expected_schema: str,
) -> JsonObject:
    payload = _resolve_bytes(resolver, digest)
    document = parse_canonical_json(payload)
    issues = validate_contract(document, expected_schema=expected_schema)
    if issues:
        raise ValueError(issues)
    return document


def _source_receipts(
    declared_ids: Sequence[JsonValue],
    source_receipt_records: Sequence[bytes],
    resolver: EvidenceResolver,
) -> tuple[JsonObject, ...]:
    rooted_ids = tuple(cast(str, value) for value in declared_ids)
    supplied: list[tuple[bytes, JsonObject]] = []
    for record in source_receipt_records:
        exact_record = bytes(record)
        supplied.append(
            (exact_record, _verified_record(exact_record, "trading.source-admission-receipt/v1"))
        )
    supplied_ids = tuple(cast(str, document["source_admission_receipt_id"]) for _record, document in supplied)
    if len(set(supplied_ids)) != len(supplied_ids) or sorted(supplied_ids) != sorted(rooted_ids):
        raise ValueError("supplied source receipts do not equal the run receipt's exact declared set")
    for record, document in supplied:
        content_id = cast(str, document["source_admission_receipt_id"])
        resolved_record, _resolved_document = _resolve_record(
            resolver,
            content_id,
            "trading.source-admission-receipt/v1",
        )
        if resolved_record != record:
            raise ValueError("supplied source receipt bytes do not match rooted resolved evidence")
    return tuple(document for _record, document in supplied)


def _config_evidence(
    run: Mapping[str, JsonValue],
    config_receipt: Mapping[str, JsonValue],
    resolver: EvidenceResolver,
) -> tuple[JsonObject | None, bytes | None]:
    validated_id = run["validated_config_sha256"]
    if validated_id is None:
        config = None
    else:
        _record, config = _resolve_record(
            resolver,
            cast(str, validated_id),
            "trading.replay-risk-config/v1",
        )

    raw_digest = config_receipt["raw_config_sha256"]
    raw_config = None if raw_digest is None else _resolve_bytes(resolver, cast(str, raw_digest))
    return config, raw_config


def _code_preimages(
    run: Mapping[str, JsonValue],
    resolver: EvidenceResolver,
) -> tuple[ContractCodePreimage, ...]:
    return tuple(
        ContractCodePreimage(
            field_name=field,
            payload=_resolve_bytes(resolver, cast(str, run[field])),
            assurance="CONTRACT_ONLY",
        )
        for field in _CODE_FIELDS
    )


def build_verified_run_inputs(
    run_receipt_record: bytes,
    admitted_candidates: Sequence[ParsedSourceCandidate],
    source_receipt_records: Sequence[bytes],
    resolver: EvidenceResolver,
    profiles: PaperKernelProfiles,
) -> ContractVerifiedRunInputs:
    """Resolve only rooted descendants, normalize them, and invoke the frozen verifier."""
    run = _verified_record(bytes(run_receipt_record), "trading.run-receipt/v1")
    if not isinstance(profiles, PaperKernelProfiles):
        raise ValueError("build_verified_run_inputs requires closed PaperKernelProfiles")
    if run["normalization_code_sha256"] != _NORMALIZATION_CODE_SHA256:
        raise ValueError("run receipt does not bind the reviewed G2 normalization code identity")

    fixture_record, fixture = _resolve_record(
        resolver,
        cast(str, run["fixture_manifest_sha256"]),
        "trading.fixture-manifest/v1",
    )
    if fixture_record != profiles.normalization_profile_record:
        raise ValueError("normalization profile is not the exact fixture manifest rooted by the run receipt")

    _config_record, config_receipt = _resolve_record(
        resolver,
        cast(str, run["config_admission_receipt_id"]),
        "trading.config-admission-receipt/v1",
    )
    replay_risk_config, raw_config_bytes = _config_evidence(run, config_receipt, resolver)
    _closure_record, closure = _resolve_record(
        resolver,
        cast(str, run["run_closure_receipt_id"]),
        "trading.run-closure-receipt/v1",
    )

    candidates = tuple(admitted_candidates)
    receipt_records = tuple(bytes(record) for record in source_receipt_records)
    if len(candidates) != len(receipt_records):
        raise ValueError("each admitted candidate requires exactly one source receipt")
    source_receipts = _source_receipts(
        cast(Sequence[JsonValue], run["source_admission_receipt_ids"]),
        receipt_records,
        resolver,
    )

    availability_schedule = _resolve_attachment(
        resolver,
        cast(str, run["availability_schedule_sha256"]),
        "trading.availability-schedule/v1",
    )
    counter_capacity = _resolve_attachment(
        resolver,
        cast(str, closure["counter_capacity_sha256"]),
        "trading.counter-capacity/v1",
    )
    source_tree = _resolve_attachment(
        resolver,
        cast(str, run["source_tree_sha256"]),
        "trading.source-tree/v1",
    )

    if (
        run["model_signal_mode"] != "DISABLED"
        or run["model_registry_sha256"] is not None
        or run["model_signal_manifest_sha256"] is not None
    ):
        raise ValueError("G2 requires disabled model mode with no model bodies")

    public_seed = _resolve_bytes(resolver, cast(str, run["public_seed_sha256"]))
    code_preimages = _code_preimages(run, resolver)
    normalized = tuple(
        normalize_admitted_candidate(candidate, receipt_record, resolver, fixture_record)
        for candidate, receipt_record in zip(candidates, receipt_records, strict=True)
    )
    normalized_events = tuple(
        _verified_record(result.raw_event_record, "trading.raw-event/v1") for result in normalized
    )
    bundle = RunInputBundle(
        fixture_manifest=fixture,
        config_admission_receipt=config_receipt,
        replay_risk_config=replay_risk_config,
        raw_config_bytes=raw_config_bytes,
        source_admission_receipts=source_receipts,
        normalized_events=normalized_events,
        run_closure_receipt=closure,
        availability_schedule=availability_schedule,
        counter_capacity=counter_capacity,
        source_tree=source_tree,
        model_registry=None,
        model_signal_manifest=None,
        public_seed_bytes=public_seed,
        code_preimages=code_preimages,
    )
    return verify_contract_run_inputs(run, bundle)
