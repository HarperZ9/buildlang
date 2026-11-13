"""Export one reproducible synthetic G2 PAPER ONLY evaluation bundle."""

# ruff: noqa: E402 - direct script execution must prefer this source checkout.

from __future__ import annotations

import argparse
import hashlib
import json
import os
import stat
import sys
from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from pathlib import Path, PurePosixPath
from typing import cast

ROOT = Path(__file__).resolve().parents[1]
while str(ROOT) in sys.path:
    sys.path.remove(str(ROOT))
sys.path.insert(0, str(ROOT))

import build_finance
from build_finance.crypto_replay.admission import admit_local_fixture
from build_finance.crypto_replay.canonical import JsonObject, canonical_record_bytes, parse_canonical_json
from build_finance.crypto_replay.local_fixture import capture_local_fixture
from build_finance.live_paper.kernel import run_offline_paper_kernel
from build_finance.live_paper.projections import kernel_result_canonical_bytes
from build_finance.live_paper.run_input_builder import build_verified_run_inputs
from build_finance.paper_core_loader import load_replay_envelope_context
from tests.live_paper.support.g2_disk_bundle import write_g2_disk_bundle

CHECKSUM_MANIFEST_NAME = "SHA256SUMS"
KERNEL_PROJECTION_NAME = "kernel-projection.json"
BUILD_FINANCE_ROOT = Path(cast(str, build_finance.__file__)).resolve().parent
_MAX_CHECKSUM_MANIFEST_BYTES = 16 * 1024
_MAX_FILE_BYTES = 1024 * 1024
_MAX_BUNDLE_BYTES = 8 * 1024 * 1024
_MAX_BUNDLE_FILE_COUNT = 64
_READ_CHUNK_SIZE = 64 * 1024
_O_BINARY = int(vars(os).get("O_BINARY", 0))
_O_CLOEXEC = int(vars(os).get("O_CLOEXEC", 0))
_O_DIRECTORY = int(vars(os).get("O_DIRECTORY", 0))
_O_NOFOLLOW = int(vars(os).get("O_NOFOLLOW", 0))
_FILE_ATTRIBUTE_REPARSE_POINT = 0x400
_OPEN_SUPPORTS_DIR_FD = os.open in getattr(os, "supports_dir_fd", set())
_STAT_SUPPORTS_DIR_FD = os.stat in getattr(os, "supports_dir_fd", set())
_STAT_SUPPORTS_NOFOLLOW = os.stat in getattr(os, "supports_follow_symlinks", set())


class ExportError(RuntimeError):
    """The synthetic evaluation bundle could not be exported safely."""


@dataclass(frozen=True, slots=True)
class EvaluationBundleExport:
    """Stable paths produced by one successful local evaluation export."""

    destination: Path
    fixture_root: Path
    checksum_manifest: Path
    kernel_projection: Path


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
class _DiscoveredFile:
    relative_path: str
    absolute_path: str
    identity: _FileIdentity
    prefix_identities: tuple[_PrefixIdentity, ...]


@dataclass(frozen=True, slots=True)
class _BundleSnapshot:
    root_path: str
    root_identity: _FileIdentity
    files: Mapping[str, _DiscoveredFile]


def _empty_destination(destination: Path) -> Path:
    destination = destination.absolute()
    try:
        metadata = os.lstat(destination)
    except FileNotFoundError:
        destination.mkdir(parents=True)
        return destination
    if _is_link_or_reparse(metadata):
        raise ExportError(f"destination must not be a link/reparse path: {destination}")
    if not stat.S_ISDIR(metadata.st_mode) or any(destination.iterdir()):
        raise ExportError(f"destination must be an empty directory: {destination}")
    return destination


def _is_link_or_reparse(metadata: os.stat_result) -> bool:
    attributes = int(getattr(metadata, "st_file_attributes", 0))
    return stat.S_ISLNK(metadata.st_mode) or bool(attributes & _FILE_ATTRIBUTE_REPARSE_POINT)


