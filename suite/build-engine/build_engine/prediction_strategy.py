"""
Trading strategy powered by oracle forecasting models.

This is the KEY integration class.  It implements the ``generate_signals``
protocol expected by ``build_finance.autotrader.AutoTrader`` and
``build_finance.backtest.Backtester``, but internally delegates to
build-oracle models (ARIMA, Prophet, SimpleForecaster) via
:class:`ModelTrainer`.

Pipeline per call to ``generate_signals``:
    1. Extract close prices from candles.
    2. Train / use cached models (retrain if stale).
    3. Each model predicts ``horizon`` steps ahead.
    4. Ensemble predictions with dynamic weights.
    5. Convert predicted direction to a buy / sell Signal.
"""

from __future__ import annotations

import time
from typing import TYPE_CHECKING

import numpy as np

from build_engine.config import EngineConfig
from build_engine.model_trainer import ModelTrainer

if TYPE_CHECKING:
    pass


class PredictionStrategy:
    """Trading strategy powered by oracle forecasting models.

    Implements the same interface as ``build_finance.strategies``::

        generate_signals(candles: list[Candle]) -> list[Signal]

    Internally:
        1. Extracts close prices from candles.
        2. Fits / uses cached ARIMA and Prophet models.
        3. Each model predicts ``horizon`` steps ahead.
        4. Converts price forecast to directional signal (buy / sell).
        5. Ensembles across models using dynamic weights.
        6. Returns Signal compatible with AutoTrader.

    Parameters
    ----------
    config:
        An :class:`EngineConfig` controlling symbols, models, risk, etc.
    model_trainer:
        Optional pre-built :class:`ModelTrainer`.  One is created
        automatically when not provided.
    """

    def __init__(
        self,
        config: EngineConfig | None = None,
        model_trainer: ModelTrainer | None = None,
    ) -> None:
        self.config = config or EngineConfig()
        self.trainer = model_trainer or ModelTrainer(self.config)
        self.model_weights: dict[str, float] = {}  # model_name -> weight
        self._last_train_time: float = 0.0

    # ------------------------------------------------------------------
    # Core interface
    # ------------------------------------------------------------------

    def generate_signals(self, candles) -> list:
        """Generate trading signals from oracle predictions.

        Steps:
            1. Extract close prices.
            2. Train models if needed (first call or stale).
            3. Get predictions from each model.
            4. Ensemble predictions with weights.
            5. Convert to buy / sell signal based on predicted direction.

        Parameters
        ----------
        candles:
            A list of ``build_finance.data.Candle`` objects (must have
            ``.close``, ``.symbol``, and ``.timestamp`` attributes).

        Returns
        -------
        list[Signal]
            Zero or one Signal per call.
        """
        from build_finance.data import Signal

        if len(candles) < 50:
            return []

        closes = np.array([c.close for c in candles])
        current_price = closes[-1]
        symbol = getattr(candles[-1], "symbol", "UNKNOWN")
        timestamp = getattr(candles[-1], "timestamp", 0.0)

        # -- Train if needed --------------------------------------------------
        self._maybe_retrain(closes)

        # -- Get predictions from each model ----------------------------------
        predictions = self.trainer.predict_all(
            closes,
            self.config.forecast_horizon,
        )
        if not predictions:
            return []

        # -- Ensemble: weighted average of predicted direction ----------------
        total_weight = 0.0
        weighted_forecast = 0.0

        for model_name, forecast in predictions.items():
            weight = self.model_weights.get(model_name, 1.0)
            # Focus on near-term predictions (first 3 steps)
            near = forecast[: min(3, len(forecast))]
            avg_predicted = float(np.mean(near))
            direction = (avg_predicted - current_price) / current_price
            weighted_forecast += direction * weight
            total_weight += weight

        if total_weight == 0:
            return []

        avg_direction = weighted_forecast / total_weight
        # Normalize: 2% predicted move = max strength
        strength = min(abs(avg_direction) / 0.02, 1.0)

        threshold = self.config.direction_threshold

        # -- Convert to Signal ------------------------------------------------
        if avg_direction > threshold and strength >= self.config.min_confidence:
            return [
                Signal(
                    symbol=symbol,
                    side="buy",
                    strength=strength,
                    timestamp=timestamp,
                    target_price=current_price * (1 + avg_direction),
                    stop_loss=current_price * 0.97,
                    take_profit=current_price * (1 + avg_direction * 2),
                )
            ]

        if avg_direction < -threshold and strength >= self.config.min_confidence:
            return [
                Signal(
                    symbol=symbol,
                    side="sell",
                    strength=strength,
                    timestamp=timestamp,
                    target_price=current_price * (1 + avg_direction),
                    stop_loss=current_price * 1.03,
                    take_profit=current_price * (1 + avg_direction * 2),
                )
            ]

        return []

    # ------------------------------------------------------------------
    # Weight management
    # ------------------------------------------------------------------

    def set_model_weights(self, weights: dict[str, float]) -> None:
        """Update ensemble weights from the performance tracker."""
        self.model_weights = weights

    # ------------------------------------------------------------------
    # Internals
    # ------------------------------------------------------------------

    def _maybe_retrain(self, series: np.ndarray) -> None:
        """Retrain all models if enough time has elapsed since last fit."""
        elapsed = time.time() - self._last_train_time
        interval = self.config.retrain_interval_hours * 3600
        if elapsed > interval or self._last_train_time == 0:
            self.trainer.train_all(series)
            self._last_train_time = time.time()

    def __repr__(self) -> str:
        models = list(self.trainer.fitted_models.keys()) or ["(none)"]
        return f"PredictionStrategy(models={models})"
