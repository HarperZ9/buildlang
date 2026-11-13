"""Strict local disk adapter for the offline paper-core replay envelope."""

from __future__ import annotations

import json
import os
import stat
from dataclasses import dataclass
from pathlib import Path
from typing import cast

from build_finance.crypto_replay.admission import AdmissionBatch, ParsedSourceCandidate
from build_finance.crypto_replay.canonical import JsonObject, canonical_json_bytes, parse_canonical_record, sha256_hex
from build_finance.crypto_replay.content_ids import compute_content_id, verify_content_id
from build_finance.crypto_replay.local_fixture import CapturedFixture
from build_finance.crypto_replay.schema_registry import validate_contract
from build_finance.live_paper.profiles import PaperKernelProfiles
from build_finance.live_paper.resolver import EvidenceResolver

_EXPECTED_ENVELOPE = {
    "layout": "content-addressed-v1",
    "model_signal_mode": "DISABLED",
    "paper_core_distribution": "build-finance-paper-core",
    "paper_core_version": "1.1.0",
    "schema": "build-finance.paper-core.replay-envelope/v1",
}
_EXPECTED_ENVELOPE_BYTES = canonical_json_bytes(_EXPECTED_ENVELOPE)
_ENVELOPE_ROOT = "replay-envelope"
_MAX_RECORD_BYTES = 2 * 1024 * 1024
_MAX_PROFILE_BYTES = 2 * 1024 * 1024
_MAX_DIGEST_BYTES = 32 * 1024 * 1024
_READ_CHUNK_SIZE = 1024 * 1024
_O_BINARY = int(vars(os).get("O_BINARY", 0))
_O_CLOEXEC = int(vars(os).get("O_CLOEXEC", 0))
_O_DIRECTORY = int(vars(os).get("O_DIRECTORY", 0))
_O_NOFOLLOW = int(vars(os).get("O_NOFOLLOW", 0))
_FILE_ATTRIBUTE_REPARSE_POINT = 0x400
_OPEN_SUPPORTS_DIR_FD = os.open in getattr(os, "supports_dir_fd", set())
_STAT_SUPPORTS_DIR_FD = os.stat in getattr(os, "supports_dir_fd", set())
_STAT_SUPPORTS_NOFOLLOW = os.stat in getattr(os, "supports_follow_symlinks", set())


@dataclass(frozen=True, slots=True)
class ReplayEnvelopeContext:
    """Closed paper-core inputs loaded from exact replay-envelope bytes."""

    run_receipt_record: bytes
    resolver: EvidenceResolver
    profiles: PaperKernelProfiles

    def __post_init__(self) -> None:
        object.__setattr__(self, "run_receipt_record", bytes(self.run_receipt_record))


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
class _OpenedMember:
    fd: int
    absolute_path: str
    before_identity: _FileIdentity
    prefix_identities: tuple[_PrefixIdentity, ...]


class _DiskReplayResolver:
    """Resolve only fixed content-addressed members below one replay envelope."""

    __slots__ = ("_root_identity", "_root_path")

    def __init__(self, root_path: str, root_identity: _FileIdentity) -> None:
        self._root_path = root_path
        self._root_identity = root_identity

    def resolve_record(self, content_id: str) -> bytes:
        _require_sha256_key(content_id, "record ContentID")
        record = _read_member(
            self._root_path,
            self._root_identity,
            f"{_ENVELOPE_ROOT}/records/{content_id}.json",
            limit=_MAX_RECORD_BYTES,
            label="self-addressed record",
        )
        try:
            document = parse_canonical_record(record)
            valid = verify_content_id(document)
            actual = compute_content_id(document)
        except Exception as error:  # noqa: BLE001 - normalize all malformed record failures.
            raise ValueError(f"self-addressed record {content_id} is malformed: {error}") from error
        if not valid or actual != content_id:
            raise ValueError(f"self-addressed record {content_id} does not match its ContentID")
        return record

    def resolve_bytes(self, sha256: str) -> bytes:
        _require_sha256_key(sha256, "byte SHA-256")
        payload = _read_member(
            self._root_path,
            self._root_identity,
            f"{_ENVELOPE_ROOT}/bytes/{sha256}.bin",
            limit=_MAX_DIGEST_BYTES,
            label="digest-addressed bytes",
        )
        if sha256_hex(payload) != sha256:
            raise ValueError(f"digest-addressed bytes {sha256} do not match their filename digest")
        return payload


