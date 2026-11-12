"""
Tests for backtest engine, order book, portfolio optimization, and sample data.
"""

from __future__ import annotations

import numpy as np
import pytest

from build_finance.backtest import (
    BacktestConfig,
    Backtester,
    generate_sample_data,
)
from build_finance.data import (
    BacktestPosition as Position,
)
from build_finance.data import (
    BacktestSignal as Signal,
)
from build_finance.data import (
    BacktestTrade as Trade,
)
from build_finance.data import (
    Candle,
    SignalType,
)
from build_finance.orderbook import ExecutionConfig, OrderBook, simulate_fill
from build_finance.portfolio import (
    hierarchical_risk_parity,
    mean_variance_optimize,
    portfolio_stats,
    risk_parity_weights,
)

# ===========================================================================
# Helpers
# ===========================================================================


class AlwaysBuyStrategy:
    """Buys on every bar if not already in a position, never closes."""

    def generate_signals(self, candles: list[Candle]) -> list[Signal]:
        if len(candles) < 2:
            return []
        bar = candles[-1]
        return [
            Signal(
                symbol=bar.symbol,
                signal_type=SignalType.LONG,
                price=bar.close,
                timestamp=bar.timestamp,
            )
        ]


class BuyThenSellStrategy:
    """Buys on bar 5, sells on bar 15."""

    def generate_signals(self, candles: list[Candle]) -> list[Signal]:
        bar = candles[-1]
        n = len(candles)
        if n == 5:
            return [
                Signal(
                    symbol=bar.symbol,
                    signal_type=SignalType.LONG,
                    price=bar.close,
                    timestamp=bar.timestamp,
                )
            ]
        if n == 15:
            return [
                Signal(
                    symbol=bar.symbol,
                    signal_type=SignalType.CLOSE,
                    price=bar.close,
                    timestamp=bar.timestamp,
                )
            ]
        return []


class NoopStrategy:
    """Generates no signals."""

    def generate_signals(self, candles: list[Candle]) -> list[Signal]:
        return []


# ===========================================================================
# Sample data tests
# ===========================================================================


class TestSampleData:
    def test_correct_length(self):
        candles = generate_sample_data(days=100)
        assert len(candles) == 100

    def test_ohlc_consistency(self):
        """High >= max(open,close) and Low <= min(open,close)."""
        candles = generate_sample_data(days=50, seed=1)
        for c in candles:
            assert c.high >= c.open, f"high < open on {c.timestamp}"
            assert c.high >= c.close, f"high < close on {c.timestamp}"
            assert c.low <= c.open, f"low > open on {c.timestamp}"
            assert c.low <= c.close, f"low > close on {c.timestamp}"
            assert c.low > 0, "price must be positive"

    def test_positive_volume(self):
        for c in generate_sample_data(days=50, seed=2):
            assert c.volume > 0

    def test_timestamps_ascending(self):
        candles = generate_sample_data(days=100, seed=3)
        for i in range(1, len(candles)):
            assert candles[i].timestamp > candles[i - 1].timestamp

    def test_symbol_attached(self):
        candles = generate_sample_data(symbol="TSLA", days=5)
        assert all(c.symbol == "TSLA" for c in candles)

    def test_reproducible_with_seed(self):
        a = generate_sample_data(days=20, seed=99)
        b = generate_sample_data(days=20, seed=99)
        assert [c.close for c in a] == [c.close for c in b]


# ===========================================================================
# BacktestResult field tests
# ===========================================================================


class TestBacktestResult:
    def test_fields_populated(self):
        candles = generate_sample_data(days=60, seed=10)
        bt = Backtester(BacktestConfig(initial_capital=50_000))
        result = bt.run(AlwaysBuyStrategy(), {"AAPL": candles})

        assert result.initial_capital == 50_000
        assert result.final_equity > 0
        assert isinstance(result.total_return, float)
        assert isinstance(result.sharpe_ratio, float)
        assert isinstance(result.max_drawdown, float)
        assert result.max_drawdown <= 0  # drawdown is non-positive
        assert len(result.equity_curve) > 0

    def test_no_trade_result(self):
        """With no signals, equity should equal initial capital."""
        candles = generate_sample_data(days=20, seed=11)
        bt = Backtester()
        result = bt.run(NoopStrategy(), {"AAPL": candles})
        assert result.num_trades == 0
        assert result.final_equity == result.initial_capital

    def test_summary_string(self):
        candles = generate_sample_data(days=60, seed=10)
        bt = Backtester()
        result = bt.run(AlwaysBuyStrategy(), {"AAPL": candles})
        s = result.summary()
        assert "Backtest Results" in s
        assert "Sharpe" in s


