"""
Prophet-style additive time series forecasting model.

    y(t) = trend(t) + seasonality(t) + residual(t)

- Trend: piecewise linear with automatic changepoints.
- Seasonality: Fourier series (configurable harmonics per period).
"""

from __future__ import annotations

from collections.abc import Sequence

import numpy as np

ArrayLike = Sequence[float] | np.ndarray


def _to_array(x: ArrayLike) -> np.ndarray:
    a = np.asarray(x, dtype=np.float64)
    if a.ndim != 1:
        raise ValueError("Input must be 1-D")
    return a


# ---------------------------------------------------------------------------
# Fourier feature generation
# ---------------------------------------------------------------------------


def _make_fourier_features(
    t: np.ndarray,
    period: float,
    order: int,
) -> np.ndarray:
    """Generate sin/cos Fourier features for a given period.

    Returns an (n, 2*order) matrix where columns are:
        sin(2*pi*1*t/period), cos(2*pi*1*t/period),
        sin(2*pi*2*t/period), cos(2*pi*2*t/period), ...
    """
    n = len(t)
    features = np.zeros((n, 2 * order))
    for k in range(1, order + 1):
        angle = 2.0 * np.pi * k * t / period
        features[:, 2 * (k - 1)] = np.sin(angle)
        features[:, 2 * (k - 1) + 1] = np.cos(angle)
    return features


# ---------------------------------------------------------------------------
# Piecewise linear trend
# ---------------------------------------------------------------------------


def _piecewise_linear(
    t: np.ndarray,
    changepoints: np.ndarray,
    k: float,
    m: float,
    deltas: np.ndarray,
) -> np.ndarray:
    """Evaluate piecewise linear trend at times *t*.

    Parameters
    ----------
    t : (n,) array of time indices.
    changepoints : (S,) array of changepoint time locations.
    k : base slope.
    m : base intercept.
    deltas : (S,) array of slope adjustments at each changepoint.

    The trend at time t is:
        trend(t) = (k + sum(delta_j for s_j <= t)) * t
                   + (m + sum(-s_j * delta_j for s_j <= t))
    """
    n = len(t)
    trend = np.zeros(n)
    for i in range(n):
        slope = k
        offset = m
        for j, cp in enumerate(changepoints):
            if t[i] >= cp:
                slope += deltas[j]
                offset -= changepoints[j] * deltas[j]
        trend[i] = slope * t[i] + offset
    return trend


# ---------------------------------------------------------------------------
# Prophet model
# ---------------------------------------------------------------------------


