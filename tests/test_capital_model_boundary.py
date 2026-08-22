from __future__ import annotations

import ast
import importlib
import sys
from pathlib import Path
from types import SimpleNamespace

import pytest

CAPITAL_ROOT = Path(__file__).parents[1] / "build_engine" / "capital"
FORBIDDEN_NAMES = {
    "AutoTrader",
    "ModelTrainer",
    "PerformanceTracker",
    "PredictionStrategy",
}
FORBIDDEN_MODULE_PARTS = {
    "autotrader",
    "broker",
    "market_data",
    "model_trainer",
    "network",
    "performance_tracker",
    "prediction_strategy",
    "provider",
    "wallet",
}


def test_capital_sources_exclude_model_execution_and_external_capital_modules() -> None:
    for path in CAPITAL_ROOT.glob("*.py"):
        tree = ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
        imported_modules = {
            alias.name
            for node in ast.walk(tree)
            if isinstance(node, ast.Import)
            for alias in node.names
        }
        imported_modules.update(
            node.module or "" for node in ast.walk(tree) if isinstance(node, ast.ImportFrom)
        )
        names = {node.id for node in ast.walk(tree) if isinstance(node, ast.Name)}

        assert not names.intersection(FORBIDDEN_NAMES)
        assert not any(
            forbidden in module.split(".")
            for module in imported_modules
            for forbidden in FORBIDDEN_MODULE_PARTS
        )


def test_build_engine_cli_import_does_not_import_build_finance(monkeypatch: pytest.MonkeyPatch) -> None:
    for name in tuple(sys.modules):
        if name == "build_engine.cli" or name.startswith("build_finance"):
            monkeypatch.delitem(sys.modules, name, raising=False)

    importlib.import_module("build_engine.cli")

    assert not any(name.startswith("build_finance") for name in sys.modules)


def test_missing_paper_core_fails_before_fixture_access(monkeypatch: pytest.MonkeyPatch) -> None:
    from build_engine.capital import MissingCapitalDependencyError
    from build_engine.capital.supervisor import CapitalSupervisor

    fixture_touched = False

    class FixtureProbe:
        def __fspath__(self) -> str:
            nonlocal fixture_touched
            fixture_touched = True
            return "must-not-be-touched"

    def missing_core() -> object:
        raise MissingCapitalDependencyError("install with: pip install 'build-engine[capital]'")

    with pytest.raises(MissingCapitalDependencyError, match=r"build-engine\[capital\]"):
        CapitalSupervisor(core_loader=missing_core).replay(FixtureProbe())

    assert fixture_touched is False


def test_capital_package_exposes_only_evidence_replay_contracts() -> None:
    from build_engine import capital

    assert capital.CapitalSupervisor
    assert capital.CapitalReplayResult
    assert capital.EvidenceSummary
    assert capital.ModelObservation
    assert capital.MissingCapitalDependencyError


def test_capital_replay_cli_prints_offline_result_identifiers(
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
) -> None:
    from build_engine import capital
    from build_engine.cli import main

    class FakeSupervisor:
        def replay(self, fixture: object) -> object:
            return SimpleNamespace(
                projection_id="projection-1",
                closure_id="closure-1",
                closure_status="CLOSED",
                receipt_ids=("run-1", "closure-1"),
            )

    monkeypatch.setattr(capital, "CapitalSupervisor", FakeSupervisor)

    main(["capital-replay", "--fixture", "local-fixture"])

    output = capsys.readouterr().out
    assert "Projection ID : projection-1" in output
    assert "Closure ID    : closure-1" in output
    assert "Receipt IDs   : run-1, closure-1" in output