def load_replay_envelope_context(
    fixture_root: Path,
    captured: CapturedFixture,
    admission: AdmissionBatch,
) -> ReplayEnvelopeContext:
    """Load exact disk replay authority for an already captured and admitted fixture."""

    root_path, root_identity = _validated_root_path(fixture_root, captured)
    _require_current_paper_core_version()
    envelope = _read_member(
        root_path,
        root_identity,
        f"{_ENVELOPE_ROOT}/envelope.json",
        limit=_MAX_RECORD_BYTES,
        label="replay envelope",
    )
    if envelope != _EXPECTED_ENVELOPE_BYTES:
        raise ValueError("replay envelope bytes do not match the fixed v1 disabled-model contract")
    run_receipt_record = _read_member(
        root_path,
        root_identity,
        f"{_ENVELOPE_ROOT}/run-receipt.json",
        limit=_MAX_RECORD_BYTES,
        label="run receipt record",
    )
    run_receipt = _verified_run_receipt(run_receipt_record)
    _require_admitted(captured, admission, run_receipt)
    resolver = _DiskReplayResolver(root_path, root_identity)
    risk_config_id = run_receipt.get("validated_config_sha256")
    if not isinstance(risk_config_id, str):
        raise ValueError("run receipt does not root a replay-risk-config record")
    if captured.manifest_payload is None:
        raise ValueError("captured fixture did not retain exact fixture manifest bytes")
    profiles = PaperKernelProfiles(
        normalization_profile_record=captured.manifest_payload,
        feature_profile_record=_read_profile(root_path, root_identity, "feature.bin"),
        algorithm_profile_records=(_read_profile(root_path, root_identity, "algorithm-0.bin"),),
        model_validation_profile_record=_read_profile(root_path, root_identity, "model-validation.bin"),
        fusion_profile_record=_read_profile(root_path, root_identity, "fusion.bin"),
        risk_config_record=resolver.resolve_record(risk_config_id),
        fill_profile_record=_read_profile(root_path, root_identity, "fill.bin"),
    )
    return ReplayEnvelopeContext(
        run_receipt_record=run_receipt_record,
        resolver=cast(EvidenceResolver, resolver),
        profiles=profiles,
    )


def _validated_root_path(fixture_root: Path, captured: CapturedFixture) -> tuple[str, _FileIdentity]:
    if not isinstance(fixture_root, Path):
        raise TypeError("load_replay_envelope_context requires an explicit pathlib.Path fixture_root")
    root_path = os.path.abspath(os.fspath(fixture_root))
    if captured.root != fixture_root or captured.root_path != root_path:
        raise ValueError("fixture_root must exactly match the captured fixture root")
    try:
        metadata = os.lstat(root_path)
    except OSError as error:
        raise ValueError(f"fixture root cannot be classified: {error}") from error
    if _is_link_or_reparse(metadata) or not stat.S_ISDIR(metadata.st_mode):
        raise ValueError("fixture root must be a local non-linked directory")
    return root_path, _identity(metadata)


def _require_admitted(
    captured: CapturedFixture,
    admission: AdmissionBatch,
    run_receipt: JsonObject,
) -> None:
    if not isinstance(admission, AdmissionBatch):
        raise TypeError("load_replay_envelope_context requires an AdmissionBatch")
    if admission.status != "ADMITTED" or admission.reason_codes:
        raise ValueError("replay envelope requires an admitted source fixture")
    if not admission.candidates or len(admission.candidates) != len(admission.source_receipt_records):
        raise ValueError("replay envelope requires paired admitted candidates and source receipts")
    fixture_id = captured.fixture_manifest_sha256
    if fixture_id is None or run_receipt["fixture_manifest_sha256"] != fixture_id:
        raise ValueError("run receipt fixture manifest does not match the captured fixture")

    receipt_ids: list[str] = []
    for candidate, record in zip(admission.candidates, admission.source_receipt_records, strict=True):
        receipt = _verified_source_receipt(record)
        if receipt["status"] != "ADMITTED" or receipt["reason_codes"] != []:
            raise ValueError("replay envelope requires exact ADMITTED source receipts")
        if receipt["fixture_manifest_sha256"] != fixture_id:
            raise ValueError("source receipt fixture manifest does not match the captured fixture")
        _require_candidate_receipt_identity(candidate, receipt)
        receipt_ids.append(cast(str, receipt["source_admission_receipt_id"]))

    declared_value = run_receipt["source_admission_receipt_ids"]
    if not isinstance(declared_value, list):  # Defensive after the contract verifier.
        raise ValueError("run receipt source receipt set is malformed")
    declared_ids = tuple(cast(str, value) for value in declared_value)
    if len(set(receipt_ids)) != len(receipt_ids) or set(receipt_ids) != set(declared_ids):
        raise ValueError("admission source receipts do not equal the run receipt's exact declared set")


