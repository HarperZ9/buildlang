"""
EOTF Calibration and Verification

Comprehensive HDR EOTF calibration for:
- PQ (ST.2084) tracking
- HLG tracking
- Near-black calibration
- Grayscale balance in HDR
- Tone mapping verification
"""

import numpy as np
from dataclasses import dataclass, field
from typing import List, Optional, Tuple, Dict, Any, Callable
from enum import Enum


# =============================================================================
# EOTF Types and Constants
# =============================================================================

class EOTFType(Enum):
    """Supported EOTF types."""
    PQ = "pq"           # SMPTE ST.2084
    HLG = "hlg"         # ARIB STD-B67
    GAMMA = "gamma"     # Power law
    SRGB = "srgb"       # sRGB/BT.1886
    LINEAR = "linear"


class CalibrationStandard(Enum):
    """Calibration standards."""
    ITU_R_BT2100 = "bt2100"        # ITU-R BT.2100 HDR
    DOLBY_VISION = "dolby"         # Dolby Vision
    HDR10 = "hdr10"                # HDR10 / HDR10+
    BROADCAST_HLG = "hlg_broadcast"


# Reference levels for different standards
REFERENCE_LEVELS = {
    "pq_sdr_white": 203,      # SDR reference white in nits (for PQ)
    "hlg_sdr_white": 100,     # SDR reference for HLG
    "pq_reference": 100,      # PQ reference white
}


# =============================================================================
# Calibration Patch Generation
# =============================================================================

@dataclass
class EOTFPatch:
    """Single EOTF calibration patch."""
    signal_level: float       # Input signal [0, 1]
    target_luminance: float   # Expected output (cd/m²)
    label: str               # Description
    is_near_black: bool = False
    is_critical: bool = False


def generate_pq_patches(
    num_patches: int = 21,
    include_near_black: bool = True,
    include_extended: bool = True,
    max_luminance: float = 10000.0
) -> List[EOTFPatch]:
    """
    Generate PQ EOTF verification patches.

    Args:
        num_patches: Number of grayscale patches
        include_near_black: Add extra near-black patches
        include_extended: Add patches above 1000 nits
        max_luminance: Maximum luminance to include

    Returns:
        List of EOTFPatch objects
    """
    from calibrate_pro.hdr.pq_st2084 import pq_eotf, pq_oetf

    patches = []

    # Near-black patches (critical for HDR)
    if include_near_black:
        near_black_nits = [0.005, 0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1.0]
        for nits in near_black_nits:
            signal = float(pq_oetf(np.array([nits]))[0])
            patches.append(EOTFPatch(
                signal_level=signal,
                target_luminance=nits,
                label=f"{nits:.3f} nits",
                is_near_black=True,
                is_critical=nits <= 0.05
            ))

    # Standard grayscale ramp
    standard_nits = [2, 5, 10, 20, 50, 100, 200, 400, 600, 800, 1000]
    for nits in standard_nits:
        if nits <= max_luminance:
            signal = float(pq_oetf(np.array([nits]))[0])
            patches.append(EOTFPatch(
                signal_level=signal,
                target_luminance=nits,
                label=f"{nits} nits",
                is_critical=(nits == 100 or nits == 1000)  # Reference points
            ))

    # Extended range
    if include_extended:
        extended_nits = [1500, 2000, 3000, 4000, 6000, 8000, 10000]
        for nits in extended_nits:
            if nits <= max_luminance:
                signal = float(pq_oetf(np.array([nits]))[0])
                patches.append(EOTFPatch(
                    signal_level=signal,
                    target_luminance=nits,
                    label=f"{nits} nits (extended)"
                ))

    # Sort by signal level
    patches.sort(key=lambda p: p.signal_level)

    return patches


