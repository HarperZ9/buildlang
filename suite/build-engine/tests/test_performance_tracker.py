"""
Tests for build_engine.performance_tracker.PerformanceTracker.

Covers record creation, direction classification, evaluation correctness,
model weight computation, per-model stats, overall accuracy, and the
accuracy_window windowing behavior.
"""

from __future__ import annotations

import time

import pytest

from build_engine.config import EngineConfig
from build_engine.performance_tracker import PerformanceTracker

# ---------------------------------------------------------------------------
# Construction
# ---------------------------------------------------------------------------


class TestTrackerCreation:
    def test_default_creation(self):
        tracker = PerformanceTracker()
        assert tracker.records == []
        assert tracker.config.accuracy_window == 50
        assert tracker.config.direction_threshold == 0.005

    def test_custom_config(self):
        cfg = EngineConfig(accuracy_window=10, direction_threshold=0.01)
        tracker = PerformanceTracker(cfg)
        assert tracker.config.accuracy_window == 10
        assert tracker.config.direction_threshold == 0.01

    def test_repr_empty(self):
        tracker = PerformanceTracker()
        assert "total=0" in repr(tracker)
        assert "evaluated=0" in repr(tracker)


# ---------------------------------------------------------------------------
# Recording predictions
# ---------------------------------------------------------------------------


class TestRecordPrediction:
    def test_record_creates_entry(self):
        tracker = PerformanceTracker()
        tracker.record_prediction("arima", 105.0, 100.0, symbol="AAPL")
        assert len(tracker.records) == 1
        assert tracker.records[0].model_name == "arima"
        assert tracker.records[0].symbol == "AAPL"

    def test_direction_up(self):
        tracker = PerformanceTracker()
        tracker.record_prediction("arima", 110.0, 100.0, symbol="AAPL")
        assert tracker.records[0].predicted_direction == "up"

    def test_direction_down(self):
        tracker = PerformanceTracker()
        tracker.record_prediction("arima", 90.0, 100.0, symbol="AAPL")
        assert tracker.records[0].predicted_direction == "down"

    def test_direction_flat(self):
        tracker = PerformanceTracker()
        # Move of 0.1% is below the default 0.5% threshold
        tracker.record_prediction("arima", 100.1, 100.0, symbol="AAPL")
        assert tracker.records[0].predicted_direction == "flat"

    def test_predicted_magnitude_stored(self):
        tracker = PerformanceTracker()
        tracker.record_prediction("arima", 110.0, 100.0, symbol="AAPL")
        assert tracker.records[0].predicted_magnitude == pytest.approx(0.1)

    def test_timestamp_defaults_to_now(self):
        tracker = PerformanceTracker()
        before = time.time()
        tracker.record_prediction("arima", 110.0, 100.0, symbol="X")
        after = time.time()
        assert before <= tracker.records[0].timestamp <= after

    def test_explicit_timestamp(self):
        tracker = PerformanceTracker()
        tracker.record_prediction("arima", 110.0, 100.0, timestamp=1234567.0, symbol="X")
        assert tracker.records[0].timestamp == 1234567.0

    def test_record_not_evaluated_initially(self):
        tracker = PerformanceTracker()
        tracker.record_prediction("arima", 110.0, 100.0, symbol="X")
        assert tracker.records[0].evaluated is False
        assert tracker.records[0].correct is False

    def test_multiple_records(self):
        tracker = PerformanceTracker()
        tracker.record_prediction("arima", 110.0, 100.0, symbol="A")
        tracker.record_prediction("prophet", 95.0, 100.0, symbol="B")
        tracker.record_prediction("neural", 100.0, 100.0, symbol="C")
        assert len(tracker.records) == 3
        assert tracker.records[0].model_name == "arima"
        assert tracker.records[1].model_name == "prophet"
        assert tracker.records[2].model_name == "neural"


# ---------------------------------------------------------------------------
# Evaluation
# ---------------------------------------------------------------------------


