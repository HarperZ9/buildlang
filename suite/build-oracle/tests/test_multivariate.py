"""
Tests for multivariate forecasting support:
  - VAR model (fit, predict, save/load)
  - Prophet with external regressors
  - SimpleForecaster with 2-D input
"""

import json
import os
import tempfile

import numpy as np
import pytest

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _make_var_data(T: int = 200, K: int = 2, seed: int = 42) -> np.ndarray:
    """Generate a synthetic VAR(1)-like multivariate series."""
    rng = np.random.default_rng(seed)
    data = np.zeros((T, K))
    data[0] = rng.standard_normal(K)
    A = np.array([[0.5, 0.1], [0.2, 0.4]])[:K, :K]
    for t in range(1, T):
        data[t] = A @ data[t - 1] + rng.standard_normal(K) * 0.3
    return data


def _make_prophet_data(n: int = 200, seed: int = 42):
    """Return (t, y, regressors) for Prophet tests."""
    rng = np.random.default_rng(seed)
    t = np.arange(n, dtype=np.float64)
    trend = 0.05 * t
    seasonal = 2.0 * np.sin(2 * np.pi * t / 7.0)
    # One external regressor that explains part of the signal
    reg = rng.standard_normal(n) * 0.5
    y = trend + seasonal + 1.5 * reg + rng.standard_normal(n) * 0.1
    return t, y, reg.reshape(-1, 1)


# ===================================================================
# VAR model tests
# ===================================================================


class TestVARFit:
    def test_fit_2var(self):
        """VAR fits on 2-variable data without error."""
        from build_oracle.var import VAR

        data = _make_var_data(T=100, K=2)
        model = VAR(p=2)
        model.fit(data)
        assert model._fitted is True
        assert model._k == 2

    def test_fit_3var(self):
        """VAR fits on 3-variable data."""
        from build_oracle.var import VAR

        rng = np.random.default_rng(99)
        data = rng.standard_normal((120, 3))
        model = VAR(p=3)
        model.fit(data)
        assert model._fitted is True
        assert model._k == 3

    def test_fit_too_short_raises(self):
        """VAR raises ValueError when series is shorter than p+1."""
        from build_oracle.var import VAR

        model = VAR(p=5)
        with pytest.raises(ValueError, match="at least"):
            model.fit(np.random.randn(5, 2))

    def test_fit_1d_raises(self):
        """VAR raises ValueError on 1-D input."""
        from build_oracle.var import VAR

        model = VAR(p=2)
        with pytest.raises(ValueError, match="2-D"):
            model.fit(np.arange(50, dtype=float))


class TestVARPredict:
    def test_predict_shape(self):
        """predict returns (horizon, K) array."""
        from build_oracle.var import VAR

        data = _make_var_data(T=100, K=2)
        model = VAR(p=2)
        model.fit(data)
        fc = model.predict(horizon=10)
        assert fc.shape == (10, 2)

    def test_predict_horizon_1(self):
        """Single-step forecast works."""
        from build_oracle.var import VAR

        data = _make_var_data(T=60, K=2)
        model = VAR(p=1)
        model.fit(data)
        fc = model.predict(horizon=1)
        assert fc.shape == (1, 2)
        assert np.all(np.isfinite(fc))

    def test_predict_before_fit_raises(self):
        """predict raises RuntimeError before fit."""
        from build_oracle.var import VAR

        model = VAR(p=2)
        with pytest.raises(RuntimeError, match="fitted"):
            model.predict(5)

    def test_predict_values_reasonable(self):
        """Forecasts from a near-stationary process stay bounded."""
        from build_oracle.var import VAR

        data = _make_var_data(T=200, K=2, seed=7)
        model = VAR(p=2)
        model.fit(data)
        fc = model.predict(horizon=20)
        # The synthetic process has bounded variance; forecasts should not explode
        assert np.all(np.abs(fc) < 50)


class TestVARPersistence:
    def test_save_load_roundtrip(self):
        """Model state survives save/load."""
        from build_oracle.var import VAR

        data = _make_var_data(T=80, K=2)
        model = VAR(p=2)
        model.fit(data)
        original_fc = model.predict(5)

        with tempfile.NamedTemporaryFile(suffix=".json", delete=False) as f:
            path = f.name
        try:
            model.save(path)
            loaded = VAR.load(path)
            loaded_fc = loaded.predict(5)
            np.testing.assert_allclose(original_fc, loaded_fc)
            assert loaded.p == model.p
            assert loaded._k == model._k
        finally:
            os.unlink(path)

    def test_load_bad_type_raises(self):
        """Loading a non-VAR JSON raises ValueError."""
        from build_oracle.var import VAR

        with tempfile.NamedTemporaryFile(suffix=".json", delete=False, mode="w") as f:
            json.dump({"model_type": "arima"}, f)
            path = f.name
        try:
            with pytest.raises(ValueError, match="var"):
                VAR.load(path)
        finally:
            os.unlink(path)


