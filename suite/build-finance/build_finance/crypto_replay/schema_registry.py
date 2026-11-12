"""Fail-closed, local-only JSON Schema subset for replay contracts."""

from __future__ import annotations

import math
import re
from collections.abc import Iterable, Mapping, Sequence
from copy import deepcopy
from typing import Any, TypeGuard

from build_finance.crypto_replay.canonical import JsonObject, JsonValue
from build_finance.crypto_replay.schema_definitions import (
    CONTRACT_SPECS_BY_SCHEMA,
    get_defined_schema_documents,
    json_schema_id,
    schema_filename,
)
from build_finance.crypto_replay.schema_model import ContractSpec, EvidenceResolver, ValidationIssue

DRAFT_2020_12 = "https://json-schema.org/draft/2020-12/schema"
_LOCAL_URN_PREFIX = "urn:build-finance:contract:"
_SUPPORTED_KEYWORDS = frozenset(
    {
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
        "maxItems",
        "uniqueItems",
        "pattern",
        "minLength",
        "maxLength",
        "minimum",
        "maximum",
        "anyOf",
        "oneOf",
        "allOf",
        "not",
    }
)
_JSON_TYPES = frozenset({"null", "boolean", "object", "array", "number", "integer", "string"})


def _schema_error(path: tuple[str | int, ...], message: str) -> ValueError:
    location = "/".join(str(part) for part in path) or "<root>"
    return ValueError(f"invalid local schema at {location}: {message}")


def _is_number(value: object) -> TypeGuard[int | float]:
    return not isinstance(value, bool) and isinstance(value, (int, float)) and math.isfinite(value)


def _json_equal(left: object, right: object) -> bool:
    if isinstance(left, bool) or isinstance(right, bool):
        return type(left) is type(right) and left == right
    if _is_number(left) and _is_number(right):
        return left == right
    if type(left) is not type(right):
        return False
    if isinstance(left, list):
        assert isinstance(right, list)
        return len(left) == len(right) and all(_json_equal(a, b) for a, b in zip(left, right))
    if isinstance(left, dict):
        assert isinstance(right, dict)
        return left.keys() == right.keys() and all(_json_equal(left[key], right[key]) for key in left)
    return left == right


def _ensure_nonnegative_integer(value: object, path: tuple[str | int, ...]) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or value < 0:
        raise _schema_error(path, "expected a non-negative integer")
    return value


def _decode_pointer_part(part: str, path: tuple[str | int, ...]) -> str:
    index = 0
    decoded: list[str] = []
    while index < len(part):
        if part[index] != "~":
            decoded.append(part[index])
            index += 1
            continue
        if index + 1 >= len(part) or part[index + 1] not in {"0", "1"}:
            raise _schema_error(path, "local $ref contains an invalid JSON Pointer escape")
        decoded.append("~" if part[index + 1] == "0" else "/")
        index += 2
    return "".join(decoded)


def _resolve_local_pointer(
    root: Mapping[str, Any],
    reference: str,
    path: tuple[str | int, ...],
) -> Mapping[str, Any]:
    if not reference.startswith("#/$defs/"):
        raise _schema_error(path, "$ref must be a registered local URN or a local #/$defs path")
    target: object = root
    for raw_part in reference[2:].split("/"):
        part = _decode_pointer_part(raw_part, path)
        if not isinstance(target, Mapping) or part not in target:
            raise _schema_error(path, f"unregistered local $ref: {reference!r}")
        target = target[part]
    if not isinstance(target, Mapping):
        raise _schema_error(path, "$ref target is not a schema object")
    return target