def _verified_source_receipt(record: bytes) -> JsonObject:
    try:
        receipt = parse_canonical_record(bytes(record))
        issues = validate_contract(receipt, expected_schema="trading.source-admission-receipt/v1")
        valid = verify_content_id(receipt)
    except Exception as error:  # noqa: BLE001 - normalize malformed admission evidence.
        raise ValueError(f"source admission receipt is malformed: {error}") from error
    if issues:
        raise ValueError(f"source admission receipt violates its contract: {issues}")
    if not valid:
        raise ValueError("source admission receipt self-ID does not match its canonical body")
    return receipt


def _require_candidate_receipt_identity(candidate: ParsedSourceCandidate, receipt: JsonObject) -> None:
    candidate_identity = (
        candidate.admission_sequence,
        candidate.relative_path,
        candidate.raw_payload_sha256,
        candidate.source_id,
        candidate.source_kind,
        candidate.source_revision,
        candidate.market_id,
        candidate.observed_at,
        candidate.ingested_at,
    )
    receipt_identity = (
        receipt["admission_sequence"],
        receipt["relative_path"],
        receipt["raw_payload_sha256"],
        receipt["source_id"],
        receipt["source_kind"],
        receipt["source_revision"],
        receipt["market_id"],
        receipt["observed_at"],
        receipt["ingested_at"],
    )
    if candidate_identity != receipt_identity:
        raise ValueError("admitted candidate identity does not match its exact source receipt")


def _require_current_paper_core_version() -> None:
    manifest_path = Path(__file__).with_name("live_paper") / "paper_core_manifest.json"
    manifest = json.loads(manifest_path.read_bytes().decode("utf-8"))
    if not isinstance(manifest, dict):
        raise ValueError("paper-core manifest is malformed")
    if manifest.get("distribution_name") != _EXPECTED_ENVELOPE["paper_core_distribution"]:
        raise ValueError("paper-core distribution does not match replay envelope contract")
    if manifest.get("distribution_version") != _EXPECTED_ENVELOPE["paper_core_version"]:
        raise ValueError("paper-core version does not match replay envelope contract")


def _verified_run_receipt(record: bytes) -> JsonObject:
    try:
        run_receipt = parse_canonical_record(record)
        issues = validate_contract(run_receipt, expected_schema="trading.run-receipt/v1")
        valid = verify_content_id(run_receipt)
    except Exception as error:  # noqa: BLE001 - normalize all malformed run receipt failures.
        raise ValueError(f"run receipt record is malformed: {error}") from error
    if issues:
        raise ValueError(f"run receipt record violates its contract: {issues}")
    if not valid:
        raise ValueError("run receipt record self-ID does not match its canonical body")
    if (
        run_receipt.get("model_signal_mode") != "DISABLED"
        or run_receipt.get("model_registry_sha256") is not None
        or run_receipt.get("model_signal_manifest_sha256") is not None
    ):
        raise ValueError("run receipt must bind disabled model mode with no model bodies")
    return run_receipt


def _read_profile(root_path: str, root_identity: _FileIdentity, filename: str) -> bytes:
    return _read_member(
        root_path,
        root_identity,
        f"{_ENVELOPE_ROOT}/profiles/{filename}",
        limit=_MAX_PROFILE_BYTES,
        label="profile snapshot",
    )


