"""
Vectorised technical indicators built on NumPy.

Every function accepts and returns ``numpy.ndarray`` objects.  The first
*period - 1* values are set to ``NaN`` where insufficient data exists to
compute the indicator.

Functions
---------
sma, ema, rsi, macd, bollinger_bands, atr, stochastic, vwap, adx, obv
"""

from __future__ import annotations

import numpy as np

# ---- helpers ----------------------------------------------------------------


def _validate(arr: np.ndarray, name: str = "data") -> np.ndarray:
    """Ensure *arr* is a 1-D float64 ndarray."""
    arr = np.asarray(arr, dtype=np.float64)
    if arr.ndim != 1:
        raise ValueError(f"{name} must be 1-D, got shape {arr.shape}")
    return arr


def _nan_prefix(length: int, period: int) -> np.ndarray:
    """Return an array of *period - 1* NaNs."""
    return np.full(min(period - 1, length), np.nan)


# ---- moving averages -------------------------------------------------------


def sma(data: np.ndarray, period: int) -> np.ndarray:
    """Simple Moving Average.

    Returns an array of the same length as *data* with the first
    ``period - 1`` entries set to NaN.
    """
    data = _validate(data)
    if period < 1:
        raise ValueError("period must be >= 1")
    n = len(data)
    if n == 0:
        return np.array([], dtype=np.float64)
    out = np.full(n, np.nan)
    if n < period:
        return out
    cumsum = np.cumsum(data)
    out[period - 1 :] = (cumsum[period - 1 :] - np.concatenate(([0.0], cumsum[:-period]))) / period
    return out


def ema(data: np.ndarray, period: int) -> np.ndarray:
    """Exponential Moving Average (Wilder smoothing).

    Uses SMA of the first *period* values as the seed, then applies
    ``alpha = 2 / (period + 1)`` recursively.
    """
    data = _validate(data)
    if period < 1:
        raise ValueError("period must be >= 1")
    n = len(data)
    if n == 0:
        return np.array([], dtype=np.float64)
    out = np.full(n, np.nan)
    if n < period:
        return out
    alpha = 2.0 / (period + 1)
    out[period - 1] = np.mean(data[:period])
    for i in range(period, n):
        out[i] = alpha * data[i] + (1 - alpha) * out[i - 1]
    return out


# ---- oscillators & momentum ------------------------------------------------


def rsi(close: np.ndarray, period: int = 14) -> np.ndarray:
    """Relative Strength Index (Wilder smoothing)."""
    close = _validate(close, "close")
    n = len(close)
    out = np.full(n, np.nan)
    if n < period + 1:
        return out

    delta = np.diff(close)
    gains = np.where(delta > 0, delta, 0.0)
    losses = np.where(delta < 0, -delta, 0.0)

    avg_gain = np.mean(gains[:period])
    avg_loss = np.mean(losses[:period])

    out[period] = 100.0 - 100.0 / (1.0 + avg_gain / avg_loss) if avg_loss != 0 else 100.0

    for i in range(period, len(delta)):
        avg_gain = (avg_gain * (period - 1) + gains[i]) / period
        avg_loss = (avg_loss * (period - 1) + losses[i]) / period
        if avg_loss == 0:
            out[i + 1] = 100.0
        else:
            out[i + 1] = 100.0 - 100.0 / (1.0 + avg_gain / avg_loss)
    return out


def macd(
    close: np.ndarray,
    fast: int = 12,
    slow: int = 26,
    signal: int = 9,
) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    """MACD indicator.

    Returns
    -------
    (macd_line, signal_line, histogram)
    """
    close = _validate(close, "close")
    fast_ema = ema(close, fast)
    slow_ema = ema(close, slow)
    macd_line = fast_ema - slow_ema
    # Signal line: EMA of the MACD line (only where MACD is valid)
    signal_line = ema(macd_line[~np.isnan(macd_line)], signal)
    # Pad signal_line back to original length
    pad = len(close) - len(signal_line)
    signal_line = np.concatenate((np.full(pad, np.nan), signal_line))
    histogram = macd_line - signal_line
    return macd_line, signal_line, histogram


def stochastic(
    high: np.ndarray,
    low: np.ndarray,
    close: np.ndarray,
    k_period: int = 14,
    d_period: int = 3,
) -> tuple[np.ndarray, np.ndarray]:
    """Stochastic Oscillator (%K, %D).

    %K = (Close - Lowest Low) / (Highest High - Lowest Low) * 100
    %D = SMA(%K, d_period)
    """
    high = _validate(high, "high")
    low = _validate(low, "low")
    close = _validate(close, "close")
    n = len(close)
    k = np.full(n, np.nan)

    for i in range(k_period - 1, n):
        hh = np.max(high[i - k_period + 1 : i + 1])
        ll = np.min(low[i - k_period + 1 : i + 1])
        rng = hh - ll
        k[i] = ((close[i] - ll) / rng * 100.0) if rng != 0 else 50.0

    d = sma(k[~np.isnan(k)], d_period) if np.any(~np.isnan(k)) else np.full(n, np.nan)
    pad = n - len(d)
    d = np.concatenate((np.full(pad, np.nan), d))
    return k, d


# ---- volatility & bands ----------------------------------------------------


def bollinger_bands(
    close: np.ndarray,
    period: int = 20,
    std_dev: float = 2.0,
) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    """Bollinger Bands.

    Returns
    -------
    (upper_band, middle_band, lower_band)
    """
    close = _validate(close, "close")
    middle = sma(close, period)
    n = len(close)
    std = np.full(n, np.nan)
    for i in range(period - 1, n):
        std[i] = np.std(close[i - period + 1 : i + 1], ddof=0)
    upper = middle + std_dev * std
    lower = middle - std_dev * std
    return upper, middle, lower


