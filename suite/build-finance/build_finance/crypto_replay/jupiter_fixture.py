"""Pure offline parser identity and source-field extraction for Jupiter fixtures."""

from __future__ import annotations

import ast
import hashlib
import json
from collections.abc import Mapping
from dataclasses import dataclass
from pathlib import Path

from build_finance.crypto_replay.canonical import JsonValue, canonical_json_bytes

PARSER_VERSION = "solana-jupiter-fixture-parser/v1"
_PACKAGE_PREFIX = "build_finance.crypto_replay"
_MAX_U32 = 4_294_967_295
_MAX_U64 = 18_446_744_073_709_551_615
_MAX_U64_TEXT = str(_MAX_U64)
_IDENTITY_ROOTS = (
    "build_finance.crypto_replay.local_fixture",
    "build_finance.crypto_replay.jupiter_fixture",
    "build_finance.crypto_replay.admission",
)


@dataclass(frozen=True, slots=True)
class ParserIdentity:
    """Immutable identity for the exact parser/runtime/resource closure."""

    version: str
    code_sha256: str


@dataclass(frozen=True, slots=True)
class ParsedJupiterFixture:
    """Declared Jupiter fixture identity and immutable market evidence only."""

    source_id: str
    source_kind: str
    source_revision: str
    market_id: str
    base_mint: str
    quote_mint: str
    base_decimals: int
    quote_decimals: int
    source_position_slot: str
    source_position_transaction_index: int
    source_position_instruction_index: int
    source_position_event_index: int
    source_native_event_id: str
    source_subsequence: str
    revision_kind: str
    supersedes_event_id: str | None
    retracts_event_id: str | None
    revision_availability_slot: str
    revision_availability_admission_sequence: str
    event_time: str | None
    source_position: Mapping[str, JsonValue]
    revision: Mapping[str, JsonValue]
    has_route: bool
    has_liquidity: bool
    has_fees: bool
    route_capacity_base_atoms: str | None
    liquidity_quote_atoms: str | None
    venue_fee_quote_atoms: str | None
    priority_fee_quote_atoms: str | None


class JupiterFixtureParseError(ValueError):
    """Raised when fixture bytes cannot be parsed as a local Jupiter payload."""


def current_parser_identity() -> ParserIdentity:
    """Return the deterministic digest of parser/admission runtime code and local resources."""
    bundle = {
        "parser_version": PARSER_VERSION,
        "runtime_modules": list(_runtime_module_closure(_IDENTITY_ROOTS)),
        "resources": list(_resource_closure()),
    }
    return ParserIdentity(version=PARSER_VERSION, code_sha256=hashlib.sha256(canonical_json_bytes(bundle)).hexdigest())


def parse_jupiter_fixture_payload(payload: bytes) -> ParsedJupiterFixture:
    """Extract declared Jupiter fixture fields without deriving replay event identity."""
    document = _parse_unique_json_object(payload)
    source_position = _mapping(document, "source_position")
    revision = _mapping(document, "revision")
    route = _optional_mapping(document, "route")
    liquidity = _optional_mapping(document, "liquidity")
    fees = _optional_mapping(document, "fees")

    return ParsedJupiterFixture(
        source_id=_required_str(document, "source_id"),
        source_kind=_required_str(document, "source_kind"),
        source_revision=_required_str(document, "source_revision"),
        market_id=_required_str(document, "market_id"),
        base_mint=_required_str(document, "base_mint"),
        quote_mint=_required_str(document, "quote_mint"),
        base_decimals=_required_int(document, "base_decimals"),
        quote_decimals=_required_int(document, "quote_decimals"),
        source_position_slot=_required_u64_str(source_position, "slot"),
        source_position_transaction_index=_required_u32(source_position, "transaction_index"),
        source_position_instruction_index=_required_u32(source_position, "instruction_index"),
        source_position_event_index=_required_u32(source_position, "event_index"),
        source_native_event_id=_required_str(source_position, "source_native_event_id"),
        source_subsequence=_required_u64_str(source_position, "source_subsequence"),
        revision_kind=_required_str(revision, "kind"),
        supersedes_event_id=_optional_str(revision, "supersedes_event_id"),
        retracts_event_id=_optional_str(revision, "retracts_event_id"),
        revision_availability_slot=_required_u64_str(revision, "availability_slot"),
        revision_availability_admission_sequence=_required_u64_str(revision, "availability_admission_sequence"),
        event_time=_optional_str(document, "event_time"),
        source_position=dict(source_position),
        revision=dict(revision),
        has_route=route is not None,
        has_liquidity=liquidity is not None,
        has_fees=fees is not None,
        route_capacity_base_atoms=None if route is None else _optional_str(route, "route_capacity_base_atoms"),
        liquidity_quote_atoms=None if liquidity is None else _optional_str(liquidity, "liquidity_quote_atoms"),
        venue_fee_quote_atoms=None if fees is None else _optional_str(fees, "venue_fee_quote_atoms"),
        priority_fee_quote_atoms=None if fees is None else _optional_str(fees, "priority_fee_quote_atoms"),
    )


