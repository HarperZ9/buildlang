"""
Dolby Vision HDR Support

Implements Dolby Vision metadata handling and tone mapping for
professional display calibration.

Dolby Vision Features:
- 12-bit precision
- Dynamic metadata per scene/frame
- Multiple profiles (5, 7, 8)
- Backwards compatibility layers
- Proprietary tone mapping curves
"""

import numpy as np
from dataclasses import dataclass, field
from typing import List, Optional, Tuple, Dict, Any, Union
from enum import IntEnum
import struct


# =============================================================================
# Dolby Vision Constants
# =============================================================================

# Dolby Vision profiles
class DVProfile(IntEnum):
    """Dolby Vision profile types."""
    PROFILE_4 = 4    # HDR10 compatible (deprecated)
    PROFILE_5 = 5    # IPT-PQ, single layer (streaming)
    PROFILE_7 = 7    # BC (Base + Enhancement layer)
    PROFILE_8 = 8    # HDR10 compatible, SDR backwards compatible


# Color spaces
class DVColorSpace(IntEnum):
    """Dolby Vision color spaces."""
    YCbCr = 0
    RGB = 1
    IPT = 2  # IPT-PQ color space (Intensity, Protan, Tritan)


# Transfer functions
class DVTransferFunction(IntEnum):
    """Transfer function types."""
    PQ = 0          # SMPTE ST.2084 PQ
    HLG = 1         # HLG
    SDR = 2         # BT.1886
    LINEAR = 3


# Signal ranges
class DVSignalRange(IntEnum):
    """Signal range types."""
    NARROW = 0      # Limited range (16-235/240)
    FULL = 1        # Full range (0-255)


# =============================================================================
# Dolby Vision Metadata Structures
# =============================================================================

@dataclass
class DVPrimaries:
    """Dolby Vision color primaries (CIE 1931 xy)."""
    red: Tuple[float, float] = (0.708, 0.292)    # BT.2020
    green: Tuple[float, float] = (0.170, 0.797)
    blue: Tuple[float, float] = (0.131, 0.046)
    white: Tuple[float, float] = (0.3127, 0.3290)  # D65


@dataclass
class DVContentRange:
    """Content light level range."""
    min_pq: int = 0           # Minimum PQ code value (12-bit)
    max_pq: int = 4095        # Maximum PQ code value
    min_luminance: float = 0.0     # cd/m²
    max_luminance: float = 10000.0  # cd/m²


@dataclass
class DVTrimPass:
    """
    Dolby Vision trim pass for target display adaptation.

    Trim passes adjust the master grade for specific target displays.
    """
    target_max_pq: int = 2081        # Target display peak (PQ code)
    target_min_pq: int = 62          # Target display black
    target_primary_index: int = 0    # 0=P3, 1=BT.2020

    # Trim adjustments
    trim_slope: float = 1.0
    trim_offset: float = 0.0
    trim_power: float = 1.0

    # Chroma adjustments
    trim_chroma_weight: float = 1.0
    trim_saturation_gain: float = 1.0

    # Mid-tone adjustments
    ms_weight: float = 1.0           # Mid-tones slope weight

    def to_pq_luminance(self, pq_code: int) -> float:
        """Convert 12-bit PQ code to luminance."""
        from calibrate_pro.hdr.pq_st2084 import pq_eotf
        signal = pq_code / 4095.0
        return float(pq_eotf(np.array([signal]))[0])

    @property
    def target_max_luminance(self) -> float:
        """Target max luminance in cd/m²."""
        return self.to_pq_luminance(self.target_max_pq)

    @property
    def target_min_luminance(self) -> float:
        """Target min luminance in cd/m²."""
        return self.to_pq_luminance(self.target_min_pq)


