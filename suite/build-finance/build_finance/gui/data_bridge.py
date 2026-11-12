"""
Bridge between GUI pages and backend broker/market-data modules.

Handles data fetching, caching, and graceful fallback to demo data
when Alpaca API keys are not configured.

Usage
-----
::

    bridge = DataBridge(use_paper=True)
    account = bridge.get_account()
    positions = bridge.get_positions()
    quote = bridge.get_quote("AAPL")
"""

from __future__ import annotations

import logging
import os
import random
import time
from dataclasses import dataclass, field
from datetime import datetime
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from build_finance.broker import AlpacaBroker

logger = logging.getLogger(__name__)


# ---------------------------------------------------------------------------
# Data containers
# ---------------------------------------------------------------------------


@dataclass
class AccountSnapshot:
    """Snapshot of broker account state."""

    equity: float = 0.0
    buying_power: float = 0.0
    cash: float = 0.0
    portfolio_value: float = 0.0
    timestamp: datetime = field(default_factory=datetime.now)


@dataclass
class PositionInfo:
    """A single open position."""

    symbol: str = ""
    qty: float = 0.0
    avg_entry: float = 0.0
    current_price: float = 0.0
    unrealized_pnl: float = 0.0
    pnl_percent: float = 0.0


@dataclass
class SignalRecord:
    """A single signal log entry."""

    time: str = ""
    symbol: str = ""
    direction: str = ""
    strength: float = 0.0
    action: str = ""


# ---------------------------------------------------------------------------
# Demo / mock data generators
# ---------------------------------------------------------------------------

_DEMO_SYMBOLS = ["AAPL", "MSFT", "GOOGL", "NVDA", "TSLA"]

_DEMO_WATCHLIST = [
    {
        "symbol": "AAPL",
        "last": 189.42,
        "change": 1.23,
        "change_pct": 0.65,
        "volume": 48_200_000,
        "bid": 189.40,
        "ask": 189.45,
    },
    {
        "symbol": "MSFT",
        "last": 417.88,
        "change": -2.15,
        "change_pct": -0.51,
        "volume": 22_700_000,
        "bid": 417.85,
        "ask": 417.90,
    },
    {
        "symbol": "GOOGL",
        "last": 153.21,
        "change": 0.87,
        "change_pct": 0.57,
        "volume": 19_300_000,
        "bid": 153.19,
        "ask": 153.23,
    },
    {
        "symbol": "NVDA",
        "last": 875.35,
        "change": 12.40,
        "change_pct": 1.44,
        "volume": 38_100_000,
        "bid": 875.30,
        "ask": 875.40,
    },
    {
        "symbol": "TSLA",
        "last": 172.60,
        "change": -3.80,
        "change_pct": -2.15,
        "volume": 65_400_000,
        "bid": 172.55,
        "ask": 172.65,
    },
    {
        "symbol": "AMZN",
        "last": 185.50,
        "change": 0.95,
        "change_pct": 0.51,
        "volume": 31_800_000,
        "bid": 185.48,
        "ask": 185.52,
    },
    {
        "symbol": "META",
        "last": 505.75,
        "change": 5.20,
        "change_pct": 1.04,
        "volume": 15_900_000,
        "bid": 505.70,
        "ask": 505.80,
    },
    {
        "symbol": "JPM",
        "last": 198.30,
        "change": -0.45,
        "change_pct": -0.23,
        "volume": 8_400_000,
        "bid": 198.28,
        "ask": 198.32,
    },
]


def _demo_account() -> AccountSnapshot:
    """Return a realistic-looking demo account."""
    return AccountSnapshot(
        equity=104_832.50,
        buying_power=62_415.00,
        cash=52_415.00,
        portfolio_value=52_417.50,
        timestamp=datetime.now(),
    )


def _demo_positions() -> list[PositionInfo]:
    """Return realistic demo positions."""
    return [
        PositionInfo("AAPL", 50, 178.25, 189.42, 558.50, 6.27),
        PositionInfo("NVDA", 10, 820.00, 875.35, 553.50, 6.75),
        PositionInfo("MSFT", 15, 405.10, 417.88, 191.70, 3.15),
        PositionInfo("GOOGL", 30, 148.50, 153.21, 141.30, 3.17),
    ]


def _demo_quote(symbol: str) -> dict:
    """Return a demo quote for any symbol."""
    rng = random.Random(hash(symbol))
    base = rng.uniform(50, 800)
    change = rng.uniform(-5, 5)
    return {
        "symbol": symbol,
        "last": round(base, 2),
        "change": round(change, 2),
        "change_pct": round(change / base * 100, 2),
        "volume": rng.randint(1_000_000, 60_000_000),
        "bid": round(base - 0.02, 2),
        "ask": round(base + 0.02, 2),
        "source": "demo",
    }


def _demo_watchlist(symbols: list[str]) -> list[dict]:
    """Return demo watchlist data, using built-in data where available."""
    built_in = {item["symbol"]: item for item in _DEMO_WATCHLIST}
    result = []
    for sym in symbols:
        if sym in built_in:
            result.append({**built_in[sym], "source": "demo"})
        else:
            result.append(_demo_quote(sym))
    return result


# ---------------------------------------------------------------------------
# DataBridge
# ---------------------------------------------------------------------------


