"""
Position sizing algorithms.

All functions return the number of shares/contracts (floored to whole units
unless the market supports fractional quantities).  Every function takes
``price`` so callers can convert dollar amounts to share counts.
"""

from __future__ import annotations

import math


def fixed_amount(equity: float, amount: float, price: float) -> float:
    """Buy a fixed dollar *amount* regardless of equity.

    Returns
    -------
    Number of shares (floored).
    """
    if price <= 0:
        return 0.0
    return math.floor(amount / price)


def percent_of_equity(equity: float, pct: float, price: float) -> float:
    """Allocate *pct* (0..1) of equity to the position.

    Example: ``percent_of_equity(100_000, 0.05, 50)`` -> 100 shares.
    """
    if price <= 0 or pct <= 0:
        return 0.0
    return math.floor(equity * pct / price)


def kelly_criterion(
    win_rate: float,
    win_loss_ratio: float,
    equity: float,
    price: float,
) -> float:
    """Full Kelly position size.

    ``f* = (p * b - q) / b``

    where *p* = win_rate, *q* = 1 - p, *b* = win_loss_ratio (average
    win / average loss).

    In practice traders often use *half-Kelly* — simply halve the result.
    """
    if price <= 0 or win_loss_ratio <= 0:
        return 0.0
    q = 1.0 - win_rate
    f_star = (win_rate * win_loss_ratio - q) / win_loss_ratio
    if f_star <= 0:
        return 0.0
    return math.floor(f_star * equity / price)


def volatility_adjusted(
    equity: float,
    target_risk_pct: float,
    atr_value: float,
    price: float,
) -> float:
    """ATR-based sizing: risk *target_risk_pct* of equity per ATR unit.

    ``shares = (equity * target_risk_pct) / atr``

    This ensures each position contributes roughly equal *volatility
    dollars* to the portfolio.
    """
    if atr_value <= 0 or price <= 0:
        return 0.0
    dollar_risk = equity * target_risk_pct
    return math.floor(dollar_risk / atr_value)


def risk_parity(
    equity: float,
    stop_distance: float,
    risk_per_trade: float,
    price: float,
) -> float:
    """Risk-parity sizing using a fixed stop distance.

    ``shares = (equity * risk_per_trade) / stop_distance``

    *stop_distance* is the dollar distance from entry to stop-loss.
    *risk_per_trade* is the fraction of equity risked (e.g. 0.01 = 1 %).
    """
    if stop_distance <= 0 or price <= 0:
        return 0.0
    dollar_risk = equity * risk_per_trade
    return math.floor(dollar_risk / stop_distance)