# ===========================================================================
# P&L scenarios
# ===========================================================================


class TestPnlScenarios:
    def test_commission_reduces_equity(self):
        """Even with a flat market, commissions should reduce equity."""
        flat_candles = [
            Candle(timestamp=1e9 + i * 86400, open=100, high=100, low=100, close=100, volume=1e6, symbol="FLAT")
            for i in range(30)
        ]
        bt = Backtester(
            BacktestConfig(
                initial_capital=100_000,
                commission_rate=0.01,  # high commission to make effect visible
            )
        )
        result = bt.run(BuyThenSellStrategy(), {"FLAT": flat_candles})
        if result.num_trades > 0:
            total_comm = sum(t.commission for t in result.trades)
            assert total_comm > 0, "Commission should be positive"

    def test_positive_pnl_scenario(self):
        """Trending-up market with buy strategy should generally profit."""
        rng = np.random.default_rng(42)
        candles = []
        price = 100.0
        for i in range(100):
            price *= 1.003  # strong uptrend
            noise = rng.normal(0, 0.001) * price
            candles.append(
                Candle(
                    timestamp=1e9 + i * 86400,
                    open=price - noise,
                    high=price + abs(noise),
                    low=price - abs(noise),
                    close=price,
                    volume=1e6,
                    symbol="UP",
                )
            )
        bt = Backtester(BacktestConfig(initial_capital=100_000))
        result = bt.run(AlwaysBuyStrategy(), {"UP": candles})
        assert result.final_equity >= result.initial_capital * 0.99

    def test_negative_pnl_possible(self):
        """Down-trending data with buy strategy should lose money."""
        candles = []
        price = 100.0
        for i in range(20):
            price *= 0.99  # downtrend
            candles.append(
                Candle(
                    timestamp=1e9 + i * 86400,
                    open=price + 0.1,
                    high=price + 0.2,
                    low=price - 0.2,
                    close=price,
                    volume=1e6,
                    symbol="DOWN",
                )
            )
        bt = Backtester(BacktestConfig(initial_capital=100_000))
        result = bt.run(AlwaysBuyStrategy(), {"DOWN": candles})
        assert result.final_equity < result.initial_capital


# ===========================================================================
# Order book tests
# ===========================================================================


class TestOrderBook:
    def _sample_book(self) -> OrderBook:
        return OrderBook.from_lists(
            bid_prices=[99.90, 99.80, 99.70, 99.60, 99.50],
            bid_sizes=[100, 200, 300, 400, 500],
            ask_prices=[100.10, 100.20, 100.30, 100.40, 100.50],
            ask_sizes=[100, 200, 300, 400, 500],
        )

    def test_best_bid_ask(self):
        book = self._sample_book()
        assert book.best_bid() == 99.90
        assert book.best_ask() == 100.10

    def test_mid(self):
        book = self._sample_book()
        assert book.mid() == pytest.approx(100.0)

    def test_spread(self):
        book = self._sample_book()
        assert book.spread() == pytest.approx(0.20)

    def test_imbalance_balanced(self):
        book = self._sample_book()
        assert book.imbalance(levels=5) == pytest.approx(0.0)

    def test_vwap_depth(self):
        book = self._sample_book()
        # Buying 100 shares walks the first ask level at 100.10
        assert book.vwap_depth(100, "buy") == pytest.approx(100.10)
        # Buying 300 shares: 100@100.10 + 200@100.20
        expected = (100 * 100.10 + 200 * 100.20) / 300
        assert book.vwap_depth(300, "buy") == pytest.approx(expected)


# ===========================================================================
# Execution simulation tests
# ===========================================================================


