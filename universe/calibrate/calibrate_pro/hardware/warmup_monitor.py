"""
Display Warm-Up Monitor

Professional displays need stabilization time before accurate measurement:
- OLED: ~30 minutes
- LED-backlit LCD: ~30-45 minutes
- CCFL-backlit LCD: ~90-120 minutes

This module monitors luminance stability and tells you when the display
is ready for calibration. Target: < 0.1% luminance change per minute
(per Portrait Displays recommendation).

Usage:
    monitor = WarmupMonitor(measure_fn)
    monitor.start()
    while not monitor.is_stable:
        status = monitor.get_status()
        print(f"{status.elapsed_min:.0f} min, drift: {status.drift_pct:.2f}%/min")
    print("Display stable. Ready for calibration.")
"""

import time
import logging
from dataclasses import dataclass, field
from typing import Callable, Optional, List
from collections import deque

logger = logging.getLogger(__name__)


@dataclass
class WarmupStatus:
    """Current warm-up status."""
    elapsed_seconds: float = 0.0
    elapsed_min: float = 0.0
    current_luminance: float = 0.0
    initial_luminance: float = 0.0
    drift_pct_per_min: float = 999.0
    is_stable: bool = False
    readings_count: int = 0
    stability_threshold: float = 0.1  # % per minute
    estimated_ready_min: float = 0.0

    @property
    def luminance_change_pct(self) -> float:
        """Total luminance change from initial reading."""
        if self.initial_luminance > 0:
            return abs(self.current_luminance - self.initial_luminance) / self.initial_luminance * 100
        return 0.0


@dataclass
class WarmupReading:
    """Single luminance reading during warm-up."""
    timestamp: float
    luminance: float


class WarmupMonitor:
    """
    Monitors display warm-up by periodically measuring luminance.

    Declares the display stable when luminance drift falls below
    the threshold (default 0.1% per minute) sustained for at least
    3 consecutive readings.
    """

    def __init__(
        self,
        measure_luminance_fn: Callable[[], Optional[float]],
        interval_seconds: float = 30.0,
        stability_threshold_pct: float = 0.1,
        stability_count: int = 3,
        display_white_fn: Optional[Callable[[], None]] = None,
    ):
        """
        Args:
            measure_luminance_fn: Returns luminance in cd/m2, or None on failure
            interval_seconds: Seconds between readings (default 30)
            stability_threshold_pct: Max % drift per minute for stability (default 0.1)
            stability_count: Consecutive stable readings needed (default 3)
            display_white_fn: Optional function to display white patch before measuring
        """
        self._measure = measure_luminance_fn
        self._display_white = display_white_fn
        self._interval = interval_seconds
        self._threshold = stability_threshold_pct
        self._stability_count = stability_count
        self._readings: List[WarmupReading] = []
        self._start_time: float = 0.0
        self._stable_streak: int = 0
        self._is_stable: bool = False

    @property
    def is_stable(self) -> bool:
        return self._is_stable

    def take_reading(self) -> Optional[WarmupStatus]:
        """Take a single luminance reading and return updated status."""
        if not self._start_time:
            self._start_time = time.time()

        # Display white if function provided
        if self._display_white:
            self._display_white()

        lum = self._measure()
        if lum is None or lum <= 0:
            return None

        now = time.time()
        self._readings.append(WarmupReading(now, lum))

        return self.get_status()

    def get_status(self) -> WarmupStatus:
        """Calculate current warm-up status from readings."""
        if not self._readings:
            return WarmupStatus()

        elapsed = time.time() - self._start_time if self._start_time else 0
        current = self._readings[-1].luminance
        initial = self._readings[0].luminance

        # Calculate drift rate over the last 3 readings
        drift = 999.0
        if len(self._readings) >= 2:
            recent = self._readings[-min(4, len(self._readings)):]
            dt_min = (recent[-1].timestamp - recent[0].timestamp) / 60.0
            if dt_min > 0:
                dlum = abs(recent[-1].luminance - recent[0].luminance)
                avg_lum = sum(r.luminance for r in recent) / len(recent)
                if avg_lum > 0:
                    drift = (dlum / avg_lum) * 100 / dt_min

        # Check stability
        if drift < self._threshold:
            self._stable_streak += 1
        else:
            self._stable_streak = 0

        self._is_stable = self._stable_streak >= self._stability_count

        # Estimate time to ready (crude exponential decay model)
        est_ready = 0.0
        if not self._is_stable and drift > self._threshold and elapsed > 60:
            # Rough estimate: assume drift decays exponentially
            if drift > 0:
                est_ready = elapsed / 60.0 * (drift / self._threshold - 1)
                est_ready = min(est_ready, 120.0)  # Cap at 2 hours

        return WarmupStatus(
            elapsed_seconds=elapsed,
            elapsed_min=elapsed / 60.0,
            current_luminance=current,
            initial_luminance=initial,
            drift_pct_per_min=drift,
            is_stable=self._is_stable,
            readings_count=len(self._readings),
            stability_threshold=self._threshold,
            estimated_ready_min=est_ready,
        )

    def reset(self):
        """Reset the monitor for a new warm-up session."""
        self._readings.clear()
        self._start_time = 0.0
        self._stable_streak = 0
        self._is_stable = False


# Recommended warm-up times by technology
WARMUP_ESTIMATES = {
    "QD-OLED": 30,     # minutes
    "WOLED": 30,
    "OLED": 30,
    "IPS": 30,
    "VA": 30,
    "Mini-LED": 45,
    "LED": 45,
    "CCFL": 90,
    "CRT": 60,
}


def get_recommended_warmup(panel_type: str) -> int:
    """Get recommended warm-up time in minutes for a panel technology."""
    for key, minutes in WARMUP_ESTIMATES.items():
        if key.lower() in panel_type.lower():
            return minutes
    return 45  # Default
