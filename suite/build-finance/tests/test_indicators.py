"""
Tests for build_finance.indicators

~30 tests covering SMA, EMA, RSI, MACD, Bollinger Bands, ATR,
Stochastic, VWAP, ADX, and OBV with edge cases.
"""

import numpy as np
import pytest

from build_finance.indicators import (
    adx,
    atr,
    bollinger_bands,
    ema,
    macd,
    obv,
    rsi,
    sma,
    stochastic,
    vwap,
)

# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------


@pytest.fixture
def close_20():
    """20-bar close series with a simple uptrend."""
    return np.array(
        [
            10.0,
            10.5,
            11.0,
            10.8,
            11.2,
            11.5,
            11.3,
            11.8,
            12.0,
            12.5,
            12.3,
            12.8,
            13.0,
            13.2,
            13.5,
            13.3,
            13.8,
            14.0,
            14.2,
            14.5,
        ]
    )


@pytest.fixture
def ohlcv_30():
    """30-bar OHLCV dataset for multi-input indicators."""
    np.random.seed(42)
    n = 30
    base = np.linspace(100, 115, n)
    noise = np.random.normal(0, 1, n)
    close = base + noise
    high = close + np.abs(np.random.normal(0, 0.5, n))
    low = close - np.abs(np.random.normal(0, 0.5, n))
    open_ = (high + low) / 2.0
    volume = np.random.uniform(1000, 5000, n)
    return open_, high, low, close, volume


# ---------------------------------------------------------------------------
# SMA
# ---------------------------------------------------------------------------


class TestSMA:
    def test_basic(self, close_20):
        result = sma(close_20, 5)
        assert len(result) == 20
        # First 4 values should be NaN
        assert all(np.isnan(result[:4]))
        # 5th value = mean of first 5
        expected = np.mean(close_20[:5])
        assert pytest.approx(result[4], rel=1e-10) == expected

    def test_period_1(self, close_20):
        result = sma(close_20, 1)
        np.testing.assert_allclose(result, close_20)

    def test_period_equals_length(self, close_20):
        result = sma(close_20, 20)
        assert np.isnan(result[18])
        assert pytest.approx(result[19], rel=1e-10) == np.mean(close_20)

    def test_empty_array(self):
        result = sma(np.array([]), 5)
        assert len(result) == 0

    def test_insufficient_data(self):
        result = sma(np.array([1.0, 2.0]), 5)
        assert all(np.isnan(result))


# ---------------------------------------------------------------------------
# EMA
# ---------------------------------------------------------------------------


class TestEMA:
    def test_seed_is_sma(self, close_20):
        result = ema(close_20, 5)
        # Seed at index 4 should equal SMA of first 5 bars
        expected_seed = np.mean(close_20[:5])
        assert pytest.approx(result[4], rel=1e-10) == expected_seed

    def test_nan_prefix(self, close_20):
        result = ema(close_20, 10)
        assert all(np.isnan(result[:9]))
        assert not np.isnan(result[9])

    def test_length_preserved(self, close_20):
        result = ema(close_20, 5)
        assert len(result) == len(close_20)

    def test_ema_reacts_faster_than_sma(self):
        # Accelerating uptrend so EMA pulls ahead of SMA
        data = np.array([10.0 + 0.05 * i**1.5 for i in range(50)])
        e = ema(data, 10)
        s = sma(data, 10)
        # EMA weights recent (higher) prices more heavily
        assert e[-1] > s[-1]


# ---------------------------------------------------------------------------
# RSI
# ---------------------------------------------------------------------------


class TestRSI:
    def test_range(self, close_20):
        result = rsi(close_20, 14)
        valid = result[~np.isnan(result)]
        assert all(0 <= v <= 100 for v in valid)

    def test_nan_prefix(self, close_20):
        result = rsi(close_20, 14)
        assert all(np.isnan(result[:14]))

    def test_overbought_flat(self):
        # Monotonically rising series => RSI near 100
        rising = np.arange(1.0, 20.0)
        result = rsi(rising, 5)
        valid = result[~np.isnan(result)]
        assert valid[-1] > 90

    def test_oversold_flat(self):
        # Monotonically falling series => RSI near 0
        falling = np.arange(20.0, 1.0, -1.0)
        result = rsi(falling, 5)
        valid = result[~np.isnan(result)]
        assert valid[-1] < 10


# ---------------------------------------------------------------------------
# MACD
# ---------------------------------------------------------------------------


