"""
Regime Change Detection and Alert System.

Detects market regime changes and model performance shifts:

- **Trend reversals** -- moving average crossovers (golden/death cross)
- **Volatility spikes** -- realized vol exceeds historical baseline
- **Accuracy drops** -- rolling model accuracy below threshold
- **Weight shifts** -- ensemble weight redistribution
- **Drawdowns** -- equity peak-to-trough breach

Delivers alerts via:

- GUI toast notifications (via registered callbacks)
- Structured JSONL log file
- Optional webhook (POST to URL)
"""

from __future__ import annotations

import json
import logging
import urllib.request
from collections.abc import Callable
from dataclasses import dataclass, field
from datetime import datetime, timedelta
from enum import Enum
from pathlib import Path

import numpy as np

logger = logging.getLogger(__name__)


# ---------------------------------------------------------------------------
# Enums
# ---------------------------------------------------------------------------


class AlertLevel(Enum):
    """Severity of an alert."""

    INFO = "info"
    WARNING = "warning"
    CRITICAL = "critical"


class AlertType(Enum):
    """Category of regime change detected."""

    TREND_REVERSAL = "trend_reversal"
    VOLATILITY_SPIKE = "volatility_spike"
    ACCURACY_DROP = "accuracy_drop"
    CORRELATION_BREAK = "correlation_break"
    WEIGHT_SHIFT = "weight_shift"
    DRAWDOWN = "drawdown"


# ---------------------------------------------------------------------------
# Data classes
# ---------------------------------------------------------------------------


@dataclass
class Alert:
    """A single alert event."""

    timestamp: datetime
    alert_type: AlertType
    level: AlertLevel
    message: str
    details: dict = field(default_factory=dict)
    acknowledged: bool = False

    def to_dict(self) -> dict:
        """Serialize to a JSON-compatible dict."""
        return {
            "timestamp": self.timestamp.isoformat(),
            "alert_type": self.alert_type.value,
            "level": self.level.value,
            "message": self.message,
            "details": self.details,
            "acknowledged": self.acknowledged,
        }


@dataclass
class AlertConfig:
    """Alert system configuration.

    Attributes
    ----------
    volatility_threshold:
        Multiplier of historical vol that triggers a spike alert.
    accuracy_threshold:
        Rolling accuracy below this fires an accuracy drop alert.
    accuracy_window:
        Number of recent predictions for the rolling accuracy check.
    weight_shift_threshold:
        Max absolute weight change between checks that triggers alert.
    drawdown_threshold:
        Equity drawdown fraction that triggers alert (e.g. 0.10 = 10%).
    trend_fast_period:
        Fast moving average period for trend crossover detection.
    trend_slow_period:
        Slow moving average period for trend crossover detection.
    log_path:
        Path to the JSONL alert log file.
    webhook_url:
        Optional URL to POST alerts to.
    max_alerts_per_hour:
        Rate limit -- alerts beyond this count within one hour are dropped.
    dedup_window_seconds:
        Suppress duplicate alerts of the same type within this window.
    """

    volatility_threshold: float = 2.0
    accuracy_threshold: float = 0.45
    accuracy_window: int = 20
    weight_shift_threshold: float = 0.3
    drawdown_threshold: float = 0.10
    trend_fast_period: int = 10
    trend_slow_period: int = 30
    log_path: Path = Path.home() / ".build-engine" / "alerts.jsonl"
    webhook_url: str | None = None
    max_alerts_per_hour: int = 10
    dedup_window_seconds: int = 300  # 5 minutes


# ---------------------------------------------------------------------------
# Regime detector
# ---------------------------------------------------------------------------


