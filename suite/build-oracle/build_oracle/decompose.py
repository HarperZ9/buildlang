"""
Classical time series decomposition.

Decomposes a series into trend, seasonal, and residual components
using centered moving averages and period-averaging.
"""

from __future__ import annotations

import numpy as np

ArrayLike = list | np.ndarray


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _centered_moving_average(series: np.ndarray, window: int) -> np.ndarray:
    """Compute a centered moving average.

    For even window sizes, a 2xm average is applied (standard practice):
    first compute a window-length moving average, then average adjacent values
    to re-center.

    Returns an array of the same length as *series* with NaN at the edges
    where the window does not fit.
    """
    n = len(series)
    result = np.full(n, np.nan)

    if window % 2 == 1:
        # Odd window: simple centered average
        half = window // 2
        for i in range(half, n - half):
            result[i] = np.mean(series[i - half : i + half + 1])
    else:
        # Even window: 2xm moving average
        half = window // 2
        # First pass: window-length trailing average
        ma = np.full(n, np.nan)
        for i in range(window - 1, n):
            ma[i] = np.mean(series[i - window + 1 : i + 1])
        # Second pass: average adjacent values to center
        for i in range(half, n - half):
            a = ma[i + half - 1] if (i + half - 1) < n else np.nan
            b = ma[i + half] if (i + half) < n else np.nan
            if not (np.isnan(a) or np.isnan(b)):
                result[i] = (a + b) / 2.0

    return result


def _estimate_seasonal(detrended: np.ndarray, period: int) -> np.ndarray:
    """Estimate seasonal component by averaging detrended values at each
    phase position, then centering so the seasonal component sums to zero
    over one full period."""
    n = len(detrended)
    seasonal_indices = np.zeros(period)

    for k in range(period):
        values = []
        for i in range(k, n, period):
            if not np.isnan(detrended[i]):
                values.append(detrended[i])
        seasonal_indices[k] = np.mean(values) if values else 0.0

    # Center: subtract mean so seasonal sums to zero over a period
    seasonal_indices -= np.mean(seasonal_indices)

    # Tile to full series length
    seasonal = np.tile(seasonal_indices, (n // period) + 1)[:n]
    return seasonal


# ---------------------------------------------------------------------------
# Main API
# ---------------------------------------------------------------------------


def classical_decompose(
    series: ArrayLike,
    period: int,
    model: str = "additive",
) -> dict:
    """Classical time series decomposition.

    Parameters
    ----------
    series : array-like
        The time series values (1-D).
    period : int
        Length of one seasonal cycle (e.g. 12 for monthly data with yearly
        seasonality, 7 for daily data with weekly seasonality).
    model : str
        ``"additive"`` (default) or ``"multiplicative"``.
        Additive:       y = trend + seasonal + residual
        Multiplicative: y = trend * seasonal * residual

    Returns
    -------
    dict with keys ``"trend"``, ``"seasonal"``, ``"residual"`` — each a
    numpy array of the same length as *series*. Entries where the trend
    could not be computed are NaN.
    """
    y = np.asarray(series, dtype=np.float64)
    if y.ndim != 1:
        raise ValueError("series must be 1-D")
    if period < 2:
        raise ValueError("period must be >= 2")
    if len(y) < 2 * period:
        raise ValueError("series must contain at least 2 full periods")

    model = model.lower()
    if model not in ("additive", "multiplicative"):
        raise ValueError(f"model must be 'additive' or 'multiplicative', got '{model}'")

    # Step 1: Trend via centered moving average
    trend = _centered_moving_average(y, period)

    # Step 2: Detrend
    if model == "additive":
        detrended = y - trend
    else:
        with np.errstate(divide="ignore", invalid="ignore"):
            detrended = y / trend

    # Step 3: Seasonal component
    seasonal = _estimate_seasonal(detrended, period)

    # Step 4: Residual
    if model == "additive":
        residual = y - trend - seasonal
    else:
        with np.errstate(divide="ignore", invalid="ignore"):
            residual = y / (trend * seasonal)

    return {
        "trend": trend,
        "seasonal": seasonal,
        "residual": residual,
    }


def trend_strength(decomposition: dict) -> float:
    """Measure of trend strength.

    F_T = max(0, 1 - var(residual) / var(trend + residual))

    Values near 1 indicate a strong trend; near 0, no trend.
    NaN entries are excluded.
    """
    trend = decomposition["trend"]
    residual = decomposition["residual"]
    combined = trend + residual

    # Mask NaN
    mask = ~(np.isnan(combined) | np.isnan(residual))
    if mask.sum() < 2:
        return 0.0

    var_resid = np.var(residual[mask], ddof=1)
    var_combined = np.var(combined[mask], ddof=1)

    if var_combined == 0:
        return 0.0
    return float(max(0.0, 1.0 - var_resid / var_combined))


def seasonal_strength(decomposition: dict) -> float:
    """Measure of seasonal strength.

    F_S = max(0, 1 - var(residual) / var(seasonal + residual))

    Values near 1 indicate strong seasonality; near 0, none.
    NaN entries are excluded.
    """
    seasonal = decomposition["seasonal"]
    residual = decomposition["residual"]
    combined = seasonal + residual

    mask = ~(np.isnan(combined) | np.isnan(residual))
    if mask.sum() < 2:
        return 0.0

    var_resid = np.var(residual[mask], ddof=1)
    var_combined = np.var(combined[mask], ddof=1)

    if var_combined == 0:
        return 0.0
    return float(max(0.0, 1.0 - var_resid / var_combined))