def _projection_document(result_payload: JsonObject) -> JsonObject:
    projection = result_payload.get("projection")
    closure = result_payload.get("closure")
    if not isinstance(projection, dict) or not isinstance(closure, dict):
        raise ExportError("kernel result did not contain projection and closure objects")
    if closure.get("status") != "CLOSED" or closure.get("reason_codes") != []:
        raise ExportError("synthetic evaluation replay did not close cleanly")
    model_rows = projection.get("model_validation")
    if (
        not isinstance(model_rows, list)
        or len(model_rows) != 2
        or not all(isinstance(row, Mapping) for row in model_rows)
        or [(row.get("disposition"), row.get("reason_code")) for row in model_rows]
        != [("ABSTAIN", "MODEL_DISABLED"), ("ABSTAIN", "MODEL_DISABLED")]
    ):
        raise ExportError("synthetic evaluation replay did not retain deterministic model abstention")
    return cast(
        JsonObject,
        {
            "closure_status": "CLOSED",
            "data_classification": "SYNTHETIC",
            "execution": "SIMULATED",
            "mode": "PAPER_ONLY",
            "model_inference": "DISABLED_ABSTAIN",
            "profitability_claim": False,
            "projection": projection,
            "schema": "build-finance.live-paper.g2-evaluation-projection/v1",
        },
    )


def _write_checksum_manifest(destination: Path) -> Path:
    manifest = destination / CHECKSUM_MANIFEST_NAME
    snapshot = _bundle_snapshot(destination)
    if CHECKSUM_MANIFEST_NAME in snapshot.files:
        raise ExportError("evaluation bundle checksum manifest already exists")
    rows: list[bytes] = []
    payload_size = 0
    for relative, discovered in sorted(snapshot.files.items()):
        row = f"{_hash_discovered_file(snapshot, discovered, limit=_MAX_FILE_BYTES)}  {relative}\n".encode()
        payload_size += len(row)
        if payload_size > _MAX_CHECKSUM_MANIFEST_BYTES:
            raise ExportError("checksum manifest exceeds the bounded read size")
        rows.append(row)
    payload = b"".join(rows)
    manifest.write_bytes(payload)
    return manifest


def _safe_checksum_path(relative: str) -> PurePosixPath:
    path = PurePosixPath(relative)
    if (
        not relative
        or "\\" in relative
        or "\0" in relative
        or path.is_absolute()
        or path.as_posix() != relative
        or any(part in {"", ".", ".."} or ":" in part for part in path.parts)
        or any(part.rstrip(" .") != part for part in path.parts)
        or relative == CHECKSUM_MANIFEST_NAME
    ):
        raise ExportError(f"checksum manifest contains an unsafe relative path: {relative!r}")
    return path


def _checksum_rows(payload: bytes) -> list[tuple[str, str]]:
    if not payload.endswith(b"\n") or b"\r" in payload:
        raise ExportError("checksum manifest must be LF-terminated")
    lines = payload[:-1].split(b"\n")
    if not lines or lines == [b""]:
        raise ExportError("checksum manifest must contain at least one row")
    rows: list[tuple[str, str]] = []
    for line in lines:
        if len(line) < 67 or line[64:66] != b"  ":
            raise ExportError("checksum manifest row must be '<lowercase-sha256>  <relative-path>'")
        try:
            digest = line[:64].decode("ascii")
            relative = line[66:].decode("utf-8", errors="strict")
        except UnicodeDecodeError as error:
            raise ExportError(f"checksum manifest row encoding is invalid: {error}") from error
        if len(digest) != 64 or any(character not in "0123456789abcdef" for character in digest):
            raise ExportError("checksum manifest digest must be lowercase 64-hex")
        _safe_checksum_path(relative)
        rows.append((digest, relative))
    relative_paths = [relative for _digest, relative in rows]
    normalized = [os.path.normcase(relative) for relative in relative_paths]
    if relative_paths != sorted(relative_paths) or len(set(normalized)) != len(normalized):
        raise ExportError("checksum manifest paths must be sorted and unique")
    return rows


