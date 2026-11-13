"""Registry-owned content IDs for offline live-paper contracts."""

from __future__ import annotations

import hashlib
from collections.abc import Mapping
from typing import Any, NoReturn, TypeAlias

from build_finance.live_paper.contracts import SELF_ID_FIELDS

JsonValue: TypeAlias = Any

_MIN_SAFE_INTEGER = -9_007_199_254_740_991
_MAX_SAFE_INTEGER = 9_007_199_254_740_991
_SHORT_ESCAPES = {
    "\b": "\\b",
    "\t": "\\t",
    "\n": "\\n",
    "\f": "\\f",
    "\r": "\\r",
}


def _canonical_string_bytes(value: str) -> bytes:
    escaped = ['"']
    for character in value:
        if character in _SHORT_ESCAPES:
            escaped.append(_SHORT_ESCAPES[character])
        elif character == '"':
            escaped.append('\\"')
        elif character == "\\":
            escaped.append("\\\\")
        elif ord(character) <= 0x1F:
            escaped.append(f"\\u{ord(character):04x}")
        else:
            escaped.append(character)
    escaped.append('"')
    return "".join(escaped).encode("utf-8", errors="strict")


def _utf16_sort_key(key: str) -> bytes:
    return key.encode("utf-16-be", errors="strict")


def _reject_unsupported(value: object) -> NoReturn:
    raise TypeError(f"unsupported live-paper canonical JSON value: {type(value).__name__}")


def canonical_json_bytes(value: JsonValue) -> bytes:
    """Return restricted canonical JSON bytes with no float authority."""
    if value is None:
        return b"null"
    if isinstance(value, bool):
        return b"true" if value else b"false"
    if isinstance(value, int):
        if not _MIN_SAFE_INTEGER <= value <= _MAX_SAFE_INTEGER:
            raise TypeError("integer is outside the interoperable JSON range")
        return str(value).encode("ascii")
    if isinstance(value, float):
        raise TypeError("floating-point JSON numbers are forbidden in live-paper authority records")
    if isinstance(value, str):
        return _canonical_string_bytes(value)
    if isinstance(value, list):
        return b"[" + b",".join(canonical_json_bytes(item) for item in value) + b"]"
    if isinstance(value, dict):
        if not all(isinstance(key, str) for key in value):
            raise TypeError("canonical JSON object keys must be strings")
        sorted_keys = sorted(value, key=_utf16_sort_key)
        members = (_canonical_string_bytes(key) + b":" + canonical_json_bytes(value[key]) for key in sorted_keys)
        return b"{" + b",".join(members) + b"}"
    _reject_unsupported(value)


def canonical_record_bytes(value: Mapping[str, JsonValue]) -> bytes:
    """Return one LF-terminated canonical object record."""
    return canonical_json_bytes(dict(value)) + b"\n"


def sha256_hex(payload: bytes) -> str:
    """Return the lowercase SHA-256 digest for exactly the supplied bytes."""
    return hashlib.sha256(payload).hexdigest()


def _self_id_field(document: Mapping[str, JsonValue]) -> str:
    schema_id = document.get("schema")
    if not isinstance(schema_id, str):
        raise ValueError("content-addressed document requires a string schema tag")
    try:
        return SELF_ID_FIELDS[schema_id]
    except KeyError as error:
        raise KeyError(f"unknown live-paper content-addressed schema: {schema_id!r}") from error


def compute_content_id(document: Mapping[str, JsonValue]) -> str:
    """Hash canonical bytes after omitting exactly the registered self-ID field."""
    self_id_field = _self_id_field(document)
    body = dict(document)
    body.pop(self_id_field, None)
    return sha256_hex(canonical_json_bytes(body))


def seal_content_id(document: Mapping[str, JsonValue]) -> dict[str, JsonValue]:
    """Return a shallow sealed copy, rejecting any supplied mismatching ID."""
    self_id_field = _self_id_field(document)
    content_id = compute_content_id(document)
    supplied = document.get(self_id_field)
    if self_id_field in document and supplied != content_id:
        raise ValueError(f"supplied {self_id_field} does not match computed content ID")
    sealed = dict(document)
    sealed[self_id_field] = content_id
    return sealed


def verify_content_id(document: Mapping[str, JsonValue]) -> bool:
    """Return whether a present registered self-ID matches its canonical document."""
    self_id_field = _self_id_field(document)
    supplied = document.get(self_id_field)
    return isinstance(supplied, str) and supplied == compute_content_id(document)
