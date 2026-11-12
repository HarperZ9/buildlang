"""
Streaming Forecast Engine

Provides incremental forecast updates without full model refit.
Accepts new data points one at a time, maintains a sliding window,
and produces updated predictions with each new observation.

Strategies:
- ARIMA: Update state vector with new observation (Kalman filter style)
- Prophet: Refit only trend component, keep seasonality fixed
- Neural: Online gradient update on new data point
- Ensemble: Reweight based on streaming accuracy
"""

from __future__ import annotations

from dataclasses import dataclass, field
from datetime import datetime
from typing import Any

import numpy as np

from build_oracle.arima import ARIMA
from build_oracle.neural import SimpleForecaster
from build_oracle.prophet import Prophet


@dataclass
class StreamConfig:
    """Configuration for streaming forecaster."""

    window_size: int = 200  # sliding window length
    refit_interval: int = 50  # full refit every N points
    min_history: int = 30  # minimum data before first prediction
    models: list[str] = field(default_factory=lambda: ["arima", "prophet", "neural"])
    forecast_horizon: int = 5  # how many steps ahead to predict
    decay_factor: float = 0.95  # exponential decay for accuracy weighting
    # ARIMA hyperparameters
    arima_p: int = 2
    arima_d: int = 1
    arima_q: int = 1
    # Neural hyperparameters
    neural_lookback: int = 20
    neural_hidden: int = 32
    neural_epochs: int = 50
    neural_lr: float = 0.001


@dataclass
class StreamUpdate:
    """Result from processing a new data point."""

    timestamp: datetime
    observed: float
    predicted: float  # what we predicted for this point
    error: float  # prediction error
    next_predictions: np.ndarray  # updated forecast for next N steps
    model_weights: dict  # current model weights
    confidence: float  # prediction confidence (0-1)


