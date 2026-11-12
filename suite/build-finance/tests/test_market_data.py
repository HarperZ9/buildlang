"""
Tests for market data fetching, CSV I/O, paper broker, and autotrader config.

All tests run offline -- no network calls are made.
"""

from __future__ import annotations

import os
import tempfile

import pytest

from build_finance.autotrader import AutoTraderConfig
from build_finance.broker import (
    BrokerConfig,
    PaperBroker,
    get_broker,
)
from build_finance.data import Candle
from build_finance.market_data import (
    POPULAR_CRYPTO,
    POPULAR_STOCKS,
    generate_sample_data,
    list_available_symbols,
    load_csv,
    save_csv,
)

# ---------------------------------------------------------------------------
# 1. generate_sample_data produces valid candles
# ---------------------------------------------------------------------------


class TestGenerateSampleData:
    def test_returns_correct_count(self):
        candles = generate_sample_data(days=100, seed=42)
        assert len(candles) == 100

    def test_candles_have_valid_ohlcv(self):
        candles = generate_sample_data(days=50, seed=42)
        for c in candles:
            assert isinstance(c, Candle)
            assert c.high >= c.low
            assert c.high >= min(c.open, c.close)
            assert c.low <= max(c.open, c.close)
            assert c.volume > 0
            assert c.timestamp > 0

    def test_seed_reproducibility(self):
        a = generate_sample_data(days=30, seed=123)
        b = generate_sample_data(days=30, seed=123)
        assert len(a) == len(b)
        for ca, cb in zip(a, b):
            assert ca.close == cb.close
            assert ca.open == cb.open


# ---------------------------------------------------------------------------
# 2. CSV round-trip
# ---------------------------------------------------------------------------


class TestCsvRoundTrip:
    def test_save_and_load_roundtrip(self):
        original = generate_sample_data(symbol="TEST", days=20, seed=7)
        assert len(original) > 0

        with tempfile.NamedTemporaryFile(
            mode="w",
            suffix=".csv",
            delete=False,
        ) as tmp:
            tmp_path = tmp.name

        try:
            save_csv(original, tmp_path)
            loaded = load_csv(tmp_path)

            assert len(loaded) == len(original)
            for orig, load in zip(original, loaded):
                # Timestamps may lose sub-second precision through the
                # ISO-8601 string format, so compare within 1 second.
                assert abs(orig.timestamp - load.timestamp) < 1.0
                assert abs(orig.open - load.open) < 0.01
                assert abs(orig.high - load.high) < 0.01
                assert abs(orig.low - load.low) < 0.01
                assert abs(orig.close - load.close) < 0.01
                assert abs(orig.volume - load.volume) < 1.0
        finally:
            os.unlink(tmp_path)

    def test_load_missing_file_returns_empty(self):
        result = load_csv("/nonexistent/path/data.csv")
        assert result == []


# ---------------------------------------------------------------------------
# 3. POPULAR_STOCKS and POPULAR_CRYPTO are non-empty
# ---------------------------------------------------------------------------


class TestPopularSymbols:
    def test_popular_stocks_non_empty(self):
        assert len(POPULAR_STOCKS) > 0
        assert all(isinstance(s, str) for s in POPULAR_STOCKS)

    def test_popular_crypto_non_empty(self):
        assert len(POPULAR_CRYPTO) > 0
        for entry in POPULAR_CRYPTO:
            assert len(entry) == 2
            cg_id, ticker = entry
            assert isinstance(cg_id, str)
            assert isinstance(ticker, str)
            assert "-" in ticker  # e.g. "BTC-USD"

    def test_list_available_symbols_all(self):
        syms = list_available_symbols("all")
        assert len(syms) == len(POPULAR_STOCKS) + len(POPULAR_CRYPTO)

    def test_list_available_symbols_stock_only(self):
        syms = list_available_symbols("stock")
        assert len(syms) == len(POPULAR_STOCKS)
        assert all(s["asset_type"] == "stock" for s in syms)


