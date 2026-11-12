"""
Tests for build_engine.alerts -- regime change detection and alert system.

Covers each detection method with synthetic data, AlertManager delivery
pipeline, rate limiting, JSONL log writing, callback delivery, and
deduplication.
"""

from __future__ import annotations

import json
import sys
from datetime import datetime, timedelta

import numpy as np
import pytest

from build_engine.alerts import (
    Alert,
    AlertConfig,
    AlertLevel,
    AlertManager,
    AlertType,
    RegimeDetector,
)

# ---------------------------------------------------------------------------
# Alert dataclass
# ---------------------------------------------------------------------------


class TestAlert:
    def test_to_dict_roundtrip(self):
        alert = Alert(
            timestamp=datetime(2026, 1, 1, 12, 0),
            alert_type=AlertType.DRAWDOWN,
            level=AlertLevel.WARNING,
            message="test",
            details={"key": "value"},
        )
        d = alert.to_dict()
        assert d["alert_type"] == "drawdown"
        assert d["level"] == "warning"
        assert d["message"] == "test"
        assert d["details"]["key"] == "value"
        assert d["acknowledged"] is False

    def test_acknowledged_default_false(self):
        alert = Alert(
            timestamp=datetime.now(),
            alert_type=AlertType.DRAWDOWN,
            level=AlertLevel.INFO,
            message="x",
        )
        assert alert.acknowledged is False


# ---------------------------------------------------------------------------
# Trend reversal detection
# ---------------------------------------------------------------------------


class TestTrendReversal:
    def test_bullish_crossover(self):
        cfg = AlertConfig(trend_fast_period=3, trend_slow_period=5)
        det = RegimeDetector(cfg)
        # Need: aligned[-2] fast < slow, aligned[-1] fast > slow.
        # Verified: at the second-to-last aligned step fast_ma=-0.40
        # and at the last step fast_ma=+2.27 -- that IS the cross.
        # Extend the data by one so the cross lands on the final pair.
        prices = np.array(
            [
                20,
                20,
                18,
                16,
                14,
                12,
                10,
                8,
                6,
                6,
                12,
                20,
            ],
            dtype=float,
        )
        alert = det.check_trend_reversal(prices)
        assert alert is not None
        assert alert.alert_type == AlertType.TREND_REVERSAL
        assert alert.details["direction"] == "bullish"

    def test_bearish_crossover(self):
        cfg = AlertConfig(trend_fast_period=3, trend_slow_period=5)
        det = RegimeDetector(cfg)
        # Mirror of the bullish case: cross at the final aligned pair.
        prices = np.array(
            [
                5,
                5,
                7,
                9,
                11,
                13,
                15,
                17,
                19,
                19,
                13,
                7,
            ],
            dtype=float,
        )
        alert = det.check_trend_reversal(prices)
        assert alert is not None
        assert alert.alert_type == AlertType.TREND_REVERSAL
        assert alert.details["direction"] == "bearish"

    def test_no_crossover_flat(self):
        cfg = AlertConfig(trend_fast_period=3, trend_slow_period=5)
        det = RegimeDetector(cfg)
        prices = np.ones(20) * 100.0
        alert = det.check_trend_reversal(prices)
        assert alert is None

    def test_insufficient_data(self):
        cfg = AlertConfig(trend_fast_period=3, trend_slow_period=10)
        det = RegimeDetector(cfg)
        prices = np.arange(5, dtype=float)
        alert = det.check_trend_reversal(prices)
        assert alert is None


# ---------------------------------------------------------------------------
# Volatility spike detection
# ---------------------------------------------------------------------------


