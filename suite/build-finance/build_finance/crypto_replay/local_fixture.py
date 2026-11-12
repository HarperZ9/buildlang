"""No-follow local fixture capture for offline crypto replay admission."""

from __future__ import annotations

import hashlib
import os
import stat
import unicodedata
from collections.abc import Mapping
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from build_finance.crypto_replay.canonical import (
    JsonObject,
    JsonValue,
    canonical_json_bytes,
    parse_canonical_record,
    sha256_hex,
)

try:
    from build_finance.crypto_replay.schema_model import ValidationIssue
except ImportError:  # pragma: no cover - compatibility for isolated downstream vendoring.

    @dataclass(frozen=True, slots=True)
    class ValidationIssue:  # type: ignore[no-redef]
        """One deterministic capture issue when the shared schema model is unavailable."""

        code: str
        path: tuple[str | int, ...]
        message: str


_FILE_ATTRIBUTE_REPARSE_POINT = 0x400
_READ_CHUNK_SIZE = 1024 * 1024
_MAX_U64 = 18_446_744_073_709_551_615
_MAX_U64_TEXT = str(_MAX_U64)
_O_BINARY = int(vars(os).get("O_BINARY", 0))
_O_CLOEXEC = int(vars(os).get("O_CLOEXEC", 0))
_O_DIRECTORY = int(vars(os).get("O_DIRECTORY", 0))
_O_NOFOLLOW = int(vars(os).get("O_NOFOLLOW", 0))
_CURRENT_MANIFEST_PATH = "manifest.json"
_PLAN_MANIFEST_PATH = "fixture-manifest.json"
_CURRENT_RIGHTS_PATH = "rights/rights-manifest.json"
_PLAN_RIGHTS_PATH = "rights-manifest.json"
_OPEN_SUPPORTS_DIR_FD = os.open in getattr(os, "supports_dir_fd", set())
_STAT_SUPPORTS_DIR_FD = os.stat in getattr(os, "supports_dir_fd", set())
_STAT_SUPPORTS_NOFOLLOW = os.stat in getattr(os, "supports_follow_symlinks", set())


@dataclass(frozen=True, slots=True)
class CapturedTerm:
    """Exact local terms bytes named by rights evidence."""

    relative_path: str | None
    expected_sha256: str | None
    payload: bytes | None
    sha256: str | None
    byte_length: int | None
    issues: tuple[ValidationIssue, ...] = ()


@dataclass(frozen=True, slots=True)
class CapturedWitness:
    """Canonical local witness evidence for one manifest row."""

    admission_sequence: str
    relative_path: str
    witness_relative_path: str | None
    record: Mapping[str, JsonValue] | None
    payload: bytes | None
    sha256: str | None
    observed_at: str | None
    ingested_at: str | None
    issues: tuple[ValidationIssue, ...] = ()


@dataclass(frozen=True, slots=True)
class CapturedFile:
    """Exact captured payload bytes and all manifest/witness evidence needed by admission."""

    admission_sequence: str
    relative_path: str
    payload: bytes | None
    sha256: str | None
    byte_length: int | None
    observed_at: str | None
    ingested_at: str | None
    expected_sha256: str | None = None
    expected_byte_length: str | None = None
    availability_slot: str | None = None
    media_type: str | None = None
    source_id: str | None = None
    source_kind: str | None = None
    source_revision: str | None = None
    market_id: str | None = None
    witness: CapturedWitness | None = None
    issues: tuple[ValidationIssue, ...] = ()


@dataclass(frozen=True, slots=True)
class CapturedFixture:
    """Total local fixture capture result.

    Ordinary fixture defects are reported in ``capture_issues``. The capture API
    raises only for invalid caller API types.
    """

    manifest: Mapping[str, JsonValue] | None
    manifest_payload: bytes | None
    rights_manifest: Mapping[str, JsonValue] | None
    rights_manifest_payload: bytes | None
    terms_by_sha256: Mapping[str, bytes]
    files: tuple[CapturedFile, ...]
    capture_issues: tuple[ValidationIssue, ...]
    root: Path | None = None
    root_path: str | None = None
    manifest_sha256: str | None = None
    fixture_manifest_sha256: str | None = None
    rights_manifest_sha256: str | None = None
    rights_manifest_payload_sha256: str | None = None
    terms: tuple[CapturedTerm, ...] = ()
    witnesses: tuple[CapturedWitness, ...] = ()


@dataclass(frozen=True, slots=True)
class _FileIdentity:
    device: int
    inode: int
    mode_type: int
    size: int
    modified_ns: int


@dataclass(frozen=True, slots=True)
class _PrefixIdentity:
    relative_path: str
    identity: _FileIdentity


@dataclass(frozen=True, slots=True)
class _OpenedLocalFile:
    fd: int
    absolute_path: str
    before_identity: _FileIdentity
    prefix_identities: tuple[_PrefixIdentity, ...]


@dataclass(frozen=True, slots=True)
class _ReadResult:
    relative_path: str
    payload: bytes | None
    sha256: str | None
    byte_length: int | None
    issues: tuple[ValidationIssue, ...]


@dataclass(frozen=True, slots=True)
class _TermRequest:
    relative_path: str | None
    expected_sha256: str | None
    source_id: str | None