def generate_hlg_patches(
    num_patches: int = 21,
    system_gamma: float = 1.2,
    peak_luminance: float = 1000.0
) -> List[EOTFPatch]:
    """
    Generate HLG EOTF verification patches.

    Args:
        num_patches: Number of patches
        system_gamma: HLG system gamma
        peak_luminance: Display peak luminance

    Returns:
        List of EOTFPatch objects
    """
    from calibrate_pro.hdr.hlg import hlg_eotf

    patches = []
    signals = np.linspace(0, 1, num_patches)

    for sig in signals:
        # Calculate expected luminance
        display_normalized = hlg_eotf(np.array([sig]), system_gamma)[0]
        luminance = display_normalized * peak_luminance

        patches.append(EOTFPatch(
            signal_level=float(sig),
            target_luminance=float(luminance),
            label=f"HLG {sig*100:.0f}%",
            is_near_black=(sig < 0.1)
        ))

    return patches


# =============================================================================
# Measurement and Analysis
# =============================================================================

@dataclass
class EOTFMeasurement:
    """Single EOTF measurement result."""
    signal_level: float
    target_luminance: float
    measured_luminance: float
    error_percent: float
    error_nits: float
    delta_e: Optional[float] = None


@dataclass
class EOTFAnalysis:
    """Complete EOTF analysis results."""
    eotf_type: EOTFType
    measurements: List[EOTFMeasurement]

    # Overall metrics
    average_error: float
    max_error: float
    near_black_error: float
    mid_tone_error: float
    highlight_error: float

    # Display characteristics
    measured_peak: float
    measured_black: float
    contrast_ratio: float
    dynamic_range_stops: float

    # Tracking quality
    gamma_tracking: float  # How well it follows target curve
    rgb_balance: Optional[float] = None

    # Grade
    grade: str = "Unknown"
    pass_fail: bool = True

    def to_dict(self) -> Dict[str, Any]:
        """Convert to dictionary."""
        return {
            "eotf_type": self.eotf_type.value,
            "average_error_percent": self.average_error,
            "max_error_percent": self.max_error,
            "near_black_error_percent": self.near_black_error,
            "measured_peak_nits": self.measured_peak,
            "measured_black_nits": self.measured_black,
            "contrast_ratio": self.contrast_ratio,
            "dynamic_range_stops": self.dynamic_range_stops,
            "grade": self.grade,
            "pass": self.pass_fail,
            "measurements": [
                {
                    "signal": m.signal_level,
                    "target_nits": m.target_luminance,
                    "measured_nits": m.measured_luminance,
                    "error_percent": m.error_percent
                }
                for m in self.measurements
            ]
        }


