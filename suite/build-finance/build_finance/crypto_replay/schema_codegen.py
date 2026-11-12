"""Deterministic generation and checking for local replay schema resources."""

from __future__ import annotations

import argparse
from collections.abc import Sequence
from dataclasses import dataclass
from pathlib import Path
from typing import Literal

from build_finance.crypto_replay.canonical import (
    JsonObject,
    canonical_json_bytes,
    canonical_record_bytes,
    parse_canonical_record,
    sha256_hex,
)
from build_finance.crypto_replay.schema_definitions import (
    ALL_JSON_SCHEMA_IDS,
    ATTACHMENT_SCHEMA_IDS,
    CONTRACT_SPECS_BY_SCHEMA,
    PRIMARY_SCHEMA_IDS,
    SUPPORTING_SCHEMA_IDS,
    get_defined_schema_documents,
    json_schema_id,
)
from build_finance.crypto_replay.schema_registry import SchemaRegistry

ScopeName = Literal["primary", "supporting", "full"]
_RESOURCE_ROOT = Path(__file__).parent / "resources"
_FORMULA_PATH = "formulas/adverse-fill-draw-v1.txt"
_FORMULA_RECORD = (
    b'SHA256(UTF8("trading.adverse-fill-draw/v1") || 0x00 || seed_bytes[32] || '
    b"decode_hex(draw_key_sha256)[32] || uint64_be(0))\n"
)


@dataclass(frozen=True, slots=True)
class ScopePlan:
    """Frozen ownership boundaries for one code-generation scope."""

    name: ScopeName
    checked_schema_ids: tuple[str, ...]
    writable_schema_ids: tuple[str, ...]
    preserved_schema_ids: tuple[str, ...]
    auxiliary_paths: tuple[str, ...]


_SCOPE_PLANS: dict[ScopeName, ScopePlan] = {
    "primary": ScopePlan(
        name="primary",
        checked_schema_ids=PRIMARY_SCHEMA_IDS,
        writable_schema_ids=PRIMARY_SCHEMA_IDS,
        preserved_schema_ids=(),
        auxiliary_paths=("primary-schema-bundle.json", "primary-schema-bundle.sha256"),
    ),
    "supporting": ScopePlan(
        name="supporting",
        checked_schema_ids=SUPPORTING_SCHEMA_IDS,
        writable_schema_ids=SUPPORTING_SCHEMA_IDS,
        preserved_schema_ids=(),
        auxiliary_paths=(),
    ),
    "full": ScopePlan(
        name="full",
        checked_schema_ids=ALL_JSON_SCHEMA_IDS,
        writable_schema_ids=(*SUPPORTING_SCHEMA_IDS, *ATTACHMENT_SCHEMA_IDS),
        preserved_schema_ids=PRIMARY_SCHEMA_IDS,
        auxiliary_paths=(
            _FORMULA_PATH,
            "schema-bundle.json",
            "schema-bundle.sha256",
            "schema-lock.json",
        ),
    ),
}

