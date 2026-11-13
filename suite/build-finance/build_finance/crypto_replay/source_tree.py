"""Fail-closed, no-follow source-tree evidence for contract-only replay inputs."""

from __future__ import annotations

import hashlib
import os
import stat
import unicodedata
from collections.abc import Iterator, Mapping
from dataclasses import dataclass
from typing import Any

from build_finance.crypto_replay.canonical import JsonValue

_FILE_ATTRIBUTE_REPARSE_POINT = 0x400
_READ_CHUNK_SIZE = 1024 * 1024
_EXCLUSION_PREFIXES = (
    ".git/",
    ".hg/",
    ".mypy_cache/",
    ".nox/",
    ".pytest_cache/",
    ".ruff_cache/",
    ".svn/",
    ".tox/",
    ".venv/",
    "__pycache__/",
    "build/",
    "dist/",
    "node_modules/",
    "scratch/",
    "tmp/",
    "venv/",
)


class SourceTreeError(ValueError):
    """The source tree cannot be represented without following or racing an entry."""


@dataclass(frozen=True, slots=True)
class _FileIdentity:
    device: int
    inode: int
    mode_type: int
    size: int
    modified_ns: int


@dataclass(frozen=True, slots=True)
class _DiscoveredFile:
    relative_path: str
    absolute_path: str
    metadata: os.stat_result
    directory_chain: tuple[_DirectoryProof, ...]


@dataclass(frozen=True, slots=True)
class _DirectoryProof:
    absolute_path: str
    label: str
    identity: _FileIdentity


@dataclass(frozen=True, slots=True)
class _DirectoryEntryProof:
    relative_path: str
    identity: _FileIdentity


@dataclass(frozen=True, slots=True)
class _VisitedDirectoryProof:
    directory_chain: tuple[_DirectoryProof, ...]
    prefix: str
    entries: tuple[_DirectoryEntryProof, ...]


def scan_source_tree(root: str | os.PathLike[str], *, root_label: str) -> dict[str, JsonValue]:
    """Return canonical source-tree evidence without resolving or following protected paths."""
    if not isinstance(root_label, str) or not root_label:
        raise SourceTreeError("root label must be a non-empty string")
    root_path = os.fspath(root)
    if not isinstance(root_path, str) or not root_path:
        raise SourceTreeError("source-tree root must be a non-empty filesystem path")

    root_absolute = os.path.abspath(root_path)
    root_metadata = _lstat(root_absolute, "root")
    if _is_link_or_reparse(root_metadata):
        raise SourceTreeError("root is a symlink, junction, or reparse point; no-follow source tree rejected")
    if not stat.S_ISDIR(root_metadata.st_mode):
        raise SourceTreeError("root is not a directory")

    seen_paths: set[str] = set()
    visited_directories: list[_VisitedDirectoryProof] = []
    rows = [
        _file_row(discovered)
        for discovered in sorted(
            _walk(root_absolute, root_absolute, "", seen_paths, root_metadata, visited_directories),
            key=lambda item: item.relative_path.encode("utf-8"),
        )
    ]
    _require_stable_visited_directories(root_absolute, visited_directories)
    return {
        "schema": "trading.source-tree/v1",
        "root_label": root_label,
        "path_policy": "UTF8_NFC_POSIX_RELATIVE_NO_SYMLINK_V1",
        "exclusion_prefixes": list(_EXCLUSION_PREFIXES),
        "files": rows,
    }


