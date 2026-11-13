"""G2 RED confinement tests for the future offline live-paper package."""

from __future__ import annotations

import ast
import re
from pathlib import Path

PACKAGE_ROOT = Path("build_finance") / "live_paper"
PACKAGE_PREFIX = "build_finance.live_paper"
_URL_RE = re.compile(r"\b(?:https?|wss?)://", re.IGNORECASE)

DETERMINISTIC_STDLIB_IMPORT_PREFIXES = (
    "__future__",
    "collections",
    "collections.abc",
    "copy",
    "dataclasses",
    "decimal",
    "enum",
    "functools",
    "hashlib",
    "itertools",
    "json",
    "math",
    "operator",
    "re",
    "types",
    "typing",
)
ALLOWED_FROZEN_REPLAY_IMPORTS = (
    "build_finance.crypto_replay.admission",
    "build_finance.crypto_replay.canonical",
    "build_finance.crypto_replay.content_ids",
    "build_finance.crypto_replay.run_inputs",
    "build_finance.crypto_replay.schema_registry",
)
FORBIDDEN_BUILD_FINANCE_IMPORTS = (
    "build_finance.autotrader",
    "build_finance.backtest",
    "build_finance.broker",
    "build_finance.cli",
    "build_finance.crypto_replay",
    "build_finance.data",
    "build_finance.gui",
    "build_finance.market_data",
    "build_finance.orderbook",
    "build_finance.portfolio",
)
FORBIDDEN_CALLS = (
    ("builtins", "__import__"),
    ("builtins", "compile"),
    ("builtins", "eval"),
    ("builtins", "exec"),
    ("builtins", "open"),
    ("importlib", "__import__"),
    ("importlib", "import_module"),
    ("os", "getenv"),
    ("os", "popen"),
    ("os", "system"),
    ("pathlib.Path", "cwd"),
    ("pathlib.Path", "home"),
    ("pathlib.Path", "open"),
    ("pathlib.Path", "read_bytes"),
    ("pathlib.Path", "read_text"),
    ("pathlib.Path", "resolve"),
    ("pathlib.Path", "write_bytes"),
    ("pathlib.Path", "write_text"),
    ("runpy", "run_module"),
    ("runpy", "run_path"),
    ("socket", "create_connection"),
    ("socket", "socket"),
    ("subprocess", "Popen"),
    ("subprocess", "call"),
    ("subprocess", "check_call"),
    ("subprocess", "check_output"),
    ("subprocess", "run"),
    ("urllib.request", "urlopen"),
)


def _module_name(path: Path, package_root: Path = PACKAGE_ROOT, package_prefix: str = PACKAGE_PREFIX) -> str:
    relative_parts = path.relative_to(package_root).with_suffix("").parts
    if relative_parts[-1] == "__init__":
        relative_parts = relative_parts[:-1]
    return ".".join((package_prefix, *relative_parts)) if relative_parts else package_prefix


def _discovered_runtime_modules(
    package_root: Path = PACKAGE_ROOT,
    package_prefix: str = PACKAGE_PREFIX,
) -> tuple[tuple[str, Path], ...]:
    return tuple(
        sorted(
            (
                (_module_name(path, package_root, package_prefix), path)
                for path in package_root.rglob("*.py")
                if "__pycache__" not in path.parts
            ),
            key=lambda item: item[0].encode("utf-8"),
        )
    )


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


def _imported_modules(tree: ast.AST, module: str, path: Path) -> tuple[tuple[str, bool], ...]:
    modules: list[tuple[str, bool]] = []
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            modules.extend((alias.name, False) for alias in node.names)
        elif isinstance(node, ast.ImportFrom):
            imported = _resolve_import_from_module(module, path, node)
            if imported is None:
                continue
            modules.append((imported, False))
            allow_named_object = imported in ALLOWED_FROZEN_REPLAY_IMPORTS
            modules.extend(
                (f"{imported}.{alias.name}", allow_named_object) for alias in node.names if alias.name != "*"
            )
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