class DataBridge:
    """Provides data to GUI pages from broker/market modules.

    Falls back to demo data when Alpaca credentials are not configured,
    so the GUI remains fully functional for demonstration purposes.

    Parameters
    ----------
    use_paper:
        When *True* (default), targets the Alpaca paper-trading endpoint.
    """

    def __init__(self, use_paper: bool = True) -> None:
        self._use_paper = use_paper
        self._broker: AlpacaBroker | None = None
        self._demo_mode = True
        self._cache: dict[str, tuple] = {}  # key -> (data, timestamp)
        self._cache_ttl = 5.0  # seconds

        self._init_broker()

    def _init_broker(self) -> None:
        """Attempt to create an Alpaca broker from environment variables."""
        api_key = os.environ.get("APCA_API_KEY_ID", "")
        api_secret = os.environ.get("APCA_API_SECRET_KEY", "")

        if not api_key or not api_secret:
            logger.info("Alpaca API keys not found in environment; running in demo mode.")
            self._demo_mode = True
            return

        try:
            from build_finance.broker import AlpacaBroker, BrokerConfig

            config = BrokerConfig(
                name="alpaca",
                api_key=api_key,
                api_secret=api_secret,
                paper_trading=self._use_paper,
            )
            self._broker = AlpacaBroker(config)
            self._demo_mode = False
            logger.info("Alpaca broker connected (paper=%s).", self._use_paper)
        except Exception as exc:
            logger.warning(
                "Failed to initialize Alpaca broker: %s. Falling back to demo mode.",
                exc,
            )
            self._demo_mode = True

    # -- cache helper -------------------------------------------------------

    def _get_cached(self, key: str):
        """Return cached value if fresh, else None."""
        if key in self._cache:
            data, ts = self._cache[key]
            if time.time() - ts < self._cache_ttl:
                return data
        return None

    def _set_cached(self, key: str, data) -> None:
        self._cache[key] = (data, time.time())

    # -- public API ---------------------------------------------------------

    @property
    def is_demo(self) -> bool:
        """True when running with mock data (no live broker)."""
        return self._demo_mode

    @property
    def source_label(self) -> str:
        """Human-readable label for the current data source."""
        if self._demo_mode:
            return "Demo"
        return "Alpaca (Paper)" if self._use_paper else "Alpaca (Live)"

    def get_account(self) -> AccountSnapshot:
        """Fetch account data from the broker or demo fallback."""
        cached = self._get_cached("account")
        if cached is not None:
            return cached

        if self._demo_mode or self._broker is None:
            result = _demo_account()
        else:
            try:
                info = self._broker.get_account()
                positions = self._broker.get_positions()
                pos_value = sum(p.get("market_value", 0) for p in positions.values())
                result = AccountSnapshot(
                    equity=info.equity,
                    buying_power=info.buying_power,
                    cash=info.cash,
                    portfolio_value=pos_value,
                    timestamp=datetime.now(),
                )
            except Exception as exc:
                logger.warning("Account fetch failed: %s", exc)
                result = _demo_account()

        self._set_cached("account", result)
        return result

    def get_positions(self) -> list[PositionInfo]:
        """Fetch open positions from the broker or demo fallback."""
        cached = self._get_cached("positions")
        if cached is not None:
            return cached

        if self._demo_mode or self._broker is None:
            result = _demo_positions()
        else:
            try:
                raw = self._broker.get_positions()
                result = []
                for sym, p in raw.items():
                    qty = p.get("quantity", 0)
                    avg = p.get("avg_cost", 0)
                    price = p.get("market_price", 0)
                    pnl = p.get("unrealized_pnl", 0)
                    pct = (price - avg) / avg * 100 if avg > 0 else 0.0
                    result.append(
                        PositionInfo(
                            symbol=sym,
                            qty=qty,
                            avg_entry=avg,
                            current_price=price,
                            unrealized_pnl=pnl,
                            pnl_percent=round(pct, 2),
                        )
                    )
            except Exception as exc:
                logger.warning("Positions fetch failed: %s", exc)
                result = _demo_positions()

        self._set_cached("positions", result)
        return result

    def get_quote(self, symbol: str) -> dict:
        """Fetch the latest quote for a single symbol."""
        cache_key = f"quote:{symbol}"
        cached = self._get_cached(cache_key)
        if cached is not None:
            return cached

        if self._demo_mode or self._broker is None:
            result = _demo_quote(symbol)
        else:
            try:
                raw = self._broker.get_quote(symbol)
                bid = raw.get("bid", 0)
                ask = raw.get("ask", 0)
                mid = (bid + ask) / 2 if bid and ask else 0
                result = {
                    "symbol": symbol,
                    "last": round(mid, 2),
                    "change": 0.0,
                    "change_pct": 0.0,
                    "volume": 0,
                    "bid": bid,
                    "ask": ask,
                    "source": "alpaca",
                }
            except Exception as exc:
                logger.warning("Quote fetch failed for %s: %s", symbol, exc)
                result = _demo_quote(symbol)

        self._set_cached(cache_key, result)
        return result

    def get_watchlist(self, symbols: list[str]) -> list[dict]:
        """Fetch quotes for a list of watchlist symbols."""
        cache_key = f"watchlist:{','.join(sorted(symbols))}"
        cached = self._get_cached(cache_key)
        if cached is not None:
            return cached

        if self._demo_mode:
            result = _demo_watchlist(symbols)
        else:
            result = []
            for sym in symbols:
                result.append(self.get_quote(sym))

        self._set_cached(cache_key, result)
        return result

    def flatten_all(self) -> list[dict]:
        """Close all open positions (sell everything).

        Returns a list of order results or an empty list in demo mode.
        """
        if self._demo_mode or self._broker is None:
            logger.info("Flatten all: demo mode, no real orders.")
            return []

        results = []
        try:
            positions = self._broker.get_positions()
            for sym, p in positions.items():
                qty = p.get("quantity", 0)
                if qty > 0:
                    result = self._broker.submit_order(sym, "sell", qty)
                    results.append(result)
        except Exception as exc:
            logger.error("Flatten all failed: %s", exc)
        return results

    def invalidate_cache(self) -> None:
        """Clear all cached data, forcing fresh fetches."""
        self._cache.clear()
