"""RED contract for rooted, offline G2 normalization."""

from __future__ import annotations

import importlib
import inspect
import json
from dataclasses import FrozenInstanceError, fields, is_dataclass, replace
from typing import Any, get_type_hints

import pytest

from build_finance.crypto_replay.canonical import (
    canonical_json_bytes,
    canonical_record_bytes,
    parse_canonical_json,
    parse_canonical_record,
)
from build_finance.crypto_replay.run_inputs import ContractVerifiedRunInputs
from tests.live_paper.support.g2_vectors import (
    G2Vector,
    assert_g2_vector_self_checks,
    build_g2_vector,
    dropped_source_receipt_records,
    mutable_profile_kwargs,
    parse_live_record,
    reseal_replay_document,
    tamper_run_receipt_record,
    wrong_normalization_profile_record,
)


def _symbol(module: str, name: str) -> Any:
    return getattr(importlib.import_module(f"build_finance.live_paper.{module}"), name)


def _assert_signature(callable_: Any, names: tuple[str, ...]) -> None:
    parameters = tuple(inspect.signature(callable_).parameters.values())
    assert tuple(parameter.name for parameter in parameters) == names
    assert all(parameter.default is inspect.Parameter.empty for parameter in parameters)
    assert all(parameter.kind is inspect.Parameter.POSITIONAL_OR_KEYWORD for parameter in parameters)


def _profiles(vector: G2Vector) -> Any:
    return _symbol("profiles", "PaperKernelProfiles")(**vector.profile_records)


def _build(vector: G2Vector, *, run: bytes | None = None, resolver: Any = None) -> ContractVerifiedRunInputs:
    return _symbol("run_input_builder", "build_verified_run_inputs")(
        vector.run_receipt_record if run is None else run,
        vector.admitted_candidates,
        vector.source_receipt_records,
        vector.resolver if resolver is None else resolver,
        _profiles(vector),
    )


def _assert_no_float(value: object) -> None:
    if isinstance(value, dict):
        for item in value.values():
            _assert_no_float(item)
    elif isinstance(value, (list, tuple)):
        for item in value:
            _assert_no_float(item)
    else:
        assert not isinstance(value, float)


def test_synthetic_rooted_vector_is_valid_before_task3_imports() -> None:
    vector = build_g2_vector()

    assert_g2_vector_self_checks(vector)
    assert vector.run_receipt["model_signal_mode"] == "DISABLED"
    assert vector.run_receipt["model_registry_sha256"] is None
    assert vector.run_receipt["model_signal_manifest_sha256"] is None
    assert not vector.resolver.resolve_bytes(vector.availability_schedule_sha256).endswith(b"\n")
    for case in vector.normalization_cases:
        payload = parse_canonical_json(case.raw_payload)
        event = parse_canonical_record(case.raw_event_record)
        assert event["event_kind"] == payload["event_kind"]
        assert event["market"] == payload["market"]


def test_task3_public_interfaces_are_exact() -> None:
    evidence_resolver = _symbol("resolver", "EvidenceResolver")
    normalized_candidate = _symbol("normalization", "NormalizedCandidate")
    normalize = _symbol("normalization", "normalize_admitted_candidate")
    profiles = _symbol("profiles", "PaperKernelProfiles")
    builder = _symbol("run_input_builder", "build_verified_run_inputs")

    assert evidence_resolver._is_protocol is True
    _assert_signature(evidence_resolver.resolve_record, ("self", "content_id"))
    _assert_signature(evidence_resolver.resolve_bytes, ("self", "sha256"))
    assert get_type_hints(evidence_resolver.resolve_record) == {"content_id": str, "return": bytes}
    assert get_type_hints(evidence_resolver.resolve_bytes) == {"sha256": str, "return": bytes}
    assert is_dataclass(normalized_candidate)
    assert tuple(field.name for field in fields(normalized_candidate)) == (
        "raw_event_record",
        "normalization_receipt_record",
    )
    _assert_signature(normalize, ("candidate", "source_receipt_record", "resolver", "profile_record"))
    assert tuple(field.name for field in fields(profiles)) == (
        "normalization_profile_record",
        "feature_profile_record",
        "algorithm_profile_records",
        "model_validation_profile_record",
        "fusion_profile_record",
        "risk_config_record",
        "fill_profile_record",
    )
    _assert_signature(
        builder,
        ("run_receipt_record", "admitted_candidates", "source_receipt_records", "resolver", "profiles"),
    )
    assert get_type_hints(builder)["return"] is ContractVerifiedRunInputs