def capture_local_fixture(root: Path) -> CapturedFixture:
    """Capture an explicit local fixture root without following links or repairing defects."""
    if not isinstance(root, Path):
        raise TypeError("capture_local_fixture(root) requires an explicit pathlib.Path")

    root_path = os.path.abspath(os.fspath(root))
    issues: list[ValidationIssue] = []
    root_identity = _capture_root_identity(root_path, issues)
    if root_identity is None:
        return CapturedFixture(
            manifest=None,
            manifest_payload=None,
            rights_manifest=None,
            rights_manifest_payload=None,
            terms_by_sha256={},
            files=(),
            capture_issues=tuple(issues),
            root=root,
            root_path=root_path,
        )

    manifest_result = _read_canonical_layout_file(
        root_path,
        root_identity,
        (_CURRENT_MANIFEST_PATH, _PLAN_MANIFEST_PATH),
        ("manifest",),
        missing_code="ADMISSION_MANIFEST_MISMATCH",
    )
    issues.extend(manifest_result.issues)
    manifest = _parse_canonical_manifest(manifest_result, issues)
    manifest_sha256 = sha256_hex(manifest_result.payload) if manifest_result.payload is not None else None
    fixture_manifest_sha256 = _fixture_manifest_sha256(manifest, issues)

    rights_result = _read_canonical_layout_file(
        root_path,
        root_identity,
        (_CURRENT_RIGHTS_PATH, _PLAN_RIGHTS_PATH),
        ("rights_manifest",),
        missing_code="ADMISSION_RIGHTS_MISSING",
    )
    issues.extend(rights_result.issues)
    rights_manifest = _parse_canonical_rights(rights_result, issues)
    rights_manifest_sha256 = sha256_hex(rights_result.payload) if rights_result.payload is not None else None
    rights_manifest_payload_sha256 = rights_manifest_sha256

    if manifest is not None:
        _check_manifest_self_digest(manifest, issues)
        _check_rights_digest(manifest, rights_manifest_sha256, issues)

    source_ids = _manifest_source_ids(manifest)
    terms = _capture_terms(root_path, root_identity, rights_manifest, source_ids, issues)
    terms_by_sha256 = {
        term.sha256: term.payload for term in terms if term.sha256 is not None and term.payload is not None
    }

    files, witnesses = _capture_manifest_files(root_path, root_identity, manifest, issues)
    _capture_extra_payload_entries(root_path, root_identity, {file.relative_path for file in files}, issues)
    _check_root_stable(root_path, root_identity, issues, ("root", "post_capture"))

    return CapturedFixture(
        manifest=manifest,
        manifest_payload=manifest_result.payload,
        rights_manifest=rights_manifest,
        rights_manifest_payload=rights_result.payload,
        terms_by_sha256=terms_by_sha256,
        files=files,
        capture_issues=tuple(issues),
        root=root,
        root_path=root_path,
        manifest_sha256=manifest_sha256,
        fixture_manifest_sha256=fixture_manifest_sha256,
        rights_manifest_sha256=rights_manifest_sha256,
        rights_manifest_payload_sha256=rights_manifest_payload_sha256,
        terms=terms,
        witnesses=witnesses,
    )


def _capture_root_identity(root_path: str, issues: list[ValidationIssue]) -> _FileIdentity | None:
    try:
        metadata = os.lstat(root_path)
    except OSError as error:
        issues.append(_issue("ADMISSION_NOT_LOCAL", ("root",), f"fixture root cannot be classified: {error}"))
        return None
    if _is_link_or_reparse(metadata):
        issues.append(_issue("ADMISSION_NOT_LOCAL", ("root",), "fixture root is a symlink, junction, or reparse point"))
        return None
    if not stat.S_ISDIR(metadata.st_mode):
        issues.append(_issue("ADMISSION_NOT_LOCAL", ("root",), "fixture root is not a directory"))
        return None
    return _identity(metadata)


def _read_canonical_layout_file(
    root_path: str,
    root_identity: _FileIdentity,
    candidates: tuple[str, ...],
    issue_path: tuple[str | int, ...],
    *,
    missing_code: str,
) -> _ReadResult:
    selected = _select_layout_path(root_path, candidates, issue_path, missing_code)
    return _read_result_with_issues(
        _read_local_file(root_path, root_identity, selected.relative_path, issue_path, missing_code=missing_code),
        selected.issues,
    )


@dataclass(frozen=True, slots=True)
class _SelectedPath:
    relative_path: str
    issues: tuple[ValidationIssue, ...]


def _select_layout_path(
    root_path: str,
    candidates: tuple[str, ...],
    issue_path: tuple[str | int, ...],
    missing_code: str,
) -> _SelectedPath:
    existing: list[str] = []
    selection_issues: list[ValidationIssue] = []
    for candidate in candidates:
        absolute = _join_checked(root_path, candidate, issue_path, selection_issues, missing_code)
        if absolute is None:
            continue
        try:
            os.lstat(absolute)
        except FileNotFoundError:
            continue
        except OSError:
            existing.append(candidate)
        else:
            existing.append(candidate)
    if len(existing) > 1:
        selection_issues.append(
            _issue(
                "ADMISSION_SET_NOT_CLOSED",
                issue_path,
                "both current and plan-compatible fixture layout paths exist; capture selected the current layout",
            )
        )
    return _SelectedPath(existing[0] if existing else candidates[0], tuple(selection_issues))


def _parse_canonical_manifest(result: _ReadResult, issues: list[ValidationIssue]) -> JsonObject | None:
    if result.payload is None:
        return None
    try:
        return parse_canonical_record(result.payload)
    except ValueError as error:
        issues.append(
            _issue("ADMISSION_MANIFEST_MISMATCH", ("manifest",), f"manifest is not a canonical LF record: {error}")
        )
        return None


def _parse_canonical_rights(result: _ReadResult, issues: list[ValidationIssue]) -> JsonObject | None:
    if result.payload is None:
        return None
    try:
        return parse_canonical_record(result.payload)
    except ValueError as error:
        issues.append(
            _issue(
                "ADMISSION_RIGHTS_MISSING",
                ("rights_manifest",),
                f"rights manifest is not a canonical LF record: {error}",
            )
        )
        return None


def _fixture_manifest_sha256(manifest: Mapping[str, JsonValue] | None, issues: list[ValidationIssue]) -> str | None:
    if manifest is None:
        return None
    try:
        body = dict(manifest)
        body.pop("fixture_manifest_sha256", None)
        return sha256_hex(canonical_json_bytes(body))
    except ValueError as error:
        issues.append(_issue("ADMISSION_MANIFEST_MISMATCH", ("manifest", "fixture_manifest_sha256"), str(error)))
        return None


