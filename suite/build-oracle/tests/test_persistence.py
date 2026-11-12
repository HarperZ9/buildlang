"""Tests for model save/load persistence across ARIMA, Prophet, and Neural."""

import json

import numpy as np
import pytest

from build_oracle.arima import ARIMA
from build_oracle.neural import SimpleForecaster
from build_oracle.prophet import Prophet

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _sample_series(n: int = 200) -> np.ndarray:
    """Deterministic sample time series for reproducible tests."""
    rng = np.random.RandomState(42)
    t = np.arange(n, dtype=np.float64)
    trend = 0.05 * t
    seasonal = 2.0 * np.sin(2 * np.pi * t / 7.0)
    noise = rng.normal(0, 0.3, n)
    return 10.0 + trend + seasonal + noise


@pytest.fixture
def sample_series():
    return _sample_series()


@pytest.fixture
def tmp_path_file(tmp_path):
    """Return a path to a temporary JSON file."""
    return str(tmp_path / "model.json")


# ---------------------------------------------------------------------------
# ARIMA persistence tests
# ---------------------------------------------------------------------------


class TestARIMAPersistence:
    def test_save_load_roundtrip(self, sample_series, tmp_path_file):
        """Fit -> save -> load -> predict must produce identical forecasts."""
        model = ARIMA(p=2, d=1, q=1)
        model.fit(sample_series)
        original_forecast = model.predict(10)

        model.save(tmp_path_file)
        loaded = ARIMA.load(tmp_path_file)
        loaded_forecast = loaded.predict(10)

        np.testing.assert_array_almost_equal(original_forecast, loaded_forecast)

    def test_save_preserves_order(self, sample_series, tmp_path_file):
        """Loaded model must retain the same (p, d, q) order."""
        model = ARIMA(p=3, d=1, q=2)
        model.fit(sample_series)
        model.save(tmp_path_file)

        loaded = ARIMA.load(tmp_path_file)
        assert loaded.p == 3
        assert loaded.d == 1
        assert loaded.q == 2

    def test_save_preserves_coefficients(self, sample_series, tmp_path_file):
        """AR and MA coefficients must survive the roundtrip."""
        model = ARIMA(p=2, d=1, q=2)
        model.fit(sample_series)
        model.save(tmp_path_file)

        loaded = ARIMA.load(tmp_path_file)
        np.testing.assert_array_almost_equal(model.phi, loaded.phi)
        np.testing.assert_array_almost_equal(model.theta, loaded.theta)
        assert abs(model.intercept - loaded.intercept) < 1e-12
        assert abs(model.sigma2 - loaded.sigma2) < 1e-12

    def test_save_unfitted_raises(self, tmp_path_file):
        """Saving an unfitted model must raise RuntimeError."""
        model = ARIMA(p=1, d=1, q=0)
        with pytest.raises(RuntimeError, match="not been fitted"):
            model.save(tmp_path_file)

    def test_load_wrong_type_raises(self, tmp_path_file):
        """Loading a file with wrong model_type must raise ValueError."""
        state = {"model_type": "prophet", "p": 1, "d": 0, "q": 0}
        with open(tmp_path_file, "w") as f:
            json.dump(state, f)
        with pytest.raises(ValueError, match="Expected model_type 'arima'"):
            ARIMA.load(tmp_path_file)


# ---------------------------------------------------------------------------
# Prophet persistence tests
# ---------------------------------------------------------------------------


class TestProphetPersistence:
    def test_save_load_roundtrip(self, sample_series, tmp_path_file):
        """Fit -> save -> load -> predict must produce identical forecasts."""
        t = np.arange(len(sample_series), dtype=np.float64)
        model = Prophet(fourier_order=3, n_changepoints=5)
        model.fit(t, sample_series)

        future = np.arange(len(sample_series), len(sample_series) + 10, dtype=np.float64)
        original = model.predict(future)

        model.save(tmp_path_file)
        loaded = Prophet.load(tmp_path_file)
        loaded_result = loaded.predict(future)

        np.testing.assert_array_almost_equal(original["yhat"], loaded_result["yhat"])

    def test_save_preserves_config(self, sample_series, tmp_path_file):
        """Loaded model must retain configuration parameters."""
        t = np.arange(len(sample_series), dtype=np.float64)
        model = Prophet(
            yearly_seasonality=False,
            weekly_seasonality=True,
            n_changepoints=10,
            fourier_order=5,
        )
        model.fit(t, sample_series)
        model.save(tmp_path_file)

        loaded = Prophet.load(tmp_path_file)
        assert loaded.yearly_seasonality is False
        assert loaded.weekly_seasonality is True
        assert loaded.n_changepoints == 10
        assert loaded.fourier_order == 5

    def test_save_unfitted_raises(self, tmp_path_file):
        """Saving an unfitted Prophet model must raise RuntimeError."""
        model = Prophet()
        with pytest.raises(RuntimeError, match="not been fitted"):
            model.save(tmp_path_file)

    def test_load_wrong_type_raises(self, tmp_path_file):
        """Loading a file with wrong model_type must raise ValueError."""
        state = {"model_type": "arima"}
        with open(tmp_path_file, "w") as f:
            json.dump(state, f)
        with pytest.raises(ValueError, match="Expected model_type 'prophet'"):
            Prophet.load(tmp_path_file)