def _bundle_snapshot(destination: Path) -> _BundleSnapshot:
    root_path = os.path.abspath(os.fspath(destination))
    try:
        root_metadata = os.lstat(root_path)
    except OSError as error:
        raise ExportError(f"evaluation bundle destination cannot be classified: {error}") from error
    if _is_link_or_reparse(root_metadata) or not stat.S_ISDIR(root_metadata.st_mode):
        raise ExportError(f"evaluation bundle destination must be a real directory: {destination}")
    root_identity = _identity(root_metadata)
    files: dict[str, _DiscoveredFile] = {}
    normalized_paths: set[str] = set()
    total_bytes = 0
    for current, directory_names, file_names in os.walk(root_path, followlinks=False):
        directory_names.sort()
        file_names.sort()
        current_path = Path(current)
        for name in directory_names:
            path = current_path / name
            try:
                metadata = os.lstat(path)
            except OSError as error:
                raise ExportError(f"evaluation bundle directory cannot be classified: {path}: {error}") from error
            if _is_link_or_reparse(metadata) or not stat.S_ISDIR(metadata.st_mode):
                raise ExportError(f"evaluation bundle contains a linked/reparse directory: {path}")
        for name in file_names:
            path = current_path / name
            try:
                metadata = os.lstat(path)
            except OSError as error:
                raise ExportError(f"evaluation bundle file cannot be classified: {path}: {error}") from error
            if _is_link_or_reparse(metadata) or not stat.S_ISREG(metadata.st_mode):
                raise ExportError(f"evaluation bundle contains a non-regular file: {path}")
            relative = path.relative_to(destination).as_posix()
            if relative != CHECKSUM_MANIFEST_NAME:
                _safe_checksum_path(relative)
            normalized = os.path.normcase(relative)
            if normalized in normalized_paths:
                raise ExportError(f"evaluation bundle contains a duplicate normalized path: {relative}")
            normalized_paths.add(normalized)
            identity = _identity(metadata)
            total_bytes += identity.size
            if len(files) + 1 > _MAX_BUNDLE_FILE_COUNT:
                raise ExportError("evaluation bundle file count exceeds the configured limit")
            if total_bytes > _MAX_BUNDLE_BYTES:
                raise ExportError("evaluation bundle total bytes exceed the configured limit")
            components = tuple(relative.split("/"))
            files[relative] = _DiscoveredFile(
                relative_path=relative,
                absolute_path=os.path.abspath(os.fspath(path)),
                identity=identity,
                prefix_identities=_snapshot_prefixes(root_path, components),
            )
    _require_root_identity(root_path, root_identity, "after discovery")
    return _BundleSnapshot(root_path, root_identity, files)


def _hash_discovered_file(snapshot: _BundleSnapshot, discovered: _DiscoveredFile, *, limit: int) -> str:
    _payload, digest = _read_or_hash_discovered_file(snapshot, discovered, limit=limit, retain_payload=False)
    return digest


def _read_discovered_file(snapshot: _BundleSnapshot, discovered: _DiscoveredFile, *, limit: int) -> bytes:
    payload, _digest = _read_or_hash_discovered_file(snapshot, discovered, limit=limit, retain_payload=True)
    if payload is None:  # Defensive: retain_payload=True always returns exact chunks.
        raise ExportError(f"evaluation bundle member {discovered.relative_path} was not retained")
    return payload