def _validate_schema_node(
    schema: object,
    *,
    root: Mapping[str, Any],
    registered_urns: frozenset[str],
    path: tuple[str | int, ...],
    is_root: bool = False,
) -> None:
    if not isinstance(schema, Mapping):
        raise _schema_error(path, "boolean and non-object schemas are not supported")
    if not all(isinstance(keyword, str) for keyword in schema):
        raise _schema_error(path, "schema keyword names must be strings")
    unsupported = set(schema).difference(_SUPPORTED_KEYWORDS)
    if unsupported:
        raise _schema_error(path, f"unsupported keyword(s): {sorted(unsupported)!r}")

    metadata = {"$schema", "$id", "x-contract-schema"}
    if is_root:
        missing = metadata.difference(schema)
        if missing:
            raise _schema_error(path, f"missing required metadata: {sorted(missing)!r}")
        if schema["$schema"] != DRAFT_2020_12:
            raise _schema_error((*path, "$schema"), "only draft 2020-12 is supported")
        contract_schema = schema["x-contract-schema"]
        schema_urn = schema["$id"]
        if not isinstance(contract_schema, str):
            raise _schema_error((*path, "x-contract-schema"), "metadata must be a string")
        if not isinstance(schema_urn, str) or not schema_urn.startswith(_LOCAL_URN_PREFIX):
            raise _schema_error((*path, "$id"), "schema ID must be an absolute local contract URN")
        if schema_urn != json_schema_id(contract_schema):
            raise _schema_error((*path, "$id"), "schema ID does not match x-contract-schema")
    elif metadata.intersection(schema):
        raise _schema_error(path, "schema metadata is permitted only at the document root")

    definitions = schema.get("$defs")
    if definitions is not None:
        if not isinstance(definitions, Mapping) or not all(isinstance(name, str) for name in definitions):
            raise _schema_error((*path, "$defs"), "$defs must be an object with string names")
        for name, definition in definitions.items():
            _validate_schema_node(
                definition,
                root=root,
                registered_urns=registered_urns,
                path=(*path, "$defs", name),
            )

    reference = schema.get("$ref")
    if reference is not None:
        if not isinstance(reference, str):
            raise _schema_error((*path, "$ref"), "$ref must be a string")
        if reference.startswith("#"):
            _resolve_local_pointer(root, reference, (*path, "$ref"))
        elif reference not in registered_urns:
            raise _schema_error((*path, "$ref"), f"unregistered or non-local $ref: {reference!r}")

    declared_type = schema.get("type")
    declared_types: tuple[str, ...] = ()
    if declared_type is not None:
        if isinstance(declared_type, str):
            declared_types = (declared_type,)
        elif isinstance(declared_type, list) and declared_type and all(isinstance(item, str) for item in declared_type):
            declared_types = tuple(declared_type)
        else:
            raise _schema_error((*path, "type"), "type must be a supported string or non-empty string array")
        if len(set(declared_types)) != len(declared_types) or set(declared_types).difference(_JSON_TYPES):
            raise _schema_error((*path, "type"), "type contains duplicate or unsupported JSON types")

    enum = schema.get("enum")
    if enum is not None:
        if not isinstance(enum, list) or not enum:
            raise _schema_error((*path, "enum"), "enum must be a non-empty array")
        if any(_json_equal(left, right) for index, left in enumerate(enum) for right in enum[index + 1 :]):
            raise _schema_error((*path, "enum"), "enum values must be unique")

    properties = schema.get("properties")
    if properties is not None:
        if not isinstance(properties, Mapping) or not all(isinstance(name, str) for name in properties):
            raise _schema_error((*path, "properties"), "properties must be an object with string names")
        for name, property_schema in properties.items():
            _validate_schema_node(
                property_schema,
                root=root,
                registered_urns=registered_urns,
                path=(*path, "properties", name),
            )

    required = schema.get("required")
    if required is not None:
        if not isinstance(required, list) or not all(isinstance(name, str) for name in required):
            raise _schema_error((*path, "required"), "required must be an array of strings")
        if len(required) != len(set(required)):
            raise _schema_error((*path, "required"), "required names must be unique")
        if properties is None or set(required).difference(properties):
            raise _schema_error((*path, "required"), "required names must all be declared properties")

    additional = schema.get("additionalProperties")
    if additional is not None and not isinstance(additional, bool):
        raise _schema_error((*path, "additionalProperties"), "only boolean additionalProperties is supported")
    object_applicators = {"properties", "required", "additionalProperties"}.intersection(schema)
    if object_applicators and "object" not in declared_types:
        raise _schema_error(
            path,
            f"object applicator keyword(s) require an explicit object type: {sorted(object_applicators)!r}",
        )
    if "object" in declared_types:
        if properties is None or required is None or additional is not False:
            raise _schema_error(path, "object schemas require properties, required, and additionalProperties false")
        if set(required) != set(properties):
            raise _schema_error(path, "object schemas require every declared property")

    items = schema.get("items")
    if items is not None:
        _validate_schema_node(items, root=root, registered_urns=registered_urns, path=(*path, "items"))

    minimum_items = schema.get("minItems")
    maximum_items = schema.get("maxItems")
    if minimum_items is not None:
        minimum_items = _ensure_nonnegative_integer(minimum_items, (*path, "minItems"))
    if maximum_items is not None:
        maximum_items = _ensure_nonnegative_integer(maximum_items, (*path, "maxItems"))
    if minimum_items is not None and maximum_items is not None and minimum_items > maximum_items:
        raise _schema_error(path, "minItems must not exceed maxItems")
    if "uniqueItems" in schema and not isinstance(schema["uniqueItems"], bool):
        raise _schema_error((*path, "uniqueItems"), "uniqueItems must be boolean")

    pattern = schema.get("pattern")
    if pattern is not None:
        if not isinstance(pattern, str):
            raise _schema_error((*path, "pattern"), "pattern must be a string")
        try:
            re.compile(pattern)
        except re.error as error:
            raise _schema_error((*path, "pattern"), f"invalid regular expression: {error}") from error

    minimum_length = schema.get("minLength")
    maximum_length = schema.get("maxLength")
    if minimum_length is not None:
        minimum_length = _ensure_nonnegative_integer(minimum_length, (*path, "minLength"))
    if maximum_length is not None:
        maximum_length = _ensure_nonnegative_integer(maximum_length, (*path, "maxLength"))
    if minimum_length is not None and maximum_length is not None and minimum_length > maximum_length:
        raise _schema_error(path, "minLength must not exceed maxLength")

    minimum = schema.get("minimum")
    maximum = schema.get("maximum")
    if minimum is not None and not _is_number(minimum):
        raise _schema_error((*path, "minimum"), "minimum must be a finite JSON number")
    if maximum is not None and not _is_number(maximum):
        raise _schema_error((*path, "maximum"), "maximum must be a finite JSON number")
    if minimum is not None and maximum is not None and minimum > maximum:
        raise _schema_error(path, "minimum must not exceed maximum")

    for keyword in ("anyOf", "oneOf", "allOf"):
        branches = schema.get(keyword)
        if branches is None:
            continue
        if not isinstance(branches, list) or not branches:
            raise _schema_error((*path, keyword), f"{keyword} must be a non-empty schema array")
        for index, branch in enumerate(branches):
            _validate_schema_node(
                branch,
                root=root,
                registered_urns=registered_urns,
                path=(*path, keyword, index),
            )

    if "not" in schema:
        _validate_schema_node(schema["not"], root=root, registered_urns=registered_urns, path=(*path, "not"))


