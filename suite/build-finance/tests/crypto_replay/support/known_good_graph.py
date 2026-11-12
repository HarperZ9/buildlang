"""Known-good T02 contract-only resolver graph for cross-artifact tests."""

from __future__ import annotations

import json
from collections.abc import Mapping
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Literal, cast

from build_finance.crypto_replay.canonical import (
    JsonValue,
    canonical_json_bytes,
    canonical_record_bytes,
    parse_canonical_record,
    sha256_hex,
)
from build_finance.crypto_replay.content_ids import compute_content_id
from build_finance.crypto_replay.run_inputs import ContractCodePreimage, RunInputBundle
from build_finance.crypto_replay.schema_definitions import (
    ALL_JSON_SCHEMA_IDS,
    ATTACHMENT_SCHEMA_IDS,
    PRIMARY_SCHEMA_IDS,
    SUPPORTING_SCHEMA_IDS,
    get_defined_schema_documents,
)
from tests.crypto_replay.support.builders import InMemoryResolver
from tests.crypto_replay.test_t02_supporting_contracts import build_t02_vector

EXPECTED_SCHEDULE = [
    {"availability_slot": "1", "equal_time_group": "1", "admission_cutoff": "2"},
    {"availability_slot": "2", "equal_time_group": "2", "admission_cutoff": "3"},
]
EXPECTED_PUBLIC_SEED_HEX = "000000000000000000000000000000000000000000000000000000000000000f"

_RESOURCE_ROOT = Path(__file__).resolve().parents[3] / "build_finance" / "crypto_replay" / "resources"
_PRIMARY_EXAMPLE_ROOT = Path(__file__).resolve().parents[1] / "resources" / "primary-v1" / "examples"


@dataclass(frozen=True, slots=True)
class KnownGoodGraph:
    """Complete local T02 authority graph and run-input bundle."""

    resolver: InMemoryResolver
    schema_lock: dict[str, JsonValue]
    schema_bundle: dict[str, JsonValue]
    authority_counts: dict[str, int]
    authority_content_ids: tuple[str, ...]
    run_receipt: Mapping[str, JsonValue]
    run_input_bundle: RunInputBundle
    public_seed_hex: str
    benchmark_authority: Literal["CONTRACT_ONLY_INELIGIBLE"]
    execution_quarantine_receipt_id: str
    ledger_member_content_ids: tuple[str, ...]


def build_known_good_graph() -> KnownGoodGraph:
    """Build the complete T02 local authority graph without replay authority."""
    resolver = InMemoryResolver()
    schema_lock = parse_canonical_record((_RESOURCE_ROOT / "schema-lock.json").read_bytes())
    schema_bundle = parse_canonical_record((_RESOURCE_ROOT / "schema-bundle.json").read_bytes())
    _retain_schema_authority_bytes(resolver, schema_bundle, schema_lock)

    vector = build_t02_vector()
    primary_authority_ids = _retain_primary_examples(resolver)
    supporting_authority_ids = _retain_supporting_authorities(resolver, vector.documents)
    _retain_run_input_support(resolver, vector)
    bundle = RunInputBundle(
        fixture_manifest=vector.documents["trading.fixture-manifest/v1"],
        config_admission_receipt=vector.documents["trading.config-admission-receipt/v1"],
        replay_risk_config=vector.documents["trading.replay-risk-config/v1"],
        raw_config_bytes=vector.raw_config_bytes,
        source_admission_receipts=vector.source_receipts,
        normalized_events=vector.raw_events,
        run_closure_receipt=vector.documents["trading.run-closure-receipt/v1"],
        availability_schedule=vector.attachments["trading.availability-schedule/v1"],
        counter_capacity=vector.attachments["trading.counter-capacity/v1"],
        source_tree=vector.attachments["trading.source-tree/v1"],
        model_registry=None,
        model_signal_manifest=None,
        public_seed_bytes=vector.public_seed_bytes,
        code_preimages=tuple(
            ContractCodePreimage(field_name=field_name, payload=payload, assurance="CONTRACT_ONLY")
            for field_name, payload in vector.code_preimages.items()
        ),
    )
    _retain_run_input_bytes(resolver, vector)
    run_receipt = vector.documents["trading.run-receipt/v1"]
    quarantine = vector.documents["trading.execution-quarantine-receipt/v1"]
    return KnownGoodGraph(
        resolver=resolver,
        schema_lock=schema_lock,
        schema_bundle=schema_bundle,
        authority_counts={
            "primary": int(schema_lock["primary_contract_count"]),
            "supporting": int(schema_lock["supporting_contract_count"]),
            "attachment": int(schema_lock["json_attachment_schema_count"]),
            "binary_formula": int(schema_lock["binary_formula_count"]),
            "generated_json_schema": int(schema_lock["generated_json_schema_count"]),
            "total_authority": int(schema_lock["total_authority_contract_count"]),
        },
        authority_content_ids=(*primary_authority_ids, *supporting_authority_ids),
        run_receipt=run_receipt,
        run_input_bundle=bundle,
        public_seed_hex=vector.public_seed_bytes.hex(),
        benchmark_authority="CONTRACT_ONLY_INELIGIBLE",
        execution_quarantine_receipt_id=cast(str, quarantine["execution_quarantine_receipt_id"]),
        ledger_member_content_ids=(compute_content_id(_primary_example("trading.ledger-record/v1")),),
    )


