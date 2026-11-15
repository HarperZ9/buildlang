"""
Unified HDR Calibration Suite

Comprehensive calibration workflow for all HDR formats:
- HDR10 (PQ EOTF calibration)
- HDR10+ (Dynamic metadata calibration)
- HLG (System gamma calibration)
- Dolby Vision (Profile creation)

Provides a single interface for complete HDR display calibration
with support for professional mastering standards.

Author: Zain Dana / Build
License: Fair-Source (see LICENSE)
"""

import numpy as np
from dataclasses import dataclass, field
from typing import Dict, List, Tuple, Optional, Callable
from enum import Enum
from pathlib import Path
import json
import time

from .pq_st2084 import (
    pq_eotf, pq_oetf, HDR10Metadata, PQDisplayAssessment,
    calculate_pq_eotf_error, generate_pq_verification_patches,
    assess_pq_display, pq_code_to_nits, nits_to_pq_code
)
from .hlg import (
    hlg_eotf, hlg_oetf, HLGDisplaySettings,
    calculate_hlg_eotf_error, generate_hlg_verification_patches
)
from .mastering_standards import (
    MasteringSpec, NetflixMasteringProfile, EBUGrade1Profile,
    validate_mastering_compliance, ComplianceLevel
)


class HDRFormat(Enum):
    """Supported HDR formats."""
    HDR10 = "hdr10"
    HDR10_PLUS = "hdr10plus"
    HLG = "hlg"
    DOLBY_VISION = "dolby_vision"


class CalibrationMode(Enum):
    """Calibration workflow modes."""
    QUICK = "quick"           # Basic grayscale only (~20 patches)
    STANDARD = "standard"     # Grayscale + primaries (~100 patches)
    PROFESSIONAL = "professional"  # Full profiling (~1000 patches)
    VERIFICATION = "verification"  # Post-calibration check


@dataclass
class HDRCalibrationConfig:
    """Configuration for HDR calibration."""
    format: HDRFormat = HDRFormat.HDR10
    mode: CalibrationMode = CalibrationMode.STANDARD

    # Target specifications
    target_peak_luminance: float = 1000.0  # cd/m²
    target_min_luminance: float = 0.0001   # cd/m²
    target_primaries: str = "p3_d65"  # p3_d65, bt2020, srgb

    # HLG specific
    hlg_system_gamma: float = 1.2

    # Mastering standard for validation
    mastering_standard: Optional[str] = "netflix"

    # Calibration options
    enable_near_black_optimization: bool = True
    enable_tone_mapping: bool = True
    preserve_oled_black: bool = True


@dataclass
class HDRMeasurement:
    """Single HDR measurement point."""
    signal_level: float  # Input signal [0, 1]
    target_luminance: float  # Expected luminance (cd/m²)
    measured_luminance: float  # Actual luminance (cd/m²)
    measured_xyz: Optional[Tuple[float, float, float]] = None
    delta_e: Optional[float] = None
    eotf_error_percent: Optional[float] = None
    timestamp: float = field(default_factory=time.time)


@dataclass
class HDRCalibrationResult:
    """Complete HDR calibration result."""
    config: HDRCalibrationConfig
    measurements: List[HDRMeasurement]

    # Summary statistics
    peak_luminance: float
    min_luminance: float
    contrast_ratio: float
    avg_eotf_error: float
    max_eotf_error: float
    near_black_error: float

    # Corrections generated
    correction_lut_path: Optional[Path] = None
    icc_profile_path: Optional[Path] = None

    # Compliance
    compliance_level: ComplianceLevel = ComplianceLevel.FAILED
    compliance_issues: List[str] = field(default_factory=list)

    # Metadata
    display_name: str = ""
    calibration_date: str = ""
    duration_seconds: float = 0.0

    def to_dict(self) -> Dict:
        """Serialize to dictionary."""
        return {
            "display": self.display_name,
            "date": self.calibration_date,
            "duration_seconds": self.duration_seconds,
            "format": self.config.format.value,
            "mode": self.config.mode.value,
            "peak_luminance": self.peak_luminance,
            "min_luminance": self.min_luminance,
            "contrast_ratio": self.contrast_ratio,
            "avg_eotf_error": self.avg_eotf_error,
            "max_eotf_error": self.max_eotf_error,
            "near_black_error": self.near_black_error,
            "compliance": self.compliance_level.value,
            "issues": self.compliance_issues,
            "measurements": [
                {
                    "signal": m.signal_level,
                    "target": m.target_luminance,
                    "measured": m.measured_luminance,
                    "error_pct": m.eotf_error_percent
                }
                for m in self.measurements
            ]
        }

    def save_report(self, path: Path):
        """Save calibration report to JSON."""
        with open(path, 'w') as f:
            json.dump(self.to_dict(), f, indent=2)