def _check_manifest_self_digest(manifest: Mapping[str, JsonValue], issues: list[ValidationIssue]) -> None:
    supplied = manifest.get("fixture_manifest_sha256")
    computed = _fixture_manifest_sha256(manifest, issues)
    if isinstance(supplied, str) and computed is not None and supplied != computed:
        issues.append(
            _issue(
                "ADMISSION_MANIFEST_MISMATCH",
                ("manifest", "fixture_manifest_sha256"),
                "fixture manifest self digest does not match canonical body",
            )
        )


def _check_rights_digest(
    manifest: Mapping[str, JsonValue],
    record_sha256: str | None,
    issues: list[ValidationIssue],
) -> None:
    expected = manifest.get("rights_manifest_sha256")
    if not isinstance(expected, str):
        issues.append(
            _issue("ADMISSION_MANIFEST_MISMATCH", ("manifest", "rights_manifest_sha256"), "missing rights digest")
        )
        return
    if record_sha256 is None:
        return
    if expected != record_sha256:
        issues.append(
            _issue(
                "ADMISSION_MANIFEST_MISMATCH",
                ("manifest", "rights_manifest_sha256"),
                "rights manifest digest does not match captured canonical rights bytes",
            )
        )


def _manifest_source_ids(manifest: Mapping[str, JsonValue] | None) -> set[str]:
    source_ids: set[str] = set()
    if manifest is None:
        return source_ids
    files = manifest.get("files")
    if not isinstance(files, list):
        return source_ids
    for row in files:
        if isinstance(row, Mapping) and isinstance(row.get("source_id"), str):
            source_ids.add(row["source_id"])
    return source_ids


def _capture_terms(
    root_path: str,
    root_identity: _FileIdentity,
    rights_manifest: Mapping[str, JsonValue] | None,
    manifest_source_ids: set[str],
    issues: list[ValidationIssue],
) -> tuple[CapturedTerm, ...]:
    if rights_manifest is None:
        return ()
    requests = _term_requests(rights_manifest, manifest_source_ids, issues)
    terms: list[CapturedTerm] = []
    for index, request in enumerate(requests):
        term_issues: list[ValidationIssue] = []
        relative_path = request.relative_path
        if relative_path is None and _is_sha256_string(request.expected_sha256):
            relative_path = f"terms/{request.expected_sha256}.bin"
        if relative_path is None:
            term_issues.append(_issue("ADMISSION_RIGHTS_MISSING", ("terms", index), "terms row has no readable path"))
            issues.extend(term_issues)
            terms.append(CapturedTerm(None, request.expected_sha256, None, None, None, tuple(term_issues)))
            continue
        result = _read_local_file(
            root_path,
            root_identity,
            relative_path,
            ("terms", index),
            missing_code="ADMISSION_RIGHTS_MISSING",
        )
        term_issues.extend(result.issues)
        if (
            result.sha256 is not None
            and request.expected_sha256 is not None
            and result.sha256 != request.expected_sha256
        ):
            term_issues.append(
                _issue(
                    "ADMISSION_RIGHTS_MISSING",
                    ("terms", index, "terms_sha256"),
                    "terms digest does not match rights row",
                )
            )
        issues.extend(term_issues)
        terms.append(
            CapturedTerm(
                relative_path=relative_path,
                expected_sha256=request.expected_sha256,
                payload=result.payload,
                sha256=result.sha256,
                byte_length=result.byte_length,
                issues=tuple(term_issues),
            )
        )
    return tuple(terms)


def _term_requests(
    rights_manifest: Mapping[str, JsonValue],
    manifest_source_ids: set[str],
    issues: list[ValidationIssue],
) -> tuple[_TermRequest, ...]:
    current_terms = rights_manifest.get("terms")
    if isinstance(current_terms, list):
        requests: list[_TermRequest] = []
        for index, row in enumerate(current_terms):
            if not isinstance(row, Mapping):
                issues.append(
                    _issue(
                        "ADMISSION_RIGHTS_MISSING", ("rights_manifest", "terms", index), "terms row is not an object"
                    )
                )
                continue
            relative_path = row.get("relative_path")
            expected_sha256 = row.get("terms_sha256")
            requests.append(
                _TermRequest(
                    relative_path=relative_path if isinstance(relative_path, str) else None,
                    expected_sha256=expected_sha256 if isinstance(expected_sha256, str) else None,
                    source_id=None,
                )
            )
        return tuple(requests)

    source_rights = rights_manifest.get("source_rights")
    if not isinstance(source_rights, list):
        issues.append(_issue("ADMISSION_RIGHTS_MISSING", ("rights_manifest",), "rights manifest has no terms evidence"))
        return ()

    requests = []
    seen_source_ids: set[str] = set()
    for index, row in enumerate(source_rights):
        if not isinstance(row, Mapping):
            issues.append(
                _issue(
                    "ADMISSION_RIGHTS_MISSING",
                    ("rights_manifest", "source_rights", index),
                    "source rights row is not an object",
                )
            )
            continue
        source_id = row.get("source_id")
        expected_sha256 = row.get("terms_sha256")
        relative_path = row.get("relative_path")
        source_id_value = source_id if isinstance(source_id, str) else None
        if source_id_value is None or source_id_value in seen_source_ids:
            issues.append(
                _issue(
                    "ADMISSION_RIGHTS_MISSING",
                    ("rights_manifest", "source_rights", index, "source_id"),
                    "source rights rows must name each source exactly once",
                )
            )
        else:
            seen_source_ids.add(source_id_value)
        requests.append(
            _TermRequest(
                relative_path=relative_path if isinstance(relative_path, str) else None,
                expected_sha256=expected_sha256 if isinstance(expected_sha256, str) else None,
                source_id=source_id_value,
            )
        )
    if manifest_source_ids and seen_source_ids != manifest_source_ids:
        issues.append(
            _issue(
                "ADMISSION_RIGHTS_MISSING",
                ("rights_manifest", "source_rights"),
                "source rights rows do not exactly cover manifest source_id values",
            )
        )
    return tuple(requests)


