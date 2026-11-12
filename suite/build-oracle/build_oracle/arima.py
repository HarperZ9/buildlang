"""
ARIMA (AutoRegressive Integrated Moving Average) model.

Provides fitting via Yule-Walker / Levinson-Durbin for the AR component
and residual-based estimation for the MA component, plus an ``auto_arima``
grid-search function.
"""

from __future__ import annotations

from collections.abc import Sequence
from typing import Any

import numpy as np

ArrayLike = Sequence[float] | np.ndarray


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _to_array(series: ArrayLike) -> np.ndarray:
    a = np.asarray(series, dtype=np.float64)
    if a.ndim != 1:
        raise ValueError("series must be 1-D")
    if len(a) == 0:
        raise ValueError("series must not be empty")
    return a


def _difference(y: np.ndarray, d: int) -> np.ndarray:
    """Apply differencing *d* times."""
    for _ in range(d):
        y = np.diff(y)
    return y


def _undifference(forecast: np.ndarray, history: np.ndarray, d: int) -> np.ndarray:
    """Reverse differencing using the tail of the original series."""
    for _ in range(d):
        # Prepend the last known level and cumsum
        last = history[-1]
        forecast = last + np.cumsum(forecast)
        history = np.diff(history)  # unused beyond first iteration
        # Narrow history for deeper undifferencing
        if d > 1:
            history = history  # already shortened
    return forecast


def _autocov(y: np.ndarray, max_lag: int) -> np.ndarray:
    """Biased autocovariance estimates for lags 0..max_lag."""
    n = len(y)
    mean = np.mean(y)
    result = np.zeros(max_lag + 1)
    for k in range(max_lag + 1):
        result[k] = np.sum((y[: n - k] - mean) * (y[k:] - mean)) / n
    return result


def _levinson_durbin(acov: np.ndarray, order: int) -> np.ndarray:
    """Levinson-Durbin recursion to solve Yule-Walker equations.

    Returns AR coefficients phi[1..order] (1-indexed convention stored in
    a 0-indexed array of length *order*).
    """
    if order == 0:
        return np.array([])

    # Initialise
    phi = np.zeros(order)
    phi_prev = np.zeros(order)

    # First order
    phi[0] = acov[1] / acov[0] if acov[0] != 0 else 0.0
    var = acov[0] * (1.0 - phi[0] ** 2)

    for k in range(1, order):
        # Reflection coefficient
        num = acov[k + 1] - np.dot(phi[:k], acov[k:0:-1])
        if var == 0:
            phi[k] = 0.0
        else:
            phi[k] = num / var

        phi_prev[:k] = phi[:k].copy()
        for j in range(k):
            phi[j] = phi_prev[j] - phi[k] * phi_prev[k - 1 - j]

        var *= 1.0 - phi[k] ** 2

    return phi


def _estimate_ma(residuals: np.ndarray, q: int) -> np.ndarray:
    """Estimate MA coefficients from residual autocorrelations.

    Uses the innovation algorithm approximation: theta_k ~ acf(k) for
    small q.  This is a simplified estimator suitable for moderate-order
    MA models.
    """
    if q == 0:
        return np.array([])
    n = len(residuals)
    if n < q + 1:
        return np.zeros(q)

    acov = _autocov(residuals, q)
    if acov[0] == 0:
        return np.zeros(q)
    acf = acov[1:] / acov[0]

    # Clamp to (-1, 1) for invertibility
    theta = np.clip(acf, -0.99, 0.99)
    return theta


# ---------------------------------------------------------------------------
# ARIMA class
# ---------------------------------------------------------------------------