# ---------------------------------------------------------------------------
# Neural (SimpleForecaster) persistence tests
# ---------------------------------------------------------------------------


class TestNeuralPersistence:
    def test_save_load_roundtrip(self, sample_series, tmp_path_file):
        """Train -> save -> load -> predict must produce identical forecasts."""
        model = SimpleForecaster(lookback=10, horizon=5, hidden=16)
        model.train(sample_series, epochs=20, lr=0.001)
        original_forecast = model.predict(sample_series)

        model.save(tmp_path_file)
        loaded = SimpleForecaster.load(tmp_path_file)
        loaded_forecast = loaded.predict(sample_series)

        np.testing.assert_array_almost_equal(original_forecast, loaded_forecast)

    def test_save_preserves_architecture(self, sample_series, tmp_path_file):
        """Loaded model must retain lookback, horizon, hidden dimensions."""
        model = SimpleForecaster(lookback=15, horizon=7, hidden=32)
        model.train(sample_series, epochs=10, lr=0.001)
        model.save(tmp_path_file)

        loaded = SimpleForecaster.load(tmp_path_file)
        assert loaded.lookback == 15
        assert loaded.horizon == 7
        assert loaded.hidden == 32

    def test_save_preserves_weights(self, sample_series, tmp_path_file):
        """All weight matrices and biases must survive the roundtrip."""
        model = SimpleForecaster(lookback=10, horizon=5, hidden=16)
        model.train(sample_series, epochs=10, lr=0.001)
        model.save(tmp_path_file)

        loaded = SimpleForecaster.load(tmp_path_file)
        np.testing.assert_array_almost_equal(model.layer1.W, loaded.layer1.W)
        np.testing.assert_array_almost_equal(model.layer1.b, loaded.layer1.b)
        np.testing.assert_array_almost_equal(model.layer2.W, loaded.layer2.W)
        np.testing.assert_array_almost_equal(model.layer2.b, loaded.layer2.b)

    def test_save_preserves_normalization(self, sample_series, tmp_path_file):
        """Training mean/std must be preserved for correct denormalization."""
        model = SimpleForecaster(lookback=10, horizon=5, hidden=16)
        model.train(sample_series, epochs=10, lr=0.001)
        model.save(tmp_path_file)

        loaded = SimpleForecaster.load(tmp_path_file)
        assert abs(model._train_mean - loaded._train_mean) < 1e-12
        assert abs(model._train_std - loaded._train_std) < 1e-12
        assert loaded._trained is True

    def test_load_wrong_type_raises(self, tmp_path_file):
        """Loading a file with wrong model_type must raise ValueError."""
        state = {"model_type": "arima"}
        with open(tmp_path_file, "w") as f:
            json.dump(state, f)
        with pytest.raises(ValueError, match="Expected model_type 'neural'"):
            SimpleForecaster.load(tmp_path_file)


# ---------------------------------------------------------------------------
# Cross-cutting / error-handling tests
# ---------------------------------------------------------------------------


class TestPersistenceErrors:
    def test_load_nonexistent_file_arima(self):
        """Loading a nonexistent file must raise FileNotFoundError."""
        with pytest.raises(FileNotFoundError):
            ARIMA.load("/nonexistent/path/model.json")

    def test_load_nonexistent_file_prophet(self):
        """Loading a nonexistent file must raise FileNotFoundError."""
        with pytest.raises(FileNotFoundError):
            Prophet.load("/nonexistent/path/model.json")

    def test_load_nonexistent_file_neural(self):
        """Loading a nonexistent file must raise FileNotFoundError."""
        with pytest.raises(FileNotFoundError):
            SimpleForecaster.load("/nonexistent/path/model.json")

    def test_load_corrupted_json(self, tmp_path_file):
        """Loading a file with invalid JSON must raise json.JSONDecodeError."""
        with open(tmp_path_file, "w") as f:
            f.write("{broken json!!!")
        with pytest.raises(json.JSONDecodeError):
            ARIMA.load(tmp_path_file)

    def test_load_corrupted_json_prophet(self, tmp_path_file):
        """Loading a file with invalid JSON via Prophet must raise."""
        with open(tmp_path_file, "w") as f:
            f.write("not valid json")
        with pytest.raises(json.JSONDecodeError):
            Prophet.load(tmp_path_file)

    def test_load_corrupted_json_neural(self, tmp_path_file):
        """Loading a file with invalid JSON via SimpleForecaster must raise."""
        with open(tmp_path_file, "w") as f:
            f.write("{{{}}")
        with pytest.raises(json.JSONDecodeError):
            SimpleForecaster.load(tmp_path_file)

    def test_saved_file_is_valid_json(self, sample_series, tmp_path_file):
        """The saved file must be parseable JSON with expected keys."""
        model = ARIMA(p=1, d=1, q=0)
        model.fit(sample_series)
        model.save(tmp_path_file)

        with open(tmp_path_file) as f:
            data = json.load(f)

        assert data["model_type"] == "arima"
        assert "phi" in data
        assert "intercept" in data
        assert isinstance(data["p"], int)