def _capture_manifest_files(
    root_path: str,
    root_identity: _FileIdentity,
    manifest: Mapping[str, JsonValue] | None,
    issues: list[ValidationIssue],
) -> tuple[tuple[CapturedFile, ...], tuple[CapturedWitness, ...]]:
    if manifest is None:
        return (), ()
    rows = manifest.get("files")
    if not isinstance(rows, list):
        issues.append(_issue("ADMISSION_MANIFEST_MISMATCH", ("manifest", "files"), "manifest files must be an array"))
        return (), ()

    files: list[CapturedFile] = []
    witnesses: list[CapturedWitness] = []
    seen_paths: set[str] = set()
    for index, row_value in enumerate(rows):
        row_path = ("manifest", "files", index)
        file_issues: list[ValidationIssue] = []
        if not isinstance(row_value, Mapping):
            issue = _issue("ADMISSION_MANIFEST_MISMATCH", row_path, "manifest file row is not an object")
            file_issues.append(issue)
            issues.append(issue)
            continue

        relative_path = _string_field(row_value, "relative_path")
        admission_sequence = _string_field(row_value, "admission_sequence")
        availability_slot = _string_field(row_value, "availability_slot")
        expected_sha256 = _string_field(row_value, "raw_payload_sha256")
        expected_byte_length = _string_field(row_value, "byte_length")
        if relative_path is None:
            relative_path = ""
            file_issues.append(
                _issue("ADMISSION_MANIFEST_MISMATCH", (*row_path, "relative_path"), "relative_path is missing")
            )
        elif relative_path in seen_paths:
            file_issues.append(
                _issue("ADMISSION_SET_NOT_CLOSED", (*row_path, "relative_path"), "duplicate manifest relative_path")
            )
        else:
            seen_paths.add(relative_path)
        if admission_sequence is None:
            admission_sequence = ""
            file_issues.append(
                _issue(
                    "ADMISSION_MANIFEST_MISMATCH", (*row_path, "admission_sequence"), "admission_sequence is missing"
                )
            )
        elif not _is_valid_admission_sequence(admission_sequence):
            file_issues.append(
                _issue(
                    "ADMISSION_SEQUENCE_INVALID",
                    (*row_path, "admission_sequence"),
                    "admission_sequence must be a nonzero canonical u64 string",
                )
            )
        if availability_slot is None:
            file_issues.append(
                _issue("ADMISSION_REVISION_CAUSALITY", (*row_path, "availability_slot"), "availability_slot is missing")
            )
        elif not _is_canonical_u64_string(availability_slot):
            file_issues.append(
                _issue(
                    "ADMISSION_REVISION_CAUSALITY",
                    (*row_path, "availability_slot"),
                    "availability_slot must be a canonical u64 string",
                )
            )

        payload_result = _read_local_file(
            root_path,
            root_identity,
            relative_path,
            ("files", index, "payload"),
            missing_code="ADMISSION_SET_NOT_CLOSED",
        )
        file_issues.extend(payload_result.issues)
        if (
            payload_result.sha256 is not None
            and expected_sha256 is not None
            and payload_result.sha256 != expected_sha256
        ):
            file_issues.append(
                _issue(
                    "ADMISSION_HASH_MISMATCH",
                    ("files", index, "raw_payload_sha256"),
                    "captured payload digest differs from manifest row",
                )
            )
        if (
            payload_result.byte_length is not None
            and expected_byte_length is not None
            and str(payload_result.byte_length) != expected_byte_length
        ):
            file_issues.append(
                _issue(
                    "ADMISSION_HASH_MISMATCH",
                    ("files", index, "byte_length"),
                    "captured payload length differs from manifest row",
                )
            )

        witness = _capture_witness(
            root_path,
            root_identity,
            index,
            admission_sequence,
            relative_path,
            expected_sha256,
            expected_byte_length,
        )
        witnesses.append(witness)
        file_issues.extend(witness.issues)
        issues.extend(file_issues)
        files.append(
            CapturedFile(
                admission_sequence=admission_sequence,
                relative_path=relative_path,
                payload=payload_result.payload,
                sha256=payload_result.sha256,
                byte_length=payload_result.byte_length,
                observed_at=witness.observed_at,
                ingested_at=witness.ingested_at,
                expected_sha256=expected_sha256,
                expected_byte_length=expected_byte_length,
                availability_slot=availability_slot,
                media_type=_string_field(row_value, "media_type"),
                source_id=_string_field(row_value, "source_id"),
                source_kind=_string_field(row_value, "source_kind"),
                source_revision=_string_field(row_value, "source_revision"),
                market_id=_string_field(row_value, "market_id"),
                witness=witness,
                issues=tuple(file_issues),
            )
        )
    files.sort(key=lambda captured: _admission_sort_key(captured.admission_sequence, captured.relative_path))
    witnesses.sort(key=lambda captured: _admission_sort_key(captured.admission_sequence, captured.relative_path))
    return tuple(files), tuple(witnesses)