def _read_member(
    root_path: str,
    root_identity: _FileIdentity,
    relative_path: str,
    *,
    limit: int,
    label: str,
) -> bytes:
    absolute_path = _absolute_member(root_path, relative_path)
    components = tuple(relative_path.split("/"))
    opened = _open_member(root_path, root_identity, absolute_path, components, relative_path, label)
    chunks: list[bytes] = []
    byte_count = 0
    try:
        _require_root_identity(root_path, root_identity, label, "after open")
        _revalidate_prefixes(root_path, opened.prefix_identities, label, "after open")
        opened_metadata = os.fstat(opened.fd)
        if not stat.S_ISREG(opened_metadata.st_mode) or _identity(opened_metadata) != opened.before_identity:
            raise OSError(f"{label} {relative_path} changed identity before read")
        if int(opened_metadata.st_size) > limit:
            raise ValueError(f"{label} {relative_path} exceeds the bounded read size")
        while True:
            chunk = os.read(opened.fd, min(_READ_CHUNK_SIZE, limit + 1 - byte_count))
            if not chunk:
                break
            chunks.append(chunk)
            byte_count += len(chunk)
            if byte_count > limit:
                raise ValueError(f"{label} {relative_path} exceeds the bounded read size")
        after_open = os.fstat(opened.fd)
        if _identity(after_open) != opened.before_identity or byte_count != int(after_open.st_size):
            raise OSError(f"{label} {relative_path} changed while being read")
    finally:
        os.close(opened.fd)

    _require_root_identity(root_path, root_identity, label, "after read")
    _revalidate_prefixes(root_path, opened.prefix_identities, label, "after read")
    try:
        after = os.lstat(opened.absolute_path)
    except OSError as error:
        raise OSError(f"{label} {relative_path} disappeared after read: {error}") from error
    if _is_link_or_reparse(after) or _identity(after) != opened.before_identity:
        raise OSError(f"{label} {relative_path} changed identity after read")
    return b"".join(chunks)


def _open_member(
    root_path: str,
    root_identity: _FileIdentity,
    absolute_path: str,
    components: tuple[str, ...],
    relative_path: str,
    label: str,
) -> _OpenedMember:
    _require_root_identity(root_path, root_identity, label, "before open")
    if _can_use_openat():
        return _open_member_openat(root_path, root_identity, absolute_path, components, relative_path, label)
    return _open_member_by_path(root_path, absolute_path, components, relative_path, label)


def _can_use_openat() -> bool:
    return (
        os.name != "nt"
        and _OPEN_SUPPORTS_DIR_FD
        and _STAT_SUPPORTS_DIR_FD
        and _STAT_SUPPORTS_NOFOLLOW
        and _O_DIRECTORY != 0
    )


def _open_member_openat(
    root_path: str,
    root_identity: _FileIdentity,
    absolute_path: str,
    components: tuple[str, ...],
    relative_path: str,
    label: str,
) -> _OpenedMember:
    directory_flags = os.O_RDONLY | _O_CLOEXEC | _O_DIRECTORY | _O_NOFOLLOW
    file_flags = os.O_RDONLY | _O_BINARY | _O_CLOEXEC | _O_NOFOLLOW
    try:
        current_fd = os.open(root_path, directory_flags)
    except OSError as error:
        raise OSError(f"{label} fixture root cannot be opened without following links: {error}") from error
    prefix_identities: list[_PrefixIdentity] = []
    try:
        root_metadata = os.fstat(current_fd)
        if not stat.S_ISDIR(root_metadata.st_mode) or _identity(root_metadata) != root_identity:
            raise OSError(f"{label} fixture root identity changed before open")
        for index, component in enumerate(components[:-1]):
            prefix_relative = "/".join(components[: index + 1])
            try:
                next_fd = os.open(component, directory_flags, dir_fd=current_fd)
            except OSError as error:
                raise OSError(
                    f"{label} path prefix {prefix_relative} cannot be opened without following links: {error}"
                ) from error
            os.close(current_fd)
            current_fd = next_fd
            prefix_metadata = os.fstat(current_fd)
            if not stat.S_ISDIR(prefix_metadata.st_mode):
                raise OSError(f"{label} path prefix {prefix_relative} is not a directory")
            prefix_identities.append(_PrefixIdentity(prefix_relative, _identity(prefix_metadata)))
        try:
            fd = os.open(components[-1], file_flags, dir_fd=current_fd)
        except OSError as error:
            raise OSError(f"{label} {relative_path} cannot be opened without following links: {error}") from error
        metadata = os.fstat(fd)
        if not stat.S_ISREG(metadata.st_mode):
            os.close(fd)
            raise OSError(f"{label} {relative_path} is not a regular file")
        return _OpenedMember(fd, absolute_path, _identity(metadata), tuple(prefix_identities))
    finally:
        os.close(current_fd)