@dataclass
class DVPolynomialCurve:
    """
    Polynomial reshaping curve.

    Used for custom tone mapping in Dolby Vision.
    """
    order: int = 0
    coefficients: List[float] = field(default_factory=list)
    mmr_coefficients: List[List[float]] = field(default_factory=list)  # Multi-model regression

    def evaluate(self, x: np.ndarray) -> np.ndarray:
        """Evaluate polynomial at x."""
        x = np.asarray(x, dtype=np.float64)
        result = np.zeros_like(x)

        for i, coef in enumerate(self.coefficients):
            result += coef * np.power(x, i)

        return np.clip(result, 0.0, 1.0)


@dataclass
class DVRPU:
    """
    Dolby Vision Reference Processing Unit (RPU) metadata.

    Contains all metadata needed for tone mapping a frame.
    """
    # Profile info
    profile: DVProfile = DVProfile.PROFILE_8
    level: int = 6  # Profile level (affects max resolution/fps)

    # Version
    rpu_type: int = 2
    rpu_format: int = 18  # Common format for Profile 5/8

    # Coefficient data
    coefficient_data_type: int = 0
    coefficient_log2_denom: int = 23

    # VDR (Visual Dynamic Range) info
    vdr_dm_metadata_present: bool = True

    # Mapping curves
    num_pivots: int = 9
    pivot_values: List[float] = field(default_factory=list)
    polynomial_curves: List[DVPolynomialCurve] = field(default_factory=list)

    # Content metadata
    source_min_pq: int = 62
    source_max_pq: int = 3079
    source_diagonal: int = 42  # Display diagonal in inches

    # Target metadata
    target_min_pq: int = 62
    target_max_pq: int = 2081
    target_diagonal: int = 42

    # Trim passes
    trim_passes: List[DVTrimPass] = field(default_factory=list)

    # Color processing
    primaries: DVPrimaries = field(default_factory=DVPrimaries)
    color_space: DVColorSpace = DVColorSpace.IPT
    transfer_function: DVTransferFunction = DVTransferFunction.PQ
    signal_range: DVSignalRange = DVSignalRange.FULL

    # L1 metadata (per-frame)
    min_pq: int = 0
    max_pq: int = 4095
    avg_pq: int = 1024

    def get_source_range(self) -> Tuple[float, float]:
        """Get source luminance range in cd/m²."""
        from calibrate_pro.hdr.pq_st2084 import pq_eotf
        min_sig = self.source_min_pq / 4095.0
        max_sig = self.source_max_pq / 4095.0
        min_lum = float(pq_eotf(np.array([min_sig]))[0])
        max_lum = float(pq_eotf(np.array([max_sig]))[0])
        return min_lum, max_lum

    def get_target_range(self) -> Tuple[float, float]:
        """Get target luminance range in cd/m²."""
        from calibrate_pro.hdr.pq_st2084 import pq_eotf
        min_sig = self.target_min_pq / 4095.0
        max_sig = self.target_max_pq / 4095.0
        min_lum = float(pq_eotf(np.array([min_sig]))[0])
        max_lum = float(pq_eotf(np.array([max_sig]))[0])
        return min_lum, max_lum

    def to_dict(self) -> Dict[str, Any]:
        """Convert to dictionary."""
        source_min, source_max = self.get_source_range()
        target_min, target_max = self.get_target_range()

        return {
            "profile": self.profile.name,
            "level": self.level,
            "color_space": self.color_space.name,
            "transfer_function": self.transfer_function.name,
            "source_luminance": {"min": source_min, "max": source_max},
            "target_luminance": {"min": target_min, "max": target_max},
            "primaries": {
                "red": self.primaries.red,
                "green": self.primaries.green,
                "blue": self.primaries.blue,
                "white": self.primaries.white
            }
        }


# =============================================================================
# Dolby Vision Tone Mapping
# =============================================================================