class TestVolatilitySpike:
    def test_spike_detected(self):
        cfg = AlertConfig(volatility_threshold=2.0)
        det = RegimeDetector(cfg)
        # 25 calm days, then 5 wild days
        calm = np.random.default_rng(42).normal(0.0, 0.01, size=25)
        wild = np.random.default_rng(42).normal(0.0, 0.10, size=5)
        returns = np.concatenate([calm, wild])
        alert = det.check_volatility_spike(returns)
        assert alert is not None
        assert alert.alert_type == AlertType.VOLATILITY_SPIKE
        assert alert.details["ratio"] > 2.0

    def test_critical_level_on_extreme_spike(self):
        # Need ratio > threshold * 1.5. With 25 calm + 5 wild days in a
        # 30-day window, the max achievable ratio is ~2.41 due to the
        # wild days inflating the 30-day std. Use a lower threshold so
        # that 2.41 exceeds threshold * 1.5 = 1.5 * 1.5 = 2.25.
        cfg = AlertConfig(volatility_threshold=1.5)
        det = RegimeDetector(cfg)
        calm = np.zeros(25)
        wild = np.array([10.0, -10.0, 10.0, -10.0, 10.0])
        returns = np.concatenate([calm, wild])
        alert = det.check_volatility_spike(returns)
        assert alert is not None
        assert alert.level == AlertLevel.CRITICAL

    def test_no_spike_calm_market(self):
        cfg = AlertConfig(volatility_threshold=2.0)
        det = RegimeDetector(cfg)
        returns = np.random.default_rng(99).normal(0.0, 0.01, size=40)
        alert = det.check_volatility_spike(returns)
        assert alert is None

    def test_insufficient_data(self):
        det = RegimeDetector()
        returns = np.array([0.01, -0.01])
        alert = det.check_volatility_spike(returns)
        assert alert is None

    def test_zero_historical_vol(self):
        det = RegimeDetector()
        returns = np.zeros(30)
        alert = det.check_volatility_spike(returns)
        assert alert is None


# ---------------------------------------------------------------------------
# Accuracy drop detection
# ---------------------------------------------------------------------------


class TestAccuracyDrop:
    def test_drop_detected(self):
        cfg = AlertConfig(accuracy_threshold=0.45, accuracy_window=10)
        det = RegimeDetector(cfg)
        preds = ["up"] * 10
        # Only 3 correct out of 10 = 30%
        actuals = ["up"] * 3 + ["down"] * 7
        alert = det.check_accuracy_drop(preds, actuals)
        assert alert is not None
        assert alert.alert_type == AlertType.ACCURACY_DROP
        assert alert.details["accuracy"] == pytest.approx(0.3)

    def test_critical_on_very_low_accuracy(self):
        cfg = AlertConfig(accuracy_threshold=0.45, accuracy_window=10)
        det = RegimeDetector(cfg)
        preds = ["up"] * 10
        actuals = ["down"] * 10  # 0% accuracy
        alert = det.check_accuracy_drop(preds, actuals)
        assert alert is not None
        assert alert.level == AlertLevel.CRITICAL

    def test_no_drop_when_accurate(self):
        cfg = AlertConfig(accuracy_threshold=0.45, accuracy_window=10)
        det = RegimeDetector(cfg)
        preds = ["up"] * 10
        actuals = ["up"] * 10
        alert = det.check_accuracy_drop(preds, actuals)
        assert alert is None

    def test_insufficient_data(self):
        cfg = AlertConfig(accuracy_window=20)
        det = RegimeDetector(cfg)
        preds = ["up"] * 5
        actuals = ["up"] * 5
        alert = det.check_accuracy_drop(preds, actuals)
        assert alert is None

    def test_uses_most_recent_window(self):
        cfg = AlertConfig(accuracy_threshold=0.45, accuracy_window=5)
        det = RegimeDetector(cfg)
        # Old accurate data + recent bad data
        preds = ["up"] * 20
        actuals = ["up"] * 15 + ["down"] * 5
        alert = det.check_accuracy_drop(preds, actuals)
        assert alert is not None
        # Last 5: 0/5 correct = 0%
        assert alert.details["accuracy"] == pytest.approx(0.0)


# ---------------------------------------------------------------------------
# Weight shift detection
# ---------------------------------------------------------------------------


class TestWeightShift:
    def test_shift_detected(self):
        cfg = AlertConfig(weight_shift_threshold=0.3)
        det = RegimeDetector(cfg)
        prev = {"arima": 0.5, "prophet": 0.5}
        curr = {"arima": 0.9, "prophet": 0.1}
        alert = det.check_weight_shift(curr, prev)
        assert alert is not None
        assert alert.alert_type == AlertType.WEIGHT_SHIFT
        assert alert.details["shift"] == pytest.approx(0.4)

    def test_no_shift_stable(self):
        cfg = AlertConfig(weight_shift_threshold=0.3)
        det = RegimeDetector(cfg)
        prev = {"arima": 0.5, "prophet": 0.5}
        curr = {"arima": 0.55, "prophet": 0.45}
        alert = det.check_weight_shift(curr, prev)
        assert alert is None

    def test_new_model_appearing(self):
        cfg = AlertConfig(weight_shift_threshold=0.3)
        det = RegimeDetector(cfg)
        prev = {"arima": 1.0}
        curr = {"arima": 0.5, "neural": 0.5}
        alert = det.check_weight_shift(curr, prev)
        assert alert is not None
        assert alert.details["shift"] == pytest.approx(0.5)

    def test_empty_weights_no_alert(self):
        det = RegimeDetector()
        assert det.check_weight_shift({}, {"arima": 1.0}) is None
        assert det.check_weight_shift({"arima": 1.0}, {}) is None
        assert det.check_weight_shift({}, {}) is None


