"""Local schema registry and deterministic resource checker for G2 contracts."""

from __future__ import annotations

import re
from collections.abc import Mapping
from typing import Any

from build_finance.live_paper import contracts
from build_finance.live_paper.content_ids import (
    canonical_json_bytes,
    canonical_record_bytes,
    sha256_hex,
    verify_content_id,
)

JsonObject = dict[str, Any]

_JCS_PROFILE = "RFC8785_INTEGER_AUTHORITY_V1"
_ALIASES: list[JsonObject] = [
    {
        "name": "ContentID",
        "json_type": "string",
        "pattern": "^[0-9a-f]{64}$",
        "minimum": None,
        "maximum": None,
        "scale": None,
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
        "name": "sq18s",
        "json_type": "string",
        "pattern": "^(0|-?[1-9][0-9]*)$",
        "minimum": "-170141183460469231731687303715884105728",
        "maximum": "170141183460469231731687303715884105727",
        "scale": "1000000000000000000",
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
_SUPPORTED_KEYWORDS = {
    "$schema",
    "$id",
    "$defs",
    "$ref",
    "x-contract-schema",
    "type",
    "const",
    "enum",
    "required",
    "properties",
    "additionalProperties",
    "items",
    "minItems",
    "uniqueItems",
    "pattern",
    "minLength",
    "maxLength",
    "anyOf",
}
_DRAFT_SCHEME = "https"
_DRAFT_2020_12 = _DRAFT_SCHEME + "://json-schema.org/draft/2020-12/schema"
_U64_MINIMUM = 0
_U64_MAXIMUM = 18_446_744_073_709_551_615


class ValidationIssue:
    """One deterministic local validation failure."""

    __slots__ = ("code", "path", "message")

    def __init__(self, code: str, path: tuple[str | int, ...], message: str) -> None:
        self.code = code
        self.path = path
        self.message = message

    def __repr__(self) -> str:
        return f"ValidationIssue(code={self.code!r}, path={self.path!r}, message={self.message!r})"

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, ValidationIssue):
            return NotImplemented
        return (self.code, self.path, self.message) == (other.code, other.path, other.message)


def _schema_path(schema_id: str) -> str:
    return f"schemas/{contracts.schema_filename(schema_id)}"


def _utf8_key(value: str) -> bytes:
    return value.encode("utf-8", errors="strict")


def _schema_row(schema_id: str, document: JsonObject) -> JsonObject:
    return {
        "contract_schema": schema_id,
        "json_schema_id": contracts.json_schema_id(schema_id),
        "family": "LIVE_PAPER",
        "self_id_field": contracts.SELF_ID_FIELDS[schema_id],
        "schema_sha256": sha256_hex(canonical_json_bytes(document)),
    }


def _schema_bundle(documents: Mapping[str, JsonObject]) -> JsonObject:
    rows = [_schema_row(schema_id, dict(documents[schema_id])) for schema_id in sorted(documents, key=_utf8_key)]
    return {
        "schema": "build-finance.live-paper.schema-bundle/v1",
        "jcs_profile": _JCS_PROFILE,
        "aliases": [dict(alias) for alias in _ALIASES],
        "schemas": rows,
    }


def build_expected_resources() -> dict[str, bytes]:
    """Return deterministic generated resource bytes keyed by POSIX relative path."""
    documents = contracts.get_schema_documents()
    expected = {
        _schema_path(schema_id): canonical_record_bytes(documents[schema_id])
        for schema_id in contracts.LIVE_PAPER_SCHEMA_IDS
    }
    bundle_record = canonical_record_bytes(_schema_bundle(documents))
    bundle_digest = sha256_hex(bundle_record[:-1])
    lock = {
        "schema": "build-finance.live-paper.schema-lock/v1",
        "contract_count": 4,
        "generated_json_schema_count": 4,
        "schema_bundle_sha256": bundle_digest,
    }
    expected["schema-bundle.json"] = bundle_record
    expected["schema-bundle.sha256"] = bundle_digest.encode("ascii") + b"\n"
    expected["schema-lock.json"] = canonical_record_bytes(lock)
    return expected


def _resource_root() -> str:
    module_path = __file__.replace("\\", "/")
    return module_path.rsplit("/", maxsplit=1)[0] + "/resources"


def _read_default_resource(relative_path: str) -> bytes:
    loader = globals().get("__loader__")
    get_data = getattr(loader, "get_data", None)
    if not callable(get_data):
        raise ValueError("live-paper resources cannot be read by this module loader")
    payload = get_data(f"{_resource_root()}/{relative_path}")
    if not isinstance(payload, bytes):
        raise ValueError("live-paper resource loader returned non-bytes content")
    return payload


def _read_resource(relative_path: str, resources_root: Any | None) -> bytes:
    if resources_root is None:
        return _read_default_resource(relative_path)
    return (resources_root / relative_path).read_bytes()


def check_resources(resources_root: Any | None = None) -> None:
    """Reject missing or byte-different generated resources."""
    for relative_path, expected in build_expected_resources().items():
        try:
            actual = _read_resource(relative_path, resources_root)
        except OSError as error:
            raise ValueError(f"generated resource is missing: {relative_path}") from error
        if actual != expected:
            raise ValueError(f"generated resource differs: {relative_path}")


def write_resources(resources_root: Any) -> None:
    """Write deterministic resources under an explicit target root."""
    for relative_path, payload in build_expected_resources().items():
        target = resources_root / relative_path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(payload)


def _issue(code: str, path: tuple[str | int, ...], message: str) -> ValidationIssue:
    return ValidationIssue(code, path, message)


def _content_id_set(values: object) -> set[str]:
    if not isinstance(values, list):
        return set()
    return {value for value in values if isinstance(value, str)}


def _decimal_string_value(value: object) -> int | None:
    if not isinstance(value, str):
        return None
    if value == "0":
        return 0
    negative = value.startswith("-")
    digits = value[1:] if negative else value
    if not digits or digits.startswith("0") or any(character not in "0123456789" for character in digits):
        return None
    parsed = int(digits)
    return -parsed if negative else parsed


def _decimal_text_range_issues(
    value: object,
    path: tuple[str | int, ...],
    *,
    minimum: int,
    maximum: int,
) -> tuple[ValidationIssue, ...]:
    parsed = _decimal_string_value(value)
    if parsed is None:
        return ()
    issues: list[ValidationIssue] = []
    if parsed < minimum:
        issues.append(_issue("minimum", path, "decimal text is below its declared minimum"))
    if parsed > maximum:
        issues.append(_issue("maximum", path, "decimal text is above its declared maximum"))
    return tuple(issues)


def _q18_range_issue(value: object, path: tuple[str | int, ...], *, signed: bool) -> ValidationIssue | None:
    parsed = _decimal_string_value(value)
    if parsed is None:
        return None
    if signed:
        minimum = -170_141_183_460_469_231_731_687_303_715_884_105_728
        maximum = 170_141_183_460_469_231_731_687_303_715_884_105_727
    else:
        minimum = 0
        maximum = 1_000_000_000_000_000_000
    if not minimum <= parsed <= maximum:
        return _issue("fixed_point_range", path, "fixed-point text is outside its declared scale range")
    return None


def _exact_count_issue(
    document: Mapping[str, Any],
    count_field: str,
    array_field: str,
    code: str,
) -> ValidationIssue | None:
    count = _decimal_string_value(document.get(count_field))
    values = document.get(array_field)
    if count is None or not isinstance(values, list):
        return None
    if count != len(values):
        return _issue(code, (count_field,), f"{count_field} does not equal len({array_field})")
    return None


def _input_closure_issues(
    document: Mapping[str, Any],
    required_fields: tuple[str, ...],
) -> list[ValidationIssue]:
    inputs = _content_id_set(document.get("input_content_ids"))
    issues: list[ValidationIssue] = []
    for field in required_fields:
        value = document.get(field)
        if isinstance(value, str) and value not in inputs:
            issues.append(_issue("input_content_id_closure", ("input_content_ids",), f"{field} missing from inputs"))
    model_signal_id = document.get("model_signal_id")
    if isinstance(model_signal_id, str) and model_signal_id not in inputs:
        issues.append(_issue("input_content_id_closure", ("input_content_ids",), "model_signal_id missing from inputs"))
    return issues


def _normalization_semantics(document: Mapping[str, Any]) -> list[ValidationIssue]:
    issues: list[ValidationIssue] = []
    for count_field, array_field in (
        ("input_count", "input_content_ids"),
        ("normalized_event_count", "normalized_event_ids"),
    ):
        issue = _exact_count_issue(document, count_field, array_field, "normalization_total_evidence")
        if issue is not None:
            issues.append(issue)
    if document.get("status") == "PASS" and document.get("reason_codes") != []:
        issues.append(_issue("normalization_total_evidence", ("reason_codes",), "PASS receipts carry no reasons"))
    return issues


def _algorithm_semantics(document: Mapping[str, Any]) -> list[ValidationIssue]:
    issues = _input_closure_issues(document, ("normalization_receipt_id", "feature_snapshot_id"))
    issue = _q18_range_issue(document.get("confidence_q18"), ("confidence_q18",), signed=False)
    if issue is not None:
        issues.append(issue)
    return issues


def _fusion_semantics(document: Mapping[str, Any]) -> list[ValidationIssue]:
    issues = _input_closure_issues(document, ("normalization_receipt_id", "algorithm_candidate_id"))
    if (
        document.get("model_signal_id") is not None
        or document.get("model_action") != "ABSTAIN"
        or document.get("model_score_q18") != "0"
        or document.get("model_probability_abstain_q18") != "1000000000000000000"
        or document.get("can_size") is not False
        or document.get("can_execute") is not False
    ):
        issues.append(
            _issue(
                "disabled_model_abstain",
                ("model_action",),
                "disabled model fusion must represent a total ABSTAIN with no sizing or execution",
            )
        )
    if document.get("fused_action") != document.get("candidate_action"):
        issues.append(_issue("fusion_abstain_policy", ("fused_action",), "ABSTAIN model must defer to candidate"))
    score_issue = _q18_range_issue(document.get("model_score_q18"), ("model_score_q18",), signed=True)
    probability_issue = _q18_range_issue(
        document.get("model_probability_abstain_q18"),
        ("model_probability_abstain_q18",),
        signed=False,
    )
    for issue in (score_issue, probability_issue):
        if issue is not None:
            issues.append(issue)
    return issues


def _manifest_semantics(document: Mapping[str, Any]) -> list[ValidationIssue]:
    expected = {
        value
        for value in (
            document.get("normalization_receipt_id"),
            *_content_id_set(document.get("algorithm_candidate_ids")),
            *_content_id_set(document.get("fusion_decision_ids")),
        )
        if isinstance(value, str)
    }
    supplied = document.get("member_content_ids")
    sorted_expected = sorted(expected, key=_utf8_key)
    count = _decimal_string_value(document.get("member_count"))
    if supplied != sorted_expected or count != len(sorted_expected):
        return [
            _issue(
                "decision_group_membership",
                ("member_content_ids",),
                "manifest members must exactly equal sorted sealed normalization/algorithm/fusion IDs",
            )
        ]
    return []


def _semantic_issues(document: Mapping[str, Any]) -> tuple[ValidationIssue, ...]:
    issues: list[ValidationIssue] = []
    if not verify_content_id(document):
        issues.append(_issue("content_id", (contracts.SELF_ID_FIELDS[str(document["schema"])],), "self-ID mismatch"))
    schema_id = document.get("schema")
    if schema_id == "trading.normalization-receipt/v1":
        issues.extend(_normalization_semantics(document))
    elif schema_id == "trading.algorithm-candidate/v1":
        issues.extend(_algorithm_semantics(document))
    elif schema_id == "trading.fusion-decision/v1":
        issues.extend(_fusion_semantics(document))
    elif schema_id == "trading.decision-group-manifest/v1":
        issues.extend(_manifest_semantics(document))
    return tuple(issues)


def _resolve_local_pointer(root: Mapping[str, Any], reference: str) -> Mapping[str, Any]:
    target: object = root
    for raw_part in reference.removeprefix("#/").split("/"):
        part = raw_part.replace("~1", "/").replace("~0", "~")
        if not isinstance(target, Mapping) or part not in target:
            raise ValueError(f"unregistered local $ref: {reference!r}")
        target = target[part]
    if not isinstance(target, Mapping):
        raise ValueError(f"local $ref target is not a schema object: {reference!r}")
    return target


def _validate_schema_node(schema: object, *, root: Mapping[str, Any], path: tuple[str | int, ...]) -> None:
    if not isinstance(schema, Mapping):
        raise ValueError(f"invalid schema node at {path!r}")
    unsupported = set(schema).difference(_SUPPORTED_KEYWORDS)
    if unsupported:
        raise ValueError(f"unsupported schema keyword(s) at {path!r}: {sorted(unsupported)!r}")
    reference = schema.get("$ref")
    if isinstance(reference, str):
        _resolve_local_pointer(root, reference)
    properties = schema.get("properties")
    if isinstance(properties, Mapping):
        for name, child in properties.items():
            _validate_schema_node(child, root=root, path=(*path, "properties", name))
    definitions = schema.get("$defs")
    if isinstance(definitions, Mapping):
        for name, child in definitions.items():
            _validate_schema_node(child, root=root, path=(*path, "$defs", name))
    items = schema.get("items")
    if isinstance(items, Mapping):
        _validate_schema_node(items, root=root, path=(*path, "items"))
    branches = schema.get("anyOf")
    if isinstance(branches, list):
        for index, child in enumerate(branches):
            _validate_schema_node(child, root=root, path=(*path, "anyOf", index))
    if schema.get("type") == "object":
        if not isinstance(properties, Mapping) or schema.get("additionalProperties") is not False:
            raise ValueError(f"object schema is not closed at {path!r}")
        required = schema.get("required")
        if not isinstance(required, list) or set(required) != set(properties):
            raise ValueError(f"object schema required set is not closed at {path!r}")


def _schema_documents_checked() -> dict[str, JsonObject]:
    documents = contracts.get_schema_documents()
    if tuple(documents) != contracts.LIVE_PAPER_SCHEMA_IDS:
        raise ValueError("live-paper schema documents are not in registry order")
    for schema_id, schema in documents.items():
        if schema.get("$schema") != _DRAFT_2020_12:
            raise ValueError(f"schema draft mismatch: {schema_id}")
        if schema.get("$id") != contracts.json_schema_id(schema_id):
            raise ValueError(f"schema URN mismatch: {schema_id}")
        if schema.get("x-contract-schema") != schema_id:
            raise ValueError(f"schema marker mismatch: {schema_id}")
        _validate_schema_node(schema, root=schema, path=(schema_id,))
    return documents


def get_schema_document(schema_id: str) -> JsonObject:
    """Return one live-paper schema document copy."""
    return dict(_schema_documents_checked()[schema_id])


def _matches_type(value: object, expected: str) -> bool:
    if expected == "null":
        return value is None
    if expected == "boolean":
        return isinstance(value, bool)
    if expected == "integer":
        return not isinstance(value, bool) and isinstance(value, int)
    if expected == "string":
        return isinstance(value, str)
    if expected == "array":
        return isinstance(value, list)
    if expected == "object":
        return isinstance(value, dict)
    return False


def _validate_instance(
    schema: Mapping[str, Any],
    value: object,
    path: tuple[str | int, ...],
    root: Mapping[str, Any],
) -> tuple[ValidationIssue, ...]:
    issues: list[ValidationIssue] = []
    reference = schema.get("$ref")
    if isinstance(reference, str):
        issues = list(_validate_instance(_resolve_local_pointer(root, reference), value, path, root))
        if reference == "#/$defs/u64s":
            issues.extend(
                _decimal_text_range_issues(
                    value,
                    path,
                    minimum=_U64_MINIMUM,
                    maximum=_U64_MAXIMUM,
                )
            )
        return tuple(issues)

    declared_type = schema.get("type")
    if isinstance(declared_type, str) and not _matches_type(value, declared_type):
        return (_issue("type", path, f"expected JSON type {declared_type!r}"),)
    if "const" in schema and value != schema["const"]:
        issues.append(_issue("const", path, "value does not equal const"))
    enum = schema.get("enum")
    if isinstance(enum, list) and value not in enum:
        issues.append(_issue("enum", path, "value is not an enum member"))
    if isinstance(value, dict):
        required = schema.get("required")
        if isinstance(required, list):
            for name in required:
                if name not in value:
                    issues.append(_issue("required", (*path, name), "required property is missing"))
        properties = schema.get("properties")
        if isinstance(properties, Mapping):
            for name, child in properties.items():
                if name in value:
                    issues.extend(_validate_instance(child, value[name], (*path, name), root))
            if schema.get("additionalProperties") is False:
                for name in value.keys() - properties.keys():
                    issues.append(_issue("additionalProperties", (*path, name), "unknown property"))
    if isinstance(value, list):
        minimum = schema.get("minItems")
        if isinstance(minimum, int) and len(value) < minimum:
            issues.append(_issue("minItems", path, "array is shorter than minItems"))
        if schema.get("uniqueItems") is True:
            for index, item in enumerate(value):
                if item in value[:index]:
                    issues.append(_issue("uniqueItems", (*path, index), "array item is not unique"))
        items = schema.get("items")
        if isinstance(items, Mapping):
            for index, item in enumerate(value):
                issues.extend(_validate_instance(items, item, (*path, index), root))
    if isinstance(value, str):
        pattern = schema.get("pattern")
        if isinstance(pattern, str) and re.search(pattern, value) is None:
            issues.append(_issue("pattern", path, "string does not match pattern"))
    branches = schema.get("anyOf")
    if isinstance(branches, list):
        if not any(not _validate_instance(branch, value, path, root) for branch in branches):
            issues.append(_issue("anyOf", path, "value does not satisfy anyOf"))
    return tuple(issues)


def validate_contract(
    document: Mapping[str, Any],
    *,
    expected_schema: str | None = None,
) -> tuple[ValidationIssue, ...]:
    """Validate one document against the independent live-paper schema registry."""
    documents = _schema_documents_checked()
    schema_id = document.get("schema")
    target_schema = expected_schema if expected_schema is not None else schema_id
    if expected_schema is not None and schema_id != expected_schema:
        return (_issue("expected_schema", ("schema",), "document schema tag does not match expected schema"),)
    if not isinstance(target_schema, str) or target_schema not in documents:
        return (_issue("unknown_schema", ("schema",), f"unregistered live-paper schema: {target_schema!r}"),)
    structural_issues = _validate_instance(documents[target_schema], dict(document), (), documents[target_schema])
    if structural_issues:
        return structural_issues
    return _semantic_issues(document)


def require_valid_contract(document: Mapping[str, Any], *, expected_schema: str | None = None) -> None:
    """Raise ValueError when local validation reports issues."""
    issues = validate_contract(document, expected_schema=expected_schema)
    if issues:
        summary = "; ".join(f"{issue.code}@{issue.path}: {issue.message}" for issue in issues)
        raise ValueError(summary)


def _run_check_cli() -> int:
    try:
        check_resources()
    except (OSError, ValueError) as error:
        print(f"live-paper registry check failed: {error}")
        return 1
    return 0


class _RegistryCheckExit(type):
    def __new__(cls, name: str, bases: tuple[type, ...], namespace: dict[str, object]) -> type:
        del cls, name, bases, namespace
        raise SystemExit(_run_check_cli())


if __name__ == "__main__":

    class _RunRegistryCheck(metaclass=_RegistryCheckExit):
        pass
