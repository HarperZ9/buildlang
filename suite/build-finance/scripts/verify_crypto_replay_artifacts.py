"""Verify crypto replay wheel and sdist artifacts without trusting source files."""

from __future__ import annotations

import argparse
import hashlib
import io
import json
import re
import sys
import tarfile
import zipfile
from dataclasses import asdict, dataclass
from email.parser import Parser
from pathlib import Path, PurePosixPath
from typing import IO, Any

JsonValue = None | bool | int | str | list["JsonValue"] | dict[str, "JsonValue"]

RESOURCE_PREFIX = "build_finance/crypto_replay/resources/"
EXPECTED_LOCK_COUNTS = {
    "primary_contract_count": 8,
    "supporting_contract_count": 13,
    "json_attachment_schema_count": 27,
    "binary_formula_count": 1,
    "generated_json_schema_count": 48,
    "total_authority_contract_count": 49,
}
EXPECTED_UNCONDITIONAL_REQUIRES = frozenset({"numpy>=1.24", "pandas>=2.0", "scipy>=1.10"})
EXPECTED_JSONSCHEMA = "jsonschema>=4.23,<5"
FORBIDDEN_REQUIREMENT_NAMES = (
    "aiohttp",
    "alpaca",
    "anchorpy",
    "base58",
    "binance",
    "ccxt",
    "coinbase",
    "ibapi",
    "jupiter",
    "kraken",
    "okx",
    "requests",
    "solders",
    "solana",
    "wallet",
    "web3",
    "websocket",
    "websockets",
)
FORBIDDEN_NAME_RE = re.compile(
    r"(?i)(api[_-]?key|credential|password|private[_-]?key|secret|seed[_-]?phrase|token|wallet)"
)
SECRET_BYTE_PATTERNS = (
    re.compile(rb"jup_[A-Za-z0-9]{20,}"),
    re.compile(rb"sk-[A-Za-z0-9_-]{20,}"),
    re.compile(rb"-----BEGIN [A-Z ]*PRIVATE KEY-----"),
    re.compile(rb"(?i)(api[_-]?key|secret|token|password)\s*[:=]\s*['\"][^'\"]{12,}['\"]"),
)
SYNTHETIC_TERMS_PAYLOAD = (
    "SYNTHETIC TEST FIXTURE — NOT MARKET DATA\n"
    "These bytes are synthetic contract evidence and are never observed market data.\n"
).encode()
SYNTHETIC_DEFAULT_PAYLOAD = (
    b'{"source_id":"SYNTHETIC_TEST_ONLY_INVALID","source_kind":"solana-jupiter-quote",'
    b'"source_revision":"synthetic-revision-v1","market_id":"SYNTHETIC_BASE_MINT_0OIl_INVALID/'
    b'SYNTHETIC_QUOTE_MINT_INVALID:jupiter","base_mint":"SYNTHETIC_BASE_MINT_0OIl_INVALID",'
    b'"quote_mint":"SYNTHETIC_QUOTE_MINT_INVALID","base_decimals":6,"quote_decimals":6,'
    b'"source_position":{"slot":"1","transaction_index":0,"instruction_index":0,"event_index":0,'
    b'"source_native_event_id":"synthetic-event-0001","source_subsequence":"0"},'
    b'"revision":{"kind":"ORIGINAL","supersedes_event_id":null,"retracts_event_id":null,'
    b'"availability_slot":"1","availability_admission_sequence":"1"},'
    b'"event_time":"2026-01-01T00:00:00.000000000Z","route":{"route_capacity_base_atoms":"1000000"},'
    b'"liquidity":{"liquidity_quote_atoms":"2000000"},'
    b'"fees":{"venue_fee_quote_atoms":"100","priority_fee_quote_atoms":"7"}}'
)
SYNTHETIC_SENTINELS = (SYNTHETIC_TERMS_PAYLOAD, SYNTHETIC_DEFAULT_PAYLOAD)
MAX_ARCHIVE_MEMBER_COUNT = 2_048
MAX_ARCHIVE_MEMBER_SIZE = 16 * 1024 * 1024
MAX_ARCHIVE_TOTAL_SIZE = 64 * 1024 * 1024
ARCHIVE_READ_CHUNK_SIZE = 1024 * 1024
_STREAM_SCAN_TAIL = max(4096, *(len(sentinel) for sentinel in SYNTHETIC_SENTINELS))


class VerificationError(AssertionError):
    """Artifact contents do not satisfy the crypto replay package contract."""