def _capture_witness(
    root_path: str,
    root_identity: _FileIdentity,
    index: int,
    admission_sequence: str,
    relative_path: str,
    expected_sha256: str | None,
    expected_byte_length: str | None,
) -> CapturedWitness:
    witness_issues: list[ValidationIssue] = []
    if not _is_valid_admission_sequence(admission_sequence):
        witness_issues.append(
            _issue(
                "ADMISSION_SEQUENCE_INVALID",
                ("witnesses", index, "admission_sequence"),
                "witness cannot be named without a nonzero canonical admission_sequence",
            )
        )
        return CapturedWitness(
            admission_sequence, relative_path, None, None, None, None, None, None, tuple(witness_issues)
        )
    candidates = _witness_candidates(admission_sequence)
    if not candidates:
        witness_issues.append(
            _issue(
                "ADMISSION_POINT_IN_TIME_MISSING",
                ("witnesses", index),
                "witness cannot be named without admission_sequence",
            )
        )
        return CapturedWitness(
            admission_sequence, relative_path, None, None, None, None, None, None, tuple(witness_issues)
        )

    selected = _select_layout_path(root_path, candidates, ("witnesses", index), "ADMISSION_POINT_IN_TIME_MISSING")
    witness_issues.extend(selected.issues)
    result = _read_local_file(
        root_path,
        root_identity,
        selected.relative_path,
        ("witnesses", index),
        missing_code="ADMISSION_POINT_IN_TIME_MISSING",
    )
    witness_issues.extend(result.issues)

    record: JsonObject | None = None
    observed_at: str | None = None
    ingested_at: str | None = None
    if result.payload is not None:
        try:
            record = parse_canonical_record(result.payload)
        except ValueError as error:
            witness_issues.append(
                _issue("ADMISSION_POINT_IN_TIME_MISSING", ("witnesses", index), f"witness is not canonical LF: {error}")
            )
        else:
            if record.get("admission_sequence") != admission_sequence:
                witness_issues.append(
                    _issue(
                        "ADMISSION_MANIFEST_MISMATCH",
                        ("witnesses", index, "admission_sequence"),
                        "witness sequence mismatch",
                    )
                )
            if record.get("relative_path") != relative_path:
                witness_issues.append(
                    _issue(
                        "ADMISSION_MANIFEST_MISMATCH", ("witnesses", index, "relative_path"), "witness path mismatch"
                    )
                )
            if expected_sha256 is not None and record.get("raw_payload_sha256") != expected_sha256:
                witness_issues.append(
                    _issue(
                        "ADMISSION_MANIFEST_MISMATCH",
                        ("witnesses", index, "raw_payload_sha256"),
                        "witness hash mismatch",
                    )
                )
            if (
                expected_byte_length is not None
                and "byte_length" in record
                and record.get("byte_length") != expected_byte_length
            ):
                witness_issues.append(
                    _issue(
                        "ADMISSION_MANIFEST_MISMATCH",
                        ("witnesses", index, "byte_length"),
                        "witness byte length mismatch",
                    )
                )
            observed_at = _validated_timestamp(
                record.get("observed_at"), ("witnesses", index, "observed_at"), witness_issues
            )
            ingested_at = _validated_timestamp(
                record.get("ingested_at"), ("witnesses", index, "ingested_at"), witness_issues
            )
            if observed_at is not None and ingested_at is not None and ingested_at < observed_at:
                witness_issues.append(
                    _issue(
                        "ADMISSION_POINT_IN_TIME_MISSING",
                        ("witnesses", index, "ingested_at"),
                        "ingested_at precedes observed_at",
                    )
                )
                ingested_at = None

    return CapturedWitness(
        admission_sequence=admission_sequence,
        relative_path=relative_path,
        witness_relative_path=selected.relative_path,
        record=record,
        payload=result.payload,
        sha256=result.sha256,
        observed_at=observed_at,
        ingested_at=ingested_at,
        issues=tuple(witness_issues),
    )


def _witness_candidates(admission_sequence: str) -> tuple[str, ...]:
    if not _is_valid_admission_sequence(admission_sequence):
        return ()
    current = f"witnesses/witness-{int(admission_sequence):04d}.json"
    plan = f"witnesses/{admission_sequence}.json"
    return (current, plan)


def _read_local_file(
    root_path: str,
    root_identity: _FileIdentity,
    relative_path: str,
    issue_path: tuple[str | int, ...],
    *,
    missing_code: str,
) -> _ReadResult:
    read_issues: list[ValidationIssue] = []
    opened = _open_local_file(root_path, root_identity, relative_path, issue_path, read_issues, missing_code)
    if opened is None:
        return _ReadResult(relative_path, None, None, None, tuple(read_issues))

    chunks: list[bytes] = []
    digest = hashlib.sha256()
    byte_count = 0
    try:
        _check_root_stable(root_path, root_identity, read_issues, (*issue_path, "root_after_open"))
        opened_metadata = os.fstat(opened.fd)
        if not stat.S_ISREG(opened_metadata.st_mode):
            read_issues.append(_issue("ADMISSION_SET_NOT_CLOSED", issue_path, "opened entry is not a regular file"))
            return _ReadResult(relative_path, None, None, None, tuple(read_issues))
        opened_identity = _identity(opened_metadata)
        if opened_identity != opened.before_identity:
            read_issues.append(
                _issue("ADMISSION_MANIFEST_MISMATCH", issue_path, "entry identity changed between lstat and open")
            )
            return _ReadResult(relative_path, None, None, None, tuple(read_issues))
        while True:
            try:
                chunk = os.read(opened.fd, _READ_CHUNK_SIZE)
            except OSError as error:
                read_issues.append(_issue(missing_code, issue_path, f"entry cannot be read stably: {error}"))
                return _ReadResult(relative_path, None, None, None, tuple(read_issues))
            if not chunk:
                break
            chunks.append(chunk)
            byte_count += len(chunk)
            digest.update(chunk)

        after_open_metadata = os.fstat(opened.fd)
        after_open_identity = _identity(after_open_metadata)
        if after_open_identity != opened_identity:
            read_issues.append(
                _issue("ADMISSION_MANIFEST_MISMATCH", issue_path, "opened entry identity changed while bytes were read")
            )
        if byte_count != int(after_open_metadata.st_size):
            read_issues.append(
                _issue("ADMISSION_MANIFEST_MISMATCH", issue_path, "opened entry size changed while bytes were read")
            )
    finally:
        os.close(opened.fd)

    _check_root_stable(root_path, root_identity, read_issues, (*issue_path, "root_post_read"))
    _revalidate_prefixes(root_path, opened.prefix_identities, read_issues, issue_path)
    try:
        post_metadata = os.lstat(opened.absolute_path)
    except OSError as error:
        read_issues.append(_issue("ADMISSION_MANIFEST_MISMATCH", issue_path, f"entry disappeared after read: {error}"))
    else:
        if _is_link_or_reparse(post_metadata) or not stat.S_ISREG(post_metadata.st_mode):
            read_issues.append(_issue("ADMISSION_MANIFEST_MISMATCH", issue_path, "entry changed type after read"))
        elif _identity(post_metadata) != opened.before_identity:
            read_issues.append(_issue("ADMISSION_MANIFEST_MISMATCH", issue_path, "entry identity changed after read"))

    if read_issues:
        return _ReadResult(relative_path, None, None, None, tuple(read_issues))

    return _ReadResult(
        relative_path=relative_path,
        payload=b"".join(chunks),
        sha256=digest.hexdigest(),
        byte_length=byte_count,
        issues=tuple(read_issues),
    )


