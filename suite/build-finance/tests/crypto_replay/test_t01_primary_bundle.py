"""T01 RED contract for the sealed primary bundle and resolving vectors."""

from __future__ import annotations

import hashlib
import importlib.resources
import json
from pathlib import Path
from typing import Any

import pytest

EXAMPLE_ROOT = Path(__file__).parent / "resources" / "primary-v1" / "examples"
EXPECTED_PRIMARY_BUNDLE_SHA256 = "3996eb230f078f5a34d295208974afe8131439ccd7bbb87da8dfa94631148ff8"


def _utf16_sort_key(value: str) -> bytes:
    return value.encode("utf-16-be", errors="strict")


def _independent_jcs_bytes(value: Any) -> bytes:
    """Serialize the bundle without importing the runtime canonicalizer."""
    if value is None:
        return b"null"
    if value is True:
        return b"true"
    if value is False:
        return b"false"
    if isinstance(value, int):
        return str(value).encode("ascii")
    if isinstance(value, str):
        return json.dumps(value, ensure_ascii=False, allow_nan=False).encode("utf-8")
    if isinstance(value, list):
        return b"[" + b",".join(_independent_jcs_bytes(item) for item in value) + b"]"
    if isinstance(value, dict):
        fields = (
            _independent_jcs_bytes(key) + b":" + _independent_jcs_bytes(value[key])
            for key in sorted(value, key=_utf16_sort_key)
        )
        return b"{" + b",".join(fields) + b"}"
    raise TypeError(f"unsupported independent JCS value: {type(value)!r}")