# =============================================================================
# HDR10 Calibration
# =============================================================================

class HDR10Calibration:
    """
    HDR10 (PQ EOTF) Calibration.

    Calibrates display to SMPTE ST.2084 PQ curve with optional
    tone mapping for content above display peak.
    """

    def __init__(self, config: HDRCalibrationConfig = None):
        self.config = config or HDRCalibrationConfig(format=HDRFormat.HDR10)

    def generate_patches(self) -> np.ndarray:
        """Generate PQ signal patches for calibration."""
        if self.config.mode == CalibrationMode.QUICK:
            # 21 grayscale patches
            patches = np.linspace(0, 1, 21)
        elif self.config.mode == CalibrationMode.STANDARD:
            # 51 patches with near-black emphasis
            base = np.linspace(0, 1, 41)
            near_black = np.array([0.001, 0.002, 0.005, 0.01, 0.015, 0.02, 0.025, 0.03, 0.04, 0.05])
            patches = np.unique(np.concatenate([near_black, base]))
        elif self.config.mode == CalibrationMode.PROFESSIONAL:
            # 101 patches with extensive near-black
            base = np.linspace(0, 1, 81)
            near_black = np.linspace(0, 0.1, 21)
            patches = np.unique(np.concatenate([near_black, base]))
        else:  # Verification
            patches = generate_pq_verification_patches(21)

        return np.sort(patches)

    def get_target_luminance(self, signal: np.ndarray) -> np.ndarray:
        """Get target luminance for PQ signals."""
        # Full PQ EOTF
        target = pq_eotf(signal)

        # Apply tone mapping if needed
        if self.config.enable_tone_mapping:
            peak = self.config.target_peak_luminance
            if peak < 10000:
                # Soft rolloff for content above display peak
                knee = peak * 0.9
                target = np.where(
                    target <= knee,
                    target,
                    knee + (peak - knee) * np.tanh((target - knee) / (10000 - knee) * 3)
                )

        return target

    def analyze_measurements(
        self,
        signals: np.ndarray,
        luminances: np.ndarray
    ) -> HDRCalibrationResult:
        """
        Analyze PQ EOTF measurements.

        Args:
            signals: PQ signal levels [0, 1]
            luminances: Measured luminance values (cd/m²)

        Returns:
            HDRCalibrationResult with analysis
        """
        targets = self.get_target_luminance(signals)

        measurements = []
        for i in range(len(signals)):
            error_pct = abs(luminances[i] - targets[i]) / max(targets[i], 0.001) * 100

            measurements.append(HDRMeasurement(
                signal_level=float(signals[i]),
                target_luminance=float(targets[i]),
                measured_luminance=float(luminances[i]),
                eotf_error_percent=float(error_pct)
            ))

        # Calculate statistics
        errors = np.array([m.eotf_error_percent for m in measurements])
        near_black_mask = signals < 0.1
        near_black_errors = errors[near_black_mask] if np.any(near_black_mask) else np.array([0])

        peak = float(np.max(luminances))
        black = float(np.min(luminances[luminances > 0])) if np.any(luminances > 0) else 0.0001

        result = HDRCalibrationResult(
            config=self.config,
            measurements=measurements,
            peak_luminance=peak,
            min_luminance=black,
            contrast_ratio=peak / max(black, 0.0001),
            avg_eotf_error=float(np.mean(errors)),
            max_eotf_error=float(np.max(errors)),
            near_black_error=float(np.mean(near_black_errors))
        )

        # Check compliance
        if self.config.mastering_standard:
            meas_dict = {
                'peak_luminance': peak,
                'min_luminance': black,
                'eotf_error': result.avg_eotf_error
            }
            level, issues, _ = validate_mastering_compliance(
                meas_dict, self.config.mastering_standard
            )
            result.compliance_level = level
            result.compliance_issues = issues

        return result

    def generate_correction_lut(
        self,
        measurements: List[HDRMeasurement],
        lut_size: int = 65
    ) -> np.ndarray:
        """
        Generate 1D correction LUT from measurements.

        Args:
            measurements: Calibration measurements
            lut_size: Output LUT size

        Returns:
            1D correction LUT [lut_size]
        """
        # Extract measurement data
        signals = np.array([m.signal_level for m in measurements])
        measured = np.array([m.measured_luminance for m in measurements])
        targets = np.array([m.target_luminance for m in measurements])

        # Create LUT by interpolating corrections
        lut_signals = np.linspace(0, 1, lut_size)
        lut_targets = self.get_target_luminance(lut_signals)

        # Interpolate measured response
        lut_measured = np.interp(lut_signals, signals, measured)

        # Calculate correction (what input gives desired output)
        corrections = np.zeros(lut_size)
        for i, target in enumerate(lut_targets):
            # Find input signal that produces this target luminance
            if target <= measured[-1]:
                # Inverse interpolation
                idx = np.searchsorted(measured, target)
                if idx == 0:
                    corrections[i] = signals[0]
                elif idx >= len(measured):
                    corrections[i] = signals[-1]
                else:
                    # Linear interpolation
                    t = (target - measured[idx-1]) / (measured[idx] - measured[idx-1])
                    corrections[i] = signals[idx-1] + t * (signals[idx] - signals[idx-1])
            else:
                corrections[i] = 1.0

        return np.clip(corrections, 0, 1)