class RegimeDetector:
    """Detects market regime changes from price/prediction data.

    Parameters
    ----------
    config:
        An :class:`AlertConfig` controlling detection thresholds.
    """

    def __init__(self, config: AlertConfig | None = None) -> None:
        self.config = config or AlertConfig()

    # -- trend ---------------------------------------------------------------

    def check_trend_reversal(self, prices: np.ndarray) -> Alert | None:
        """Detect MA crossover indicating trend change.

        Compares the fast and slow simple moving averages at the last
        two time steps.  A crossover (fast crosses above or below slow)
        is a trend reversal signal.

        Parameters
        ----------
        prices:
            1-D array of recent prices, oldest first.

        Returns
        -------
        Alert or None
        """
        fast = self.config.trend_fast_period
        slow = self.config.trend_slow_period

        if len(prices) < slow + 1:
            return None

        fast_ma = np.convolve(prices, np.ones(fast) / fast, mode="valid")
        slow_ma = np.convolve(prices, np.ones(slow) / slow, mode="valid")

        # Align: slow_ma is shorter; trim fast_ma to match
        offset = len(fast_ma) - len(slow_ma)
        fast_ma = fast_ma[offset:]

        if len(fast_ma) < 2:
            return None

        prev_diff = fast_ma[-2] - slow_ma[-2]
        curr_diff = fast_ma[-1] - slow_ma[-1]

        if prev_diff <= 0 < curr_diff:
            return Alert(
                timestamp=datetime.now(),
                alert_type=AlertType.TREND_REVERSAL,
                level=AlertLevel.WARNING,
                message="Bullish crossover: fast MA crossed above slow MA",
                details={
                    "fast_period": fast,
                    "slow_period": slow,
                    "fast_ma": float(fast_ma[-1]),
                    "slow_ma": float(slow_ma[-1]),
                    "direction": "bullish",
                },
            )

        if prev_diff >= 0 > curr_diff:
            return Alert(
                timestamp=datetime.now(),
                alert_type=AlertType.TREND_REVERSAL,
                level=AlertLevel.WARNING,
                message="Bearish crossover: fast MA crossed below slow MA",
                details={
                    "fast_period": fast,
                    "slow_period": slow,
                    "fast_ma": float(fast_ma[-1]),
                    "slow_ma": float(slow_ma[-1]),
                    "direction": "bearish",
                },
            )

        return None

    # -- volatility ----------------------------------------------------------

    def check_volatility_spike(self, returns: np.ndarray) -> Alert | None:
        """Detect sudden volatility expansion.

        Compares the 5-day realized volatility against the 30-day
        historical baseline, scaled by ``volatility_threshold``.

        Parameters
        ----------
        returns:
            1-D array of daily returns, oldest first.

        Returns
        -------
        Alert or None
        """
        if len(returns) < 30:
            return None

        recent_vol = float(np.std(returns[-5:]))
        historical_vol = float(np.std(returns[-30:]))

        if historical_vol == 0:
            return None

        ratio = recent_vol / historical_vol

        if ratio > self.config.volatility_threshold:
            level = AlertLevel.CRITICAL if ratio > self.config.volatility_threshold * 1.5 else AlertLevel.WARNING
            return Alert(
                timestamp=datetime.now(),
                alert_type=AlertType.VOLATILITY_SPIKE,
                level=level,
                message=f"Volatility spike: {ratio:.1f}x historical baseline",
                details={
                    "recent_vol": recent_vol,
                    "historical_vol": historical_vol,
                    "ratio": ratio,
                    "threshold": self.config.volatility_threshold,
                },
            )

        return None

    # -- accuracy ------------------------------------------------------------

    def check_accuracy_drop(
        self,
        predictions: list[str],
        actuals: list[str],
    ) -> Alert | None:
        """Detect model accuracy degradation.

        Computes hit rate over the most recent ``accuracy_window``
        prediction/actual pairs.

        Parameters
        ----------
        predictions:
            List of predicted directions (``"up"``, ``"down"``, ``"flat"``).
        actuals:
            List of actual directions (same encoding).

        Returns
        -------
        Alert or None
        """
        window = self.config.accuracy_window

        if len(predictions) < window or len(actuals) < window:
            return None

        recent_preds = predictions[-window:]
        recent_acts = actuals[-window:]

        hits = sum(1 for p, a in zip(recent_preds, recent_acts) if p == a)
        accuracy = hits / window

        if accuracy < self.config.accuracy_threshold:
            level = AlertLevel.CRITICAL if accuracy < self.config.accuracy_threshold * 0.5 else AlertLevel.WARNING
            return Alert(
                timestamp=datetime.now(),
                alert_type=AlertType.ACCURACY_DROP,
                level=level,
                message=(f"Model accuracy dropped to {accuracy:.1%} (threshold: {self.config.accuracy_threshold:.1%})"),
                details={
                    "accuracy": accuracy,
                    "threshold": self.config.accuracy_threshold,
                    "window": window,
                    "hits": hits,
                },
            )

        return None

    # -- weight shift --------------------------------------------------------

    def check_weight_shift(
        self,
        current_weights: dict[str, float],
        previous_weights: dict[str, float],
    ) -> Alert | None:
        """Detect sudden ensemble weight redistribution.

        Fires when any model's weight changes by more than
        ``weight_shift_threshold`` between consecutive checks.

        Parameters
        ----------
        current_weights:
            ``{model_name: weight}`` from the latest evaluation.
        previous_weights:
            ``{model_name: weight}`` from the prior evaluation.

        Returns
        -------
        Alert or None
        """
        if not current_weights or not previous_weights:
            return None

        all_models = set(current_weights) | set(previous_weights)
        max_shift = 0.0
        shifted_model = ""

        for model in all_models:
            curr = current_weights.get(model, 0.0)
            prev = previous_weights.get(model, 0.0)
            shift = abs(curr - prev)
            if shift > max_shift:
                max_shift = shift
                shifted_model = model

        if max_shift > self.config.weight_shift_threshold:
            return Alert(
                timestamp=datetime.now(),
                alert_type=AlertType.WEIGHT_SHIFT,
                level=AlertLevel.WARNING,
                message=(
                    f"Weight shift: '{shifted_model}' changed by "
                    f"{max_shift:.2f} (threshold: "
                    f"{self.config.weight_shift_threshold:.2f})"
                ),
                details={
                    "model": shifted_model,
                    "shift": max_shift,
                    "current": current_weights.get(shifted_model, 0.0),
                    "previous": previous_weights.get(shifted_model, 0.0),
                    "threshold": self.config.weight_shift_threshold,
                },
            )

        return None

    # -- drawdown ------------------------------------------------------------

    def check_drawdown(self, equity_curve: np.ndarray) -> Alert | None:
        """Detect significant drawdown from peak equity.

        Parameters
        ----------
        equity_curve:
            1-D array of equity values over time, oldest first.

        Returns
        -------
        Alert or None
        """
        if len(equity_curve) < 2:
            return None

        peak = float(np.maximum.accumulate(equity_curve).max())
        if peak <= 0:
            return None

        current = float(equity_curve[-1])
        drawdown = (peak - current) / peak

        if drawdown > self.config.drawdown_threshold:
            level = AlertLevel.CRITICAL if drawdown > self.config.drawdown_threshold * 2 else AlertLevel.WARNING
            return Alert(
                timestamp=datetime.now(),
                alert_type=AlertType.DRAWDOWN,
                level=level,
                message=f"Drawdown alert: {drawdown:.1%} from peak",
                details={
                    "drawdown": drawdown,
                    "peak": peak,
                    "current": current,
                    "threshold": self.config.drawdown_threshold,
                },
            )

        return None


