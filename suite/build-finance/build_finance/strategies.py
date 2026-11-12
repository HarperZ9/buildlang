"""
Trading strategies that consume Candle data and produce Signal lists.

Each strategy implements:
    ``generate_signals(candles: list[Candle]) -> list[Signal]``

Strategies
----------
MomentumStrategy        - EMA crossover + RSI filter
MeanReversionStrategy   - Bollinger Band bounce
TrendFollowingStrategy  - MA crossover + ATR trailing stops
BreakoutStrategy        - N-period high/low break with volume confirmation
EnsembleStrategy        - Weighted combination of all four strategies
"""

from __future__ import annotations

from dataclasses import dataclass, field

import numpy as np

from .data import Candle, Signal
from .indicators import (
    atr as calc_atr,
)
from .indicators import (
    bollinger_bands,
    ema,
    rsi,
    sma,
)

# ---------------------------------------------------------------------------
# helpers
# ---------------------------------------------------------------------------


def _extract_arrays(candles: list[Candle]):
    """Return (open, high, low, close, volume, timestamps) as numpy arrays."""
    o = np.array([c.open for c in candles], dtype=np.float64)
    h = np.array([c.high for c in candles], dtype=np.float64)
    lo = np.array([c.low for c in candles], dtype=np.float64)
    c = np.array([c.close for c in candles], dtype=np.float64)
    v = np.array([c.volume for c in candles], dtype=np.float64)
    ts = np.array([c.timestamp for c in candles], dtype=np.float64)
    return o, h, lo, c, v, ts


# ---------------------------------------------------------------------------
# 1. Momentum — EMA crossover + RSI filter
# ---------------------------------------------------------------------------


@dataclass
class MomentumStrategy:
    """EMA crossover with RSI confirmation.

    BUY  when fast_ema > slow_ema AND RSI < overbought (70)
    SELL when fast_ema < slow_ema AND RSI > oversold   (30)
    """

    fast_period: int = 12
    slow_period: int = 26
    rsi_period: int = 14
    rsi_overbought: float = 70.0
    rsi_oversold: float = 30.0
    symbol: str = ""

    def generate_signals(self, candles: list[Candle]) -> list[Signal]:
        if len(candles) < self.slow_period + 1:
            return []

        _, _, _, close, _, ts = _extract_arrays(candles)
        fast = ema(close, self.fast_period)
        slow = ema(close, self.slow_period)
        rsi_vals = rsi(close, self.rsi_period)

        signals: list[Signal] = []
        symbol = self.symbol or "UNKNOWN"

        for i in range(self.slow_period, len(candles)):
            if np.isnan(fast[i]) or np.isnan(slow[i]) or np.isnan(rsi_vals[i]):
                continue

            if fast[i] > slow[i] and rsi_vals[i] < self.rsi_overbought:
                # Strength proportional to how far below overbought
                strength = (self.rsi_overbought - rsi_vals[i]) / self.rsi_overbought
                signals.append(
                    Signal(
                        symbol=symbol,
                        side="buy",
                        strength=min(1.0, max(0.0, strength)),
                        target_price=close[i],
                        timestamp=ts[i],
                    )
                )
            elif fast[i] < slow[i] and rsi_vals[i] > self.rsi_oversold:
                strength = (rsi_vals[i] - self.rsi_oversold) / (100.0 - self.rsi_oversold)
                signals.append(
                    Signal(
                        symbol=symbol,
                        side="sell",
                        strength=min(1.0, max(0.0, strength)),
                        target_price=close[i],
                        timestamp=ts[i],
                    )
                )

        return signals


# ---------------------------------------------------------------------------
# 2. Mean Reversion — Bollinger Band bounce
# ---------------------------------------------------------------------------