def analyze_pq_eotf(
    signal_levels: np.ndarray,
    measured_luminance: np.ndarray,
    reference_white: float = 100.0
) -> EOTFAnalysis:
    """
    Analyze PQ EOTF tracking.

    Args:
        signal_levels: Input PQ signal levels [0, 1]
        measured_luminance: Measured display luminance (cd/m²)
        reference_white: SDR reference white level

    Returns:
        EOTFAnalysis with detailed results
    """
    from calibrate_pro.hdr.pq_st2084 import pq_eotf

    # Calculate target luminance
    target_luminance = pq_eotf(signal_levels)

    # Create measurements
    measurements = []
    for i in range(len(signal_levels)):
        target = target_luminance[i]
        measured = measured_luminance[i]

        if target > 0:
            error_pct = abs(measured - target) / target * 100
        else:
            error_pct = 0.0

        error_nits = abs(measured - target)

        measurements.append(EOTFMeasurement(
            signal_level=float(signal_levels[i]),
            target_luminance=float(target),
            measured_luminance=float(measured),
            error_percent=float(error_pct),
            error_nits=float(error_nits)
        ))

    # Calculate overall metrics
    errors = np.array([m.error_percent for m in measurements])

    # Split by luminance region
    near_black_mask = signal_levels < 0.1
    mid_tone_mask = (signal_levels >= 0.1) & (signal_levels < 0.5)
    highlight_mask = signal_levels >= 0.5

    near_black_err = float(np.mean(errors[near_black_mask])) if np.any(near_black_mask) else 0.0
    mid_tone_err = float(np.mean(errors[mid_tone_mask])) if np.any(mid_tone_mask) else 0.0
    highlight_err = float(np.mean(errors[highlight_mask])) if np.any(highlight_mask) else 0.0

    # Display characteristics
    peak = float(np.max(measured_luminance))
    positive_lum = measured_luminance[measured_luminance > 0]
    black = float(np.min(positive_lum)) if len(positive_lum) > 0 else 0.0001

    contrast = peak / max(black, 0.0001)
    dr_stops = np.log2(contrast)

    # Gamma tracking (correlation with target)
    valid_mask = target_luminance > 0
    if np.sum(valid_mask) > 2:
        log_target = np.log10(target_luminance[valid_mask] + 0.0001)
        log_measured = np.log10(measured_luminance[valid_mask] + 0.0001)
        correlation = np.corrcoef(log_target, log_measured)[0, 1]
        gamma_tracking = float(correlation * 100)
    else:
        gamma_tracking = 0.0

    # Determine grade
    avg_err = float(np.mean(errors))
    max_err = float(np.max(errors))

    if avg_err < 2.0 and near_black_err < 5.0 and peak >= 1000:
        grade = "Reference HDR"
        pass_fail = True
    elif avg_err < 5.0 and near_black_err < 10.0 and peak >= 600:
        grade = "Professional HDR"
        pass_fail = True
    elif avg_err < 10.0 and peak >= 400:
        grade = "Good HDR"
        pass_fail = True
    elif avg_err < 15.0 and peak >= 300:
        grade = "Basic HDR"
        pass_fail = True
    else:
        grade = "Below Standard"
        pass_fail = False

    return EOTFAnalysis(
        eotf_type=EOTFType.PQ,
        measurements=measurements,
        average_error=avg_err,
        max_error=max_err,
        near_black_error=near_black_err,
        mid_tone_error=mid_tone_err,
        highlight_error=highlight_err,
        measured_peak=peak,
        measured_black=black,
        contrast_ratio=contrast,
        dynamic_range_stops=float(dr_stops),
        gamma_tracking=gamma_tracking,
        grade=grade,
        pass_fail=pass_fail
    )


def analyze_hlg_eotf(
    signal_levels: np.ndarray,
    measured_luminance: np.ndarray,
    system_gamma: float = 1.2,
    peak_luminance: float = 1000.0
) -> EOTFAnalysis:
    """
    Analyze HLG EOTF tracking.

    Args:
        signal_levels: Input HLG signal levels [0, 1]
        measured_luminance: Measured display luminance (cd/m²)
        system_gamma: Expected system gamma
        peak_luminance: Expected peak luminance

    Returns:
        EOTFAnalysis with detailed results
    """
    from calibrate_pro.hdr.hlg import hlg_eotf

    # Calculate target
    target_normalized = hlg_eotf(signal_levels, system_gamma)
    target_luminance = target_normalized * peak_luminance

    # Create measurements
    measurements = []
    for i in range(len(signal_levels)):
        target = target_luminance[i]
        measured = measured_luminance[i]

        if target > 0:
            error_pct = abs(measured - target) / target * 100
        else:
            error_pct = 0.0

        measurements.append(EOTFMeasurement(
            signal_level=float(signal_levels[i]),
            target_luminance=float(target),
            measured_luminance=float(measured),
            error_percent=float(error_pct),
            error_nits=float(abs(measured - target))
        ))

    # Metrics
    errors = np.array([m.error_percent for m in measurements])
    avg_err = float(np.mean(errors))
    max_err = float(np.max(errors))

    peak = float(np.max(measured_luminance))
    positive_lum = measured_luminance[measured_luminance > 0]
    black = float(np.min(positive_lum)) if len(positive_lum) > 0 else 0.0001

    contrast = peak / max(black, 0.0001)
    dr_stops = np.log2(contrast)

    # HLG grading
    if avg_err < 3.0 and peak >= 800:
        grade = "Reference HLG"
    elif avg_err < 7.0 and peak >= 500:
        grade = "Broadcast HLG"
    elif avg_err < 12.0:
        grade = "Acceptable HLG"
    else:
        grade = "Below Standard"

    return EOTFAnalysis(
        eotf_type=EOTFType.HLG,
        measurements=measurements,
        average_error=avg_err,
        max_error=max_err,
        near_black_error=float(np.mean(errors[:5])) if len(errors) >= 5 else avg_err,
        mid_tone_error=avg_err,
        highlight_error=float(np.mean(errors[-5:])) if len(errors) >= 5 else avg_err,
        measured_peak=peak,
        measured_black=black,
        contrast_ratio=contrast,
        dynamic_range_stops=float(dr_stops),
        gamma_tracking=100.0 - avg_err,
        grade=grade,
        pass_fail=(avg_err < 12.0)
    )