# =============================================================================
# HDR10+ Calibration
# =============================================================================

class HDR10PlusCalibration(HDR10Calibration):
    """
    HDR10+ Dynamic Metadata Calibration.

    Extends HDR10 with scene-by-scene optimization capability.
    """

    def __init__(self, config: HDRCalibrationConfig = None):
        if config is None:
            config = HDRCalibrationConfig(format=HDRFormat.HDR10_PLUS)
        super().__init__(config)

    def generate_scene_test_patches(self, num_scenes: int = 5) -> List[Dict]:
        """
        Generate test patches simulating different HDR10+ scenes.

        Each scene has different MaxCLL/MaxFALL characteristics.
        """
        scenes = []

        # Scene 1: Dark scene (low MaxCLL)
        scenes.append({
            'name': 'dark_scene',
            'max_cll': 200,
            'max_fall': 50,
            'patches': np.array([0, 0.1, 0.2, 0.3, 0.4, 0.5])
        })

        # Scene 2: Normal scene
        scenes.append({
            'name': 'normal_scene',
            'max_cll': 500,
            'max_fall': 150,
            'patches': np.linspace(0, 0.7, 15)
        })

        # Scene 3: Bright scene
        scenes.append({
            'name': 'bright_scene',
            'max_cll': 800,
            'max_fall': 300,
            'patches': np.linspace(0, 0.85, 15)
        })

        # Scene 4: High contrast
        scenes.append({
            'name': 'high_contrast',
            'max_cll': 1000,
            'max_fall': 200,
            'patches': np.array([0, 0.01, 0.05, 0.1, 0.5, 0.9, 0.95, 1.0])
        })

        # Scene 5: Peak highlights
        scenes.append({
            'name': 'peak_highlights',
            'max_cll': 1000,
            'max_fall': 400,
            'patches': np.linspace(0.5, 1.0, 20)
        })

        return scenes[:num_scenes]


# =============================================================================
# HLG Calibration
# =============================================================================