def _call_path_from_func(func: ast.AST, aliases: dict[str, str]) -> tuple[str | None, str | None]:
    if isinstance(func, ast.Name):
        resolved = aliases.get(func.id)
        if resolved is None:
            return "builtins", func.id
        module, _separator, function = resolved.rpartition(".")
        return module or resolved, function or None
    parts: list[str] = []
    current = func
    while isinstance(current, ast.Attribute):
        parts.append(current.attr)
        current = current.value
    if isinstance(current, ast.Name):
        root = aliases.get(current.id, current.id)
        path = ".".join((root, *reversed(parts)))
        module, _separator, function = path.rpartition(".")
        return module or path, function or None
    return None, None


def _call_path(node: ast.Call, aliases: dict[str, str]) -> tuple[str | None, str | None]:
    return _call_path_from_func(node.func, aliases)


def _is_allowed_import(
    module: str,
    package_prefix: str,
    *,
    allow_allowed_module_member: bool = False,
) -> bool:
    if module == package_prefix or module.startswith(f"{package_prefix}."):
        return True
    if module in ALLOWED_FROZEN_REPLAY_IMPORTS:
        return True
    if allow_allowed_module_member and _matches_module_prefix(module, ALLOWED_FROZEN_REPLAY_IMPORTS):
        return True
    return _matches_module_prefix(module, DETERMINISTIC_STDLIB_IMPORT_PREFIXES)


def _matches_module_prefix(module: str, prefixes: tuple[str, ...]) -> bool:
    return any(module == prefix or module.startswith(f"{prefix}.") for prefix in prefixes)


def _is_forbidden_build_finance_import(
    module: str,
    package_prefix: str,
    *,
    allow_allowed_module_member: bool = False,
) -> bool:
    if module == package_prefix or module.startswith(f"{package_prefix}."):
        return False
    if module in ALLOWED_FROZEN_REPLAY_IMPORTS:
        return False
    if allow_allowed_module_member and _matches_module_prefix(module, ALLOWED_FROZEN_REPLAY_IMPORTS):
        return False
    return module.startswith("build_finance.") or module in FORBIDDEN_BUILD_FINANCE_IMPORTS


class _ImportTimeCallVisitor(ast.NodeVisitor):
    def __init__(self, module: str, aliases: dict[str, str]) -> None:
        self._module = module
        self._aliases = aliases
        self.failures: list[str] = []

    def visit_Call(self, node: ast.Call) -> None:
        call_module, call_name = _call_path(node, self._aliases)
        self.failures.append(f"{self._module}:import-time call {call_module}.{call_name}")
        self.generic_visit(node)

    def visit_ClassDef(self, node: ast.ClassDef) -> None:
        for decorator in node.decorator_list:
            if not self._is_allowed_frozen_slotted_dataclass_decorator(decorator):
                self.visit(decorator)
        for base in node.bases:
            self.visit(base)
        for keyword in node.keywords:
            self.visit(keyword)
        for statement in node.body:
            self.visit(statement)

    def visit_FunctionDef(self, node: ast.FunctionDef) -> None:
        self._visit_function_signature(node)

    def visit_AsyncFunctionDef(self, node: ast.AsyncFunctionDef) -> None:
        self._visit_function_signature(node)

    def visit_Lambda(self, node: ast.Lambda) -> None:
        self._visit_arguments(node.args)

    def _is_allowed_frozen_slotted_dataclass_decorator(self, node: ast.expr) -> bool:
        if not isinstance(node, ast.Call):
            return False
        if _call_path_from_func(node.func, self._aliases) != ("dataclasses", "dataclass"):
            return False
        if node.args or len(node.keywords) != 2:
            return False
        literal_keywords: dict[str, bool] = {}
        for keyword in node.keywords:
            if keyword.arg not in {"frozen", "slots"} or not isinstance(keyword.value, ast.Constant):
                return False
            if keyword.value.value is not True:
                return False
            literal_keywords[keyword.arg] = True
        return literal_keywords == {"frozen": True, "slots": True}

    def _visit_function_signature(self, node: ast.AsyncFunctionDef | ast.FunctionDef) -> None:
        for decorator in node.decorator_list:
            self.visit(decorator)
        self._visit_arguments(node.args)
        if node.returns is not None:
            self.visit(node.returns)

    def _visit_arguments(self, arguments: ast.arguments) -> None:
        for default in (*arguments.defaults, *(item for item in arguments.kw_defaults if item is not None)):
            self.visit(default)
        for argument in (*arguments.posonlyargs, *arguments.args, *arguments.kwonlyargs):
            if argument.annotation is not None:
                self.visit(argument.annotation)
        if arguments.vararg is not None and arguments.vararg.annotation is not None:
            self.visit(arguments.vararg.annotation)
        if arguments.kwarg is not None and arguments.kwarg.annotation is not None:
            self.visit(arguments.kwarg.annotation)