class TestMACD:
    def test_output_shapes(self, close_20):
        ml, sl, hist = macd(close_20, fast=5, slow=10, signal=3)
        assert len(ml) == len(close_20)
        assert len(sl) == len(close_20)
        assert len(hist) == len(close_20)

    def test_histogram_is_diff(self, close_20):
        ml, sl, hist = macd(close_20, fast=5, slow=10, signal=3)
        # Where both are valid, histogram = macd - signal
        valid = ~(np.isnan(ml) | np.isnan(sl))
        np.testing.assert_allclose(hist[valid], ml[valid] - sl[valid], atol=1e-12)


# ---------------------------------------------------------------------------
# Bollinger Bands
# ---------------------------------------------------------------------------


class TestBollingerBands:
    def test_middle_equals_sma(self, close_20):
        upper, middle, lower = bollinger_bands(close_20, period=10)
        s = sma(close_20, 10)
        valid = ~np.isnan(middle)
        np.testing.assert_allclose(middle[valid], s[valid])

    def test_symmetry(self, close_20):
        upper, middle, lower = bollinger_bands(close_20, period=10, std_dev=2.0)
        valid = ~np.isnan(upper)
        np.testing.assert_allclose(
            upper[valid] - middle[valid],
            middle[valid] - lower[valid],
            atol=1e-12,
        )

    def test_upper_above_lower(self, close_20):
        upper, middle, lower = bollinger_bands(close_20, period=10)
        valid = ~np.isnan(upper)
        assert all(upper[valid] >= lower[valid])


# ---------------------------------------------------------------------------
# ATR
# ---------------------------------------------------------------------------


class TestATR:
    def test_positive(self, ohlcv_30):
        _, h, lo, c, _ = ohlcv_30
        result = atr(h, lo, c, 14)
        valid = result[~np.isnan(result)]
        assert all(v >= 0 for v in valid)

    def test_nan_prefix(self, ohlcv_30):
        _, h, lo, c, _ = ohlcv_30
        result = atr(h, lo, c, 14)
        assert all(np.isnan(result[:13]))
        assert not np.isnan(result[13])

    def test_length(self, ohlcv_30):
        _, h, lo, c, _ = ohlcv_30
        result = atr(h, lo, c, 14)
        assert len(result) == 30


# ---------------------------------------------------------------------------
# Stochastic
# ---------------------------------------------------------------------------


class TestStochastic:
    def test_k_range(self, ohlcv_30):
        _, h, lo, c, _ = ohlcv_30
        k, d = stochastic(h, lo, c, k_period=14, d_period=3)
        valid_k = k[~np.isnan(k)]
        assert all(0 <= v <= 100 for v in valid_k)

    def test_shapes(self, ohlcv_30):
        _, h, lo, c, _ = ohlcv_30
        k, d = stochastic(h, lo, c)
        assert len(k) == 30
        assert len(d) == 30


# ---------------------------------------------------------------------------
# VWAP
# ---------------------------------------------------------------------------


class TestVWAP:
    def test_within_price_range(self, ohlcv_30):
        _, h, lo, c, v = ohlcv_30
        result = vwap(h, lo, c, v)
        # VWAP should be within the overall price range
        valid = result[~np.isnan(result)]
        assert valid.min() >= lo.min() - 1
        assert valid.max() <= h.max() + 1

    def test_length(self, ohlcv_30):
        _, h, lo, c, v = ohlcv_30
        result = vwap(h, lo, c, v)
        assert len(result) == 30


# ---------------------------------------------------------------------------
# OBV
# ---------------------------------------------------------------------------


class TestOBV:
    def test_first_value_equals_volume(self, ohlcv_30):
        _, _, _, c, v = ohlcv_30
        result = obv(c, v)
        assert pytest.approx(result[0]) == v[0]

    def test_length(self, ohlcv_30):
        _, _, _, c, v = ohlcv_30
        result = obv(c, v)
        assert len(result) == 30

    def test_empty(self):
        result = obv(np.array([]), np.array([]))
        assert len(result) == 0


# ---------------------------------------------------------------------------
# ADX
# ---------------------------------------------------------------------------


class TestADX:
    def test_range(self, ohlcv_30):
        _, h, lo, c, _ = ohlcv_30
        result = adx(h, lo, c, period=7)
        valid = result[~np.isnan(result)]
        assert all(0 <= v <= 100 for v in valid)

    def test_nan_for_short_data(self):
        h = np.array([10.0, 11.0, 12.0])
        lo = np.array([9.0, 10.0, 11.0])
        c = np.array([9.5, 10.5, 11.5])
        result = adx(h, lo, c, period=14)
        assert all(np.isnan(result))