def _read_or_hash_discovered_file(
    snapshot: _BundleSnapshot,
    discovered: _DiscoveredFile,
    *,
    limit: int,
    retain_payload: bool,
) -> tuple[bytes | None, str]:
    fd = _open_discovered_file(snapshot, discovered)
    chunks: list[bytes] = []
    digest = hashlib.sha256()
    byte_count = 0
    try:
        _require_root_identity(snapshot.root_path, snapshot.root_identity, "after open")
        _revalidate_prefixes(snapshot.root_path, discovered.prefix_identities, "after open")
        opened_metadata = os.fstat(fd)
        if not stat.S_ISREG(opened_metadata.st_mode) or _identity(opened_metadata) != discovered.identity:
            raise ExportError(f"evaluation bundle member {discovered.relative_path} changed identity before read")
        if int(opened_metadata.st_size) > limit:
            raise ExportError(f"evaluation bundle member {discovered.relative_path} exceeds the bounded read size")
        while True:
            try:
                chunk = os.read(fd, min(_READ_CHUNK_SIZE, limit + 1 - byte_count))
            except OSError as error:
                raise ExportError(
                    f"evaluation bundle member {discovered.relative_path} cannot be read stably: {error}"
                ) from error
            if not chunk:
                break
            byte_count += len(chunk)
            if byte_count > limit:
                raise ExportError(f"evaluation bundle member {discovered.relative_path} exceeds the bounded read size")
            digest.update(chunk)
            if retain_payload:
                chunks.append(chunk)
        after_open = os.fstat(fd)
        if _identity(after_open) != discovered.identity or byte_count != int(after_open.st_size):
            raise ExportError(f"evaluation bundle member {discovered.relative_path} changed while being read")
    finally:
        os.close(fd)

    _require_root_identity(snapshot.root_path, snapshot.root_identity, "after read")
    _revalidate_prefixes(snapshot.root_path, discovered.prefix_identities, "after read")
    try:
        after = os.lstat(discovered.absolute_path)
    except OSError as error:
        raise ExportError(
            f"evaluation bundle member {discovered.relative_path} disappeared after read: {error}"
        ) from error
    if (
        _is_link_or_reparse(after)
        or not stat.S_ISREG(after.st_mode)
        or _identity(after) != discovered.identity
    ):
        raise ExportError(f"evaluation bundle member {discovered.relative_path} changed identity after read")
    return (b"".join(chunks) if retain_payload else None), digest.hexdigest()


def _open_discovered_file(snapshot: _BundleSnapshot, discovered: _DiscoveredFile) -> int:
    _require_root_identity(snapshot.root_path, snapshot.root_identity, "before open")
    _revalidate_prefixes(snapshot.root_path, discovered.prefix_identities, "before open")
    if _can_use_openat():
        return _open_discovered_file_openat(snapshot, discovered)
    return _open_discovered_file_by_path(snapshot, discovered)


def _can_use_openat() -> bool:
    return (
        os.name != "nt"
        and _OPEN_SUPPORTS_DIR_FD
        and _STAT_SUPPORTS_DIR_FD
        and _STAT_SUPPORTS_NOFOLLOW
        and _O_DIRECTORY != 0
    )


def _open_discovered_file_openat(snapshot: _BundleSnapshot, discovered: _DiscoveredFile) -> int:
    directory_flags = os.O_RDONLY | _O_CLOEXEC | _O_DIRECTORY | _O_NOFOLLOW
    file_flags = os.O_RDONLY | _O_BINARY | _O_CLOEXEC | _O_NOFOLLOW
    components = tuple(discovered.relative_path.split("/"))
    try:
        current_fd = os.open(snapshot.root_path, directory_flags)
    except OSError as error:
        raise ExportError(f"evaluation bundle root cannot be opened without following links: {error}") from error
    try:
        root_metadata = os.fstat(current_fd)
        if not stat.S_ISDIR(root_metadata.st_mode) or _identity(root_metadata) != snapshot.root_identity:
            raise ExportError("evaluation bundle root identity changed before member open")
        for component, expected_prefix in zip(
            components[:-1], discovered.prefix_identities, strict=True
        ):
            try:
                next_fd = os.open(component, directory_flags, dir_fd=current_fd)
            except OSError as error:
                raise ExportError(
                    f"evaluation bundle path prefix {expected_prefix.relative_path} cannot be opened "
                    f"without following links: {error}"
                ) from error
            os.close(current_fd)
            current_fd = next_fd
            prefix_metadata = os.fstat(current_fd)
            if (
                not stat.S_ISDIR(prefix_metadata.st_mode)
                or _identity(prefix_metadata) != expected_prefix.identity
            ):
                raise ExportError(
                    f"evaluation bundle path prefix {expected_prefix.relative_path} changed identity before open"
                )
        try:
            fd = os.open(components[-1], file_flags, dir_fd=current_fd)
        except OSError as error:
            raise ExportError(
                f"evaluation bundle member {discovered.relative_path} cannot be opened without following links: {error}"
            ) from error
        metadata = os.fstat(fd)
        if not stat.S_ISREG(metadata.st_mode) or _identity(metadata) != discovered.identity:
            os.close(fd)
            raise ExportError(f"evaluation bundle member {discovered.relative_path} changed identity before open")
        return fd
    finally:
        os.close(current_fd)


