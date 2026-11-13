"""Build the explicit offline G2 paper-core allowlist into a local wheel/sdist."""

from __future__ import annotations

import argparse
import gzip
import hashlib
import io
import json
import os
import subprocess
import sys
import tarfile
import tempfile
from collections.abc import Mapping, Sequence
from pathlib import Path, PurePosixPath
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
MANIFEST_PATH = ROOT / "build_finance" / "live_paper" / "paper_core_manifest.json"
SOURCE_DATE_EPOCH = 946_684_800


class BuildError(RuntimeError):
    """The paper-core source allowlist or build result is invalid."""


def _canonical_json_bytes(value: object) -> bytes:
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"), sort_keys=True).encode("utf-8")


def _sha256(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()


def _load_manifest() -> dict[str, Any]:
    payload = MANIFEST_PATH.read_bytes()
    if not payload.endswith(b"\n"):
        raise BuildError("paper-core manifest must end with one LF")
    manifest = json.loads(payload)
    if not isinstance(manifest, dict) or _canonical_json_bytes(manifest) + b"\n" != payload:
        raise BuildError("paper-core manifest must be canonical JSON")
    claimed = manifest.get("manifest_sha256")
    unsigned = dict(manifest)
    unsigned.pop("manifest_sha256", None)
    if claimed != _sha256(_canonical_json_bytes(unsigned)):
        raise BuildError("paper-core manifest digest does not match its canonical payload")
    return manifest


def _validate_source_path(raw_path: object) -> Path:
    if not isinstance(raw_path, str):
        raise BuildError(f"manifest path is not a string: {raw_path!r}")
    path = PurePosixPath(raw_path)
    if path.is_absolute() or ".." in path.parts or not path.parts:
        raise BuildError(f"unsafe manifest path: {raw_path!r}")
    if path.parts[0] != "build_finance":
        raise BuildError(f"manifest source is outside build_finance: {raw_path}")
    allowed_root_sources = {"build_finance/__init__.py", "build_finance/paper_core_loader.py"}
    if len(path.parts) == 2 and path.as_posix() not in allowed_root_sources:
        raise BuildError(f"manifest source is not the package root marker: {raw_path}")
    if len(path.parts) > 2 and path.parts[1] not in {"crypto_replay", "live_paper"}:
        raise BuildError(f"manifest source escapes paper-core packages: {raw_path}")
    return Path(*path.parts)


def _copy_verified_sources(staging: Path, manifest: Mapping[str, Any]) -> tuple[str, ...]:
    rows = manifest.get("files")
    if not isinstance(rows, list) or not rows:
        raise BuildError("paper-core manifest files must be a non-empty list")
    copied: list[str] = []
    seen: set[str] = set()
    for row in rows:
        if not isinstance(row, dict):
            raise BuildError("paper-core manifest file row must be an object")
        relative = _validate_source_path(row.get("path"))
        relative_name = relative.as_posix()
        if relative_name in seen:
            raise BuildError(f"duplicate manifest source path: {relative_name}")
        seen.add(relative_name)
        source = ROOT / relative
        payload = source.read_bytes()
        if row.get("sha256") != _sha256(payload):
            raise BuildError(f"source digest mismatch: {relative.as_posix()}")
        destination = staging / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(payload)
        copied.append(relative_name)

    manifest_destination = staging / MANIFEST_PATH.relative_to(ROOT)
    manifest_destination.parent.mkdir(parents=True, exist_ok=True)
    manifest_destination.write_bytes(MANIFEST_PATH.read_bytes())
    return tuple(copied)


def _write_build_metadata(staging: Path, manifest: Mapping[str, Any]) -> None:
    version = manifest.get("distribution_version")
    if not isinstance(version, str):
        raise BuildError("paper-core manifest distribution_version must be a string")
    pyproject = f'''[build-system]
requires = ["setuptools>=68.0"]
build-backend = "setuptools.build_meta"

[project]
name = "build-finance-paper-core"
version = "{version}"
description = "Deterministic offline-only Build Finance G2 paper core"
readme = "README.md"
requires-python = ">=3.10"
dependencies = ["numpy>=1.24", "pandas>=2.0", "scipy>=1.10"]

[project.optional-dependencies]
test = ["pytest>=8.0", "pytest-cov>=5", "jsonschema>=4.23,<5"]
dev = ["pytest>=8.0", "pytest-cov>=5", "jsonschema>=4.23,<5", "ruff>=0.6", "mypy>=1.10", "build>=1.2"]

[tool.setuptools.packages.find]
include = ["build_finance", "build_finance.crypto_replay", "build_finance.live_paper"]

[tool.setuptools.package-data]
"build_finance.crypto_replay" = ["resources/*.json", "resources/*.sha256", "resources/schemas/*.json", "resources/formulas/*.txt"]
"build_finance.live_paper" = ["paper_core_manifest.json", "resources/*.json", "resources/*.sha256", "resources/schemas/*.json"]
'''
    (staging / "pyproject.toml").write_text(pyproject, encoding="utf-8", newline="\n")
    (staging / "README.md").write_text(
        "# Build Finance Paper Core\n\nOffline deterministic G2 paper-only artifact. No live execution path.\n",
        encoding="utf-8",
        newline="\n",
    )


def _canonicalize_sdist(path: Path) -> None:
    """Normalize backend-created tar/gzip metadata without changing payloads."""

    members: list[tuple[tarfile.TarInfo, bytes | None]] = []
    with tarfile.open(path, "r:gz") as source:
        for member in source:
            if not (member.isdir() or member.isfile()):
                raise BuildError(f"sdist contains non-regular member: {member.name}")
            payload = None
            if member.isfile():
                fileobj = source.extractfile(member)
                if fileobj is None:
                    raise BuildError(f"sdist member could not be read: {member.name}")
                payload = fileobj.read()
            member.mtime = SOURCE_DATE_EPOCH
            member.uid = 0
            member.gid = 0
            member.uname = ""
            member.gname = ""
            member.pax_headers = {}
            members.append((member, payload))

    temporary = path.with_name(f".{path.name}.canonical")
    with temporary.open("wb") as raw:
        with gzip.GzipFile(filename="", mode="wb", fileobj=raw, mtime=SOURCE_DATE_EPOCH) as compressed:
            with tarfile.open(fileobj=compressed, mode="w", format=tarfile.PAX_FORMAT) as destination:
                for member, payload in members:
                    destination.addfile(member, None if payload is None else io.BytesIO(payload))
    temporary.replace(path)


def build_artifacts(out_dir: Path) -> tuple[Path, Path]:
    manifest = _load_manifest()
    out_dir = out_dir.resolve()
    out_dir.mkdir(parents=True, exist_ok=True)
    for existing in (*out_dir.glob("*.whl"), *out_dir.glob("*.tar.gz")):
        existing.unlink()
    with tempfile.TemporaryDirectory(prefix="build-finance-paper-core-") as temporary:
        staging = Path(temporary)
        _copy_verified_sources(staging, manifest)
        _write_build_metadata(staging, manifest)
        for path in staging.rglob("*"):
            os.utime(path, (SOURCE_DATE_EPOCH, SOURCE_DATE_EPOCH))
        environment = os.environ.copy()
        environment.update(
            {
                "PIP_DISABLE_PIP_VERSION_CHECK": "1",
                "PIP_NO_INDEX": "1",
                "PYTHONDONTWRITEBYTECODE": "1",
                "SOURCE_DATE_EPOCH": str(SOURCE_DATE_EPOCH),
            }
        )
        completed = subprocess.run(
            [sys.executable, "-m", "build", "--no-isolation", "--outdir", str(out_dir)],
            cwd=staging,
            env=environment,
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
        )
        if completed.returncode:
            raise BuildError(f"standard build backend failed ({completed.returncode}):\n{completed.stdout}")
    wheels = tuple(out_dir.glob("*.whl"))
    sdists = tuple(out_dir.glob("*.tar.gz"))
    if len(wheels) != 1 or len(sdists) != 1:
        raise BuildError(f"expected one wheel and one sdist, found {wheels!r}, {sdists!r}")
    _canonicalize_sdist(sdists[0])
    return wheels[0], sdists[0]


def _parse_args(argv: Sequence[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out-dir", required=True, type=Path)
    return parser.parse_args(argv)


def main(argv: Sequence[str] | None = None) -> int:
    args = _parse_args(sys.argv[1:] if argv is None else argv)
    try:
        wheel, sdist = build_artifacts(args.out_dir)
    except (BuildError, OSError, json.JSONDecodeError) as error:
        print(f"paper-core build failed: {error}", file=sys.stderr)
        return 1
    print(
        json.dumps(
            {
                "sdist": str(sdist),
                "sdist_sha256": _sha256(sdist.read_bytes()),
                "wheel": str(wheel),
                "wheel_sha256": _sha256(wheel.read_bytes()),
            },
            separators=(",", ":"),
            sort_keys=True,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