def test_complete_root_normalizes_once_and_is_copy_stable() -> None:
    vector = build_g2_vector()
    normalize = _symbol("normalization", "normalize_admitted_candidate")
    normalized = tuple(
        normalize(case.candidate, case.source_receipt_record, vector.resolver, vector.fixture_manifest_record)
        for case in vector.normalization_cases
    )

    assert tuple(result.raw_event_record for result in normalized) == tuple(
        case.raw_event_record for case in vector.normalization_cases
    )
    assert tuple(result.normalization_receipt_record for result in normalized) == tuple(
        case.normalization_receipt_record for case in vector.normalization_cases
    )
    for case, result in zip(vector.normalization_cases, normalized, strict=True):
        source = parse_canonical_record(case.source_receipt_record)
        event = parse_canonical_record(result.raw_event_record)
        receipt = parse_live_record(result.normalization_receipt_record)
        profile = parse_canonical_record(vector.fixture_manifest_record)
        assert source["status"] == "ADMITTED"
        assert source["parser_version"] == "solana-jupiter-fixture-parser/v1"
        assert source["raw_payload_sha256"] == case.candidate.raw_payload_sha256
        assert vector.resolver.resolve_bytes(case.candidate.raw_payload_sha256) == case.raw_payload
        assert event["source_admission_receipt_id"] == source["source_admission_receipt_id"]
        assert event["raw_payload_sha256"] == case.candidate.raw_payload_sha256
        assert event["fixture_manifest_sha256"] == profile["fixture_manifest_sha256"]
        assert event["source_position"] == case.candidate.source_position
        assert event["revision"] == case.candidate.revision
        assert (event["observed_at"], event["ingested_at"], event["event_time"]) == (
            case.candidate.observed_at,
            case.candidate.ingested_at,
            case.candidate.event_time,
        )
        assert receipt["input_content_ids"] == [source["source_admission_receipt_id"]]
        assert receipt["normalized_event_ids"] == [event["event_id"]]
        assert receipt["input_count"] == receipt["normalized_event_count"] == "1"
        assert receipt["normalization_code_sha256"] == vector.run_receipt["normalization_code_sha256"]
        _assert_no_float(event)
        _assert_no_float(receipt)

    first = build_g2_vector()
    second = build_g2_vector()
    verified = _build(first)
    copied = _build(second, run=bytes(second.run_receipt_record))
    assert type(verified) is ContractVerifiedRunInputs
    assert verified == copied
    assert tuple(
        canonical_json_bytes(json.loads(json.dumps(event, default=dict)))
        for event in verified.bundle.normalized_events
    ) == tuple(
        canonical_json_bytes(parse_canonical_record(case.raw_event_record))
        for case in first.normalization_cases
    )
    assert set(first.resolver.record_hits) == set(first.required_record_content_ids)
    assert set(first.resolver.byte_hits) == set(first.required_byte_sha256s)
    code_ids = {value for key, value in first.run_receipt.items() if key.endswith("_code_sha256")}
    assert len(code_ids) == 10
    assert code_ids <= set(first.resolver.byte_hits)
    assert not hasattr(verified, "decision_groups")