# ---------------------------------------------------------------------------
# 4. PaperBroker tracks equity correctly
# ---------------------------------------------------------------------------


class TestPaperBrokerEquity:
    def test_initial_equity(self):
        broker = PaperBroker(initial_capital=50_000)
        account = broker.get_account()
        assert account.equity == 50_000
        assert account.cash == 50_000

    def test_equity_after_buy(self):
        broker = PaperBroker(initial_capital=100_000, slippage_bps=0)
        # Set a known price before buying
        broker.update_prices({"AAPL": 150.0})
        broker.submit_order("AAPL", "buy", 10, limit_price=150.0)

        account = broker.get_account()
        # Cash should decrease by 10 * 150 = 1500
        assert account.cash == pytest.approx(100_000 - 1500, abs=1.0)
        # Equity should remain ~100k (position value offsets cash decrease)
        assert account.equity == pytest.approx(100_000, abs=1.0)


# ---------------------------------------------------------------------------
# 5. PaperBroker submit_order updates positions
# ---------------------------------------------------------------------------


class TestPaperBrokerPositions:
    def test_buy_creates_position(self):
        broker = PaperBroker(initial_capital=100_000, slippage_bps=0)
        broker.submit_order("MSFT", "buy", 5, limit_price=300.0)

        positions = broker.get_positions()
        assert "MSFT" in positions
        assert positions["MSFT"]["quantity"] == 5

    def test_sell_closes_position(self):
        broker = PaperBroker(initial_capital=100_000, slippage_bps=0)
        broker.submit_order("MSFT", "buy", 5, limit_price=300.0)
        broker.submit_order("MSFT", "sell", 5, limit_price=310.0)

        positions = broker.get_positions()
        assert "MSFT" not in positions

    def test_trade_history_recorded(self):
        broker = PaperBroker(initial_capital=100_000, slippage_bps=0)
        broker.submit_order("TSLA", "buy", 3, limit_price=200.0)
        broker.submit_order("TSLA", "sell", 3, limit_price=210.0)

        trades = broker.get_trades()
        assert len(trades) == 2
        assert trades[0]["side"] == "buy"
        assert trades[1]["side"] == "sell"

    def test_invalid_side_raises(self):
        broker = PaperBroker()
        with pytest.raises(ValueError, match="side must be"):
            broker.submit_order("AAPL", "short", 10)


# ---------------------------------------------------------------------------
# 6. AutoTraderConfig has sensible defaults
# ---------------------------------------------------------------------------


class TestAutoTraderConfig:
    def test_default_values(self):
        config = AutoTraderConfig()
        assert config.strategy_name == "ensemble"
        assert config.interval_seconds == 300
        assert config.risk_per_trade == 0.02
        assert config.max_positions == 5
        assert config.paper_trading is True
        assert config.asset_type == "mixed"
        assert len(config.symbols) > 0

    def test_custom_values(self):
        config = AutoTraderConfig(
            symbols=["NVDA"],
            strategy_name="momentum",
            interval_seconds=60,
            risk_per_trade=0.01,
            max_positions=3,
        )
        assert config.symbols == ["NVDA"]
        assert config.strategy_name == "momentum"
        assert config.interval_seconds == 60
        assert config.risk_per_trade == 0.01
        assert config.max_positions == 3


# ---------------------------------------------------------------------------
# 7. get_broker factory
# ---------------------------------------------------------------------------


class TestGetBroker:
    def test_default_returns_paper(self):
        broker = get_broker()
        assert isinstance(broker, PaperBroker)

    def test_paper_config_returns_paper(self):
        broker = get_broker(BrokerConfig(name="paper"))
        assert isinstance(broker, PaperBroker)

    def test_unknown_broker_raises(self):
        with pytest.raises(ValueError, match="Unknown broker"):
            get_broker(BrokerConfig(name="robinhood"))
