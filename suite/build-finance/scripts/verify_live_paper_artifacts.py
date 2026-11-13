"""Verify the dedicated G2 paper-core artifact pair and import closure."""

from __future__ import annotations

import argparse
import ast
import base64
import csv
import hashlib
import io
import json
import re
import subprocess
import sys
from dataclasses import asdict, dataclass
from email.parser import Parser
from pathlib import Path, PurePosixPath
from typing import Any

try:
    from scripts.verify_crypto_replay_artifacts import (
        ArchiveMembers,
        VerificationError,
        _read_sdist_members,
        _read_wheel_members,
    )
except ModuleNotFoundError:  # Direct ``python scripts/...`` execution.
    from verify_crypto_replay_artifacts import (
        ArchiveMembers,
        VerificationError,
        _read_sdist_members,
        _read_wheel_members,
    )

ROOT = Path(__file__).resolve().parents[1]
MANIFEST_PATH = ROOT / "build_finance" / "live_paper" / "paper_core_manifest.json"


def _distribution_version() -> str:
    value = json.loads(MANIFEST_PATH.read_bytes()).get("distribution_version")
    if not isinstance(value, str) or not value.strip():
        raise VerificationError("paper-core manifest distribution_version is invalid")
    return value


PAPER_CORE_VERSION = _distribution_version()
PAPER_CORE_DIST_INFO = f"build_finance_paper_core-{PAPER_CORE_VERSION}.dist-info"
EXPECTED_REQUIREMENTS = frozenset({"numpy>=1.24", "pandas>=2.0", "scipy>=1.10"})
FORBIDDEN_IMPORT_PREFIXES = (
    "aiohttp",
    "alpaca",
    "binance",
    "ccxt",
    "coinbase",
    "ftplib",
    "http",
    "ibapi",
    "importlib",
    "kraken",
    "_socket",
    "requests",
    "runpy",
    "socket",
    "ssl",
    "subprocess",
    "urllib",
    "wallet",
    "web3",
    "websocket",
    "websockets",
)
WHEEL_METADATA = frozenset(
    {
        f"{PAPER_CORE_DIST_INFO}/METADATA",
        f"{PAPER_CORE_DIST_INFO}/RECORD",
        f"{PAPER_CORE_DIST_INFO}/WHEEL",
        f"{PAPER_CORE_DIST_INFO}/top_level.txt",
    }
)
SDIST_METADATA = frozenset(
    {
        "PKG-INFO",
        "README.md",
        "build_finance_paper_core.egg-info/PKG-INFO",
        "build_finance_paper_core.egg-info/SOURCES.txt",
        "build_finance_paper_core.egg-info/dependency_links.txt",
        "build_finance_paper_core.egg-info/requires.txt",
        "build_finance_paper_core.egg-info/top_level.txt",
        "pyproject.toml",
        "setup.cfg",
    }
)
FORBIDDEN_BUILD_FINANCE_PREFIXES = (
    "build_finance.autotrader",
    "build_finance.broker",
    "build_finance.market_data",
)
FORBIDDEN_OS_CALLS = frozenset(
    {
        "environ",
        "execl",
        "execle",
        "execlp",
        "execlpe",
        "execv",
        "execve",
        "execvp",
        "execvpe",
        "getenv",
        "popen",
        "spawnl",
        "spawnle",
        "spawnlp",
        "spawnlpe",
        "spawnv",
        "spawnve",
        "spawnvp",
        "spawnvpe",
        "startfile",
        "system",
    }
)


@dataclass(frozen=True, slots=True)
class VerificationReport:
    """Machine-readable paper-core artifact verification result."""

    allowed_source_count: int
    import_closure: str
    manifest_sha256: str
    sdist: str
    sdist_member_count: int
    sdist_sha256: str
    wheel: str
    wheel_member_count: int
    wheel_sha256: str