class StreamForecaster:
    """
    Streaming forecast engine with incremental updates.

    Usage:
        sf = StreamForecaster(StreamConfig())
        for value in data_stream:
            update = sf.observe(value)
            if update:
                print(f"Next prediction: {update.next_predictions[0]}")
    """

    def __init__(self, config: StreamConfig | None = None):
        self.config = config or StreamConfig()
        self._history: list[float] = []
        self._predictions: dict[str, np.ndarray] = {}
        self._errors: dict[str, list[float]] = {}
        self._weights: dict[str, float] = {}
        self._fitted = False
        self._update_count = 0
        self._models: dict[str, Any] = {}
        self._last_prediction: float | None = None
        self._refit_count = 0

    def observe(
        self,
        value: float,
        timestamp: datetime | None = None,
    ) -> StreamUpdate | None:
        """
        Process a new observation. Returns StreamUpdate if enough
        history, None if still warming up.
        """
        ts = timestamp or datetime.now()
        self._history.append(float(value))

        # Trim to sliding window
        if len(self._history) > self.config.window_size:
            self._history = self._history[-self.config.window_size :]

        # Still warming up
        if len(self._history) < self.config.min_history:
            return None

        # Capture what we predicted for this point (before updating)
        predicted_for_this = (
            self._last_prediction if self._last_prediction is not None else value  # no error on the very first
        )
        error = abs(value - predicted_for_this)

        # First time we have enough data -- do initial fit
        if not self._fitted:
            self._initial_fit()
            self._fitted = True
        else:
            self._update_count += 1

            # Periodic full refit
            if self._update_count % self.config.refit_interval == 0:
                self._initial_fit()
                self._refit_count += 1
            else:
                self._incremental_update(value)

        # Update per-model error tracking
        if self._last_prediction is not None:
            for name in self._models:
                if name in self._predictions and len(self._predictions[name]) > 0:
                    model_pred = float(self._predictions[name][0])
                    model_err = abs(value - model_pred)
                    if name not in self._errors:
                        self._errors[name] = []
                    self._errors[name].append(model_err)

        # Recalculate weights
        self._update_weights()

        # Produce ensemble prediction
        horizon = self.config.forecast_horizon
        next_preds = self._ensemble_predict(horizon)

        # Store the 1-step-ahead for next observation comparison
        self._last_prediction = float(next_preds[0]) if len(next_preds) > 0 else value

        # Store per-model predictions for next error tracking
        for name in self._models:
            try:
                self._predictions[name] = self._predict_single(name, horizon)
            except (ValueError, KeyError):
                self._predictions[name] = next_preds.copy()

        # Compute confidence from recent error trend
        confidence = self._compute_confidence()

        return StreamUpdate(
            timestamp=ts,
            observed=value,
            predicted=predicted_for_this,
            error=error,
            next_predictions=next_preds,
            model_weights=dict(self._weights),
            confidence=confidence,
        )

    def _initial_fit(self) -> None:
        """Full model fit on current window."""
        data = np.array(self._history, dtype=np.float64)
        cfg = self.config

        for name in cfg.models:
            model: Any
            try:
                if name == "arima":
                    model = ARIMA(p=cfg.arima_p, d=cfg.arima_d, q=cfg.arima_q)
                    model.fit(data)
                    self._models["arima"] = model
                elif name == "prophet":
                    t = np.arange(len(data), dtype=np.float64)
                    model = Prophet(
                        n_changepoints=min(10, len(data) // 5),
                        fourier_order=min(5, len(data) // 10),
                    )
                    model.fit(t, data)
                    self._models["prophet"] = model
                elif name == "neural":
                    lookback = min(cfg.neural_lookback, len(data) // 3)
                    if lookback < 5:
                        continue
                    horizon = cfg.forecast_horizon
                    model = SimpleForecaster(
                        lookback=lookback,
                        horizon=horizon,
                        hidden=cfg.neural_hidden,
                    )
                    model.fit(data, epochs=cfg.neural_epochs, lr=cfg.neural_lr)
                    self._models["neural"] = model
            except (ValueError, np.linalg.LinAlgError, RuntimeError):
                # Model failed to fit, skip it
                pass

        # Initialize equal weights for any new models
        if self._models:
            for name in self._models:
                if name not in self._weights:
                    self._weights[name] = 1.0 / len(self._models)

    def _incremental_update(self, value: float) -> None:
        """Update each model incrementally with new observation."""
        data = np.array(self._history, dtype=np.float64)
        cfg = self.config

        for name in list(self._models.keys()):
            try:
                if name == "arima":
                    # Re-estimate AR coefficients on the latest window
                    # This is cheaper than full auto_arima but refreshes
                    # the state with the new observation
                    model = self._models[name]
                    model._series = data.copy()
                    from build_oracle.arima import _autocov, _difference, _levinson_durbin

                    z = _difference(data, model.d)
                    model._diff_series = z.copy()
                    model.intercept = float(np.mean(z))
                    z_centered = z - model.intercept
                    if model.p > 0:
                        acov = _autocov(z_centered, model.p)
                        model.phi = _levinson_durbin(acov, model.p)
                    # Recompute residuals
                    n = len(z_centered)
                    residuals = np.zeros(n)
                    for t in range(n):
                        ar_part = 0.0
                        for j in range(model.p):
                            if t - j - 1 >= 0:
                                ar_part += model.phi[j] * z_centered[t - j - 1]
                        residuals[t] = z_centered[t] - ar_part
                    model._residuals = residuals
                    from build_oracle.arima import _estimate_ma

                    if model.q > 0:
                        model.theta = _estimate_ma(residuals, model.q)

                elif name == "prophet":
                    # Refit trend only, keeping seasonality coefficients
                    model = self._models[name]
                    # `t` is an int index in the ARIMA branch above; here it is a float grid.
                    # The branches are mutually exclusive, which mypy's scope narrowing can't see.
                    t = np.arange(len(data), dtype=np.float64)  # type: ignore[assignment]
                    saved_seasonal = model._seasonal_coeffs.copy() if model._seasonal_coeffs is not None else None
                    # Quick trend refit via least-squares on residuals
                    n = len(data)
                    n_cp = len(model._changepoints)
                    A_trend = np.ones((n, 2 + n_cp))
                    A_trend[:, 1] = t
                    for j, cp in enumerate(model._changepoints):
                        A_trend[:, 2 + j] = np.maximum(0.0, t - cp)
                    trend_coeffs, _, _, _ = np.linalg.lstsq(
                        A_trend,
                        data,
                        rcond=None,
                    )
                    model._m = float(trend_coeffs[0])
                    model._k = float(trend_coeffs[1])
                    if n_cp > 0:
                        model._deltas = trend_coeffs[2:].copy()
                    # Keep seasonality from the full fit
                    if saved_seasonal is not None:
                        model._seasonal_coeffs = saved_seasonal

                elif name == "neural":
                    # One online gradient step on the most recent window
                    model = self._models[name]
                    if len(data) >= model.lookback + model.horizon:
                        window = data[-(model.lookback + model.horizon) :]
                        x_norm = (window[: model.lookback] - model._train_mean) / model._train_std
                        y_norm = (window[model.lookback :] - model._train_mean) / model._train_std
                        x_batch = x_norm.reshape(1, -1)
                        y_batch = y_norm.reshape(1, -1)

                        # Forward
                        output = model._forward(x_batch)
                        diff = output - y_batch
                        d_loss = 2.0 * diff / diff.size

                        # Backward
                        d_z = model.layer2._backward(d_loss)
                        d_z = model.relu._backward(d_z)
                        model.layer1._backward(d_z)

                        # SGD step
                        lr = cfg.neural_lr
                        model.layer2.W -= lr * model.layer2._dW
                        model.layer2.b -= lr * model.layer2._db
                        model.layer1.W -= lr * model.layer1._dW
                        model.layer1.b -= lr * model.layer1._db

            except (ValueError, np.linalg.LinAlgError, RuntimeError):
                pass  # model update failed, keep previous state

    def _update_weights(self) -> None:
        """Recalculate model weights based on recent accuracy."""
        if not self._models:
            return

        decay = self.config.decay_factor
        model_scores: dict[str, float] = {}

        for name in self._models:
            errors = self._errors.get(name, [])
            if not errors:
                # No error history yet, give equal weight
                model_scores[name] = 1.0
                continue

            # Exponentially weighted MAE (recent errors weighted more)
            n = len(errors)
            weights = np.array(
                [decay ** (n - 1 - i) for i in range(n)],
                dtype=np.float64,
            )
            weighted_mae = float(np.average(np.abs(errors), weights=weights))
            # Inverse MAE as score (lower error -> higher score)
            model_scores[name] = 1.0 / (weighted_mae + 1e-10)

        # Normalize to sum to 1
        total = sum(model_scores.values())
        if total > 0:
            self._weights = {name: score / total for name, score in model_scores.items()}
        else:
            n = len(self._models)
            self._weights = {name: 1.0 / n for name in self._models}

    def _ensemble_predict(self, steps: int) -> np.ndarray:
        """Weighted ensemble prediction."""
        if not self._models:
            # Fallback: repeat last value
            return np.full(steps, self._history[-1])

        predictions: list[np.ndarray] = []
        weights: list[float] = []

        for name in self._models:
            try:
                pred = self._predict_single(name, steps)
                pred = np.asarray(pred, dtype=np.float64)
                if len(pred) < steps:
                    pred = np.pad(pred, (0, steps - len(pred)), mode="edge")
                predictions.append(pred[:steps])
                weights.append(self._weights.get(name, 0.0))
            except (ValueError, KeyError):
                continue

        if not predictions:
            return np.full(steps, self._history[-1])

        w = np.array(weights, dtype=np.float64)
        w_sum = w.sum()
        if w_sum > 0:
            w = w / w_sum
        else:
            w = np.ones(len(w)) / len(w)

        stacked = np.column_stack(predictions)
        return stacked @ w

    def _predict_single(self, name: str, steps: int) -> np.ndarray:
        """Dispatch prediction to a specific sub-model."""
        model = self._models[name]
        data = np.array(self._history, dtype=np.float64)

        if name == "arima":
            return model.predict(steps)
        elif name == "prophet":
            n = len(data)
            t_future = np.arange(n, n + steps, dtype=np.float64)
            result = model.predict(t_future)
            return np.asarray(result["yhat"], dtype=np.float64)
        elif name == "neural":
            return model.predict(data)
        else:
            raise ValueError(f"Unknown model: {name}")

    @property
    def is_ready(self) -> bool:
        """Whether enough history has accumulated for predictions."""
        return len(self._history) >= self.config.min_history

    @property
    def history_length(self) -> int:
        return len(self._history)

    @property
    def refit_count(self) -> int:
        """Number of full refits that have occurred."""
        return self._refit_count

    @property
    def update_count(self) -> int:
        """Number of incremental updates since initial fit."""
        return self._update_count

    def reset(self) -> None:
        """Clear all state."""
        self._history = []
        self._predictions = {}
        self._errors = {}
        self._weights = {}
        self._fitted = False
        self._update_count = 0
        self._models = {}
        self._last_prediction = None
        self._refit_count = 0

    def _compute_confidence(self) -> float:
        """Compute prediction confidence from recent error trend.

        Returns a value between 0 and 1. Lower recent errors relative
        to the data range yield higher confidence.
        """
        if not self._errors:
            return 0.5

        # Gather all recent errors across models
        all_recent: list[float] = []
        for errs in self._errors.values():
            recent = errs[-20:] if len(errs) > 20 else errs
            all_recent.extend(recent)

        if not all_recent:
            return 0.5

        avg_error = float(np.mean(all_recent))

        # Scale against the data range
        data = np.array(self._history, dtype=np.float64)
        data_range = float(np.ptp(data))
        if data_range == 0:
            return 0.5

        # Normalized error ratio: 0 = perfect, 1+ = bad
        ratio = avg_error / data_range
        # child safety assessment to confidence: exp(-ratio) gives (0, 1]
        confidence = float(np.exp(-2.0 * ratio))
        return max(0.0, min(1.0, confidence))

    def __repr__(self) -> str:
        if self._fitted:
            models = ", ".join(self._models.keys())
            return f"StreamForecaster(history={self.history_length}, models=[{models}], updates={self._update_count})"
        return f"StreamForecaster(warming_up, history={self.history_length}/{self.config.min_history})"