def _matches_type(value: object, expected: str) -> bool:
    if expected == "null":
        return value is None
    if expected == "boolean":
        return isinstance(value, bool)
    if expected == "integer":
        return not isinstance(value, bool) and isinstance(value, int)
    if expected == "number":
        return _is_number(value)
    if expected == "string":
        return isinstance(value, str)
    if expected == "array":
        return isinstance(value, list)
    if expected == "object":
        return isinstance(value, dict)
    return False


def _issue(code: str, path: tuple[str | int, ...], message: str) -> ValidationIssue:
    return ValidationIssue(code=code, path=path, message=message)


class SchemaRegistry:
    """An immutable set of preloaded schemas with no external resolver."""

    __slots__ = ("_documents_by_contract", "_documents_by_urn", "_specs")

    def __init__(
        self,
        documents_by_contract: dict[str, JsonObject],
        documents_by_urn: dict[str, JsonObject],
        specs: dict[str, ContractSpec],
    ) -> None:
        self._documents_by_contract = documents_by_contract
        self._documents_by_urn = documents_by_urn
        self._specs = specs

    @classmethod
    def from_documents(
        cls,
        documents: Iterable[Mapping[str, JsonValue]],
        *,
        specs: Iterable[ContractSpec] | None = None,
    ) -> SchemaRegistry:
        """Preload and validate local schema dictionaries without I/O."""
        copied = [deepcopy(dict(document)) for document in documents]
        by_contract: dict[str, JsonObject] = {}
        by_urn: dict[str, JsonObject] = {}
        for index, document in enumerate(copied):
            contract_schema = document.get("x-contract-schema")
            schema_urn = document.get("$id")
            if not isinstance(contract_schema, str):
                raise _schema_error((index, "x-contract-schema"), "metadata must be a string")
            if not isinstance(schema_urn, str):
                raise _schema_error((index, "$id"), "schema ID must be a string")
            if contract_schema in by_contract:
                raise ValueError(f"duplicate contract schema: {contract_schema!r}")
            if schema_urn in by_urn:
                raise ValueError(f"duplicate JSON Schema ID: {schema_urn!r}")
            by_contract[contract_schema] = document
            by_urn[schema_urn] = document

        registered_urns = frozenset(by_urn)
        for contract_schema, document in by_contract.items():
            _validate_schema_node(
                document, root=document, registered_urns=registered_urns, path=(contract_schema,), is_root=True
            )

        if specs is None:
            spec_map = {
                schema_id: ContractSpec(
                    schema_id=schema_id,
                    self_id_field=None,
                    family="ATTACHMENT",
                    schema_filename=schema_filename(schema_id),
                )
                for schema_id in by_contract
            }
        else:
            spec_values = tuple(specs)
            spec_map = {spec.schema_id: spec for spec in spec_values}
            if len(spec_map) != len(spec_values):
                raise ValueError("duplicate ContractSpec schema IDs")
            document_ids = set(by_contract)
            spec_ids = set(spec_map)
            if document_ids != spec_ids:
                missing_specs = document_ids.difference(spec_ids)
                extra_specs = spec_ids.difference(document_ids)
                raise ValueError(
                    "schema document/ContractSpec ID sets differ: "
                    f"missing={sorted(missing_specs)!r}, extra={sorted(extra_specs)!r}"
                )
            spec_map = {schema_id: spec_map[schema_id] for schema_id in by_contract}
        return cls(by_contract, by_urn, spec_map)

    @property
    def schema_ids(self) -> tuple[str, ...]:
        """Return preloaded protocol tags in insertion order."""
        return tuple(self._documents_by_contract)

    @property
    def specs(self) -> tuple[ContractSpec, ...]:
        """Return metadata for all preloaded contract schemas."""
        return tuple(self._specs.values())

    def get_schema_document(self, schema_id: str) -> JsonObject:
        """Return a defensive copy of one preloaded schema dictionary."""
        try:
            return deepcopy(self._documents_by_contract[schema_id])
        except KeyError as error:
            raise KeyError(f"unregistered contract schema: {schema_id!r}") from error

    def _resolve_reference(
        self,
        reference: str,
        root: Mapping[str, Any],
    ) -> tuple[Mapping[str, Any], Mapping[str, Any]]:
        if reference.startswith("#"):
            return _resolve_local_pointer(root, reference, ("$ref",)), root
        target = self._documents_by_urn[reference]
        return target, target

    def _validate_instance(
        self,
        schema: Mapping[str, Any],
        value: object,
        path: tuple[str | int, ...],
        root: Mapping[str, Any],
        active_refs: frozenset[tuple[int, int]],
    ) -> tuple[ValidationIssue, ...]:
        issues: list[ValidationIssue] = []
        reference = schema.get("$ref")
        if isinstance(reference, str):
            target, target_root = self._resolve_reference(reference, root)
            marker = (id(target), id(value))
            if marker not in active_refs:
                issues.extend(self._validate_instance(target, value, path, target_root, active_refs | {marker}))

        declared_type = schema.get("type")
        if declared_type is not None:
            expected_types: Sequence[str] = (declared_type,) if isinstance(declared_type, str) else declared_type
            if not any(_matches_type(value, expected) for expected in expected_types):
                return (*issues, _issue("type", path, f"expected JSON type {list(expected_types)!r}"))

        if "const" in schema and not _json_equal(value, schema["const"]):
            issues.append(_issue("const", path, "value does not equal const"))
        enum = schema.get("enum")
        if isinstance(enum, list) and not any(_json_equal(value, candidate) for candidate in enum):
            issues.append(_issue("enum", path, "value is not an enum member"))

        if isinstance(value, dict):
            required = schema.get("required")
            if isinstance(required, list):
                for name in required:
                    if name not in value:
                        issues.append(_issue("required", (*path, name), "required property is missing"))
            properties = schema.get("properties")
            if isinstance(properties, Mapping):
                for name, property_schema in properties.items():
                    if name in value:
                        issues.extend(
                            self._validate_instance(property_schema, value[name], (*path, name), root, active_refs)
                        )
                if schema.get("additionalProperties") is False:
                    for name in value.keys() - properties.keys():
                        issues.append(_issue("additionalProperties", (*path, name), "unknown property"))

        if isinstance(value, list):
            minimum_items = schema.get("minItems")
            maximum_items = schema.get("maxItems")
            if isinstance(minimum_items, int) and len(value) < minimum_items:
                issues.append(_issue("minItems", path, "array is shorter than minItems"))
            if isinstance(maximum_items, int) and len(value) > maximum_items:
                issues.append(_issue("maxItems", path, "array is longer than maxItems"))
            if schema.get("uniqueItems") is True:
                for index, item in enumerate(value):
                    if any(_json_equal(item, prior) for prior in value[:index]):
                        issues.append(_issue("uniqueItems", (*path, index), "array item is not unique"))
                        break
            item_schema = schema.get("items")
            if isinstance(item_schema, Mapping):
                for index, item in enumerate(value):
                    issues.extend(self._validate_instance(item_schema, item, (*path, index), root, active_refs))

        if isinstance(value, str):
            minimum_length = schema.get("minLength")
            maximum_length = schema.get("maxLength")
            if isinstance(minimum_length, int) and len(value) < minimum_length:
                issues.append(_issue("minLength", path, "string is shorter than minLength"))
            if isinstance(maximum_length, int) and len(value) > maximum_length:
                issues.append(_issue("maxLength", path, "string is longer than maxLength"))
            pattern = schema.get("pattern")
            if isinstance(pattern, str) and re.search(pattern, value) is None:
                issues.append(_issue("pattern", path, "string does not match pattern"))

        if _is_number(value):
            minimum = schema.get("minimum")
            maximum = schema.get("maximum")
            if _is_number(minimum) and value < minimum:
                issues.append(_issue("minimum", path, "number is below minimum"))
            if _is_number(maximum) and value > maximum:
                issues.append(_issue("maximum", path, "number is above maximum"))

        for keyword in ("anyOf", "oneOf"):
            branches = schema.get(keyword)
            if isinstance(branches, list):
                match_count = sum(
                    not self._validate_instance(branch, value, path, root, active_refs) for branch in branches
                )
                expected_matches = match_count >= 1 if keyword == "anyOf" else match_count == 1
                if not expected_matches:
                    issues.append(_issue(keyword, path, f"value does not satisfy {keyword}"))

        branches = schema.get("allOf")
        if isinstance(branches, list):
            for branch in branches:
                issues.extend(self._validate_instance(branch, value, path, root, active_refs))

        negated = schema.get("not")
        if isinstance(negated, Mapping) and not self._validate_instance(negated, value, path, root, active_refs):
            issues.append(_issue("not", path, "value satisfies forbidden schema"))
        return tuple(issues)

    def validate(
        self,
        document: Mapping[str, JsonValue],
        *,
        expected_schema: str | None = None,
    ) -> tuple[ValidationIssue, ...]:
        """Validate one document using only the preloaded local graph."""
        issues: list[ValidationIssue] = []
        declared_schema = document.get("schema")
        target_schema = expected_schema if expected_schema is not None else declared_schema
        if not isinstance(declared_schema, str):
            issues.append(_issue("schema", ("schema",), "document schema tag must be a string"))
        if expected_schema is not None and declared_schema != expected_schema:
            issues.append(_issue("expected_schema", ("schema",), "document schema tag does not match expected schema"))
        if not isinstance(target_schema, str) or target_schema not in self._documents_by_contract:
            issues.append(_issue("unknown_schema", ("schema",), f"unregistered contract schema: {target_schema!r}"))
            return tuple(issues)
        schema = self._documents_by_contract[target_schema]
        issues.extend(self._validate_instance(schema, dict(document), (), schema, frozenset()))
        return tuple(issues)


