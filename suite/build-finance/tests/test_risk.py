"""
Tests for build_finance.risk

~30 tests covering Sharpe, Sortino, drawdown, VaR, CVaR, volatility,
CAPM metrics, and trade-level statistics.
"""

import math
from dataclasses import dataclass

import numpy as np
import pytest

from build_finance.risk import (
    alpha,
    beta,
    calmar_ratio,
    cvar,
    information_ratio,
    max_drawdown,
    max_drawdown_duration,
    profit_factor,
    sharpe_ratio,
    sortino_ratio,
    var_historical,
    volatility,
    win_rate,
)

# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------


@pytest.fixture
def daily_returns():
    """252 days of simulated daily returns (mean ~0.04% / day)."""
    np.random.seed(123)
    return np.random.normal(0.0004, 0.01, 252)


@pytest.fixture
def equity_curve(daily_returns):
    """Equity curve from daily returns starting at 10 000."""
    return 10_000 * np.cumprod(1 + daily_returns)


@pytest.fixture
def benchmark_returns():
    np.random.seed(456)
    return np.random.normal(0.0003, 0.009, 252)


@dataclass
class _FakeTrade:
    pnl: float


# ---------------------------------------------------------------------------
# Sharpe
# ---------------------------------------------------------------------------


class TestSharpe:
    def test_positive_returns(self, daily_returns):
        sr = sharpe_ratio(daily_returns, risk_free_rate=0.02)
        assert isinstance(sr, float)

    def test_zero_vol(self):
        flat = np.ones(100) * 0.001
        sr = sharpe_ratio(flat)
        # constant excess returns => std ≈ 0 but not exactly due to rf
        # the function should not crash
        assert isinstance(sr, float)

    def test_single_value(self):
        assert sharpe_ratio(np.array([0.01])) == 0.0

    def test_empty(self):
        assert sharpe_ratio(np.array([])) == 0.0

    def test_negative_returns_negative_sharpe(self):
        # Add a tiny bit of noise so std != 0
        np.random.seed(77)
        bad = np.random.normal(-0.005, 0.001, 252)
        sr = sharpe_ratio(bad, risk_free_rate=0.0)
        assert sr < 0


# ---------------------------------------------------------------------------
# Sortino
# ---------------------------------------------------------------------------


class TestSortino:
    def test_higher_than_sharpe_for_positive_skew(self):
        # A series with few negative returns
        np.random.seed(99)
        returns = np.abs(np.random.normal(0.001, 0.005, 252))
        so = sortino_ratio(returns, risk_free_rate=0.0)
        # No downside => inf
        assert so == float("inf") or so > 5.0

    def test_short_array(self):
        assert sortino_ratio(np.array([0.01])) == 0.0

    def test_type(self, daily_returns):
        assert isinstance(sortino_ratio(daily_returns), float)


# ---------------------------------------------------------------------------
# Max Drawdown
# ---------------------------------------------------------------------------


class TestMaxDrawdown:
    def test_known_drawdown(self):
        # Peak at 100, trough at 80 => 20 % drawdown
        curve = np.array([100, 110, 105, 90, 80, 95, 100])
        assert pytest.approx(max_drawdown(curve), abs=1e-10) == (110 - 80) / 110

    def test_monotonically_increasing(self):
        curve = np.arange(1.0, 100.0)
        assert max_drawdown(curve) == 0.0

    def test_single_point(self):
        assert max_drawdown(np.array([100])) == 0.0


class TestMaxDrawdownDuration:
    def test_known_duration(self):
        curve = np.array([100, 110, 105, 100, 95, 90, 95, 110, 120])
        # Drawdown from index 2 through index 6 (5 bars below peak of 110)
        # Index 7 equals the prior peak so is no longer in drawdown
        dur = max_drawdown_duration(curve)
        assert dur == 5

    def test_no_drawdown(self):
        curve = np.arange(1.0, 10.0)
        assert max_drawdown_duration(curve) == 0


