"""
Tests for build_engine.prediction_strategy.PredictionStrategy.

Covers strategy creation, signal generation with various data patterns
(insufficient, trending up, trending down, flat), weight management,
signal attribute validation, and retrain triggering.
"""

from __future__ import annotations

import numpy as np

from build_engine.config import EngineConfig
from build_engine.model_trainer import ModelTrainer
from build_engine.prediction_strategy import PredictionStrategy

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _make_candles(prices: list[float], symbol: str = "TEST") -> list:
    """Build Candle objects from a list of close prices.

    ``build_finance`` is imported lazily here (rather than at module
    scope) so that collecting this test module does not hard-fail when
    the optional sibling package ``build-finance`` is not installed --
    matching the pattern used in ``tests/test_engine.py``.
    """
    from build_finance.data import Candle

    candles = []
    base_ts = 1_700_000_000.0
    for i, price in enumerate(prices):
        candles.append(
            Candle(
                timestamp=base_ts + i * 86_400,
                open=price * 0.999,
                high=price * 1.01,
                low=price * 0.99,
                close=price,
                volume=1_000_000.0,
                symbol=symbol,
            )
        )
    return candles


def _trending_up(n: int = 100, start: float = 100.0) -> list[float]:
    """Strong upward trend."""
    rng = np.random.default_rng(42)
    prices = [start]
    for _ in range(n - 1):
        prices.append(prices[-1] * (1 + 0.01 + rng.normal(0, 0.002)))
    return prices


def _trending_down(n: int = 100, start: float = 200.0) -> list[float]:
    """Strong downward trend."""
    rng = np.random.default_rng(42)
    prices = [start]
    for _ in range(n - 1):
        prices.append(prices[-1] * (1 - 0.01 + rng.normal(0, 0.002)))
    return prices


def _flat_series(n: int = 100, center: float = 100.0) -> list[float]:
    """Nearly flat series oscillating tightly around center."""
    rng = np.random.default_rng(42)
    return [center + rng.normal(0, 0.01) for _ in range(n)]


def _low_confidence_strategy() -> PredictionStrategy:
    """Strategy configured to accept any signal."""
    cfg = EngineConfig(
        models=["arima"],
        min_confidence=0.0,
        direction_threshold=0.0,
    )
    return PredictionStrategy(cfg)


# ---------------------------------------------------------------------------
# Construction
# ---------------------------------------------------------------------------


class TestPredictionStrategyCreation:
    def test_default_creation(self):
        strategy = PredictionStrategy()
        assert strategy.config.models == ["arima", "prophet"]
        assert strategy.model_weights == {}
        assert strategy._last_train_time == 0.0

    def test_custom_config(self):
        cfg = EngineConfig(models=["neural"], min_confidence=0.5)
        strategy = PredictionStrategy(cfg)
        assert strategy.config.models == ["neural"]
        assert strategy.config.min_confidence == 0.5

    def test_custom_trainer_injected(self):
        cfg = EngineConfig(models=["arima"])
        trainer = ModelTrainer(cfg)
        strategy = PredictionStrategy(cfg, model_trainer=trainer)
        assert strategy.trainer is trainer

    def test_repr_before_signals(self):
        strategy = PredictionStrategy()
        assert "(none)" in repr(strategy)


# ---------------------------------------------------------------------------
# Insufficient data
# ---------------------------------------------------------------------------


class TestInsufficientData:
    def test_fewer_than_50_candles_returns_empty(self):
        strategy = PredictionStrategy()
        candles = _make_candles([100.0] * 10)
        assert strategy.generate_signals(candles) == []

    def test_exactly_49_returns_empty(self):
        strategy = PredictionStrategy()
        candles = _make_candles([100.0] * 49)
        assert strategy.generate_signals(candles) == []

    def test_exactly_50_candles_accepted(self):
        strategy = _low_confidence_strategy()
        candles = _make_candles(_trending_up(50), symbol="AAPL")
        # Should not return the empty-due-to-length list
        signals = strategy.generate_signals(candles)
        assert isinstance(signals, list)


# ---------------------------------------------------------------------------
# Signal generation with trending data
# ---------------------------------------------------------------------------