# ---------------------------------------------------------------------------
# Drawdown detection
# ---------------------------------------------------------------------------


class TestDrawdown:
    def test_drawdown_detected(self):
        cfg = AlertConfig(drawdown_threshold=0.10)
        det = RegimeDetector(cfg)
        # Equity rises to 100k then drops 15%
        equity = np.array([90_000, 95_000, 100_000, 92_000, 85_000], dtype=float)
        alert = det.check_drawdown(equity)
        assert alert is not None
        assert alert.alert_type == AlertType.DRAWDOWN
        assert alert.details["drawdown"] == pytest.approx(0.15)

    def test_critical_on_deep_drawdown(self):
        cfg = AlertConfig(drawdown_threshold=0.10)
        det = RegimeDetector(cfg)
        equity = np.array([100_000, 70_000], dtype=float)
        alert = det.check_drawdown(equity)
        assert alert is not None
        assert alert.level == AlertLevel.CRITICAL

    def test_no_drawdown_rising(self):
        cfg = AlertConfig(drawdown_threshold=0.10)
        det = RegimeDetector(cfg)
        equity = np.array([80_000, 90_000, 95_000, 100_000], dtype=float)
        alert = det.check_drawdown(equity)
        assert alert is None

    def test_insufficient_data(self):
        det = RegimeDetector()
        equity = np.array([100_000], dtype=float)
        alert = det.check_drawdown(equity)
        assert alert is None

    def test_exactly_at_threshold_no_alert(self):
        cfg = AlertConfig(drawdown_threshold=0.10)
        det = RegimeDetector(cfg)
        # Exactly 10% drawdown -- threshold is >, not >=
        equity = np.array([100_000, 90_000], dtype=float)
        alert = det.check_drawdown(equity)
        assert alert is None


# ---------------------------------------------------------------------------
# AlertManager -- callback delivery
# ---------------------------------------------------------------------------


class TestCallbackDelivery:
    def test_callback_receives_alert(self, tmp_path):
        cfg = AlertConfig(
            drawdown_threshold=0.05,
            log_path=tmp_path / "alerts.jsonl",
            dedup_window_seconds=0,
        )
        mgr = AlertManager(cfg)
        received: list[Alert] = []
        mgr.register_callback(lambda a: received.append(a))

        equity = np.array([100_000, 90_000], dtype=float)
        alerts = mgr.check_all(equity=equity)

        assert len(alerts) == 1
        assert len(received) == 1
        assert received[0].alert_type == AlertType.DRAWDOWN

    def test_multiple_callbacks(self, tmp_path):
        cfg = AlertConfig(
            drawdown_threshold=0.05,
            log_path=tmp_path / "alerts.jsonl",
            dedup_window_seconds=0,
        )
        mgr = AlertManager(cfg)
        box_a: list[Alert] = []
        box_b: list[Alert] = []
        mgr.register_callback(lambda a: box_a.append(a))
        mgr.register_callback(lambda a: box_b.append(a))

        equity = np.array([100_000, 80_000], dtype=float)
        mgr.check_all(equity=equity)

        assert len(box_a) == 1
        assert len(box_b) == 1

    def test_callback_error_does_not_crash(self, tmp_path):
        cfg = AlertConfig(
            drawdown_threshold=0.05,
            log_path=tmp_path / "alerts.jsonl",
            dedup_window_seconds=0,
        )
        mgr = AlertManager(cfg)

        def bad_callback(a: Alert) -> None:
            raise RuntimeError("boom")

        mgr.register_callback(bad_callback)
        equity = np.array([100_000, 80_000], dtype=float)
        # Should not raise
        alerts = mgr.check_all(equity=equity)
        assert len(alerts) == 1


# ---------------------------------------------------------------------------
# AlertManager -- JSONL log
# ---------------------------------------------------------------------------