# ---------------------------------------------------------------------------
# Calmar
# ---------------------------------------------------------------------------


class TestCalmar:
    def test_positive(self, daily_returns, equity_curve):
        cr = calmar_ratio(daily_returns, equity_curve)
        assert isinstance(cr, float)

    def test_no_drawdown(self):
        returns = np.full(100, 0.001)
        curve = 100 * np.cumprod(1 + returns)
        cr = calmar_ratio(returns, curve)
        assert cr == 0.0  # max_drawdown == 0 => returns 0


# ---------------------------------------------------------------------------
# VaR
# ---------------------------------------------------------------------------


class TestVaRHistorical:
    def test_positive(self, daily_returns):
        v = var_historical(daily_returns, confidence=0.95)
        assert v > 0  # typical for a returns series with negatives

    def test_99_greater_than_95(self, daily_returns):
        v95 = var_historical(daily_returns, 0.95)
        v99 = var_historical(daily_returns, 0.99)
        assert v99 >= v95

    def test_short(self):
        assert var_historical(np.array([0.01]), 0.95) == 0.0


class TestCVaR:
    def test_greater_than_var(self, daily_returns):
        v = var_historical(daily_returns, 0.95)
        cv = cvar(daily_returns, 0.95)
        assert cv >= v

    def test_short(self):
        assert cvar(np.array([0.01]), 0.95) == 0.0


# ---------------------------------------------------------------------------
# Volatility
# ---------------------------------------------------------------------------


class TestVolatility:
    def test_annualized_greater(self, daily_returns):
        ann = volatility(daily_returns, annualize=True)
        daily = volatility(daily_returns, annualize=False)
        assert ann > daily
        assert pytest.approx(ann, rel=1e-10) == daily * math.sqrt(252)

    def test_empty(self):
        assert volatility(np.array([])) == 0.0


# ---------------------------------------------------------------------------
# CAPM: beta, alpha, information ratio
# ---------------------------------------------------------------------------


class TestBeta:
    def test_self_beta(self, daily_returns):
        b = beta(daily_returns, daily_returns)
        assert pytest.approx(b, abs=1e-10) == 1.0

    def test_short(self):
        assert beta(np.array([0.01]), np.array([0.02])) == 0.0


class TestAlpha:
    def test_type(self, daily_returns, benchmark_returns):
        a = alpha(daily_returns, benchmark_returns)
        assert isinstance(a, float)


class TestInformationRatio:
    def test_identical(self, daily_returns):
        ir = information_ratio(daily_returns, daily_returns)
        # identical series => active return 0 => IR 0 or NaN-safe 0
        assert ir == 0.0

    def test_type(self, daily_returns, benchmark_returns):
        ir = information_ratio(daily_returns, benchmark_returns)
        assert isinstance(ir, float)


# ---------------------------------------------------------------------------
# Trade-level metrics
# ---------------------------------------------------------------------------


class TestProfitFactor:
    def test_known(self):
        trades = [_FakeTrade(100), _FakeTrade(50), _FakeTrade(-30), _FakeTrade(-20)]
        pf = profit_factor(trades)
        assert pytest.approx(pf, rel=1e-10) == 150 / 50

    def test_no_losses(self):
        trades = [_FakeTrade(10), _FakeTrade(20)]
        assert profit_factor(trades) == float("inf")

    def test_no_wins(self):
        trades = [_FakeTrade(-10), _FakeTrade(-5)]
        assert profit_factor(trades) == 0.0

    def test_empty(self):
        assert profit_factor([]) == 0.0


class TestWinRate:
    def test_known(self):
        trades = [_FakeTrade(10), _FakeTrade(-5), _FakeTrade(3), _FakeTrade(-1)]
        assert pytest.approx(win_rate(trades)) == 0.5

    def test_all_winners(self):
        trades = [_FakeTrade(10)] * 5
        assert win_rate(trades) == 1.0

    def test_empty(self):
        assert win_rate([]) == 0.0