def reconstruct_source_tree(source_tree: Mapping[str, JsonValue]) -> dict[str, JsonValue]:
    """Reconstruct and validate canonical source-tree evidence from a retained body."""
    if source_tree.get("schema") != "trading.source-tree/v1":
        raise SourceTreeError("source-tree schema is not trading.source-tree/v1")
    root_label = source_tree.get("root_label")
    if not isinstance(root_label, str) or not root_label:
        raise SourceTreeError("source-tree root_label must be a non-empty string")
    if source_tree.get("path_policy") != "UTF8_NFC_POSIX_RELATIVE_NO_SYMLINK_V1":
        raise SourceTreeError("source-tree path policy is not the no-follow UTF-8 NFC POSIX policy")
    if source_tree.get("exclusion_prefixes") != list(_EXCLUSION_PREFIXES):
        raise SourceTreeError("source-tree exclusion prefixes do not match the frozen policy")
    files_value = source_tree.get("files")
    if not isinstance(files_value, list):
        raise SourceTreeError("source-tree files must be an array")

    rows: list[dict[str, JsonValue]] = []
    seen_paths: set[str] = set()
    for index, row_value in enumerate(files_value):
        if not isinstance(row_value, Mapping):
            raise SourceTreeError(f"source-tree file row {index} is not an object")
        relative_path = row_value.get("relative_path")
        byte_length = row_value.get("byte_length")
        file_digest = row_value.get("file_sha256")
        if not isinstance(relative_path, str):
            raise SourceTreeError(f"source-tree file row {index} has no string relative_path")
        safe_path = _safe_relative_path(relative_path)
        if safe_path in seen_paths:
            raise SourceTreeError("source-tree relative_path values must be unique")
        seen_paths.add(safe_path)
        if not _is_canonical_u64_string(byte_length):
            raise SourceTreeError(f"{safe_path} byte_length is not a canonical u64 string")
        if not _is_sha256_string(file_digest):
            raise SourceTreeError(f"{safe_path} file_sha256 is not a lowercase SHA-256 digest")
        rows.append(
            {
                "relative_path": safe_path,
                "byte_length": byte_length,
                "file_sha256": file_digest,
            }
        )

    sorted_rows = sorted(rows, key=lambda row: str(row["relative_path"]).encode("utf-8"))
    if rows != sorted_rows:
        raise SourceTreeError("source-tree files are not sorted by unsigned UTF-8 path bytes")
    return {
        "schema": "trading.source-tree/v1",
        "root_label": root_label,
        "path_policy": "UTF8_NFC_POSIX_RELATIVE_NO_SYMLINK_V1",
        "exclusion_prefixes": list(_EXCLUSION_PREFIXES),
        "files": sorted_rows,
    }


def _walk(
    root_absolute: str,
    directory_absolute: str,
    prefix: str,
    seen_paths: set[str],
    expected_metadata: os.stat_result,
    visited_directories: list[_VisitedDirectoryProof],
    directory_chain: tuple[_DirectoryProof, ...] = (),
) -> Iterator[_DiscoveredFile]:
    _require_inside_root(root_absolute, directory_absolute)
    directory_label = prefix[:-1] if prefix else "root"
    expected_identity = _identity(expected_metadata, directory_label)
    _require_stable_directory(directory_absolute, directory_label, expected_identity)
    directory_proof = _DirectoryProof(
        absolute_path=directory_absolute,
        label=directory_label,
        identity=expected_identity,
    )
    current_chain = (*directory_chain, directory_proof)
    _require_stable_directory_chain(current_chain)
    try:
        entries = os.scandir(directory_absolute)
    except OSError as error:
        raise SourceTreeError(f"directory cannot be listed without stable access: {error}") from error
    try:
        _require_stable_directory_chain(current_chain)
        entry_proofs: list[_DirectoryEntryProof] = []
        with entries:
            for entry in entries:
                _require_stable_directory_chain(current_chain)
                name = _safe_component(entry.name)
                relative_path = f"{prefix}{name}"
                relative_directory = f"{relative_path}/"
                if _is_excluded_entry(relative_path, relative_directory):
                    continue
                if relative_path in seen_paths:
                    raise SourceTreeError("normalized relative path collision prevents stable source-tree identity")
                seen_paths.add(relative_path)

                absolute_path = os.path.abspath(entry.path)
                _require_inside_root(root_absolute, absolute_path)
                metadata = _lstat(absolute_path, relative_path)
                if _is_link_or_reparse(metadata):
                    raise SourceTreeError(
                        f"{relative_path} is a symlink, junction, or reparse point; no-follow required"
                    )
                entry_proofs.append(
                    _DirectoryEntryProof(
                        relative_path=relative_path,
                        identity=_identity(metadata, relative_path),
                    )
                )
                if stat.S_ISDIR(metadata.st_mode):
                    yield from _walk(
                        root_absolute,
                        absolute_path,
                        relative_directory,
                        seen_paths,
                        metadata,
                        visited_directories,
                        current_chain,
                    )
                elif stat.S_ISREG(metadata.st_mode):
                    yield _DiscoveredFile(
                        relative_path=relative_path,
                        absolute_path=absolute_path,
                        metadata=metadata,
                        directory_chain=current_chain,
                    )
                else:
                    raise SourceTreeError(f"{relative_path} is not a regular file or directory entry")
            _require_stable_directory_chain(current_chain)
            visited_directories.append(
                _VisitedDirectoryProof(
                    directory_chain=current_chain,
                    prefix=prefix,
                    entries=_sort_directory_entries(entry_proofs),
                )
            )
    finally:
        entries.close()


