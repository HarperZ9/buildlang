"""
Ensemble Forecaster -- Dynamic multi-model combination.

Combines ARIMA, Prophet-style, and Neural forecasters with
accuracy-based dynamic weighting. Models that perform better
on recent data get higher weight in the ensemble prediction.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Protocol

import numpy as np


class _SubModel(Protocol):
    """Structural type for the heterogeneous sub-models an ensemble may hold.

    ARIMA, Prophet, and SimpleForecaster each expose a ``predict`` method
    with a model-specific signature/return type, so this Protocol only
    pins down the common calling shape used via ``_predict_single``.
    """

    def predict(self, x: Any) -> Any: ...


@dataclass
class EnsembleConfig:
    """Configuration for the ensemble forecaster."""

    use_arima: bool = True
    use_prophet: bool = True
    use_neural: bool = True
    validation_window: int = 20  # lookback for weight calculation
    min_weight: float = 0.05  # minimum weight per model (prevents zero-out)
    reweight_interval: int = 10  # recalculate weights every N steps
    # ARIMA hyperparameters
    arima_p: int = 2
    arima_d: int = 1
    arima_q: int = 2
    # Neural hyperparameters
    neural_lookback: int = 20
    neural_hidden: int = 64
    neural_epochs: int = 100
    neural_lr: float = 0.001


class EnsembleForecaster:
    """
    Dynamic ensemble combining multiple forecasting models.

    Weights are calculated based on each model's recent accuracy
    (inverse of MAE on validation window). Models with lower error
    get higher weight.
    """

    def __init__(self, config: EnsembleConfig | None = None):
        self._config = config or EnsembleConfig()
        self._models: dict[str, _SubModel] = {}
        self._weights: np.ndarray = np.array([])
        self._model_names: list[str] = []
        self._model_mae: dict[str, float] = {}
        self._fitted = False
        self._training_data: np.ndarray | None = None
        self._fit_count: int = 0

    # ----- Fitting --------------------------------------------------------

    def fit(self, data: np.ndarray, periods: int = 1) -> EnsembleForecaster:
        """Fit all sub-models on the training data.

        Parameters
        ----------
        data : 1-D array
            Time series to fit.
        periods : int
            Seasonal period hint (unused by some sub-models but passed
            through for consistency).

        Returns
        -------
        self
        """
        data = np.asarray(data, dtype=np.float64).ravel()
        if len(data) < 30:
            raise ValueError("Need at least 30 data points for ensemble")

        self._training_data = data.copy()
        cfg = self._config
        val_win = min(cfg.validation_window, len(data) // 4)
        train_part = data[:-val_win]
        val_part = data[-val_win:]

        self._model_names = []
        self._models = {}
        self._model_mae = {}

        # --- ARIMA ---
        if cfg.use_arima:
            self._fit_arima(train_part, val_part, val_win)

        # --- Prophet ---
        if cfg.use_prophet:
            self._fit_prophet(train_part, val_part, val_win)

        # --- Neural ---
        if cfg.use_neural:
            self._fit_neural(train_part, val_part, val_win, data)

        if not self._model_names:
            raise RuntimeError("All sub-models failed to fit")

        self._weights = self._calculate_weights()
        self._fitted = True
        self._fit_count += 1
        return self

    # ----- Sub-model fitting helpers -------------------------------------

    def _fit_arima(
        self,
        train: np.ndarray,
        val: np.ndarray,
        val_win: int,
    ) -> None:
        """Attempt to fit ARIMA and score on validation window."""
        cfg = self._config
        try:
            from build_oracle.arima import ARIMA

            model = ARIMA(p=cfg.arima_p, d=cfg.arima_d, q=cfg.arima_q)
            model.fit(train)
            forecast = model.predict(val_win)
            forecast = np.asarray(forecast, dtype=np.float64)[:val_win]
            error = float(np.mean(np.abs(val[: len(forecast)] - forecast)))
            self._models["arima"] = model
            self._model_names.append("arima")
            self._model_mae["arima"] = error
        except (ValueError, np.linalg.LinAlgError, RuntimeError):
            pass  # exclude failed model

    def _fit_prophet(
        self,
        train: np.ndarray,
        val: np.ndarray,
        val_win: int,
    ) -> None:
        """Attempt to fit Prophet and score on validation window."""
        try:
            from build_oracle.prophet import Prophet

            t_train = np.arange(len(train), dtype=np.float64)
            model = Prophet()
            model.fit(t_train, train)
            t_val = np.arange(len(train), len(train) + val_win, dtype=np.float64)
            result = model.predict(t_val)
            forecast = np.asarray(result["yhat"], dtype=np.float64)[:val_win]
            error = float(np.mean(np.abs(val[: len(forecast)] - forecast)))
            self._models["prophet"] = model
            self._model_names.append("prophet")
            self._model_mae["prophet"] = error
        except (ValueError, np.linalg.LinAlgError, RuntimeError):
            pass  # exclude failed model

    def _fit_neural(
        self,
        train: np.ndarray,
        val: np.ndarray,
        val_win: int,
        full_data: np.ndarray,
    ) -> None:
        """Attempt to fit SimpleForecaster and score on validation window."""
        cfg = self._config
        try:
            from build_oracle.neural import SimpleForecaster

            lookback = min(cfg.neural_lookback, len(train) // 3)
            if lookback < 5:
                return
            horizon = min(val_win, len(train) // 3)
            if horizon < 1:
                return
            model = SimpleForecaster(
                lookback=lookback,
                horizon=horizon,
                hidden=cfg.neural_hidden,
            )
            model.fit(train, epochs=cfg.neural_epochs, lr=cfg.neural_lr)
            forecast = model.predict(train)
            forecast = np.asarray(forecast, dtype=np.float64)[:val_win]
            # Pad if forecast is shorter than val window
            if len(forecast) < val_win:
                forecast = np.pad(
                    forecast,
                    (0, val_win - len(forecast)),
                    mode="edge",
                )
            error = float(np.mean(np.abs(val[: len(forecast)] - forecast)))
            self._models["neural"] = model
            self._model_names.append("neural")
            self._model_mae["neural"] = error
        except (ValueError, np.linalg.LinAlgError, RuntimeError):
            pass  # exclude failed model

    # ----- Prediction -----------------------------------------------------

    def predict(self, steps: int = 1) -> np.ndarray:
        """Generate weighted ensemble prediction.

        Parameters
        ----------
        steps : int
            Number of future steps to forecast.

        Returns
        -------
        (steps,) array of ensemble predictions.
        """
        if not self._fitted:
            raise RuntimeError("Model has not been fitted yet")
        if steps < 1:
            raise ValueError("steps must be >= 1")

        # Optionally reweight
        if (self._fit_count % self._config.reweight_interval) == 0:
            self._weights = self._calculate_weights()

        predictions: list[np.ndarray] = []
        active_weights: list[float] = []

        for i, name in enumerate(self._model_names):
            try:
                pred = self._predict_single(name, steps)
                pred = np.asarray(pred, dtype=np.float64)
                if len(pred) < steps:
                    pred = np.pad(pred, (0, steps - len(pred)), mode="edge")
                predictions.append(pred[:steps])
                active_weights.append(float(self._weights[i]))
            except (ValueError, KeyError):
                continue  # skip failed prediction

        if not predictions:
            raise RuntimeError("All sub-models failed to predict")

        # Renormalize weights for active models
        w = np.array(active_weights, dtype=np.float64)
        w = w / w.sum()

        # Weighted average
        stacked = np.column_stack(predictions)  # (steps, n_models)
        ensemble = stacked @ w
        return ensemble

    def _predict_single(self, name: str, steps: int) -> np.ndarray:
        """Dispatch prediction to the named sub-model."""
        model = self._models[name]
        if name == "arima":
            return np.asarray(model.predict(steps), dtype=np.float64)
        elif name == "prophet":
            assert self._training_data is not None
            n_train = len(self._training_data) - self._config.validation_window
            t_future = np.arange(n_train, n_train + steps, dtype=np.float64)
            result = model.predict(t_future)
            return np.asarray(result["yhat"], dtype=np.float64)
        elif name == "neural":
            assert self._training_data is not None
            return np.asarray(model.predict(self._training_data), dtype=np.float64)
        else:
            raise ValueError(f"Unknown model: {name}")

    # ----- Weight calculation ---------------------------------------------

    def _calculate_weights(self) -> np.ndarray:
        """Calculate accuracy-based weights using validation MAE.

        Weights are proportional to 1/MAE (inverse error). A floor of
        ``min_weight`` prevents any model from being zeroed out entirely.
        The floor is enforced iteratively: models at the floor are locked,
        and the remaining budget is redistributed proportionally among
        the unclamped models.
        """
        if not self._model_names:
            return np.array([])

        cfg = self._config
        n = len(self._model_names)
        mae_vals = np.array(
            [self._model_mae.get(name, float("inf")) for name in self._model_names],
            dtype=np.float64,
        )

        # Inverse MAE weighting (add small epsilon to avoid division by zero)
        inv_mae = 1.0 / (mae_vals + 1e-10)
        raw_weights = inv_mae / inv_mae.sum()

        # Iteratively enforce min_weight floor: lock clamped models,
        # redistribute remaining weight budget proportionally.
        weights = raw_weights.copy()
        locked = np.zeros(n, dtype=bool)

        for _ in range(n):
            below = (~locked) & (weights < cfg.min_weight)
            if not np.any(below):
                break
            locked |= below
            weights[below] = cfg.min_weight
            budget = 1.0 - weights[locked].sum()
            free = ~locked
            if np.any(free):
                free_sum = raw_weights[free].sum()
                if free_sum > 0:
                    weights[free] = raw_weights[free] / free_sum * budget
                else:
                    weights[free] = budget / free.sum()

        # Final normalization (safety)
        weights = weights / weights.sum()
        return weights

    # ----- Properties -----------------------------------------------------

    @property
    def weights(self) -> dict[str, float]:
        """Current model weights as a dict."""
        return {name: float(self._weights[i]) for i, name in enumerate(self._model_names)}

    @property
    def model_errors(self) -> dict[str, float]:
        """Individual model MAE on validation window."""
        return dict(self._model_mae)

    @property
    def active_models(self) -> list[str]:
        """Names of models that were successfully fitted."""
        return list(self._model_names)

    # ----- Persistence ----------------------------------------------------

    def _get_state(self) -> dict[str, Any]:
        """Return a JSON-serializable dictionary of fitted state."""
        if not self._fitted:
            raise RuntimeError("Model has not been fitted yet")
        return {
            "model_type": "ensemble",
            "model_names": self._model_names,
            "weights": self._weights.tolist(),
            "model_mae": self._model_mae,
            "config": {
                "use_arima": self._config.use_arima,
                "use_prophet": self._config.use_prophet,
                "use_neural": self._config.use_neural,
                "validation_window": self._config.validation_window,
                "min_weight": self._config.min_weight,
                "reweight_interval": self._config.reweight_interval,
            },
        }

    def __repr__(self) -> str:
        if self._fitted:
            parts = ", ".join(f"{n}={w:.2f}" for n, w in zip(self._model_names, self._weights))
            return f"EnsembleForecaster({parts})"
        return "EnsembleForecaster(unfitted)"