@dataclass
class MeanReversionStrategy:
    """Bollinger Band mean-reversion.

    BUY  when price <= lower band
    SELL when price >= upper band
    """

    period: int = 20
    std_dev: float = 2.0
    symbol: str = ""

    def generate_signals(self, candles: list[Candle]) -> list[Signal]:
        if len(candles) < self.period:
            return []

        _, _, _, close, _, ts = _extract_arrays(candles)
        upper, middle, lower = bollinger_bands(close, self.period, self.std_dev)

        signals: list[Signal] = []
        symbol = self.symbol or "UNKNOWN"

        for i in range(self.period - 1, len(candles)):
            if np.isnan(upper[i]) or np.isnan(lower[i]):
                continue

            band_width = upper[i] - lower[i]
            if band_width == 0:
                continue

            if close[i] <= lower[i]:
                # Distance below lower band => strength
                overshoot = (lower[i] - close[i]) / band_width
                strength = min(1.0, 0.5 + overshoot)
                signals.append(
                    Signal(
                        symbol=symbol,
                        side="buy",
                        strength=strength,
                        target_price=middle[i],
                        stop_loss=lower[i] - 0.5 * band_width,
                        take_profit=middle[i],
                        timestamp=ts[i],
                    )
                )
            elif close[i] >= upper[i]:
                overshoot = (close[i] - upper[i]) / band_width
                strength = min(1.0, 0.5 + overshoot)
                signals.append(
                    Signal(
                        symbol=symbol,
                        side="sell",
                        strength=strength,
                        target_price=middle[i],
                        stop_loss=upper[i] + 0.5 * band_width,
                        take_profit=middle[i],
                        timestamp=ts[i],
                    )
                )

        return signals


# ---------------------------------------------------------------------------
# 3. Trend Following — MA crossover + ATR stops
# ---------------------------------------------------------------------------


@dataclass
class TrendFollowingStrategy:
    """Golden / Death cross with ATR-based trailing stops.

    BUY  on golden cross (fast MA > slow MA)
    SELL on death cross  (fast MA < slow MA)

    Stop-loss is set at ``atr_multiplier * ATR`` from entry.
    """

    fast_period: int = 50
    slow_period: int = 200
    atr_period: int = 14
    atr_multiplier: float = 2.0
    symbol: str = ""

    def generate_signals(self, candles: list[Candle]) -> list[Signal]:
        min_len = max(self.slow_period, self.atr_period) + 1
        if len(candles) < min_len:
            return []

        _, high, low, close, _, ts = _extract_arrays(candles)
        fast = sma(close, self.fast_period)
        slow = sma(close, self.slow_period)
        atr_vals = calc_atr(high, low, close, self.atr_period)

        signals: list[Signal] = []
        symbol = self.symbol or "UNKNOWN"

        for i in range(self.slow_period, len(candles)):
            if (
                np.isnan(fast[i])
                or np.isnan(slow[i])
                or np.isnan(fast[i - 1])
                or np.isnan(slow[i - 1])
                or np.isnan(atr_vals[i])
            ):
                continue

            # Golden cross
            if fast[i - 1] <= slow[i - 1] and fast[i] > slow[i]:
                stop = close[i] - self.atr_multiplier * atr_vals[i]
                signals.append(
                    Signal(
                        symbol=symbol,
                        side="buy",
                        strength=0.8,
                        target_price=close[i],
                        stop_loss=stop,
                        timestamp=ts[i],
                    )
                )
            # Death cross
            elif fast[i - 1] >= slow[i - 1] and fast[i] < slow[i]:
                stop = close[i] + self.atr_multiplier * atr_vals[i]
                signals.append(
                    Signal(
                        symbol=symbol,
                        side="sell",
                        strength=0.8,
                        target_price=close[i],
                        stop_loss=stop,
                        timestamp=ts[i],
                    )
                )

        return signals


# ---------------------------------------------------------------------------
# 4. Breakout — N-period high/low with volume confirmation
# ---------------------------------------------------------------------------