def _import_time_call_failures(tree: ast.Module, module: str, aliases: dict[str, str]) -> tuple[str, ...]:
    visitor = _ImportTimeCallVisitor(module, aliases)
    visitor.visit(tree)
    return tuple(visitor.failures)


def _literal_url_failures(tree: ast.AST, module: str) -> tuple[str, ...]:
    failures: list[str] = []
    for node in ast.walk(tree):
        if isinstance(node, ast.Constant) and isinstance(node.value, str) and _URL_RE.search(node.value):
            failures.append(f"{module}:URL literal")
    return tuple(failures)


def _ast_confinement_failures(
    package_root: Path = PACKAGE_ROOT,
    package_prefix: str = PACKAGE_PREFIX,
) -> tuple[str, ...]:
    failures: list[str] = []
    for module, path in _discovered_runtime_modules(package_root, package_prefix):
        tree = ast.parse(path.read_bytes(), filename=str(path))
        aliases = _import_aliases(tree, module, path)
        for imported, allow_allowed_module_member in _imported_modules(tree, module, path):
            if _is_forbidden_build_finance_import(
                imported,
                package_prefix,
                allow_allowed_module_member=allow_allowed_module_member,
            ):
                failures.append(f"{module}:forbidden build_finance import {imported}")
            elif not _is_allowed_import(
                imported,
                package_prefix,
                allow_allowed_module_member=allow_allowed_module_member,
            ):
                failures.append(f"{module}:forbidden import {imported}")
        for node in ast.walk(tree):
            if isinstance(node, ast.Call) and _call_path(node, aliases) in FORBIDDEN_CALLS:
                call_module, call_name = _call_path(node, aliases)
                failures.append(f"{module}:forbidden call {call_module}.{call_name}")
        failures.extend(_import_time_call_failures(tree, module, aliases))
        failures.extend(_literal_url_failures(tree, module))
    return tuple(dict.fromkeys(failures))


def test_ast_scan_recursively_enumerates_future_modules_without_init_exports(tmp_path: Path) -> None:
    """The confinement scanner must catch hidden future modules and forbidden capabilities."""

    package_root = tmp_path / "isolated_live_paper"
    nested = package_root / "nested"
    nested.mkdir(parents=True)
    (package_root / "__init__.py").write_text('"""Empty package root with no submodule exports."""\n')
    (package_root / "kernel.py").write_text("KERNEL_MODE = 'OFFLINE_PAPER_ONLY'\n")
    (nested / "risk.py").write_text("from urllib.request import urlopen\n")
    (package_root / "side_effect.py").write_text("escaped = __import__('os')\n")

    discovered = tuple(module for module, _path in _discovered_runtime_modules(package_root, "isolated_live_paper"))
    failures = _ast_confinement_failures(package_root, "isolated_live_paper")

    assert discovered == (
        "isolated_live_paper",
        "isolated_live_paper.kernel",
        "isolated_live_paper.nested.risk",
        "isolated_live_paper.side_effect",
    )
    assert "isolated_live_paper.nested.risk:forbidden import urllib.request" in failures
    assert "isolated_live_paper.side_effect:forbidden call builtins.__import__" in failures
    assert "isolated_live_paper.side_effect:import-time call builtins.__import__" in failures