def _retain_schema_authority_bytes(
    resolver: InMemoryResolver,
    schema_bundle: Mapping[str, JsonValue],
    schema_lock: Mapping[str, JsonValue],
) -> None:
    schema_documents = get_defined_schema_documents()
    schema_rows = schema_bundle["schemas"]
    formula_rows = schema_bundle["binary_formulas"]
    if not isinstance(schema_rows, list) or not isinstance(formula_rows, list):
        raise ValueError("schema bundle rows are malformed")
    by_schema = {cast(str, row["contract_schema"]): row for row in schema_rows if isinstance(row, Mapping)}
    for schema_id in ALL_JSON_SCHEMA_IDS:
        row = by_schema[schema_id]
        document = schema_documents[schema_id]
        payload = canonical_json_bytes(document)
        digest = cast(str, row["schema_sha256"])
        if sha256_hex(payload) != digest:
            raise ValueError(f"schema authority digest mismatch for {schema_id}")
        resolver.retain_bytes(digest, payload)

    formula = cast(Mapping[str, JsonValue], formula_rows[0])
    formula_path = _RESOURCE_ROOT / cast(str, formula["formula_resource"])
    formula_payload = formula_path.read_bytes()
    resolver.retain_bytes(cast(str, formula["formula_sha256"]), formula_payload)

    schema_bundle_payload = canonical_json_bytes(dict(schema_bundle))
    resolver.retain_bytes(cast(str, schema_lock["schema_bundle_sha256"]), schema_bundle_payload)
    primary_bundle = parse_canonical_record((_RESOURCE_ROOT / "primary-schema-bundle.json").read_bytes())
    resolver.retain_bytes(cast(str, schema_lock["primary_bundle_sha256"]), canonical_json_bytes(primary_bundle))
    supporting_rows = [row for row in schema_rows if isinstance(row, Mapping) and row["family"] == "SUPPORTING"]
    attachment_rows = [row for row in schema_rows if isinstance(row, Mapping) and row["family"] == "ATTACHMENT"]
    resolver.retain_bytes(cast(str, schema_lock["supporting_bundle_sha256"]), canonical_json_bytes(supporting_rows))
    resolver.retain_bytes(cast(str, schema_lock["attachment_bundle_sha256"]), canonical_json_bytes(attachment_rows))
    resolver.retain_bytes(cast(str, schema_lock["binary_formula_bundle_sha256"]), canonical_json_bytes(formula_rows))


def _retain_primary_examples(resolver: InMemoryResolver) -> tuple[str, ...]:
    content_ids: list[str] = []
    for schema_id in PRIMARY_SCHEMA_IDS:
        document = _primary_example(schema_id)
        retained = resolver.retain_record(canonical_record_bytes(document))
        content_ids.append(compute_content_id(retained))
    return tuple(content_ids)


def _retain_supporting_authorities(
    resolver: InMemoryResolver,
    documents_by_schema: Mapping[str, Mapping[str, JsonValue]],
) -> tuple[str, ...]:
    content_ids: list[str] = []
    for schema_id in SUPPORTING_SCHEMA_IDS:
        document = dict(documents_by_schema[schema_id])
        retained = resolver.retain_supporting_record(canonical_record_bytes(document))
        content_ids.append(compute_content_id(retained))
    return tuple(content_ids)


def _retain_run_input_support(resolver: InMemoryResolver, vector: Any) -> None:
    for document in (*vector.source_receipts, *vector.raw_events):
        resolver.retain_self_addressed_authority_record(canonical_record_bytes(document))


def _retain_run_input_bytes(resolver: InMemoryResolver, vector: Any) -> None:
    for payload in (
        *vector.attachment_payloads.values(),
        *vector.raw_payloads,
        vector.raw_config_bytes,
        vector.public_seed_bytes,
        *vector.code_preimages.values(),
    ):
        resolver.retain_payload(payload)


def _primary_example(schema_id: str) -> dict[str, JsonValue]:
    filename = f"{schema_id.split('.', maxsplit=1)[1].split('/', maxsplit=1)[0]}.json"
    document = json.loads((_PRIMARY_EXAMPLE_ROOT / filename).read_text(encoding="utf-8"))
    if not isinstance(document, dict) or document.get("schema") != schema_id:
        raise ValueError(f"primary example is malformed for {schema_id}")
    return cast(dict[str, JsonValue], document)


def assert_authority_inventory_is_complete() -> None:
    """Guard imported constants against accidental drift."""
    if len(PRIMARY_SCHEMA_IDS) != 8 or len(SUPPORTING_SCHEMA_IDS) != 13 or len(ATTACHMENT_SCHEMA_IDS) != 27:
        raise AssertionError("T02 authority schema inventory drifted")
