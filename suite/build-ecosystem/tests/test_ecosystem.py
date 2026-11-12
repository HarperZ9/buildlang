"""Tests for build-ecosystem: verify every package is importable when installed.

The sibling packages (build-color, build-finance, build-oracle, build-engine, build-ui,
calibrate-pro) are separate repositories. In an isolated checkout they may not be
installed, so each test uses importorskip and skips (rather than fails) when its package
is absent. When the packages are installed, the tests run and assert the cross-wiring.
"""

import pytest


class TestPackageImports:
    """Verify every package in the ecosystem is importable when installed."""

    def test_build_color(self):
        build_color = pytest.importorskip("build_color")
        assert build_color.__version__ == "1.0.2"

    def test_build_finance(self):
        indicators = pytest.importorskip("build_finance.indicators")
        assert indicators is not None

    def test_build_oracle(self):
        arima = pytest.importorskip("build_oracle.arima")
        assert arima is not None

    def test_build_engine(self):
        adaptive_engine = pytest.importorskip("build_engine.adaptive_engine")
        assert adaptive_engine is not None

    def test_calibrate_pro(self):
        color_math = pytest.importorskip("calibrate_pro.core.color_math")
        assert color_math is not None

    def test_build_ui(self):
        theme = pytest.importorskip("build_ui.theme")
        assert theme.C.ACCENT == "#d4a0a0"


class TestCrossWiring:
    """Verify cross-package dependencies work when the packages are installed."""

    def test_calibrate_pro_uses_build_color(self):
        np = pytest.importorskip("numpy")
        color_math = pytest.importorskip("calibrate_pro.core.color_math")
        result = color_math.srgb_to_xyz(np.array([1.0, 0.0, 0.0]))
        assert result is not None
        assert len(result) == 3

    def test_engine_imports_oracle(self):
        model_trainer = pytest.importorskip("build_engine.model_trainer")
        assert model_trainer.ModelTrainer is not None

    def test_engine_imports_finance(self):
        prediction_strategy = pytest.importorskip("build_engine.prediction_strategy")
        assert prediction_strategy.PredictionStrategy is not None


class TestEcosystemMeta:
    """Test the ecosystem meta-package itself."""

    def test_version(self):
        build_ecosystem = pytest.importorskip("build_ecosystem")
        assert build_ecosystem.__version__ == "1.0.1"