def test_ast_scan_denies_nondeterministic_stdlib_capability_modules(tmp_path: Path) -> None:
    """The scanner must reject stdlib capability modules outside a deterministic allowlist."""

    package_root = tmp_path / "isolated_live_paper"
    package_root.mkdir()
    (package_root / "__init__.py").write_text('"""Synthetic package for stdlib confinement."""\n')
    for module_name in ("smtplib", "imaplib", "webbrowser", "asyncio", "multiprocessing"):
        (package_root / f"{module_name}_touch.py").write_text(f"import {module_name}\n")

    failures = set(_ast_confinement_failures(package_root, "isolated_live_paper"))

    assert {
        "isolated_live_paper.asyncio_touch:forbidden import asyncio",
        "isolated_live_paper.imaplib_touch:forbidden import imaplib",
        "isolated_live_paper.multiprocessing_touch:forbidden import multiprocessing",
        "isolated_live_paper.smtplib_touch:forbidden import smtplib",
        "isolated_live_paper.webbrowser_touch:forbidden import webbrowser",
    }.issubset(failures)


def test_ast_scan_flags_import_time_calls_in_control_class_decorator_and_defaults(tmp_path: Path) -> None:
    """Import-time call detection must recurse through import-reachable syntax while skipping function bodies."""

    package_root = tmp_path / "isolated_live_paper"
    package_root.mkdir()
    (package_root / "__init__.py").write_text('"""Synthetic package for import-time calls."""\n')
    (package_root / "top_if.py").write_text(
        "if guard():\n"
        "    top_level_value = danger()\n"
        "\n"
        "def not_import_time():\n"
        "    return body_call()\n"
    )
    (package_root / "class_body.py").write_text(
        "class PaperState:\n"
        "    class_value = class_body_call()\n"
        "\n"
        "    def method(self):\n"
        "        return method_body_call()\n"
    )
    (package_root / "decorator_default.py").write_text(
        "@decorator_factory()\n"
        "def decide(quantity=default_quantity()):\n"
        "    return function_body_call(quantity)\n"
    )
    (package_root / "dataclass_assignment.py").write_text(
        "from dataclasses import dataclass\n"
        "Carrier = dataclass(frozen=True, slots=True)(object)\n"
    )
    (package_root / "dataclass_control_flow.py").write_text(
        "from dataclasses import dataclass\n"
        "if enabled:\n"
        "    dataclass(frozen=True, slots=True)\n"
    )
    (package_root / "dataclass_empty_decorator.py").write_text(
        "from dataclasses import dataclass\n"
        "\n"
        "@dataclass()\n"
        "class Carrier:\n"
        "    value: int\n"
    )
    (package_root / "dataclass_mutable_decorator.py").write_text(
        "from dataclasses import dataclass\n"
        "\n"
        "@dataclass(frozen=False, slots=True)\n"
        "class Carrier:\n"
        "    value: int\n"
    )

    failures = set(_ast_confinement_failures(package_root, "isolated_live_paper"))

    assert {
        "isolated_live_paper.class_body:import-time call builtins.class_body_call",
        "isolated_live_paper.dataclass_assignment:import-time call dataclasses.dataclass",
        "isolated_live_paper.dataclass_control_flow:import-time call dataclasses.dataclass",
        "isolated_live_paper.dataclass_empty_decorator:import-time call dataclasses.dataclass",
        "isolated_live_paper.dataclass_mutable_decorator:import-time call dataclasses.dataclass",
        "isolated_live_paper.decorator_default:import-time call builtins.decorator_factory",
        "isolated_live_paper.decorator_default:import-time call builtins.default_quantity",
        "isolated_live_paper.top_if:import-time call builtins.danger",
        "isolated_live_paper.top_if:import-time call builtins.guard",
    }.issubset(failures)
    assert "isolated_live_paper.class_body:import-time call builtins.method_body_call" not in failures
    assert "isolated_live_paper.decorator_default:import-time call builtins.function_body_call" not in failures
    assert "isolated_live_paper.top_if:import-time call builtins.body_call" not in failures


