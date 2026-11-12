"""
Tests for build_engine.model_trainer.ModelTrainer.

Covers model creation with default/custom configs, training individual and
combined models, prediction output, introspection methods, and edge cases
(unknown model names, insufficient data).
"""

from __future__ import annotations

import numpy as np
import pytest

from build_engine.config import EngineConfig
from build_engine.model_trainer import ModelTrainer

# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------


@pytest.fixture
def sin_series() -> np.ndarray:
    """Deterministic sinusoidal test series (200 points)."""
    rng = np.random.default_rng(0)
    return np.sin(np.linspace(0, 4 * np.pi, 200)) * 10 + 100 + rng.standard_normal(200) * 0.5


@pytest.fixture
def short_series() -> np.ndarray:
    """Very short series (5 points) for insufficient-data tests."""
    return np.array([100.0, 101.0, 99.5, 100.5, 101.5])


# ---------------------------------------------------------------------------
# Construction
# ---------------------------------------------------------------------------


class TestModelTrainerCreation:
    def test_default_config(self):
        trainer = ModelTrainer()
        assert trainer.config.models == ["arima", "prophet"]
        assert trainer.fitted_models == {}
        assert trainer._last_series is None

    def test_custom_config(self):
        cfg = EngineConfig(models=["neural"], forecast_horizon=3)
        trainer = ModelTrainer(cfg)
        assert trainer.config.models == ["neural"]
        assert trainer.config.forecast_horizon == 3
        assert trainer.fitted_models == {}

    def test_has_models_initially_false(self):
        trainer = ModelTrainer()
        assert trainer.has_models() is False

    def test_repr_before_training(self):
        trainer = ModelTrainer()
        assert "(none)" in repr(trainer)


# ---------------------------------------------------------------------------
# Training individual models
# ---------------------------------------------------------------------------


class TestTrainIndividual:
    def test_train_arima(self, sin_series):
        cfg = EngineConfig(models=["arima"])
        trainer = ModelTrainer(cfg)
        trainer.train_all(sin_series)
        assert "arima" in trainer.fitted_models
        assert trainer.has_models() is True

    def test_train_prophet(self, sin_series):
        cfg = EngineConfig(models=["prophet"])
        trainer = ModelTrainer(cfg)
        trainer.train_all(sin_series)
        assert "prophet" in trainer.fitted_models
        assert trainer.has_models() is True

    def test_train_neural(self, sin_series):
        cfg = EngineConfig(models=["neural"], forecast_horizon=3)
        trainer = ModelTrainer(cfg)
        trainer.train_all(sin_series)
        assert "neural" in trainer.fitted_models
        assert trainer.has_models() is True

    def test_last_series_stored_after_training(self, sin_series):
        trainer = ModelTrainer(EngineConfig(models=["arima"]))
        trainer.train_all(sin_series)
        assert trainer._last_series is not None
        np.testing.assert_array_equal(trainer._last_series, sin_series)


# ---------------------------------------------------------------------------
# Training all models simultaneously
# ---------------------------------------------------------------------------


class TestTrainAll:
    def test_train_all_three(self, sin_series):
        cfg = EngineConfig(models=["arima", "prophet", "neural"], forecast_horizon=3)
        trainer = ModelTrainer(cfg)
        trainer.train_all(sin_series)
        assert len(trainer.fitted_models) == 3
        assert set(trainer.fitted_models.keys()) == {"arima", "prophet", "neural"}

    def test_train_all_default_pair(self, sin_series):
        trainer = ModelTrainer()
        trainer.train_all(sin_series)
        assert "arima" in trainer.fitted_models
        assert "prophet" in trainer.fitted_models
        assert len(trainer.fitted_models) == 2


# ---------------------------------------------------------------------------
# Predictions
# ---------------------------------------------------------------------------


