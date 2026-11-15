"""
PQ (Perceptual Quantizer) / ST.2084 HDR Transfer Function

Implements the SMPTE ST.2084 perceptual quantizer EOTF for HDR10 content.
The PQ curve is designed to match the human visual system's contrast
sensitivity across a 10,000 nit luminance range.
"""

import numpy as np
from dataclasses import dataclass
from typing import Tuple, Optional, Union

# =============================================================================
# ST.2084 Constants
# =============================================================================

# PQ curve constants (SMPTE ST.2084)
PQ_M1 = 2610.0 / 16384.0          # 0.1593017578125
PQ_M2 = 2523.0 / 32.0 * 128.0     # 78.84375
PQ_C1 = 3424.0 / 4096.0           # 0.8359375
PQ_C2 = 2413.0 / 128.0            # 18.8515625
PQ_C3 = 2392.0 / 128.0            # 18.6875

# Reference luminance
PQ_REFERENCE_WHITE = 10000.0  # cd/m2 (nits)
SDR_REFERENCE_WHITE = 100.0   # cd/m2 (sRGB reference)

# =============================================================================
# PQ EOTF (Electro-Optical Transfer Function)
# =============================================================================

def pq_eotf(signal: np.ndarray, normalize: bool = False) -> np.ndarray:
    """
    Convert PQ-encoded signal to linear luminance (cd/m2).

    ST.2084 EOTF: converts code values to display light.

    Args:
        signal: PQ signal values in [0, 1] range
        normalize: If True, normalize output to [0, 1] instead of [0, 10000]

    Returns:
        Luminance in cd/m2 (or normalized to [0, 1] if normalize=True)
    """
    signal = np.asarray(signal, dtype=np.float64)
    signal = np.clip(signal, 0.0, 1.0)

    # Avoid division by zero
    signal_m2 = np.power(signal, 1.0 / PQ_M2)

    numerator = np.maximum(signal_m2 - PQ_C1, 0.0)
    denominator = PQ_C2 - PQ_C3 * signal_m2

    # Avoid division by zero
    denominator = np.where(denominator == 0, 1e-10, denominator)

    Y = PQ_REFERENCE_WHITE * np.power(numerator / denominator, 1.0 / PQ_M1)

    if normalize:
        return Y / PQ_REFERENCE_WHITE

    return Y


def pq_oetf(luminance: np.ndarray, normalize_input: bool = False) -> np.ndarray:
    """
    Convert linear luminance (cd/m2) to PQ-encoded signal.

    Inverse EOTF (OETF): converts linear light to code values.

    Args:
        luminance: Luminance values in cd/m2 [0, 10000] or normalized [0, 1]
        normalize_input: If True, input is normalized [0, 1], scale to [0, 10000]

    Returns:
        PQ signal values in [0, 1] range
    """
    luminance = np.asarray(luminance, dtype=np.float64)

    if normalize_input:
        luminance = luminance * PQ_REFERENCE_WHITE

    luminance = np.clip(luminance, 0.0, PQ_REFERENCE_WHITE)

    Y_norm = luminance / PQ_REFERENCE_WHITE
    Y_m1 = np.power(Y_norm, PQ_M1)

    numerator = PQ_C1 + PQ_C2 * Y_m1
    denominator = 1.0 + PQ_C3 * Y_m1

    E = np.power(numerator / denominator, PQ_M2)

    return np.clip(E, 0.0, 1.0)

# =============================================================================
# HDR10 Metadata
# =============================================================================

@dataclass
class HDR10Metadata:
    """HDR10 static metadata (SMPTE ST.2086)."""
    # Mastering display primaries (CIE 1931 xy)
    red_primary: Tuple[float, float] = (0.680, 0.320)
    green_primary: Tuple[float, float] = (0.265, 0.690)
    blue_primary: Tuple[float, float] = (0.150, 0.060)
    white_point: Tuple[float, float] = (0.3127, 0.3290)

    # Luminance range
    max_luminance: float = 1000.0    # cd/m2
    min_luminance: float = 0.0001   # cd/m2

    # Content light level (CEA-861.3)
    max_cll: float = 1000.0     # Maximum Content Light Level
    max_fall: float = 400.0     # Maximum Frame-Average Light Level

    def to_dict(self) -> dict:
        """Convert to dictionary for serialization."""
        return {
            "primaries": {
                "red": self.red_primary,
                "green": self.green_primary,
                "blue": self.blue_primary,
                "white": self.white_point
            },
            "luminance": {
                "max": self.max_luminance,
                "min": self.min_luminance
            },
            "content_light_level": {
                "max_cll": self.max_cll,
                "max_fall": self.max_fall
            }
        }

