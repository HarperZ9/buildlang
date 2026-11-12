"""
Feature engineering for time series data.

Provides statistical, temporal, rolling, and lag-based feature extraction
as well as common transformations (differencing, log, Box-Cox).
"""

from __future__ import annotations

from collections.abc import Sequence

import numpy as np
from scipy import stats as sp_stats

ArrayLike = Sequence[float] | np.ndarray


def _to_array(series: ArrayLike) -> np.ndarray:
    a = np.asarray(series, dtype=np.float64)
    if a.ndim != 1:
        raise ValueError("series must be 1-D")
    if len(a) == 0:
        raise ValueError("series must not be empty")
    return a


# ---------------------------------------------------------------------------
# Statistical features
# ---------------------------------------------------------------------------


def statistical_features(series: ArrayLike) -> dict:
    """Compute a dictionary of summary statistics for the series.

    Keys: mean, std, var, min, max, range, skewness, kurtosis, median,
          iqr, cv (coefficient of variation), rms (root mean square).
    """
    y = _to_array(series)
    n = len(y)

    mean_val = float(np.mean(y))
    std_val = float(np.std(y, ddof=1)) if n > 1 else 0.0
    var_val = float(np.var(y, ddof=1)) if n > 1 else 0.0
    min_val = float(np.min(y))
    max_val = float(np.max(y))
    range_val = max_val - min_val
    median_val = float(np.median(y))
    q1, q3 = float(np.percentile(y, 25)), float(np.percentile(y, 75))
    iqr_val = q3 - q1
    rms_val = float(np.sqrt(np.mean(y**2)))

    # Skewness and kurtosis (excess kurtosis, Fisher definition)
    if n > 2:
        skew_val = float(sp_stats.skew(y, bias=False))
    else:
        skew_val = 0.0
    if n > 3:
        kurt_val = float(sp_stats.kurtosis(y, bias=False))
    else:
        kurt_val = 0.0

    # Coefficient of variation
    cv_val = std_val / abs(mean_val) if mean_val != 0 else float("inf")

    return {
        "mean": mean_val,
        "std": std_val,
        "var": var_val,
        "min": min_val,
        "max": max_val,
        "range": range_val,
        "skewness": skew_val,
        "kurtosis": kurt_val,
        "median": median_val,
        "iqr": iqr_val,
        "cv": cv_val,
        "rms": rms_val,
    }


# ---------------------------------------------------------------------------
# Temporal features
# ---------------------------------------------------------------------------


def _autocorrelation(y: np.ndarray, lag: int) -> float:
    """Sample autocorrelation at a given lag."""
    n = len(y)
    if lag >= n:
        return 0.0
    mean = np.mean(y)
    c0 = np.sum((y - mean) ** 2)
    if c0 == 0:
        return 0.0
    ck = np.sum((y[lag:] - mean) * (y[: n - lag] - mean))
    return float(ck / c0)


def temporal_features(series: ArrayLike, max_lag: int = 20) -> dict:
    """Compute temporal features.

    Returns a dict with:
        ``autocorrelation`` — list of autocorrelation values at lags 1..max_lag
        ``trend``           — slope of a simple linear regression (OLS) on the series
    """
    y = _to_array(series)
    n = len(y)

    actual_max_lag = min(max_lag, n - 1)
    acf = [_autocorrelation(y, lag) for lag in range(1, actual_max_lag + 1)]

    # Trend via linear regression slope: beta = cov(x, y) / var(x)
    x = np.arange(n, dtype=np.float64)
    x_mean = np.mean(x)
    y_mean = np.mean(y)
    cov_xy = np.sum((x - x_mean) * (y - y_mean))
    var_x = np.sum((x - x_mean) ** 2)
    trend_slope = float(cov_xy / var_x) if var_x != 0 else 0.0

    return {
        "autocorrelation": acf,
        "trend": trend_slope,
    }


# ---------------------------------------------------------------------------
# Rolling features
# ---------------------------------------------------------------------------


