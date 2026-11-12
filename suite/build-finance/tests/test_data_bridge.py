"""
Tests for the GUI DataBridge.

All tests run offline in demo mode (no Alpaca API keys needed).
Verifies that the bridge returns well-structured mock data,
caching works, and edge cases are handled gracefully.
"""

from __future__ import annotations

import os
import time

from build_finance.gui.data_bridge import (
    AccountSnapshot,
    DataBridge,
    PositionInfo,
    SignalRecord,
    _demo_account,
    _demo_positions,
    _demo_quote,
    _demo_watchlist,
)

# ---------------------------------------------------------------------------
# 1. DataBridge starts in demo mode without API keys
# ---------------------------------------------------------------------------


class TestDataBridgeDemoMode:
    def test_starts_in_demo_mode(self):
        """Without APCA env vars, bridge falls back to demo."""
        # Ensure no API keys leak from the test environment
        env = os.environ.copy()
        os.environ.pop("APCA_API_KEY_ID", None)
        os.environ.pop("APCA_API_SECRET_KEY", None)
        try:
            bridge = DataBridge(use_paper=True)
            assert bridge.is_demo is True
            assert "Demo" in bridge.source_label
        finally:
            os.environ.update(env)

    def test_source_label_demo(self):
        bridge = DataBridge(use_paper=True)
        assert bridge.source_label == "Demo"


# ---------------------------------------------------------------------------
# 2. get_account returns valid AccountSnapshot
# ---------------------------------------------------------------------------


class TestGetAccount:
    def test_returns_account_snapshot(self):
        bridge = DataBridge()
        account = bridge.get_account()
        assert isinstance(account, AccountSnapshot)

    def test_account_has_positive_equity(self):
        bridge = DataBridge()
        account = bridge.get_account()
        assert account.equity > 0

    def test_account_has_all_fields(self):
        bridge = DataBridge()
        account = bridge.get_account()
        assert account.buying_power > 0
        assert account.cash > 0
        assert account.portfolio_value >= 0
        assert account.timestamp is not None


# ---------------------------------------------------------------------------
# 3. get_positions returns valid position list
# ---------------------------------------------------------------------------


class TestGetPositions:
    def test_returns_list(self):
        bridge = DataBridge()
        positions = bridge.get_positions()
        assert isinstance(positions, list)

    def test_positions_have_all_fields(self):
        bridge = DataBridge()
        positions = bridge.get_positions()
        assert len(positions) > 0
        for pos in positions:
            assert isinstance(pos, PositionInfo)
            assert pos.symbol != ""
            assert pos.qty > 0
            assert pos.avg_entry > 0
            assert pos.current_price > 0

    def test_position_pnl_calculation(self):
        bridge = DataBridge()
        positions = bridge.get_positions()
        for pos in positions:
            # P&L percent should match (current - avg) / avg * 100
            expected_pct = (pos.current_price - pos.avg_entry) / pos.avg_entry * 100
            assert abs(pos.pnl_percent - expected_pct) < 0.1


# ---------------------------------------------------------------------------
# 4. get_quote returns valid quote dict
# ---------------------------------------------------------------------------


class TestGetQuote:
    def test_returns_dict(self):
        bridge = DataBridge()
        quote = bridge.get_quote("AAPL")
        assert isinstance(quote, dict)

    def test_quote_has_required_keys(self):
        bridge = DataBridge()
        quote = bridge.get_quote("AAPL")
        required_keys = {"symbol", "last", "bid", "ask", "source"}
        assert required_keys.issubset(quote.keys())

    def test_quote_symbol_matches(self):
        bridge = DataBridge()
        quote = bridge.get_quote("TSLA")
        assert quote["symbol"] == "TSLA"

    def test_quote_prices_positive(self):
        bridge = DataBridge()
        quote = bridge.get_quote("GOOGL")
        assert quote["last"] > 0
        assert quote["bid"] > 0
        assert quote["ask"] > 0


# ---------------------------------------------------------------------------
# 5. get_watchlist returns a list of quote dicts
# ---------------------------------------------------------------------------