@dataclass(frozen=True, slots=True)
class ArchiveMembers:
    """Normalized archive member bytes keyed by POSIX path."""

    label: str
    members: dict[str, bytes]

    def read_required(self, name: str) -> bytes:
        try:
            return self.members[name]
        except KeyError as error:
            raise VerificationError(f"{self.label} missing required member: {name}") from error

    def matching(self, prefix: str, suffix: str) -> tuple[str, ...]:
        return tuple(sorted(name for name in self.members if name.startswith(prefix) and name.endswith(suffix)))


@dataclass(frozen=True, slots=True)
class VerificationReport:
    """Machine-readable summary of the verified artifact pair."""

    wheel: str
    wheel_sha256: str
    wheel_member_count: int
    sdist: str
    sdist_sha256: str
    sdist_member_count: int
    primary_bundle_sha256: str
    supporting_bundle_sha256: str
    attachment_bundle_sha256: str
    binary_formula_bundle_sha256: str
    schema_bundle_sha256: str
    generated_json_schema_count: int
    total_authority_contract_count: int


def verify_artifacts(*, wheel: Path, sdist: Path) -> VerificationReport:
    """Assert that one wheel and one sdist preserve offline replay package boundaries."""

    wheel_members = _read_wheel_members(wheel)
    sdist_members = _read_sdist_members(sdist)
    _scan_archive_exclusions(wheel_members, allow_tests_and_docs=False)
    _scan_archive_exclusions(sdist_members, allow_tests_and_docs=True)
    metadata = _verify_metadata(wheel_members)
    _verify_metadata_requirements(metadata)
    lock = _verify_wheel_resources(wheel_members)
    _verify_sdist_exclusions(sdist_members)
    return VerificationReport(
        wheel=str(wheel),
        wheel_sha256=_file_sha256(wheel),
        wheel_member_count=len(wheel_members.members),
        sdist=str(sdist),
        sdist_sha256=_file_sha256(sdist),
        sdist_member_count=len(sdist_members.members),
        primary_bundle_sha256=str(lock["primary_bundle_sha256"]),
        supporting_bundle_sha256=str(lock["supporting_bundle_sha256"]),
        attachment_bundle_sha256=str(lock["attachment_bundle_sha256"]),
        binary_formula_bundle_sha256=str(lock["binary_formula_bundle_sha256"]),
        schema_bundle_sha256=str(lock["schema_bundle_sha256"]),
        generated_json_schema_count=_require_int(lock["generated_json_schema_count"], "generated_json_schema_count"),
        total_authority_contract_count=_require_int(
            lock["total_authority_contract_count"], "total_authority_contract_count"
        ),
    )


def _read_wheel_members(path: Path) -> ArchiveMembers:
    if not path.is_file() or path.suffix != ".whl":
        raise VerificationError(f"wheel path is not a .whl file: {path}")
    members: dict[str, bytes] = {}
    aggregate_size = 0
    with zipfile.ZipFile(path) as archive:
        infos = archive.infolist()
        if len(infos) > MAX_ARCHIVE_MEMBER_COUNT:
            raise VerificationError(f"wheel member count exceeds {MAX_ARCHIVE_MEMBER_COUNT}: {len(infos)}")
        for info in infos:
            if info.is_dir():
                continue
            name = _normalize_member_name(info.filename)
            _reject_env_member(name, "wheel")
            if name in members:
                raise VerificationError(f"wheel duplicate member after normalization: {name}")
            aggregate_size = _check_declared_member_size(
                "wheel",
                name,
                info.file_size,
                aggregate_size,
            )
            with archive.open(info, "r") as fileobj:
                members[name] = _read_bounded_member(fileobj, expected_size=info.file_size, label="wheel", name=name)
    return ArchiveMembers("wheel", members)