def _open_local_file(
    root_path: str,
    root_identity: _FileIdentity,
    relative_path: str,
    issue_path: tuple[str | int, ...],
    issues: list[ValidationIssue],
    missing_code: str,
) -> _OpenedLocalFile | None:
    absolute_path = _join_checked(root_path, relative_path, issue_path, issues, missing_code)
    if absolute_path is None:
        return None
    safe_path = _safe_relative_path(relative_path)
    if safe_path is None:  # _join_checked already recorded the issue.
        return None
    components = tuple(safe_path.split("/"))

    _check_root_stable(root_path, root_identity, issues, (*issue_path, "root_pre_open"))
    if issues:
        return None

    if _can_use_openat():
        return _open_local_file_openat(
            root_path, root_identity, absolute_path, components, issue_path, issues, missing_code
        )
    return _open_local_file_by_path(root_path, absolute_path, components, issue_path, issues, missing_code)


def _can_use_openat() -> bool:
    return (
        os.name != "nt"
        and _OPEN_SUPPORTS_DIR_FD
        and _STAT_SUPPORTS_DIR_FD
        and _STAT_SUPPORTS_NOFOLLOW
        and _O_DIRECTORY != 0
    )


def _open_local_file_openat(
    root_path: str,
    root_identity: _FileIdentity,
    absolute_path: str,
    components: tuple[str, ...],
    issue_path: tuple[str | int, ...],
    issues: list[ValidationIssue],
    missing_code: str,
) -> _OpenedLocalFile | None:
    dir_flags = os.O_RDONLY | _O_CLOEXEC | _O_DIRECTORY | _O_NOFOLLOW
    file_flags = os.O_RDONLY | _O_BINARY | _O_CLOEXEC | _O_NOFOLLOW
    try:
        current_fd = os.open(root_path, dir_flags)
    except OSError as error:
        issues.append(
            _issue("ADMISSION_NOT_LOCAL", (*issue_path, "root_open"), f"fixture root cannot be opened locally: {error}")
        )
        return None
    prefix_identities: list[_PrefixIdentity] = []
    try:
        root_metadata = os.fstat(current_fd)
        if not stat.S_ISDIR(root_metadata.st_mode) or _identity(root_metadata) != root_identity:
            issues.append(
                _issue("ADMISSION_NOT_LOCAL", (*issue_path, "root_open"), "fixture root identity changed before open")
            )
            return None
        for index, component in enumerate(components[:-1]):
            try:
                next_fd = os.open(component, dir_flags, dir_fd=current_fd)
            except OSError as error:
                prefix_relative = "/".join(components[: index + 1])
                issues.append(
                    _issue(
                        "ADMISSION_NOT_LOCAL",
                        (*issue_path, "prefix", prefix_relative),
                        f"path prefix cannot be opened without following links: {error}",
                    )
                )
                return None
            os.close(current_fd)
            current_fd = next_fd
            prefix_metadata = os.fstat(current_fd)
            if not stat.S_ISDIR(prefix_metadata.st_mode):
                issues.append(
                    _issue(
                        "ADMISSION_SET_NOT_CLOSED", (*issue_path, "prefix", component), "path prefix is not a directory"
                    )
                )
                return None
            prefix_identities.append(_PrefixIdentity("/".join(components[: index + 1]), _identity(prefix_metadata)))
        try:
            fd = os.open(components[-1], file_flags, dir_fd=current_fd)
        except OSError as error:
            issues.append(
                _issue(missing_code, issue_path, f"entry cannot be opened with no-follow read access: {error}")
            )
            return None
        metadata = os.fstat(fd)
        if not stat.S_ISREG(metadata.st_mode):
            os.close(fd)
            issues.append(_issue("ADMISSION_SET_NOT_CLOSED", issue_path, "entry is not a regular file"))
            return None
        return _OpenedLocalFile(fd, absolute_path, _identity(metadata), tuple(prefix_identities))
    finally:
        os.close(current_fd)


def _open_local_file_by_path(
    root_path: str,
    absolute_path: str,
    components: tuple[str, ...],
    issue_path: tuple[str | int, ...],
    issues: list[ValidationIssue],
    missing_code: str,
) -> _OpenedLocalFile | None:
    prefix_identities = _snapshot_prefixes(root_path, components, issue_path, issues, missing_code)
    if prefix_identities is None:
        return None
    try:
        before_metadata = os.lstat(absolute_path)
    except OSError as error:
        issues.append(_issue(missing_code, issue_path, f"entry cannot be classified without following links: {error}"))
        return None
    if _is_link_or_reparse(before_metadata):
        issues.append(_issue("ADMISSION_NOT_LOCAL", issue_path, "entry is a symlink, junction, or reparse point"))
        return None
    if not stat.S_ISREG(before_metadata.st_mode):
        issues.append(_issue("ADMISSION_SET_NOT_CLOSED", issue_path, "entry is not a regular file"))
        return None

    flags = os.O_RDONLY | _O_BINARY | _O_CLOEXEC | _O_NOFOLLOW
    try:
        fd = os.open(absolute_path, flags)
    except OSError as error:
        issues.append(_issue(missing_code, issue_path, f"entry cannot be opened with no-follow read access: {error}"))
        return None
    return _OpenedLocalFile(fd, absolute_path, _identity(before_metadata), prefix_identities)