def test_frozen_replay_import_boundary_allows_only_audited_contract_modules(tmp_path: Path) -> None:
    """Only the frozen replay contract modules required by G2 live-paper production are permitted."""

    for module in ALLOWED_FROZEN_REPLAY_IMPORTS:
        assert not _is_forbidden_build_finance_import(module, PACKAGE_PREFIX)

    for module in (
        "build_finance.crypto_replay",
        "build_finance.crypto_replay.admission.unlisted_submodule",
        "build_finance.crypto_replay.canonical.provider_escape",
        "build_finance.crypto_replay.jupiter_fixture",
        "build_finance.crypto_replay.local_fixture",
        "build_finance.crypto_replay.schema_definitions",
        "build_finance.crypto_replay.source_tree",
        "build_finance.market_data",
        "build_finance.broker",
        "build_finance.broker.AlpacaBroker",
    ):
        assert _is_forbidden_build_finance_import(module, PACKAGE_PREFIX)

    allowed_package_root = tmp_path / "allowed_live_paper"
    allowed_package_root.mkdir()
    (allowed_package_root / "__init__.py").write_text(
        '"""Synthetic package for allowed frozen replay imports."""\n'
    )
    (allowed_package_root / "allowed.py").write_text(
        "from build_finance.crypto_replay.admission import ParsedSourceCandidate\n"
        "from build_finance.crypto_replay.canonical import canonical_json_bytes\n"
        "from build_finance.crypto_replay.content_ids import compute_content_id\n"
        "from build_finance.crypto_replay.run_inputs import ContractVerifiedRunInputs\n"
        "from build_finance.crypto_replay.schema_registry import require_valid_contract\n"
    )

    assert _ast_confinement_failures(allowed_package_root, "isolated_live_paper") == ()

    escaped_package_root = tmp_path / "escaped_live_paper"
    escaped_package_root.mkdir()
    (escaped_package_root / "__init__.py").write_text(
        '"""Synthetic package for escaped frozen replay imports."""\n'
    )
    (escaped_package_root / "escaped.py").write_text(
        "import build_finance.crypto_replay.admission.unlisted_submodule\n"
        "from build_finance.crypto_replay.canonical.provider_escape import Provider\n"
    )

    failures = set(_ast_confinement_failures(escaped_package_root, "isolated_live_paper"))

    assert {
        (
            "isolated_live_paper.escaped:forbidden build_finance import "
            "build_finance.crypto_replay.admission.unlisted_submodule"
        ),
        (
            "isolated_live_paper.escaped:forbidden build_finance import "
            "build_finance.crypto_replay.canonical.provider_escape"
        ),
    }.issubset(failures)


def test_ast_scan_allows_frozen_slotted_dataclass_carrier(tmp_path: Path) -> None:
    """A deterministic frozen/slotted dataclass carrier is definition-time structure, not capability."""

    package_root = tmp_path / "isolated_live_paper"
    package_root.mkdir()
    (package_root / "__init__.py").write_text('"""Synthetic package for dataclass confinement."""\n')
    (package_root / "carrier.py").write_text(
        "from dataclasses import dataclass\n"
        "\n"
        "@dataclass(frozen=True, slots=True)\n"
        "class Carrier:\n"
        "    value: int\n"
    )

    assert _ast_confinement_failures(package_root, "isolated_live_paper") == ()


def test_live_paper_runtime_ast_denies_forbidden_imports_calls_urls_and_side_effects() -> None:
    """Future G2 package must stay offline, stdlib-only, paper-only, and import-safe."""

    assert PACKAGE_ROOT.is_dir(), "missing G2 production package root: build_finance/live_paper"
    assert _ast_confinement_failures() == ()


def test_live_paper_gate_recorder_normalizes_repo_paths_and_only_pytest_elapsed_time(tmp_path: Path) -> None:
    """The RED receipt script keeps semantic output while normalizing path/timing noise."""

    from scripts.capture_live_paper_gate import _normalize_output

    semantic_output = (
        f"{tmp_path}\\tests\\live_paper\\test_example.py::test_example FAILED\n"
        "E model deadline duration remains semantic: expected 25ns, got 30ns\n"
        "FAILED tests/live_paper/test_example.py::test_example - AssertionError\n"
    )
    first = _normalize_output(f"{semantic_output}5 failed, 3 passed in 0.25s\n".encode(), tmp_path)
    second = _normalize_output(f"{semantic_output}5 failed, 3 passed in 0.26s\n".encode(), tmp_path)

    assert first == second
    assert "<repo>\\tests\\live_paper\\test_example.py::test_example FAILED" in first
    assert "expected 25ns, got 30ns" in first
    assert first.endswith("5 failed, 3 passed in <pytest-duration>\n")