class TestGetWatchlist:
    def test_returns_list(self):
        bridge = DataBridge()
        watchlist = bridge.get_watchlist(["AAPL", "MSFT"])
        assert isinstance(watchlist, list)

    def test_correct_count(self):
        symbols = ["AAPL", "MSFT", "GOOGL"]
        bridge = DataBridge()
        watchlist = bridge.get_watchlist(symbols)
        assert len(watchlist) == len(symbols)

    def test_each_item_has_symbol(self):
        symbols = ["AAPL", "NVDA"]
        bridge = DataBridge()
        watchlist = bridge.get_watchlist(symbols)
        returned_symbols = [item["symbol"] for item in watchlist]
        for sym in symbols:
            assert sym in returned_symbols

    def test_unknown_symbol_still_returns_data(self):
        bridge = DataBridge()
        watchlist = bridge.get_watchlist(["XYZFAKE123"])
        assert len(watchlist) == 1
        assert watchlist[0]["symbol"] == "XYZFAKE123"
        assert watchlist[0]["last"] > 0


# ---------------------------------------------------------------------------
# 6. Caching works
# ---------------------------------------------------------------------------


class TestCaching:
    def test_repeated_calls_return_same_object(self):
        bridge = DataBridge()
        a1 = bridge.get_account()
        a2 = bridge.get_account()
        # Should be the exact same object from cache
        assert a1 is a2

    def test_invalidate_clears_cache(self):
        bridge = DataBridge()
        a1 = bridge.get_account()
        bridge.invalidate_cache()
        a2 = bridge.get_account()
        # After invalidation, should be a fresh object
        assert a1 is not a2

    def test_cache_ttl_expiry(self):
        bridge = DataBridge()
        bridge._cache_ttl = 0.1  # 100ms for testing
        a1 = bridge.get_account()
        time.sleep(0.15)
        a2 = bridge.get_account()
        assert a1 is not a2


# ---------------------------------------------------------------------------
# 7. flatten_all in demo mode returns empty list
# ---------------------------------------------------------------------------


class TestFlattenAll:
    def test_demo_flatten_returns_empty(self):
        bridge = DataBridge()
        result = bridge.flatten_all()
        assert result == []


# ---------------------------------------------------------------------------
# 8. Demo data generators produce valid output
# ---------------------------------------------------------------------------


class TestDemoGenerators:
    def test_demo_account_valid(self):
        acct = _demo_account()
        assert isinstance(acct, AccountSnapshot)
        assert acct.equity > 0
        assert acct.cash > 0

    def test_demo_positions_valid(self):
        positions = _demo_positions()
        assert len(positions) > 0
        for p in positions:
            assert isinstance(p, PositionInfo)

    def test_demo_quote_valid(self):
        quote = _demo_quote("AAPL")
        assert quote["symbol"] == "AAPL"
        assert quote["last"] > 0

    def test_demo_quote_deterministic(self):
        """Same symbol should produce the same demo quote."""
        q1 = _demo_quote("AAPL")
        q2 = _demo_quote("AAPL")
        assert q1["last"] == q2["last"]

    def test_demo_watchlist_built_in(self):
        wl = _demo_watchlist(["AAPL", "MSFT"])
        assert len(wl) == 2
        assert wl[0]["symbol"] == "AAPL"

    def test_demo_watchlist_unknown(self):
        wl = _demo_watchlist(["UNKNOWN_SYM"])
        assert len(wl) == 1
        assert wl[0]["symbol"] == "UNKNOWN_SYM"


# ---------------------------------------------------------------------------
# 9. SignalRecord dataclass
# ---------------------------------------------------------------------------


class TestSignalRecord:
    def test_default_values(self):
        sr = SignalRecord()
        assert sr.time == ""
        assert sr.symbol == ""
        assert sr.direction == ""
        assert sr.strength == 0.0
        assert sr.action == ""

    def test_custom_values(self):
        sr = SignalRecord(
            time="12:00:00",
            symbol="AAPL",
            direction="BUY",
            strength=0.85,
            action="Executed BUY",
        )
        assert sr.symbol == "AAPL"
        assert sr.strength == 0.85
