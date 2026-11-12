"""
Risk and performance metrics for portfolio and strategy evaluation.

All functions accept NumPy arrays and assume daily return series unless
otherwise noted.  Annualisation uses 252 trading days.
"""

from __future__ import annotations

import math
from collections.abc import Sequence

import numpy as np

TRADING_DAYS = 252


# ---- core ratios -----------------------------------------------------------


def sharpe_ratio(
    returns: np.ndarray,
    risk_free_rate: float = 0.02,
) -> float:
    """Annualised Sharpe ratio.

    ``(mean_excess * sqrt(252)) / std(returns)``
    """
    returns = np.asarray(returns, dtype=np.float64)
    if len(returns) < 2:
        return 0.0
    daily_rf = risk_free_rate / TRADING_DAYS
    excess = returns - daily_rf
    std = np.std(excess, ddof=1)
    if std < 1e-12:
        return 0.0
    return float(np.mean(excess) / std * math.sqrt(TRADING_DAYS))


def sortino_ratio(
    returns: np.ndarray,
    risk_free_rate: float = 0.02,
) -> float:
    """Annualised Sortino ratio (downside deviation denominator)."""
    returns = np.asarray(returns, dtype=np.float64)
    if len(returns) < 2:
        return 0.0
    daily_rf = risk_free_rate / TRADING_DAYS
    excess = returns - daily_rf
    downside = excess[excess < 0]
    if len(downside) == 0:
        return float("inf") if np.mean(excess) > 0 else 0.0
    dd = np.sqrt(np.mean(downside**2))
    if dd < 1e-12:
        return 0.0
    return float(np.mean(excess) / dd * math.sqrt(TRADING_DAYS))


def information_ratio(
    returns: np.ndarray,
    benchmark: np.ndarray,
) -> float:
    """Annualised Information Ratio."""
    returns = np.asarray(returns, dtype=np.float64)
    benchmark = np.asarray(benchmark, dtype=np.float64)
    if len(returns) < 2:
        return 0.0
    active = returns - benchmark
    te = np.std(active, ddof=1)
    if te < 1e-12:
        return 0.0
    return float(np.mean(active) / te * math.sqrt(TRADING_DAYS))


# ---- CAPM ------------------------------------------------------------------


def beta(
    returns: np.ndarray,
    benchmark: np.ndarray,
) -> float:
    """CAPM beta relative to *benchmark*."""
    returns = np.asarray(returns, dtype=np.float64)
    benchmark = np.asarray(benchmark, dtype=np.float64)
    if len(returns) < 2:
        return 0.0
    cov = np.cov(returns, benchmark, ddof=1)
    var_b = cov[1, 1]
    if var_b < 1e-12:
        return 0.0
    return float(cov[0, 1] / var_b)


def alpha(
    returns: np.ndarray,
    benchmark: np.ndarray,
    risk_free: float = 0.02,
) -> float:
    """Annualised Jensen's alpha."""
    returns = np.asarray(returns, dtype=np.float64)
    benchmark = np.asarray(benchmark, dtype=np.float64)
    if len(returns) < 2:
        return 0.0
    b = beta(returns, benchmark)
    risk_free / TRADING_DAYS
    ann_ret = np.mean(returns) * TRADING_DAYS
    ann_bench = np.mean(benchmark) * TRADING_DAYS
    return float(ann_ret - (risk_free + b * (ann_bench - risk_free)))


# ---- drawdown --------------------------------------------------------------


def max_drawdown(equity_curve: np.ndarray) -> float:
    """Maximum peak-to-trough drawdown (returned as a positive fraction).

    E.g. a 20 % drawdown returns ``0.20``.
    """
    equity = np.asarray(equity_curve, dtype=np.float64)
    if len(equity) < 2:
        return 0.0
    running_max = np.maximum.accumulate(equity)
    drawdowns = (running_max - equity) / np.where(running_max != 0, running_max, 1.0)
    return float(np.max(drawdowns))


def max_drawdown_duration(equity_curve: np.ndarray) -> int:
    """Duration of the longest drawdown in number of periods."""
    equity = np.asarray(equity_curve, dtype=np.float64)
    if len(equity) < 2:
        return 0
    running_max = np.maximum.accumulate(equity)
    in_drawdown = equity < running_max
    longest = 0
    current = 0
    for dd in in_drawdown:
        if dd:
            current += 1
            longest = max(longest, current)
        else:
            current = 0
    return longest


def calmar_ratio(
    returns: np.ndarray,
    equity_curve: np.ndarray,
) -> float:
    """Calmar ratio: annualised return / max drawdown."""
    returns = np.asarray(returns, dtype=np.float64)
    mdd = max_drawdown(equity_curve)
    if mdd == 0:
        return 0.0
    ann_return = float(np.mean(returns) * TRADING_DAYS)
    return ann_return / mdd


# ---- Value at Risk ----------------------------------------------------------


def var_parametric(
    returns: np.ndarray,
    confidence: float = 0.95,
) -> float:
    """Parametric (Gaussian) Value at Risk.

    Returns the loss threshold (positive number) such that losses exceed
    this value with probability ``1 - confidence``.
    """
    returns = np.asarray(returns, dtype=np.float64)
    if len(returns) < 2:
        return 0.0
    from scipy.stats import norm  # local import — scipy is optional dep

    z = norm.ppf(1 - confidence)
    mu = np.mean(returns)
    sigma = np.std(returns, ddof=1)
    return float(-(mu + z * sigma))


def var_historical(
    returns: np.ndarray,
    confidence: float = 0.95,
) -> float:
    """Historical Value at Risk (percentile method)."""
    returns = np.asarray(returns, dtype=np.float64)
    if len(returns) < 2:
        return 0.0
    percentile = (1 - confidence) * 100
    return float(-np.percentile(returns, percentile))


def cvar(
    returns: np.ndarray,
    confidence: float = 0.95,
) -> float:
    """Conditional Value at Risk (Expected Shortfall)."""
    returns = np.asarray(returns, dtype=np.float64)
    if len(returns) < 2:
        return 0.0
    threshold = np.percentile(returns, (1 - confidence) * 100)
    tail = returns[returns <= threshold]
    if len(tail) == 0:
        return 0.0
    return float(-np.mean(tail))


# ---- volatility -------------------------------------------------------------


def volatility(
    returns: np.ndarray,
    annualize: bool = True,
) -> float:
    """Standard deviation of returns, optionally annualised."""
    returns = np.asarray(returns, dtype=np.float64)
    if len(returns) < 2:
        return 0.0
    vol = float(np.std(returns, ddof=1))
    if annualize:
        vol *= math.sqrt(TRADING_DAYS)
    return vol


# ---- trade-level metrics ---------------------------------------------------


def profit_factor(trades: Sequence) -> float:
    """Gross profit / gross loss.

    *trades* is a list of objects with a ``.pnl`` attribute (e.g. ``Trade``).
    """
    gross_profit = sum(t.pnl for t in trades if t.pnl > 0)
    gross_loss = abs(sum(t.pnl for t in trades if t.pnl < 0))
    if gross_loss == 0:
        return float("inf") if gross_profit > 0 else 0.0
    return gross_profit / gross_loss


def win_rate(trades: Sequence) -> float:
    """Fraction of trades with positive PnL."""
    if len(trades) == 0:
        return 0.0
    wins = sum(1 for t in trades if t.pnl > 0)
    return wins / len(trades)
