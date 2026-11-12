"""Tests for build_oracle.ensemble."""

import numpy as np
import pytest

from build_oracle.ensemble import EnsembleConfig, EnsembleForecaster

# ---------------------------------------------------------------------------
# Helpers — synthetic data generators
# ---------------------------------------------------------------------------


def _sine_wave(n: int = 200, period: float = 20.0, seed: int = 42) -> np.ndarray:
    """Sine wave with light noise."""
    rng = np.random.default_rng(seed)
    t = np.arange(n, dtype=np.float64)
    return 10.0 + 3.0 * np.sin(2 * np.pi * t / period) + rng.normal(0, 0.3, n)


def _linear_trend(n: int = 200, seed: int = 42) -> np.ndarray:
    """Linear trend with noise."""
    rng = np.random.default_rng(seed)
    return 5.0 + 0.05 * np.arange(n, dtype=np.float64) + rng.normal(0, 0.2, n)


def _random_walk(n: int = 200, seed: int = 42) -> np.ndarray:
    rng = np.random.default_rng(seed)
    return np.cumsum(rng.normal(0, 1, n)) + 50.0


# ---------------------------------------------------------------------------
# Config tests
# ---------------------------------------------------------------------------


class TestEnsembleConfig:
    def test_defaults(self):
        cfg = EnsembleConfig()
        assert cfg.use_arima is True
        assert cfg.use_prophet is True
        assert cfg.use_neural is True
        assert cfg.validation_window == 20
        assert cfg.min_weight == 0.05

    def test_custom_config(self):
        cfg = EnsembleConfig(use_neural=False, validation_window=30)
        assert cfg.use_neural is False
        assert cfg.validation_window == 30


# ---------------------------------------------------------------------------
# Fitting & prediction
# ---------------------------------------------------------------------------


class TestEnsembleFit:
    def test_fit_sine_wave(self):
        data = _sine_wave(200)
        ens = EnsembleForecaster()
        ens.fit(data)
        assert ens._fitted
        assert len(ens.active_models) > 0

    def test_fit_linear_trend(self):
        data = _linear_trend(200)
        ens = EnsembleForecaster()
        ens.fit(data)
        assert ens._fitted

    def test_fit_random_walk(self):
        data = _random_walk(200)
        ens = EnsembleForecaster()
        ens.fit(data)
        assert ens._fitted

    def test_predict_returns_correct_length(self):
        data = _sine_wave(200)
        ens = EnsembleForecaster()
        ens.fit(data)
        for steps in [1, 5, 10]:
            pred = ens.predict(steps)
            assert len(pred) == steps

    def test_predict_values_are_finite(self):
        data = _linear_trend(200)
        ens = EnsembleForecaster()
        ens.fit(data)
        pred = ens.predict(10)
        assert np.all(np.isfinite(pred))

    def test_predict_before_fit_raises(self):
        ens = EnsembleForecaster()
        with pytest.raises(RuntimeError, match="not been fitted"):
            ens.predict(5)

    def test_fit_too_short_raises(self):
        with pytest.raises(ValueError, match="at least 30"):
            EnsembleForecaster().fit(np.array([1.0, 2.0, 3.0]))


# ---------------------------------------------------------------------------
# Weight properties
# ---------------------------------------------------------------------------


class TestWeights:
    def test_weights_sum_to_one(self):
        data = _sine_wave(200)
        ens = EnsembleForecaster()
        ens.fit(data)
        w = ens.weights
        total = sum(w.values())
        assert abs(total - 1.0) < 1e-8, f"Weights sum to {total}, expected 1.0"

    def test_weights_all_positive(self):
        data = _linear_trend(200)
        ens = EnsembleForecaster()
        ens.fit(data)
        for name, w in ens.weights.items():
            assert w > 0, f"Weight for {name} is {w}, expected > 0"

    def test_min_weight_enforced(self):
        cfg = EnsembleConfig(min_weight=0.10)
        data = _sine_wave(200)
        ens = EnsembleForecaster(config=cfg)
        ens.fit(data)
        for name, w in ens.weights.items():
            assert w >= 0.10 - 1e-8, f"{name} weight {w} < min_weight 0.10"

    def test_model_errors_populated(self):
        data = _sine_wave(200)
        ens = EnsembleForecaster()
        ens.fit(data)
        errors = ens.model_errors
        assert len(errors) > 0
        for name, e in errors.items():
            assert e >= 0, f"MAE for {name} is {e}, expected >= 0"


# ---------------------------------------------------------------------------
# Disabled models
# ---------------------------------------------------------------------------


class TestDisabledModels:
    def test_arima_only(self):
        cfg = EnsembleConfig(use_prophet=False, use_neural=False)
        data = _sine_wave(200)
        ens = EnsembleForecaster(config=cfg)
        ens.fit(data)
        assert ens.active_models == ["arima"]
        pred = ens.predict(5)
        assert len(pred) == 5

    def test_prophet_only(self):
        cfg = EnsembleConfig(use_arima=False, use_neural=False)
        data = _sine_wave(200)
        ens = EnsembleForecaster(config=cfg)
        ens.fit(data)
        assert ens.active_models == ["prophet"]
        pred = ens.predict(5)
        assert len(pred) == 5

    def test_neural_only(self):
        cfg = EnsembleConfig(use_arima=False, use_prophet=False)
        data = _sine_wave(200)
        ens = EnsembleForecaster(config=cfg)
        ens.fit(data)
        assert ens.active_models == ["neural"]

    def test_two_models(self):
        cfg = EnsembleConfig(use_neural=False)
        data = _linear_trend(200)
        ens = EnsembleForecaster(config=cfg)
        ens.fit(data)
        assert len(ens.active_models) == 2
        assert "arima" in ens.active_models
        assert "prophet" in ens.active_models

    def test_all_disabled_raises(self):
        cfg = EnsembleConfig(use_arima=False, use_prophet=False, use_neural=False)
        data = _sine_wave(200)
        ens = EnsembleForecaster(config=cfg)
        with pytest.raises(RuntimeError, match="All sub-models failed"):
            ens.fit(data)


# ---------------------------------------------------------------------------
# Ensemble quality — MAE should be competitive
# ---------------------------------------------------------------------------


class TestEnsembleQuality:
    def test_ensemble_competitive_with_best_model(self):
        """Ensemble MAE should not be dramatically worse than the best model."""
        data = _sine_wave(300)
        val_win = 20
        data[:-val_win]
        actual = data[-val_win:]

        ens = EnsembleForecaster()
        ens.fit(data)

        # Best individual model error from the ensemble's own scoring
        best_individual = min(ens.model_errors.values())

        # Ensemble forecast on the same horizon
        pred = ens.predict(val_win)
        ensemble_mae = float(np.mean(np.abs(actual - pred[:val_win])))

        # Allow up to 3x the best model's error — ensemble should be
        # in the same ballpark, not orders of magnitude worse.
        assert ensemble_mae < best_individual * 3.0, (
            f"Ensemble MAE {ensemble_mae:.4f} >> best model MAE {best_individual:.4f}"
        )


# ---------------------------------------------------------------------------
# Repr
# ---------------------------------------------------------------------------


class TestRepr:
    def test_unfitted_repr(self):
        ens = EnsembleForecaster()
        assert "unfitted" in repr(ens)

    def test_fitted_repr(self):
        data = _sine_wave(200)
        ens = EnsembleForecaster()
        ens.fit(data)
        r = repr(ens)
        assert "EnsembleForecaster" in r
        # Should contain at least one model name
        assert any(name in r for name in ["arima", "prophet", "neural"])