class TestEvaluatePast:
    def test_evaluate_marks_evaluated(self):
        tracker = PerformanceTracker()
        tracker.record_prediction("arima", 110.0, 100.0, symbol="AAPL")
        count = tracker.evaluate_past({"AAPL": 108.0})
        assert count == 1
        assert tracker.records[0].evaluated is True

    def test_correct_up_prediction(self):
        tracker = PerformanceTracker()
        tracker.record_prediction("arima", 110.0, 100.0, symbol="AAPL")
        tracker.evaluate_past({"AAPL": 108.0})
        assert tracker.records[0].correct is True
        assert tracker.records[0].actual_direction == "up"

    def test_incorrect_up_prediction(self):
        tracker = PerformanceTracker()
        tracker.record_prediction("arima", 110.0, 100.0, symbol="AAPL")
        tracker.evaluate_past({"AAPL": 92.0})
        assert tracker.records[0].correct is False
        assert tracker.records[0].actual_direction == "down"

    def test_correct_down_prediction(self):
        tracker = PerformanceTracker()
        tracker.record_prediction("arima", 90.0, 100.0, symbol="AAPL")
        tracker.evaluate_past({"AAPL": 92.0})
        assert tracker.records[0].correct is True
        assert tracker.records[0].actual_direction == "down"

    def test_incorrect_down_prediction(self):
        tracker = PerformanceTracker()
        tracker.record_prediction("arima", 90.0, 100.0, symbol="AAPL")
        tracker.evaluate_past({"AAPL": 108.0})
        assert tracker.records[0].correct is False
        assert tracker.records[0].actual_direction == "up"

    def test_already_evaluated_skipped(self):
        tracker = PerformanceTracker()
        tracker.record_prediction("arima", 110.0, 100.0, symbol="AAPL")
        tracker.evaluate_past({"AAPL": 108.0})
        count = tracker.evaluate_past({"AAPL": 115.0})
        assert count == 0  # already evaluated, no re-evaluation

    def test_missing_symbol_not_evaluated(self):
        tracker = PerformanceTracker()
        tracker.record_prediction("arima", 110.0, 100.0, symbol="AAPL")
        count = tracker.evaluate_past({"MSFT": 108.0})
        assert count == 0
        assert tracker.records[0].evaluated is False

    def test_evaluate_multiple_symbols(self):
        tracker = PerformanceTracker()
        tracker.record_prediction("arima", 110.0, 100.0, symbol="A")
        tracker.record_prediction("arima", 90.0, 100.0, symbol="B")
        count = tracker.evaluate_past({"A": 108.0, "B": 92.0})
        assert count == 2
        assert tracker.records[0].correct is True
        assert tracker.records[1].correct is True


# ---------------------------------------------------------------------------
# Model weights
# ---------------------------------------------------------------------------


class TestModelWeights:
    def test_default_weights_empty(self):
        tracker = PerformanceTracker()
        assert tracker.get_model_weights() == {}

    def test_no_evaluated_returns_empty(self):
        tracker = PerformanceTracker()
        tracker.record_prediction("arima", 110.0, 100.0, symbol="X")
        assert tracker.get_model_weights() == {}

    def test_perfect_accuracy_weight_is_one(self):
        tracker = PerformanceTracker()
        for _ in range(5):
            tracker.record_prediction("arima", 110.0, 100.0, symbol="A")
        tracker.evaluate_past({"A": 108.0})
        weights = tracker.get_model_weights()
        assert weights["arima"] == pytest.approx(1.0)

    def test_zero_accuracy_weight_is_minimum(self):
        tracker = PerformanceTracker()
        for _ in range(5):
            tracker.record_prediction("arima", 110.0, 100.0, symbol="A")
        tracker.evaluate_past({"A": 92.0})  # predicted up, went down
        weights = tracker.get_model_weights()
        assert weights["arima"] == pytest.approx(0.01)

    def test_fifty_percent_accuracy_weight(self):
        tracker = PerformanceTracker(EngineConfig(accuracy_window=20))
        # 5 correct
        for _ in range(5):
            tracker.record_prediction("m", 110.0, 100.0, symbol="Y")
        tracker.evaluate_past({"Y": 108.0})
        # 5 wrong
        for _ in range(5):
            tracker.record_prediction("m", 110.0, 100.0, symbol="Z")
        tracker.evaluate_past({"Z": 92.0})
        weights = tracker.get_model_weights()
        # 50% accuracy -> weight = 0.5^2 = 0.25
        assert weights["m"] == pytest.approx(0.25)

    def test_more_accurate_model_has_higher_weight(self):
        tracker = PerformanceTracker(EngineConfig(accuracy_window=20))
        # Model A: 100% correct
        for _ in range(5):
            tracker.record_prediction("model_a", 110.0, 100.0, symbol="X")
        tracker.evaluate_past({"X": 108.0})
        # Model B: 50% correct
        for _ in range(3):
            tracker.record_prediction("model_b", 110.0, 100.0, symbol="Y")
        tracker.evaluate_past({"Y": 108.0})
        for _ in range(3):
            tracker.record_prediction("model_b", 110.0, 100.0, symbol="Z")
        tracker.evaluate_past({"Z": 92.0})
        weights = tracker.get_model_weights()
        assert weights["model_a"] > weights["model_b"]