@dataclass
class BreakoutStrategy:
    """Channel breakout with volume filter.

    BUY  when close > N-period high AND volume > vol_multiplier * avg_volume
    SELL when close < N-period low  AND volume > vol_multiplier * avg_volume
    """

    lookback: int = 20
    vol_multiplier: float = 1.5
    symbol: str = ""

    def generate_signals(self, candles: list[Candle]) -> list[Signal]:
        if len(candles) < self.lookback + 1:
            return []

        _, high, low, close, volume, ts = _extract_arrays(candles)
        avg_vol = sma(volume, self.lookback)

        signals: list[Signal] = []
        symbol = self.symbol or "UNKNOWN"

        for i in range(self.lookback, len(candles)):
            window_high = np.max(high[i - self.lookback : i])
            window_low = np.min(low[i - self.lookback : i])

            if np.isnan(avg_vol[i]) or avg_vol[i] == 0:
                continue

            vol_ratio = volume[i] / avg_vol[i]
            vol_confirmed = vol_ratio >= self.vol_multiplier

            if close[i] > window_high and vol_confirmed:
                strength = min(1.0, vol_ratio / (self.vol_multiplier * 2))
                signals.append(
                    Signal(
                        symbol=symbol,
                        side="buy",
                        strength=max(0.3, strength),
                        target_price=close[i],
                        stop_loss=window_low,
                        timestamp=ts[i],
                    )
                )
            elif close[i] < window_low and vol_confirmed:
                strength = min(1.0, vol_ratio / (self.vol_multiplier * 2))
                signals.append(
                    Signal(
                        symbol=symbol,
                        side="sell",
                        strength=max(0.3, strength),
                        target_price=close[i],
                        stop_loss=window_high,
                        timestamp=ts[i],
                    )
                )

        return signals


# ---------------------------------------------------------------------------
# 5. Ensemble — weighted combination of all four strategies
# ---------------------------------------------------------------------------


@dataclass
class EnsembleStrategy:
    """Weighted ensemble of Momentum, MeanReversion, Trend, and Breakout.

    Default weights: momentum 0.25, mean_reversion 0.25,
    trend 0.30, breakout 0.20.

    Signals are aggregated per bar — if the majority agree on a direction,
    a single combined signal is emitted with averaged strength.
    """

    momentum: MomentumStrategy = field(default_factory=MomentumStrategy)
    mean_reversion: MeanReversionStrategy = field(default_factory=MeanReversionStrategy)
    trend: TrendFollowingStrategy = field(default_factory=TrendFollowingStrategy)
    breakout: BreakoutStrategy = field(default_factory=BreakoutStrategy)
    weights: dict = field(
        default_factory=lambda: {
            "momentum": 0.25,
            "mean_reversion": 0.25,
            "trend": 0.30,
            "breakout": 0.20,
        }
    )
    symbol: str = ""

    def __post_init__(self):
        for strat in (self.momentum, self.mean_reversion, self.trend, self.breakout):
            if self.symbol:
                strat.symbol = self.symbol

    def generate_signals(self, candles: list[Candle]) -> list[Signal]:
        # Collect signals from each sub-strategy
        sub_signals: dict[str, list[Signal]] = {
            "momentum": self.momentum.generate_signals(candles),
            "mean_reversion": self.mean_reversion.generate_signals(candles),
            "trend": self.trend.generate_signals(candles),
            "breakout": self.breakout.generate_signals(candles),
        }

        # Build a timestamp -> { side: (weighted_strength, weight_sum) } child safety assessment
        ts_map: dict[float, dict[str, tuple[float, float]]] = {}
        for name, sigs in sub_signals.items():
            w = self.weights.get(name, 0.0)
            for sig in sigs:
                if sig.timestamp not in ts_map:
                    ts_map[sig.timestamp] = {}
                entry = ts_map[sig.timestamp]
                prev = entry.get(sig.side, (0.0, 0.0))
                entry[sig.side] = (
                    prev[0] + sig.strength * w,
                    prev[1] + w,
                )

        symbol = self.symbol or "UNKNOWN"
        signals: list[Signal] = []

        for ts_val in sorted(ts_map):
            sides = ts_map[ts_val]
            # Pick the side with higher weighted strength
            best_side: str | None = None
            best_score = 0.0
            best_weight = 0.0
            for side, (score, weight) in sides.items():
                if score > best_score:
                    best_side = side
                    best_score = score
                    best_weight = weight

            if best_side and best_weight > 0:
                avg_strength = best_score / best_weight
                signals.append(
                    Signal(
                        symbol=symbol,
                        side=best_side,
                        strength=min(1.0, avg_strength),
                        timestamp=ts_val,
                    )
                )

        return signals