class TestJsonlLog:
    def test_alert_written_to_log(self, tmp_path):
        log_file = tmp_path / "alerts.jsonl"
        cfg = AlertConfig(
            drawdown_threshold=0.05,
            log_path=log_file,
            dedup_window_seconds=0,
        )
        mgr = AlertManager(cfg)
        equity = np.array([100_000, 80_000], dtype=float)
        mgr.check_all(equity=equity)

        assert log_file.exists()
        lines = log_file.read_text(encoding="utf-8").strip().split("\n")
        assert len(lines) == 1
        data = json.loads(lines[0])
        assert data["alert_type"] == "drawdown"
        assert data["level"] == "critical"

    def test_multiple_alerts_appended(self, tmp_path):
        log_file = tmp_path / "alerts.jsonl"
        cfg = AlertConfig(
            drawdown_threshold=0.05,
            log_path=log_file,
            dedup_window_seconds=0,
        )
        mgr = AlertManager(cfg)

        # First alert: drawdown
        equity = np.array([100_000, 80_000], dtype=float)
        mgr.check_all(equity=equity)

        # Second alert: accuracy drop (reset dedup via new manager)
        mgr2 = AlertManager(cfg)
        preds = ["up"] * 20
        actuals = ["down"] * 20
        mgr2.check_all(predictions=preds, actuals=actuals)

        lines = log_file.read_text(encoding="utf-8").strip().split("\n")
        assert len(lines) == 2

    def test_log_creates_parent_dirs(self, tmp_path):
        log_file = tmp_path / "sub" / "dir" / "alerts.jsonl"
        cfg = AlertConfig(
            drawdown_threshold=0.05,
            log_path=log_file,
            dedup_window_seconds=0,
        )
        mgr = AlertManager(cfg)
        equity = np.array([100_000, 80_000], dtype=float)
        mgr.check_all(equity=equity)
        assert log_file.exists()


# ---------------------------------------------------------------------------
# AlertManager -- rate limiting
# ---------------------------------------------------------------------------


@pytest.mark.skipif(sys.platform == "win32", reason="Timing-sensitive tests unstable on Windows CI")
class TestRateLimiting:
    def test_respects_max_alerts_per_hour(self, tmp_path):
        cfg = AlertConfig(
            drawdown_threshold=0.01,
            log_path=tmp_path / "alerts.jsonl",
            max_alerts_per_hour=3,
            dedup_window_seconds=0,
        )
        mgr = AlertManager(cfg)
        equity = np.array([100_000, 50_000], dtype=float)

        delivered = 0
        for _ in range(10):
            alerts = mgr.check_all(equity=equity)
            delivered += len(alerts)

        assert delivered == 3

    def test_rate_limit_resets_after_hour(self, tmp_path):
        cfg = AlertConfig(
            drawdown_threshold=0.01,
            log_path=tmp_path / "alerts.jsonl",
            max_alerts_per_hour=2,
            dedup_window_seconds=0,
        )
        mgr = AlertManager(cfg)
        equity = np.array([100_000, 50_000], dtype=float)

        # Exhaust the limit
        for _ in range(5):
            mgr.check_all(equity=equity)

        assert mgr._hourly_count == 2

        # Simulate hour rollover
        mgr._hour_start = datetime.now() - timedelta(hours=2)

        alerts = mgr.check_all(equity=equity)
        assert len(alerts) == 1
        assert mgr._hourly_count == 1


# ---------------------------------------------------------------------------
# AlertManager -- deduplication
# ---------------------------------------------------------------------------


class TestDeduplication:
    def test_same_alert_type_suppressed(self, tmp_path):
        cfg = AlertConfig(
            drawdown_threshold=0.05,
            log_path=tmp_path / "alerts.jsonl",
            dedup_window_seconds=300,
            max_alerts_per_hour=100,
        )
        mgr = AlertManager(cfg)
        equity = np.array([100_000, 80_000], dtype=float)

        first = mgr.check_all(equity=equity)
        second = mgr.check_all(equity=equity)

        assert len(first) == 1
        assert len(second) == 0  # suppressed as duplicate

    def test_different_alert_types_not_suppressed(self, tmp_path):
        cfg = AlertConfig(
            drawdown_threshold=0.05,
            accuracy_threshold=0.45,
            accuracy_window=10,
            log_path=tmp_path / "alerts.jsonl",
            dedup_window_seconds=300,
            max_alerts_per_hour=100,
        )
        mgr = AlertManager(cfg)

        # Drawdown alert
        equity = np.array([100_000, 80_000], dtype=float)
        a1 = mgr.check_all(equity=equity)

        # Accuracy alert (different type)
        preds = ["up"] * 10
        actuals = ["down"] * 10
        a2 = mgr.check_all(predictions=preds, actuals=actuals)

        assert len(a1) == 1
        assert len(a2) == 1
        assert a1[0].alert_type != a2[0].alert_type

    def test_dedup_expires(self, tmp_path):
        cfg = AlertConfig(
            drawdown_threshold=0.05,
            log_path=tmp_path / "alerts.jsonl",
            dedup_window_seconds=300,
            max_alerts_per_hour=100,
        )
        mgr = AlertManager(cfg)
        equity = np.array([100_000, 80_000], dtype=float)

        first = mgr.check_all(equity=equity)
        assert len(first) == 1

        # Simulate the first alert being old
        mgr._history[0].timestamp = datetime.now() - timedelta(seconds=600)

        second = mgr.check_all(equity=equity)
        assert len(second) == 1