class Prophet:
    """Additive time series model with piecewise-linear trend and Fourier
    seasonality.

    Parameters
    ----------
    yearly_seasonality : bool
        Include a yearly seasonal component (period = 365.25 for daily data).
    weekly_seasonality : bool
        Include a weekly seasonal component (period = 7 for daily data).
    n_changepoints : int
        Number of potential changepoints placed in the first 80% of the series.
    fourier_order : int
        Number of Fourier harmonics for each seasonal component.
    yearly_period : float
        Override the yearly period (default 365.25).
    weekly_period : float
        Override the weekly period (default 7.0).
    """

    def __init__(
        self,
        yearly_seasonality: bool = True,
        weekly_seasonality: bool = True,
        n_changepoints: int = 25,
        fourier_order: int = 10,
        yearly_period: float = 365.25,
        weekly_period: float = 7.0,
    ):
        self.yearly_seasonality = yearly_seasonality
        self.weekly_seasonality = weekly_seasonality
        self.n_changepoints = max(n_changepoints, 0)
        self.fourier_order = max(fourier_order, 1)
        self.yearly_period = yearly_period
        self.weekly_period = weekly_period

        # Fitted state
        self._fitted = False
        self._k: float = 0.0  # base slope
        self._m: float = 0.0  # base intercept
        self._changepoints: np.ndarray = np.array([])
        self._deltas: np.ndarray = np.array([])
        self._seasonal_coeffs: np.ndarray | None = None
        self._regressor_coeffs: np.ndarray | None = None
        self._n_regressors: int = 0
        self._residual_std: float = 0.0
        self._t_train: np.ndarray | None = None
        self._y_train: np.ndarray | None = None

    # ------------------------------------------------------------------
    # Fitting
    # ------------------------------------------------------------------

    def fit(
        self,
        timestamps: ArrayLike,
        values: ArrayLike,
        regressors: np.ndarray | None = None,
    ) -> Prophet:
        """Fit the Prophet model.

        Parameters
        ----------
        timestamps : array-like of float
            Numeric time indices (e.g. days since epoch, or simply 0..n-1).
        values : array-like of float
            Observed time series values.
        regressors : (T, R) array, optional
            External regressor matrix with R regressor columns aligned to
            the timestamps.  When provided, regressor coefficients are
            estimated and included in the forecast.

        Returns
        -------
        self
        """
        t = _to_array(timestamps)
        y = _to_array(values)
        if len(t) != len(y):
            raise ValueError("timestamps and values must have equal length")
        n = len(t)
        if n < 4:
            raise ValueError("Need at least 4 data points")

        self._t_train = t.copy()
        self._y_train = y.copy()

        # Validate regressors
        if regressors is not None:
            regressors = np.asarray(regressors, dtype=np.float64)
            if regressors.ndim == 1:
                regressors = regressors.reshape(-1, 1)
            if regressors.shape[0] != n:
                raise ValueError(f"regressors rows ({regressors.shape[0]}) must match timestamps length ({n})")
            self._n_regressors = regressors.shape[1]
        else:
            self._n_regressors = 0

        # --- Step 1: Place changepoints in the first 80% of the series ---
        cp_range = int(0.8 * n)
        n_cp = min(self.n_changepoints, cp_range - 1)
        if n_cp > 0:
            cp_indices = np.linspace(1, cp_range - 1, n_cp, dtype=int)
            self._changepoints = t[cp_indices]
        else:
            self._changepoints = np.array([])

        # --- Step 2: Fit piecewise-linear trend via least squares ---------
        # Design matrix columns: [1, t, A_1, A_2, ..., A_S]
        # where A_j(t) = max(0, t - s_j)  (hinge basis)
        n_cp_actual = len(self._changepoints)
        n_trend_cols = 2 + n_cp_actual
        A_trend = np.ones((n, n_trend_cols))
        A_trend[:, 1] = t
        for j, cp in enumerate(self._changepoints):
            A_trend[:, 2 + j] = np.maximum(0.0, t - cp)

        # Solve via least squares
        trend_coeffs, _, _, _ = np.linalg.lstsq(A_trend, y, rcond=None)

        self._m = float(trend_coeffs[0])
        self._k = float(trend_coeffs[1])
        self._deltas = trend_coeffs[2:].copy() if n_cp_actual > 0 else np.array([])

        trend_fitted = A_trend @ trend_coeffs

        # --- Step 3: Fit Fourier seasonality on detrended data ------------
        detrended = y - trend_fitted

        seasonal_features = []
        if self.yearly_seasonality:
            seasonal_features.append(_make_fourier_features(t, self.yearly_period, self.fourier_order))
        if self.weekly_seasonality:
            seasonal_features.append(_make_fourier_features(t, self.weekly_period, self.fourier_order))

        if seasonal_features:
            X_seasonal = np.hstack(seasonal_features)
            self._seasonal_coeffs, _, _, _ = np.linalg.lstsq(X_seasonal, detrended, rcond=None)
            seasonal_fitted = X_seasonal @ self._seasonal_coeffs
        else:
            self._seasonal_coeffs = None
            seasonal_fitted = np.zeros(n)

        # --- Step 4: Fit external regressor coefficients ------------------
        if regressors is not None and self._n_regressors > 0:
            residual_for_regressors = y - trend_fitted - seasonal_fitted
            self._regressor_coeffs, _, _, _ = np.linalg.lstsq(regressors, residual_for_regressors, rcond=None)
            regressor_fitted = regressors @ self._regressor_coeffs
        else:
            self._regressor_coeffs = None
            regressor_fitted = np.zeros(n)

        # Residual standard deviation (for uncertainty intervals)
        residual = y - trend_fitted - seasonal_fitted - regressor_fitted
        self._residual_std = float(np.std(residual))

        self._fitted = True
        return self

    # ------------------------------------------------------------------
    # Prediction
    # ------------------------------------------------------------------

    def predict(
        self,
        future_timestamps: ArrayLike,
        regressors: np.ndarray | None = None,
    ) -> dict:
        """Generate forecasts for *future_timestamps*.

        Parameters
        ----------
        future_timestamps : array-like of float
            Numeric time indices to forecast.
        regressors : (N, R) array, optional
            External regressor values for the future timestamps.  Must have
            the same number of regressor columns as used during fitting.

        Returns
        -------
        dict with keys:
            ``yhat``       — point forecast
            ``trend``      — trend component
            ``seasonal``   — seasonal component
            ``regressors`` — regressor component (zeros if none)
            ``yhat_lower`` — lower 95% interval
            ``yhat_upper`` — upper 95% interval
        """
        if not self._fitted:
            raise RuntimeError("Model has not been fitted yet")

        t = _to_array(future_timestamps)
        n = len(t)

        # Trend
        n_cp = len(self._changepoints)
        A_trend = np.ones((n, 2 + n_cp))
        A_trend[:, 1] = t
        for j, cp in enumerate(self._changepoints):
            A_trend[:, 2 + j] = np.maximum(0.0, t - cp)

        trend_coeffs = np.concatenate([[self._m, self._k], self._deltas])
        trend = A_trend @ trend_coeffs

        # Seasonality
        seasonal_features = []
        if self.yearly_seasonality:
            seasonal_features.append(_make_fourier_features(t, self.yearly_period, self.fourier_order))
        if self.weekly_seasonality:
            seasonal_features.append(_make_fourier_features(t, self.weekly_period, self.fourier_order))

        if seasonal_features and self._seasonal_coeffs is not None:
            X_seasonal = np.hstack(seasonal_features)
            seasonal = X_seasonal @ self._seasonal_coeffs
        else:
            seasonal = np.zeros(n)

        # External regressors
        regressor_component = np.zeros(n)
        if self._regressor_coeffs is not None and self._n_regressors > 0:
            if regressors is not None:
                regressors = np.asarray(regressors, dtype=np.float64)
                if regressors.ndim == 1:
                    regressors = regressors.reshape(-1, 1)
                if regressors.shape[0] != n:
                    raise ValueError(
                        f"regressors rows ({regressors.shape[0]}) must match future_timestamps length ({n})"
                    )
                if regressors.shape[1] != self._n_regressors:
                    raise ValueError(f"Expected {self._n_regressors} regressor columns, got {regressors.shape[1]}")
                regressor_component = regressors @ self._regressor_coeffs

        yhat = trend + seasonal + regressor_component
        margin = 1.96 * self._residual_std

        return {
            "yhat": yhat,
            "trend": trend,
            "seasonal": seasonal,
            "regressors": regressor_component,
            "yhat_lower": yhat - margin,
            "yhat_upper": yhat + margin,
        }

    # ------------------------------------------------------------------
    # Persistence
    # ------------------------------------------------------------------

    def _get_state(self) -> dict:
        """Return a JSON-serializable dictionary of fitted model state."""
        if not self._fitted:
            raise RuntimeError("Model has not been fitted yet")
        return {
            "model_type": "prophet",
            "yearly_seasonality": self.yearly_seasonality,
            "weekly_seasonality": self.weekly_seasonality,
            "n_changepoints": self.n_changepoints,
            "fourier_order": self.fourier_order,
            "yearly_period": self.yearly_period,
            "weekly_period": self.weekly_period,
            "k": self._k,
            "m": self._m,
            "changepoints": self._changepoints.tolist(),
            "deltas": self._deltas.tolist(),
            "seasonal_coeffs": (self._seasonal_coeffs.tolist() if self._seasonal_coeffs is not None else None),
            "regressor_coeffs": (self._regressor_coeffs.tolist() if self._regressor_coeffs is not None else None),
            "n_regressors": self._n_regressors,
            "residual_std": self._residual_std,
        }

    @classmethod
    def _from_state(cls, state: dict) -> Prophet:
        """Reconstruct a fitted Prophet model from a state dictionary."""
        if state.get("model_type") != "prophet":
            raise ValueError(f"Expected model_type 'prophet', got '{state.get('model_type')}'")
        obj = cls(
            yearly_seasonality=state["yearly_seasonality"],
            weekly_seasonality=state["weekly_seasonality"],
            n_changepoints=state["n_changepoints"],
            fourier_order=state["fourier_order"],
            yearly_period=state["yearly_period"],
            weekly_period=state["weekly_period"],
        )
        obj._k = float(state["k"])
        obj._m = float(state["m"])
        obj._changepoints = np.array(state["changepoints"], dtype=np.float64)
        obj._deltas = np.array(state["deltas"], dtype=np.float64)
        obj._seasonal_coeffs = (
            np.array(state["seasonal_coeffs"], dtype=np.float64) if state["seasonal_coeffs"] is not None else None
        )
        regressor_coeffs = state.get("regressor_coeffs")
        obj._regressor_coeffs = np.array(regressor_coeffs, dtype=np.float64) if regressor_coeffs is not None else None
        obj._n_regressors = int(state.get("n_regressors", 0))
        obj._residual_std = float(state["residual_std"])
        obj._fitted = True
        return obj

    def save(self, path: str) -> None:
        """Save fitted model to disk as JSON."""
        import json

        state = self._get_state()
        with open(path, "w") as f:
            json.dump(state, f)

    @classmethod
    def load(cls, path: str) -> Prophet:
        """Load a previously saved model from *path*."""
        import json

        with open(path) as f:
            state = json.load(f)
        return cls._from_state(state)

    def _make_fourier_features(
        self,
        t: np.ndarray,
        period: float,
        order: int,
    ) -> np.ndarray:
        """Instance-method wrapper around module-level Fourier features."""
        return _make_fourier_features(t, period, order)

    def __repr__(self) -> str:
        return (
            f"Prophet(yearly={self.yearly_seasonality}, "
            f"weekly={self.weekly_seasonality}, "
            f"n_changepoints={self.n_changepoints}, "
            f"fourier_order={self.fourier_order})"
        )