class DolbyVisionToneMapper:
    """
    Dolby Vision tone mapping engine.

    Implements the Dolby Vision color volume transform for
    adapting content to target displays.
    """

    def __init__(
        self,
        target_peak: float = 1000.0,
        target_black: float = 0.005,
        target_primaries: str = "P3"  # "P3" or "BT2020"
    ):
        """
        Initialize Dolby Vision tone mapper.

        Args:
            target_peak: Target display peak (cd/m²)
            target_black: Target display black (cd/m²)
            target_primaries: Target color gamut
        """
        self.target_peak = target_peak
        self.target_black = target_black
        self.target_primaries = target_primaries

        # Compute target PQ codes
        from calibrate_pro.hdr.pq_st2084 import pq_oetf
        self.target_max_pq = int(pq_oetf(np.array([target_peak]))[0] * 4095)
        self.target_min_pq = int(pq_oetf(np.array([target_black]))[0] * 4095)

    def generate_tone_curve(
        self,
        rpu: DVRPU,
        size: int = 4096
    ) -> np.ndarray:
        """
        Generate tone mapping curve from RPU metadata.

        Args:
            rpu: Dolby Vision RPU metadata
            size: Output curve size

        Returns:
            1D LUT for tone mapping (12-bit precision)
        """
        from calibrate_pro.hdr.pq_st2084 import pq_eotf, pq_oetf

        # Input PQ range
        pq_in = np.linspace(0, 1, size)

        # Convert to luminance
        lum_in = pq_eotf(pq_in)

        # Source/target ranges
        source_min, source_max = rpu.get_source_range()
        target_min, target_max = self.target_peak, self.target_black

        # Apply DV tone mapping algorithm
        lum_out = self._apply_dv_tonemap(
            lum_in,
            source_min, source_max,
            target_min, target_max,
            rpu
        )

        # Convert back to PQ
        pq_out = pq_oetf(lum_out)

        return np.clip(pq_out, 0.0, 1.0)

    def _apply_dv_tonemap(
        self,
        lum: np.ndarray,
        src_min: float,
        src_max: float,
        tgt_min: float,
        tgt_max: float,
        rpu: DVRPU
    ) -> np.ndarray:
        """Apply Dolby Vision tone mapping algorithm."""
        # Normalize to source range
        lum_norm = (lum - src_min) / (src_max - src_min)
        lum_norm = np.clip(lum_norm, 0.0, 1.0)

        if src_max <= tgt_max:
            # Source fits in target - minimal processing
            lum_out = lum_norm * (tgt_max - tgt_min) + tgt_min
        else:
            # Apply S-curve tone mapping
            # Dolby uses proprietary curves - this is an approximation

            # Calculate compression ratio
            ratio = tgt_max / src_max

            # Knee point (where roll-off begins)
            knee = 0.5 * ratio

            # Apply polynomial-like curve
            below_knee = lum_norm <= knee
            lum_out = np.zeros_like(lum_norm)

            # Linear region
            lum_out[below_knee] = lum_norm[below_knee]

            # Roll-off region
            above = lum_norm[~below_knee]

            # Modified Reinhard with DV characteristics
            max_stretch = (src_max - src_min * knee) / (tgt_max - tgt_min * knee)
            compressed = knee + (1.0 - knee) * (
                (above - knee) / (above - knee + (1.0 - knee) / max_stretch)
            )
            lum_out[~below_knee] = compressed

            # Apply trim if available
            if rpu.trim_passes:
                trim = rpu.trim_passes[0]
                lum_out = np.power(lum_out * trim.trim_slope + trim.trim_offset, trim.trim_power)

            # Scale to target range
            lum_out = lum_out * (tgt_max - tgt_min) + tgt_min

        return np.clip(lum_out, tgt_min, tgt_max)

    def apply_to_frame(
        self,
        frame: np.ndarray,
        rpu: DVRPU
    ) -> np.ndarray:
        """
        Apply Dolby Vision processing to a frame.

        Args:
            frame: Input frame in PQ domain [0, 1]
            rpu: RPU metadata for frame

        Returns:
            Processed frame
        """
        # Generate per-frame curve
        curve = self.generate_tone_curve(rpu, 4096)

        # Apply curve
        result = np.zeros_like(frame)
        indices = (frame * 4095).astype(np.int32)
        indices = np.clip(indices, 0, 4095)

        for c in range(3):
            result[..., c] = curve[indices[..., c]]

        return result


