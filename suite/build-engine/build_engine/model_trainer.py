"""
Manages oracle model lifecycle: fitting, caching, prediction, and retraining.

Wraps build_oracle models (ARIMA, Prophet, SimpleForecaster) behind a
uniform train/predict interface so the rest of the engine never touches
model internals directly.
"""

from __future__ import annotations

import logging

import numpy as np

from build_engine.config import EngineConfig

logger = logging.getLogger(__name__)


class ModelTrainer:
    """Manages oracle model lifecycle: fitting, caching, retraining.

    Parameters
    ----------
    config:
        An :class:`EngineConfig` specifying which models to use and their
        hyperparameters.
    """

    def __init__(self, config: EngineConfig | None = None) -> None:
        self.config = config or EngineConfig()
        self.fitted_models: dict = {}  # name -> fitted model object
        self._last_series: np.ndarray | None = None

    # ------------------------------------------------------------------
    # Training
    # ------------------------------------------------------------------

    def train_all(self, series: np.ndarray) -> None:
        """Fit all configured models on *series*.

        Each model type is imported lazily from ``build_oracle`` and
        trained according to its API.  Failures are logged and skipped
        so that one broken model does not block the rest.
        """
        self._last_series = series.copy()

        for model_name in self.config.models:
            try:
                if model_name == "arima":
                    from build_oracle.arima import ARIMA

                    model = ARIMA(p=2, d=1, q=1)
                    model.fit(series)
                    self.fitted_models["arima"] = model

                elif model_name == "prophet":
                    from build_oracle.prophet import Prophet

                    model = Prophet(fourier_order=5)
                    t = np.arange(len(series), dtype=float)
                    model.fit(t, series)
                    self.fitted_models["prophet"] = model

                elif model_name == "neural":
                    from build_oracle.neural import SimpleForecaster

                    model = SimpleForecaster(
                        lookback=20,
                        horizon=self.config.forecast_horizon,
                    )
                    model.train(series, epochs=50, lr=0.001)
                    self.fitted_models["neural"] = model

                else:
                    logger.warning("Unknown model name: %s", model_name)

            except (ValueError, RuntimeError) as exc:
                logger.warning("Failed to train %s: %s", model_name, exc)

    # ------------------------------------------------------------------
    # Prediction
    # ------------------------------------------------------------------

    def predict_all(
        self,
        series: np.ndarray,
        horizon: int,
    ) -> dict[str, np.ndarray]:
        """Get predictions from all fitted models.

        Parameters
        ----------
        series:
            The full price series available at prediction time.
        horizon:
            Number of future steps to forecast.

        Returns
        -------
        dict mapping model name to an array of predicted values.
        """
        predictions: dict[str, np.ndarray] = {}

        for name, model in self.fitted_models.items():
            try:
                if name == "arima":
                    predictions["arima"] = model.predict(horizon)

                elif name == "prophet":
                    t_future = np.arange(
                        len(series),
                        len(series) + horizon,
                        dtype=float,
                    )
                    result = model.predict(t_future)
                    predictions["prophet"] = result["yhat"]

                elif name == "neural":
                    predictions["neural"] = model.predict(series[-30:])

            except (ValueError, RuntimeError) as exc:
                logger.debug("Predict failed for %s: %s", name, exc)

        return predictions

    # ------------------------------------------------------------------
    # Introspection
    # ------------------------------------------------------------------

    def get_model_info(self) -> dict[str, str]:
        """Return ``{model_name: class_name}`` for all fitted models."""
        return {name: type(model).__name__ for name, model in self.fitted_models.items()}

    def has_models(self) -> bool:
        """True if at least one model has been fitted."""
        return len(self.fitted_models) > 0

    def __repr__(self) -> str:
        fitted = list(self.fitted_models.keys()) or ["(none)"]
        return f"ModelTrainer(fitted={fitted})"