def rolling_features(series: ArrayLike, window: int = 20) -> dict:
    """Compute rolling (moving window) statistics.

    Returns dict with arrays ``rolling_mean``, ``rolling_std``,
    ``rolling_min``, ``rolling_max`` — each of length ``n``.
    The first ``window-1`` entries are NaN.
    """
    y = _to_array(series)
    n = len(y)
    if window < 1:
        raise ValueError("window must be >= 1")
    window = min(window, n)

    r_mean = np.full(n, np.nan)
    r_std = np.full(n, np.nan)
    r_min = np.full(n, np.nan)
    r_max = np.full(n, np.nan)

    # Use cumulative sums for efficient rolling mean / std
    for i in range(window - 1, n):
        segment = y[i - window + 1 : i + 1]
        r_mean[i] = np.mean(segment)
        r_std[i] = np.std(segment, ddof=1) if window > 1 else 0.0
        r_min[i] = np.min(segment)
        r_max[i] = np.max(segment)

    return {
        "rolling_mean": r_mean,
        "rolling_std": r_std,
        "rolling_min": r_min,
        "rolling_max": r_max,
    }


# ---------------------------------------------------------------------------
# Lag features
# ---------------------------------------------------------------------------


def lag_features(
    series: ArrayLike,
    lags: Sequence[int] = (1, 2, 3, 5, 10, 20),
) -> np.ndarray:
    """Create a lagged feature matrix.

    For each lag ``k`` in *lags*, column ``j`` contains ``series[t - k]``.
    Rows that would require out-of-bounds lookups are removed, so the output
    has shape ``(n - max_lag, len(lags))``.

    Parameters
    ----------
    series : array-like
        1-D time series.
    lags : sequence of int
        Positive lag values.

    Returns
    -------
    np.ndarray of shape ``(n - max(lags), len(lags))``
    """
    y = _to_array(series)
    n = len(y)
    lags = list(lags)
    if not lags:
        raise ValueError("lags must not be empty")
    max_lag = max(lags)
    if max_lag >= n:
        raise ValueError(f"max lag ({max_lag}) must be < series length ({n})")

    rows = n - max_lag
    matrix = np.empty((rows, len(lags)), dtype=np.float64)
    for j, k in enumerate(lags):
        for i in range(rows):
            matrix[i, j] = y[max_lag + i - k]
    return matrix


# ---------------------------------------------------------------------------
# Transformations
# ---------------------------------------------------------------------------


def difference(series: ArrayLike, order: int = 1) -> np.ndarray:
    """Apply differencing *order* times.

    First-order differencing: y'[t] = y[t] - y[t-1]
    Each application reduces the series length by 1.
    """
    y = _to_array(series)
    if order < 0:
        raise ValueError("order must be >= 0")
    for _ in range(order):
        y = np.diff(y)
    return y


def log_transform(series: ArrayLike) -> np.ndarray:
    """Natural log transform.  All values must be positive."""
    y = _to_array(series)
    if np.any(y <= 0):
        raise ValueError("log_transform requires all values > 0")
    return np.log(y)


def box_cox(
    series: ArrayLike,
    lam: float | None = None,
) -> tuple[np.ndarray, float]:
    """Box-Cox power transformation.

    If *lam* is ``None``, the optimal lambda is found via maximum likelihood
    (using ``scipy.stats.boxcox``).

    Parameters
    ----------
    series : array-like
        Must be strictly positive.
    lam : float or None
        Fixed lambda.  When ``None``, lambda is estimated.

    Returns
    -------
    (transformed, lambda) — the transformed series and the lambda used.
    """
    y = _to_array(series)
    if np.any(y <= 0):
        raise ValueError("Box-Cox requires all values > 0")

    if lam is None:
        transformed, fitted_lam = sp_stats.boxcox(y)
        return transformed, float(fitted_lam)

    # Manual transform
    if lam == 0:
        transformed = np.log(y)
    else:
        transformed = (y**lam - 1.0) / lam
    return transformed, float(lam)
