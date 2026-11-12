"""Restricted canonical JSON parsing for replay contracts."""

from __future__ import annotations

import hashlib
import json
from typing import NoReturn, TypeAlias

from build_finance.crypto_replay.errors import CanonicalJSONError, DuplicateKeyError

# mypy does not model the mandated runtime type(None) expression as a type alias operand.
JsonScalar: TypeAlias = type(None) | bool | int | str  # type: ignore[valid-type]
JsonValue: TypeAlias = JsonScalar | list["JsonValue"] | dict[str, "JsonValue"]
JsonObject: TypeAlias = dict[str, JsonValue]

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
    escaped: list[str] = ['"']
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

    try:
        return "".join(escaped).encode("utf-8", errors="strict")
    except UnicodeEncodeError as error:
        raise CanonicalJSONError("string contains a lone surrogate") from error


def _utf16_sort_key(key: str) -> bytes:
    try:
        return key.encode("utf-16-be", errors="strict")
    except UnicodeEncodeError as error:
        raise CanonicalJSONError("object key contains a lone surrogate") from error


def canonical_json_bytes(value: JsonValue) -> bytes:
    """Return restricted canonical JSON bytes for a supported value."""
    if value is None:
        return b"null"
    if isinstance(value, bool):
        return b"true" if value else b"false"
    if isinstance(value, int):
        if not _MIN_SAFE_INTEGER <= value <= _MAX_SAFE_INTEGER:
            raise CanonicalJSONError("integer is outside the interoperable JSON range")
        return str(value).encode("ascii")
    if isinstance(value, str):
        return _canonical_string_bytes(value)
    if isinstance(value, list):
        return b"[" + b",".join(canonical_json_bytes(item) for item in value) + b"]"
    if isinstance(value, dict):
        if not all(isinstance(key, str) for key in value):
            raise CanonicalJSONError("canonical JSON object keys must be strings")
        sorted_keys = sorted(value, key=_utf16_sort_key)
        members = (_canonical_string_bytes(key) + b":" + canonical_json_bytes(value[key]) for key in sorted_keys)
        return b"{" + b",".join(members) + b"}"
    raise CanonicalJSONError(f"unsupported canonical JSON value: {type(value).__name__}")


def _unique_object(pairs: list[tuple[str, JsonValue]]) -> dict[str, JsonValue]:
    result: dict[str, JsonValue] = {}
    for key, value in pairs:
        if key in result:
            raise DuplicateKeyError(key)
        result[key] = value
    return result


def _reject_float(value: str) -> NoReturn:
    raise CanonicalJSONError(f"floating-point JSON number is forbidden: {value}")


def _reject_constant(value: str) -> NoReturn:
    raise CanonicalJSONError(f"non-finite JSON number is forbidden: {value}")


def parse_canonical_json(payload: bytes) -> JsonObject:
    """Parse one canonical object payload without a record terminator."""
    try:
        text = payload.decode("utf-8", errors="strict")
    except UnicodeDecodeError as error:
        raise CanonicalJSONError("payload is not strict UTF-8") from error

    try:
        value = json.loads(
            text,
            object_pairs_hook=_unique_object,
            parse_float=_reject_float,
            parse_constant=_reject_constant,
        )
    except CanonicalJSONError:
        raise
    except json.JSONDecodeError as error:
        raise CanonicalJSONError("payload is not valid JSON") from error

    if not isinstance(value, dict):
        raise CanonicalJSONError("canonical JSON root must be an object")
    if canonical_json_bytes(value) != payload:
        raise CanonicalJSONError("payload is not canonical")
    return value


def canonical_record_bytes(value: JsonObject) -> bytes:
    """Return a canonical object payload followed by one LF."""
    return canonical_json_bytes(value) + b"\n"


def parse_canonical_record(record: bytes) -> JsonObject:
    """Parse one canonical object record terminated by exactly one LF."""
    if not record.endswith(b"\n") or record.endswith(b"\n\n"):
        raise CanonicalJSONError("record requires exactly one LF")
    payload = record[:-1]
    value = parse_canonical_json(payload)
    if canonical_json_bytes(value) != payload:
        raise CanonicalJSONError("payload is not canonical")
    return value


def sha256_hex(payload: bytes) -> str:
    """Return the lowercase SHA-256 digest of exactly the supplied bytes."""
    return hashlib.sha256(payload).hexdigest()
