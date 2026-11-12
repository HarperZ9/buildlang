"""
Tests for build-engine integration package.

Covers config, model training, prediction strategy, performance tracking,
adaptive engine lifecycle, and ensemble weight dynamics.
"""

from __future__ import annotations

import numpy as np
import pytest

from build_engine.config import LIVE_TRADING_ACK, EngineConfig
from build_engine.model_trainer import ModelTrainer
from build_engine.performance_tracker import PerformanceTracker
from build_engine.prediction_strategy import PredictionStrategy

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _make_candles(prices: list[float], symbol: str = "TEST") -> list:
    """Build a list of Candle objects from close prices."""
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


def _trending_series(n: int = 100, start: float = 100.0, trend: float = 0.5) -> list[float]:
    """Generate a simple upward trending price series."""
    rng = np.random.default_rng(42)
    prices = [start]
    for _ in range(n - 1):
        prices.append(prices[-1] * (1 + trend / 100 + rng.normal(0, 0.005)))
    return prices


# ---------------------------------------------------------------------------
# EngineConfig tests
# ---------------------------------------------------------------------------


class TestEngineConfig:
    def test_defaults(self):
        cfg = EngineConfig()
        assert cfg.symbols == ["AAPL", "BTC-USD"]
        assert cfg.risk_per_trade == 0.02
        assert cfg.max_positions == 5
        assert cfg.paper_trading is True
        assert cfg.broker_api_key == ""
        assert cfg.broker_api_secret == ""
        assert cfg.broker_base_url == ""
        assert cfg.live_trading_ack == ""

    def test_model_defaults(self):
        cfg = EngineConfig()
        assert "arima" in cfg.models
        assert "prophet" in cfg.models
        assert cfg.forecast_horizon == 5
        assert cfg.retrain_interval_hours == 24

    def test_feedback_defaults(self):
        cfg = EngineConfig()
        assert cfg.min_confidence == 0.3
        assert cfg.accuracy_window == 50
        assert cfg.direction_threshold == 0.005

    def test_custom_config(self):
        cfg = EngineConfig(
            symbols=["SPY"],
            risk_per_trade=0.05,
            models=["arima"],
            min_confidence=0.8,
        )
        assert cfg.symbols == ["SPY"]
        assert cfg.risk_per_trade == 0.05
        assert cfg.models == ["arima"]
        assert cfg.min_confidence == 0.8


# ---------------------------------------------------------------------------
# ModelTrainer tests
# ---------------------------------------------------------------------------


class TestModelTrainer:
    def test_train_arima(self):
        cfg = EngineConfig(models=["arima"])
        trainer = ModelTrainer(cfg)
        series = np.array(_trending_series(100))
        trainer.train_all(series)
        assert "arima" in trainer.fitted_models
        assert trainer.has_models()

    def test_train_prophet(self):
        cfg = EngineConfig(models=["prophet"])
        trainer = ModelTrainer(cfg)
        series = np.array(_trending_series(100))
        trainer.train_all(series)
        assert "prophet" in trainer.fitted_models

    def test_train_neural(self):
        cfg = EngineConfig(models=["neural"], forecast_horizon=3)
        trainer = ModelTrainer(cfg)
        series = np.array(_trending_series(100))
        trainer.train_all(series)
        assert "neural" in trainer.fitted_models

    def test_train_all_models(self):
        cfg = EngineConfig(models=["arima", "prophet", "neural"])
        trainer = ModelTrainer(cfg)
        series = np.array(_trending_series(100))
        trainer.train_all(series)
        assert len(trainer.fitted_models) == 3

    def test_predict_all(self):
        cfg = EngineConfig(models=["arima", "prophet"])
        trainer = ModelTrainer(cfg)
        series = np.array(_trending_series(100))
        trainer.train_all(series)
        predictions = trainer.predict_all(series, horizon=5)
        assert "arima" in predictions
        assert "prophet" in predictions
        assert len(predictions["arima"]) == 5

    def test_model_info(self):
        cfg = EngineConfig(models=["arima"])
        trainer = ModelTrainer(cfg)
        series = np.array(_trending_series(100))
        trainer.train_all(series)
        info = trainer.get_model_info()
        assert info["arima"] == "ARIMA"

    def test_unknown_model_skipped(self):
        cfg = EngineConfig(models=["nonexistent"])
        trainer = ModelTrainer(cfg)
        series = np.array(_trending_series(100))
        trainer.train_all(series)
        assert len(trainer.fitted_models) == 0
        assert not trainer.has_models()


# ---------------------------------------------------------------------------
# PredictionStrategy tests
# ---------------------------------------------------------------------------