# ---------------------------------------------------------------------------
# Alert manager
# ---------------------------------------------------------------------------


class AlertManager:
    """Manages alert delivery, deduplication, and rate limiting.

    Parameters
    ----------
    config:
        An :class:`AlertConfig` controlling thresholds and delivery.
    """

    def __init__(self, config: AlertConfig | None = None) -> None:
        self.config = config or AlertConfig()
        self._detector = RegimeDetector(self.config)
        self._history: list[Alert] = []
        self._callbacks: list[Callable[[Alert], None]] = []
        self._hourly_count: int = 0
        self._hour_start: datetime = datetime.now()

    # -- callback registration -----------------------------------------------

    def register_callback(self, callback: Callable[[Alert], None]) -> None:
        """Register a callback for alert delivery (e.g. GUI toast).

        Parameters
        ----------
        callback:
            A callable that accepts a single :class:`Alert` argument.
        """
        self._callbacks.append(callback)

    # -- bulk check ----------------------------------------------------------

    def check_all(
        self,
        prices: np.ndarray | None = None,
        returns: np.ndarray | None = None,
        predictions: list[str] | None = None,
        actuals: list[str] | None = None,
        weights: dict[str, float] | None = None,
        previous_weights: dict[str, float] | None = None,
        equity: np.ndarray | None = None,
    ) -> list[Alert]:
        """Run all regime checks and deliver any triggered alerts.

        Each parameter is optional; checks are skipped when their
        required data is ``None``.

        Returns
        -------
        list[Alert]
            Alerts that were actually delivered (after dedup + rate limit).
        """
        fired: list[Alert] = []

        checks: list[Alert | None] = []

        if prices is not None:
            checks.append(self._detector.check_trend_reversal(prices))
        if returns is not None:
            checks.append(self._detector.check_volatility_spike(returns))
        if predictions is not None and actuals is not None:
            checks.append(
                self._detector.check_accuracy_drop(predictions, actuals),
            )
        if weights is not None and previous_weights is not None:
            checks.append(
                self._detector.check_weight_shift(weights, previous_weights),
            )
        if equity is not None:
            checks.append(self._detector.check_drawdown(equity))

        for alert in checks:
            if alert is not None:
                if self._should_deliver(alert):
                    self._deliver(alert)
                    fired.append(alert)

        return fired

    # -- delivery pipeline ---------------------------------------------------

    def _should_deliver(self, alert: Alert) -> bool:
        """Check rate limit and deduplication before delivery."""
        now = datetime.now()

        # Reset hourly counter if the window has rolled over
        if (now - self._hour_start) > timedelta(hours=1):
            self._hourly_count = 0
            self._hour_start = now

        # Rate limit
        if self._hourly_count >= self.config.max_alerts_per_hour:
            logger.debug("Rate limit reached, dropping alert: %s", alert.message)
            return False

        # Dedup: suppress same alert type within dedup window
        cutoff = now - timedelta(seconds=self.config.dedup_window_seconds)
        for past in reversed(self._history):
            if past.timestamp < cutoff:
                break
            if past.alert_type == alert.alert_type:
                logger.debug(
                    "Duplicate suppressed for %s",
                    alert.alert_type.value,
                )
                return False

        return True

    def _deliver(self, alert: Alert) -> None:
        """Deliver alert via all channels (log, callbacks, webhook)."""
        self._history.append(alert)
        self._hourly_count += 1

        logger.info("ALERT [%s] %s: %s", alert.level.value, alert.alert_type.value, alert.message)

        self._log_alert(alert)

        for cb in self._callbacks:
            try:
                cb(alert)
            except Exception as exc:
                logger.error("Alert callback failed: %s", exc)

        if self.config.webhook_url:
            self._send_webhook(alert)

    def _log_alert(self, alert: Alert) -> None:
        """Append alert to JSONL log file."""
        try:
            self.config.log_path.parent.mkdir(parents=True, exist_ok=True)
            with open(self.config.log_path, "a", encoding="utf-8") as f:
                f.write(json.dumps(alert.to_dict(), separators=(",", ":")) + "\n")
        except OSError as exc:
            logger.error("Failed to write alert log: %s", exc)

    def _send_webhook(self, alert: Alert) -> None:
        """POST alert to webhook URL if configured."""
        if not self.config.webhook_url:
            return
        try:
            payload = json.dumps(alert.to_dict()).encode("utf-8")
            req = urllib.request.Request(
                self.config.webhook_url,
                data=payload,
                headers={"Content-Type": "application/json"},
                method="POST",
            )
            urllib.request.urlopen(req, timeout=5)
        except (ConnectionError, TimeoutError, OSError) as exc:
            logger.error("Webhook delivery failed: %s", exc)

    # -- history access ------------------------------------------------------

    def get_recent(self, count: int = 20) -> list[Alert]:
        """Get the most recent alerts.

        Parameters
        ----------
        count:
            Maximum number of alerts to return.

        Returns
        -------
        list[Alert]
            Most recent alerts, newest last.
        """
        return self._history[-count:]

    def acknowledge(self, index: int) -> None:
        """Mark an alert as acknowledged.

        Parameters
        ----------
        index:
            Index into the history list (0-based, oldest first).
        """
        if 0 <= index < len(self._history):
            self._history[index].acknowledged = True

    def __repr__(self) -> str:
        total = len(self._history)
        unack = sum(1 for a in self._history if not a.acknowledged)
        return f"AlertManager(total={total}, unacknowledged={unack})"