# =============================================================================
# Profile-Specific Handling
# =============================================================================

def create_profile5_rpu(
    source_peak: float = 1000.0,
    source_black: float = 0.0001,
    frame_max: float = 500.0,
    frame_avg: float = 100.0
) -> DVRPU:
    """
    Create Profile 5 RPU metadata.

    Profile 5 is used for streaming with single-layer IPT-PQ encoding.
    """
    from calibrate_pro.hdr.pq_st2084 import pq_oetf

    source_max_pq = int(pq_oetf(np.array([source_peak]))[0] * 4095)
    source_min_pq = int(pq_oetf(np.array([max(source_black, 0.0001)]))[0] * 4095)
    frame_max_pq = int(pq_oetf(np.array([frame_max]))[0] * 4095)
    frame_avg_pq = int(pq_oetf(np.array([frame_avg]))[0] * 4095)

    return DVRPU(
        profile=DVProfile.PROFILE_5,
        level=6,
        rpu_format=18,
        color_space=DVColorSpace.IPT,
        transfer_function=DVTransferFunction.PQ,
        source_min_pq=source_min_pq,
        source_max_pq=source_max_pq,
        max_pq=frame_max_pq,
        avg_pq=frame_avg_pq,
        vdr_dm_metadata_present=True
    )


def create_profile8_rpu(
    source_peak: float = 1000.0,
    source_black: float = 0.0001,
    hdr10_compatible: bool = True
) -> DVRPU:
    """
    Create Profile 8 RPU metadata.

    Profile 8 is HDR10-compatible with optional enhancement layer.
    """
    from calibrate_pro.hdr.pq_st2084 import pq_oetf

    source_max_pq = int(pq_oetf(np.array([source_peak]))[0] * 4095)
    source_min_pq = int(pq_oetf(np.array([max(source_black, 0.0001)]))[0] * 4095)

    rpu = DVRPU(
        profile=DVProfile.PROFILE_8,
        level=6,
        rpu_format=18,
        color_space=DVColorSpace.YCbCr if hdr10_compatible else DVColorSpace.IPT,
        transfer_function=DVTransferFunction.PQ,
        source_min_pq=source_min_pq,
        source_max_pq=source_max_pq,
        signal_range=DVSignalRange.NARROW if hdr10_compatible else DVSignalRange.FULL
    )

    # Add default trim for 1000 nit target
    target_1000 = DVTrimPass(
        target_max_pq=2081,  # ~1000 nits
        target_min_pq=62,    # ~0.005 nits
        trim_slope=1.0,
        trim_offset=0.0,
        trim_power=1.0
    )
    rpu.trim_passes.append(target_1000)

    return rpu


# =============================================================================
# RPU Parsing/Serialization (Simplified)
# =============================================================================

def parse_rpu_header(data: bytes) -> Optional[Dict[str, Any]]:
    """
    Parse RPU header to extract basic info.

    Args:
        data: Raw RPU data

    Returns:
        Dict with header info or None
    """
    if len(data) < 8:
        return None

    try:
        # RPU starts with NAL unit header
        # This is simplified - real parsing needs bit-level access

        rpu_type = data[0] >> 4
        rpu_format = data[0] & 0x0F

        return {
            "rpu_type": rpu_type,
            "rpu_format": rpu_format,
            "data_length": len(data)
        }

    except Exception:
        return None


def create_calibration_rpu(
    display_peak: float,
    display_black: float = 0.005,
    profile: DVProfile = DVProfile.PROFILE_8
) -> DVRPU:
    """
    Create RPU metadata for display calibration.

    Args:
        display_peak: Calibrated display peak (cd/m²)
        display_black: Calibrated display black (cd/m²)
        profile: Dolby Vision profile to use

    Returns:
        DVRPU configured for the target display
    """
    if profile == DVProfile.PROFILE_5:
        return create_profile5_rpu(source_peak=display_peak)
    else:
        rpu = create_profile8_rpu(source_peak=display_peak)

        # Add trim pass for this specific display
        from calibrate_pro.hdr.pq_st2084 import pq_oetf
        target_max_pq = int(pq_oetf(np.array([display_peak]))[0] * 4095)
        target_min_pq = int(pq_oetf(np.array([display_black]))[0] * 4095)

        trim = DVTrimPass(
            target_max_pq=target_max_pq,
            target_min_pq=target_min_pq
        )
        rpu.trim_passes = [trim]

        return rpu