# ---------------------------------------------------------------------------
# Accuracy window
# ---------------------------------------------------------------------------


class TestAccuracyWindow:
    def test_window_limits_records(self):
        cfg = EngineConfig(accuracy_window=3)
        tracker = PerformanceTracker(cfg)
        # Record 5 predictions, all wrong
        for _ in range(5):
            tracker.record_prediction("arima", 110.0, 100.0, symbol="A")
        tracker.evaluate_past({"A": 92.0})
        # Record 3 more, all correct (these fill the window=3)
        for _ in range(3):
            tracker.record_prediction("arima", 110.0, 100.0, symbol="B")
        tracker.evaluate_past({"B": 108.0})
        weights = tracker.get_model_weights()
        # Window=3 means only the 3 most recent evaluated records count
        # All 3 most recent are correct -> weight = 1.0
        assert weights["arima"] == pytest.approx(1.0)


# ---------------------------------------------------------------------------
# Statistics
# ---------------------------------------------------------------------------


class TestStats:
    def test_get_stats_empty(self):
        tracker = PerformanceTracker()
        assert tracker.get_stats() == {}

    def test_get_stats_single_model(self):
        tracker = PerformanceTracker()
        tracker.record_prediction("arima", 110.0, 100.0, symbol="A")
        tracker.evaluate_past({"A": 108.0})
        stats = tracker.get_stats()
        assert "arima" in stats
        assert stats["arima"]["total"] == 1
        assert stats["arima"]["correct"] == 1
        assert stats["arima"]["accuracy"] == 1.0

    def test_get_stats_multiple_models(self):
        tracker = PerformanceTracker()
        tracker.record_prediction("arima", 110.0, 100.0, symbol="A")
        tracker.record_prediction("prophet", 90.0, 100.0, symbol="B")
        tracker.evaluate_past({"A": 108.0, "B": 92.0})
        stats = tracker.get_stats()
        assert len(stats) == 2
        assert stats["arima"]["accuracy"] == 1.0
        assert stats["prophet"]["accuracy"] == 1.0

    def test_get_stats_with_incorrect(self):
        tracker = PerformanceTracker()
        tracker.record_prediction("arima", 110.0, 100.0, symbol="A")
        tracker.record_prediction("arima", 110.0, 100.0, symbol="B")
        tracker.evaluate_past({"A": 108.0, "B": 92.0})
        stats = tracker.get_stats()
        assert stats["arima"]["total"] == 2
        assert stats["arima"]["correct"] == 1
        assert stats["arima"]["accuracy"] == pytest.approx(0.5)


# ---------------------------------------------------------------------------
# Overall accuracy
# ---------------------------------------------------------------------------


class TestOverallAccuracy:
    def test_no_evaluated_returns_zero(self):
        tracker = PerformanceTracker()
        assert tracker.get_overall_accuracy() == 0.0

    def test_all_correct(self):
        tracker = PerformanceTracker()
        tracker.record_prediction("arima", 110.0, 100.0, symbol="A")
        tracker.record_prediction("arima", 90.0, 100.0, symbol="B")
        tracker.evaluate_past({"A": 108.0, "B": 92.0})
        assert tracker.get_overall_accuracy() == pytest.approx(1.0)

    def test_mixed_results(self):
        tracker = PerformanceTracker()
        tracker.record_prediction("arima", 110.0, 100.0, symbol="A")
        tracker.record_prediction("arima", 90.0, 100.0, symbol="B")
        tracker.record_prediction("arima", 110.0, 100.0, symbol="C")
        tracker.evaluate_past({"A": 108.0, "B": 92.0, "C": 92.0})
        assert tracker.get_overall_accuracy() == pytest.approx(2.0 / 3.0)

    def test_all_wrong(self):
        tracker = PerformanceTracker()
        tracker.record_prediction("arima", 110.0, 100.0, symbol="A")
        tracker.record_prediction("arima", 110.0, 100.0, symbol="B")
        tracker.evaluate_past({"A": 92.0, "B": 92.0})
        assert tracker.get_overall_accuracy() == pytest.approx(0.0)

    def test_repr_after_evaluation(self):
        tracker = PerformanceTracker()
        tracker.record_prediction("arima", 110.0, 100.0, symbol="A")
        tracker.evaluate_past({"A": 108.0})
        assert "total=1" in repr(tracker)
        assert "evaluated=1" in repr(tracker)