class TestTrendingSignals:
    def test_trending_up_produces_buy(self):
        strategy = _low_confidence_strategy()
        candles = _make_candles(_trending_up(80), symbol="AAPL")
        signals = strategy.generate_signals(candles)
        assert len(signals) >= 1
        assert signals[0].side == "buy"

    def test_trending_down_produces_sell(self):
        cfg = EngineConfig(
            models=["arima"],
            min_confidence=0.0,
            direction_threshold=0.0,
        )
        strategy = PredictionStrategy(cfg)
        candles = _make_candles(_trending_down(80), symbol="AAPL")
        signals = strategy.generate_signals(candles)
        assert len(signals) >= 1
        assert signals[0].side == "sell"

    def test_flat_data_high_threshold_no_signal(self):
        cfg = EngineConfig(
            models=["arima"],
            min_confidence=0.9,
            direction_threshold=0.5,
        )
        strategy = PredictionStrategy(cfg)
        candles = _make_candles(_flat_series(80), symbol="AAPL")
        signals = strategy.generate_signals(candles)
        assert signals == []


# ---------------------------------------------------------------------------
# Signal attributes
# ---------------------------------------------------------------------------


class TestSignalAttributes:
    def test_signal_is_signal_instance(self):
        from build_finance.data import Signal

        strategy = _low_confidence_strategy()
        candles = _make_candles(_trending_up(80), symbol="AAPL")
        signals = strategy.generate_signals(candles)
        assert len(signals) >= 1
        assert isinstance(signals[0], Signal)

    def test_signal_symbol_matches_candle(self):
        strategy = _low_confidence_strategy()
        candles = _make_candles(_trending_up(80), symbol="MSFT")
        signals = strategy.generate_signals(candles)
        assert signals[0].symbol == "MSFT"

    def test_signal_side_is_buy_or_sell(self):
        strategy = _low_confidence_strategy()
        candles = _make_candles(_trending_up(80), symbol="AAPL")
        signals = strategy.generate_signals(candles)
        assert signals[0].side in ("buy", "sell")

    def test_signal_strength_bounded(self):
        strategy = _low_confidence_strategy()
        candles = _make_candles(_trending_up(80), symbol="AAPL")
        signals = strategy.generate_signals(candles)
        assert 0.0 <= signals[0].strength <= 1.0

    def test_signal_has_risk_levels(self):
        strategy = _low_confidence_strategy()
        candles = _make_candles(_trending_up(80), symbol="AAPL")
        signals = strategy.generate_signals(candles)
        sig = signals[0]
        assert sig.stop_loss is not None
        assert sig.take_profit is not None
        assert sig.target_price is not None

    def test_buy_signal_stop_loss_below_price(self):
        strategy = _low_confidence_strategy()
        candles = _make_candles(_trending_up(80), symbol="AAPL")
        signals = strategy.generate_signals(candles)
        sig = signals[0]
        if sig.side == "buy":
            current = candles[-1].close
            assert sig.stop_loss < current

    def test_sell_signal_stop_loss_above_price(self):
        cfg = EngineConfig(
            models=["arima"],
            min_confidence=0.0,
            direction_threshold=0.0,
        )
        strategy = PredictionStrategy(cfg)
        candles = _make_candles(_trending_down(80), symbol="AAPL")
        signals = strategy.generate_signals(candles)
        sig = signals[0]
        if sig.side == "sell":
            current = candles[-1].close
            assert sig.stop_loss > current


# ---------------------------------------------------------------------------
# Weight management
# ---------------------------------------------------------------------------


class TestWeightManagement:
    def test_set_model_weights(self):
        strategy = PredictionStrategy()
        strategy.set_model_weights({"arima": 0.8, "prophet": 0.5})
        assert strategy.model_weights["arima"] == 0.8
        assert strategy.model_weights["prophet"] == 0.5

    def test_weights_default_to_one(self):
        strategy = _low_confidence_strategy()
        candles = _make_candles(_trending_up(80), symbol="AAPL")
        # Before setting weights, default weight of 1.0 is used internally
        signals = strategy.generate_signals(candles)
        assert isinstance(signals, list)
        # model_weights dict stays empty until explicitly set
        assert strategy.model_weights == {}


# ---------------------------------------------------------------------------
# Retrain behavior
# ---------------------------------------------------------------------------


class TestRetrainBehavior:
    def test_first_call_triggers_training(self):
        strategy = _low_confidence_strategy()
        assert strategy._last_train_time == 0.0
        candles = _make_candles(_trending_up(80), symbol="AAPL")
        strategy.generate_signals(candles)
        assert strategy._last_train_time > 0.0
        assert strategy.trainer.has_models() is True

    def test_second_call_does_not_retrain_immediately(self):
        strategy = _low_confidence_strategy()
        candles = _make_candles(_trending_up(80), symbol="AAPL")
        strategy.generate_signals(candles)
        first_train_time = strategy._last_train_time
        strategy.generate_signals(candles)
        # Retrain interval is 24h so the second call should not retrain
        assert strategy._last_train_time == first_train_time