def _parse_independent(payload: bytes) -> Any:
    def object_without_duplicates(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
        result: dict[str, Any] = {}
        for key, value in pairs:
            if key in result:
                raise ValueError(f"duplicate JSON key: {key!r}")
            result[key] = value
        return result

    def reject_constant(value: str) -> None:
        raise ValueError(f"forbidden JSON constant: {value}")

    return json.loads(
        payload.decode("utf-8", errors="strict"),
        object_pairs_hook=object_without_duplicates,
        parse_constant=reject_constant,
    )


EXPECTED_ALIASES = [
    {
        "name": "CausalDigest",
        "json_type": "string",
        "pattern": "^[0-9a-f]{64}$",
        "minimum": None,
        "maximum": None,
        "scale": None,
    },
    {
        "name": "ContentID",
        "json_type": "string",
        "pattern": "^[0-9a-f]{64}$",
        "minimum": None,
        "maximum": None,
        "scale": None,
    },
    {
        "name": "i128s",
        "json_type": "string",
        "pattern": "^(0|-?[1-9][0-9]*)$",
        "minimum": "-170141183460469231731687303715884105728",
        "maximum": "170141183460469231731687303715884105727",
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
        "name": "u64s",
        "json_type": "string",
        "pattern": "^(0|[1-9][0-9]*)$",
        "minimum": "0",
        "maximum": "18446744073709551615",
        "scale": None,
    },
    {
        "name": "uints",
        "json_type": "string",
        "pattern": "^(0|[1-9][0-9]*)$",
        "minimum": "0",
        "maximum": None,
        "scale": None,
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


def test_primary_bundle_hash_is_locked() -> None:
    package = importlib.resources.files("build_finance.crypto_replay")
    bundle_record = package.joinpath("resources/primary-schema-bundle.json").read_bytes()
    digest_record = package.joinpath("resources/primary-schema-bundle.sha256").read_bytes()

    assert bundle_record.endswith(b"\n") and not bundle_record.endswith(b"\n\n")
    payload = bundle_record[:-1]
    bundle = _parse_independent(payload)
    independent_payload = _independent_jcs_bytes(bundle)
    assert independent_payload == payload
    assert hashlib.sha256(independent_payload).hexdigest() == EXPECTED_PRIMARY_BUNDLE_SHA256
    assert digest_record == EXPECTED_PRIMARY_BUNDLE_SHA256.encode("ascii") + b"\n"
    assert set(bundle) == {"schema", "jcs_profile", "protocol_constants", "aliases", "schemas"}
    assert bundle["schema"] == "build-finance.primary-schema-bundle/v1"
    assert bundle["jcs_profile"] == "RFC8785_INTEGER_AUTHORITY_V1"
    assert bundle["protocol_constants"] == {
        "MAX_RUN_CLOSURE_PROOF_ROWS_V0": "1000000",
        "MAX_U64_V0": "18446744073709551615",
    }
    assert bundle["aliases"] == EXPECTED_ALIASES
    assert len(bundle["schemas"]) == 8
    assert [row["contract_schema"] for row in bundle["schemas"]] == sorted(
        (row["contract_schema"] for row in bundle["schemas"]),
        key=lambda value: value.encode("utf-8"),
    )
    for row in bundle["schemas"]:
        assert set(row) == {"contract_schema", "json_schema_id", "family", "self_id_field", "schema_sha256"}
        assert row["family"] == "PRIMARY"
        assert isinstance(row["self_id_field"], str) and row["self_id_field"]
        contract_name, version = row["contract_schema"].removeprefix("trading.").split("/", maxsplit=1)
        assert row["json_schema_id"] == f"urn:build-finance:contract:{contract_name}:{version}"
        filename = f"{contract_name}-{version}.schema.json"
        schema_record = package.joinpath(f"resources/schemas/{filename}").read_bytes()
        assert schema_record.endswith(b"\n") and not schema_record.endswith(b"\n\n")
        schema_document = _parse_independent(schema_record[:-1])
        assert _independent_jcs_bytes(schema_document) + b"\n" == schema_record
        assert schema_document["$id"] == row["json_schema_id"]
        assert schema_document["x-contract-schema"] == row["contract_schema"]
        assert hashlib.sha256(schema_record[:-1]).hexdigest() == row["schema_sha256"]


def test_primary_vectors_contain_two_events_and_resolve_retained_byte_digests() -> None:
    from build_finance.crypto_replay.canonical import canonical_record_bytes, parse_canonical_record
    from build_finance.crypto_replay.content_ids import compute_content_id, verify_content_id
    from build_finance.crypto_replay.schema_definitions import PRIMARY_SCHEMA_IDS, PRIMARY_SELF_ID_FIELDS
    from build_finance.crypto_replay.schema_registry import require_valid_contract
    from tests.crypto_replay.support.primary_vectors import build_primary_vector_set

    vectors = build_primary_vector_set()
    events = [document for document in vectors.documents if document["schema"] == "trading.raw-event/v1"]

    assert len(events) >= 2
    assert len({event["revision"]["availability_slot"] for event in events}) >= 2
    assert {document["schema"] for document in vectors.documents} == set(PRIMARY_SCHEMA_IDS)

    for document in vectors.documents:
        schema_id = document["schema"]
        self_id = document[PRIMARY_SELF_ID_FIELDS[schema_id]]
        require_valid_contract(document, expected_schema=schema_id)
        assert verify_content_id(document)
        assert compute_content_id(document) == self_id
        resolved = vectors.resolver.resolve_object(self_id)
        assert resolved is not None
        assert resolved.assurance == "SCHEMA_VALID"
        assert resolved.document == document
        assert parse_canonical_record(resolved.record) == document
        assert resolved.record == canonical_record_bytes(document)

    for event in events:
        raw_payload = vectors.resolver.resolve_bytes(event["raw_payload_sha256"])
        assert raw_payload is not None
        assert hashlib.sha256(raw_payload).hexdigest() == event["raw_payload_sha256"]
        assert raw_payload.startswith(b"SYNTHETIC_T01_CONTRACT_VECTOR\n")

    for content_id in vectors.supporting_content_ids:
        resolved = vectors.resolver.resolve_object(content_id)
        assert resolved is not None
        assert resolved.assurance == "DIGEST_ONLY"
        assert parse_canonical_record(resolved.record) == resolved.document
        assert resolved.record == canonical_record_bytes(dict(resolved.document))
        assert verify_content_id(resolved.document)
        assert compute_content_id(resolved.document) == content_id

    referenced_supporting_ids: set[str] = set()
    referenced_primary_ids: set[str] = set()
    for document in vectors.documents:
        schema_id = document["schema"]
        if schema_id == "trading.raw-event/v1":
            referenced_supporting_ids.update(
                (document["fixture_manifest_sha256"], document["source_admission_receipt_id"])
            )
        elif schema_id == "trading.feature-snapshot/v1":
            if document["as_of_event_id"] is not None:
                referenced_primary_ids.add(document["as_of_event_id"])
        elif schema_id == "trading.model-signal/v1":
            referenced_primary_ids.add(document["feature_snapshot_id"])
        elif schema_id == "trading.risk-decision/v1":
            referenced_supporting_ids.add(document["config_admission_receipt_id"])
            if document["validated_config_sha256"] is not None:
                referenced_supporting_ids.add(document["validated_config_sha256"])
            referenced_primary_ids.update((document["feature_snapshot_id"], document["portfolio_state_before_id"]))
        elif schema_id == "trading.simulated-order-intent/v1":
            referenced_supporting_ids.update(
                (document["config_admission_receipt_id"], document["validated_config_sha256"])
            )
            referenced_primary_ids.update((document["risk_decision_id"], document["portfolio_state_before_id"]))
        elif schema_id == "trading.simulated-fill-receipt/v1":
            referenced_primary_ids.update(
                (document["intent_id"], document["portfolio_state_before_id"], document["fill_event_id"])
            )
        elif schema_id == "trading.portfolio-state/v1":
            referenced_supporting_ids.update(
                (
                    document["fixture_manifest_sha256"],
                    document["config_admission_receipt_id"],
                    document["validated_config_sha256"],
                )
            )
            if document["previous_portfolio_state_id"] is not None:
                referenced_primary_ids.add(document["previous_portfolio_state_id"])
            referenced_primary_ids.update(document["open_intent_ids"])
            resolved_cause = vectors.resolver.resolve_object(document["causation_id"])
            assert resolved_cause is not None
        elif schema_id == "trading.ledger-record/v1":
            referenced_supporting_ids.update(
                (
                    document["run_receipt_id"],
                    document["fixture_manifest_sha256"],
                    document["config_admission_receipt_id"],
                    document["validated_config_sha256"],
                )
            )
            referenced_primary_ids.update((document["object_id"], document["reconciliation"]["portfolio_state_id"]))
            if document["previous_ledger_record_id"] is not None:
                referenced_primary_ids.add(document["previous_ledger_record_id"])
            referenced_primary_ids.update(document["causation_ids"])

    assert referenced_supporting_ids == set(vectors.supporting_content_ids)
    for content_id in referenced_primary_ids:
        resolved = vectors.resolver.resolve_object(content_id)
        assert resolved is not None
        assert resolved.assurance == "SCHEMA_VALID"

    disabled = vectors.disabled_primary_vectors
    assert disabled.evidence_classification == "SYNTHETIC_CONTRACT_VECTOR"
    assert disabled.authority == "NONE"
    assert len([item for item in disabled.documents if item["schema"] == "trading.raw-event/v1"]) >= 2
    assert all(
        decision["model_signal_status"] == "ABSENT"
        and decision["effective_action"] == "HOLD"
        and decision["verdict"] != "APPROVE"
        for decision in disabled.documents
        if decision["schema"] == "trading.risk-decision/v1"
    )
    assert not any(
        item["schema"] in {"trading.simulated-order-intent/v1", "trading.simulated-fill-receipt/v1"}
        for item in disabled.documents
    )

    model = vectors.contract_only_model_vector
    assert model.evidence_classification == "SYNTHETIC_CONTRACT_VECTOR"
    assert model.authority == "CONTRACT_ONLY"
    assert len([item for item in model.documents if item["schema"] == "trading.model-signal/v1"]) == 1
    assert not any(
        item["schema"]
        in {
            "trading.risk-decision/v1",
            "trading.simulated-order-intent/v1",
            "trading.simulated-fill-receipt/v1",
        }
        for item in model.documents
    )

    execution = vectors.contract_only_execution_vector
    assert execution.evidence_classification == "SYNTHETIC_CONTRACT_VECTOR"
    assert execution.authority == "CONTRACT_ONLY"
    intents = [item for item in execution.documents if item["schema"] == "trading.simulated-order-intent/v1"]
    fills = [item for item in execution.documents if item["schema"] == "trading.simulated-fill-receipt/v1"]
    states = [item for item in execution.documents if item["schema"] == "trading.portfolio-state/v1"]
    ledgers = [item for item in execution.documents if item["schema"] == "trading.ledger-record/v1"]
    assert len(intents) == len(fills) == 1
    assert len(states) >= 3
    assert len(ledgers) >= 2
    assert fills[0]["intent_id"] == intents[0]["intent_id"]
    assert fills[0]["portfolio_state_before_id"] in {state["portfolio_state_id"] for state in states}
    assert {ledger["object_id"] for ledger in ledgers} >= {
        intents[0]["intent_id"],
        fills[0]["fill_receipt_id"],
    }

    claim_surface = json.dumps(
        [*vectors.documents, *(resolved.document for resolved in vectors.supporting_contents)],
        sort_keys=True,
    ).lower()
    for forbidden_claim in ("provider_origin", "observed_market", "profitability", "p2_admission"):
        assert forbidden_claim not in claim_surface


def test_in_memory_resolver_keeps_content_and_retained_byte_keys_separate() -> None:
    from tests.crypto_replay.support.builders import InMemoryResolver

    resolver = InMemoryResolver()
    payload = b"SYNTHETIC_T01_CONTRACT_VECTOR\nresolver-byte-key\n"
    digest = hashlib.sha256(payload).hexdigest()

    resolver.retain_bytes(digest, payload)
    assert resolver.resolve_bytes(digest) == payload
    assert resolver.resolve_object(digest) is None
    with pytest.raises(ValueError, match="different bytes"):
        resolver.retain_bytes(digest, payload + b"collision-probe")


def _raw_event_example() -> dict[str, Any]:
    return json.loads((EXAMPLE_ROOT / "raw-event.json").read_text(encoding="utf-8"))


def _resolver_fingerprint(resolver: Any) -> tuple[Any, ...]:
    return (
        resolver.content_ids,
        resolver.byte_sha256s,
        tuple(
            (resolved.record, resolved.assurance, json.dumps(resolved.document, sort_keys=True))
            for resolved in resolver.resolved_contents
        ),
    )


def _assert_retained_entry_unchanged(
    resolver: Any,
    content_id: str,
    record: bytes,
    assurance: str,
) -> None:
    from build_finance.crypto_replay.canonical import canonical_record_bytes, parse_canonical_record
    from build_finance.crypto_replay.content_ids import verify_content_id

    resolved = resolver.resolve_object(content_id)
    assert resolved is not None
    assert resolved.record == record
    assert resolved.assurance == assurance
    assert resolved.document == parse_canonical_record(record)
    assert canonical_record_bytes(dict(resolved.document)) == record
    assert verify_content_id(resolved.document)


def test_resolver_derives_assurance_and_all_failed_insertions_are_atomic() -> None:
    from build_finance.crypto_replay.canonical import canonical_record_bytes
    from build_finance.crypto_replay.content_ids import compute_content_id, seal_content_id
    from tests.crypto_replay.support.builders import (
        InMemoryResolver,
        build_digest_only_supporting_document,
    )

    resolver = InMemoryResolver()
    empty = _resolver_fingerprint(resolver)
    minimal_primary = seal_content_id({"schema": "trading.raw-event/v1"})
    minimal_record = canonical_record_bytes(minimal_primary)

    with pytest.raises(ValueError):
        resolver.retain_record(minimal_record, assurance="SCHEMA_VALID")
    assert _resolver_fingerprint(resolver) == empty

    invalid_id = dict(minimal_primary)
    invalid_id["event_id"] = "0" * 64
    with pytest.raises(ValueError, match="self-ID"):
        resolver.retain_record(canonical_record_bytes(invalid_id), assurance="SCHEMA_VALID")
    assert _resolver_fingerprint(resolver) == empty

    with pytest.raises(ValueError):
        resolver.retain_record(b"{}\n\n", assurance="SCHEMA_VALID")
    assert _resolver_fingerprint(resolver) == empty

    valid_record = canonical_record_bytes(_raw_event_example())
    with pytest.raises(ValueError, match="assurance"):
        resolver.retain_record(valid_record, assurance="FORGED")  # type: ignore[arg-type]
    assert _resolver_fingerprint(resolver) == empty

    with pytest.raises(ValueError, match="SHA-256"):
        resolver.retain_bytes("0" * 64, b"digest-mismatch")
    assert _resolver_fingerprint(resolver) == empty

    retained_payload = b"SYNTHETIC_T01_CONTRACT_VECTOR\natomic-collision\n"
    retained_sha256 = hashlib.sha256(retained_payload).hexdigest()
    resolver.retain_bytes(retained_sha256, retained_payload)
    before_collision = _resolver_fingerprint(resolver)
    with pytest.raises(ValueError, match="different bytes"):
        resolver.retain_bytes(retained_sha256, retained_payload + b"different")
    assert _resolver_fingerprint(resolver) == before_collision

    direct = resolver.retain_record(valid_record, assurance="SCHEMA_VALID")
    direct_id = compute_content_id(direct)
    direct_content = resolver.resolve_object(direct_id)
    assert direct_content is not None
    assert direct_content.assurance == "SCHEMA_VALID"
    before_idempotent = _resolver_fingerprint(resolver)
    resolver.retain_record(valid_record, assurance="SCHEMA_VALID")
    assert _resolver_fingerprint(resolver) == before_idempotent

    supporting_resolver = InMemoryResolver()
    supporting = build_digest_only_supporting_document(
        {
            "schema": "trading.fixture-manifest/v1",
            "t01_vector_classification": "SYNTHETIC_DIGEST_ONLY",
            "nested_probe": {"values": ["one"]},
        },
        supporting_resolver,
    )
    supporting_id = compute_content_id(supporting)
    supporting_content = supporting_resolver.resolve_object(supporting_id)
    assert supporting_content is not None
    assert supporting_content.assurance == "DIGEST_ONLY"

    escalation_resolver = InMemoryResolver()
    before_escalation = _resolver_fingerprint(escalation_resolver)
    with pytest.raises(ValueError, match="PRIMARY|primary"):
        escalation_resolver.retain_record(supporting_content.record, assurance="SCHEMA_VALID")
    assert _resolver_fingerprint(escalation_resolver) == before_escalation


def test_documents_returned_by_retain_and_build_do_not_alias_retained_evidence() -> None:
    from build_finance.crypto_replay.canonical import canonical_record_bytes
    from build_finance.crypto_replay.content_ids import compute_content_id
    from tests.crypto_replay.support.builders import InMemoryResolver, build_primary_document

    unsealed = _raw_event_example()
    unsealed.pop("event_id")
    build_resolver = InMemoryResolver()
    built = build_primary_document(unsealed, build_resolver)
    built_id = compute_content_id(built)
    built_resolved = build_resolver.resolve_object(built_id)
    assert built_resolved is not None
    built["market"]["base_amount_atoms"] = "0"
    _assert_retained_entry_unchanged(
        build_resolver,
        built_id,
        built_resolved.record,
        "SCHEMA_VALID",
    )

    retain_resolver = InMemoryResolver()
    retained = retain_resolver.retain_record(
        canonical_record_bytes(_raw_event_example()),
        assurance="SCHEMA_VALID",
    )
    retained_id = compute_content_id(retained)
    retained_resolved = retain_resolver.resolve_object(retained_id)
    assert retained_resolved is not None
    retained["quality_flags"].append("NON_EXECUTABLE")
    _assert_retained_entry_unchanged(
        retain_resolver,
        retained_id,
        retained_resolved.record,
        "SCHEMA_VALID",
    )


def test_resolver_views_return_mutation_isolated_documents() -> None:
    from build_finance.crypto_replay.content_ids import compute_content_id
    from tests.crypto_replay.support.primary_vectors import build_primary_vector_set

    vectors = build_primary_vector_set()
    event = next(document for document in vectors.documents if document["schema"] == "trading.raw-event/v1")
    event_id = compute_content_id(event)
    retained = vectors.resolver.resolve_object(event_id)
    assert retained is not None

    retained.document["market"]["route_capacity_base_atoms"] = "0"
    _assert_retained_entry_unchanged(vectors.resolver, event_id, retained.record, "SCHEMA_VALID")

    from_collection = next(
        resolved for resolved in vectors.resolver.resolved_contents if compute_content_id(resolved.document) == event_id
    )
    from_collection.document["quality_flags"].append("NON_EXECUTABLE")
    _assert_retained_entry_unchanged(vectors.resolver, event_id, retained.record, "SCHEMA_VALID")


def test_scenario_and_supporting_views_do_not_alias_retained_evidence() -> None:
    from build_finance.crypto_replay.content_ids import compute_content_id
    from tests.crypto_replay.support.primary_vectors import build_primary_vector_set

    vectors = build_primary_vector_set()
    scenario_event = next(
        document
        for document in vectors.disabled_primary_vectors.documents
        if document["schema"] == "trading.raw-event/v1"
    )
    event_id = compute_content_id(scenario_event)
    event_content = vectors.resolver.resolve_object(event_id)
    assert event_content is not None
    scenario_event["market"]["liquidity_quote_atoms"] = "0"
    _assert_retained_entry_unchanged(vectors.resolver, event_id, event_content.record, "SCHEMA_VALID")

    supporting = vectors.supporting_contents[0]
    supporting_id = compute_content_id(supporting.document)
    supporting.document["mutation_probe"] = {"nested": ["changed"]}
    _assert_retained_entry_unchanged(
        vectors.resolver,
        supporting_id,
        supporting.record,
        "DIGEST_ONLY",
    )


def test_primary_examples_cover_exactly_eight_schema_ids() -> None:
    from build_finance.crypto_replay.schema_definitions import PRIMARY_SCHEMA_IDS

    examples = [json.loads(path.read_text(encoding="utf-8")) for path in sorted(EXAMPLE_ROOT.glob("*.json"))]
    assert len(examples) == 8
    assert {document["schema"] for document in examples} == set(PRIMARY_SCHEMA_IDS)
