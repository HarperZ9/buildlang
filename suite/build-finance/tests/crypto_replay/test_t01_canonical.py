"""T01 RED contract for strict canonical JSON bytes and LF records."""

from __future__ import annotations

import base64
import hashlib
import json
from pathlib import Path

import pytest

RESOURCE_ROOT = Path(__file__).parent / "resources"


def _adversarial_resource() -> dict[str, object]:
    resource = json.loads((RESOURCE_ROOT / "adversarial-bytes.json").read_text(encoding="utf-8"))
    expected_names = [
        "bom",
        "duplicate_key",
        "leading_whitespace",
        "trailing_whitespace",
        "crlf",
        "missing_lf",
        "double_lf",
        "comment",
        "nan",
        "infinity",
        "float",
        "negative_zero",
        "unsafe_json_integer",
        "trailing_object",
        "invalid_utf8",
        "noncanonical_key_order",
        "unnecessary_escape",
        "lone_surrogate",
    ]
    assert resource["schema"] == "build-finance.adversarial-bytes/v1"
    assert [row["name"] for row in resource["cases"]] == expected_names
    return resource


def _adversarial_base64(name: str) -> str:
    resource = _adversarial_resource()
    return next(row["record_base64"] for row in resource["cases"] if row["name"] == name)


def _adversarial_rows() -> list[pytest.ParameterSet]:
    resource = _adversarial_resource()
    return [pytest.param(row["record_base64"], id=row["name"]) for row in resource["cases"]]


def test_duplicate_key_rejected_before_parse() -> None:
    from build_finance.crypto_replay.canonical import parse_canonical_record
    from build_finance.crypto_replay.errors import CanonicalJSONError

    duplicate = base64.b64decode(_adversarial_base64("duplicate_key"), validate=True)
    with pytest.raises(CanonicalJSONError):
        parse_canonical_record(duplicate)


def test_jcs_integer_authority_boundaries() -> None:
    from build_finance.crypto_replay.canonical import canonical_json_bytes
    from build_finance.crypto_replay.errors import CanonicalJSONError

    assert canonical_json_bytes(-(2**53) + 1) == b"-9007199254740991"
    assert canonical_json_bytes(2**53 - 1) == b"9007199254740991"
    assert canonical_json_bytes(True) == b"true"

    for value in (-(2**53), 2**53):
        with pytest.raises(CanonicalJSONError):
            canonical_json_bytes(value)


def test_jcs_known_vectors() -> None:
    from build_finance.crypto_replay.canonical import canonical_json_bytes

    resource = json.loads((RESOURCE_ROOT / "jcs-known-vectors.json").read_text(encoding="utf-8"))
    assert resource["schema"] == "build-finance.jcs-known-vectors/v1"
    assert [row["name"] for row in resource["vectors"]] == [
        "canonical_nested",
        "control_and_string_escaping",
        "integer_rendering",
        "utf16_key_order",
    ]

    for row in resource["vectors"]:
        expected = base64.b64decode(row["canonical_base64"], validate=True)
        assert canonical_json_bytes(row["value"]) == expected
        assert hashlib.sha256(expected).hexdigest() == row["sha256"]

    assert canonical_json_bytes(["é", {"\ue000": "bmp", "\U0001f600": "astral"}]) == (
        '["é",{"😀":"astral","\ue000":"bmp"}]'.encode()
    )


@pytest.mark.parametrize("record_base64", _adversarial_rows())
def test_noncanonical_byte_matrix_is_rejected(record_base64: str) -> None:
    from build_finance.crypto_replay.canonical import parse_canonical_record
    from build_finance.crypto_replay.errors import CanonicalJSONError

    record = base64.b64decode(record_base64, validate=True)
    with pytest.raises(CanonicalJSONError):
        parse_canonical_record(record)


def test_record_lf_is_excluded_from_digest() -> None:
    from build_finance.crypto_replay.canonical import (
        canonical_json_bytes,
        canonical_record_bytes,
        parse_canonical_record,
        sha256_hex,
    )

    document = {"answer": 42}
    payload = canonical_json_bytes(document)
    record = canonical_record_bytes(document)

    assert record == payload + b"\n"
    assert parse_canonical_record(record) == document
    assert sha256_hex(payload) != sha256_hex(record)