def _file_row(discovered: _DiscoveredFile) -> dict[str, JsonValue]:
    before = _identity(discovered.metadata, discovered.relative_path)
    _require_stable_directory_chain(discovered.directory_chain)
    flags = os.O_RDONLY | getattr(os, "O_BINARY", 0) | getattr(os, "O_CLOEXEC", 0)
    no_follow = getattr(os, "O_NOFOLLOW", 0)
    if no_follow:
        flags |= no_follow
    try:
        fd = os.open(discovered.absolute_path, flags)
    except OSError as error:
        raise SourceTreeError(f"{discovered.relative_path} cannot be read with stable no-follow access: {error}") from (
            error
        )
    try:
        _require_stable_directory_chain(discovered.directory_chain)
        opened_metadata = os.fstat(fd)
        if not stat.S_ISREG(opened_metadata.st_mode):
            raise SourceTreeError(f"{discovered.relative_path} changed to a non-regular entry before read")
        _require_same_identity(discovered.relative_path, before, _identity(opened_metadata, discovered.relative_path))

        digest = hashlib.sha256()
        byte_count = 0
        while True:
            try:
                chunk = os.read(fd, _READ_CHUNK_SIZE)
            except OSError as error:
                raise SourceTreeError(f"{discovered.relative_path} cannot be read stably: {error}") from error
            if not chunk:
                break
            byte_count += len(chunk)
            digest.update(chunk)

        after_open_metadata = os.fstat(fd)
        _require_stable_directory_chain(discovered.directory_chain)
        post_metadata = _lstat(discovered.absolute_path, discovered.relative_path)
        if _is_link_or_reparse(post_metadata) or not stat.S_ISREG(post_metadata.st_mode):
            raise SourceTreeError(f"{discovered.relative_path} changed entry type during source-tree scan")
        _require_same_identity(
            discovered.relative_path,
            before,
            _identity(after_open_metadata, discovered.relative_path),
        )
        _require_same_identity(discovered.relative_path, before, _identity(post_metadata, discovered.relative_path))
        _require_stable_directory_chain(discovered.directory_chain)
    finally:
        os.close(fd)

    if byte_count != before.size:
        raise SourceTreeError(f"{discovered.relative_path} changed size during source-tree scan")
    return {
        "relative_path": discovered.relative_path,
        "byte_length": str(byte_count),
        "file_sha256": digest.hexdigest(),
    }


def _safe_component(name: Any) -> str:
    if not isinstance(name, str):
        raise SourceTreeError("entry name is not a UTF-8 filesystem string")
    normalized = unicodedata.normalize("NFC", name)
    if (
        not normalized
        or normalized in {".", ".."}
        or "/" in normalized
        or "\\" in normalized
        or ":" in normalized
        or "\x00" in normalized
    ):
        raise SourceTreeError("entry name is not a safe POSIX relative path component")
    try:
        normalized.encode("utf-8", errors="strict")
    except UnicodeEncodeError as error:
        raise SourceTreeError("entry name is not strict UTF-8") from error
    return normalized


def _safe_relative_path(path: str) -> str:
    normalized = unicodedata.normalize("NFC", path)
    if normalized != path:
        raise SourceTreeError("source-tree relative paths must already be NFC-normalized")
    if normalized == ".git" or normalized.startswith(".git/"):
        raise SourceTreeError("source-tree cannot retain Git control entries")
    if normalized.startswith("/") or "\\" in normalized or ":" in normalized or "\x00" in normalized:
        raise SourceTreeError("source-tree path must be POSIX relative and cannot contain unsafe separators")
    if normalized.startswith("../") or normalized in {".", ".."}:
        raise SourceTreeError("source-tree path cannot escape the root")
    components = normalized.split("/")
    if any(component in {"", ".", ".."} for component in components):
        raise SourceTreeError("source-tree path components must be safe relative names")
    for prefix in _EXCLUSION_PREFIXES:
        if normalized.startswith(prefix):
            raise SourceTreeError("source-tree cannot retain excluded source-control or build/cache entries")
    try:
        normalized.encode("utf-8", errors="strict")
    except UnicodeEncodeError as error:
        raise SourceTreeError("source-tree path is not strict UTF-8") from error
    return normalized


def _is_excluded_entry(relative_path: str, relative_directory: str) -> bool:
    if relative_path == ".git":
        return True
    return any(relative_directory.startswith(prefix) or relative_path.startswith(prefix) for prefix in _EXCLUSION_PREFIXES)


def _lstat(path: str, label: str) -> os.stat_result:
    try:
        return os.lstat(path)
    except OSError as error:
        raise SourceTreeError(f"{label} cannot be classified without following links: {error}") from error


def _is_link_or_reparse(metadata: os.stat_result) -> bool:
    attributes = int(getattr(metadata, "st_file_attributes", 0))
    return stat.S_ISLNK(metadata.st_mode) or bool(attributes & _FILE_ATTRIBUTE_REPARSE_POINT)