CRITICAL_IMPLEMENTATION_PATHS = (
    ".github/workflows/ci.yml",
    "build_finance/paper_core_loader.py",
    "build_finance/crypto_replay",
    "build_finance/live_paper",
    "pyproject.toml",
    "scripts/build_paper_core_artifacts.py",
    "scripts/capture_live_paper_gate.py",
    "scripts/capture_paper_core_gate.py",
    "scripts/export_g2_evaluation_bundle.py",
    "scripts/run_network_denied.py",
    "scripts/verify_crypto_replay_artifacts.py",
    "scripts/verify_live_paper_artifacts.py",
    "tests/live_paper",
)
PAPER_CORE_WHEEL = (
    f".artifacts/paper-core/dist/build_finance_paper_core-{PAPER_CORE_VERSION}-py3-none-any.whl"
)
PAPER_CORE_SDIST = f".artifacts/paper-core/dist/build_finance_paper_core-{PAPER_CORE_VERSION}.tar.gz"
PRESCRIBED_GATE_COMMANDS = (
    ("python", "-m", "pytest", "tests/live_paper", "tests/crypto_replay", "-q", "-p", "no:cacheprovider"),
    (
        "python",
        "scripts/run_network_denied.py",
        "--pytest",
        "tests/live_paper",
        "tests/crypto_replay",
        "-q",
        "-p",
        "no:cacheprovider",
    ),
    ("python", "scripts/run_network_denied.py", "--import", "build_finance.live_paper.kernel"),
    ("python", "-m", "build_finance.crypto_replay.schema_codegen", "--scope", "full", "--check"),
    ("python", "-m", "build_finance.live_paper.registry", "--check"),
    (
        "python",
        "-m",
        "ruff",
        "check",
        "build_finance/paper_core_loader.py",
        "build_finance/live_paper",
        "tests/live_paper",
        "scripts/capture_live_paper_gate.py",
        "scripts/capture_paper_core_gate.py",
        "scripts/verify_live_paper_artifacts.py",
        "scripts/run_network_denied.py",
        "scripts/build_paper_core_artifacts.py",
        "scripts/export_g2_evaluation_bundle.py",
    ),
    (
        "python",
        "-m",
        "mypy",
        "--follow-imports=skip",
        "build_finance/paper_core_loader.py",
        "build_finance/live_paper",
        "scripts/export_g2_evaluation_bundle.py",
    ),
    ("python", "scripts/build_paper_core_artifacts.py", "--out-dir", ".artifacts/paper-core/dist"),
    ("python", "scripts/verify_crypto_replay_artifacts.py", "--wheel", PAPER_CORE_WHEEL, "--sdist", PAPER_CORE_SDIST),
    ("python", "scripts/verify_live_paper_artifacts.py", "--wheel", PAPER_CORE_WHEEL, "--sdist", PAPER_CORE_SDIST),
    ("git", "diff", "--check"),
)