def _open_discovered_file_by_path(snapshot: _BundleSnapshot, discovered: _DiscoveredFile) -> int:
    try:
        before = os.lstat(discovered.absolute_path)
    except OSError as error:
        raise ExportError(
            f"evaluation bundle member {discovered.relative_path} cannot be classified before open: {error}"
        ) from error
    if (
        _is_link_or_reparse(before)
        or not stat.S_ISREG(before.st_mode)
        or _identity(before) != discovered.identity
    ):
        raise ExportError(f"evaluation bundle member {discovered.relative_path} changed identity before open")
    flags = os.O_RDONLY | _O_BINARY | _O_CLOEXEC | _O_NOFOLLOW
    try:
        fd = os.open(discovered.absolute_path, flags)
    except OSError as error:
        raise ExportError(
            f"evaluation bundle member {discovered.relative_path} cannot be opened without following links: {error}"
        ) from error
    metadata = os.fstat(fd)
    if not stat.S_ISREG(metadata.st_mode) or _identity(metadata) != discovered.identity:
        os.close(fd)
        raise ExportError(f"evaluation bundle member {discovered.relative_path} changed identity before read")
    return fd


def _snapshot_prefixes(root_path: str, components: tuple[str, ...]) -> tuple[_PrefixIdentity, ...]:
    prefix_identities: list[_PrefixIdentity] = []
    prefix_path = root_path
    for index, component in enumerate(components[:-1]):
        prefix_path = os.path.abspath(os.path.join(prefix_path, component))
        prefix_relative = "/".join(components[: index + 1])
        try:
            metadata = os.lstat(prefix_path)
        except OSError as error:
            raise ExportError(f"evaluation bundle path prefix cannot be classified: {error}") from error
        if _is_link_or_reparse(metadata) or not stat.S_ISDIR(metadata.st_mode):
            raise ExportError(f"evaluation bundle path prefix {prefix_relative} is a link/reparse member")
        prefix_identities.append(_PrefixIdentity(prefix_relative, _identity(metadata)))
    return tuple(prefix_identities)


def _revalidate_prefixes(
    root_path: str,
    prefix_identities: tuple[_PrefixIdentity, ...],
    phase: str,
) -> None:
    for prefix in prefix_identities:
        absolute_path = os.path.abspath(os.path.join(root_path, *prefix.relative_path.split("/")))
        try:
            metadata = os.lstat(absolute_path)
        except OSError as error:
            raise ExportError(f"evaluation bundle path prefix disappeared {phase}: {error}") from error
        if (
            _is_link_or_reparse(metadata)
            or not stat.S_ISDIR(metadata.st_mode)
            or _identity(metadata) != prefix.identity
        ):
            raise ExportError(f"evaluation bundle path prefix {prefix.relative_path} changed identity {phase}")


def _require_root_identity(root_path: str, expected: _FileIdentity, phase: str) -> None:
    try:
        metadata = os.lstat(root_path)
    except OSError as error:
        raise ExportError(f"evaluation bundle root cannot be reclassified {phase}: {error}") from error
    if _is_link_or_reparse(metadata) or not stat.S_ISDIR(metadata.st_mode) or _identity(metadata) != expected:
        raise ExportError(f"evaluation bundle root identity changed {phase}")


def _identity(metadata: os.stat_result) -> _FileIdentity:
    return _FileIdentity(
        device=int(metadata.st_dev),
        inode=int(metadata.st_ino),
        mode_type=stat.S_IFMT(metadata.st_mode),
        size=int(metadata.st_size),
        modified_ns=int(metadata.st_mtime_ns),
    )