# =============================================================================
# Dolby Vision Calibration
# =============================================================================

@dataclass
class DVCalibrationResult:
    """Dolby Vision display calibration result."""
    profile: DVProfile
    measured_peak: float
    measured_black: float
    target_max_pq: int
    target_min_pq: int
    eotf_accuracy: float  # Average tracking error %
    grade: str

    def to_rpu(self) -> DVRPU:
        """Generate RPU from calibration."""
        return create_calibration_rpu(
            self.measured_peak,
            self.measured_black,
            self.profile
        )


def calibrate_for_dolby_vision(
    measured_levels: np.ndarray,
    measured_luminance: np.ndarray,
    profile: DVProfile = DVProfile.PROFILE_8
) -> DVCalibrationResult:
    """
    Calibrate display for Dolby Vision.

    Args:
        measured_levels: PQ signal levels [0, 1]
        measured_luminance: Measured luminance (cd/m²)
        profile: Target DV profile

    Returns:
        Calibration result with recommended settings
    """
    from calibrate_pro.hdr.pq_st2084 import pq_eotf, pq_oetf

    # Find peak and black
    peak = float(np.max(measured_luminance))
    black_candidates = measured_luminance[measured_luminance > 0]
    black = float(np.min(black_candidates)) if len(black_candidates) > 0 else 0.0

    # Calculate EOTF tracking
    target_luminance = pq_eotf(measured_levels)
    errors = np.abs(measured_luminance - target_luminance) / np.maximum(target_luminance, 0.01) * 100
    avg_error = float(np.mean(errors))

    # Determine grade
    if avg_error < 2.0 and peak >= 1000:
        grade = "Dolby Vision Reference"
    elif avg_error < 5.0 and peak >= 600:
        grade = "Dolby Vision Mastering"
    elif avg_error < 10.0 and peak >= 400:
        grade = "Dolby Vision Compatible"
    else:
        grade = "Basic HDR"

    # Calculate target PQ codes
    target_max_pq = int(pq_oetf(np.array([peak]))[0] * 4095)
    target_min_pq = int(pq_oetf(np.array([max(black, 0.0001)]))[0] * 4095)

    return DVCalibrationResult(
        profile=profile,
        measured_peak=peak,
        measured_black=black,
        target_max_pq=target_max_pq,
        target_min_pq=target_min_pq,
        eotf_accuracy=avg_error,
        grade=grade
    )


def generate_dv_verification_patches() -> List[Tuple[float, str]]:
    """
    Generate patches for Dolby Vision verification.

    Returns:
        List of (pq_level, description) tuples
    """
    from calibrate_pro.hdr.pq_st2084 import pq_oetf

    patches = []

    # Near-black (critical for DV)
    for nits in [0.005, 0.01, 0.02, 0.05, 0.1]:
        pq = float(pq_oetf(np.array([nits]))[0])
        patches.append((pq, f"{nits} nits (near-black)"))

    # SDR range
    for nits in [1, 5, 10, 20, 50, 100]:
        pq = float(pq_oetf(np.array([nits]))[0])
        patches.append((pq, f"{nits} nits"))

    # HDR range
    for nits in [200, 400, 600, 800, 1000]:
        pq = float(pq_oetf(np.array([nits]))[0])
        patches.append((pq, f"{nits} nits"))

    # High luminance (if supported)
    for nits in [2000, 4000, 10000]:
        pq = float(pq_oetf(np.array([nits]))[0])
        patches.append((pq, f"{nits} nits (extended)"))

    return patches