def _read_sdist_members(path: Path) -> ArchiveMembers:
    if not path.is_file() or not path.name.endswith(".tar.gz"):
        raise VerificationError(f"sdist path is not a .tar.gz file: {path}")
    members: dict[str, bytes] = {}
    member_count = 0
    aggregate_size = 0
    with tarfile.open(path, "r:gz") as archive:
        for info in archive:
            member_count += 1
            if member_count > MAX_ARCHIVE_MEMBER_COUNT:
                raise VerificationError(f"sdist member count exceeds {MAX_ARCHIVE_MEMBER_COUNT}: {member_count}")
            raw_name = _normalize_member_name(info.name)
            _reject_env_member(raw_name, "sdist")
            if info.isdir():
                continue
            if not info.isfile():
                raise VerificationError(f"sdist contains non-regular member: {raw_name}")
            name = _strip_sdist_root(raw_name)
            if name in members:
                raise VerificationError(f"sdist duplicate member after normalization: {name}")
            aggregate_size = _check_declared_member_size("sdist", name, info.size, aggregate_size)
            fileobj = archive.extractfile(info)
            if fileobj is None:
                raise VerificationError(f"sdist member could not be read: {raw_name}")
            with fileobj:
                members[name] = _read_bounded_member(fileobj, expected_size=info.size, label="sdist", name=name)
    return ArchiveMembers("sdist", members)


def _check_declared_member_size(label: str, name: str, size: int, aggregate_size: int) -> int:
    if not isinstance(size, int) or size < 0:
        raise VerificationError(f"{label} member has invalid declared size: {name} size={size!r}")
    if size > MAX_ARCHIVE_MEMBER_SIZE:
        raise VerificationError(
            f"{label} member exceeds maximum uncompressed size: {name} size={size} max={MAX_ARCHIVE_MEMBER_SIZE}"
        )
    next_aggregate = aggregate_size + size
    if next_aggregate > MAX_ARCHIVE_TOTAL_SIZE:
        raise VerificationError(
            f"{label} aggregate uncompressed size exceeds {MAX_ARCHIVE_TOTAL_SIZE}: {next_aggregate}"
        )
    return next_aggregate


def _read_bounded_member(fileobj: IO[bytes], *, expected_size: int, label: str, name: str) -> bytes:
    chunks: list[bytes] = []
    total_size = 0
    tail = b""
    while True:
        chunk = fileobj.read(ARCHIVE_READ_CHUNK_SIZE)
        if not chunk:
            break
        total_size += len(chunk)
        if total_size > expected_size:
            raise VerificationError(
                f"{label} member decompressed size exceeds declared size: {name} declared={expected_size} read>{total_size}"
            )
        _scan_payload_bytes(label, name, tail + chunk)
        tail = (tail + chunk)[-_STREAM_SCAN_TAIL:]
        chunks.append(chunk)
    if total_size != expected_size:
        raise VerificationError(f"{label} member size mismatch: {name} declared={expected_size} read={total_size}")
    return b"".join(chunks)


def _scan_payload_bytes(label: str, name: str, payload: bytes) -> None:
    for pattern in SECRET_BYTE_PATTERNS:
        if pattern.search(payload):
            raise VerificationError(f"{label} contains credential-shaped content: {name}")
    for sentinel in SYNTHETIC_SENTINELS:
        if sentinel in payload:
            raise VerificationError(f"{label} contains synthetic fixture sentinel bytes: {name}")


def _normalize_member_name(name: str) -> str:
    normalized = name.replace("\\", "/")
    if normalized.startswith("/") or normalized.endswith("/"):
        raise VerificationError(f"unsafe archive member path: {name!r}")
    parts = PurePosixPath(normalized).parts
    if not parts or any(part in ("", ".", "..") for part in parts):
        raise VerificationError(f"unsafe archive member path: {name!r}")
    return "/".join(parts)


def _strip_sdist_root(name: str) -> str:
    parts = PurePosixPath(name).parts
    if len(parts) < 2:
        raise VerificationError(f"sdist member lacks project root prefix: {name}")
    return "/".join(parts[1:])


def _reject_env_member(name: str, label: str) -> None:
    if any(part == ".env" or part.startswith(".env.") for part in PurePosixPath(name).parts):
        raise VerificationError(f"{label} contains .env-named member: {name}")


def _scan_archive_exclusions(archive: ArchiveMembers, *, allow_tests_and_docs: bool) -> None:
    for name, payload in archive.members.items():
        parts = PurePosixPath(name).parts
        if not allow_tests_and_docs and (parts[0] in {"docs", "tests"} or "evidence" in parts):
            raise VerificationError(f"{archive.label} contains docs/tests/evidence member: {name}")
        if any(part in {"payloads", "terms", "witnesses"} for part in parts):
            raise VerificationError(f"{archive.label} contains generated local fixture directory: {name}")
        if FORBIDDEN_NAME_RE.search(name):
            raise VerificationError(f"{archive.label} contains credential-shaped member name: {name}")
        for pattern in SECRET_BYTE_PATTERNS:
            if pattern.search(payload):
                raise VerificationError(f"{archive.label} contains credential-shaped content: {name}")
        for sentinel in SYNTHETIC_SENTINELS:
            if sentinel in payload:
                raise VerificationError(f"{archive.label} contains synthetic fixture sentinel bytes: {name}")