# ===================================================================
# Prophet with regressors
# ===================================================================


class TestProphetRegressors:
    def test_fit_with_regressors(self):
        """Prophet fits when regressors are provided."""
        from build_oracle.prophet import Prophet

        t, y, reg = _make_prophet_data(n=200)
        model = Prophet(n_changepoints=5, fourier_order=3)
        model.fit(t, y, regressors=reg)
        assert model._fitted is True
        assert model._regressor_coeffs is not None
        assert model._n_regressors == 1

    def test_predict_with_regressors(self):
        """Predictions include regressor component."""
        from build_oracle.prophet import Prophet

        t, y, reg = _make_prophet_data(n=200)
        model = Prophet(n_changepoints=5, fourier_order=3)
        model.fit(t, y, regressors=reg)

        rng = np.random.default_rng(0)
        future_t = np.arange(200, 210, dtype=np.float64)
        future_reg = rng.standard_normal((10, 1))
        result = model.predict(future_t, regressors=future_reg)

        assert "regressors" in result
        assert result["yhat"].shape == (10,)
        assert result["regressors"].shape == (10,)
        # Regressor component should be nonzero when regressors are nonzero
        assert not np.allclose(result["regressors"], 0.0)

    def test_predict_without_regressors_still_works(self):
        """Prophet trained without regressors still works normally."""
        from build_oracle.prophet import Prophet

        t = np.arange(100, dtype=np.float64)
        y = 0.1 * t + np.sin(2 * np.pi * t / 7.0)
        model = Prophet(n_changepoints=5, fourier_order=3)
        model.fit(t, y)
        result = model.predict(np.arange(100, 110, dtype=np.float64))
        assert result["yhat"].shape == (10,)
        assert np.allclose(result["regressors"], 0.0)

    def test_save_load_with_regressors(self):
        """Prophet with regressors survives save/load."""
        from build_oracle.prophet import Prophet

        t, y, reg = _make_prophet_data(n=200)
        model = Prophet(n_changepoints=5, fourier_order=3)
        model.fit(t, y, regressors=reg)

        with tempfile.NamedTemporaryFile(suffix=".json", delete=False) as f:
            path = f.name
        try:
            model.save(path)
            loaded = Prophet.load(path)
            assert loaded._n_regressors == 1
            assert loaded._regressor_coeffs is not None
            np.testing.assert_allclose(loaded._regressor_coeffs, model._regressor_coeffs)
        finally:
            os.unlink(path)


# ===================================================================
# Neural with multivariate input
# ===================================================================


class TestNeuralMultivariate:
    def test_train_2d_input(self):
        """SimpleForecaster trains on (T, 2) data."""
        from build_oracle.neural import SimpleForecaster

        rng = np.random.default_rng(42)
        data = rng.standard_normal((100, 2))
        model = SimpleForecaster(lookback=10, horizon=3, hidden=16, n_vars=2)
        losses = model.train(data, epochs=5, lr=0.001)
        assert len(losses) == 5
        assert model._trained is True

    def test_predict_2d_input_shape(self):
        """Prediction from multivariate input has shape (horizon,)."""
        from build_oracle.neural import SimpleForecaster

        rng = np.random.default_rng(42)
        data = rng.standard_normal((100, 2))
        model = SimpleForecaster(lookback=10, horizon=3, hidden=16, n_vars=2)
        model.train(data, epochs=5, lr=0.001)
        fc = model.predict(data)
        assert fc.shape == (3,)
        assert np.all(np.isfinite(fc))

    def test_univariate_backward_compat(self):
        """Univariate path still works unchanged."""
        from build_oracle.neural import SimpleForecaster

        rng = np.random.default_rng(42)
        series = np.cumsum(rng.standard_normal(80))
        model = SimpleForecaster(lookback=10, horizon=3, hidden=16)
        model.train(series, epochs=5, lr=0.001)
        fc = model.predict(series)
        assert fc.shape == (3,)

    def test_save_load_multivariate(self):
        """Multivariate neural model survives save/load."""
        from build_oracle.neural import SimpleForecaster

        rng = np.random.default_rng(42)
        data = rng.standard_normal((100, 2))
        model = SimpleForecaster(lookback=10, horizon=3, hidden=16, n_vars=2)
        model.train(data, epochs=5, lr=0.001)
        original_fc = model.predict(data)

        with tempfile.NamedTemporaryFile(suffix=".json", delete=False) as f:
            path = f.name
        try:
            model.save(path)
            loaded = SimpleForecaster.load(path)
            assert loaded.n_vars == 2
            loaded_fc = loaded.predict(data)
            np.testing.assert_allclose(original_fc, loaded_fc)
        finally:
            os.unlink(path)