@pytest.mark.parametrize(
    "case",
    ("missing_lf", "wrong_schema", "tampered_id", "missing_record", "lf_attachment", "wrong_profile"),
)
def test_root_and_resolved_evidence_fail_closed(case: str) -> None:
    vector = build_g2_vector()
    run = vector.run_receipt_record
    resolver = vector.resolver
    expected: type[Exception] = ValueError
    if case == "missing_lf":
        run = run[:-1]
    elif case == "wrong_schema":
        run = tamper_run_receipt_record(vector, "schema", "trading.raw-event/v1")
    elif case == "tampered_id":
        run = tamper_run_receipt_record(vector, "public_seed_sha256", "0" * 64)
    elif case == "missing_record":
        resolver = resolver.without_record(vector.required_record_content_ids[0])
        expected = KeyError
    elif case == "lf_attachment":
        attachment = resolver.resolve_bytes(vector.availability_schedule_sha256)
        resolver = resolver.with_resolved_bytes(vector.availability_schedule_sha256, attachment + b"\n")
    else:
        wrong_profile = wrong_normalization_profile_record(vector)
        normalize = _symbol("normalization", "normalize_admitted_candidate")
        with pytest.raises(ValueError):
            normalize(
                vector.admitted_candidates[0],
                vector.source_receipt_records[0],
                vector.resolver,
                wrong_profile,
            )
        profile_kwargs = dict(vector.profile_records)
        profile_kwargs["normalization_profile_record"] = wrong_profile
        profiles = _symbol("profiles", "PaperKernelProfiles")(**profile_kwargs)
        with pytest.raises(ValueError):
            _symbol("run_input_builder", "build_verified_run_inputs")(
                vector.run_receipt_record,
                vector.admitted_candidates,
                vector.source_receipt_records,
                vector.resolver,
                profiles,
            )
        return

    with pytest.raises(expected):
        _build(vector, run=run, resolver=resolver)


@pytest.mark.parametrize("case", ("candidate_identity", "candidate_position", "source_set", "model_enabled"))
def test_candidate_source_set_and_model_disabled_authority_close(case: str) -> None:
    vector = build_g2_vector()
    builder = _symbol("run_input_builder", "build_verified_run_inputs")
    run = vector.run_receipt_record
    candidates = vector.admitted_candidates
    receipts = vector.source_receipt_records
    if case == "candidate_identity":
        candidates = (replace(candidates[0], source_revision="tampered-revision"), *candidates[1:])
    elif case == "candidate_position":
        assert candidates[0].source_position is not None
        changed_position = dict(candidates[0].source_position)
        changed_position["source_native_event_id"] = "tampered-native-event"
        candidates = (replace(candidates[0], source_position=changed_position), *candidates[1:])
        with pytest.raises(ValueError):
            _symbol("normalization", "normalize_admitted_candidate")(
                candidates[0],
                receipts[0],
                vector.resolver,
                vector.fixture_manifest_record,
            )
        return
    elif case == "source_set":
        candidates = candidates[:-1]
        receipts = dropped_source_receipt_records(vector)
    else:
        changed = vector.run_receipt
        changed["model_signal_mode"] = "CACHED_FIXTURES"
        run = canonical_record_bytes(reseal_replay_document(changed))

    with pytest.raises(ValueError) as failure:
        builder(run, candidates, receipts, vector.resolver, _profiles(vector))
    if case == "model_enabled":
        assert isinstance(failure.value.args[0], tuple)
        assert {getattr(issue, "code", None) for issue in failure.value.args[0]} == {"semantic_model_mode"}


def test_profiles_are_closed_frozen_byte_snapshots() -> None:
    vector = build_g2_vector()
    profile_type = _symbol("profiles", "PaperKernelProfiles")
    kwargs = mutable_profile_kwargs(vector)
    normalization_input = kwargs["normalization_profile_record"]
    assert isinstance(normalization_input, bytearray)
    expected = bytes(normalization_input)

    profiles = profile_type(**kwargs)
    normalization_input[0] ^= 1

    assert profiles.normalization_profile_record == expected
    assert type(profiles.normalization_profile_record) is bytes
    assert all(type(record) is bytes for record in profiles.algorithm_profile_records)
    assert not hasattr(profiles, "__dict__")
    with pytest.raises(FrozenInstanceError):
        profiles.normalization_profile_record = b"changed\n"
    with pytest.raises((AttributeError, FrozenInstanceError, TypeError)):
        profiles.extra_profile_record = b"forbidden\n"
