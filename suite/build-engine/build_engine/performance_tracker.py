"""
Tracks prediction accuracy and dynamically adjusts model weights.

Every prediction is recorded with its timestamp, model, and predicted
direction / magnitude.  When actual outcomes become available the tracker
evaluates correctness and recalculates per-model weights so that more
accurate models get heavier influence in the ensemble.
"""

from __future__ import annotations

import time
from dataclasses import dataclass

from build_engine.config import EngineConfig


@dataclass
class PredictionRecord:
    """One recorded prediction from a single model."""

    timestamp: float
    model_name: str
    predicted_direction: str  # "up", "down", "flat"
    predicted_magnitude: float
    symbol: str = ""
    predicted_price: float = 0.0
    current_price: float = 0.0
    actual_direction: str = ""
    actual_magnitude: float = 0.0
    correct: bool = False
    evaluated: bool = False


class PerformanceTracker:
    """Tracks prediction accuracy and adjusts model weights dynamically.

    The tracker stores every prediction and, when new price data arrives,
    evaluates past predictions to determine directional accuracy.  Weights
    are computed as ``accuracy ** 2`` so that consistently accurate models
    receive exponentially more influence.

    Parameters
    ----------
    config:
        An :class:`EngineConfig` whose ``accuracy_window`` and
        ``direction_threshold`` are used.
    """

    def __init__(self, config: EngineConfig | None = None) -> None:
        self.config = config or EngineConfig()
        self.records: list[PredictionRecord] = []

    # ------------------------------------------------------------------
    # Recording
    # ------------------------------------------------------------------

    def record_prediction(
        self,
        model_name: str,
        predicted_price: float,
        current_price: float,
        timestamp: float | None = None,
        symbol: str = "",
    ) -> None:
        """Record a prediction for later evaluation.

        Parameters
        ----------
        model_name:
            Which oracle model produced the forecast.
        predicted_price:
            The model's forecast price.
        current_price:
            Price at the time the prediction was made.
        timestamp:
            When the prediction was made (defaults to ``time.time()``).
        symbol:
            Ticker symbol the prediction is for.
        """
        if timestamp is None:
            timestamp = time.time()

        threshold = self.config.direction_threshold
        delta = (predicted_price - current_price) / current_price if current_price else 0.0

        if delta > threshold:
            direction = "up"
        elif delta < -threshold:
            direction = "down"
        else:
            direction = "flat"

        self.records.append(
            PredictionRecord(
                timestamp=timestamp,
                model_name=model_name,
                predicted_direction=direction,
                predicted_magnitude=abs(delta),
                symbol=symbol,
                predicted_price=predicted_price,
                current_price=current_price,
            )
        )

    # ------------------------------------------------------------------
    # Evaluation
    # ------------------------------------------------------------------

    def evaluate_past(self, current_prices: dict[str, float]) -> int:
        """Check past predictions against actual outcomes.

        For every unevaluated record whose symbol appears in
        *current_prices*, determines whether the predicted direction
        was correct.

        Parameters
        ----------
        current_prices:
            ``{symbol: latest_price}`` mapping.

        Returns
        -------
        int
            Number of records evaluated in this call.
        """
        evaluated_count = 0
        threshold = self.config.direction_threshold

        for record in self.records:
            if record.evaluated:
                continue
            if record.symbol not in current_prices and record.current_price == 0:
                continue

            actual_price = current_prices.get(record.symbol, 0.0)
            if actual_price == 0:
                continue

            actual_delta = (actual_price - record.current_price) / record.current_price if record.current_price else 0.0

            if actual_delta > threshold:
                record.actual_direction = "up"
            elif actual_delta < -threshold:
                record.actual_direction = "down"
            else:
                record.actual_direction = "flat"

            record.actual_magnitude = abs(actual_delta)
            record.correct = record.predicted_direction == record.actual_direction
            record.evaluated = True
            evaluated_count += 1

        return evaluated_count

    # ------------------------------------------------------------------
    # Weight computation
    # ------------------------------------------------------------------

    def get_model_weights(self) -> dict[str, float]:
        """Dynamic weights based on recent accuracy.

        Uses the most recent ``accuracy_window`` evaluated predictions
        per model.  Weight = ``accuracy ** 2`` so that more accurate
        models are rewarded exponentially.

        Returns
        -------
        dict mapping model name to weight (>= 0.01).
        """
        window = self.config.accuracy_window
        model_records: dict[str, list[PredictionRecord]] = {}

        for record in reversed(self.records):
            if not record.evaluated:
                continue
            bucket = model_records.setdefault(record.model_name, [])
            if len(bucket) < window:
                bucket.append(record)

        weights: dict[str, float] = {}
        for name, recs in model_records.items():
            if not recs:
                weights[name] = 1.0
                continue
            accuracy = sum(1 for r in recs if r.correct) / len(recs)
            # Squared accuracy so better models dominate
            weights[name] = max(accuracy**2, 0.01)

        return weights

    # ------------------------------------------------------------------
    # Statistics
    # ------------------------------------------------------------------

    def get_stats(self) -> dict[str, dict]:
        """Return per-model accuracy statistics.

        Returns
        -------
        dict of ``{model_name: {"total", "correct", "accuracy"}}``.
        """
        model_stats: dict[str, dict] = {}

        for record in self.records:
            if not record.evaluated:
                continue
            stats = model_stats.setdefault(
                record.model_name,
                {"total": 0, "correct": 0, "accuracy": 0.0},
            )
            stats["total"] += 1
            if record.correct:
                stats["correct"] += 1

        for stats in model_stats.values():
            if stats["total"] > 0:
                stats["accuracy"] = stats["correct"] / stats["total"]

        return model_stats

    def get_overall_accuracy(self) -> float:
        """Overall directional accuracy across all evaluated predictions."""
        evaluated = [r for r in self.records if r.evaluated]
        if not evaluated:
            return 0.0
        return sum(1 for r in evaluated if r.correct) / len(evaluated)

    def __repr__(self) -> str:
        total = len(self.records)
        evaluated = sum(1 for r in self.records if r.evaluated)
        return f"PerformanceTracker(total={total}, evaluated={evaluated})"