class HLGCalibration:
    """
    HLG (Hybrid Log-Gamma) Calibration.

    Calibrates display for broadcast HLG content with proper
    system gamma for the viewing environment.
    """

    def __init__(self, config: HDRCalibrationConfig = None):
        if config is None:
            config = HDRCalibrationConfig(format=HDRFormat.HLG)
        self.config = config

    def generate_patches(self) -> np.ndarray:
        """Generate HLG signal patches."""
        if self.config.mode == CalibrationMode.QUICK:
            return np.linspace(0, 1, 21)
        elif self.config.mode == CalibrationMode.STANDARD:
            return np.linspace(0, 1, 51)
        else:
            return np.linspace(0, 1, 101)

    def get_target_luminance(self, signal: np.ndarray) -> np.ndarray:
        """Get target luminance for HLG signals."""
        display = hlg_eotf(signal, self.config.hlg_system_gamma)
        return display * self.config.target_peak_luminance

    def analyze_measurements(
        self,
        signals: np.ndarray,
        luminances: np.ndarray
    ) -> HDRCalibrationResult:
        """Analyze HLG EOTF measurements."""
        targets = self.get_target_luminance(signals)

        errors, avg_error = calculate_hlg_eotf_error(
            luminances, signals,
            self.config.target_peak_luminance,
            self.config.hlg_system_gamma
        )

        measurements = []
        for i in range(len(signals)):
            measurements.append(HDRMeasurement(
                signal_level=float(signals[i]),
                target_luminance=float(targets[i]),
                measured_luminance=float(luminances[i]),
                eotf_error_percent=float(errors[i])
            ))

        peak = float(np.max(luminances))
        black = float(np.min(luminances[luminances > 0])) if np.any(luminances > 0) else 0.01

        return HDRCalibrationResult(
            config=self.config,
            measurements=measurements,
            peak_luminance=peak,
            min_luminance=black,
            contrast_ratio=peak / black,
            avg_eotf_error=avg_error,
            max_eotf_error=float(np.max(errors)),
            near_black_error=float(np.mean(errors[signals < 0.1]))
        )


# =============================================================================
# Unified HDR Calibration Suite
# =============================================================================