class ARIMA:
    """ARIMA(p, d, q) time series model.

    Parameters
    ----------
    p : int
        Order of the autoregressive (AR) component.
    d : int
        Order of differencing.
    q : int
        Order of the moving average (MA) component.
    """

    def __init__(self, p: int = 1, d: int = 1, q: int = 1):
        if p < 0 or d < 0 or q < 0:
            raise ValueError("Orders p, d, q must be non-negative")
        self.p = p
        self.d = d
        self.q = q

        # Fitted parameters (populated by .fit())
        self.phi: np.ndarray | None = None  # AR coefficients
        self.theta: np.ndarray | None = None  # MA coefficients
        self.intercept: float = 0.0
        self.sigma2: float = 0.0  # residual variance
        self._series: np.ndarray | None = None  # original series
        self._diff_series: np.ndarray | None = None
        self._residuals: np.ndarray | None = None
        self._fitted = False

    # ----- Fitting --------------------------------------------------------

    def fit(self, series: ArrayLike) -> ARIMA:
        """Fit the ARIMA model to *series*.

        Steps:
        1. Apply differencing *d* times.
        2. Estimate AR parameters via Levinson-Durbin.
        3. Compute residuals.
        4. Estimate MA parameters from residual autocorrelation.
        """
        y = _to_array(series)
        self._series = y.copy()

        # Step 1 — differencing
        z = _difference(y, self.d)
        if len(z) < max(self.p, self.q) + 1:
            raise ValueError("Series too short after differencing for the chosen orders")
        self._diff_series = z.copy()
        self.intercept = float(np.mean(z))
        z_centered = z - self.intercept

        # Step 2 — AR estimation via Yule-Walker
        if self.p > 0:
            acov = _autocov(z_centered, self.p)
            self.phi = _levinson_durbin(acov, self.p)
        else:
            self.phi = np.array([])

        # Step 3 — residuals
        n = len(z_centered)
        residuals = np.zeros(n)
        for t in range(n):
            ar_part = 0.0
            for j in range(self.p):
                if t - j - 1 >= 0:
                    ar_part += self.phi[j] * z_centered[t - j - 1]
            residuals[t] = z_centered[t] - ar_part
        self._residuals = residuals

        # Step 4 — MA estimation
        if self.q > 0:
            self.theta = _estimate_ma(residuals, self.q)
        else:
            self.theta = np.array([])

        # Residual variance
        self.sigma2 = float(np.var(residuals, ddof=self.p + self.q))
        if self.sigma2 <= 0:
            self.sigma2 = float(np.var(residuals))

        self._fitted = True
        return self

    # ----- Prediction -----------------------------------------------------

    def predict(self, horizon: int) -> np.ndarray:
        """Forecast *horizon* steps ahead.

        For the differenced series:
          z[t+h] = intercept + sum(phi[j] * z[t+h-j]) + sum(theta[j] * eps[t+h-j])

        MA residuals beyond the known range are set to 0 (expectation).
        Then undoes differencing to return forecasts on the original scale.
        """
        self._check_fitted()
        if horizon < 1:
            raise ValueError("horizon must be >= 1")
        assert self._diff_series is not None
        assert self._residuals is not None
        assert self._series is not None
        assert self.phi is not None
        assert self.theta is not None

        z = self._diff_series.copy()
        z_centered = z - self.intercept
        residuals = self._residuals.copy()
        n = len(z_centered)

        forecasts = np.zeros(horizon)
        # Extend z and residuals with forecasts
        z_ext = np.concatenate([z_centered, np.zeros(horizon)])
        r_ext = np.concatenate([residuals, np.zeros(horizon)])

        for h in range(horizon):
            t = n + h
            ar_part = 0.0
            for j in range(self.p):
                if t - j - 1 >= 0:
                    ar_part += self.phi[j] * z_ext[t - j - 1]
            ma_part = 0.0
            for j in range(self.q):
                if t - j - 1 >= 0:
                    ma_part += self.theta[j] * r_ext[t - j - 1]
            z_ext[t] = ar_part + ma_part
            forecasts[h] = z_ext[t] + self.intercept

        # Undo differencing
        if self.d > 0:
            forecasts = _undo_diff_multi(forecasts, self._series, self.d)

        return forecasts

    # ----- Information criteria -------------------------------------------

    def aic(self) -> float:
        """Akaike Information Criterion.

        AIC = n * ln(sigma2) + 2 * (p + q + 1)
        """
        self._check_fitted()
        assert self._diff_series is not None
        n = len(self._diff_series)
        k = self.p + self.q + 1
        s2 = max(self.sigma2, 1e-300)
        return float(n * np.log(s2) + 2 * k)

    def bic(self) -> float:
        """Bayesian Information Criterion.

        BIC = n * ln(sigma2) + (p + q + 1) * ln(n)
        """
        self._check_fitted()
        assert self._diff_series is not None
        n = len(self._diff_series)
        k = self.p + self.q + 1
        s2 = max(self.sigma2, 1e-300)
        return float(n * np.log(s2) + k * np.log(n))

    # ----- Persistence ---------------------------------------------------

    def _get_state(self) -> dict[str, Any]:
        """Return a JSON-serializable dictionary of fitted model state."""
        self._check_fitted()
        return {
            "model_type": "arima",
            "p": self.p,
            "d": self.d,
            "q": self.q,
            "phi": self.phi.tolist() if self.phi is not None else [],
            "theta": self.theta.tolist() if self.theta is not None else [],
            "intercept": self.intercept,
            "sigma2": self.sigma2,
            "series": self._series.tolist() if self._series is not None else [],
            "diff_series": self._diff_series.tolist() if self._diff_series is not None else [],
            "residuals": self._residuals.tolist() if self._residuals is not None else [],
        }

    @classmethod
    def _from_state(cls, state: dict[str, Any]) -> ARIMA:
        """Reconstruct a fitted ARIMA model from a state dictionary."""
        if state.get("model_type") != "arima":
            raise ValueError(f"Expected model_type 'arima', got '{state.get('model_type')}'")
        obj = cls(p=state["p"], d=state["d"], q=state["q"])
        obj.phi = np.array(state["phi"], dtype=np.float64)
        obj.theta = np.array(state["theta"], dtype=np.float64)
        obj.intercept = float(state["intercept"])
        obj.sigma2 = float(state["sigma2"])
        obj._series = np.array(state["series"], dtype=np.float64)
        obj._diff_series = np.array(state["diff_series"], dtype=np.float64)
        obj._residuals = np.array(state["residuals"], dtype=np.float64)
        obj._fitted = True
        return obj

    def save(self, path: str) -> None:
        """Save fitted model to disk as JSON."""
        import json

        state = self._get_state()
        with open(path, "w") as f:
            json.dump(state, f)

    @classmethod
    def load(cls, path: str) -> ARIMA:
        """Load a previously saved model from *path*."""
        import json

        with open(path) as f:
            state = json.load(f)
        return cls._from_state(state)

    # ----- Internals ------------------------------------------------------

    def _check_fitted(self) -> None:
        if not self._fitted:
            raise RuntimeError("Model has not been fitted yet")

    def __repr__(self) -> str:
        return f"ARIMA(p={self.p}, d={self.d}, q={self.q})"


