"""Registry-owned content IDs for self-addressed replay contracts."""

from __future__ import annotations

from collections.abc import Mapping

from build_finance.crypto_replay.canonical import JsonValue, canonical_json_bytes, sha256_hex
from build_finance.crypto_replay.schema_definitions import CONTRACT_SPECS_BY_SCHEMA


def _self_id_field(document: Mapping[str, JsonValue]) -> str:
    schema_id = document.get("schema")
    if not isinstance(schema_id, str):
        raise ValueError("content-addressed document requires a string schema tag")
    try:
        spec = CONTRACT_SPECS_BY_SCHEMA[schema_id]
    except KeyError as error:
        raise KeyError(f"unknown content-addressed schema: {schema_id!r}") from error
    if spec.self_id_field is None:
        raise ValueError(f"schema is not a self-addressed contract: {schema_id!r}")
    return spec.self_id_field


def compute_content_id(document: Mapping[str, JsonValue]) -> str:
    """Hash canonical bytes after omitting exactly the registered self-ID."""
    self_id_field = _self_id_field(document)
    body: dict[str, JsonValue] = dict(document)
    body.pop(self_id_field, None)
    return sha256_hex(canonical_json_bytes(body))


def seal_content_id(document: Mapping[str, JsonValue]) -> dict[str, JsonValue]:
    """Return a shallow sealed copy, rejecting any supplied mismatching ID."""
    self_id_field = _self_id_field(document)
    content_id = compute_content_id(document)
    supplied = document.get(self_id_field)
    if self_id_field in document and supplied != content_id:
        raise ValueError(f"supplied {self_id_field} does not match computed content ID")
    sealed: dict[str, JsonValue] = dict(document)
    sealed[self_id_field] = content_id
    return sealed


def verify_content_id(document: Mapping[str, JsonValue]) -> bool:
    """Return whether a present registered self-ID matches its document."""
    self_id_field = _self_id_field(document)
    if self_id_field not in document:
        return False
    supplied = document[self_id_field]
    return isinstance(supplied, str) and supplied == compute_content_id(document)
