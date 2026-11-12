"""
Order execution simulation and order book modeling.

Provides realistic fill-price estimation with slippage, commission,
and market-impact models, plus a simple L2 order book.
"""

from __future__ import annotations

import math
from dataclasses import dataclass

# ---------------------------------------------------------------------------
# Execution simulation
# ---------------------------------------------------------------------------


@dataclass
class ExecutionConfig:
    """Tunable knobs for fill-price simulation."""

    slippage_bps: float = 5.0  # Basis points of slippage
    commission_per_share: float = 0.005  # Per-share commission
    commission_min: float = 1.0  # Floor
    commission_max: float = 10.0  # Cap
    market_impact_factor: float = 0.0001  # Linear impact coefficient


def simulate_fill(
    price: float,
    quantity: float,
    side: str,
    config: ExecutionConfig | None = None,
) -> tuple[float, float]:
    """Simulate order execution and return ``(fill_price, commission)``.

    Parameters
    ----------
    price:
        Reference (mid / last) price.
    quantity:
        Number of shares (positive).
    side:
        ``"buy"`` or ``"sell"``.
    config:
        Execution parameters; uses defaults when *None*.
    """
    if config is None:
        config = ExecutionConfig()

    # --- slippage -----------------------------------------------------------
    slippage = price * config.slippage_bps / 10_000
    if side == "buy":
        fill = price + slippage
    else:
        fill = price - slippage

    # --- market impact (linear in quantity) ---------------------------------
    impact = abs(quantity) * config.market_impact_factor
    if side == "buy":
        fill *= 1.0 + impact
    else:
        fill *= 1.0 - impact

    # --- commission ---------------------------------------------------------
    raw_commission = abs(quantity) * config.commission_per_share
    commission = max(config.commission_min, min(config.commission_max, raw_commission))

    return fill, commission


# ---------------------------------------------------------------------------
# Order book
# ---------------------------------------------------------------------------


@dataclass(order=True)
class OrderBookLevel:
    """Single price level in the order book."""

    price: float
    size: float

    def notional(self) -> float:
        """Dollar value at this level."""
        return self.price * self.size


class OrderBook:
    """Simple limit-order book with L2 (price + aggregated size) data.

    *bids* are stored **descending** by price (best bid first).
    *asks* are stored **ascending** by price (best ask first).
    """

    def __init__(
        self,
        bids: list[OrderBookLevel] | None = None,
        asks: list[OrderBookLevel] | None = None,
    ) -> None:
        self.bids: list[OrderBookLevel] = sorted(
            bids or [],
            key=lambda l: l.price,
            reverse=True,
        )
        self.asks: list[OrderBookLevel] = sorted(
            asks or [],
            key=lambda l: l.price,
        )

    # -- convenience constructors -------------------------------------------

    @classmethod
    def from_lists(
        cls,
        bid_prices: list[float],
        bid_sizes: list[float],
        ask_prices: list[float],
        ask_sizes: list[float],
    ) -> OrderBook:
        """Build an *OrderBook* from flat price/size lists."""
        bids = [OrderBookLevel(p, s) for p, s in zip(bid_prices, bid_sizes)]
        asks = [OrderBookLevel(p, s) for p, s in zip(ask_prices, ask_sizes)]
        return cls(bids=bids, asks=asks)

    # -- top-of-book --------------------------------------------------------

    def best_bid(self) -> float:
        """Highest bid price, or *0.0* if book is empty."""
        return self.bids[0].price if self.bids else 0.0

    def best_ask(self) -> float:
        """Lowest ask price, or *inf* if book is empty."""
        return self.asks[0].price if self.asks else math.inf

    def mid(self) -> float:
        """Mid-point between best bid and best ask."""
        bb = self.best_bid()
        ba = self.best_ask()
        if bb == 0.0 or ba == math.inf:
            return 0.0
        return (bb + ba) / 2.0

    def spread(self) -> float:
        """Absolute spread (ask - bid)."""
        return self.best_ask() - self.best_bid()

    def spread_bps(self) -> float:
        """Spread in basis points relative to mid price."""
        m = self.mid()
        if m == 0.0:
            return 0.0
        return (self.spread() / m) * 10_000

    # -- depth analytics ----------------------------------------------------

    def imbalance(self, levels: int = 5) -> float:
        """Order-book imbalance over the top *levels* on each side.

        Returns a value in ``[-1, 1]``.  Positive means more bid volume
        (buying pressure); negative means more ask volume.
        """
        bid_vol = sum(l.size for l in self.bids[:levels])
        ask_vol = sum(l.size for l in self.asks[:levels])
        total = bid_vol + ask_vol
        if total == 0.0:
            return 0.0
        return (bid_vol - ask_vol) / total

    def vwap_depth(self, depth: float, side: str) -> float:
        """Volume-weighted average price to fill *depth* shares on *side*.

        Parameters
        ----------
        depth:
            Total shares to fill.
        side:
            ``"buy"`` (walk the asks) or ``"sell"`` (walk the bids).

        Returns the VWAP fill price, or 0.0 if there is insufficient
        liquidity.
        """
        levels = self.asks if side == "buy" else self.bids
        remaining = abs(depth)
        notional = 0.0

        for level in levels:
            fill_qty = min(remaining, level.size)
            notional += fill_qty * level.price
            remaining -= fill_qty
            if remaining <= 0:
                break

        filled = abs(depth) - remaining
        if filled == 0.0:
            return 0.0
        return notional / filled

    def total_bid_depth(self, levels: int | None = None) -> float:
        """Total bid volume across *levels* (or all levels)."""
        subset = self.bids[:levels] if levels else self.bids
        return sum(l.size for l in subset)

    def total_ask_depth(self, levels: int | None = None) -> float:
        """Total ask volume across *levels* (or all levels)."""
        subset = self.asks[:levels] if levels else self.asks
        return sum(l.size for l in subset)

    # -- mutations ----------------------------------------------------------

    def add_level(self, side: str, price: float, size: float) -> None:
        """Insert or update a price level."""
        levels = self.bids if side == "bid" else self.asks
        for lv in levels:
            if lv.price == price:
                lv.size = size
                return
        levels.append(OrderBookLevel(price, size))
        if side == "bid":
            levels.sort(key=lambda l: l.price, reverse=True)
        else:
            levels.sort(key=lambda l: l.price)

    def remove_level(self, side: str, price: float) -> None:
        """Remove a price level."""
        levels = self.bids if side == "bid" else self.asks
        self_list = [l for l in levels if l.price != price]
        if side == "bid":
            self.bids = self_list
        else:
            self.asks = self_list

    # -- dunder -------------------------------------------------------------

    def __repr__(self) -> str:
        bb = f"{self.best_bid():.2f}" if self.bids else "---"
        ba = f"{self.best_ask():.2f}" if self.asks else "---"
        return (
            f"OrderBook(bid={bb}, ask={ba}, "
            f"spread={self.spread():.4f}, "
            f"bid_levels={len(self.bids)}, ask_levels={len(self.asks)})"
        )