def atr(
    high: np.ndarray,
    low: np.ndarray,
    close: np.ndarray,
    period: int = 14,
) -> np.ndarray:
    """Average True Range (Wilder smoothing)."""
    high = _validate(high, "high")
    low = _validate(low, "low")
    close = _validate(close, "close")
    n = len(close)
    out = np.full(n, np.nan)
    if n < 2:
        return out

    # True Range series (first element uses high-low only)
    tr = np.empty(n)
    tr[0] = high[0] - low[0]
    for i in range(1, n):
        tr[i] = max(
            high[i] - low[i],
            abs(high[i] - close[i - 1]),
            abs(low[i] - close[i - 1]),
        )

    if n < period:
        return out

    out[period - 1] = np.mean(tr[:period])
    for i in range(period, n):
        out[i] = (out[i - 1] * (period - 1) + tr[i]) / period
    return out


# ---- volume indicators -----------------------------------------------------


def vwap(
    high: np.ndarray,
    low: np.ndarray,
    close: np.ndarray,
    volume: np.ndarray,
) -> np.ndarray:
    """Volume Weighted Average Price (cumulative intraday)."""
    high = _validate(high, "high")
    low = _validate(low, "low")
    close = _validate(close, "close")
    volume = _validate(volume, "volume")

    tp = (high + low + close) / 3.0
    cum_tp_vol = np.cumsum(tp * volume)
    cum_vol = np.cumsum(volume)
    # Avoid division by zero
    out = np.where(cum_vol != 0, cum_tp_vol / cum_vol, np.nan)
    return out


def obv(close: np.ndarray, volume: np.ndarray) -> np.ndarray:
    """On-Balance Volume."""
    close = _validate(close, "close")
    volume = _validate(volume, "volume")
    n = len(close)
    if n == 0:
        return np.array([], dtype=np.float64)
    out = np.empty(n)
    out[0] = volume[0]
    direction = np.sign(np.diff(close))
    out[1:] = volume[1:] * direction
    return np.cumsum(out)


# ---- trend ------------------------------------------------------------------


def adx(
    high: np.ndarray,
    low: np.ndarray,
    close: np.ndarray,
    period: int = 14,
) -> np.ndarray:
    """Average Directional Index.

    Measures the *strength* of a trend regardless of direction.
    Values above 25 indicate a strong trend.
    """
    high = _validate(high, "high")
    low = _validate(low, "low")
    close = _validate(close, "close")
    n = len(close)
    out = np.full(n, np.nan)
    if n < period + 1:
        return out

    # True Range
    tr = np.empty(n)
    tr[0] = high[0] - low[0]
    up_move = np.empty(n)
    down_move = np.empty(n)
    up_move[0] = 0.0
    down_move[0] = 0.0

    for i in range(1, n):
        tr[i] = max(
            high[i] - low[i],
            abs(high[i] - close[i - 1]),
            abs(low[i] - close[i - 1]),
        )
        up = high[i] - high[i - 1]
        down = low[i - 1] - low[i]
        up_move[i] = up if (up > down and up > 0) else 0.0
        down_move[i] = down if (down > up and down > 0) else 0.0

    # Smoothed TR, +DM, -DM (Wilder smoothing)
    atr_s = np.full(n, np.nan)
    plus_dm_s = np.full(n, np.nan)
    minus_dm_s = np.full(n, np.nan)

    atr_s[period] = np.sum(tr[1 : period + 1])
    plus_dm_s[period] = np.sum(up_move[1 : period + 1])
    minus_dm_s[period] = np.sum(down_move[1 : period + 1])

    for i in range(period + 1, n):
        atr_s[i] = atr_s[i - 1] - atr_s[i - 1] / period + tr[i]
        plus_dm_s[i] = plus_dm_s[i - 1] - plus_dm_s[i - 1] / period + up_move[i]
        minus_dm_s[i] = minus_dm_s[i - 1] - minus_dm_s[i - 1] / period + down_move[i]

    # +DI, -DI
    plus_di = np.full(n, np.nan)
    minus_di = np.full(n, np.nan)
    for i in range(period, n):
        if atr_s[i] != 0:
            plus_di[i] = 100.0 * plus_dm_s[i] / atr_s[i]
            minus_di[i] = 100.0 * minus_dm_s[i] / atr_s[i]
        else:
            plus_di[i] = 0.0
            minus_di[i] = 0.0

    # DX
    dx = np.full(n, np.nan)
    for i in range(period, n):
        di_sum = plus_di[i] + minus_di[i]
        if di_sum != 0:
            dx[i] = 100.0 * abs(plus_di[i] - minus_di[i]) / di_sum
        else:
            dx[i] = 0.0

    # ADX: first value is SMA of first `period` DX values, then Wilder smoothing
    first_valid = period
    valid_dx = dx[first_valid:]
    valid_dx_clean = valid_dx[~np.isnan(valid_dx)]

    if len(valid_dx_clean) < period:
        return out

    adx_start_idx = first_valid + period - 1
    if adx_start_idx >= n:
        return out

    out[adx_start_idx] = np.mean(valid_dx_clean[:period])
    for i in range(adx_start_idx + 1, n):
        if not np.isnan(dx[i]):
            out[i] = (out[i - 1] * (period - 1) + dx[i]) / period
    return out