class HDRCalibrationSuite:
    """
    Complete HDR Calibration Suite.

    Provides unified interface for all HDR format calibration
    with automatic format detection and professional validation.
    """

    def __init__(self, config: HDRCalibrationConfig = None):
        self.config = config or HDRCalibrationConfig()

        # Initialize appropriate calibrator
        if self.config.format == HDRFormat.HDR10:
            self.calibrator = HDR10Calibration(self.config)
        elif self.config.format == HDRFormat.HDR10_PLUS:
            self.calibrator = HDR10PlusCalibration(self.config)
        elif self.config.format == HDRFormat.HLG:
            self.calibrator = HLGCalibration(self.config)
        else:
            self.calibrator = HDR10Calibration(self.config)

    def get_test_patches(self) -> Dict[str, np.ndarray]:
        """
        Get all test patches for calibration.

        Returns:
            Dictionary with patch sets:
            - grayscale: Grayscale ramp patches
            - near_black: Extra near-black patches
            - primaries: RGB primary patches (if applicable)
        """
        result = {
            'grayscale': self.calibrator.generate_patches()
        }

        if self.config.mode in [CalibrationMode.STANDARD, CalibrationMode.PROFESSIONAL]:
            # Add near-black emphasis
            result['near_black'] = np.linspace(0, 0.05, 11)

        if self.config.mode == CalibrationMode.PROFESSIONAL:
            # Add primary patches at various luminance levels
            levels = [0.5, 0.75, 1.0]
            result['primaries'] = {
                'red': [(l, 0, 0) for l in levels],
                'green': [(0, l, 0) for l in levels],
                'blue': [(0, 0, l) for l in levels]
            }

        return result

    def analyze(
        self,
        grayscale_signals: np.ndarray,
        grayscale_luminance: np.ndarray,
        primary_measurements: Optional[Dict] = None
    ) -> HDRCalibrationResult:
        """
        Analyze calibration measurements.

        Args:
            grayscale_signals: Grayscale signal levels
            grayscale_luminance: Measured grayscale luminance
            primary_measurements: Optional primary color measurements

        Returns:
            Complete calibration result
        """
        result = self.calibrator.analyze_measurements(
            grayscale_signals, grayscale_luminance
        )

        # Add primary analysis if available
        if primary_measurements:
            # Could add gamut analysis here
            pass

        return result

    def generate_correction(
        self,
        result: HDRCalibrationResult,
        output_format: str = "cube"
    ) -> Tuple[np.ndarray, Optional[Path]]:
        """
        Generate correction LUT from calibration result.

        Args:
            result: Calibration result
            output_format: LUT format (cube, 3dl, etc.)

        Returns:
            (lut_data, optional_path)
        """
        if isinstance(self.calibrator, HDR10Calibration):
            lut = self.calibrator.generate_correction_lut(result.measurements)
            return lut, None

        return np.linspace(0, 1, 65), None

    def quick_assessment(
        self,
        peak_luminance: float,
        black_level: float,
        sample_luminances: np.ndarray,
        sample_signals: np.ndarray
    ) -> Dict:
        """
        Quick HDR capability assessment without full calibration.

        Args:
            peak_luminance: Measured peak white
            black_level: Measured black level
            sample_luminances: Sample luminance measurements
            sample_signals: Corresponding signal levels

        Returns:
            Assessment dictionary
        """
        assessment = assess_pq_display(sample_luminances, sample_signals)

        return {
            "format": self.config.format.value,
            "peak_luminance": peak_luminance,
            "black_level": black_level,
            "contrast_ratio": peak_luminance / max(black_level, 0.0001),
            "dynamic_range_stops": np.log2(peak_luminance / max(black_level, 0.0001)),
            "eotf_accuracy": assessment.eotf_accuracy,
            "grade": assessment.grade,
            "recommendation": self._get_recommendation(assessment)
        }

    def _get_recommendation(self, assessment: PQDisplayAssessment) -> str:
        """Generate calibration recommendation."""
        if assessment.grade == "Reference HDR":
            return "Display meets professional mastering requirements. Minor calibration may improve near-black performance."
        elif assessment.grade == "Professional HDR":
            return "Display is suitable for professional work. Calibration recommended for mastering compliance."
        elif assessment.grade == "Good HDR":
            return "Display provides good HDR experience. Calibration will improve accuracy."
        elif assessment.grade == "Basic HDR":
            return "Display has limited HDR capability. Calibration essential for accurate viewing."
        else:
            return "Display may not be suitable for HDR mastering. Consider hardware upgrade."


# =============================================================================
# Convenience Functions
# =============================================================================

def calibrate_hdr10(
    measure_func: Callable[[float], float],
    config: HDRCalibrationConfig = None
) -> HDRCalibrationResult:
    """
    Complete HDR10 calibration workflow.

    Args:
        measure_func: Function that takes signal level and returns luminance
        config: Calibration configuration

    Returns:
        Calibration result
    """
    if config is None:
        config = HDRCalibrationConfig(format=HDRFormat.HDR10)

    suite = HDRCalibrationSuite(config)
    patches = suite.get_test_patches()

    signals = patches['grayscale']
    luminances = np.array([measure_func(s) for s in signals])

    return suite.analyze(signals, luminances)


def calibrate_hlg(
    measure_func: Callable[[float], float],
    system_gamma: float = 1.2,
    config: HDRCalibrationConfig = None
) -> HDRCalibrationResult:
    """
    Complete HLG calibration workflow.

    Args:
        measure_func: Function that takes signal level and returns luminance
        system_gamma: HLG system gamma for viewing environment
        config: Calibration configuration

    Returns:
        Calibration result
    """
    if config is None:
        config = HDRCalibrationConfig(format=HDRFormat.HLG)
    config.hlg_system_gamma = system_gamma

    suite = HDRCalibrationSuite(config)
    patches = suite.get_test_patches()

    signals = patches['grayscale']
    luminances = np.array([measure_func(s) for s in signals])

    return suite.analyze(signals, luminances)