def _default_registry() -> SchemaRegistry:
    documents = get_defined_schema_documents()
    specs = (CONTRACT_SPECS_BY_SCHEMA[schema_id] for schema_id in documents)
    return SchemaRegistry.from_documents(documents.values(), specs=specs)


def get_schema_document(schema_id: str) -> JsonObject:
    """Return one explicitly defined local schema dictionary."""
    return _default_registry().get_schema_document(schema_id)


def validate_contract(
    document: Mapping[str, JsonValue],
    *,
    expected_schema: str | None = None,
    resolver: EvidenceResolver | None = None,
) -> tuple[ValidationIssue, ...]:
    """Validate local structure, then pure cross-field contract semantics."""
    del resolver
    structural_issues = _default_registry().validate(document, expected_schema=expected_schema)
    if structural_issues:
        return structural_issues
    from build_finance.crypto_replay.contract_semantics import validate_contract_semantics

    return validate_contract_semantics(document)


def require_valid_contract(
    document: Mapping[str, JsonValue],
    *,
    expected_schema: str | None = None,
    resolver: EvidenceResolver | None = None,
) -> None:
    """Raise ValueError when local structural validation reports issues."""
    issues = validate_contract(document, expected_schema=expected_schema, resolver=resolver)
    if issues:
        summary = "; ".join(f"{issue.code}@{issue.path}: {issue.message}" for issue in issues)
        raise ValueError(summary)