def _identity(metadata: os.stat_result, label: str) -> _FileIdentity:
    try:
        return _FileIdentity(
            device=int(metadata.st_dev),
            inode=int(metadata.st_ino),
            mode_type=stat.S_IFMT(metadata.st_mode),
            size=int(metadata.st_size),
            modified_ns=int(metadata.st_mtime_ns),
        )
    except (AttributeError, TypeError, ValueError) as error:
        raise SourceTreeError(f"{label} stable identity cannot be proven") from error


def _require_same_identity(label: str, expected: _FileIdentity, observed: _FileIdentity) -> None:
    if observed != expected:
        raise SourceTreeError(f"{label} changed identity, size, or stability metadata during scan")


def _require_stable_directory(path: str, label: str, expected: _FileIdentity) -> None:
    metadata = _lstat(path, label)
    if _is_link_or_reparse(metadata) or not stat.S_ISDIR(metadata.st_mode):
        raise SourceTreeError(f"{label} changed to a symlink, junction, reparse point, or non-directory entry")
    _require_same_identity(label, expected, _identity(metadata, label))


def _require_stable_directory_chain(chain: tuple[_DirectoryProof, ...]) -> None:
    for proof in chain:
        _require_stable_directory(proof.absolute_path, proof.label, proof.identity)


def _require_stable_visited_directories(root_absolute: str, visited_directories: list[_VisitedDirectoryProof]) -> None:
    for visited_directory in visited_directories:
        directory = visited_directory.directory_chain[-1]
        _require_stable_directory_chain(visited_directory.directory_chain)
        observed_entries = _directory_entry_proofs(
            root_absolute,
            directory.absolute_path,
            directory.label,
            visited_directory.prefix,
        )
        _require_stable_directory_chain(visited_directory.directory_chain)
        if observed_entries != visited_directory.entries:
            raise SourceTreeError(f"{directory.label} changed source-tree entries during scan")


def _directory_entry_proofs(
    root_absolute: str,
    directory_absolute: str,
    directory_label: str,
    prefix: str,
) -> tuple[_DirectoryEntryProof, ...]:
    try:
        entries = os.scandir(directory_absolute)
    except OSError as error:
        raise SourceTreeError(f"{directory_label} cannot be listed with stable source-tree entries: {error}") from error
    proofs: list[_DirectoryEntryProof] = []
    seen_relative_paths: set[str] = set()
    try:
        with entries:
            for entry in entries:
                name = _safe_component(entry.name)
                relative_path = f"{prefix}{name}"
                relative_directory = f"{relative_path}/"
                if _is_excluded_entry(relative_path, relative_directory):
                    continue
                if relative_path in seen_relative_paths:
                    raise SourceTreeError("normalized relative path collision prevents stable source-tree identity")
                seen_relative_paths.add(relative_path)

                absolute_path = os.path.abspath(entry.path)
                _require_inside_root(root_absolute, absolute_path)
                metadata = _lstat(absolute_path, relative_path)
                if _is_link_or_reparse(metadata):
                    raise SourceTreeError(
                        f"{relative_path} is a symlink, junction, or reparse point; no-follow required"
                    )
                if not (stat.S_ISDIR(metadata.st_mode) or stat.S_ISREG(metadata.st_mode)):
                    raise SourceTreeError(f"{relative_path} is not a regular file or directory entry")
                proofs.append(
                    _DirectoryEntryProof(
                        relative_path=relative_path,
                        identity=_identity(metadata, relative_path),
                    )
                )
    finally:
        entries.close()
    return _sort_directory_entries(proofs)


def _sort_directory_entries(entries: list[_DirectoryEntryProof]) -> tuple[_DirectoryEntryProof, ...]:
    return tuple(sorted(entries, key=lambda entry: entry.relative_path.encode("utf-8")))


def _require_inside_root(root_absolute: str, candidate_absolute: str) -> None:
    try:
        common = os.path.commonpath((os.path.normcase(root_absolute), os.path.normcase(candidate_absolute)))
    except ValueError as error:
        raise SourceTreeError("entry path is outside the explicit source-tree root") from error
    if common != os.path.normcase(root_absolute):
        raise SourceTreeError("entry path is outside the explicit source-tree root")


def _is_canonical_u64_string(value: object) -> bool:
    if not isinstance(value, str):
        return False
    if value == "0":
        return True
    if not value or value[0] == "0" or not value.isdecimal():
        return False
    return int(value) <= 18_446_744_073_709_551_615


def _is_sha256_string(value: object) -> bool:
    return isinstance(value, str) and len(value) == 64 and all(character in "0123456789abcdef" for character in value)