def _sha256(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()


def _canonical_json_bytes(value: object) -> bytes:
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"), sort_keys=True).encode("utf-8")


def _load_canonical_json(path: Path) -> dict[str, Any]:
    payload = path.read_bytes()
    if not payload.endswith(b"\n") or b"\r" in payload:
        raise VerificationError(f"evidence JSON must be LF-terminated canonical JSON: {path}")
    value = json.loads(payload)
    if not isinstance(value, dict) or _canonical_json_bytes(value) + b"\n" != payload:
        raise VerificationError(f"evidence JSON is not canonical: {path}")
    return value


def _relative_artifact_path(path: Path, repo_root: Path) -> str:
    try:
        return path.resolve().relative_to(repo_root.resolve()).as_posix()
    except ValueError as error:
        raise VerificationError(f"gate artifact must be beneath the repository: {path}") from error


AUTHORIZATION_KEYS = frozenset({"authorized_at", "authorized_by", "implementation_sha", "node"})


def _validate_authorization(implementation_sha: str, authorization: object) -> dict[str, str]:
    """Fail closed unless the authorization record is complete and bound to this build."""

    if not isinstance(authorization, dict) or set(authorization) != AUTHORIZATION_KEYS:
        raise VerificationError("gate authorization fields are invalid")
    for key in ("authorized_at", "authorized_by", "node"):
        value = authorization[key]
        if not isinstance(value, str) or not value.strip():
            raise VerificationError(f"gate authorization {key} is invalid")
    recorded = authorization["implementation_sha"]
    if not isinstance(recorded, str) or re.fullmatch(r"[0-9a-f]{40}", recorded) is None:
        raise VerificationError("gate authorization implementation_sha is invalid")
    if recorded != implementation_sha:
        raise VerificationError("gate authorization is bound to a different implementation")
    return {key: authorization[key] for key in sorted(authorization)}


def _derived_promotion(
    implementation_sha: str,
    receipt_path: str,
    authorization: dict[str, str] | None = None,
) -> dict[str, object]:
    promotion: dict[str, object] = {
        "artifact": "build-finance-paper-core",
        "evidence_receipt": receipt_path,
        "g2": "GREEN",
        "implementation_sha": implementation_sha,
        "live_readiness": "BLOCKED",
        "model_inference": "DISABLED_ABSTAIN",
        "next_authorized_node": None,
        "paper_only": True,
        "profitability_claim": False,
        "publication": "BLOCKED",
    }
    if authorization is not None:
        promotion["authorization"] = _validate_authorization(implementation_sha, authorization)
        promotion["next_authorized_node"] = authorization["node"]
        promotion["publication"] = "AUTHORIZED"
    return promotion


def verify_gate_evidence(
    *,
    receipt_path: Path,
    promotion_path: Path,
    transcript_path: Path,
    wheel: Path,
    sdist: Path,
    manifest_sha256: str,
    repo_root: Path = ROOT,
    verify_git: bool = True,
) -> None:
    """Verify GREEN is derived from captured output and the current local artifacts."""

    receipt = _load_canonical_json(receipt_path)
    promotion = _load_canonical_json(promotion_path)
    transcript_payload = transcript_path.read_bytes()
    transcript = _load_canonical_json(transcript_path)
    implementation_sha = receipt.get("implementation_sha")
    if not isinstance(implementation_sha, str) or re.fullmatch(r"[0-9a-f]{40}", implementation_sha) is None:
        raise VerificationError("gate receipt has an invalid implementation_sha")
    transcript_ref = receipt.get("transcript")
    expected_transcript_path = _relative_artifact_path(transcript_path, repo_root)
    if transcript_ref != {"path": expected_transcript_path, "sha256": _sha256(transcript_payload)}:
        raise VerificationError("gate receipt transcript path or digest is stale")
    transcript_commands = transcript.get("commands")
    if transcript.get("implementation_sha") != implementation_sha or not isinstance(transcript_commands, list):
        raise VerificationError("gate transcript implementation or commands are invalid")
    command_args = [row.get("command_args") if isinstance(row, dict) else None for row in transcript_commands]
    if command_args != [list(command) for command in PRESCRIBED_GATE_COMMANDS]:
        raise VerificationError("gate transcript does not contain the exact prescribed Task 13 command list")
    verified_commands: list[dict[str, object]] = []
    for index, row in enumerate(transcript_commands):
        if not isinstance(row, dict) or not isinstance(row.get("output"), str):
            raise VerificationError(f"gate transcript command row {index} is invalid")
        output_digest = _sha256(row["output"].encode("utf-8"))
        if row.get("stdout_sha256") != output_digest:
            raise VerificationError(f"gate transcript command row {index} output digest is invalid")
        command_args = row.get("command_args")
        exit_code = row.get("exit_code")
        if not isinstance(command_args, list) or not all(isinstance(arg, str) for arg in command_args):
            raise VerificationError(f"gate transcript command row {index} command_args are invalid")
        if exit_code != 0:
            raise VerificationError(f"gate transcript command row {index} did not pass: exit_code={exit_code!r}")
        verified_commands.append(
            {"command_args": command_args, "exit_code": exit_code, "stdout_sha256": output_digest}
        )
    if receipt.get("commands") != verified_commands or not verified_commands:
        raise VerificationError("gate receipt command evidence differs from captured transcript")
    artifact_ref = receipt.get("artifacts")
    expected_artifacts = {
        "sdist": {
            "path": _relative_artifact_path(sdist, repo_root),
            "sha256": _sha256(sdist.read_bytes()),
        },
        "wheel": {
            "path": _relative_artifact_path(wheel, repo_root),
            "sha256": _sha256(wheel.read_bytes()),
        },
    }
    if artifact_ref != expected_artifacts:
        raise VerificationError("gate receipt artifact paths or digests are stale")
    receipt_relative = _relative_artifact_path(receipt_path, repo_root)
    recorded_promotion = receipt.get("promotion_status")
    authorization = (
        recorded_promotion.get("authorization") if isinstance(recorded_promotion, dict) else None
    )
    expected_promotion = _derived_promotion(implementation_sha, receipt_relative, authorization)
    if receipt.get("paper_core_manifest_sha256") != manifest_sha256:
        raise VerificationError("gate receipt paper-core manifest digest is stale")
    if receipt.get("status") != "GREEN" or receipt.get("promotion_status") != expected_promotion:
        raise VerificationError("gate receipt GREEN/promotion state is not derived from passing evidence")
    if promotion != expected_promotion:
        raise VerificationError("promotion status differs from the derived gate state")
    if verify_git:
        ancestor = subprocess.run(
            ["git", "merge-base", "--is-ancestor", implementation_sha, "HEAD"],
            cwd=repo_root,
            check=False,
        )
        unchanged = subprocess.run(
            ["git", "diff", "--quiet", implementation_sha, "HEAD", "--", *CRITICAL_IMPLEMENTATION_PATHS],
            cwd=repo_root,
            check=False,
        )
        if ancestor.returncode != 0 or unchanged.returncode != 0:
            raise VerificationError("gate receipt implementation_sha is stale for critical package-gate paths")


def _load_expected_manifest() -> tuple[bytes, dict[str, Any], dict[str, str]]:
    payload = MANIFEST_PATH.read_bytes()
    if not payload.endswith(b"\n"):
        raise VerificationError("source paper-core manifest is not LF terminated")
    manifest = json.loads(payload)
    if not isinstance(manifest, dict) or _canonical_json_bytes(manifest) + b"\n" != payload:
        raise VerificationError("source paper-core manifest is not canonical JSON")
    unsigned = dict(manifest)
    claimed = unsigned.pop("manifest_sha256", None)
    if claimed != _sha256(_canonical_json_bytes(unsigned)):
        raise VerificationError("source paper-core manifest digest is invalid")
    rows = manifest.get("files")
    if not isinstance(rows, list):
        raise VerificationError("source paper-core manifest files is not a list")
    expected: dict[str, str] = {}
    for row in rows:
        if not isinstance(row, dict) or not isinstance(row.get("path"), str) or not isinstance(row.get("sha256"), str):
            raise VerificationError("source paper-core manifest contains an invalid file row")
        path = row["path"]
        if path in expected:
            raise VerificationError(f"source paper-core manifest contains duplicate path: {path}")
        expected[path] = row["sha256"]
    expected["build_finance/live_paper/paper_core_manifest.json"] = _sha256(payload)
    return payload, manifest, expected


def _package_members(archive: ArchiveMembers) -> dict[str, bytes]:
    return {name: payload for name, payload in archive.members.items() if name.startswith("build_finance/")}


def _verify_exact_sources(archive: ArchiveMembers, expected: dict[str, str], manifest_payload: bytes) -> None:
    actual = _package_members(archive)
    if actual.keys() != expected.keys():
        raise VerificationError(
            f"{archive.label} package allowlist mismatch: "
            f"missing={sorted(expected.keys() - actual.keys())!r} extra={sorted(actual.keys() - expected.keys())!r}"
        )
    manifest_name = "build_finance/live_paper/paper_core_manifest.json"
    if actual[manifest_name] != manifest_payload:
        raise VerificationError(f"{archive.label} packaged manifest differs from reviewed source manifest")
    for name, expected_digest in expected.items():
        if _sha256(actual[name]) != expected_digest:
            raise VerificationError(f"{archive.label} source digest mismatch: {name}")


def _verify_complete_member_sets(
    wheel: ArchiveMembers,
    sdist: ArchiveMembers,
    expected: dict[str, str],
) -> None:
    """Reject every archive member outside the reviewed sources and narrow metadata."""

    for archive, metadata in ((wheel, WHEEL_METADATA), (sdist, SDIST_METADATA)):
        allowed = set(expected) | set(metadata)
        actual = set(archive.members)
        if actual != allowed:
            raise VerificationError(
                f"{archive.label} archive allowlist mismatch: "
                f"missing={sorted(allowed - actual)!r} extra={sorted(actual - allowed)!r}"
            )


def _verify_wheel_record(wheel: ArchiveMembers) -> None:
    """Validate RECORD coverage, sizes, and sha256 digests for the complete wheel."""

    record_names = [name for name in wheel.members if name.endswith(".dist-info/RECORD")]
    expected_record = f"{PAPER_CORE_DIST_INFO}/RECORD"
    if record_names != [expected_record]:
        raise VerificationError(f"wheel must contain the expected RECORD member, found {record_names!r}")
    record_name = record_names[0]
    try:
        rows = list(csv.reader(io.StringIO(wheel.members[record_name].decode("utf-8", errors="strict"))))
    except (UnicodeDecodeError, csv.Error) as error:
        raise VerificationError(f"wheel RECORD is invalid: {error}") from error
    indexed: dict[str, tuple[str, str]] = {}
    for row in rows:
        if len(row) != 3:
            raise VerificationError(f"wheel RECORD row must have three fields: {row!r}")
        path, digest, size = row
        if path in indexed:
            raise VerificationError(f"wheel RECORD contains duplicate path: {path}")
        if not path or PurePosixPath(path).is_absolute() or ".." in PurePosixPath(path).parts:
            raise VerificationError(f"wheel RECORD contains unsafe path: {path!r}")
        indexed[path] = (digest, size)
    if set(indexed) != set(wheel.members):
        raise VerificationError(
            "wheel RECORD member set mismatch: "
            f"missing={sorted(set(wheel.members) - set(indexed))!r} "
            f"extra={sorted(set(indexed) - set(wheel.members))!r}"
        )
    for path, payload in wheel.members.items():
        digest, size = indexed[path]
        if path == record_name:
            if digest or size:
                raise VerificationError("wheel RECORD self-row must have empty digest and size")
            continue
        expected_digest = base64.urlsafe_b64encode(hashlib.sha256(payload).digest()).rstrip(b"=").decode("ascii")
        if digest != f"sha256={expected_digest}":
            raise VerificationError(f"wheel RECORD digest mismatch: {path}")
        if size != str(len(payload)):
            raise VerificationError(f"wheel RECORD size mismatch: {path}")


def _module_name(path: str) -> str:
    pure = PurePosixPath(path)
    parts = pure.with_suffix("").parts
    if parts[-1] == "__init__":
        parts = parts[:-1]
    return ".".join(parts)


def _is_forbidden_import(name: str) -> bool:
    prefixes = (*FORBIDDEN_IMPORT_PREFIXES, *FORBIDDEN_BUILD_FINANCE_PREFIXES)
    return any(name == prefix or name.startswith(f"{prefix}.") for prefix in prefixes)


def _expression_path(node: ast.AST, aliases: dict[str, str]) -> str | None:
    parts: list[str] = []
    current = node
    while isinstance(current, ast.Attribute):
        parts.append(current.attr)
        current = current.value
    if not isinstance(current, ast.Name):
        return None
    root = aliases.get(current.id, current.id)
    return ".".join((root, *reversed(parts)))


def _is_capability_alias(path: str) -> bool:
    root = path.split(".", 1)[0]
    return root in {"__builtins__", "builtins", "importlib", "os", "runpy"} or _is_forbidden_import(path)


def _resolve_import_from(module: str, source_name: str, node: ast.ImportFrom) -> str | None:
    if node.level == 0:
        return node.module
    package = module if source_name.endswith("/__init__.py") else module.rpartition(".")[0]
    parts = package.split(".")
    if node.level > len(parts):
        return None
    base = parts[: len(parts) - (node.level - 1)]
    if node.module:
        base.extend(node.module.split("."))
    return ".".join(base)


def _import_aliases(tree: ast.AST, module: str, source_name: str) -> dict[str, str]:
    aliases: dict[str, str] = {}
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            for alias in node.names:
                aliases[alias.asname or alias.name.split(".", 1)[0]] = alias.name
        elif isinstance(node, ast.ImportFrom):
            imported = _resolve_import_from(module, source_name, node)
            if imported is None:
                continue
            for alias in node.names:
                if alias.name != "*":
                    aliases[alias.asname or alias.name] = f"{imported}.{alias.name}"
    assignments: list[tuple[ast.Name, ast.AST]] = []
    for node in ast.walk(tree):
        if isinstance(node, ast.Assign):
            assignments.extend((target, node.value) for target in node.targets if isinstance(target, ast.Name))
        elif (
            isinstance(node, (ast.AnnAssign, ast.NamedExpr))
            and isinstance(node.target, ast.Name)
            and node.value is not None
        ):
            assignments.append((node.target, node.value))
    changed = True
    while changed:
        changed = False
        for target, value in assignments:
            resolved = _expression_path(value, aliases)
            if resolved is not None and _is_capability_alias(resolved) and aliases.get(target.id) != resolved:
                aliases[target.id] = resolved
                changed = True
    return aliases


def _verify_import_target(name: str, imported: str, modules: set[str], *, member: bool = False) -> None:
    if _is_forbidden_import(imported):
        raise VerificationError(f"wheel imports forbidden capability: {name}:{imported}")
    if not imported.startswith("build_finance"):
        return
    if imported in modules:
        return
    if member:
        base, _separator, _symbol = imported.rpartition(".")
        if base in modules:
            return
    raise VerificationError(f"wheel internal import is outside allowlist: {name}:{imported}")


def _verify_ast_closure(wheel: ArchiveMembers) -> None:
    sources = _package_members(wheel)
    python_sources = {name: payload for name, payload in sources.items() if name.endswith(".py")}
    modules = {_module_name(name) for name in python_sources}
    for name, payload in sorted(python_sources.items()):
        tree = ast.parse(payload, filename=name)
        module = _module_name(name)
        aliases = _import_aliases(tree, module, name)
        for node in ast.walk(tree):
            if isinstance(node, ast.Import):
                for alias in node.names:
                    _verify_import_target(name, alias.name, modules)
            elif isinstance(node, ast.ImportFrom):
                imported = _resolve_import_from(module, name, node)
                if imported is None:
                    raise VerificationError(f"wheel contains invalid relative import: {name}")
                _verify_import_target(name, imported, modules)
                for alias in node.names:
                    if alias.name != "*":
                        _verify_import_target(name, f"{imported}.{alias.name}", modules, member=True)
            if isinstance(node, ast.Call):
                call_path = _expression_path(node.func, aliases)
                if call_path in {
                    "__builtins__.__import__",
                    "__builtins__.compile",
                    "__builtins__.eval",
                    "__builtins__.exec",
                    "builtins.__import__",
                    "builtins.compile",
                    "builtins.eval",
                    "builtins.exec",
                    "importlib.__import__",
                    "importlib.import_module",
                    "runpy.run_module",
                    "runpy.run_path",
                } or (isinstance(node.func, ast.Name) and node.func.id in {"__import__", "compile", "eval", "exec"}):
                    raise VerificationError(f"wheel calls dynamic execution/import capability: {name}:{call_path}")
                if call_path and call_path.startswith("os.") and call_path.removeprefix("os.") in FORBIDDEN_OS_CALLS:
                    raise VerificationError(f"wheel calls forbidden OS capability: {name}:{call_path}")
            if isinstance(node, (ast.Attribute, ast.Name)) and _expression_path(node, aliases) == "os.environ":
                raise VerificationError(f"wheel reads process environment: {name}:os.environ")


def _verify_metadata(wheel: ArchiveMembers) -> None:
    candidates = [name for name in wheel.members if name.endswith(".dist-info/METADATA")]
    if len(candidates) != 1:
        raise VerificationError(f"wheel must contain exactly one METADATA member, found {candidates!r}")
    metadata = Parser().parsestr(wheel.members[candidates[0]].decode("utf-8", errors="strict"))
    if metadata.get("Name") != "build-finance-paper-core":
        raise VerificationError(f"unexpected paper-core distribution name: {metadata.get('Name')!r}")
    requires = metadata.get_all("Requires-Dist") or []
    unconditional = frozenset(
        re.sub(r"\s+", "", requirement).lower() for requirement in requires if ";" not in requirement
    )
    if unconditional != EXPECTED_REQUIREMENTS:
        raise VerificationError(f"unexpected paper-core runtime requirements: {sorted(unconditional)!r}")
    for requirement in requires:
        normalized = requirement.lower()
        if any(token in normalized for token in FORBIDDEN_IMPORT_PREFIXES):
            raise VerificationError(f"forbidden paper-core requirement: {requirement}")


def verify_artifacts(*, wheel: Path, sdist: Path) -> VerificationReport:
    """Assert exact package closure for one local paper-core artifact pair."""

    manifest_payload, manifest, expected = _load_expected_manifest()
    wheel_members = _read_wheel_members(wheel)
    sdist_members = _read_sdist_members(sdist)
    _verify_complete_member_sets(wheel_members, sdist_members, expected)
    _verify_exact_sources(wheel_members, expected, manifest_payload)
    _verify_exact_sources(sdist_members, expected, manifest_payload)
    _verify_wheel_record(wheel_members)
    _verify_metadata(wheel_members)
    _verify_ast_closure(wheel_members)
    return VerificationReport(
        allowed_source_count=len(expected),
        import_closure="PASS",
        manifest_sha256=str(manifest["manifest_sha256"]),
        sdist=str(sdist),
        sdist_member_count=len(sdist_members.members),
        sdist_sha256=_sha256(sdist.read_bytes()),
        wheel=str(wheel),
        wheel_member_count=len(wheel_members.members),
        wheel_sha256=_sha256(wheel.read_bytes()),
    )


def _parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--wheel", required=True, type=Path)
    parser.add_argument("--sdist", required=True, type=Path)
    parser.add_argument("--receipt", type=Path)
    parser.add_argument("--promotion-status", type=Path)
    parser.add_argument("--transcript", type=Path)
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = _parse_args(sys.argv[1:] if argv is None else argv)
    try:
        report = verify_artifacts(wheel=args.wheel, sdist=args.sdist)
        evidence_paths = (args.receipt, args.promotion_status, args.transcript)
        if any(evidence_paths):
            if not all(evidence_paths):
                raise VerificationError("--receipt, --promotion-status, and --transcript must be supplied together")
            verify_gate_evidence(
                receipt_path=args.receipt,
                promotion_path=args.promotion_status,
                transcript_path=args.transcript,
                wheel=args.wheel,
                sdist=args.sdist,
                manifest_sha256=report.manifest_sha256,
            )
    except (OSError, json.JSONDecodeError, subprocess.SubprocessError, SyntaxError, VerificationError) as error:
        print(f"live-paper artifact verification failed: {error}", file=sys.stderr)
        return 1
    print(json.dumps(asdict(report), separators=(",", ":"), sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