# =============================================================================
# Calibration LUT Generation
# =============================================================================

def generate_eotf_correction_lut(
    analysis: EOTFAnalysis,
    size: int = 1024,
    smooth: bool = True
) -> np.ndarray:
    """
    Generate EOTF correction LUT from analysis.

    Args:
        analysis: EOTF analysis results
        size: Output LUT size
        smooth: Apply smoothing

    Returns:
        1D LUT for EOTF correction
    """
    # Extract measured data
    signals = np.array([m.signal_level for m in analysis.measurements])
    targets = np.array([m.target_luminance for m in analysis.measurements])
    measured = np.array([m.measured_luminance for m in analysis.measurements])

    # Create correction curve
    # We want: correction(signal) -> corrected_signal
    # Such that display(corrected_signal) ≈ target

    # Calculate inverse relationship
    # If display shows 'measured' for 'signal', and we want 'target',
    # we need to find what signal produces 'target'

    # Build interpolation from measured luminance to signal
    # Then query at target luminance to get corrected signal
    sort_idx = np.argsort(measured)
    measured_sorted = measured[sort_idx]
    signals_sorted = signals[sort_idx]

    # Generate output LUT
    output_signals = np.linspace(0, 1, size)

    # For each desired signal level, find what the display actually produces
    # and calculate the correction

    if analysis.eotf_type == EOTFType.PQ:
        from calibrate_pro.hdr.pq_st2084 import pq_eotf, pq_oetf

        # Target luminance for each output level
        target_lum = pq_eotf(output_signals)

        # Interpolate to find what input signal gives this luminance
        # Clip to valid range
        target_lum_clipped = np.clip(target_lum, measured_sorted[0], measured_sorted[-1])
        corrected = np.interp(target_lum_clipped, measured_sorted, signals_sorted)

    else:
        # Linear correction
        corrected = output_signals.copy()

        # Simple ratio correction
        for i, sig in enumerate(output_signals):
            idx = np.argmin(np.abs(signals - sig))
            if targets[idx] > 0 and measured[idx] > 0:
                ratio = targets[idx] / measured[idx]
                corrected[i] = np.clip(sig * ratio, 0, 1)

    # Smooth if requested
    if smooth:
        from scipy.ndimage import gaussian_filter1d
        corrected = gaussian_filter1d(corrected, sigma=size/100)

    return np.clip(corrected, 0, 1)


def generate_grayscale_correction_matrix(
    rgb_measurements: np.ndarray,
    target_white: Tuple[float, float] = (0.3127, 0.3290)
) -> np.ndarray:
    """
    Generate RGB correction matrix for grayscale balance.

    Args:
        rgb_measurements: RGB luminance measurements (N, 3)
        target_white: Target white point (x, y)

    Returns:
        3x3 correction matrix
    """
    # Calculate average RGB ratios
    avg_rgb = np.mean(rgb_measurements, axis=0)

    # Normalize to green (reference)
    if avg_rgb[1] > 0:
        ratios = avg_rgb[1] / avg_rgb
        ratios = np.clip(ratios, 0.5, 2.0)
    else:
        ratios = np.ones(3)

    # Create diagonal correction matrix
    correction = np.diag(ratios)

    return correction


