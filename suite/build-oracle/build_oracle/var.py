"""
Vector Autoregression (VAR) model for multivariate time series.

A VAR(p) model predicts K variables jointly using p lagged observations:

    Y_t = A_1 * Y_{t-1} + A_2 * Y_{t-2} + ... + A_p * Y_{t-p} + c + e_t

Coefficients are estimated via ordinary least squares on the stacked
lag matrix.
"""

from __future__ import annotations

from typing import Any

import numpy as np

JSONDict = dict[str, Any]


class VAR:
    """Vector Autoregression for multivariate time series.

    Parameters
    ----------
    p : int
        Number of lag terms (order of the VAR model).
    """

    def __init__(self, p: int = 2):
        if p < 1:
            raise ValueError("Lag order p must be >= 1")
        self.p = p

        # Fitted state
        self._fitted = False
        self._k: int = 0  # number of variables
        self._coefficients: np.ndarray | None = None  # (Kp+1, K) with intercept
        self._residuals: np.ndarray | None = None
        self._sigma: np.ndarray | None = None  # residual covariance
        self._history: np.ndarray | None = None  # last p rows for prediction

    def _require_fitted(self) -> tuple[np.ndarray, np.ndarray, np.ndarray, np.ndarray]:
        """Return (coefficients, residuals, sigma, history), raising if unfitted."""
        if (
            not self._fitted
            or self._coefficients is None
            or self._residuals is None
            or self._sigma is None
            or self._history is None
        ):
            raise RuntimeError("Model has not been fitted yet")
        return self._coefficients, self._residuals, self._sigma, self._history

    # ------------------------------------------------------------------
    # Fitting
    # ------------------------------------------------------------------

    def fit(self, data: np.ndarray) -> None:
        """Fit the VAR(p) model on multivariate data.

        Parameters
        ----------
        data : (T, K) array
            T time steps and K variables.

        Raises
        ------
        ValueError
            If data has fewer rows than p + 1 or is not 2-D.
        """
        data = np.asarray(data, dtype=np.float64)
        if data.ndim != 2:
            raise ValueError("data must be a 2-D array of shape (T, K)")
        T, K = data.shape
        if self.p + 1 > T:
            raise ValueError(f"Need at least p+1={self.p + 1} observations, got {T}")

        self._k = K

        # Build the design matrix Z and response matrix Y.
        # For each t = p..T-1:
        #   Y row = data[t]                         shape (K,)
        #   Z row = [data[t-1], data[t-2], ..., data[t-p], 1]  shape (Kp+1,)
        n_obs = T - self.p
        Z = np.ones((n_obs, K * self.p + 1), dtype=np.float64)
        Y = np.zeros((n_obs, K), dtype=np.float64)

        for i in range(n_obs):
            t = i + self.p
            Y[i] = data[t]
            for lag in range(self.p):
                start_col = lag * K
                Z[i, start_col : start_col + K] = data[t - lag - 1]
            # The last column is already 1 (intercept)

        # OLS:  coefficients = (Z'Z)^{-1} Z'Y
        ZtZ = Z.T @ Z
        ZtY = Z.T @ Y

        # Use lstsq for numerical stability
        self._coefficients, _, _, _ = np.linalg.lstsq(ZtZ, ZtY, rcond=None)

        # Residuals and covariance
        Y_hat = Z @ self._coefficients
        self._residuals = Y - Y_hat
        self._sigma = (self._residuals.T @ self._residuals) / n_obs

        # Store the last p rows for prediction
        self._history = data[-self.p :].copy()
        self._fitted = True

    # ------------------------------------------------------------------
    # Prediction
    # ------------------------------------------------------------------

    def predict(self, horizon: int) -> np.ndarray:
        """Predict the next *horizon* time steps.

        Parameters
        ----------
        horizon : int
            Number of future steps to forecast.

        Returns
        -------
        (horizon, K) array of forecasted values.
        """
        if horizon < 1:
            raise ValueError("horizon must be >= 1")
        coeffs, _residuals, _sigma, history = self._require_fitted()

        K = self._k

        # Extend history with predictions
        extended = np.vstack([history.copy(), np.zeros((horizon, K))])
        p = self.p

        for h in range(horizon):
            t = p + h
            z = np.ones(K * p + 1, dtype=np.float64)
            for lag in range(p):
                start_col = lag * K
                z[start_col : start_col + K] = extended[t - lag - 1]
            extended[t] = z @ coeffs

        return extended[p:]

    # ------------------------------------------------------------------
    # Diagnostics
    # ------------------------------------------------------------------

    @property
    def residuals(self) -> np.ndarray:
        """Return fitted residuals as (T-p, K) array."""
        _coeffs, residuals, _sigma, _history = self._require_fitted()
        return residuals.copy()

    @property
    def sigma(self) -> np.ndarray:
        """Return residual covariance matrix (K, K)."""
        _coeffs, _residuals, sigma, _history = self._require_fitted()
        return sigma.copy()

    # ------------------------------------------------------------------
    # Persistence
    # ------------------------------------------------------------------

    def _get_state(self) -> JSONDict:
        """Return a JSON-serializable dictionary of fitted model state."""
        coeffs, _residuals, sigma, history = self._require_fitted()
        return {
            "model_type": "var",
            "p": self.p,
            "k": self._k,
            "coefficients": coeffs.tolist(),
            "sigma": sigma.tolist(),
            "history": history.tolist(),
        }

    @classmethod
    def _from_state(cls, state: JSONDict) -> VAR:
        """Reconstruct a fitted VAR model from a state dictionary."""
        if state.get("model_type") != "var":
            raise ValueError(f"Expected model_type 'var', got '{state.get('model_type')}'")
        obj = cls(p=int(state["p"]))
        obj._k = int(state["k"])
        obj._coefficients = np.array(state["coefficients"], dtype=np.float64)
        obj._sigma = np.array(state["sigma"], dtype=np.float64)
        obj._history = np.array(state["history"], dtype=np.float64)
        # Residuals are not persisted (only needed for in-session diagnostics),
        # but _require_fitted() checks all fitted-state fields for None, so we
        # restore a zero-shaped placeholder consistent with the saved shapes.
        obj._residuals = np.zeros((0, obj._k), dtype=np.float64)
        obj._fitted = True
        return obj

    def save(self, path: str) -> None:
        """Save fitted model to disk as JSON."""
        import json

        state = self._get_state()
        with open(path, "w") as f:
            json.dump(state, f)

    @classmethod
    def load(cls, path: str) -> VAR:
        """Load a previously saved model from *path*."""
        import json

        with open(path) as f:
            state = json.load(f)
        return cls._from_state(state)

    def __repr__(self) -> str:
        status = "fitted" if self._fitted else "unfitted"
        k_str = f", K={self._k}" if self._fitted else ""
        return f"VAR(p={self.p}{k_str}, {status})"
