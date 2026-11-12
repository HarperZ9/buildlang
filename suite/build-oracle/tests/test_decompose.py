"""Tests for build_oracle.decompose."""

import numpy as np
import pytest

from build_oracle.decompose import (
    classical_decompose,
    seasonal_strength,
    trend_strength,
)

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _make_seasonal_series(n: int = 120, period: int = 12) -> np.ndarray:
    """Create a series with clear trend + seasonality."""
    t = np.arange(n, dtype=np.float64)
    trend = 0.5 * t
    seasonal = 10.0 * np.sin(2 * np.pi * t / period)
    noise = np.random.default_rng(42).normal(0, 0.5, n)
    return trend + seasonal + noise


def _make_trend_only(n: int = 100) -> np.ndarray:
    """Pure linear trend with small noise."""
    t = np.arange(n, dtype=np.float64)
    return 2.0 * t + np.random.default_rng(7).normal(0, 0.1, n)


def _make_constant(n: int = 60, period: int = 12) -> np.ndarray:
    """Constant series (no trend, no seasonality)."""
    return np.full(n, 42.0)


# ---------------------------------------------------------------------------
# classical_decompose
# ---------------------------------------------------------------------------


class TestClassicalDecompose:
    def test_output_keys(self):
        y = _make_seasonal_series()
        result = classical_decompose(y, period=12)
        assert set(result.keys()) == {"trend", "seasonal", "residual"}

    def test_output_lengths(self):
        y = _make_seasonal_series(n=120, period=12)
        result = classical_decompose(y, period=12)
        for key in ("trend", "seasonal", "residual"):
            assert len(result[key]) == 120

    def test_additive_reconstruction(self):
        """trend + seasonal + residual should reconstruct original (where defined)."""
        y = _make_seasonal_series(n=120, period=12)
        d = classical_decompose(y, period=12, model="additive")
        reconstructed = d["trend"] + d["seasonal"] + d["residual"]
        # Only check where trend is not NaN
        mask = ~np.isnan(d["trend"])
        np.testing.assert_allclose(reconstructed[mask], y[mask], atol=1e-10)

    def test_multiplicative_reconstruction(self):
        """trend * seasonal * residual should reconstruct original."""
        y = np.abs(_make_seasonal_series(n=120, period=12)) + 50  # keep positive
        d = classical_decompose(y, period=12, model="multiplicative")
        reconstructed = d["trend"] * d["seasonal"] * d["residual"]
        mask = ~np.isnan(d["trend"])
        np.testing.assert_allclose(reconstructed[mask], y[mask], atol=1e-8)

    def test_seasonal_sums_to_zero(self):
        """Seasonal component should sum to ~0 over one period."""
        y = _make_seasonal_series(n=120, period=12)
        d = classical_decompose(y, period=12)
        # Take one full period from the middle (away from NaN edges)
        start = 30
        chunk = d["seasonal"][start : start + 12]
        assert abs(np.sum(chunk)) < 1e-10

    def test_trend_nan_at_edges(self):
        """Centered MA should produce NaN at the edges."""
        y = _make_seasonal_series(n=120, period=12)
        d = classical_decompose(y, period=12)
        assert np.isnan(d["trend"][0])
        assert np.isnan(d["trend"][-1])

    def test_short_series_raises(self):
        with pytest.raises(ValueError, match="2 full periods"):
            classical_decompose(np.arange(10.0), period=12)

    def test_invalid_model_raises(self):
        with pytest.raises(ValueError, match="model"):
            classical_decompose(np.arange(100.0), period=5, model="invalid")

    def test_period_too_small_raises(self):
        with pytest.raises(ValueError, match="period"):
            classical_decompose(np.arange(50.0), period=1)

    def test_odd_period(self):
        """Should work with odd period (different CMA branch)."""
        y = _make_seasonal_series(n=100, period=7)
        d = classical_decompose(y, period=7)
        mask = ~np.isnan(d["trend"])
        reconstructed = d["trend"] + d["seasonal"] + d["residual"]
        np.testing.assert_allclose(reconstructed[mask], y[mask], atol=1e-10)


# ---------------------------------------------------------------------------
# Strength measures
# ---------------------------------------------------------------------------


class TestStrengthMeasures:
    def test_trend_strength_high_for_trend(self):
        y = _make_trend_only(n=120)
        d = classical_decompose(y, period=12)
        ts = trend_strength(d)
        assert ts > 0.8

    def test_seasonal_strength_high_for_seasonal(self):
        y = _make_seasonal_series(n=120, period=12)
        d = classical_decompose(y, period=12)
        ss = seasonal_strength(d)
        assert ss > 0.7

    def test_trend_strength_range(self):
        y = _make_seasonal_series()
        d = classical_decompose(y, period=12)
        ts = trend_strength(d)
        assert 0.0 <= ts <= 1.0

    def test_seasonal_strength_range(self):
        y = _make_seasonal_series()
        d = classical_decompose(y, period=12)
        ss = seasonal_strength(d)
        assert 0.0 <= ss <= 1.0

    def test_constant_series_low_strength(self):
        y = _make_constant(n=60, period=12)
        d = classical_decompose(y, period=12)
        assert trend_strength(d) == 0.0
        assert seasonal_strength(d) == 0.0