# Common display metadata presets
HDR10_PRESETS = {
    "DCI-P3_1000": HDR10Metadata(
        red_primary=(0.680, 0.320),
        green_primary=(0.265, 0.690),
        blue_primary=(0.150, 0.060),
        white_point=(0.3127, 0.3290),
        max_luminance=1000.0,
        min_luminance=0.0001
    ),
    "BT2020_1000": HDR10Metadata(
        red_primary=(0.708, 0.292),
        green_primary=(0.170, 0.797),
        blue_primary=(0.131, 0.046),
        white_point=(0.3127, 0.3290),
        max_luminance=1000.0,
        min_luminance=0.0001
    ),
    "BT2020_4000": HDR10Metadata(
        red_primary=(0.708, 0.292),
        green_primary=(0.170, 0.797),
        blue_primary=(0.131, 0.046),
        white_point=(0.3127, 0.3290),
        max_luminance=4000.0,
        min_luminance=0.0001
    ),
}

# =============================================================================
# HDR Calibration Functions
# =============================================================================

def calculate_pq_eotf_error(
    measured_luminance: np.ndarray,
    signal_levels: np.ndarray,
    reference_white: float = 100.0
) -> Tuple[np.ndarray, float]:
    """
    Calculate EOTF tracking error for PQ curve.

    Args:
        measured_luminance: Measured display luminance at each level
        signal_levels: Input signal levels [0, 1]
        reference_white: Display SDR white level (cd/m2)

    Returns:
        (error_percentage, average_error)
    """
    # Target luminance from PQ EOTF
    target_luminance = pq_eotf(signal_levels)

    # Scale target to display's SDR reference
    # (assumes display is set to SDR white = reference_white)
    scale_factor = reference_white / SDR_REFERENCE_WHITE
    target_luminance_scaled = target_luminance * scale_factor

    # Calculate percentage error
    errors = np.abs(measured_luminance - target_luminance_scaled) / target_luminance_scaled * 100
    errors = np.nan_to_num(errors, nan=0.0, posinf=100.0, neginf=0.0)

    avg_error = np.mean(errors)

    return errors, avg_error


def generate_pq_calibration_lut(
    display_peak: float,
    display_black: float = 0.0,
    size: int = 33,
    tone_map: bool = True
) -> np.ndarray:
    """
    Generate PQ calibration LUT for a specific display.

    Args:
        display_peak: Display peak luminance (cd/m2)
        display_black: Display black level (cd/m2)
        size: LUT size
        tone_map: Apply tone mapping for content above display peak

    Returns:
        1D LUT array (size,) for PQ calibration
    """
    signal = np.linspace(0, 1, size)

    # Target luminance from PQ
    target = pq_eotf(signal)

    # Clip to display capabilities
    if tone_map and display_peak < PQ_REFERENCE_WHITE:
        # Apply soft roll-off tone mapping
        knee_start = display_peak * 0.9
        target = np.where(
            target <= knee_start,
            target,
            knee_start + (display_peak - knee_start) * np.tanh(
                (target - knee_start) / (PQ_REFERENCE_WHITE - knee_start)
            )
        )

    # Add black level offset
    target = np.clip(target + display_black, display_black, display_peak)

    # Normalize to display range
    output = (target - display_black) / (display_peak - display_black)

    return np.clip(output, 0, 1)