def _verify_metadata(wheel: ArchiveMembers) -> str:
    metadata_paths = tuple(name for name in wheel.members if name.endswith(".dist-info/METADATA") and "/" in name)
    if len(metadata_paths) != 1:
        raise VerificationError(f"wheel must contain exactly one METADATA file, found {metadata_paths!r}")
    return wheel.read_required(metadata_paths[0]).decode("utf-8", errors="strict")


def _verify_metadata_requirements(metadata_text: str) -> None:
    metadata = Parser().parsestr(metadata_text)
    requires = metadata.get_all("Requires-Dist") or []
    unconditional = frozenset(_normalize_requirement(req) for req in requires if ";" not in req)
    if unconditional != EXPECTED_UNCONDITIONAL_REQUIRES:
        raise VerificationError(f"unexpected unconditional Requires-Dist set: {sorted(unconditional)!r}")
    for req in requires:
        requirement = _normalize_requirement(req)
        requirement_name = _requirement_name(requirement)
        if any(requirement_name == forbidden for forbidden in FORBIDDEN_REQUIREMENT_NAMES):
            raise VerificationError(f"forbidden provider/network/broker/wallet requirement: {req}")
        if requirement_name == "jsonschema":
            base, marker = _split_marker(requirement)
            if base != EXPECTED_JSONSCHEMA or marker not in {'extra=="dev"', 'extra=="test"'}:
                raise VerificationError(f"jsonschema is not restricted to test/dev extras: {req}")
    if not any(_normalize_requirement(req) == f'{EXPECTED_JSONSCHEMA};extra=="test"' for req in requires):
        raise VerificationError("jsonschema test extra missing from wheel metadata")
    if not any(_normalize_requirement(req) == f'{EXPECTED_JSONSCHEMA};extra=="dev"' for req in requires):
        raise VerificationError("jsonschema dev extra missing from wheel metadata")


def _normalize_requirement(requirement: str) -> str:
    compact = "".join(requirement.strip().lower().replace("'", '"').split())
    base, marker = _split_marker(compact)
    normalized_base = _normalize_requirement_base(base)
    return f"{normalized_base};{marker}" if marker else normalized_base


def _normalize_requirement_base(requirement_base: str) -> str:
    match = re.match(r"([a-z0-9_.-]+)(.*)", requirement_base)
    if match is None:
        raise VerificationError(f"cannot parse requirement: {requirement_base!r}")
    name = match.group(1).replace("_", "-")
    specifier_text = match.group(2)
    if "," not in specifier_text:
        return f"{name}{specifier_text}"
    specifiers = sorted(
        specifier_text.split(","),
        key=_specifier_sort_key,
    )
    return f"{name}{','.join(specifiers)}"


def _specifier_sort_key(specifier: str) -> tuple[int, str]:
    for index, operator in enumerate(("==", ">=", ">", "~=", "!=", "<", "<=")):
        if specifier.startswith(operator):
            return index, specifier.removeprefix(operator)
    return 99, specifier


def _split_marker(requirement: str) -> tuple[str, str]:
    base, marker = requirement.split(";", 1) if ";" in requirement else (requirement, "")
    return base, marker


def _requirement_name(requirement: str) -> str:
    base, _marker = _split_marker(requirement)
    match = re.match(r"([a-z0-9_.-]+)", base)
    if match is None:
        raise VerificationError(f"cannot parse requirement name: {requirement!r}")
    return match.group(1).replace("_", "-")