def _snapshot_prefixes(
    root_path: str,
    components: tuple[str, ...],
    issue_path: tuple[str | int, ...],
    issues: list[ValidationIssue],
    missing_code: str,
) -> tuple[_PrefixIdentity, ...] | None:
    prefix_identities: list[_PrefixIdentity] = []
    prefix_path = root_path
    for index, component in enumerate(components[:-1]):
        prefix_path = os.path.abspath(os.path.join(prefix_path, component))
        prefix_relative = "/".join(components[: index + 1])
        try:
            metadata = os.lstat(prefix_path)
        except OSError as error:
            issues.append(
                _issue(
                    missing_code, (*issue_path, "prefix", prefix_relative), f"path prefix cannot be classified: {error}"
                )
            )
            return None
        if _is_link_or_reparse(metadata):
            issues.append(
                _issue(
                    "ADMISSION_NOT_LOCAL", (*issue_path, "prefix", prefix_relative), "path prefix is a reparse point"
                )
            )
            return None
        if not stat.S_ISDIR(metadata.st_mode):
            issues.append(
                _issue(
                    "ADMISSION_SET_NOT_CLOSED",
                    (*issue_path, "prefix", prefix_relative),
                    "path prefix is not a directory",
                )
            )
            return None
        prefix_identities.append(_PrefixIdentity(prefix_relative, _identity(metadata)))
    return tuple(prefix_identities)


def _revalidate_prefixes(
    root_path: str,
    prefix_identities: tuple[_PrefixIdentity, ...],
    issues: list[ValidationIssue],
    issue_path: tuple[str | int, ...],
) -> None:
    for prefix in prefix_identities:
        absolute_path = os.path.abspath(os.path.join(root_path, *prefix.relative_path.split("/")))
        try:
            metadata = os.lstat(absolute_path)
        except OSError as error:
            issues.append(
                _issue(
                    "ADMISSION_NOT_LOCAL",
                    (*issue_path, "prefix", prefix.relative_path),
                    f"path prefix disappeared after read: {error}",
                )
            )
            continue
        if (
            _is_link_or_reparse(metadata)
            or not stat.S_ISDIR(metadata.st_mode)
            or _identity(metadata) != prefix.identity
        ):
            issues.append(
                _issue(
                    "ADMISSION_NOT_LOCAL",
                    (*issue_path, "prefix", prefix.relative_path),
                    "path prefix identity changed during read",
                )
            )


def _capture_extra_payload_entries(
    root_path: str,
    root_identity: _FileIdentity,
    expected_paths: set[str],
    issues: list[ValidationIssue],
) -> None:
    payload_root = "payloads"
    payload_root_abs = _join_checked(root_path, payload_root, ("payloads",), issues, "ADMISSION_SET_NOT_CLOSED")
    if payload_root_abs is None:
        return
    try:
        metadata = os.lstat(payload_root_abs)
    except FileNotFoundError:
        return
    except OSError as error:
        issues.append(
            _issue("ADMISSION_SET_NOT_CLOSED", ("payloads",), f"payload directory cannot be classified: {error}")
        )
        return
    if _is_link_or_reparse(metadata):
        issues.append(_issue("ADMISSION_NOT_LOCAL", ("payloads",), "payload directory is a reparse point"))
        return
    if not stat.S_ISDIR(metadata.st_mode):
        return
    _scan_payload_directory(root_path, root_identity, payload_root_abs, payload_root, expected_paths, issues)


def _scan_payload_directory(
    root_path: str,
    root_identity: _FileIdentity,
    directory_path: str,
    relative_directory: str,
    expected_paths: set[str],
    issues: list[ValidationIssue],
) -> None:
    _check_root_stable(root_path, root_identity, issues, ("payloads", "scan_root"))
    try:
        entries = os.scandir(directory_path)
    except OSError as error:
        issues.append(
            _issue(
                "ADMISSION_SET_NOT_CLOSED",
                ("payloads", relative_directory),
                f"payload directory cannot be listed: {error}",
            )
        )
        return
    try:
        with entries:
            for entry in entries:
                name = _safe_component(entry.name)
                child_relative = f"{relative_directory}/{name}"
                child_abs = _join_checked(
                    root_path, child_relative, ("payloads", child_relative), issues, "ADMISSION_SET_NOT_CLOSED"
                )
                if child_abs is None:
                    continue
                try:
                    metadata = os.lstat(child_abs)
                except OSError as error:
                    issues.append(
                        _issue(
                            "ADMISSION_SET_NOT_CLOSED",
                            ("payloads", child_relative),
                            f"payload entry cannot be classified: {error}",
                        )
                    )
                    continue
                if _is_link_or_reparse(metadata):
                    issues.append(
                        _issue("ADMISSION_NOT_LOCAL", ("payloads", child_relative), "payload entry is a reparse point")
                    )
                elif stat.S_ISDIR(metadata.st_mode):
                    _scan_payload_directory(root_path, root_identity, child_abs, child_relative, expected_paths, issues)
                elif stat.S_ISREG(metadata.st_mode):
                    if child_relative not in expected_paths:
                        issues.append(
                            _issue(
                                "ADMISSION_SET_NOT_CLOSED",
                                ("payloads", child_relative),
                                "payload entry is not named by manifest",
                            )
                        )
                else:
                    issues.append(
                        _issue("ADMISSION_SET_NOT_CLOSED", ("payloads", child_relative), "payload entry is not regular")
                    )
    except ValueError as error:
        issues.append(_issue("ADMISSION_SET_NOT_CLOSED", ("payloads", relative_directory), str(error)))


