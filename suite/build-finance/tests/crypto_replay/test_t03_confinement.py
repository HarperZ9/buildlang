"""T03 crypto-replay confinement tests for offline-only imports and calls."""

from __future__ import annotations

import ast
import json
import subprocess
import sys
from pathlib import Path

PACKAGE_ROOT = Path("build_finance") / "crypto_replay"
PACKAGE_PREFIX = "build_finance.crypto_replay"
FORBIDDEN_IMPORT_PREFIXES = (
    "aiohttp",
    "alpaca",
    "alpaca_trade_api",
    "anchorpy",
    "base58",
    "binance",
    "ccxt",
    "cffi",
    "coinbase",
    "ctypes",
    "ftplib",
    "http",
    "ibapi",
    "jupiter",
    "kraken",
    "native",
    "okx",
    "requests",
    "solders",
    "solana",
    "socket",
    "ssl",
    "urllib",
    "wallet",
    "web3",
    "websocket",
    "websockets",
)
FORBIDDEN_BUILD_FINANCE_IMPORTS = (
    "build_finance.autotrader",
    "build_finance.broker",
    "build_finance.market_data",
)
FORBIDDEN_CALLS = (
    ("builtins", "__import__"),
    ("builtins", "compile"),
    ("builtins", "eval"),
    ("builtins", "exec"),
    ("importlib", "__import__"),
    ("importlib", "import_module"),
    ("os", "execl"),
    ("os", "execle"),
    ("os", "execlp"),
    ("os", "execlpe"),
    ("os", "execv"),
    ("os", "execve"),
    ("os", "execvp"),
    ("os", "execvpe"),
    ("os", "popen"),
    ("os", "spawnl"),
    ("os", "spawnle"),
    ("os", "spawnlp"),
    ("os", "spawnlpe"),
    ("os", "spawnv"),
    ("os", "spawnve"),
    ("os", "spawnvp"),
    ("os", "spawnvpe"),
    ("os", "startfile"),
    ("os", "system"),
    ("runpy", "run_module"),
    ("runpy", "run_path"),
    ("subprocess", "Popen"),
    ("subprocess", "call"),
    ("subprocess", "check_call"),
    ("subprocess", "check_output"),
    ("subprocess", "run"),
)


def _runtime_module_paths(
    package_root: Path = PACKAGE_ROOT,
    package_prefix: str = PACKAGE_PREFIX,
) -> dict[str, Path]:
    return {
        _module_name(path, package_root, package_prefix): path
        for path in sorted(package_root.rglob("*.py"), key=lambda item: item.as_posix().encode("utf-8"))
    }


def _module_name(path: Path, package_root: Path = PACKAGE_ROOT, package_prefix: str = PACKAGE_PREFIX) -> str:
    relative_parts = path.relative_to(package_root).with_suffix("").parts
    if relative_parts[-1] == "__init__":
        relative_parts = relative_parts[:-1]
    return ".".join((package_prefix, *relative_parts)) if relative_parts else package_prefix


def _package_context(module: str, path: Path) -> str:
    if path.name == "__init__.py":
        return module
    package, _separator, _name = module.rpartition(".")
    return package


def _resolve_import_from_module(module: str, path: Path, node: ast.ImportFrom) -> str | None:
    if node.level == 0:
        return node.module
    package_parts = _package_context(module, path).split(".")
    if node.level > len(package_parts):
        return None
    base_parts = package_parts[: len(package_parts) - (node.level - 1)]
    if node.module:
        base_parts.extend(node.module.split("."))
    return ".".join(base_parts)


def _imported_modules(tree: ast.AST, module: str, path: Path) -> tuple[str, ...]:
    modules: list[str] = []
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            modules.extend(alias.name for alias in node.names)
        elif isinstance(node, ast.ImportFrom):
            imported = _resolve_import_from_module(module, path, node)
            if imported is None:
                continue
            modules.append(imported)
            modules.extend(f"{imported}.{alias.name}" for alias in node.names if alias.name != "*")
    return tuple(modules)


def _import_aliases(tree: ast.AST, module: str, path: Path) -> dict[str, str]:
    aliases: dict[str, str] = {}
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            for alias in node.names:
                aliases[alias.asname or alias.name.split(".", 1)[0]] = alias.name
        elif isinstance(node, ast.ImportFrom):
            imported = _resolve_import_from_module(module, path, node)
            if imported is None:
                continue
            for alias in node.names:
                if alias.name == "*":
                    continue
                aliases[alias.asname or alias.name] = f"{imported}.{alias.name}"
    return aliases