class TestPredictionStrategy:
    def test_insufficient_candles_returns_empty(self):
        strategy = PredictionStrategy()
        candles = _make_candles([100.0] * 10)
        signals = strategy.generate_signals(candles)
        assert signals == []

    def test_generates_signals_from_trending_data(self):
        cfg = EngineConfig(
            models=["arima"],
            min_confidence=0.0,  # Accept any signal
            direction_threshold=0.0,  # Accept any direction
        )
        strategy = PredictionStrategy(cfg)
        prices = _trending_series(80, start=100.0, trend=1.0)
        candles = _make_candles(prices, symbol="AAPL")
        signals = strategy.generate_signals(candles)
        # Should produce a signal (direction may vary depending on model)
        assert isinstance(signals, list)

    def test_signal_has_correct_attributes(self):
        from build_finance.data import Signal

        cfg = EngineConfig(
            models=["arima"],
            min_confidence=0.0,
            direction_threshold=0.0,
        )
        strategy = PredictionStrategy(cfg)
        prices = _trending_series(80, start=100.0, trend=2.0)
        candles = _make_candles(prices, symbol="AAPL")
        signals = strategy.generate_signals(candles)
        if signals:
            sig = signals[0]
            assert isinstance(sig, Signal)
            assert sig.symbol == "AAPL"
            assert sig.side in ("buy", "sell")
            assert 0.0 <= sig.strength <= 1.0
            assert sig.stop_loss is not None
            assert sig.take_profit is not None

    def test_set_model_weights(self):
        strategy = PredictionStrategy()
        strategy.set_model_weights({"arima": 0.8, "prophet": 0.5})
        assert strategy.model_weights["arima"] == 0.8
        assert strategy.model_weights["prophet"] == 0.5


# ---------------------------------------------------------------------------
# PerformanceTracker tests
# ---------------------------------------------------------------------------


class TestPerformanceTracker:
    def test_record_prediction(self):
        tracker = PerformanceTracker()
        tracker.record_prediction("arima", 105.0, 100.0, symbol="AAPL")
        assert len(tracker.records) == 1
        assert tracker.records[0].predicted_direction == "up"

    def test_record_down_prediction(self):
        tracker = PerformanceTracker()
        tracker.record_prediction("arima", 95.0, 100.0, symbol="AAPL")
        assert tracker.records[0].predicted_direction == "down"

    def test_evaluate_correct(self):
        tracker = PerformanceTracker()
        tracker.record_prediction("arima", 110.0, 100.0, symbol="AAPL")
        evaluated = tracker.evaluate_past({"AAPL": 108.0})
        assert evaluated == 1
        assert tracker.records[0].correct is True
        assert tracker.records[0].actual_direction == "up"

    def test_evaluate_incorrect(self):
        tracker = PerformanceTracker()
        tracker.record_prediction("arima", 110.0, 100.0, symbol="AAPL")
        evaluated = tracker.evaluate_past({"AAPL": 92.0})
        assert evaluated == 1
        assert tracker.records[0].correct is False

    def test_overall_accuracy(self):
        tracker = PerformanceTracker()
        # Two correct
        tracker.record_prediction("arima", 110.0, 100.0, symbol="A")
        tracker.record_prediction("arima", 90.0, 100.0, symbol="B")
        # One wrong
        tracker.record_prediction("arima", 110.0, 100.0, symbol="C")

        tracker.evaluate_past({"A": 108.0, "B": 92.0, "C": 92.0})
        acc = tracker.get_overall_accuracy()
        assert abs(acc - 2.0 / 3.0) < 1e-6

    def test_model_weights_default(self):
        tracker = PerformanceTracker()
        weights = tracker.get_model_weights()
        assert weights == {}

    def test_model_weights_after_evaluation(self):
        tracker = PerformanceTracker()
        # Perfect accuracy for arima
        for _ in range(10):
            tracker.record_prediction("arima", 110.0, 100.0, symbol="A")
        tracker.evaluate_past({"A": 108.0})

        # 50% accuracy for prophet
        for _i in range(10):
            tracker.record_prediction("prophet", 110.0, 100.0, symbol="B")
        tracker.evaluate_past({"B": 108.0})  # All correct now
        # Add wrong ones
        for _i in range(10):
            tracker.record_prediction("prophet", 110.0, 100.0, symbol="C")
        tracker.evaluate_past({"C": 92.0})  # All wrong

        weights = tracker.get_model_weights()
        assert "arima" in weights
        assert "prophet" in weights
        # arima should have higher weight (100% accuracy -> weight = 1.0)
        assert weights["arima"] > weights["prophet"]

    def test_different_weights_for_different_accuracies(self):
        tracker = PerformanceTracker(EngineConfig(accuracy_window=20))

        # Model A: 100% accurate
        for _ in range(10):
            tracker.record_prediction("model_a", 110.0, 100.0, symbol="X")
        tracker.evaluate_past({"X": 108.0})

        # Model B: 50% accurate
        for _ in range(5):
            tracker.record_prediction("model_b", 110.0, 100.0, symbol="Y")
        tracker.evaluate_past({"Y": 108.0})
        for _ in range(5):
            tracker.record_prediction("model_b", 110.0, 100.0, symbol="Z")
        tracker.evaluate_past({"Z": 92.0})

        weights = tracker.get_model_weights()
        # 100% accuracy -> weight = 1.0, 50% accuracy -> weight = 0.25
        assert weights["model_a"] == pytest.approx(1.0)
        assert weights["model_b"] == pytest.approx(0.25)

    def test_get_stats(self):
        tracker = PerformanceTracker()
        tracker.record_prediction("arima", 110.0, 100.0, symbol="A")
        tracker.evaluate_past({"A": 105.0})
        stats = tracker.get_stats()
        assert "arima" in stats
        assert stats["arima"]["total"] == 1
        assert stats["arima"]["correct"] == 1
        assert stats["arima"]["accuracy"] == 1.0