# =============================================================================
# Calibration Workflow
# =============================================================================

@dataclass
class CalibrationTarget:
    """HDR calibration target settings."""
    eotf_type: EOTFType = EOTFType.PQ
    peak_luminance: float = 1000.0
    black_level: float = 0.005
    white_point: Tuple[float, float] = (0.3127, 0.3290)  # D65
    color_space: str = "BT.2020"
    system_gamma: float = 1.2  # For HLG


@dataclass
class CalibrationResult:
    """Complete HDR calibration result."""
    target: CalibrationTarget
    eotf_analysis: EOTFAnalysis
    correction_lut: np.ndarray
    rgb_matrix: Optional[np.ndarray] = None

    # Verification
    pre_calibration_error: float = 0.0
    post_calibration_error: float = 0.0
    improvement: float = 0.0

    def to_dict(self) -> Dict[str, Any]:
        """Convert to dictionary."""
        return {
            "target": {
                "eotf": self.target.eotf_type.value,
                "peak_nits": self.target.peak_luminance,
                "black_nits": self.target.black_level,
                "white_point": self.target.white_point,
                "color_space": self.target.color_space
            },
            "analysis": self.eotf_analysis.to_dict(),
            "pre_error_percent": self.pre_calibration_error,
            "post_error_percent": self.post_calibration_error,
            "improvement_percent": self.improvement
        }


class EOTFCalibrator:
    """
    HDR EOTF calibration workflow manager.
    """

    def __init__(
        self,
        target: CalibrationTarget,
        measure_func: Optional[Callable[[float], float]] = None
    ):
        """
        Initialize calibrator.

        Args:
            target: Calibration target settings
            measure_func: Function to measure luminance at signal level
        """
        self.target = target
        self.measure_func = measure_func
        self._patches: List[EOTFPatch] = []
        self._measurements: List[Tuple[float, float]] = []

    def generate_patches(self) -> List[EOTFPatch]:
        """Generate calibration patches for target EOTF."""
        if self.target.eotf_type == EOTFType.PQ:
            self._patches = generate_pq_patches(
                include_extended=(self.target.peak_luminance > 1000)
            )
        elif self.target.eotf_type == EOTFType.HLG:
            self._patches = generate_hlg_patches(
                system_gamma=self.target.system_gamma,
                peak_luminance=self.target.peak_luminance
            )
        else:
            # Gamma patches
            signals = np.linspace(0, 1, 21)
            self._patches = [
                EOTFPatch(
                    signal_level=float(s),
                    target_luminance=float(s ** 2.2 * self.target.peak_luminance),
                    label=f"{s*100:.0f}%"
                )
                for s in signals
            ]

        return self._patches

    def add_measurement(self, signal: float, luminance: float):
        """Add a measurement."""
        self._measurements.append((signal, luminance))

    def analyze(self) -> EOTFAnalysis:
        """Analyze collected measurements."""
        if not self._measurements:
            raise ValueError("No measurements collected")

        signals = np.array([m[0] for m in self._measurements])
        luminance = np.array([m[1] for m in self._measurements])

        if self.target.eotf_type == EOTFType.PQ:
            return analyze_pq_eotf(signals, luminance)
        elif self.target.eotf_type == EOTFType.HLG:
            return analyze_hlg_eotf(
                signals, luminance,
                self.target.system_gamma,
                self.target.peak_luminance
            )
        else:
            # Generic analysis
            return analyze_pq_eotf(signals, luminance)

    def generate_correction(
        self,
        analysis: Optional[EOTFAnalysis] = None
    ) -> CalibrationResult:
        """Generate calibration correction."""
        if analysis is None:
            analysis = self.analyze()

        # Generate correction LUT
        lut = generate_eotf_correction_lut(analysis)

        return CalibrationResult(
            target=self.target,
            eotf_analysis=analysis,
            correction_lut=lut,
            pre_calibration_error=analysis.average_error
        )

    def clear_measurements(self):
        """Clear all measurements."""
        self._measurements.clear()
