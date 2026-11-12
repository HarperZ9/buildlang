"""
Forecast evaluation metrics.

All functions accept array-like inputs (lists or numpy arrays) of equal length
and return a single float score.
"""

from __future__ import annotations

import math
from collections.abc import Sequence

import numpy as np

ArrayLike = Sequence[float] | np.ndarray


def _to_arrays(actual: ArrayLike, predicted: ArrayLike) -> tuple[np.ndarray, np.ndarray]:
    """Convert inputs to float64 arrays and validate equal length."""
    a = np.asarray(actual, dtype=np.float64)
    p = np.asarray(predicted, dtype=np.float64)
    if a.shape != p.shape:
        raise ValueError(f"Shape mismatch: actual {a.shape} vs predicted {p.shape}")
    if a.ndim != 1:
        raise ValueError("Inputs must be 1-D arrays")
    if len(a) == 0:
        raise ValueError("Inputs must not be empty")
    return a, p


# ---------------------------------------------------------------------------
# Core metrics
# ---------------------------------------------------------------------------


def mae(actual: ArrayLike, predicted: ArrayLike) -> float:
    """Mean Absolute Error.

    MAE = (1/n) * sum(|actual_i - predicted_i|)
    """
    a, p = _to_arrays(actual, predicted)
    return float(np.mean(np.abs(a - p)))


def mse(actual: ArrayLike, predicted: ArrayLike) -> float:
    """Mean Squared Error.

    MSE = (1/n) * sum((actual_i - predicted_i)^2)
    """
    a, p = _to_arrays(actual, predicted)
    return float(np.mean((a - p) ** 2))


def rmse(actual: ArrayLike, predicted: ArrayLike) -> float:
    """Root Mean Squared Error.

    RMSE = sqrt(MSE)
    """
    return math.sqrt(mse(actual, predicted))


def mape(actual: ArrayLike, predicted: ArrayLike) -> float:
    """Mean Absolute Percentage Error.

    MAPE = (100/n) * sum(|actual_i - predicted_i| / |actual_i|)

    Raises ValueError when any actual value is zero.
    """
    a, p = _to_arrays(actual, predicted)
    if np.any(a == 0):
        raise ValueError("MAPE is undefined when actual values contain zero")
    return float(100.0 * np.mean(np.abs((a - p) / a)))


def smape(actual: ArrayLike, predicted: ArrayLike) -> float:
    """Symmetric Mean Absolute Percentage Error.

    sMAPE = (100/n) * sum(2*|a_i - p_i| / (|a_i| + |p_i|))

    Returns 0.0 when both actual and predicted are zero for all entries.
    """
    a, p = _to_arrays(actual, predicted)
    denom = np.abs(a) + np.abs(p)
    # Where both are zero, the term contributes 0
    mask = denom != 0
    result = np.zeros_like(a)
    result[mask] = 2.0 * np.abs(a[mask] - p[mask]) / denom[mask]
    return float(100.0 * np.mean(result))


def mase(
    actual: ArrayLike,
    predicted: ArrayLike,
    seasonal_period: int = 1,
) -> float:
    """Mean Absolute Scaled Error.

    MASE = MAE / naive_MAE

    where naive_MAE is the MAE of the seasonal naive forecast
    (y[t] predicted by y[t - seasonal_period]).

    A MASE < 1 means the model beats the naive forecast.
    """
    a, p = _to_arrays(actual, predicted)
    n = len(a)
    if n <= seasonal_period:
        raise ValueError(f"Series length ({n}) must exceed seasonal_period ({seasonal_period})")
    # Naive MAE on in-sample
    naive_errors = np.abs(a[seasonal_period:] - a[:-seasonal_period])
    naive_mae = np.mean(naive_errors)
    if naive_mae == 0:
        raise ValueError("Naive MAE is zero — series is constant within season")
    return float(mae(actual, predicted) / naive_mae)


def r_squared(actual: ArrayLike, predicted: ArrayLike) -> float:
    """Coefficient of determination (R^2).

    R^2 = 1 - SS_res / SS_tot

    where SS_res = sum((a_i - p_i)^2), SS_tot = sum((a_i - mean(a))^2).
    Returns negative values when the model is worse than predicting the mean.
    """
    a, p = _to_arrays(actual, predicted)
    ss_res = np.sum((a - p) ** 2)
    ss_tot = np.sum((a - np.mean(a)) ** 2)
    if ss_tot == 0:
        # All actual values are identical
        return 1.0 if ss_res == 0 else 0.0
    return float(1.0 - ss_res / ss_tot)