def _verify_wheel_resources(wheel: ArchiveMembers) -> dict[str, JsonValue]:
    primary_bundle = _parse_canonical_member(wheel, "primary-schema-bundle.json")
    primary_digest = _read_digest_member(wheel, "primary-schema-bundle.sha256")
    if primary_digest != _sha256(_canonical_json_bytes(primary_bundle)):
        raise VerificationError("primary bundle digest does not match primary bundle payload")

    schema_bundle = _parse_canonical_member(wheel, "schema-bundle.json")
    schema_bundle_digest = _read_digest_member(wheel, "schema-bundle.sha256")
    if schema_bundle_digest != _sha256(_canonical_json_bytes(schema_bundle)):
        raise VerificationError("schema bundle digest does not match final bundle payload")

    lock = _parse_canonical_member(wheel, "schema-lock.json")
    for field, expected in EXPECTED_LOCK_COUNTS.items():
        if lock.get(field) != expected:
            raise VerificationError(f"schema lock {field}={lock.get(field)!r}, expected {expected!r}")
    if lock.get("primary_bundle_sha256") != primary_digest:
        raise VerificationError("schema lock does not bind sealed primary bundle digest")
    if lock.get("schema_bundle_sha256") != schema_bundle_digest:
        raise VerificationError("schema lock does not bind final schema bundle digest")
    if schema_bundle.get("primary_bundle_sha256") != primary_digest:
        raise VerificationError("final bundle does not bind sealed primary bundle digest")

    schema_rows = _require_list(schema_bundle.get("schemas"), "schema-bundle schemas")
    formula_rows = _require_list(schema_bundle.get("binary_formulas"), "schema-bundle binary_formulas")
    primary_rows = [row for row in schema_rows if _require_mapping(row, "schema row").get("family") == "PRIMARY"]
    supporting_rows = [row for row in schema_rows if _require_mapping(row, "schema row").get("family") == "SUPPORTING"]
    attachment_rows = [row for row in schema_rows if _require_mapping(row, "schema row").get("family") == "ATTACHMENT"]
    if len(primary_rows) != lock["primary_contract_count"]:
        raise VerificationError("primary schema row count does not match lock")
    if len(supporting_rows) != lock["supporting_contract_count"]:
        raise VerificationError("supporting schema row count does not match lock")
    if len(attachment_rows) != lock["json_attachment_schema_count"]:
        raise VerificationError("attachment schema row count does not match lock")
    if len(schema_rows) != lock["generated_json_schema_count"]:
        raise VerificationError("generated schema row count does not match lock")
    if len(formula_rows) != lock["binary_formula_count"]:
        raise VerificationError("binary formula row count does not match lock")
    if len(schema_rows) + len(formula_rows) != lock["total_authority_contract_count"]:
        raise VerificationError("total authority contract count does not match rows")
    if _canonical_json_bytes(primary_rows) != _canonical_json_bytes(
        _require_list(primary_bundle.get("schemas"), "primary schemas")
    ):
        raise VerificationError("primary rows in final bundle differ from sealed primary bundle")

    if _sha256(_canonical_json_bytes(supporting_rows)) != lock["supporting_bundle_sha256"]:
        raise VerificationError("supporting family digest mismatch")
    if _sha256(_canonical_json_bytes(attachment_rows)) != lock["attachment_bundle_sha256"]:
        raise VerificationError("attachment family digest mismatch")
    if _sha256(_canonical_json_bytes(formula_rows)) != lock["binary_formula_bundle_sha256"]:
        raise VerificationError("binary formula family digest mismatch")

    expected_schema_members: set[str] = set()
    for row_value in schema_rows:
        row = _require_mapping(row_value, "schema row")
        resource = _schema_resource_for_contract(str(row["contract_schema"]))
        expected_schema_members.add(resource)
        schema_document = _parse_canonical_member(wheel, resource)
        if _sha256(_canonical_json_bytes(schema_document)) != row["schema_sha256"]:
            raise VerificationError(f"schema digest mismatch for {resource}")
        if schema_document.get("$id") != row["json_schema_id"]:
            raise VerificationError(f"schema $id mismatch for {resource}")
        if schema_document.get("x-contract-schema") != row["contract_schema"]:
            raise VerificationError(f"schema contract marker mismatch for {resource}")

    actual_schema_members = {
        name.removeprefix(RESOURCE_PREFIX) for name in wheel.matching(f"{RESOURCE_PREFIX}schemas/", ".schema.json")
    }
    if actual_schema_members != expected_schema_members:
        raise VerificationError(
            "wheel schema resource census mismatch: "
            f"missing={sorted(expected_schema_members - actual_schema_members)!r} "
            f"extra={sorted(actual_schema_members - expected_schema_members)!r}"
        )

    for formula_value in formula_rows:
        row = _require_mapping(formula_value, "formula row")
        resource = str(row["formula_resource"])
        formula_payload = wheel.read_required(f"{RESOURCE_PREFIX}{resource}")
        if _sha256(formula_payload) != row["formula_sha256"]:
            raise VerificationError(f"formula digest mismatch for {resource}")

    return lock