def _runtime_module_closure(roots: tuple[str, ...]) -> tuple[dict[str, str], ...]:
    seen: set[str] = set()
    pending = list(roots)
    rows: list[dict[str, str]] = []
    while pending:
        module = pending.pop()
        if module in seen:
            continue
        seen.add(module)
        path = _module_path(module)
        payload = path.read_bytes()
        rows.append({"module": module, "sha256": hashlib.sha256(payload).hexdigest(), "byte_length": str(len(payload))})
        tree = ast.parse(payload, filename=str(path))
        for node in ast.walk(tree):
            if isinstance(node, ast.Import):
                for alias in node.names:
                    if alias.name.startswith(f"{_PACKAGE_PREFIX}."):
                        pending.append(alias.name)
            elif isinstance(node, ast.ImportFrom) and node.module and node.module.startswith(_PACKAGE_PREFIX):
                pending.append(node.module)
    return tuple(sorted(rows, key=lambda row: row["module"].encode("utf-8")))


def _resource_closure() -> tuple[dict[str, str], ...]:
    resources_root = Path(__file__).resolve().parent / "resources"
    rows: list[dict[str, str]] = []
    for path in sorted(
        resources_root.rglob("*"), key=lambda item: item.relative_to(resources_root).as_posix().encode("utf-8")
    ):
        if not path.is_file():
            continue
        payload = path.read_bytes()
        rows.append(
            {
                "relative_path": path.relative_to(resources_root).as_posix(),
                "sha256": hashlib.sha256(payload).hexdigest(),
                "byte_length": str(len(payload)),
            }
        )
    return tuple(rows)


def _module_path(module: str) -> Path:
    package_root = Path(__file__).resolve().parent
    if module == _PACKAGE_PREFIX:
        return package_root / "__init__.py"
    if not module.startswith(f"{_PACKAGE_PREFIX}."):
        raise ValueError(f"module is outside crypto_replay parser closure: {module!r}")
    relative_parts = module.removeprefix(f"{_PACKAGE_PREFIX}.").split(".")
    module_path = package_root.joinpath(*relative_parts).with_suffix(".py")
    if module_path.is_file():
        return module_path
    package_init = package_root.joinpath(*relative_parts) / "__init__.py"
    if package_init.is_file():
        return package_init
    raise FileNotFoundError(module)


def _parse_unique_json_object(payload: bytes) -> Mapping[str, JsonValue]:
    try:
        text = payload.decode("utf-8", errors="strict")
    except UnicodeDecodeError as error:
        raise JupiterFixtureParseError("payload is not strict UTF-8") from error
    try:
        document = json.loads(
            text,
            object_pairs_hook=_unique_object,
            parse_float=_reject_float,
            parse_constant=_reject_constant,
        )
    except (JupiterFixtureParseError, json.JSONDecodeError) as error:
        raise JupiterFixtureParseError("payload is not a single JSON object") from error
    if not isinstance(document, Mapping):
        raise JupiterFixtureParseError("payload root is not an object")
    return document


def _unique_object(pairs: list[tuple[str, JsonValue]]) -> dict[str, JsonValue]:
    result: dict[str, JsonValue] = {}
    for key, value in pairs:
        if key in result:
            raise JupiterFixtureParseError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def _reject_float(value: str) -> None:
    raise JupiterFixtureParseError(f"floating-point JSON number is forbidden: {value}")


def _reject_constant(value: str) -> None:
    raise JupiterFixtureParseError(f"non-finite JSON number is forbidden: {value}")


def _mapping(document: Mapping[str, JsonValue], field: str) -> Mapping[str, JsonValue]:
    value = document.get(field)
    if not isinstance(value, Mapping):
        raise JupiterFixtureParseError(f"{field} must be an object")
    return value


def _optional_mapping(document: Mapping[str, JsonValue], field: str) -> Mapping[str, JsonValue] | None:
    value = document.get(field)
    if value is None:
        return None
    if not isinstance(value, Mapping):
        raise JupiterFixtureParseError(f"{field} must be an object when present")
    return value


def _required_str(document: Mapping[str, JsonValue], field: str) -> str:
    value = document.get(field)
    if not isinstance(value, str) or value == "":
        raise JupiterFixtureParseError(f"{field} must be a non-empty string")
    return value


def _optional_str(document: Mapping[str, JsonValue], field: str) -> str | None:
    value = document.get(field)
    if value is None:
        return None
    if not isinstance(value, str):
        raise JupiterFixtureParseError(f"{field} must be a string when present")
    return value


def _required_int(document: Mapping[str, JsonValue], field: str) -> int:
    value = document.get(field)
    if isinstance(value, bool) or not isinstance(value, int):
        raise JupiterFixtureParseError(f"{field} must be an integer")
    return value


def _required_u32(document: Mapping[str, JsonValue], field: str) -> int:
    value = _required_int(document, field)
    if value < 0 or value > _MAX_U32:
        raise JupiterFixtureParseError(f"{field} must fit uint32")
    return value


def _required_u64_str(document: Mapping[str, JsonValue], field: str) -> str:
    value = _required_str(document, field)
    if not _is_canonical_u64_string(value):
        raise JupiterFixtureParseError(f"{field} must be a canonical u64 string")
    return value


def _is_canonical_u64_string(value: str) -> bool:
    if value == "0":
        return True
    if not value.isascii() or not value.isdecimal() or value.startswith("0"):
        return False
    return len(value) < len(_MAX_U64_TEXT) or (len(value) == len(_MAX_U64_TEXT) and value <= _MAX_U64_TEXT)