def _open_member_by_path(
    root_path: str,
    absolute_path: str,
    components: tuple[str, ...],
    relative_path: str,
    label: str,
) -> _OpenedMember:
    prefix_identities = _snapshot_prefixes(root_path, components, label)
    try:
        before = os.lstat(absolute_path)
    except OSError as error:
        raise FileNotFoundError(f"{label} {relative_path} cannot be classified: {error}") from error
    if _is_link_or_reparse(before):
        raise OSError(f"{label} {relative_path} is a link/reparse member")
    if not stat.S_ISREG(before.st_mode):
        raise OSError(f"{label} {relative_path} is not a regular file")

    flags = os.O_RDONLY | _O_BINARY | _O_CLOEXEC | _O_NOFOLLOW
    try:
        fd = os.open(absolute_path, flags)
    except OSError as error:
        raise OSError(f"{label} {relative_path} cannot be opened without following links: {error}") from error
    return _OpenedMember(fd, absolute_path, _identity(before), prefix_identities)


def _absolute_member(root_path: str, relative_path: str) -> str:
    components = relative_path.split("/")
    if any(component in {"", ".", ".."} for component in components):
        raise ValueError(f"unsafe replay envelope path: {relative_path!r}")
    absolute_path = os.path.abspath(os.path.join(root_path, *components))
    try:
        common = os.path.commonpath((os.path.normcase(root_path), os.path.normcase(absolute_path)))
    except ValueError as error:
        raise ValueError("replay envelope path is outside the fixture root") from error
    if common != os.path.normcase(root_path):
        raise ValueError("replay envelope path escapes the fixture root")
    return absolute_path


def _snapshot_prefixes(
    root_path: str,
    components: tuple[str, ...],
    label: str,
) -> tuple[_PrefixIdentity, ...]:
    prefix_identities: list[_PrefixIdentity] = []
    prefix_path = root_path
    for index, component in enumerate(components[:-1]):
        prefix_path = os.path.abspath(os.path.join(prefix_path, component))
        prefix_relative = "/".join(components[: index + 1])
        try:
            metadata = os.lstat(prefix_path)
        except OSError as error:
            raise FileNotFoundError(f"{label} path prefix cannot be classified: {error}") from error
        if _is_link_or_reparse(metadata):
            raise OSError(f"{label} path prefix is a link/reparse member")
        if not stat.S_ISDIR(metadata.st_mode):
            raise OSError(f"{label} path prefix is not a directory")
        prefix_identities.append(_PrefixIdentity(prefix_relative, _identity(metadata)))
    return tuple(prefix_identities)


def _revalidate_prefixes(
    root_path: str,
    prefix_identities: tuple[_PrefixIdentity, ...],
    label: str,
    phase: str,
) -> None:
    for prefix in prefix_identities:
        absolute_path = os.path.abspath(os.path.join(root_path, *prefix.relative_path.split("/")))
        try:
            metadata = os.lstat(absolute_path)
        except OSError as error:
            raise OSError(f"{label} path prefix disappeared {phase}: {error}") from error
        if (
            _is_link_or_reparse(metadata)
            or not stat.S_ISDIR(metadata.st_mode)
            or _identity(metadata) != prefix.identity
        ):
            raise OSError(f"{label} path prefix identity changed {phase}")


def _require_root_identity(
    root_path: str,
    expected: _FileIdentity,
    label: str,
    phase: str,
) -> None:
    try:
        metadata = os.lstat(root_path)
    except OSError as error:
        raise OSError(f"{label} fixture root cannot be reclassified {phase}: {error}") from error
    if _is_link_or_reparse(metadata) or not stat.S_ISDIR(metadata.st_mode) or _identity(metadata) != expected:
        raise OSError(f"{label} fixture root identity changed {phase}")


def _require_sha256_key(value: str, label: str) -> None:
    if not isinstance(value, str) or len(value) != 64 or any(character not in "0123456789abcdef" for character in value):
        raise ValueError(f"{label} must be a lowercase 64-hex key")


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


__all__ = ["ReplayEnvelopeContext", "load_replay_envelope_context"]