# ---------------------------------------------------------------------------
# Undifferencing helper
# ---------------------------------------------------------------------------


def _undo_diff_multi(
    forecasts: np.ndarray,
    original: np.ndarray,
    d: int,
) -> np.ndarray:
    """Undo *d* levels of differencing.

    At each level, we prepend the last known value of the previous
    integration level and cumulative-sum.
    """
    # Build the integration anchors
    levels = [original.copy()]
    for _i in range(d - 1):
        levels.append(np.diff(levels[-1]))

    result = forecasts.copy()
    for i in range(d):
        level_series = levels[d - 1 - i]
        anchor = level_series[-1]
        result = anchor + np.cumsum(result)

    return result


# ---------------------------------------------------------------------------
# auto_arima
# ---------------------------------------------------------------------------


def auto_arima(
    series: ArrayLike,
    max_p: int = 5,
    max_d: int = 2,
    max_q: int = 5,
) -> ARIMA:
    """Select the best ARIMA(p, d, q) by minimising AIC via grid search.

    Parameters
    ----------
    series : array-like
        The time series to model.
    max_p, max_d, max_q : int
        Upper bounds (inclusive) for the grid search.

    Returns
    -------
    The fitted :class:`ARIMA` model with the lowest AIC.
    """
    y = _to_array(series)
    best_aic = float("inf")
    best_model: ARIMA | None = None

    for d in range(max_d + 1):
        for p in range(max_p + 1):
            for q in range(max_q + 1):
                if p == 0 and q == 0:
                    continue  # degenerate
                try:
                    model = ARIMA(p=p, d=d, q=q)
                    model.fit(y)
                    score = model.aic()
                    if score < best_aic:
                        best_aic = score
                        best_model = model
                except (ValueError, np.linalg.LinAlgError):
                    continue

    if best_model is None:
        # Fallback
        model = ARIMA(p=1, d=1, q=0)
        model.fit(y)
        return model

    return best_model