class TestPredictions:
    def test_predict_all_returns_dict(self, sin_series):
        cfg = EngineConfig(models=["arima", "prophet"])
        trainer = ModelTrainer(cfg)
        trainer.train_all(sin_series)
        predictions = trainer.predict_all(sin_series, horizon=5)
        assert isinstance(predictions, dict)
        assert "arima" in predictions
        assert "prophet" in predictions

    def test_predict_arima_length(self, sin_series):
        cfg = EngineConfig(models=["arima"])
        trainer = ModelTrainer(cfg)
        trainer.train_all(sin_series)
        predictions = trainer.predict_all(sin_series, horizon=5)
        assert len(predictions["arima"]) == 5

    def test_predict_prophet_length(self, sin_series):
        cfg = EngineConfig(models=["prophet"])
        trainer = ModelTrainer(cfg)
        trainer.train_all(sin_series)
        predictions = trainer.predict_all(sin_series, horizon=7)
        assert len(predictions["prophet"]) == 7

    def test_predict_neural_returns_array(self, sin_series):
        cfg = EngineConfig(models=["neural"], forecast_horizon=3)
        trainer = ModelTrainer(cfg)
        trainer.train_all(sin_series)
        predictions = trainer.predict_all(sin_series, horizon=3)
        assert "neural" in predictions
        assert isinstance(predictions["neural"], np.ndarray)

    def test_predict_all_no_fitted_models_returns_empty(self, sin_series):
        trainer = ModelTrainer()
        predictions = trainer.predict_all(sin_series, horizon=5)
        assert predictions == {}


# ---------------------------------------------------------------------------
# Introspection
# ---------------------------------------------------------------------------


class TestIntrospection:
    def test_get_model_info_arima(self, sin_series):
        cfg = EngineConfig(models=["arima"])
        trainer = ModelTrainer(cfg)
        trainer.train_all(sin_series)
        info = trainer.get_model_info()
        assert info["arima"] == "ARIMA"

    def test_get_model_info_prophet(self, sin_series):
        cfg = EngineConfig(models=["prophet"])
        trainer = ModelTrainer(cfg)
        trainer.train_all(sin_series)
        info = trainer.get_model_info()
        assert info["prophet"] == "Prophet"

    def test_get_model_info_neural(self, sin_series):
        cfg = EngineConfig(models=["neural"], forecast_horizon=3)
        trainer = ModelTrainer(cfg)
        trainer.train_all(sin_series)
        info = trainer.get_model_info()
        assert info["neural"] == "SimpleForecaster"

    def test_get_model_info_empty_before_training(self):
        trainer = ModelTrainer()
        assert trainer.get_model_info() == {}

    def test_repr_after_training(self, sin_series):
        cfg = EngineConfig(models=["arima"])
        trainer = ModelTrainer(cfg)
        trainer.train_all(sin_series)
        assert "arima" in repr(trainer)


# ---------------------------------------------------------------------------
# Edge cases
# ---------------------------------------------------------------------------


class TestEdgeCases:
    def test_unknown_model_skipped(self, sin_series):
        cfg = EngineConfig(models=["nonexistent"])
        trainer = ModelTrainer(cfg)
        trainer.train_all(sin_series)
        assert len(trainer.fitted_models) == 0
        assert trainer.has_models() is False

    def test_unknown_plus_known_only_trains_known(self, sin_series):
        cfg = EngineConfig(models=["arima", "doesnotexist"])
        trainer = ModelTrainer(cfg)
        trainer.train_all(sin_series)
        assert "arima" in trainer.fitted_models
        assert "doesnotexist" not in trainer.fitted_models
        assert len(trainer.fitted_models) == 1

    def test_training_with_short_data_does_not_crash(self, short_series):
        cfg = EngineConfig(models=["arima", "prophet", "neural"], forecast_horizon=2)
        trainer = ModelTrainer(cfg)
        # Should not raise -- errors are logged and skipped
        trainer.train_all(short_series)

    def test_retrain_replaces_previous_model(self, sin_series):
        cfg = EngineConfig(models=["arima"])
        trainer = ModelTrainer(cfg)
        trainer.train_all(sin_series)
        first_model = trainer.fitted_models["arima"]
        trainer.train_all(sin_series)
        second_model = trainer.fitted_models["arima"]
        # A fresh model object should be created on each train
        assert first_model is not second_model

    def test_empty_model_list(self, sin_series):
        cfg = EngineConfig(models=[])
        trainer = ModelTrainer(cfg)
        trainer.train_all(sin_series)
        assert trainer.fitted_models == {}
        assert trainer.has_models() is False
