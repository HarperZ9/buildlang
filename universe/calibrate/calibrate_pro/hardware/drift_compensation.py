"""
Measurement Drift Compensation

During long profiling sessions (hundreds of patches), both the sensor
and the display can drift. This module inserts reference patches at
regular intervals and compensates for measured drift.

Professional tools (ColourSpace, Calman) do this automatically.
The technique:
1. Measure a reference white patch at the start
2. Every N patches, re-measure the reference
3. Compute drift per channel (ratio of current / initial reference)
4. Apply inverse drift to all patches since the last reference

This is critical for:
- OLED displays (ABL causes luminance drift with changing APL)
- Long volumetric profiling sessions (700+ patches)
- Displays that haven't fully warmed up
"""

import time
import logging
import numpy as np
from dataclasses import dataclass, field
from typing import List, Optional, Tuple

logger = logging.getLogger(__name__)


@dataclass
class DriftReading:
    """A reference patch measurement for drift tracking."""
    timestamp: float
    xyz: np.ndarray
    patch_index: int  # Index in the overall measurement sequence


@dataclass
class DriftStats:
    """Drift statistics for a profiling session."""
    total_readings: int = 0
    reference_count: int = 0
    max_drift_pct: float = 0.0
    avg_drift_pct: float = 0.0
    per_channel_drift: Tuple[float, float, float] = (0.0, 0.0, 0.0)
    compensated_patches: int = 0


class DriftCompensator:
    """
    Compensates for sensor/display drift during measurement sessions.

    Insert reference measurements at regular intervals. The compensator
    interpolates drift between references and corrects all intermediate
    measurements.
    """

    def __init__(
        self,
        reference_interval: int = 25,
        reference_color: Tuple[float, float, float] = (1.0, 1.0, 1.0),
    ):
        """
        Args:
            reference_interval: Insert reference every N patches
            reference_color: RGB color of the reference patch (default: white)
        """
        self.interval = reference_interval
        self.reference_color = reference_color
        self._references: List[DriftReading] = []
        self._initial_xyz: Optional[np.ndarray] = None
        self._patch_count: int = 0
        self._compensated_count: int = 0

    def set_initial_reference(self, xyz: np.ndarray):
        """Set the initial reference measurement (first white)."""
        self._initial_xyz = xyz.copy()
        self._references.append(DriftReading(
            timestamp=time.time(),
            xyz=xyz.copy(),
            patch_index=0,
        ))
        logger.info("Drift compensation: initial reference Y=%.2f", xyz[1])

    def should_measure_reference(self, patch_index: int) -> bool:
        """Check if a reference measurement is needed at this patch index."""
        return patch_index > 0 and patch_index % self.interval == 0

    def add_reference(self, xyz: np.ndarray, patch_index: int):
        """Add a reference measurement during the session."""
        self._references.append(DriftReading(
            timestamp=time.time(),
            xyz=xyz.copy(),
            patch_index=patch_index,
        ))

        if self._initial_xyz is not None:
            drift = np.abs(xyz - self._initial_xyz) / np.maximum(self._initial_xyz, 1e-6) * 100
            logger.debug(
                "Drift at patch %d: X=%.2f%% Y=%.2f%% Z=%.2f%%",
                patch_index, drift[0], drift[1], drift[2]
            )

    def compensate(self, xyz: np.ndarray, patch_index: int) -> np.ndarray:
        """
        Compensate a measurement for drift.

        Interpolates between the two nearest reference measurements
        and applies an inverse drift correction.
        """
        if self._initial_xyz is None or len(self._references) < 2:
            return xyz

        # Find the two references bracketing this patch index
        before = self._references[0]
        after = self._references[-1]

        for i in range(len(self._references) - 1):
            if self._references[i].patch_index <= patch_index <= self._references[i + 1].patch_index:
                before = self._references[i]
                after = self._references[i + 1]
                break

        # Interpolation factor
        span = after.patch_index - before.patch_index
        if span > 0:
            t = (patch_index - before.patch_index) / span
        else:
            t = 0.0

        # Interpolated reference at this patch index
        interp_ref = before.xyz * (1 - t) + after.xyz * t

        # Drift ratio: how much the reference has shifted from initial
        drift_ratio = np.where(
            interp_ref > 1e-6,
            self._initial_xyz / interp_ref,
            1.0
        )

        # Apply inverse drift
        compensated = xyz * drift_ratio
        self._compensated_count += 1

        return compensated

    def get_stats(self) -> DriftStats:
        """Get drift statistics for the session."""
        if not self._references or self._initial_xyz is None:
            return DriftStats()

        drifts = []
        for ref in self._references[1:]:
            d = np.abs(ref.xyz - self._initial_xyz) / np.maximum(self._initial_xyz, 1e-6) * 100
            drifts.append(d)

        if drifts:
            drifts_arr = np.array(drifts)
            max_drift = float(np.max(drifts_arr))
            avg_drift = float(np.mean(drifts_arr))
            latest = drifts_arr[-1]
            per_ch = (float(latest[0]), float(latest[1]), float(latest[2]))
        else:
            max_drift = 0.0
            avg_drift = 0.0
            per_ch = (0.0, 0.0, 0.0)

        return DriftStats(
            total_readings=self._patch_count,
            reference_count=len(self._references),
            max_drift_pct=max_drift,
            avg_drift_pct=avg_drift,
            per_channel_drift=per_ch,
            compensated_patches=self._compensated_count,
        )
