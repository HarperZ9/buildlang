"""
Core market data structures used throughout Build Finance.

Provides Candle, Quote, Signal, Trade, and Position dataclasses
with convenience methods for common calculations.
"""

from __future__ import annotations

import time
from dataclasses import dataclass, field
from enum import Enum


class Side(str, Enum):
    BUY = "buy"
    SELL = "sell"


class SignalType(str, Enum):
    LONG = "long"
    SHORT = "short"
    CLOSE = "close"


@dataclass
class Candle:
    """OHLCV bar representing a single time period."""

    timestamp: float
    open: float
    high: float
    low: float
    close: float
    volume: float
    symbol: str = ""

    def true_range(self, prev_close: float) -> float:
        """True Range: max(high-low, |high-prev_close|, |low-prev_close|)."""
        return max(
            self.high - self.low,
            abs(self.high - prev_close),
            abs(self.low - prev_close),
        )

    def typical_price(self) -> float:
        """(High + Low + Close) / 3."""
        return (self.high + self.low + self.close) / 3.0

    def body(self) -> float:
        """Absolute size of the candle body (|close - open|)."""
        return abs(self.close - self.open)

    def is_bullish(self) -> bool:
        """True when close >= open."""
        return self.close >= self.open

    def mid(self) -> float:
        """(High + Low) / 2."""
        return (self.high + self.low) / 2.0

    def __repr__(self) -> str:
        return (
            f"Candle(ts={self.timestamp}, O={self.open}, H={self.high}, L={self.low}, C={self.close}, V={self.volume})"
        )


@dataclass
class Quote:
    """Level-1 quote (best bid / best ask)."""

    bid: float
    ask: float
    bid_size: float = 0.0
    ask_size: float = 0.0

    def mid(self) -> float:
        """Mid-point price."""
        return (self.bid + self.ask) / 2.0

    def spread(self) -> float:
        """Absolute spread (ask - bid)."""
        return self.ask - self.bid

    def spread_bps(self) -> float:
        """Spread expressed in basis points relative to the mid price."""
        m = self.mid()
        if m == 0:
            return 0.0
        return (self.spread() / m) * 10_000.0


@dataclass
class Signal:
    """Trading signal emitted by a strategy."""

    symbol: str
    side: str  # "buy" or "sell"
    strength: float  # 0.0 .. 1.0
    target_price: float | None = None
    stop_loss: float | None = None
    take_profit: float | None = None
    timestamp: float = 0.0

    def __post_init__(self) -> None:
        if self.side not in ("buy", "sell"):
            raise ValueError(f"side must be 'buy' or 'sell', got '{self.side}'")
        self.strength = max(0.0, min(1.0, self.strength))
        if self.timestamp == 0.0:
            self.timestamp = time.time()


@dataclass
class Trade:
    """Record of an executed trade."""

    symbol: str
    side: str  # "buy" or "sell"
    quantity: float
    price: float
    commission: float = 0.0
    timestamp: float = 0.0
    pnl: float = 0.0

    def __post_init__(self) -> None:
        if self.timestamp == 0.0:
            self.timestamp = time.time()

    @property
    def notional(self) -> float:
        """Absolute notional value of the trade."""
        return abs(self.quantity * self.price)

    @property
    def net_pnl(self) -> float:
        """PnL after commission."""
        return self.pnl - self.commission


@dataclass
class Position:
    """Tracks a live position in a single symbol."""

    symbol: str
    quantity: float = 0.0
    average_cost: float = 0.0
    realized_pnl: float = 0.0

    def unrealized_pnl(self, market_price: float) -> float:
        """Mark-to-market unrealized PnL."""
        return self.quantity * (market_price - self.average_cost)

    def market_value(self, market_price: float) -> float:
        """Current market value of the position."""
        return self.quantity * market_price

    def update(self, qty: float, price: float) -> None:
        """Update position with a new fill.

        Handles adding to a position, reducing, and reversing.

        Args:
            qty: Positive to buy, negative to sell.
            price: Execution price.
        """
        if self.quantity == 0:
            self.quantity = qty
            self.average_cost = price
            return

        same_direction = (self.quantity > 0 and qty > 0) or (self.quantity < 0 and qty < 0)

        if same_direction:
            total_cost = self.average_cost * abs(self.quantity) + price * abs(qty)
            self.quantity += qty
            self.average_cost = total_cost / abs(self.quantity)
        else:
            close_qty = min(abs(qty), abs(self.quantity))
            direction = 1.0 if self.quantity > 0 else -1.0
            self.realized_pnl += direction * close_qty * (price - self.average_cost)

            remaining = abs(self.quantity) - close_qty
            if remaining == 0:
                leftover = abs(qty) - close_qty
                if leftover > 0:
                    self.quantity = qty + (close_qty if qty < 0 else -close_qty)
                    self.average_cost = price
                else:
                    self.quantity = 0.0
                    self.average_cost = 0.0
            else:
                self.quantity += qty

    @property
    def is_long(self) -> bool:
        return self.quantity > 0

    @property
    def is_short(self) -> bool:
        return self.quantity < 0

    @property
    def is_flat(self) -> bool:
        return self.quantity == 0.0


# ---------------------------------------------------------------------------
# Backtest-specific data types
# ---------------------------------------------------------------------------


@dataclass
class BacktestSignal:
    """Trading signal used by the backtesting engine."""

    symbol: str
    signal_type: SignalType
    price: float
    timestamp: float
    strength: float = 1.0
    metadata: dict = field(default_factory=dict)


@dataclass
class BacktestTrade:
    """A completed round-trip trade from a backtest run."""

    symbol: str
    side: str
    entry_price: float
    exit_price: float
    quantity: float
    entry_time: float
    exit_time: float
    commission: float
    pnl: float

    @property
    def net_pnl(self) -> float:
        return self.pnl - self.commission

    @property
    def return_pct(self) -> float:
        if self.entry_price == 0:
            return 0.0
        if self.side == "buy":
            return (self.exit_price - self.entry_price) / self.entry_price
        return (self.entry_price - self.exit_price) / self.entry_price


@dataclass
class BacktestPosition:
    """An open position tracked during a backtest."""

    symbol: str
    side: str
    quantity: float
    entry_price: float
    entry_time: float
    commission: float = 0.0

    def unrealized_pnl(self, current_price: float) -> float:
        if self.side == "buy":
            return (current_price - self.entry_price) * self.quantity
        return (self.entry_price - current_price) * self.quantity