def _join_checked(
    root_path: str,
    relative_path: str,
    issue_path: tuple[str | int, ...],
    issues: list[ValidationIssue],
    code: str,
) -> str | None:
    safe_path = _safe_relative_path(relative_path)
    if safe_path is None:
        issues.append(_issue(code, issue_path, f"unsafe fixture relative path: {relative_path!r}"))
        return None
    absolute_path = os.path.abspath(os.path.join(root_path, *safe_path.split("/")))
    try:
        common = os.path.commonpath((os.path.normcase(root_path), os.path.normcase(absolute_path)))
    except ValueError:
        issues.append(_issue(code, issue_path, "fixture path is outside the explicit root"))
        return None
    if common != os.path.normcase(root_path):
        issues.append(_issue(code, issue_path, "fixture path escapes the explicit root"))
        return None
    return absolute_path


def _safe_relative_path(path: object) -> str | None:
    if not isinstance(path, str):
        return None
    normalized = unicodedata.normalize("NFC", path)
    if normalized != path:
        return None
    if (
        not normalized
        or normalized.startswith("/")
        or "\\" in normalized
        or ":" in normalized
        or "\x00" in normalized
        or normalized in {".", ".."}
        or normalized.startswith("../")
    ):
        return None
    components = normalized.split("/")
    if any(component in {"", ".", ".."} for component in components):
        return None
    try:
        normalized.encode("utf-8", errors="strict")
    except UnicodeEncodeError:
        return None
    return normalized


def _safe_component(name: Any) -> str:
    if not isinstance(name, str):
        raise ValueError("payload entry name is not a filesystem string")
    normalized = unicodedata.normalize("NFC", name)
    if (
        not normalized
        or normalized in {".", ".."}
        or "/" in normalized
        or "\\" in normalized
        or ":" in normalized
        or "\x00" in normalized
    ):
        raise ValueError("payload entry name is not a safe POSIX component")
    try:
        normalized.encode("utf-8", errors="strict")
    except UnicodeEncodeError as error:
        raise ValueError("payload entry name is not strict UTF-8") from error
    return normalized


def _check_root_stable(
    root_path: str,
    expected: _FileIdentity,
    issues: list[ValidationIssue],
    issue_path: tuple[str | int, ...],
) -> None:
    try:
        metadata = os.lstat(root_path)
    except OSError as error:
        issues.append(_issue("ADMISSION_NOT_LOCAL", issue_path, f"fixture root cannot be reclassified: {error}"))
        return
    if _is_link_or_reparse(metadata) or not stat.S_ISDIR(metadata.st_mode) or _identity(metadata) != expected:
        issues.append(_issue("ADMISSION_NOT_LOCAL", issue_path, "fixture root identity changed during capture"))


def _is_link_or_reparse(metadata: os.stat_result) -> bool:
    attributes = int(getattr(metadata, "st_file_attributes", 0))
    return stat.S_ISLNK(metadata.st_mode) or bool(attributes & _FILE_ATTRIBUTE_REPARSE_POINT)


def _identity(metadata: os.stat_result) -> _FileIdentity:
    return _FileIdentity(
        device=int(metadata.st_dev),
        inode=int(metadata.st_ino),
        mode_type=stat.S_IFMT(metadata.st_mode),
        size=int(metadata.st_size),
        modified_ns=int(metadata.st_mtime_ns),
    )


def _validated_timestamp(value: JsonValue, path: tuple[str | int, ...], issues: list[ValidationIssue]) -> str | None:
    if isinstance(value, str) and _is_rfc3339_ns_utc(value):
        return value
    issues.append(_issue("ADMISSION_POINT_IN_TIME_MISSING", path, "witness timestamp is missing or invalid"))
    return None


def _is_rfc3339_ns_utc(value: str) -> bool:
    if len(value) != 30:
        return False
    if value[4] != "-" or value[7] != "-" or value[10] != "T" or value[13] != ":" or value[16] != ":":
        return False
    if value[19] != "." or value[-1] != "Z":
        return False
    digit_spans = (value[0:4], value[5:7], value[8:10], value[11:13], value[14:16], value[17:19], value[20:29])
    if not all(span.isdecimal() for span in digit_spans):
        return False
    year = int(value[0:4])
    month = int(value[5:7])
    day = int(value[8:10])
    hour = int(value[11:13])
    minute = int(value[14:16])
    second = int(value[17:19])
    if not (1 <= month <= 12 and 0 <= hour <= 23 and 0 <= minute <= 59 and 0 <= second <= 59):
        return False
    return 1 <= day <= _days_in_month(year, month)


def _days_in_month(year: int, month: int) -> int:
    if month == 2:
        if year % 400 == 0 or (year % 4 == 0 and year % 100 != 0):
            return 29
        return 28
    if month in {4, 6, 9, 11}:
        return 30
    return 31


def _string_field(row: Mapping[str, JsonValue], field: str) -> str | None:
    value = row.get(field)
    return value if isinstance(value, str) else None


def _admission_sort_key(admission_sequence: str, relative_path: str) -> tuple[int, str, str]:
    if _is_canonical_u64_string(admission_sequence):
        return int(admission_sequence), "", relative_path
    return 0, admission_sequence, relative_path


def _is_canonical_u64_string(value: object) -> bool:
    if not isinstance(value, str):
        return False
    if value == "0":
        return True
    if not value or not value.isascii() or not value.isdecimal() or value.startswith("0"):
        return False
    return len(value) < len(_MAX_U64_TEXT) or (len(value) == len(_MAX_U64_TEXT) and value <= _MAX_U64_TEXT)


def _is_valid_admission_sequence(value: object) -> bool:
    return _is_canonical_u64_string(value) and value != "0"


def _is_sha256_string(value: object) -> bool:
    return isinstance(value, str) and len(value) == 64 and all(character in "0123456789abcdef" for character in value)


def _issue(code: str, path: tuple[str | int, ...], message: str) -> ValidationIssue:
    return ValidationIssue(code=code, path=path, message=message)


def _read_result_with_issues(result: _ReadResult, issues: tuple[ValidationIssue, ...]) -> _ReadResult:
    return _ReadResult(
        result.relative_path, result.payload, result.sha256, result.byte_length, (*issues, *result.issues)
    )