def generate_pq_verification_patches(
    num_patches: int = 21,
    include_near_black: bool = True
) -> np.ndarray:
    """
    Generate PQ signal levels for EOTF verification.

    Args:
        num_patches: Number of verification patches
        include_near_black: Include extra near-black patches

    Returns:
        Array of PQ signal values [0, 1]
    """
    # Standard grayscale ramp
    patches = np.linspace(0, 1, num_patches)

    if include_near_black:
        # Add near-black patches for shadow detail verification
        near_black = np.array([0.001, 0.002, 0.005, 0.01, 0.02, 0.03])
        patches = np.unique(np.concatenate([near_black, patches]))

    return np.sort(patches)


def pq_code_to_nits(code_value: Union[int, float], bit_depth: int = 10) -> float:
    """
    Convert PQ code value to luminance in nits.

    Args:
        code_value: Integer code value (0 to 2^bit_depth - 1)
        bit_depth: Bit depth (8, 10, 12)

    Returns:
        Luminance in cd/m2 (nits)
    """
    max_code = (2 ** bit_depth) - 1
    signal = code_value / max_code
    return float(pq_eotf(np.array([signal]))[0])


def nits_to_pq_code(luminance: float, bit_depth: int = 10) -> int:
    """
    Convert luminance in nits to PQ code value.

    Args:
        luminance: Luminance in cd/m2
        bit_depth: Bit depth (8, 10, 12)

    Returns:
        Integer code value
    """
    max_code = (2 ** bit_depth) - 1
    signal = pq_oetf(np.array([luminance]))[0]
    return int(round(signal * max_code))


# =============================================================================
# Display Capability Assessment
# =============================================================================

@dataclass
class PQDisplayAssessment:
    """Assessment of display's HDR/PQ capabilities."""
    peak_luminance: float          # Measured peak (cd/m2)
    black_level: float             # Measured black (cd/m2)
    contrast_ratio: float          # Peak / Black
    dynamic_range_stops: float     # log2(Peak / Black)
    eotf_accuracy: float           # Average EOTF tracking error (%)
    near_black_accuracy: float     # Near-black EOTF error (%)
    grade: str                     # Performance grade

    def to_dict(self) -> dict:
        return {
            "peak_luminance_nits": self.peak_luminance,
            "black_level_nits": self.black_level,
            "contrast_ratio": self.contrast_ratio,
            "dynamic_range_stops": self.dynamic_range_stops,
            "eotf_accuracy_percent": self.eotf_accuracy,
            "near_black_accuracy_percent": self.near_black_accuracy,
            "grade": self.grade
        }


def assess_pq_display(
    measured_luminance: np.ndarray,
    signal_levels: np.ndarray
) -> PQDisplayAssessment:
    """
    Assess display's PQ/HDR performance.

    Args:
        measured_luminance: Measured luminance values (cd/m2)
        signal_levels: Corresponding PQ signal levels [0, 1]

    Returns:
        PQDisplayAssessment with performance metrics
    """
    peak = float(np.max(measured_luminance))
    black = float(np.min(measured_luminance[measured_luminance > 0]))

    contrast = peak / max(black, 0.0001)
    dr_stops = np.log2(contrast)

    # Calculate EOTF error
    errors, avg_error = calculate_pq_eotf_error(
        measured_luminance, signal_levels, reference_white=100.0
    )

    # Near-black error (signal < 10%)
    near_black_mask = signal_levels < 0.1
    if np.any(near_black_mask):
        near_black_error = float(np.mean(errors[near_black_mask]))
    else:
        near_black_error = 0.0

    # Grade based on performance
    if avg_error < 2.0 and near_black_error < 5.0 and peak >= 1000:
        grade = "Reference HDR"
    elif avg_error < 5.0 and near_black_error < 10.0 and peak >= 600:
        grade = "Professional HDR"
    elif avg_error < 10.0 and peak >= 400:
        grade = "Good HDR"
    elif peak >= 300:
        grade = "Basic HDR"
    else:
        grade = "Limited HDR"

    return PQDisplayAssessment(
        peak_luminance=peak,
        black_level=black,
        contrast_ratio=contrast,
        dynamic_range_stops=dr_stops,
        eotf_accuracy=avg_error,
        near_black_accuracy=near_black_error,
        grade=grade
    )