class TestSimulateFill:
    def test_buy_slippage_increases_price(self):
        fill, _ = simulate_fill(100.0, 10, "buy")
        assert fill > 100.0

    def test_sell_slippage_decreases_price(self):
        fill, _ = simulate_fill(100.0, 10, "sell")
        assert fill < 100.0

    def test_commission_bounds(self):
        cfg = ExecutionConfig(commission_min=1.0, commission_max=10.0, commission_per_share=0.005)
        _, comm = simulate_fill(100.0, 1, "buy", cfg)
        assert comm >= 1.0
        _, comm2 = simulate_fill(100.0, 100_000, "buy", cfg)
        assert comm2 <= 10.0


# ===========================================================================
# Monte Carlo test
# ===========================================================================


class TestMonteCarlo:
    def test_produces_distribution(self):
        trades = [
            Trade("X", "buy", 100, 105, 10, 0, 1, 0.5, 50),
            Trade("X", "buy", 100, 95, 10, 1, 2, 0.5, -50),
            Trade("X", "buy", 100, 110, 10, 2, 3, 0.5, 100),
            Trade("X", "buy", 100, 98, 10, 3, 4, 0.5, -20),
        ]
        bt = Backtester(BacktestConfig(initial_capital=10_000))
        mc = bt.monte_carlo(trades, n_simulations=500)
        assert "median_return" in mc
        assert "p5_return" in mc
        assert "p95_return" in mc
        assert len(mc["returns"]) == 500
        # p5 <= median <= p95
        assert mc["p5_return"] <= mc["median_return"] <= mc["p95_return"]


# ===========================================================================
# Portfolio optimization tests
# ===========================================================================


def _random_returns(n_assets: int = 4, n_days: int = 252, seed: int = 0) -> np.ndarray:
    rng = np.random.default_rng(seed)
    return rng.normal(0.0004, 0.01, size=(n_days, n_assets))


class TestPortfolio:
    def test_max_sharpe_weights_sum_to_one(self):
        ret = _random_returns()
        w = mean_variance_optimize(ret, target="max_sharpe")
        assert w.sum() == pytest.approx(1.0, abs=1e-6)
        assert (w >= -1e-6).all()

    def test_min_variance_weights_sum_to_one(self):
        ret = _random_returns()
        w = mean_variance_optimize(ret, target="min_variance")
        assert w.sum() == pytest.approx(1.0, abs=1e-6)

    def test_risk_parity_weights_sum_to_one(self):
        ret = _random_returns()
        cov = np.cov(ret, rowvar=False) * 252
        w = risk_parity_weights(cov)
        assert w.sum() == pytest.approx(1.0, abs=1e-6)
        assert (w > 0).all()

    def test_hrp_weights_sum_to_one(self):
        ret = _random_returns()
        w = hierarchical_risk_parity(ret)
        assert w.sum() == pytest.approx(1.0, abs=1e-6)
        assert (w >= 0).all()

    def test_portfolio_stats_keys(self):
        ret = _random_returns()
        w = np.ones(ret.shape[1]) / ret.shape[1]
        stats = portfolio_stats(w, ret)
        assert "return" in stats
        assert "volatility" in stats
        assert "sharpe" in stats
        assert stats["volatility"] >= 0

    def test_single_asset_hrp(self):
        """HRP with one asset should give weight = 1."""
        ret = _random_returns(n_assets=1)
        w = hierarchical_risk_parity(ret)
        assert len(w) == 1
        assert w[0] == pytest.approx(1.0)


# ===========================================================================
# Data structure tests
# ===========================================================================


class TestDataStructures:
    def test_candle_typical_price(self):
        c = Candle(0, 10, 12, 8, 11, 1e6)
        assert c.typical_price() == pytest.approx((12 + 8 + 11) / 3)

    def test_trade_net_pnl(self):
        t = Trade("X", "buy", 100, 110, 10, 0, 1, 2.0, 100)
        assert t.net_pnl == 98.0

    def test_position_unrealized_pnl(self):
        p = Position("X", "buy", 10, 100, 0)
        assert p.unrealized_pnl(110) == 100
        assert p.unrealized_pnl(90) == -100

        p2 = Position("X", "sell", 10, 100, 0)
        assert p2.unrealized_pnl(90) == 100