def verify_g2_evaluation_bundle(destination: Path) -> int:
    """Verify exact sorted checksum closure for one exported evaluation bundle."""

    destination = destination.absolute()
    snapshot = _bundle_snapshot(destination)
    manifest = snapshot.files.get(CHECKSUM_MANIFEST_NAME)
    if manifest is None:
        raise ExportError("evaluation bundle checksum manifest is missing")
    rows = _checksum_rows(
        _read_discovered_file(snapshot, manifest, limit=_MAX_CHECKSUM_MANIFEST_BYTES)
    )
    files = {relative: discovered for relative, discovered in snapshot.files.items() if relative != CHECKSUM_MANIFEST_NAME}
    claimed = {relative: digest for digest, relative in rows}
    if set(claimed) != set(files):
        raise ExportError(
            "checksum manifest file set mismatch: "
            f"missing={sorted(set(files) - set(claimed))!r} extra={sorted(set(claimed) - set(files))!r}"
        )
    for relative, expected in claimed.items():
        if _hash_discovered_file(snapshot, files[relative], limit=_MAX_FILE_BYTES) != expected:
            raise ExportError(f"checksum mismatch: {relative}")
    return len(files)


def export_g2_evaluation_bundle(destination: Path) -> EvaluationBundleExport:
    """Materialize, replay, and seal one offline synthetic evaluation bundle."""

    destination = _empty_destination(destination)
    bundle = write_g2_disk_bundle(destination)
    captured = capture_local_fixture(bundle.fixture_root)
    admission = admit_local_fixture(captured)
    if admission.status != "ADMITTED" or admission.reason_codes:
        raise ExportError(f"synthetic fixture admission failed: {admission.status} {admission.reason_codes}")
    context = load_replay_envelope_context(bundle.fixture_root, captured, admission)
    verified = build_verified_run_inputs(
        context.run_receipt_record,
        admission.candidates,
        admission.source_receipt_records,
        context.resolver,
        context.profiles,
    )
    result = run_offline_paper_kernel(verified, context.resolver, context.profiles)
    result_document = parse_canonical_json(kernel_result_canonical_bytes(result))
    result_payload = result_document.get("result")
    if not isinstance(result_payload, dict):
        raise ExportError("kernel result payload is invalid")
    projection_path = destination / KERNEL_PROJECTION_NAME
    projection_path.write_bytes(canonical_record_bytes(_projection_document(result_payload)))
    checksum_manifest = _write_checksum_manifest(destination)
    verify_g2_evaluation_bundle(destination)
    return EvaluationBundleExport(
        destination=destination,
        fixture_root=bundle.fixture_root,
        checksum_manifest=checksum_manifest,
        kernel_projection=projection_path,
    )


def _parse_args(argv: Sequence[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--verify", action="store_true")
    parser.add_argument("destination", type=Path)
    return parser.parse_args(argv)


def main(argv: Sequence[str] | None = None) -> int:
    args = _parse_args(sys.argv[1:] if argv is None else argv)
    try:
        if args.verify:
            verified_file_count = verify_g2_evaluation_bundle(args.destination)
            print(
                json.dumps(
                    {
                        "destination": str(args.destination.absolute()),
                        "status": "PASS",
                        "verified_file_count": verified_file_count,
                    },
                    separators=(",", ":"),
                    sort_keys=True,
                )
            )
            return 0
        exported = export_g2_evaluation_bundle(args.destination)
    except (ExportError, OSError, ValueError, AssertionError) as error:
        print(f"G2 evaluation export failed: {error}", file=sys.stderr)
        return 1
    print(
        json.dumps(
            {
                "checksum_manifest": str(exported.checksum_manifest),
                "build_finance_root": str(BUILD_FINANCE_ROOT),
                "destination": str(exported.destination),
                "fixture_root": str(exported.fixture_root),
                "kernel_projection": str(exported.kernel_projection),
            },
            separators=(",", ":"),
            sort_keys=True,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