def _call_path(node: ast.Call, aliases: dict[str, str]) -> tuple[str | None, str | None]:
    if isinstance(node.func, ast.Name):
        resolved = aliases.get(node.func.id)
        if resolved is None:
            return "builtins", node.func.id
        module, _, function = resolved.rpartition(".")
        return module or resolved, function or None
    parts: list[str] = []
    current: ast.AST = node.func
    while isinstance(current, ast.Attribute):
        parts.append(current.attr)
        current = current.value
    if isinstance(current, ast.Name):
        root = aliases.get(current.id, current.id)
        path = ".".join((root, *reversed(parts)))
        module, _, function = path.rpartition(".")
        return module or path, function or None
    return None, None


def _discovered_runtime_modules(
    package_root: Path = PACKAGE_ROOT,
    package_prefix: str = PACKAGE_PREFIX,
) -> tuple[tuple[str, Path], ...]:
    return tuple(
        sorted(
            _runtime_module_paths(package_root, package_prefix).items(),
            key=lambda item: item[0].encode("utf-8"),
        )
    )


def _is_forbidden_module(module: str) -> bool:
    return module in FORBIDDEN_BUILD_FINANCE_IMPORTS or any(
        module == prefix or module.startswith(f"{prefix}.") for prefix in FORBIDDEN_IMPORT_PREFIXES
    )


def _ast_forbidden_capability_failures(
    package_root: Path = PACKAGE_ROOT,
    package_prefix: str = PACKAGE_PREFIX,
) -> tuple[str, ...]:
    failures: list[str] = []
    for module, path in _discovered_runtime_modules(package_root, package_prefix):
        tree = ast.parse(path.read_bytes(), filename=str(path))
        aliases = _import_aliases(tree, module, path)
        for imported in _imported_modules(tree, module, path):
            if _is_forbidden_module(imported):
                failures.append(f"{module}:{imported}")
        for node in ast.walk(tree):
            if isinstance(node, ast.Call) and _call_path(node, aliases) in FORBIDDEN_CALLS:
                call_module, call_name = _call_path(node, aliases)
                failures.append(f"{module}:{call_module}.{call_name}")
    return tuple(failures)


def test_ast_scan_enumerates_future_unimported_submodules_and_catches_forbidden_calls(tmp_path: Path) -> None:
    package_root = tmp_path / "isolated_crypto_replay"
    package_root.mkdir()
    (package_root / "__init__.py").write_text('"""Empty package root that intentionally imports no submodules."""\n')
    (package_root / "admission.py").write_text("ADMISSION_MODE = 'offline-only'\n")
    (package_root / "jupiter_fixture.py").write_text("import os\nos.system('SYNTHETIC_AST_FORBIDDEN_CALL')\n")
    (package_root / "local_fixture.py").write_text("LOCAL_FIXTURE_VERSION = '2026-08-20'\n")

    discovered = tuple(module for module, _path in _discovered_runtime_modules(package_root, "isolated_crypto_replay"))

    assert discovered == (
        "isolated_crypto_replay",
        "isolated_crypto_replay.admission",
        "isolated_crypto_replay.jupiter_fixture",
        "isolated_crypto_replay.local_fixture",
    )
    assert _ast_forbidden_capability_failures(package_root, "isolated_crypto_replay") == (
        "isolated_crypto_replay.jupiter_fixture:os.system",
    )


def test_crypto_replay_runtime_ast_denies_direct_and_transitive_forbidden_capabilities() -> None:
    assert _ast_forbidden_capability_failures() == ()


def test_clean_process_imports_crypto_replay_without_transitive_live_or_external_dependencies() -> None:
    script = f"""
import importlib
import json
import pathlib
import pkgutil
import sys

baseline_modules = set(sys.modules)
import build_finance.crypto_replay as package

modules = [package.__name__]
modules.extend(info.name for info in pkgutil.walk_packages(package.__path__, package.__name__ + "."))
for name in sorted(modules):
    importlib.import_module(name)
forbidden_prefixes = {FORBIDDEN_IMPORT_PREFIXES!r}
forbidden_exact = {FORBIDDEN_BUILD_FINANCE_IMPORTS!r}
forbidden = sorted(
    name for name in sys.modules
    if name not in baseline_modules
    and (name in forbidden_exact or any(name == prefix or name.startswith(prefix + ".") for prefix in forbidden_prefixes))
)
print(json.dumps({{"forbidden": forbidden, "modules": sorted(modules)}}, sort_keys=True))
"""
    completed = subprocess.run(
        [sys.executable, "-c", script],
        check=True,
        text=True,
        capture_output=True,
    )
    result = json.loads(completed.stdout)
    assert result["forbidden"] == []
    assert "build_finance.crypto_replay.admission" in result["modules"]
    assert "build_finance.crypto_replay.jupiter_fixture" in result["modules"]
    assert "build_finance.crypto_replay.local_fixture" in result["modules"]