# ---------------------------------------------------------------------------
# AlertManager -- history and acknowledgment
# ---------------------------------------------------------------------------


class TestHistoryAndAck:
    @pytest.mark.skipif(sys.platform == "win32", reason="Timing-sensitive test unstable on Windows CI")
    def test_get_recent(self, tmp_path):
        cfg = AlertConfig(
            drawdown_threshold=0.01,
            log_path=tmp_path / "alerts.jsonl",
            dedup_window_seconds=0,
            max_alerts_per_hour=100,
        )
        mgr = AlertManager(cfg)
        equity = np.array([100_000, 50_000], dtype=float)

        for _ in range(5):
            mgr.check_all(equity=equity)

        recent = mgr.get_recent(3)
        assert len(recent) == 3

    def test_get_recent_fewer_than_count(self, tmp_path):
        cfg = AlertConfig(
            drawdown_threshold=0.01,
            log_path=tmp_path / "alerts.jsonl",
            dedup_window_seconds=0,
        )
        mgr = AlertManager(cfg)
        equity = np.array([100_000, 50_000], dtype=float)
        mgr.check_all(equity=equity)

        recent = mgr.get_recent(50)
        assert len(recent) == 1

    def test_acknowledge(self, tmp_path):
        cfg = AlertConfig(
            drawdown_threshold=0.01,
            log_path=tmp_path / "alerts.jsonl",
            dedup_window_seconds=0,
        )
        mgr = AlertManager(cfg)
        equity = np.array([100_000, 50_000], dtype=float)
        mgr.check_all(equity=equity)

        assert mgr._history[0].acknowledged is False
        mgr.acknowledge(0)
        assert mgr._history[0].acknowledged is True

    def test_acknowledge_out_of_range(self, tmp_path):
        cfg = AlertConfig(log_path=tmp_path / "alerts.jsonl")
        mgr = AlertManager(cfg)
        # Should not raise
        mgr.acknowledge(999)
        mgr.acknowledge(-999)

    def test_repr(self, tmp_path):
        cfg = AlertConfig(
            drawdown_threshold=0.01,
            log_path=tmp_path / "alerts.jsonl",
            dedup_window_seconds=0,
        )
        mgr = AlertManager(cfg)
        equity = np.array([100_000, 50_000], dtype=float)
        mgr.check_all(equity=equity)

        r = repr(mgr)
        assert "total=1" in r
        assert "unacknowledged=1" in r

        mgr.acknowledge(0)
        r2 = repr(mgr)
        assert "unacknowledged=0" in r2


# ---------------------------------------------------------------------------
# AlertManager -- check_all integration
# ---------------------------------------------------------------------------


class TestCheckAllIntegration:
    def test_skips_checks_with_none_data(self, tmp_path):
        cfg = AlertConfig(log_path=tmp_path / "alerts.jsonl")
        mgr = AlertManager(cfg)
        # All None -- nothing should fire
        alerts = mgr.check_all()
        assert alerts == []

    def test_multiple_alerts_in_single_check(self, tmp_path):
        cfg = AlertConfig(
            drawdown_threshold=0.05,
            accuracy_threshold=0.45,
            accuracy_window=10,
            log_path=tmp_path / "alerts.jsonl",
            dedup_window_seconds=0,
            max_alerts_per_hour=100,
        )
        mgr = AlertManager(cfg)

        equity = np.array([100_000, 80_000], dtype=float)
        preds = ["up"] * 10
        actuals = ["down"] * 10

        alerts = mgr.check_all(
            equity=equity,
            predictions=preds,
            actuals=actuals,
        )

        types = {a.alert_type for a in alerts}
        assert AlertType.DRAWDOWN in types
        assert AlertType.ACCURACY_DROP in types
