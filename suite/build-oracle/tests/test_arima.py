"""Tests for build_oracle.arima."""

import numpy as np
import pytest

from build_oracle.arima import ARIMA, auto_arima

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _random_walk(n: int = 200, seed: int = 42) -> np.ndarray:
    rng = np.random.default_rng(seed)
    return np.cumsum(rng.normal(0, 1, n))


def _ar1_series(n: int = 200, phi: float = 0.7, seed: int = 42) -> np.ndarray:
    """Generate an AR(1) process: y[t] = phi * y[t-1] + eps."""
    rng = np.random.default_rng(seed)
    y = np.zeros(n)
    for t in range(1, n):
        y[t] = phi * y[t - 1] + rng.normal(0, 1)
    return y


def _linear_trend(n: int = 100) -> np.ndarray:
    return np.arange(n, dtype=np.float64) * 0.5 + 10.0


# ---------------------------------------------------------------------------
# ARIMA construction
# ---------------------------------------------------------------------------


class TestARIMAConstruction:
    def test_default_orders(self):
        m = ARIMA()
        assert m.p == 1 and m.d == 1 and m.q == 1

    def test_custom_orders(self):
        m = ARIMA(p=3, d=0, q=2)
        assert m.p == 3 and m.d == 0 and m.q == 2

    def test_negative_order_raises(self):
        with pytest.raises(ValueError, match="non-negative"):
            ARIMA(p=-1, d=0, q=0)

    def test_repr(self):
        assert "ARIMA" in repr(ARIMA(2, 1, 0))


# ---------------------------------------------------------------------------
# Fitting
# ---------------------------------------------------------------------------


class TestARIMAFit:
    def test_fit_returns_self(self):
        m = ARIMA(1, 1, 0)
        result = m.fit(_random_walk())
        assert result is m

    def test_phi_length_matches_p(self):
        m = ARIMA(p=3, d=1, q=0).fit(_random_walk())
        assert len(m.phi) == 3

    def test_theta_length_matches_q(self):
        m = ARIMA(p=1, d=1, q=2).fit(_random_walk())
        assert len(m.theta) == 2

    def test_fit_ar1_recovers_phi(self):
        """AR(1) with phi=0.7 should be roughly recovered."""
        y = _ar1_series(n=1000, phi=0.7)
        m = ARIMA(p=1, d=0, q=0).fit(y)
        assert abs(m.phi[0] - 0.7) < 0.15

    def test_sigma2_positive(self):
        m = ARIMA(1, 1, 0).fit(_random_walk())
        assert m.sigma2 > 0

    def test_short_series_raises(self):
        with pytest.raises(ValueError, match="too short"):
            ARIMA(p=5, d=2, q=5).fit(np.arange(5.0))


# ---------------------------------------------------------------------------
# Prediction
# ---------------------------------------------------------------------------


class TestARIMAPredict:
    def test_predict_length(self):
        m = ARIMA(1, 1, 0).fit(_random_walk())
        fc = m.predict(horizon=10)
        assert len(fc) == 10

    def test_predict_before_fit_raises(self):
        m = ARIMA()
        with pytest.raises(RuntimeError, match="not been fitted"):
            m.predict(5)

    def test_predict_zero_horizon_raises(self):
        m = ARIMA(1, 1, 0).fit(_random_walk())
        with pytest.raises(ValueError, match="horizon"):
            m.predict(0)

    def test_forecast_continues_trend(self):
        """For a linear trend with d=1, forecast should continue upward."""
        y = _linear_trend(100)
        m = ARIMA(p=1, d=1, q=0).fit(y)
        fc = m.predict(5)
        # Forecast values should be larger than the last observed value
        assert fc[-1] > y[-1]

    def test_forecast_finite(self):
        m = ARIMA(1, 1, 1).fit(_random_walk())
        fc = m.predict(20)
        assert np.all(np.isfinite(fc))


# ---------------------------------------------------------------------------
# Information criteria
# ---------------------------------------------------------------------------


class TestInformationCriteria:
    def test_aic_finite(self):
        m = ARIMA(1, 1, 0).fit(_random_walk())
        assert np.isfinite(m.aic())

    def test_bic_finite(self):
        m = ARIMA(1, 1, 0).fit(_random_walk())
        assert np.isfinite(m.bic())

    def test_bic_penalises_more_than_aic(self):
        """For n > 8, BIC penalty > AIC penalty, so BIC >= AIC typically."""
        y = _random_walk(200)
        m = ARIMA(1, 1, 1).fit(y)
        # BIC uses ln(n) per param vs 2 per param for AIC
        # With n=200, ln(200)≈5.3 > 2, so bic penalty > aic penalty
        assert m.bic() > m.aic()

    def test_aic_before_fit_raises(self):
        with pytest.raises(RuntimeError):
            ARIMA().aic()


# ---------------------------------------------------------------------------
# auto_arima
# ---------------------------------------------------------------------------


class TestAutoARIMA:
    def test_returns_fitted_model(self):
        y = _random_walk(100)
        m = auto_arima(y, max_p=2, max_d=1, max_q=2)
        assert m._fitted
        assert isinstance(m, ARIMA)

    def test_best_model_has_finite_aic(self):
        y = _ar1_series(200)
        m = auto_arima(y, max_p=3, max_d=1, max_q=2)
        assert np.isfinite(m.aic())

    def test_can_forecast(self):
        y = _random_walk(150)
        m = auto_arima(y, max_p=2, max_d=1, max_q=2)
        fc = m.predict(5)
        assert len(fc) == 5
        assert np.all(np.isfinite(fc))