_JCS_PROFILE = "RFC8785_INTEGER_AUTHORITY_V1"
_PROTOCOL_CONSTANTS: JsonObject = {
    "MAX_RUN_CLOSURE_PROOF_ROWS_V0": "1000000",
    "MAX_U64_V0": "18446744073709551615",
}
_ALIASES: list[JsonObject] = [
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


def get_scope_plan(scope: str) -> ScopePlan:
    """Return exact checked/write/preserve ownership for a frozen scope."""
    try:
        return _SCOPE_PLANS[scope]  # type: ignore[index]
    except KeyError as error:
        raise ValueError(f"unknown schema code-generation scope: {scope!r}") from error


def _utf8_key(value: str) -> bytes:
    return value.encode("utf-8", errors="strict")


def _schema_path(schema_id: str) -> str:
    return f"schemas/{CONTRACT_SPECS_BY_SCHEMA[schema_id].schema_filename}"


_KNOWN_ROOT_JSON_RESOURCES = frozenset(
    {
        "primary-schema-bundle.json",
        "schema-bundle.json",
        "schema-lock.json",
    }
)
_KNOWN_ROOT_DIGEST_RESOURCES = frozenset(
    {
        "primary-schema-bundle.sha256",
        "schema-bundle.sha256",
    }
)
_KNOWN_SCHEMA_RESOURCES = frozenset(_schema_path(schema_id) for schema_id in ALL_JSON_SCHEMA_IDS)
_KNOWN_FORMULA_RESOURCES = frozenset({_FORMULA_PATH})


def _schema_row(schema_id: str, document: JsonObject) -> JsonObject:
    spec = CONTRACT_SPECS_BY_SCHEMA[schema_id]
    return {
        "contract_schema": schema_id,
        "json_schema_id": json_schema_id(schema_id),
        "family": spec.family,
        "self_id_field": spec.self_id_field,
        "schema_sha256": sha256_hex(canonical_json_bytes(document)),
    }


def _sorted_rows(schema_ids: Sequence[str], documents: dict[str, JsonObject]) -> list[JsonObject]:
    return [_schema_row(schema_id, documents[schema_id]) for schema_id in sorted(schema_ids, key=_utf8_key)]


def _primary_bundle(documents: dict[str, JsonObject]) -> JsonObject:
    return {
        "schema": "build-finance.primary-schema-bundle/v1",
        "jcs_profile": _JCS_PROFILE,
        "protocol_constants": _PROTOCOL_CONSTANTS,
        "aliases": _ALIASES,
        "schemas": _sorted_rows(PRIMARY_SCHEMA_IDS, documents),
    }


def _family_digest(rows: list[JsonObject]) -> str:
    return sha256_hex(canonical_json_bytes(rows))


def _require_definitions(
    scope: ScopeName,
    documents: dict[str, JsonObject],
) -> tuple[str, ...]:
    if scope == "supporting":
        defined = tuple(schema_id for schema_id in SUPPORTING_SCHEMA_IDS if schema_id in documents)
        if not defined:
            raise ValueError("supporting scope has no supporting schema definitions to generate")
        return defined
    required = PRIMARY_SCHEMA_IDS if scope == "primary" else ALL_JSON_SCHEMA_IDS
    missing = tuple(schema_id for schema_id in required if schema_id not in documents)
    if missing:
        raise ValueError(f"{scope} scope is missing schema definitions: {missing!r}")
    return required


def _validate_definitions(documents: dict[str, JsonObject]) -> None:
    specs = (CONTRACT_SPECS_BY_SCHEMA[schema_id] for schema_id in documents)
    SchemaRegistry.from_documents(documents.values(), specs=specs)


def _read_sealed_primary(
    resources_root: Path,
    documents: dict[str, JsonObject],
) -> tuple[JsonObject, str]:
    bundle_path = resources_root / "primary-schema-bundle.json"
    digest_path = resources_root / "primary-schema-bundle.sha256"
    if not bundle_path.is_file() or not digest_path.is_file():
        raise ValueError("full scope requires the sealed T01 primary bundle and digest")
    record = bundle_path.read_bytes()
    digest_record = digest_path.read_bytes()
    if len(digest_record) != 65 or not digest_record.endswith(b"\n"):
        raise ValueError("primary-schema-bundle.sha256 is not one lowercase digest plus LF")
    digest = digest_record[:-1].decode("ascii", errors="strict")
    if not re_full_sha256(digest) or sha256_hex(record[:-1]) != digest:
        raise ValueError("sealed primary bundle digest does not match its canonical payload")
    sealed = parse_canonical_record(record)
    expected = _primary_bundle(documents)
    if canonical_json_bytes(sealed) != canonical_json_bytes(expected):
        raise ValueError("sealed primary bundle differs from current primary schema authority")
    return sealed, digest


def re_full_sha256(value: str) -> bool:
    """Return whether a digest record body is exact lowercase SHA-256."""
    return len(value) == 64 and all(character in "0123456789abcdef" for character in value)


def _full_metadata_resources(
    resources_root: Path,
    documents: dict[str, JsonObject],
) -> dict[str, bytes]:
    primary_bundle, primary_digest = _read_sealed_primary(resources_root, documents)
    rows = _sorted_rows(ALL_JSON_SCHEMA_IDS, documents)
    primary_rows = [row for row in rows if row["family"] == "PRIMARY"]
    if canonical_json_bytes(primary_rows) != canonical_json_bytes(primary_bundle["schemas"]):
        raise ValueError("full scope primary rows are not byte-identical to the sealed primary bundle")

    supporting_rows = [row for row in rows if row["family"] == "SUPPORTING"]
    attachment_rows = [row for row in rows if row["family"] == "ATTACHMENT"]
    formula_row: JsonObject = {
        "contract_schema": "trading.adverse-fill-draw/v1",
        "family": "ATTACHMENT_BINARY_FORMULA",
        "formula_resource": "formulas/adverse-fill-draw-v1.txt",
        "formula_sha256": sha256_hex(_FORMULA_RECORD),
    }
    formula_rows = [formula_row]
    bundle: JsonObject = {
        "schema": "build-finance.schema-bundle/v1",
        "jcs_profile": _JCS_PROFILE,
        "protocol_constants": _PROTOCOL_CONSTANTS,
        "primary_bundle_sha256": primary_digest,
        "aliases": _ALIASES,
        "schemas": rows,
        "binary_formulas": formula_rows,
    }
    bundle_payload = canonical_json_bytes(bundle)
    bundle_digest = sha256_hex(bundle_payload)
    lock: JsonObject = {
        "schema": "build-finance.schema-lock/v1",
        "primary_contract_count": 8,
        "supporting_contract_count": 13,
        "json_attachment_schema_count": 27,
        "binary_formula_count": 1,
        "generated_json_schema_count": 48,
        "total_authority_contract_count": 49,
        "primary_bundle_sha256": primary_digest,
        "supporting_bundle_sha256": _family_digest(supporting_rows),
        "attachment_bundle_sha256": _family_digest(attachment_rows),
        "binary_formula_bundle_sha256": _family_digest(formula_rows),
        "schema_bundle_sha256": bundle_digest,
    }
    return {
        _FORMULA_PATH: _FORMULA_RECORD,
        "schema-bundle.json": bundle_payload + b"\n",
        "schema-bundle.sha256": bundle_digest.encode("ascii") + b"\n",
        "schema-lock.json": canonical_record_bytes(lock),
    }


def build_expected_resources(
    scope: ScopeName,
    *,
    resources_root: Path = _RESOURCE_ROOT,
) -> tuple[dict[str, bytes], frozenset[str]]:
    """Build expected bytes and return paths that writes must preserve."""
    plan = get_scope_plan(scope)
    documents = get_defined_schema_documents()
    selected = _require_definitions(scope, documents)
    _validate_definitions(documents)

    expected = {_schema_path(schema_id): canonical_record_bytes(documents[schema_id]) for schema_id in selected}
    preserved: set[str] = set()
    if scope == "primary":
        bundle_record = canonical_record_bytes(_primary_bundle(documents))
        expected["primary-schema-bundle.json"] = bundle_record
        expected["primary-schema-bundle.sha256"] = sha256_hex(bundle_record[:-1]).encode("ascii") + b"\n"
    elif scope == "full":
        expected.update(_full_metadata_resources(resources_root, documents))
        preserved.update(_schema_path(schema_id) for schema_id in plan.preserved_schema_ids)
    return expected, frozenset(preserved)


def _owned_schema_paths(scope: ScopeName, documents: dict[str, JsonObject]) -> frozenset[str]:
    if scope == "primary":
        schema_ids = PRIMARY_SCHEMA_IDS
    elif scope == "supporting":
        schema_ids = tuple(schema_id for schema_id in SUPPORTING_SCHEMA_IDS if schema_id in documents)
    else:
        schema_ids = ALL_JSON_SCHEMA_IDS
    return frozenset(_schema_path(schema_id) for schema_id in schema_ids)


def _resource_paths(directory: Path, pattern: str, *, prefix: str = "") -> frozenset[str]:
    if not directory.is_dir():
        return frozenset()
    return frozenset(f"{prefix}{path.name}" for path in directory.glob(pattern))


def _check_global_managed_inventory(resources_root: Path) -> frozenset[str]:
    actual_root_resources = _resource_paths(resources_root, "*.json") | _resource_paths(resources_root, "*.sha256")
    known_root_resources = _KNOWN_ROOT_JSON_RESOURCES | _KNOWN_ROOT_DIGEST_RESOURCES
    unknown_root_resources = actual_root_resources.difference(known_root_resources)
    if unknown_root_resources:
        raise ValueError(f"unknown managed root resource(s): {sorted(unknown_root_resources)!r}")

    actual_schema_resources = _resource_paths(resources_root / "schemas", "*.json", prefix="schemas/")
    unknown_schema_resources = actual_schema_resources.difference(_KNOWN_SCHEMA_RESOURCES)
    if unknown_schema_resources:
        raise ValueError(f"unknown managed schema resource(s): {sorted(unknown_schema_resources)!r}")

    actual_formula_resources = _resource_paths(resources_root / "formulas", "*.txt", prefix="formulas/")
    unknown_formula_resources = actual_formula_resources.difference(_KNOWN_FORMULA_RESOURCES)
    if unknown_formula_resources:
        raise ValueError(f"unknown managed formula resource(s): {sorted(unknown_formula_resources)!r}")
    return actual_schema_resources


def _check_scope_managed_inventory(scope: ScopeName, resources_root: Path) -> frozenset[str]:
    """Reject unknown global resources and undefined schemas owned by this scope."""
    actual_schema_resources = _check_global_managed_inventory(resources_root)
    documents = get_defined_schema_documents()
    owned_schema_paths = _owned_schema_paths(scope, documents)
    checked_schema_paths = frozenset(_schema_path(schema_id) for schema_id in get_scope_plan(scope).checked_schema_ids)
    undefined_owned_resources = actual_schema_resources.intersection(checked_schema_paths).difference(
        owned_schema_paths
    )
    if undefined_owned_resources:
        raise ValueError(f"extra generated schema resource(s): {sorted(undefined_owned_resources)!r}")
    return actual_schema_resources


def write_resources(scope: ScopeName, *, resources_root: Path = _RESOURCE_ROOT) -> None:
    """Write only scope-owned bytes, never overwriting sealed primary schemas."""
    _check_scope_managed_inventory(scope, resources_root)
    expected, preserved = build_expected_resources(scope, resources_root=resources_root)
    for relative_path in preserved:
        path = resources_root / relative_path
        if not path.is_file() or path.read_bytes() != expected[relative_path]:
            raise ValueError(f"sealed primary resource is missing or differs: {relative_path}")
    _check_scope_managed_inventory(scope, resources_root)
    for relative_path, payload in expected.items():
        if relative_path in preserved:
            continue
        path = resources_root / relative_path
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(payload)


def check_resources(scope: ScopeName, *, resources_root: Path = _RESOURCE_ROOT) -> None:
    """Reject any missing, extra, or byte-different resource owned by scope."""
    _check_scope_managed_inventory(scope, resources_root)
    expected, _ = build_expected_resources(scope, resources_root=resources_root)
    for relative_path, payload in expected.items():
        path = resources_root / relative_path
        if not path.is_file():
            raise ValueError(f"generated resource is missing: {relative_path}")
        if path.read_bytes() != payload:
            raise ValueError(f"generated resource differs: {relative_path}")


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--scope", choices=tuple(_SCOPE_PLANS), required=True)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--write", action="store_true")
    mode.add_argument("--check", action="store_true")
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    """Run deterministic write or check mode."""
    parser = _parser()
    arguments = parser.parse_args(argv)
    try:
        if arguments.write:
            write_resources(arguments.scope)
        else:
            check_resources(arguments.scope)
    except (OSError, UnicodeError, ValueError) as error:
        parser.error(str(error))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