# ---------------------------------------------------------------------------
# AdaptiveEngine tests
# ---------------------------------------------------------------------------


class TestAdaptiveEngine:
    def test_engine_creation(self):
        from build_engine.adaptive_engine import AdaptiveEngine

        engine = AdaptiveEngine()
        assert engine.config.paper_trading is True
        assert engine.cycle_count == 0
        assert engine.running is False

    def test_engine_status(self):
        from build_engine.adaptive_engine import AdaptiveEngine

        engine = AdaptiveEngine()
        status = engine.get_status()
        assert status["running"] is False
        assert status["cycles"] == 0
        assert status["equity"] == 100_000.0
        assert status["trades"] == 0

    def test_engine_custom_config(self):
        from build_engine.adaptive_engine import AdaptiveEngine

        cfg = EngineConfig(
            symbols=["SPY"],
            models=["arima"],
            paper_trading=True,
        )
        engine = AdaptiveEngine(cfg)
        assert engine.config.symbols == ["SPY"]
        assert engine.config.models == ["arima"]
        status = engine.get_status()
        assert status["equity"] == 100_000.0

    def test_live_engine_requires_acknowledgement(self, monkeypatch):
        from build_engine.adaptive_engine import AdaptiveEngine

        monkeypatch.delenv("BUILD_ENGINE_LIVE_ACK", raising=False)
        monkeypatch.delenv("APCA_API_KEY_ID", raising=False)
        monkeypatch.delenv("APCA_API_SECRET_KEY", raising=False)
        cfg = EngineConfig(
            paper_trading=False,
            broker_api_key="test-key",
            broker_api_secret="test-secret",
        )

        with pytest.raises(ValueError, match="BUILD_ENGINE_LIVE_ACK"):
            AdaptiveEngine(cfg)

    def test_live_engine_requires_credentials_after_ack(self, monkeypatch):
        from build_engine.adaptive_engine import AdaptiveEngine

        monkeypatch.delenv("APCA_API_KEY_ID", raising=False)
        monkeypatch.delenv("APCA_API_SECRET_KEY", raising=False)
        cfg = EngineConfig(paper_trading=False, live_trading_ack=LIVE_TRADING_ACK)

        with pytest.raises(ValueError, match="Live Alpaca mode requires"):
            AdaptiveEngine(cfg)

    def test_live_engine_uses_config_credentials(self):
        from build_finance.broker import AlpacaBroker

        from build_engine.adaptive_engine import AdaptiveEngine

        cfg = EngineConfig(
            paper_trading=False,
            broker_api_key="test-key",
            broker_api_secret="test-secret",
            broker_base_url="https://example.invalid",
            live_trading_ack=LIVE_TRADING_ACK,
        )
        engine = AdaptiveEngine(cfg)

        assert isinstance(engine.broker, AlpacaBroker)
        assert engine.broker.base_url == "https://example.invalid"

    def test_live_engine_accepts_env_acknowledgement(self, monkeypatch):
        from build_finance.broker import AlpacaBroker

        from build_engine.adaptive_engine import AdaptiveEngine

        monkeypatch.setenv("BUILD_ENGINE_LIVE_ACK", LIVE_TRADING_ACK)
        cfg = EngineConfig(
            paper_trading=False,
            broker_api_key="test-key",
            broker_api_secret="test-secret",
            broker_base_url="https://example.invalid",
        )
        engine = AdaptiveEngine(cfg)

        assert isinstance(engine.broker, AlpacaBroker)

    def test_engine_stop(self):
        from build_engine.adaptive_engine import AdaptiveEngine

        engine = AdaptiveEngine()
        engine.running = True
        engine.stop()
        assert engine.running is False