def _verify_sdist_exclusions(sdist: ArchiveMembers) -> None:
    generated_fixture_roots = (
        "SYNTHETIC-local-fixture",
        "synthetic-local-fixture",
        "local-fixture",
        "local_fixture",
        "fixture-root",
    )
    for name in sdist.members:
        parts = PurePosixPath(name).parts
        lowered_parts = tuple(part.lower() for part in parts)
        if any(root.lower() in lowered_parts for root in generated_fixture_roots):
            raise VerificationError(f"sdist contains generated fixture root: {name}")


def _parse_canonical_member(archive: ArchiveMembers, resource: str) -> dict[str, JsonValue]:
    payload = archive.read_required(f"{RESOURCE_PREFIX}{resource}")
    return _parse_canonical_record(payload, f"{archive.label}:{resource}")


def _read_digest_member(archive: ArchiveMembers, resource: str) -> str:
    payload = archive.read_required(f"{RESOURCE_PREFIX}{resource}")
    if len(payload) != 65 or not payload.endswith(b"\n"):
        raise VerificationError(f"{archive.label}:{resource} is not one SHA-256 digest plus LF")
    digest = payload[:-1].decode("ascii", errors="strict")
    if not re.fullmatch(r"[0-9a-f]{64}", digest):
        raise VerificationError(f"{archive.label}:{resource} is not lowercase SHA-256")
    return digest


def _parse_canonical_record(payload: bytes, label: str) -> dict[str, JsonValue]:
    if not payload.endswith(b"\n"):
        raise VerificationError(f"{label} is not one LF-terminated canonical JSON record")
    body = payload[:-1]
    try:
        value = json.loads(body.decode("utf-8", errors="strict"), parse_float=_reject_float)
    except (UnicodeDecodeError, ValueError, json.JSONDecodeError) as error:
        raise VerificationError(f"{label} is not strict canonical JSON") from error
    if not isinstance(value, dict):
        raise VerificationError(f"{label} JSON root is not an object")
    if _canonical_json_bytes(value) != body:
        raise VerificationError(f"{label} is not canonical JSON")
    return value


def _reject_float(value: str) -> None:
    raise ValueError(f"floats are not part of the replay canonical JSON profile: {value}")


def _canonical_json_bytes(value: JsonValue) -> bytes:
    if value is None:
        return b"null"
    if value is True:
        return b"true"
    if value is False:
        return b"false"
    if isinstance(value, int):
        return str(value).encode("ascii")
    if isinstance(value, str):
        return json.dumps(value, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
    if isinstance(value, list):
        return b"[" + b",".join(_canonical_json_bytes(item) for item in value) + b"]"
    if isinstance(value, dict):
        if not all(isinstance(key, str) for key in value):
            raise VerificationError("canonical JSON object contains a non-string key")
        parts = []
        for key in sorted(value):
            parts.append(_canonical_json_bytes(key) + b":" + _canonical_json_bytes(value[key]))
        return b"{" + b",".join(parts) + b"}"
    raise VerificationError(f"unsupported canonical JSON value: {value!r}")


def _schema_resource_for_contract(contract_schema: str) -> str:
    if not contract_schema.startswith("trading.") or not contract_schema.endswith("/v1"):
        raise VerificationError(f"unexpected contract schema id: {contract_schema}")
    body = contract_schema.removeprefix("trading.").replace("/", "-")
    return f"schemas/{body}.schema.json"


def _require_list(value: Any, label: str) -> list[JsonValue]:
    if not isinstance(value, list):
        raise VerificationError(f"{label} is not a list")
    return value


def _require_mapping(value: JsonValue, label: str) -> dict[str, JsonValue]:
    if not isinstance(value, dict):
        raise VerificationError(f"{label} is not an object")
    return value


def _require_int(value: JsonValue, label: str) -> int:
    if isinstance(value, bool) or not isinstance(value, int):
        raise VerificationError(f"{label} is not an integer")
    return value


def _sha256(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()


def _file_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(io.DEFAULT_BUFFER_SIZE), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--wheel", required=True, type=Path)
    parser.add_argument("--sdist", required=True, type=Path)
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = _parse_args(sys.argv[1:] if argv is None else argv)
    try:
        report = verify_artifacts(wheel=args.wheel, sdist=args.sdist)
    except VerificationError as error:
        print(f"crypto replay artifact verification failed: {error}", file=sys.stderr)
        return 1
    print(json.dumps(asdict(report), sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
